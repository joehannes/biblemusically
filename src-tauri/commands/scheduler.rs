// ────────────────────────────────────────────────────────────────
// Scheduler: turns a project's `schedule_config` into automatic daily/weekly
// progress through a Bible book — fetch the next chapter, generate that
// chapter's song lyrics via AI, insert it as a draft, and kick off music
// generation. Deliberately stops there: analysis/images/video/upload stay
// manual so a human reviews content before anything goes public.
//
// `schedule_config` is stored as a loose JSON object directly on the project
// document (no dedicated Rust model — `update_project`/`get_project` already
// round-trip arbitrary fields through Mongo via `bson_to_value`), shaped like:
//   {
//     "enabled": bool,
//     "frequency": "daily" | "weekly",
//     "time": "HH:MM"        // 24h, local time
//     "day_of_week": 0-6,     // 0 = Sunday, only used when frequency == "weekly"
//     "book": "psalms",       // a src-tauri/commands/bible.rs BIBLE_BOOKS id
//     "translation": "web",
//     "languages": ["English"],
//     "styles": ["Cinematic"],
//     "next_chapter": 1,
//     "last_run_at": "2026-07-08T12:00:00+00:00" // set by the scheduler itself
//   }
//
// The same core function backs both the automatic background tick (started
// once at app startup, see src-tauri/src/lib.rs) and the manual "Generate Now"
// button in the Dashboard's Daily Content panel — one source of truth for
// "make today's song" either way.
// ────────────────────────────────────────────────────────────────

use crate::{jobs::enqueue, models::{now_iso, Job, Song}, state::AppState};
use bson::{doc, Document};
use serde_json::Value;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

// ── Daily "salt" ────────────────────────────────────────────────────────────────
// A small deterministic-per-day flavor modifier so the same configured musical style doesn't
// produce an identical-sounding song every single day. Picked from (today's date, salt) so a
// given project+style combo gets one consistent flavor for the whole day (not a fresh random
// pick per song within the same scheduler run), while different days/styles/projects diverge.
const FLAVOR_MODIFIERS: &[&str] = &[
    "with a touch of warmth", "slightly more percussive energy", "a hint of nostalgic texture",
    "brighter, more optimistic tone", "subtly darker undertone", "a touch more cinematic space",
    "gentle acoustic warmth layered in", "a bit more rhythmic drive", "softened, more intimate mix",
    "extra shimmer on the highs", "a touch of raw, live-band energy", "smoother, more polished production",
    "a dash of unexpected texture", "slightly more minimal arrangement", "richer harmonic layering",
];

fn daily_flavor(salt: &str) -> &'static str {
    let day = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let seed = format!("{day}:{salt}");
    let mut h: u32 = 0;
    for b in seed.bytes() { h = h.wrapping_mul(31).wrapping_add(b as u32); }
    FLAVOR_MODIFIERS[(h as usize) % FLAVOR_MODIFIERS.len()]
}

fn bson_to_value(doc: Document) -> Value {
    let mut m = serde_json::Map::new();
    for (k, v) in doc {
        if k == "_id" { continue; }
        if let Ok(jv) = bson::from_bson::<Value>(v) { m.insert(k, jv); }
    }
    Value::Object(m)
}

fn parse_time_hhmm(s: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 { return None; }
    let h: u32 = parts[0].trim().parse().ok()?;
    let m: u32 = parts[1].trim().parse().ok()?;
    if h > 23 || m > 59 { return None; }
    Some((h, m))
}

/// Whether a project's schedule is due to fire, given the current local time.
fn is_due(cfg: &Value, now: chrono::DateTime<chrono::Local>) -> bool {
    use chrono::{Datelike, Timelike};

    if !cfg["enabled"].as_bool().unwrap_or(false) {
        return false;
    }
    let (want_h, want_m) = cfg["time"].as_str()
        .and_then(parse_time_hhmm)
        .unwrap_or((9, 0));
    if (now.hour(), now.minute()) < (want_h, want_m) {
        return false;
    }
    if cfg["frequency"].as_str().unwrap_or("daily") == "weekly" {
        let want_dow = cfg["day_of_week"].as_i64().unwrap_or(0).clamp(0, 6) as u32;
        if now.weekday().num_days_from_sunday() != want_dow {
            return false;
        }
    }
    if let Some(last) = cfg["last_run_at"].as_str() {
        if let Ok(last_dt) = chrono::DateTime::parse_from_rfc3339(last) {
            if last_dt.with_timezone(&chrono::Local).date_naive() == now.date_naive() {
                return false; // already ran today
            }
        }
    }
    true
}

/// Resolves the chapter to fetch, wrapping back to chapter 1 once the requested chapter runs
/// past the book's known chapter count so a project's daily/weekly cadence never just stops.
async fn resolve_chapter(book_id: &str, requested_chapter: i64) -> i64 {
    if let Ok(books) = crate::commands::bible::list_bible_books().await {
        if let Some(b) = books.as_array().and_then(|a| a.iter().find(|b| b["id"].as_str() == Some(book_id))) {
            let total = b["chapters"].as_i64().unwrap_or(1);
            if requested_chapter > total || requested_chapter < 1 {
                return 1;
            }
        }
    }
    requested_chapter.max(1)
}

fn string_list(v: &Value, fallback: &Value, default_single: &str) -> Vec<String> {
    let from = |val: &Value| -> Vec<String> {
        val.as_array()
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default()
    };
    let primary = from(v);
    if !primary.is_empty() { return primary; }
    let fb = from(fallback);
    if !fb.is_empty() { return fb; }
    vec![default_single.to_string()]
}

/// Core generation step: fetch the next chapter for a project's configured book, generate one
/// song per configured language/style (capped at 4 combos per run), insert each as a draft song,
/// enqueue music generation for it, and advance the chapter cursor. Used both by the background
/// scheduler tick and the manual "Generate Now" button.
pub async fn run_scheduled_generation(state: &Arc<AppState>, project_id: &str) -> Res<Value> {
    let db = &state.db;
    let project_doc = db.collection::<Document>("projects")
        .find_one(doc! { "id": project_id }).await.map_err(e)?
        .ok_or_else(|| "Project not found".to_string())?;
    let project = bson_to_value(project_doc);

    let cfg = project.get("schedule_config").cloned().unwrap_or_else(|| serde_json::json!({}));
    let book = cfg["book"].as_str().unwrap_or("").trim().to_string();
    if book.is_empty() {
        return Err("No Bible book configured for this project's schedule. Set one in the Dashboard's Daily Content panel.".to_string());
    }
    let translation = cfg["translation"].as_str().filter(|s| !s.is_empty()).unwrap_or("web").to_string();
    let requested_chapter = cfg["next_chapter"].as_i64().unwrap_or(1);
    let chapter = resolve_chapter(&book, requested_chapter).await;

    let languages = string_list(&cfg["languages"], &project["languages"], "English");
    let styles = string_list(&cfg["styles"], &project["styles"], "Cinematic");

    let chapter_data = crate::commands::bible::fetch_chapter(translation.clone(), book.clone(), chapter as u32).await
        .map_err(|err| format!("Failed to fetch {} {}: {}", book, chapter, err))?;
    let chapter_text = chapter_data["verses"].as_array().cloned().unwrap_or_default()
        .iter()
        .filter_map(|v| v["text"].as_str())
        .collect::<Vec<_>>()
        .join(" ");
    if chapter_text.trim().is_empty() {
        return Err(format!("Fetched {} {} but it had no verse text.", book, chapter));
    }

    let topic = project["topic"].as_str().unwrap_or("").to_string();

    // Cap combos per run so one scheduler tick can't fan out into an unbounded number of AI
    // calls (and downstream Suno jobs) if a project is configured with many languages/styles.
    let mut combos: Vec<(String, String)> = Vec::new();
    'outer: for lang in &languages {
        for style in &styles {
            combos.push((lang.clone(), style.clone()));
            if combos.len() >= 4 { break 'outer; }
        }
    }

    let mut created_song_ids = Vec::new();
    let mut errors = Vec::new();

    for (language, style) in combos {
        // Daily salt: the same configured style text every scheduled run would make e.g. every
        // "Cinematic" song sound alike — this nudges the AI's production choices without
        // changing the style name itself (kept out of the title/image_styles, which stay on the
        // plain `style`). Salted by project+style so different projects/styles diverge too.
        let flavored_style = format!("{}, {}", style, daily_flavor(&format!("{project_id}:{style}")));
        let system = "You compose a single song's lyrics for AI music video production, based on a Bible chapter.\n\
            Return ONLY a valid JSON object — no prose, no markdown fences.\n\
            The object MUST have exactly these string fields: title, lyrics, annotations, image_styles.\n\
            `lyrics`: the full song lyrics as plain text, one lyric line per line.\n\
            `annotations`: alternating lines — first a bracketed Midjourney-style image prompt in square brackets on its own line, then the matching lyric line, repeated for every lyric line.\n\
            Translate the lyrics into the target language faithfully if it isn't English. Reflect the chapter's imagery and themes in the bracket prompts.";
        let user = format!(
            "Source chapter text ({book} {chapter}):\n{chapter_text}\n\n\
             Project topic: {topic}\n\
             Target language: {language}\n\
             Musical/visual style: {flavored_style}\n\
             Title should read like: \"{book} {chapter} ({style})\"\n\n\
             Output the JSON object now."
        );

        match crate::commands::ai::call_openrouter(db, system, &user, 0.85, true).await {
            Ok(resp) => {
                let text = resp["text"].as_str().unwrap_or("");
                match crate::commands::ai::extract_json_value(text) {
                    Some(parsed) => {
                        let song = Song {
                            id: Uuid::new_v4().to_string(),
                            project_id: project_id.to_string(),
                            title: parsed["title"].as_str().unwrap_or(&format!("{book} {chapter}")).to_string(),
                            language: language.clone(),
                            styles: flavored_style.clone(),
                            lyrics: parsed["lyrics"].as_str().unwrap_or("").to_string(),
                            annotations: parsed["annotations"].as_str().unwrap_or("").to_string(),
                            image_styles: parsed["image_styles"].as_str().unwrap_or(&style).to_string(),
                            audio_url: None,
                            video_url: None,
                            duration: 0.0,
                            channel_id: None,
                            local_audio_path: None,
                            local_audio_path_alt: None,
                            status: "draft".into(),
                            created_at: now_iso(),
                        };
                        let bson_song = bson::to_document(&song).map_err(e)?;
                        db.collection::<Document>("songs").insert_one(bson_song).await.map_err(e)?;
                        created_song_ids.push(song.id.clone());
                        // Automate the tedious first pipeline step; analysis/images/video/upload
                        // stay manual by design (see module doc comment).
                        let _ = enqueue("music", &song.id, state).await;
                    }
                    None => errors.push(format!("{language}/{style}: AI response was not valid JSON")),
                }
            }
            Err(err) => errors.push(format!("{language}/{style}: {err}")),
        }
    }

    // Advance the cursor and record the run time regardless of partial failures — otherwise one
    // failing language/style combo would wedge the project's cadence indefinitely.
    let ts = now_iso();
    let mut new_cfg = cfg.clone();
    if let Some(obj) = new_cfg.as_object_mut() {
        obj.insert("book".into(), Value::String(book.clone()));
        obj.insert("next_chapter".into(), serde_json::json!(chapter + 1));
        obj.insert("last_run_at".into(), Value::String(ts.clone()));
    }
    db.collection::<Document>("projects")
        .update_one(doc! { "id": project_id }, doc! { "$set": { "schedule_config": bson::to_bson(&new_cfg).map_err(e)? } })
        .await.map_err(e)?;

    let summary = serde_json::json!({
        "book": book,
        "chapter": chapter,
        "created_song_ids": created_song_ids,
        "errors": errors,
    });

    // Leave a visible trail in the Jobs Monitor even though this ran synchronously rather than
    // through jobs::enqueue (there's no long-running async work here to poll for).
    let job = Job {
        id: Uuid::new_v4().to_string(),
        kind: "scheduler_run".into(),
        target_id: project_id.to_string(),
        status: if created_song_ids_is_empty(&summary) && !errors_is_empty(&summary) { "failed".into() } else { "done".into() },
        progress: 100,
        logs: vec![format!("[{ts}] generated {} song(s) for {} {}", summary["created_song_ids"].as_array().map(|a| a.len()).unwrap_or(0), book, chapter)],
        attempts: 0,
        created_at: ts.clone(),
        updated_at: ts,
        error: if errors.is_empty() { None } else { Some(errors.join("; ")) },
        result: summary.clone(),
        // Ran synchronously, so it lands in the Jobs Monitor already finished.
        stage: Some("done".into()),
        stage_message: None,
        stage_started_at: None,
    };
    if let Ok(bson_job) = bson::to_document(&job) {
        let _ = db.collection::<Document>("jobs").insert_one(bson_job).await;
    }

    Ok(summary)
}

fn created_song_ids_is_empty(summary: &Value) -> bool {
    summary["created_song_ids"].as_array().map(|a| a.is_empty()).unwrap_or(true)
}
fn errors_is_empty(summary: &Value) -> bool {
    summary["errors"].as_array().map(|a| a.is_empty()).unwrap_or(true)
}

/// Manual trigger for the Dashboard's "Generate Now" button — runs the exact same logic the
/// background scheduler uses, so there is one implementation of "make today's song."
#[tauri::command]
pub async fn generate_next_chapter_now(
    state_arc: State<'_, Arc<AppState>>,
    project_id: String,
) -> Res<Value> {
    let arc = Arc::clone(&*state_arc);
    run_scheduled_generation(&arc, &project_id).await
}

/// Background scheduler tick: checks every project's `schedule_config` and runs generation for
/// any that are due. Spawned once, on an interval, from `src-tauri/src/lib.rs`'s app setup.
pub async fn run_scheduler_tick(state: &Arc<AppState>) {
    use futures_util::StreamExt;

    let now = chrono::Local::now();
    let mut cursor = match state.db.collection::<Document>("projects").find(doc! {}).await {
        Ok(c) => c,
        Err(_) => return,
    };
    let mut due_ids = Vec::new();
    while let Some(Ok(d)) = cursor.next().await {
        let proj = bson_to_value(d);
        let cfg = proj.get("schedule_config").cloned().unwrap_or(Value::Null);
        if is_due(&cfg, now) {
            if let Some(pid) = proj["id"].as_str() {
                due_ids.push(pid.to_string());
            }
        }
    }
    for pid in due_ids {
        if let Err(err) = run_scheduled_generation(state, &pid).await {
            eprintln!("Scheduler: generation failed for project {}: {}", pid, err);
        }
    }
}
