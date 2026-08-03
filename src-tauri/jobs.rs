use crate::{
    helpers::{derive_mood, parse_annotations, resolve_node_executable, suggest_effects},
    models::{now_iso, Job, Section},
    state::AppState,
};
use bson::{doc, Document};
use serde_json::Value;
use std::collections::HashSet;
use std::env;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Shared type alias for the job-cancellation flag set on `AppState`.
type CancelSet = Arc<Mutex<HashSet<String>>>;

async fn is_cancelled(cancelled: &CancelSet, job_id: &str) -> bool {
    cancelled.lock().await.contains(job_id)
}

// ────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────

async fn db_log(db: &crate::store::Db, job_id: &str, msg: &str) {
    let ts = now_iso();
    let entry = format!("[{ts}] {msg}");
    let _ = db.collection::<Document>("jobs")
        .update_one(
            doc! { "id": job_id },
            doc! { "$push": { "logs": &entry }, "$set": { "updated_at": &ts } },
        )
        .await;
}

async fn set_progress(db: &crate::store::Db, job_id: &str, p: i32) {
    let ts = now_iso();
    let _ = db.collection::<Document>("jobs")
        .update_one(
            doc! { "id": job_id },
            doc! { "$set": { "progress": p, "updated_at": &ts },
                   "$push": { "logs": format!("[{ts}] progress {p}%") } },
        )
        .await;
}

/// "45s" / "3m 20s" — compact elapsed time for job status messages.
fn fmt_dur(secs: u64) -> String {
    if secs < 60 { format!("{}s", secs) } else { format!("{}m {:02}s", secs / 60, secs % 60) }
}

fn unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Publish a coarse pipeline `stage` + percentage + human message in a single write.
///
/// Why this exists: the UI used to render `logs.last()` as its status line, which meant it mostly
/// displayed `set_progress`'s own "progress 42%" spam instead of anything meaningful, and it had no
/// way to tell "waiting in a queue" apart from "actively rendering". Writing an explicit stage lets
/// the frontend draw a real staged view (and a live elapsed timer via `stage_started_at`) without
/// regex-scraping log text. `new_stage` = false keeps the original stage-start timestamp, so a
/// long-running stage can refresh its percentage/message without resetting its timer.
async fn set_stage(db: &crate::store::Db, job_id: &str, stage: &str, p: i32, msg: &str, new_stage: bool) {
    let ts = now_iso();
    let mut set = doc! { "stage": stage, "progress": p, "stage_message": msg, "updated_at": &ts };
    if new_stage {
        set.insert("stage_started_at", unix_secs());
    }
    // Only a stage *transition* earns a log line; percentage refreshes within a stage would
    // otherwise recreate the very spam this replaces.
    let update = if new_stage {
        doc! { "$set": set, "$push": { "logs": format!("[{ts}] {msg}") } }
    } else {
        doc! { "$set": set }
    };
    let _ = db.collection::<Document>("jobs").update_one(doc! { "id": job_id }, update).await;
}

// ────────────────────────────────────────────────────────────────
// Engine-aware style adaptation
// ────────────────────────────────────────────────────────────────

/// What each music engine actually wants in its style field, plus that field's practical limit.
///
/// All three take comma-separated descriptors, but they reward different shapes:
/// * **Suno** — this lands in the "Style of Music" box, which is short and whose content filters
///   reject real artist/band names outright. Keep it compact and name nobody.
/// * **ACE-Step** — a caption-style tag list. It was trained on captions carrying instrumentation
///   and tempo, so concrete instruments and an explicit BPM help materially.
/// * **HeartMuLa** — the serve cell writes this *verbatim* into a `tags.txt` beside `lyrics.txt`,
///   so it must be a clean tag list: no prose, no `key: value` syntax, no trailing period.
fn engine_style_brief(engine: &str) -> (&'static str, usize) {
    match engine {
        "suno" => (
            "Target engine: Suno. Produce a COMPACT comma-separated style tag list for Suno's \
             'Style of Music' field: genre, sub-genre, mood, key instrumentation, and a vocal \
             descriptor (e.g. 'male tenor lead', 'mixed choir', 'instrumental'). Never name a real \
             artist, band or song — Suno's filters reject those outright. No sentences.",
            200,
        ),
        "acestep" => (
            "Target engine: ACE-Step. Produce a comma-separated caption-style tag list: genre, \
             mood, concrete instrumentation, production character, vocal type, and an explicit \
             tempo in BPM at the end. ACE-Step responds well to instrument and tempo detail. \
             No sentences.",
            300,
        ),
        _ => (
            "Target engine: HeartMuLa. Produce a clean comma-separated tag list that will be \
             written verbatim into a tags.txt file: genre, mood, instrumentation, vocal type, and \
             a tempo in BPM. Roughly 8-14 tags. No prose, no 'key: value' pairs, no trailing period.",
            300,
        ),
    }
}

/// Reduce a model reply to one clean tag line: first non-empty line, stripped of fences/quotes and
/// the "Style:"-type labels models add even when told not to, then clamped on a comma boundary so
/// a tag is never cut in half.
fn clean_style_line(content: &str, max_len: usize) -> String {
    let mut line = content
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty() && !l.starts_with("```"))
        .unwrap_or("")
        .to_string();

    let lower = line.to_ascii_lowercase();
    for label in ["style of music:", "styles:", "style:", "tags:", "output:"] {
        if lower.starts_with(label) {
            line = line[label.len()..].trim().to_string();
            break;
        }
    }
    line = line
        .trim_matches(|c| c == '"' || c == '\'' || c == '`' || c == '*')
        .trim()
        .to_string();

    // Char-wise, not byte-wise: an em-dash or accent in a tag would otherwise panic the slice.
    if line.chars().count() > max_len {
        let truncated: String = line.chars().take(max_len).collect();
        let cut = truncated.rfind(',').unwrap_or(truncated.len());
        line = truncated[..cut].trim_end_matches(',').trim().to_string();
    }
    line
}

/// Rewrite a song's free-text `styles` into the shape the selected engine responds best to.
///
/// Deliberately re-runs on every generation instead of caching, so repeat renders of the same song
/// get fresh variation. Any failure — no AI provider configured, provider down, empty reply — falls
/// back to the raw styles: this is an enhancement and must never be able to block a render.
async fn adapt_styles_for_engine(
    engine: &str,
    song: &Value,
    settings: &Value,
    job_id: &str,
    db: &crate::store::Db,
) -> String {
    let styles = song.get("styles").and_then(|v| v.as_str()).unwrap_or("");
    let title = song.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let raw = styles.trim();
    if raw.is_empty() {
        return String::new();
    }
    let (brief, max_len) = engine_style_brief(engine);
    let system = format!(
        "You adapt music style descriptions for a specific AI music generator. {brief}\n\
         Reply with ONLY the tag list on a single line — no quotes, no markdown, no preamble."
    );

    // Project Brief + today's topic + this channel's cached research bias the rewrite, so each
    // channel's DAILY style leans into the project's mood and its region's musical culture instead
    // of being a context-free reshuffle of the same tags.
    let project_id = song.get("project_id").and_then(|v| v.as_str()).unwrap_or("");
    let brief_block = crate::commands::ai::project_brief_block(db, project_id).await;
    let mut research_note = String::new();
    if let Some(cid) = song.get("channel_id").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        if let Ok(Some(ch)) = db.collection::<Document>("channels").find_one(doc! { "id": cid }).await {
            if let Ok(r) = ch.get_str("research") {
                if !r.trim().is_empty() {
                    research_note = format!("Channel research (regional/cultural/musical notes — lean the styles toward this): {}\n", r.trim());
                }
            }
        }
    }
    let language = song.get("language").and_then(|v| v.as_str()).unwrap_or("");

    let user = format!(
        "{brief_block}{research_note}Song title: {}\nLanguage: {}\nCurrent style description: {}\n\n\
         Rewrite it for the target engine, letting the brief's mood, today's topic and the channel's \
         regional musical culture shade the choice of genres/instrumentation.",
        if title.trim().is_empty() { "(untitled)" } else { title.trim() },
        if language.is_empty() { "unspecified" } else { language },
        raw
    );

    // High temperature on purpose: with no caching this runs fresh on every render, and some
    // variation between takes of the same song is a feature rather than noise.
    match crate::commands::ai::provider_chat(settings, &system, &user, 0.9, false).await {
        Ok((content, model)) => {
            let cleaned = clean_style_line(&content, max_len);
            if cleaned.is_empty() {
                db_log(db, job_id, "styles: AI returned nothing usable — sending the original styles").await;
                raw.to_string()
            } else {
                db_log(db, job_id, &format!("styles in : {}", raw)).await;
                db_log(db, job_id, &format!("styles out ({} via {}): {}", engine, model, cleaned)).await;
                cleaned
            }
        }
        Err(err) => {
            db_log(db, job_id, &format!("styles: adaptation skipped ({}) — sending the original styles", err)).await;
            raw.to_string()
        }
    }
}

// ────────────────────────────────────────────────────────────────
// Real Suno integration
// ────────────────────────────────────────────────────────────────

fn normalize_suno_cookie(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let raw = if raw.to_ascii_lowercase().starts_with("cookie:") {
        raw[7..].trim()
    } else {
        raw
    };
    let cookie = raw
        .split(';')
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    let cookie = cookie.trim();
    if cookie.is_empty() {
        return None;
    }
    if !cookie.contains('=') {
        return Some(format!("studio-api_key={cookie}"));
    }
    Some(cookie.to_string())
}

async fn real_suno(
    song: &Value,
    settings: &Value,
    job_id: &str,
    db: &crate::store::Db,
    cancelled: &CancelSet,
) -> Option<Vec<Value>> {
    let raw_cookie = settings.get("suno_cookie")?.as_str()?.trim();
    let cookie = match normalize_suno_cookie(raw_cookie) {
        Some(c) => c,
        None => {
            db_log(db, job_id, "suno: cookie not configured").await;
            return None;
        }
    };

    // Pre-check cookie validity before attempting generation
    let precheck_client = reqwest::Client::new();
    let precheck = precheck_client
        .get("https://studio-api.suno.com/api/user/")
        .header("Cookie", &cookie)
        .header("Accept", "application/json, text/plain, */*")
        .header("User-Agent", "Mozilla/5.0")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;
    match precheck {
        Ok(r) if r.status() == 200 => { /* cookie is valid */ }
        Ok(r) if r.status() == 401 || r.status() == 403 => {
            db_log(db, job_id, "suno: cookie has expired or is invalid. Please renew it in Settings (F12 → Cookies → suno.com → copy studio-api_key). Generation cancelled.").await;
            return None;
        }
        Ok(_) => {
            db_log(db, job_id, "suno: pre-check HTTP error — cookie may still be valid, proceeding anyway.").await;
        }
        Err(e) => {
            db_log(db, job_id, &format!("suno: pre-check connection error ({}), proceeding anyway.", e)).await;
        }
    }
    
    let styles_suno = adapt_styles_for_engine("suno", song, settings, job_id, db).await;

    db_log(db, job_id, "suno: submitting music generation request...").await;

    let payload = serde_json::json!({
        "gpt_description_prompt": format!("{} — {}",
            styles_suno,
            song.get("title").and_then(|v|v.as_str()).unwrap_or("")),
        "make_instrumental": false,
        "mv": "chirp-v3-5",
        "prompt": song.get("lyrics").and_then(|v|v.as_str()).unwrap_or(""),
        "title": song.get("title").and_then(|v|v.as_str()).unwrap_or(""),
    });
    let client = reqwest::Client::new();
    let res = client
        .post("https://studio-api.suno.com/api/generate/v2/")
        .header("Cookie", &cookie)
        .header("Accept", "application/json, text/plain, */*")
        .header("User-Agent", "Mozilla/5.0")
        .json(&payload)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await;
    
    let res = match res {
        Ok(r) => r,
        Err(e) => {
            db_log(db, job_id, &format!("suno: request failed: {}", e)).await;
            return None;
        }
    };
    
    if !res.status().is_success() {
        let status = res.status();
        if status == 401 || status == 403 {
            db_log(db, job_id, "suno: authentication failed - cookie may be expired or invalid").await;
        } else {
            db_log(db, job_id, &format!("suno: HTTP {} - service error", status)).await;
        }
        return None;
    }
    
    let data: Value = match res.json().await {
        Ok(d) => d,
        Err(e) => {
            db_log(db, job_id, &format!("suno: invalid response format: {}", e)).await;
            return None;
        }
    };
    
    let ids: Vec<String> = data["clips"]
        .as_array()?
        .iter()
        .filter_map(|c| c["id"].as_str().map(|s| s.to_string()))
        .collect();
    
    if ids.is_empty() {
        db_log(db, job_id, "suno: no clip IDs returned - generation may have failed").await;
        return None;
    }
    
    db_log(db, job_id, &format!("suno: generation submitted, polling for results ({} clips)...", ids.len())).await;
    
    // Poll for results (up to 200 seconds)
    for attempt in 0..40 {
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        if is_cancelled(cancelled, job_id).await {
            db_log(db, job_id, "suno: cancelled by user").await;
            return None;
        }

        let fr = client
            .get(format!("https://studio-api.suno.com/api/feed/?ids={}", ids.join(",")))
            .header("Cookie", &cookie)
            .header("Accept", "application/json, text/plain, */*")
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await;
        
        let fr = match fr {
            Ok(f) => f,
            Err(e) => {
                db_log(db, job_id, &format!("suno: poll request {}/{} failed: {}", attempt + 1, 40, e)).await;
                continue;
            }
        };
        
        if !fr.status().is_success() {
            db_log(db, job_id, &format!("suno: poll {}/{} returned HTTP {}", attempt + 1, 40, fr.status())).await;
            continue;
        }
        
        let clips: Value = match fr.json().await {
            Ok(c) => c,
            Err(e) => {
                db_log(db, job_id, &format!("suno: poll {}/{} invalid response: {}", attempt + 1, 40, e)).await;
                continue;
            }
        };
        
        if let Some(arr) = clips.as_array() {
            let ready: Vec<&Value> = arr.iter()
                .filter(|x| x["audio_url"].is_string()
                    && matches!(x["status"].as_str(), Some("complete") | Some("streaming")))
                .collect();
            
            if !ready.is_empty() {
                db_log(db, job_id, &format!("suno: generation complete! {} clips ready", ready.len())).await;
                let mut results = Vec::new();
                for clip in ready.iter().take(2) {
                    results.push(serde_json::json!({
                        "audio_url": clip["audio_url"],
                        "duration": clip["metadata"]["duration"].as_f64().unwrap_or(120.0),
                    }));
                }
                return Some(results);
            }
        }
        
        set_progress(db, job_id, 15 + (attempt as i32 * 70 / 40).min(70)).await;
    }
    
    db_log(db, job_id, "suno: timeout after 200 seconds - generation did not complete").await;
    None
}

// ────────────────────────────────────────────────────────────────
// Real ACE-Step integration (free, self-hosted REST server)
// ────────────────────────────────────────────────────────────────

/// Trim a trailing slash so we can concatenate `base` + `/path` cleanly.
fn trim_base_url(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_string()
}

/// Generate a song via an ACE-Step 1.5 REST server (`acestep-api`).
///
/// Mirrors `real_suno`'s submit→poll→collect shape so the `"music"` job arm can use either
/// engine interchangeably. Points at whatever `acestep_api_url` is configured — typically a
/// free Kaggle/Colab `acestep-api --share` public URL, or `http://localhost:8001`.
///
/// Flow: `POST /release_task` → `task_id`; poll `POST /query_result` until the result's
/// `status == 1`; the result's `file` field is a server path (`/v1/audio?path=…`) that we
/// turn into an absolute download URL.
/// Generate a song via a self-hosted REST server that implements the ACE-Step task API
/// (`POST /release_task` → poll `POST /query_result` → `/v1/audio` download). Both ACE-Step and
/// HeartMuLa use this — their Kaggle notebooks expose the identical contract — so the `"music"`
/// job arm can pick any of them. `label` is only for log prefixes.
async fn generate_song_api(
    mut base: String,
    api_key: String,
    label: &str,
    song: &Value,
    settings: &Value,
    job_id: &str,
    db: &crate::store::Db,
    cancelled: &CancelSet,
) -> Option<Vec<Value>> {
    // No saved URL is the single most common cause of "generation failed: server url missing":
    // the tunnel rotates every run and the user started the server but never clicked "Fetch live
    // URL", so `<engine>_api_url` is empty even though a server is up right now. Rather than fail,
    // discover it the same way the Fetch button does (scrape the live kernel log + verify) and use
    // it. This is exactly the case the user hit after the server reached "Ready".
    if base.is_empty() {
        db_log(db, job_id, &format!("{label}: no saved server URL — auto-discovering from the running Kaggle kernel…")).await;
        match crate::commands::settings::refresh_kaggle_url(db, label).await {
            Some(found) => {
                db_log(db, job_id, &format!("{label}: discovered a live server URL: {}", found)).await;
                base = found;
            }
            None => {
                db_log(db, job_id, &format!(
                    "{label}: no server URL saved and none could be auto-discovered. Open Settings → {label} and press \
                     'Start & connect' (or 'Fetch live URL' if the notebook is already running). The Kaggle notebook \
                     must be running with a live tunnel.", )).await;
                return None;
            }
        }
    }

    let title = song.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let lyrics = song.get("lyrics").and_then(|v| v.as_str()).unwrap_or("");

    // Target length: per-song `duration` if set, else the configured default, clamped to 10–600s.
    // The settings value may arrive as a JSON number or a string (the UI form stores it as text).
    let settings_dur = settings.get("acestep_duration")
        .and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse::<f64>().ok())));
    let duration = song.get("duration").and_then(|v| v.as_f64()).filter(|d| *d >= 10.0)
        .or(settings_dur)
        .unwrap_or(240.0)
        .clamp(10.0, 600.0);

    // Rewrite the free-text styles into the shape this engine responds to best (comma tag list for
    // HeartMuLa's tags.txt, caption-with-BPM for ACE-Step). Falls back to the raw styles on any AI
    // failure, so it can never block a render.
    set_stage(db, job_id, "preparing", 4,
              &format!("Tuning the style for {}…", label), true).await;
    let styles = adapt_styles_for_engine(label, song, settings, job_id, db).await;
    let prompt = format!("{} — {}", styles, title);
    set_stage(db, job_id, "preparing", 6,
              &format!("Preparing a {:.0}s track…", duration), false).await;

    let client = reqwest::Client::new();

    // Small helper to attach the optional bearer token.
    let with_auth = |rb: reqwest::RequestBuilder| {
        if api_key.is_empty() { rb } else { rb.header("Authorization", format!("Bearer {}", api_key)) }
    };

    let payload = serde_json::json!({
        "prompt": prompt,
        "lyrics": lyrics,
        "audio_duration": duration,
        "audio_format": "mp3",
        "inference_steps": 8,
        "batch_size": 2,
        "use_random_seed": true,
    });

    // Kaggle's Cloudflare tunnel rotates every run, so a URL that was fine an hour ago (the
    // notebook's idle watchdog killed it, or Kaggle's batch time limit ended the run) can be
    // dead by the time a job actually runs — the connection simply fails to resolve/connect.
    // Give it one automatic recovery attempt: re-scrape the kernel's current log for a fresh
    // tunnel URL (same logic the "Fetch live URL" button uses) and retry once against that,
    // instead of failing the whole job over a URL the user has no reason to think is stale.
    let mut submit = None;
    for attempt in 0..2 {
        set_stage(db, job_id, "submitting", 8, &format!("Sending the request to {label}…"), true).await;
        db_log(db, job_id, &format!("{label}: submitting generation ({:.0}s) to {}", duration, base)).await;
        let res = with_auth(client.post(format!("{}/release_task", base)))
            .header("Accept", "application/json")
            .json(&payload)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await;
        match res {
            Ok(r) => { submit = Some(r); break; }
            Err(e) => {
                if attempt == 0 {
                    db_log(db, job_id, &format!("{label}: {} is unreachable ({}) — the Kaggle tunnel may have rotated; checking for a fresh URL…", base, e)).await;
                    if let Some(fresh) = crate::commands::settings::refresh_kaggle_url(db, label).await {
                        if fresh != base {
                            db_log(db, job_id, &format!("{label}: found a fresh tunnel URL, retrying: {}", fresh)).await;
                            base = fresh;
                            continue;
                        }
                    }
                }
                db_log(db, job_id, &format!("{label}: request failed: {} (the Kaggle notebook may have stopped — try \"Fetch live URL\" or \"Start server\" in Settings)", e)).await;
                return None;
            }
        }
    }
    let submit = submit?;
    if !submit.status().is_success() {
        let status = submit.status();
        // Include the server's response body — a 503 here carries the notebook's real reason
        // (e.g. `model not loaded: <error>`), which is exactly what makes a failure diagnosable.
        let body = submit.text().await.unwrap_or_default();
        let short: String = body.chars().take(400).collect();
        db_log(db, job_id, &format!("{label}: submit returned HTTP {} — {} (check the server URL, API key, and that the model finished loading)", status, short)).await;
        return None;
    }
    let submit_json: Value = match submit.json().await {
        Ok(v) => v,
        Err(e) => { db_log(db, job_id, &format!("song-gen: invalid submit response: {}", e)).await; return None; }
    };
    let task_id = match submit_json["data"]["task_id"].as_str() {
        Some(id) => id.to_string(),
        None => {
            db_log(db, job_id, &format!("song-gen: no task_id in response: {}", submit_json)).await;
            return None;
        }
    };

    set_stage(db, job_id, "generating", 15,
              "Queued on the GPU — starting the render…", true).await;
    db_log(db, job_id, &format!("song-gen: task {} queued, polling for results...", task_id)).await;

    // Poll for completion. ACE-Step on a good GPU finishes in seconds, but HeartMuLa's 3B model on
    // a free Kaggle T4 runs at roughly real-time or worse — a 4-minute track can take 10-20 minutes.
    // The old ceiling here was 60 × 5s = 5 minutes, which timed out on essentially every
    // full-length song. Polling also counts as activity for the notebook's 45-minute idle
    // watchdog, so a longer window here can never strand a healthy run.
    const POLL_EVERY_S: u64 = 5;
    const POLL_ATTEMPTS: u64 = 360; // 360 × 5s = 30 minutes
    let poll_started = std::time::Instant::now();

    for attempt in 0..POLL_ATTEMPTS {
        tokio::time::sleep(tokio::time::Duration::from_secs(POLL_EVERY_S)).await;

        if is_cancelled(cancelled, job_id).await {
            db_log(db, job_id, "song-gen: cancelled by user").await;
            return None;
        }

        let qr = with_auth(client.post(format!("{}/query_result", base)))
            .header("Accept", "application/json")
            .json(&serde_json::json!({ "task_id_list": [task_id] }))
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await;

        let qr = match qr {
            Ok(r) => r,
            Err(e) => { db_log(db, job_id, &format!("song-gen: poll {}/{} failed: {}", attempt + 1, POLL_ATTEMPTS, e)).await; continue; }
        };
        if !qr.status().is_success() {
            db_log(db, job_id, &format!("song-gen: poll {}/{} HTTP {}", attempt + 1, POLL_ATTEMPTS, qr.status())).await;
            continue;
        }
        let qr_json: Value = match qr.json().await {
            Ok(v) => v,
            Err(e) => { db_log(db, job_id, &format!("song-gen: poll {}/{} invalid response: {}", attempt + 1, POLL_ATTEMPTS, e)).await; continue; }
        };

        // `data` is a list of per-task entries; each `result` is a JSON string we must parse.
        let entries = qr_json["data"].as_array().cloned().unwrap_or_default();
        let mut results: Vec<Value> = Vec::new();
        let mut any_failed = false;

        for entry in &entries {
            let parsed: Value = match entry.get("result") {
                Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
                Some(other) => other.clone(),
                None => Value::Null,
            };
            let status = parsed["status"].as_i64().or_else(|| entry["status"].as_i64());
            if status == Some(2) { any_failed = true; }

            // Status 0 means "still rendering". The task API reports a pending task as
            // `{"file": "", "status": 0}` — an EMPTY STRING, not a missing key. The old guard here
            // was `.filter(|v| v.is_string())`, and `""` *is* a string, so the very first 5-second
            // poll harvested a blank filename, resolved it against the base URL to the bare tunnel
            // root (`https://…trycloudflare.com/`), and returned it as a finished track. The job
            // then reported "generation complete!" seconds after starting and saved a URL that
            // 404s — which is exactly why generation looked like it flashed past and did nothing.
            // Skip pending entries outright.
            if status == Some(0) { continue; }

            // A single result may carry one `file` or a `files` array (batch_size > 1).
            let mut files: Vec<Value> = Vec::new();
            if let Some(arr) = parsed.get("files").and_then(|v| v.as_array()) {
                files.extend(arr.iter().cloned());
            }
            if let Some(f) = parsed.get("file") {
                files.push(f.clone());
            }
            // Belt-and-braces: a blank/whitespace filename is never a real result, so drop it even
            // if a server omits `status` entirely and the check above can't fire.
            files.retain(|f| f.as_str().is_some_and(|s| !s.trim().is_empty()));

            let dur = parsed["metas"]["duration"].as_f64()
                .or_else(|| parsed["duration"].as_f64())
                .unwrap_or(duration);

            for f in files {
                if let Some(path) = f.as_str() {
                    // `path` is a server-relative URL like `/v1/audio?path=…`; make it absolute.
                    let url = if path.starts_with("http") {
                        path.to_string()
                    } else {
                        format!("{}{}", base, if path.starts_with('/') { path.to_string() } else { format!("/{}", path) })
                    };
                    results.push(serde_json::json!({ "audio_url": url, "duration": dur }));
                }
            }
        }

        if !results.is_empty() {
            set_stage(db, job_id, "fetching", 92,
                      &format!("Rendered {} track(s) — fetching audio…", results.len()), true).await;
            return Some(results);
        }
        if any_failed && entries.iter().all(|e| {
            let parsed: Value = match e.get("result") {
                Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
                Some(other) => other.clone(),
                None => Value::Null,
            };
            parsed["status"].as_i64() == Some(2) || e["status"].as_i64() == Some(2)
        }) {
            // Surface the server's own exception text (the notebook now returns it in `error`), so
            // a generation failure is diagnosable straight from the app's job log instead of only
            // in the Kaggle notebook output. e.g. an OOM or a heartlib API change shows up here.
            let server_err = entries.iter().find_map(|e| {
                let parsed: Value = match e.get("result") {
                    Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
                    Some(other) => other.clone(),
                    None => Value::Null,
                };
                parsed["error"].as_str().filter(|s| !s.trim().is_empty()).map(|s| s.to_string())
            });
            let msg = match &server_err {
                Some(err) => format!("The server reported a generation error: {}", err.chars().take(300).collect::<String>()),
                None => "The server reported that generation failed (no detail returned).".to_string(),
            };
            set_stage(db, job_id, "failed", 100, &msg, true).await;
            db_log(db, job_id, &format!("song-gen: generation failed on the server (status=2){}",
                server_err.map(|e| format!(" — {}", e)).unwrap_or_default())).await;
            return None;
        }

        // Asymptotic ramp 15% → ~85% with a 4-minute time constant: the bar keeps visibly moving
        // through a long render but never implies "almost done" early, and it can't run out of
        // headroom the way the old linear `attempt/60` ramp did. `new_stage=false` so this 5-second
        // refresh updates the number without logging a line or resetting the elapsed timer.
        let elapsed = poll_started.elapsed().as_secs();
        let pct = 15 + (70.0 * (1.0 - (-(elapsed as f64) / 240.0).exp())) as i32;
        // Message stays constant; the UI renders a live per-second timer from `stage_started_at`,
        // which is smoother than restating elapsed time only every 5 seconds.
        set_stage(db, job_id, "generating", pct, "Rendering audio on the GPU…", false).await;
    }

    db_log(db, job_id, &format!(
        "song-gen: timed out after {} — the server may be overloaded, or the tunnel URL expired mid-render",
        fmt_dur(POLL_ATTEMPTS * POLL_EVERY_S))).await;
    None
}

/// ACE-Step music engine — thin wrapper over the shared task-API client.
/// Generate ONE image sample for a style descriptor, using the configured image engine. Returns the
/// image URL. Used by the on-demand style-sample gallery (commands/style_samples.rs) so the user can
/// see what a style/genre-mix actually looks like on their own engine — not a bundled library.
pub(crate) async fn sample_image(style_prompt: &str, settings: &Value, db: &crate::store::Db, cancelled: &CancelSet) -> Option<String> {
    // A style sample is a square swatch with no restraint text of its own: it exists to show what a
    // style looks like, not to be published.
    gen_images(style_prompt, None, "1:1", None, settings, "style-sample", db, cancelled).await
        .and_then(|mut v| { if v.is_empty() { None } else { Some(v.remove(0)) } })
}

/// Generate ONE short music sample for a genre descriptor, using the configured music engine.
/// Returns the audio URL. Short + lyric-light so it renders fast.
pub(crate) async fn sample_music(genre: &str, settings: &Value, db: &crate::store::Db, cancelled: &CancelSet) -> Option<String> {
    let engine = settings["music_engine"].as_str().unwrap_or("heartmula");
    let song = serde_json::json!({
        "styles": genre, "title": "style sample", "lyrics": "[Verse]\nlight and sound\n", "duration": 20.0
    });
    let clips = match engine {
        "acestep" => real_acestep(&song, settings, "style-sample", db, cancelled).await,
        "heartmula" => real_heartmula(&song, settings, "style-sample", db, cancelled).await,
        _ => real_suno(&song, settings, "style-sample", db, cancelled).await,
    }?;
    clips.into_iter().next()
        .and_then(|c| c["audio_url"].as_str().map(|s| s.to_string()))
}

async fn real_acestep(song: &Value, settings: &Value, job_id: &str, db: &crate::store::Db, cancelled: &CancelSet) -> Option<Vec<Value>> {
    let base = trim_base_url(settings.get("acestep_api_url").and_then(|v| v.as_str()).unwrap_or(""));
    // Keep the idle guard's clock honest: this engine is in use, so it must not be stopped from
    // under the job that is about to run (see idle_guard.rs).
    crate::idle_guard::touch("acestep");
    let key = settings.get("acestep_api_key").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    generate_song_api(base, key, "acestep", song, settings, job_id, db, cancelled).await
}

/// HeartMuLa music engine (free, Apache-2.0 Suno competitor). Its Kaggle notebook exposes the same
/// task API as ACE-Step, so it reuses the shared client.
async fn real_heartmula(song: &Value, settings: &Value, job_id: &str, db: &crate::store::Db, cancelled: &CancelSet) -> Option<Vec<Value>> {
    let base = trim_base_url(settings.get("heartmula_api_url").and_then(|v| v.as_str()).unwrap_or(""));
    crate::idle_guard::touch("heartmula");
    let key = settings.get("heartmula_api_key").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    generate_song_api(base, key, "heartmula", song, settings, job_id, db, cancelled).await
}

/// Apply the saved imagery choice to a section's own prompt.
///
/// Returns the subject unchanged when nothing has been chosen. That matters: the imagery panel is
/// an offer, not a gate, and somebody who has never opened it must still be able to generate.
/// The prompt, and the shape and negative prompt that go with it.
///
/// One choice has to reach every engine, not just the ones that read the prompt. The paid APIs take
/// an aspect ratio and (where they honour one) a negative prompt as separate fields, so returning
/// only a string meant a Leonardo render silently ignored the destination the user had picked.
struct Imagery {
    prompt: String,
    negative: Option<String>,
    aspect: String,
}

async fn apply_imagery(subject: &str, settings: &Value, job_id: &str, db: &crate::store::Db) -> Imagery {
    let plain = |aspect: &str| Imagery {
        prompt: subject.to_string(),
        negative: settings.get("image_negative_prompt").and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty()).map(String::from),
        aspect: aspect.to_string(),
    };
    let fallback_aspect = settings.get("image_aspect").and_then(|v| v.as_str()).unwrap_or("16:9");
    let Some(choice) = settings.get("imagery").filter(|v| v.is_object()) else {
        return plain(fallback_aspect);
    };
    let model = choice["model"].as_str().unwrap_or("flux_dev");
    let intent_id = choice["intent"].as_str().unwrap_or("chapter_opener");
    let Some(intent) = crate::imagery::intent(intent_id) else { return plain(fallback_aspect) };
    let controls: crate::imagery::Controls = choice.get("controls")
        .and_then(|c| serde_json::from_value(c.clone()).ok())
        .unwrap_or_else(|| crate::imagery::controls_for(intent));

    match crate::imagery::compose(model, intent.destination, &controls, subject) {
        Ok(composed) => {
            // The notes are where the app admits what it decided — that a negative prompt would be
            // ignored on this model, that a destination forbids text. Putting them in the job log
            // means the decision is visible after the fact, not only in the panel.
            for note in &composed.notes {
                db_log(db, job_id, &format!("imagery: {note}")).await;
            }
            if let Some(neg) = &composed.negative {
                db_log(db, job_id, &format!("imagery: avoiding — {neg}")).await;
            }
            Imagery { prompt: composed.positive, negative: composed.negative, aspect: composed.aspect }
        }
        Err(why) => {
            db_log(db, job_id, &format!(
                "imagery: keeping the section's own prompt — {why}")).await;
            plain(fallback_aspect)
        }
    }
}

/// Does this music engine charge per track?
///
/// One list, consulted by the fallback rule and by the job log. Kept beside the engines rather than
/// in the interface, because a rule the backend does not know is a rule the interface can be talked
/// out of — and the consequence of getting this one wrong arrives as an invoice.
pub fn music_engine_is_paid(engine: &str) -> bool {
    matches!(engine.trim().to_ascii_lowercase().as_str(), "elevenlabs")
}

/// Riffusion, on a free Kaggle GPU.
///
/// A different notebook API from ACE-Step and HeartMuLa — `POST /generate_song`, then poll
/// `GET /status/{id}` and collect from `GET /download/{id}` — so it does not go through
/// `generate_song_api`. It arranges a song from the section tags in the lyrics itself, which is why
/// it takes minutes per track and why the poll window here is generous.
async fn real_riffusion(song: &Value, settings: &Value, job_id: &str, db: &crate::store::Db, cancelled: &CancelSet) -> Option<Vec<Value>> {
    let mut base = trim_base_url(settings.get("riffusion_api_url").and_then(|v| v.as_str()).unwrap_or(""));
    crate::idle_guard::touch("riffusion");
    if base.is_empty() {
        db_log(db, job_id, "riffusion: no saved server URL — auto-discovering from the running Kaggle kernel…").await;
        match crate::commands::settings::refresh_kaggle_url(db, "riffusion").await {
            Some(found) => { db_log(db, job_id, &format!("riffusion: discovered {found}")).await; base = found; }
            None => {
                db_log(db, job_id, "riffusion: no server URL saved and none could be discovered.                                     Settings → Riffusion → Start & connect.").await;
                return None;
            }
        }
    }
    let key = settings.get("riffusion_api_key").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let with_auth = |rb: reqwest::RequestBuilder| {
        if key.is_empty() { rb } else { rb.header("Authorization", format!("Bearer {key}")) }
    };

    let title = song.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled");
    let lyrics = song.get("lyrics").and_then(|v| v.as_str()).unwrap_or("");
    // The notebook takes minutes, not seconds, and refuses anything outside 5–10.
    let minutes = song.get("duration").and_then(|v| v.as_f64())
        .or_else(|| settings.get("acestep_duration").and_then(|v| v.as_f64()))
        .map(|secs| secs / 60.0)
        .unwrap_or(5.0)
        .clamp(5.0, 10.0);
    let style = adapt_styles_for_engine("riffusion", song, settings, job_id, db).await;

    set_stage(db, job_id, "submitting", 8, "Sending the song to Riffusion…", true).await;
    let client = reqwest::Client::new();
    let payload = serde_json::json!({
        "title": title, "lyrics": lyrics,
        "style_preset": style, "target_duration_minutes": minutes,
    });
    let res = with_auth(client.post(format!("{base}/generate_song")))
        .json(&payload).timeout(std::time::Duration::from_secs(60)).send().await;
    let res = match res {
        Ok(r) => r,
        Err(err) => { db_log(db, job_id, &format!("riffusion: {base} is unreachable: {err}")).await; return None; }
    };
    if !res.status().is_success() {
        let status = res.status();
        let body: String = res.text().await.unwrap_or_default().chars().take(300).collect();
        db_log(db, job_id, &format!("riffusion: submit returned HTTP {status} — {body}")).await;
        return None;
    }
    let started: Value = res.json().await.ok()?;
    let task = started["job_id"].as_str().or_else(|| started["id"].as_str())?.to_string();
    db_log(db, job_id, &format!("riffusion: job {task} queued ({minutes:.0} minutes of audio)")).await;

    // A 5–10 minute arrangement on a free T4 is slow, and polling also counts as activity for the
    // notebook's idle watchdog, so a long window here can never strand a healthy run.
    for attempt in 0..360u32 {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        if is_cancelled(cancelled, job_id).await {
            let _ = with_auth(client.post(format!("{base}/cancel/{task}"))).send().await;
            db_log(db, job_id, "riffusion: cancelled by user").await;
            return None;
        }
        let Ok(r) = with_auth(client.get(format!("{base}/status/{task}"))).send().await else { continue };
        let Ok(body) = r.json::<Value>().await else { continue };
        match body["status"].as_str().unwrap_or("") {
            "completed" | "complete" | "done" => {
                let url = format!("{base}/download/{task}");
                set_stage(db, job_id, "fetching", 92, "Rendered — fetching audio…", true).await;
                return Some(vec![serde_json::json!({ "audio_url": url, "duration": minutes * 60.0 })]);
            }
            "failed" | "error" => {
                let why = body["error"].as_str().unwrap_or("no detail returned");
                db_log(db, job_id, &format!("riffusion: the server reported a failure — {why}")).await;
                return None;
            }
            _ => {
                let pct = body["progress"]["percent"].as_f64().unwrap_or(0.0);
                let done = 15.0 + (pct.clamp(0.0, 100.0) * 0.7);
                set_stage(db, job_id, "generating", done as i32,
                          "Arranging the song section by section…", attempt == 0).await;
            }
        }
    }
    db_log(db, job_id, "riffusion: gave up waiting after thirty minutes.").await;
    None
}

/// ElevenLabs Music — the paid one.
///
/// A genuine public API with commercial licensing, so nothing here automates anybody's account. Two
/// things make it different from every other engine in this file:
///
///   * **It costs money**, so the log says what it is about to spend before it spends it, and a
///     cancellation after the request has gone says the charge may already have happened rather
///     than implying it did not.
///   * **It answers with the audio itself**, not a URL. The bytes go to the media root and come back
///     as a local URL, because every stage downstream fetches over HTTP (see local_media.rs).
async fn real_elevenlabs(song: &Value, settings: &Value, job_id: &str, db: &crate::store::Db, cancelled: &CancelSet) -> Option<Vec<Value>> {
    let key = settings.get("elevenlabs_api_key").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if key.is_empty() {
        db_log(db, job_id, "ElevenLabs Music needs an API key — Settings → Music engine → ElevenLabs.                             It is a paid engine, at roughly $0.10 a track, and nothing is charged                             until a song is generated.").await;
        return None;
    }
    if is_cancelled(cancelled, job_id).await {
        db_log(db, job_id, "elevenlabs: cancelled before anything was charged").await;
        return None;
    }

    let base = {
        let configured = settings.get("elevenlabs_base_url").and_then(|v| v.as_str()).unwrap_or("").trim();
        if configured.is_empty() { "https://api.elevenlabs.io".to_string() }
        else { configured.trim_end_matches('/').to_string() }
    };
    let title = song.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let lyrics = song.get("lyrics").and_then(|v| v.as_str()).unwrap_or("");
    let styles = adapt_styles_for_engine("elevenlabs", song, settings, job_id, db).await;
    let seconds = song.get("duration").and_then(|v| v.as_f64())
        .or_else(|| settings.get("acestep_duration").and_then(|v| v.as_f64()))
        .unwrap_or(180.0)
        .clamp(10.0, 300.0);

    // One prompt carrying style, title and words: the music endpoint takes a description rather
    // than separate lyric and style fields.
    let prompt = format!("{styles}. Song title: {title}.\n\nLyrics:\n{lyrics}");
    let payload = serde_json::json!({
        "prompt": prompt,
        "music_length_ms": (seconds * 1000.0) as i64,
    });

    set_stage(db, job_id, "submitting", 8, "Sending the song to ElevenLabs…", true).await;
    db_log(db, job_id, &format!(
        "elevenlabs: generating {seconds:.0}s of music — this is a paid engine, roughly $0.10 a track."
    )).await;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build().ok()?;
    let res = match client.post(format!("{base}/v1/music"))
        .header("xi-api-key", &key)
        .header("Accept", "audio/mpeg")
        .json(&payload).send().await {
        Ok(r) => r,
        Err(err) => { db_log(db, job_id, &format!("elevenlabs: request failed: {err}")).await; return None; }
    };
    let status = res.status();
    if !status.is_success() {
        // The provider's own words. "quota exceeded" is the thing the user must act on, and a
        // generic message would hide exactly that.
        let body: String = res.text().await.unwrap_or_default().chars().take(400).collect();
        db_log(db, job_id, &format!("elevenlabs: HTTP {status} — {body}")).await;
        return None;
    }
    let bytes = match res.bytes().await {
        Ok(b) if !b.is_empty() => b,
        _ => { db_log(db, job_id, "elevenlabs: the answer carried no audio.").await; return None; }
    };

    set_stage(db, job_id, "fetching", 92, "Saving the track…", true).await;
    let path = match crate::local_media::store("elevenlabs", "mp3", &bytes) {
        Ok(p) => p,
        Err(err) => {
            // Worth being loud: the money is already spent at this point.
            db_log(db, job_id, &format!(
                "elevenlabs: the track was generated and paid for but could not be saved: {err}")).await;
            return None;
        }
    };
    db_log(db, job_id, &format!("elevenlabs: {} KB of audio saved.", bytes.len() / 1024)).await;
    Some(vec![serde_json::json!({
        "audio_url": crate::local_media::url_for(&path),
        "duration": seconds,
    })])
}

// ────────────────────────────────────────────────────────────────
// Real Midjourney (proxy) integration
// ────────────────────────────────────────────────────────────────

async fn real_mj(
    prompt: &str,
    settings: &Value,
    job_id: &str,
    db: &crate::store::Db,
    cancelled: &CancelSet,
) -> Option<Vec<String>> {
    // New Playwright-driven flow: spawn a Node helper which controls a visible
    // browser to submit the prompt and download resulting images. The helper
    // will print a JSON array of saved image paths to stdout on success.
    // Prefer a persistent Playwright profile captured earlier instead of cookies
    let mj_profile = settings.get("mj_profile_dir").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if mj_profile.is_empty() {
        db_log(db, job_id, "mj: mj_profile_dir not configured - open Settings → Capture session").await;
        return None;
    }

    db_log(db, job_id, "mj: launching Playwright generator (visible browser)...").await;

    let out_dir = format!("/tmp/studio_mj_{}", Uuid::new_v4());
    let _ = tokio::fs::create_dir_all(&out_dir).await;

    let node = resolve_node_executable();
    let node = match node {
        Some(p) => p,
        None => {
            db_log(db, job_id, "mj: 'node' runtime not found in PATH or bundled resources").await;
            return None;
        }
    };

    let script = std::path::Path::new("src-tauri").join("packaging").join("midjourney-generator.js");
    if !script.exists() {
        db_log(db, job_id, "mj: generator script missing").await;
        return None;
    }

    let mut cmd = tokio::process::Command::new(node);
    cmd.arg(script.to_string_lossy().to_string())
        .arg("--prompt")
        .arg(prompt.to_string())
        .arg("--profile")
        .arg(mj_profile.clone())
        .arg("--outdir")
        .arg(out_dir.clone())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            db_log(db, job_id, &format!("mj: failed to spawn generator: {}", e)).await;
            return None;
        }
    };

    // Capture PID before moving `child` into wait_with_output
    let child_pid = child.id();

    // Wait for the generator, but race it against a periodic cancellation check (in addition to
    // the overall 6 minute timeout) so a user-requested cancel kills the browser automation
    // within ~2s instead of only being noticed after the full wait completes.
    let wait_future = child.wait_with_output();
    tokio::pin!(wait_future);
    let overall_timeout = tokio::time::sleep(tokio::time::Duration::from_secs(360));
    tokio::pin!(overall_timeout);
    let mut cancel_check = tokio::time::interval(tokio::time::Duration::from_secs(2));

    let outcome = loop {
        tokio::select! {
            result = &mut wait_future => break Some(result),
            _ = &mut overall_timeout => break None,
            _ = cancel_check.tick() => {
                if is_cancelled(cancelled, job_id).await {
                    if let Some(pid) = child_pid {
                        let _ = std::process::Command::new("kill").arg("-9").arg(pid.to_string()).spawn();
                    }
                    db_log(db, job_id, "mj: cancelled by user").await;
                    return None;
                }
            }
        }
    };

    match outcome {
        Some(Ok(output)) => {
            if !output.status.success() {
                let err = String::from_utf8_lossy(&output.stderr);
                db_log(db, job_id, &format!("mj: generator failed: {}", err)).await;
                return None;
            }
            let out = String::from_utf8_lossy(&output.stdout).to_string();
            let parsed: Result<Vec<String>, _> = serde_json::from_str(&out);
            match parsed {
                Ok(v) if !v.is_empty() => {
                    db_log(db, job_id, &format!("mj: generator returned {} images", v.len())).await;
                    return Some(v);
                }
                Ok(_) => {
                    db_log(db, job_id, "mj: generator returned no images").await;
                    return None;
                }
                Err(e) => {
                    db_log(db, job_id, &format!("mj: failed parsing generator output: {}", e)).await;
                    return None;
                }
            }
        }
        Some(Err(e)) => {
            db_log(db, job_id, &format!("mj: generator process error: {}", e)).await;
            return None;
        }
        None => {
            db_log(db, job_id, "mj: generator timed out").await;
            // Try to kill by pid if available
            if let Some(pid) = child_pid {
                let _ = std::process::Command::new("kill").arg("-9").arg(pid.to_string()).spawn();
            }
            return None;
        }
    }
}

// ────────────────────────────────────────────────────────────────
// Real FLUX / Stable-Diffusion integration (free, self-hosted image server)
// ────────────────────────────────────────────────────────────────

/// Generate images via a self-hosted FLUX.1 [schnell] REST server (the free OSS alternative to
/// Midjourney — see `scripts/kaggle_flux/`). Mirrors `real_mj`'s contract: returns a list of
/// image URLs (here, absolute URLs pointing back at the tunnelled server, which the downstream
/// video step downloads just like Midjourney CDN URLs).
///
/// Server contract: `POST {base}/generate` with `{prompt, num_images, steps, width, height}` →
/// `{"images": ["/images/…png", …]}` (server-relative paths served by the same host).
async fn real_flux(
    prompt: &str,
    settings: &Value,
    job_id: &str,
    db: &crate::store::Db,
    cancelled: &CancelSet,
) -> Option<Vec<String>> {
    let mut base = settings.get("flux_api_url").and_then(|v| v.as_str()).unwrap_or("").trim().trim_end_matches('/').to_string();
    crate::idle_guard::touch("flux");
    if base.is_empty() {
        db_log(db, job_id, "flux: api url not configured — set it in Settings (paste the Kaggle/Colab share URL or http://localhost:8002)").await;
        return None;
    }
    let api_key = settings.get("flux_api_key").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();

    if is_cancelled(cancelled, job_id).await {
        db_log(db, job_id, "flux: cancelled by user").await;
        return None;
    }

    let payload = serde_json::json!({
        "prompt": prompt,
        "num_images": 4,
        "steps": 4,
        "width": 1024,
        "height": 1024,
    });

    let client = reqwest::Client::builder()
        // FLUX.1 [schnell] is 4-step but a free/shared GPU may queue; allow generous time.
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .ok()?;

    // Same stale-tunnel recovery as generate_song_api (see there for why): one retry against a
    // freshly re-scraped tunnel URL before giving up.
    let mut res = None;
    for attempt in 0..2 {
        db_log(db, job_id, &format!("flux: requesting images from {}", base)).await;
        let mut rb = client.post(format!("{}/generate", base))
            .header("Accept", "application/json")
            .json(&payload);
        if !api_key.is_empty() {
            rb = rb.header("Authorization", format!("Bearer {}", api_key));
        }
        match rb.send().await {
            Ok(r) => { res = Some(r); break; }
            Err(e) => {
                if attempt == 0 {
                    db_log(db, job_id, &format!("flux: {} is unreachable ({}) — the Kaggle tunnel may have rotated; checking for a fresh URL…", base, e)).await;
                    if let Some(fresh) = crate::commands::settings::refresh_kaggle_url(db, "flux").await {
                        if fresh != base {
                            db_log(db, job_id, &format!("flux: found a fresh tunnel URL, retrying: {}", fresh)).await;
                            base = fresh;
                            continue;
                        }
                    }
                }
                db_log(db, job_id, &format!("flux: request failed: {} (the Kaggle notebook may have stopped — try \"Fetch live URL\" or \"Start server\" in Settings)", e)).await;
                return None;
            }
        }
    }
    let res = res?;
    if !res.status().is_success() {
        db_log(db, job_id, &format!("flux: HTTP {} — check URL and API key", res.status())).await;
        return None;
    }
    let data: Value = match res.json().await {
        Ok(v) => v,
        Err(e) => { db_log(db, job_id, &format!("flux: invalid response: {}", e)).await; return None; }
    };

    let images: Vec<String> = data["images"].as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(|p| {
            if p.starts_with("http") { p.to_string() }
            else { format!("{}{}", base, if p.starts_with('/') { p.to_string() } else { format!("/{}", p) }) }
        }).collect())
        .unwrap_or_default();

    if images.is_empty() {
        db_log(db, job_id, "flux: server returned no images").await;
        return None;
    }
    db_log(db, job_id, &format!("flux: received {} image(s)", images.len())).await;
    Some(images)
}

/// Generate through one of the paid image APIs (fal.ai, Leonardo, Ideogram, Recraft).
///
/// One function for four providers: `image_apis.rs` holds everything that differs between them, and
/// everything that differs is data rather than a branch. What is left here is the part that is the
/// same everywhere — send, wait if it polls, read the URLs out.
///
/// Every log line names the engine and says money is involved, because the failure that costs
/// somebody real money is the one they cannot see happening.
async fn real_paid_image(
    engine: &str,
    prompt: &str,
    negative: Option<&str>,
    aspect: &str,
    settings: &Value,
    job_id: &str,
    db: &crate::store::Db,
    cancelled: &CancelSet,
) -> Option<Vec<String>> {
    use crate::image_apis;

    let p = image_apis::provider(engine)?;
    let key = image_apis::api_key(p, settings);
    if key.is_empty() {
        db_log(db, job_id, &image_apis::missing_key_message(p)).await;
        return None;
    }
    if is_cancelled(cancelled, job_id).await {
        db_log(db, job_id, &format!("{}: cancelled before anything was charged", p.id)).await;
        return None;
    }

    let (w, h) = match aspect {
        "16:9" => (1920u32, 1080u32),
        "9:16" => (1080, 1920),
        "4:3" => (1440, 1080),
        "2:3" => (1600, 2400),
        _ => (1024, 1024),
    };
    let count = 1u32;
    let body = image_apis::request_body(p, settings, prompt, negative, aspect, w, h, count);

    if negative.map(|n| !n.trim().is_empty()).unwrap_or(false) && !image_apis::honours_negative(p) {
        // Said out loud rather than dropped in silence: on an engine with no negative prompt, the
        // restraint has to be carried by the positive one, and the user needs to know that.
        db_log(db, job_id, &format!(
            "{}: this engine has no negative prompt, so the \"avoid\" text was not sent. Put the \
             restraint in the prompt itself.", p.id)).await;
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build().ok()?;
    let url = image_apis::generate_url(p, settings);
    let (header, value) = image_apis::auth_header(p, &key);

    db_log(db, job_id, &format!("{}: generating {} image(s) — about ${:.3} each",
                                p.id, count, p.price_hint)).await;
    let res = match client.post(&url).header(header, value.clone())
        .header("Content-Type", "application/json").json(&body).send().await {
        Ok(r) => r,
        Err(err) => { db_log(db, job_id, &format!("{}: {url} could not be reached: {err}", p.id)).await; return None; }
    };
    let status = res.status();
    let started: Value = match res.json().await {
        Ok(v) => v,
        Err(err) => { db_log(db, job_id, &format!("{}: HTTP {status}, unreadable answer: {err}", p.id)).await; return None; }
    };
    if !status.is_success() {
        // The provider's own words: "insufficient credits" is worth reading, and a generic failure
        // message would hide exactly the thing the user has to act on.
        let detail = started["error"].as_str()
            .or_else(|| started["detail"].as_str())
            .or_else(|| started["message"].as_str())
            .unwrap_or("no detail given");
        db_log(db, job_id, &format!("{}: HTTP {status} — {detail}", p.id)).await;
        return None;
    }

    let mut images = image_apis::extract_images(p, &started);
    if images.is_empty() {
        if let Some(poll) = image_apis::poll_url(p, settings, &started) {
            // The job is running and already costing money, so a cancel from here stops waiting but
            // cannot un-spend it; say so rather than implying the charge was avoided.
            for attempt in 0..60 {
                if is_cancelled(cancelled, job_id).await {
                    db_log(db, job_id, &format!(
                        "{}: stopped waiting. The request was already sent, so it may still be \
                         charged.", p.id)).await;
                    return None;
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                let Ok(r) = client.get(&poll).header(header, value.clone()).send().await else { continue };
                let Ok(body) = r.json::<Value>().await else { continue };
                if let Some(done) = image_apis::poll_done(p, &body) { images = done; break; }
                if attempt == 59 {
                    db_log(db, job_id, &format!("{}: still not finished after two minutes.", p.id)).await;
                }
            }
        }
    }

    if images.is_empty() {
        db_log(db, job_id, &format!("{}: no images came back.", p.id)).await;
        return None;
    }
    db_log(db, job_id, &format!("{}: {} image(s) received.", p.id, images.len())).await;
    Some(images)
}

// The bundled ComfyUI graphs now live in `comfy_registry.rs`, which owns both the templates and the
// numbers that go with them — the two were drifting apart while the templates were included here and
// the steps came from a separate model profile.

/// Style presets: (positive prompt prefix, base negative prompt). These are the built-in
/// "style filters"; per-channel sticky styles (a later phase) will layer on top of these.
/// The look for a style id, preferring the composable catalogue.
///
/// `image_styles.rs` holds the styles that can be blended (and the named mixes); the match below is
/// the original six, kept so every setting already stored on somebody's machine still resolves to
/// exactly what it used to. New looks go in the catalogue, not here.
fn comfy_look(style: &str) -> (String, String) {
    if let Some(look) = crate::image_styles::resolve(style) {
        return (format!("{}, ", look.prompt), look.negative);
    }
    let (p, n) = comfy_style_preset(style);
    (p.to_string(), n.to_string())
}

fn comfy_style_preset(style: &str) -> (&'static str, &'static str) {
    match style {
        "comic" => ("comic book style, bold ink outlines, halftone shading, dynamic paneling, vibrant flat colors, ",
                    "photorealistic, 3d render, blurry, lowres, watermark, text, signature"),
        "graphic_novel" => ("graphic novel illustration, moody cinematic lighting, painterly ink, detailed linework, muted palette, ",
                    "cartoonish, flat, low detail, blurry, watermark, text, signature"),
        "anime" => ("anime key visual, clean cel shading, expressive, studio quality, ",
                    "photorealistic, 3d, blurry, extra fingers, watermark, text"),
        "oil_painting" => ("classical oil painting, visible brushstrokes, rich impasto texture, chiaroscuro lighting, ",
                    "photograph, 3d render, blurry, watermark, text"),
        "watercolor" => ("delicate watercolor painting, soft color washes, paper texture, gentle gradients, ",
                    "photograph, hard edges, 3d render, blurry, watermark, text"),
        _ /* photoreal */ => ("ultra realistic photograph, natural lighting, sharp focus, fine detail, 85mm, ",
                    "cartoon, illustration, painting, lowres, blurry, deformed, watermark, text, extra limbs"),
    }
}

/// Per-checkpoint sampler config. Different SDXL-family checkpoints need very different settings:
/// a Lightning/Turbo model run at SDXL-base's 30 steps + cfg 6.5 produces burnt, oversaturated
/// garbage, and SDXL base run at Lightning's 6 steps + cfg 1.5 produces mush. Picking these off the
/// checkpoint name is what "best config for the model, automatically" means here.
struct ComfyProfile {
    steps: i64,
    cfg: f64,
    sampler: &'static str,
    scheduler: &'static str,
    /// A human tag shown in the job log so it's obvious which profile was applied.
    label: &'static str,
}

/// Resolve a config profile from a checkpoint filename. Recognises the distinctive tokens rather
/// than exact filenames so a re-quantised or renamed community build still gets sane settings.
fn comfy_model_profile(ckpt: &str) -> ComfyProfile {
    let c = ckpt.to_ascii_lowercase();
    if c.contains("lightning") || c.contains("_lcm") || c.contains("-lcm") || c.contains("hyper") {
        // Distilled few-step models: low steps, cfg ~1.5, euler/sgm_uniform.
        ComfyProfile { steps: 8, cfg: 1.6, sampler: "euler", scheduler: "sgm_uniform", label: "lightning/few-step" }
    } else if c.contains("turbo") {
        ComfyProfile { steps: 6, cfg: 2.0, sampler: "dpmpp_sde", scheduler: "karras", label: "turbo" }
    } else {
        // Full SDXL-family checkpoints (base, Juggernaut-style community models, etc.).
        ComfyProfile { steps: 30, cfg: 6.5, sampler: "dpmpp_2m", scheduler: "karras", label: "sdxl-standard" }
    }
}

/// Which node classes this server actually has.
///
/// `/object_info` lists every registered class, so a graph's custom-node requirements can be checked
/// before anything is submitted. An empty answer means "could not tell" rather than "none installed",
/// and the caller treats it as such — a server that will not answer must not block a render that
/// would have worked.
async fn comfy_installed_nodes(client: &reqwest::Client, base: &str, api_key: &str) -> Vec<String> {
    let mut rb = client.get(format!("{base}/object_info"))
        .timeout(std::time::Duration::from_secs(20));
    if !api_key.is_empty() { rb = rb.header("Authorization", format!("Bearer {api_key}")); }
    let Ok(res) = rb.send().await else { return Vec::new() };
    if !res.status().is_success() { return Vec::new(); }
    let Ok(body) = res.json::<Value>().await else { return Vec::new() };
    body.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default()
}

/// Ask a running ComfyUI what checkpoints it actually has, via the same `/object_info` endpoint its
/// own web UI uses to populate the checkpoint dropdown. Version-independent (every ComfyUI exposes
/// it) and authoritative, so the app never has to trust a hand-typed filename that may not exist on
/// this particular server. Returns the installed checkpoint filenames, or `None` if the probe fails
/// (caller then just trusts the configured value).
async fn comfy_installed_checkpoints(
    client: &reqwest::Client,
    base: &str,
    with_auth: &impl Fn(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
) -> Option<Vec<String>> {
    let res = with_auth(client.get(format!("{}/object_info/CheckpointLoaderSimple", base)))
        .timeout(std::time::Duration::from_secs(20))
        .send().await.ok()?;
    if !res.status().is_success() { return None; }
    let v: Value = res.json().await.ok()?;
    // Shape: {"CheckpointLoaderSimple": {"input": {"required": {"ckpt_name": [[<names>], {...}]}}}}
    let names = v["CheckpointLoaderSimple"]["input"]["required"]["ckpt_name"]
        .get(0)?
        .as_array()?
        .iter()
        .filter_map(|n| n.as_str().map(|s| s.to_string()))
        .collect::<Vec<_>>();
    if names.is_empty() { None } else { Some(names) }
}

/// Choose the best installed SDXL-family checkpoint when the configured one is absent, so a freshly
/// provisioned server "just works" without the user hand-typing whatever the notebook happened to
/// download. Prefers a few-step (Lightning/Turbo) model for speed on a free T4, then SDXL base, then
/// anything. FLUX checkpoints are excluded — they need a different graph (the separate flux engine).
fn comfy_pick_checkpoint(installed: &[String]) -> Option<String> {
    let sdxl_ish: Vec<&String> = installed.iter()
        .filter(|n| { let l = n.to_ascii_lowercase(); !l.contains("flux") && !l.contains("ae.safetensors") })
        .collect();
    let by = |pred: &dyn Fn(&str) -> bool| sdxl_ish.iter().find(|n| pred(&n.to_ascii_lowercase())).map(|s| s.to_string());
    by(&|l| l.contains("lightning") || l.contains("turbo") || l.contains("hyper") || l.contains("lcm"))
        .or_else(|| by(&|l| l.contains("xl_base") || l.contains("xl-base") || l.contains("sdxl")))
        .or_else(|| sdxl_ish.first().map(|s| s.to_string()))
}

/// Fill an integer/float/string token in a workflow template. String values are inserted as
/// JSON-escaped strings (replacing the quoted token `"__TOKEN__"`); numbers replace the bare token.
fn fill_str(t: &str, token: &str, val: &str) -> String {
    let quoted = serde_json::to_string(val).unwrap_or_else(|_| "\"\"".into());
    t.replace(&format!("\"{token}\""), &quoted)
}

/// Generate images via a self-hosted ComfyUI server. Returns absolute `/view` URLs (downloaded by
/// the video step like any other image engine). When `ref_image` is set and the character workflow
/// is used, the reference is uploaded and driven through IP-Adapter for character consistency.
async fn real_comfy(
    prompt: &str,
    ref_image: Option<&str>,
    settings: &Value,
    job_id: &str,
    db: &crate::store::Db,
    cancelled: &CancelSet,
) -> Option<Vec<String>> {
    let base = settings.get("comfyui_api_url").and_then(|v| v.as_str()).unwrap_or("").trim().trim_end_matches('/').to_string();
    crate::idle_guard::touch("comfyui");
    if base.is_empty() {
        db_log(db, job_id, "comfy: api url not configured — set it in Settings (paste the ComfyUI share URL or http://localhost:8188)").await;
        return None;
    }
    let api_key = settings.get("comfyui_api_key").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let with_auth = |rb: reqwest::RequestBuilder| {
        if api_key.is_empty() { rb } else { rb.header("Authorization", format!("Bearer {}", api_key)) }
    };

    let style = settings.get("comfyui_style").and_then(|v| v.as_str()).unwrap_or("photoreal");
    let (prefix, base_neg) = comfy_look(style);
    let (prefix, base_neg) = (prefix.as_str(), base_neg.as_str());
    let extra_neg = settings.get("comfyui_negative").and_then(|v| v.as_str()).unwrap_or("");
    // Style-nuance prefix (often set per-channel) sits between the preset prefix and the prompt.
    let nuance = settings.get("comfyui_prompt_prefix").and_then(|v| v.as_str()).unwrap_or("").trim();
    let full_prompt = if nuance.is_empty() {
        format!("{}{}", prefix, prompt)
    } else {
        format!("{}{}, {}", prefix, nuance, prompt)
    };
    let negative = if extra_neg.trim().is_empty() { base_neg.to_string() } else { format!("{}, {}", base_neg, extra_neg.trim()) };

    let width = settings.get("comfyui_width").and_then(|v| v.as_i64()).unwrap_or(1024);
    let height = settings.get("comfyui_height").and_then(|v| v.as_i64()).unwrap_or(1024);
    let ip_weight = settings.get("comfyui_ip_weight").and_then(|v| v.as_f64()).unwrap_or(0.7);
    let batch = 2i64;
    let seed: i64 = (Uuid::new_v4().as_u128() & 0x7fff_ffff) as i64;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build().ok()?;

    set_stage(db, job_id, "preparing", 6, "Checking the ComfyUI model setup…", true).await;

    // Resolve the checkpoint against what the server ACTUALLY has. A hand-typed or stale
    // `comfyui_ckpt` that isn't installed makes /prompt fail with a cryptic node error; instead we
    // ask the server (via /object_info) and auto-pick the best installed SDXL checkpoint so a fresh
    // Kaggle server works with zero configuration.
    let configured_ckpt = settings.get("comfyui_ckpt").and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty()).unwrap_or("sd_xl_base_1.0.safetensors").to_string();
    let ckpt = match comfy_installed_checkpoints(&client, &base, &with_auth).await {
        Some(installed) if installed.iter().any(|n| n == &configured_ckpt) => configured_ckpt,
        Some(installed) => {
            match comfy_pick_checkpoint(&installed) {
                Some(pick) => {
                    db_log(db, job_id, &format!(
                        "comfy: configured checkpoint '{}' not on the server; using '{}' (installed: {})",
                        configured_ckpt, pick, installed.join(", "))).await;
                    pick
                }
                None => {
                    db_log(db, job_id, &format!(
                        "comfy: server reports NO usable checkpoints (installed: {}) — check the notebook's model download cell",
                        installed.join(", "))).await;
                    return None;
                }
            }
        }
        None => configured_ckpt, // probe failed (old server / auth) — trust the configured value
    };

    // Model-critical sampler config. `comfyui_auto_config` (default on) lets the resolved
    // checkpoint's profile drive steps/cfg/sampler/scheduler, which is what makes a Lightning/Turbo
    // model actually usable; turn it off to force the explicit Settings values.
    let auto_config = settings.get("comfyui_auto_config").and_then(|v| v.as_bool()).unwrap_or(true);
    let profile = comfy_model_profile(&ckpt);
    let (steps, cfg, sampler, scheduler) = if auto_config {
        (profile.steps, profile.cfg, profile.sampler, profile.scheduler)
    } else {
        (settings.get("comfyui_steps").and_then(|v| v.as_i64()).unwrap_or(30),
         settings.get("comfyui_cfg").and_then(|v| v.as_f64()).unwrap_or(6.5),
         "dpmpp_2m", "karras")
    };
    db_log(db, job_id, &format!(
        "comfy: checkpoint '{}' [{}] — {} steps, cfg {:.1}, {}/{}",
        ckpt, profile.label, steps, cfg, sampler, scheduler)).await;
    let ckpt = ckpt.as_str();

    // If a reference image is provided, upload it to ComfyUI's input folder for IP-Adapter.
    let mut ref_name: Option<String> = None;
    let use_character = ref_image.map(|s| !s.trim().is_empty()).unwrap_or(false);
    if use_character {
        let src = ref_image.unwrap().trim();
        let bytes = if src.starts_with("http") {
            match client.get(src).send().await.ok() {
                Some(r) => r.bytes().await.ok().map(|b| b.to_vec()),
                None => None,
            }
        } else {
            tokio::fs::read(src).await.ok()
        };
        if let Some(data) = bytes {
            let part = reqwest::multipart::Part::bytes(data)
                .file_name("ref.png")
                .mime_str("image/png").ok()?;
            let form = reqwest::multipart::Form::new()
                .part("image", part)
                .text("type", "input")
                .text("overwrite", "true");
            match with_auth(client.post(format!("{}/upload/image", base))).multipart(form).send().await {
                Ok(r) if r.status().is_success() => {
                    if let Ok(v) = r.json::<Value>().await {
                        ref_name = v["name"].as_str().map(|s| s.to_string());
                    }
                }
                Ok(r) => db_log(db, job_id, &format!("comfy: reference upload HTTP {} — proceeding without character reference", r.status())).await,
                Err(e) => db_log(db, job_id, &format!("comfy: reference upload failed: {} — proceeding without reference", e)).await,
            }
        }
    }

    // Pick the graph. A user-supplied custom workflow (e.g. AnimateDiff exported from the ComfyUI
    // UI) still wins when workflow mode is "custom"; otherwise `comfy_registry` decides, because the
    // choice depends on three things at once — what is being asked for, which model is loaded, and
    // whether this is a draft or a final — and two of the graphs cannot run on the wrong model at all.
    use crate::comfy_registry::{self, Want, Quality};
    let wf_mode = settings.get("comfyui_workflow_mode").and_then(|v| v.as_str()).unwrap_or("auto");
    let custom = settings.get("comfyui_custom_workflow").and_then(|v| v.as_str()).unwrap_or("").trim();

    let want = if ref_name.is_some() {
        Want::SameCharacter
    } else {
        Want::parse(settings.get("comfyui_want").and_then(|v| v.as_str()).unwrap_or(""))
    };
    let quality = if settings.get("comfyui_draft").and_then(|v| v.as_bool()).unwrap_or(false) {
        Quality::Draft
    } else {
        Quality::Final
    };
    let strict = settings.get("imagery").and_then(|i| i.get("controls"))
        .and_then(|c| c.get("strict")).and_then(|v| v.as_bool()).unwrap_or(false);
    let choice = comfy_registry::choose(want, ckpt, quality, strict);

    // A missing custom node otherwise surfaces as a /prompt rejection naming an internal class,
    // which reads like the server is broken rather than like a five-minute install. Ask first.
    if !choice.needs_nodes.is_empty() {
        let installed = comfy_installed_nodes(&client, &base, &api_key).await;
        if !installed.is_empty() {
            let missing = comfy_registry::missing_nodes(&choice, &installed);
            if !missing.is_empty() {
                db_log(db, job_id, &comfy_registry::missing_node_advice(&missing)).await;
                set_stage(db, job_id, "failed", 100,
                          "This ComfyUI server is missing a node this workflow needs.", true).await;
                return None;
            }
        }
    }
    if let Some(note) = choice.note {
        db_log(db, job_id, &format!("comfy: {note}")).await;
    }
    if !comfy_registry::honours_negative(&choice) && !negative.trim().is_empty() {
        // Said out loud rather than dropped in silence: on FLUX the restraint has to be carried by
        // the positive prompt, and somebody who typed it into "avoid" deserves to know it was not.
        db_log(db, job_id,
               "comfy: this model has no negative prompt, so the \"avoid\" text was not sent. \
                Put the restraint in the prompt itself, or switch on strict mode.").await;
    }

    let template: &str = if wf_mode == "custom" && !custom.is_empty() { custom } else { choice.template };
    // The registry's numbers win over the generic profile: a print pass and a draft are not the
    // same job at different sizes, and a few-step model given thirty steps burns rather than improves.
    let (steps, cfg) = if wf_mode == "custom" && !custom.is_empty() { (steps, cfg) } else { (choice.steps, choice.cfg) };
    db_log(db, job_id, &format!("comfy: workflow '{}' — {} steps, cfg {:.1}", choice.name, steps, cfg)).await;
    let mut wf = template.to_string();
    wf = wf.replace("__DENOISE__", &format!("{:.2}", choice.denoise));
    wf = fill_str(&wf, "__INPUT_IMAGE__", ref_name.as_deref().unwrap_or(""));
    wf = fill_str(&wf, "__UPSCALER__",
                  settings.get("comfyui_upscaler").and_then(|v| v.as_str())
                          .unwrap_or("4x-UltraSharp.pth"));
    // The style LoRA, for the graph that carries one. Filled unconditionally rather than only for
    // that graph: an unfilled placeholder survives into the submitted JSON and fails as "workflow
    // template invalid after fill", which points at the template instead of at the empty setting
    // that actually caused it. No default name — ComfyUI has no LoRA to fall back to, and inventing
    // a filename produces a node error further away from the cause.
    wf = fill_str(&wf, "__LORA__",
                  settings.get("comfyui_lora").and_then(|v| v.as_str()).unwrap_or(""));
    // Same contract as the LoRA above: filled unconditionally so an unused placeholder can never
    // survive into the submitted graph, and with no invented default, because a wrong ControlNet
    // name fails further from its cause than an empty one.
    wf = fill_str(&wf, "__CONTROLNET__",
                  settings.get("comfyui_controlnet").and_then(|v| v.as_str()).unwrap_or(""));
    // A second LoRA, a second checkpoint, a second prompt — for the stacking, refiner and
    // blend/area graphs. Same contract as above: always filled, so an unused token can never reach
    // the server as literal text.
    wf = fill_str(&wf, "__LORA2__",
                  settings.get("comfyui_lora2").and_then(|v| v.as_str()).unwrap_or(""));
    wf = wf.replace("__LORA2_STRENGTH__", &format!("{:.2}",
                  settings.get("comfyui_lora2_strength").and_then(|v| v.as_f64())
                          .filter(|v| *v > 0.0 && *v <= 2.0).unwrap_or(0.65)));
    wf = fill_str(&wf, "__CKPT2__",
                  settings.get("comfyui_refiner_ckpt").and_then(|v| v.as_str()).unwrap_or(""));
    // The second prompt falls back to the first rather than to nothing: an empty conditioning in a
    // blend or an area graph does not degrade the picture, it erases half of it.
    {
        let p2 = settings.get("comfyui_prompt2").and_then(|v| v.as_str())
                         .map(str::trim).filter(|s| !s.is_empty())
                         .map(str::to_string).unwrap_or_else(|| full_prompt.clone());
        wf = fill_str(&wf, "__PROMPT2__", &p2);
    }
    wf = wf.replace("__LORA_STRENGTH__", &format!("{:.2}",
                  settings.get("comfyui_lora_strength").and_then(|v| v.as_f64())
                          .filter(|v| *v > 0.0 && *v <= 2.0).unwrap_or(0.85)));
    wf = fill_str(&wf, "__CKPT__", ckpt);
    wf = fill_str(&wf, "__PROMPT__", &full_prompt);
    wf = fill_str(&wf, "__NEGATIVE__", &negative);
    if let Some(rn) = &ref_name { wf = fill_str(&wf, "__REF_IMAGE__", rn); }
    wf = fill_str(&wf, "__SAMPLER__", sampler);
    wf = fill_str(&wf, "__SCHEDULER__", scheduler);
    wf = wf.replace("__WIDTH__", &width.to_string())
           .replace("__HEIGHT__", &height.to_string())
           .replace("__BATCH__", &batch.to_string())
           .replace("__SEED__", &seed.to_string())
           .replace("__STEPS__", &steps.to_string())
           .replace("__CFG__", &format!("{:.2}", cfg))
           .replace("__IPWEIGHT__", &format!("{:.2}", ip_weight));

    let workflow: Value = match serde_json::from_str(&wf) {
        Ok(v) => v,
        Err(e) => { db_log(db, job_id, &format!("comfy: workflow template invalid after fill: {}", e)).await; return None; }
    };

    let client_id = Uuid::new_v4().to_string();
    set_stage(db, job_id, "submitting", 12, &format!("Submitting the {} workflow…", style), true).await;
    db_log(db, job_id, &format!("comfy: submitting '{}' workflow ({} steps, cfg {:.1}) to {}", style, steps, cfg, base)).await;

    let submit = with_auth(client.post(format!("{}/prompt", base)))
        .json(&serde_json::json!({ "prompt": workflow, "client_id": client_id }))
        .send().await;
    let submit = match submit {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            let body = r.text().await.unwrap_or_default();
            let short = if body.len() > 300 { &body[..300] } else { &body };
            db_log(db, job_id, &format!("comfy: /prompt rejected the workflow: {}", short)).await;
            return None;
        }
        Err(e) => { db_log(db, job_id, &format!("comfy: request failed: {} (server reachable?)", e)).await; return None; }
    };
    let prompt_id = match submit.json::<Value>().await {
        Ok(v) => match v["prompt_id"].as_str() { Some(id) => id.to_string(), None => { db_log(db, job_id, &format!("comfy: no prompt_id in response: {}", v)).await; return None; } },
        Err(e) => { db_log(db, job_id, &format!("comfy: invalid submit response: {}", e)).await; return None; }
    };

    set_stage(db, job_id, "generating", 15, "Rendering on the GPU…", true).await;
    db_log(db, job_id, &format!("comfy: queued {}, polling for output...", prompt_id)).await;
    let poll_started = std::time::Instant::now();

    for _ in 0..90 {
        tokio::time::sleep(tokio::time::Duration::from_secs(4)).await;
        if is_cancelled(cancelled, job_id).await {
            db_log(db, job_id, "comfy: cancelled by user").await;
            return None;
        }
        let hr = with_auth(client.get(format!("{}/history/{}", base, prompt_id))).send().await;
        let hr = match hr { Ok(r) => r, Err(_) => continue };
        if !hr.status().is_success() { continue; }
        let hist: Value = match hr.json().await { Ok(v) => v, Err(_) => continue };
        let entry = &hist[&prompt_id];
        if entry.is_null() {
            // Not in history yet → still queued/running. Asymptotic 15→85% ramp with a 40s time
            // constant (SDXL on a T4 is ~20-40s for a small batch).
            let secs = poll_started.elapsed().as_secs();
            let pct = 15 + (70.0 * (1.0 - (-(secs as f64) / 40.0).exp())) as i32;
            set_stage(db, job_id, "generating", pct, "Rendering on the GPU…", false).await;
            continue;
        }

        // Gather outputs across all nodes. Still images live under "images"; AnimateDiff / VHS video
        // nodes emit under "gifs" (webp/mp4/gif) or "videos" — collect all so animations flow back.
        let mut urls: Vec<String> = Vec::new();
        if let Some(outputs) = entry["outputs"].as_object() {
            for (_node, out) in outputs {
                for key in ["images", "gifs", "videos"] {
                    if let Some(arr) = out[key].as_array() {
                        for item in arr {
                            let filename = item["filename"].as_str().unwrap_or("");
                            if filename.is_empty() { continue; }
                            let subfolder = item["subfolder"].as_str().unwrap_or("");
                            let itype = item["type"].as_str().unwrap_or("output");
                            urls.push(format!("{}/view?filename={}&subfolder={}&type={}",
                                base,
                                urlencode(filename), urlencode(subfolder), urlencode(itype)));
                        }
                    }
                }
            }
        }
        if !urls.is_empty() {
            set_stage(db, job_id, "fetching", 92, &format!("Rendered {} image(s) — fetching…", urls.len()), true).await;
            return Some(urls);
        }
        // Entry exists but no images yet (or failed).
        if entry["status"]["status_str"].as_str() == Some("error") {
            set_stage(db, job_id, "failed", 100, "ComfyUI reported a workflow error.", true).await;
            db_log(db, job_id, "comfy: workflow errored on the server — check the ComfyUI logs / node names").await;
            return None;
        }
    }

    db_log(db, job_id, "comfy: timeout — server overloaded or share URL expired").await;
    None
}

/// Whitelist of ffmpeg `xfade` transition names we allow (guards the filter string and gives the
/// UI/AI a fixed vocabulary). Unknown values fall back to a plain crossfade.
const XFADE_TRANSITIONS: &[&str] = &[
    "fade", "fadeblack", "fadewhite", "dissolve", "distance",
    "wipeleft", "wiperight", "wipeup", "wipedown",
    "slideleft", "slideright", "slideup", "slidedown",
    "smoothleft", "smoothright", "smoothup", "smoothdown",
    "circleopen", "circleclose", "circlecrop", "rectcrop", "radial",
    "pixelize", "hlslice", "hrslice", "vuslice", "vdslice",
    "diagtl", "diagtr", "diagbl", "diagbr", "zoomin", "squeezev", "squeezeh",
];
fn sanitize_transition(t: &str) -> String {
    let t = t.trim();
    if XFADE_TRANSITIONS.contains(&t) { t.to_string() } else { "fade".to_string() }
}

/// Minimal percent-encoding for ComfyUI /view query values.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Dispatch image generation to the configured engine ("midjourney" default, "flux", or "comfyui").
/// `ref_image` (a URL or local path) is only used by ComfyUI's character-consistency workflow;
/// the other engines ignore it. All return a list of image URLs.
async fn gen_images(
    prompt: &str,
    negative: Option<&str>,
    aspect: &str,
    ref_image: Option<&str>,
    settings: &Value,
    job_id: &str,
    db: &crate::store::Db,
    cancelled: &CancelSet,
) -> Option<Vec<String>> {
    // FLUX by default, not Midjourney. Midjourney has no official API, and every route to it drives a
    // Discord account with a user token — self-botting, against Discord's terms, and a terminated account
    // takes the Midjourney subscription with it. That is not a risk to ship inside something sold
    // publicly, so the default is an engine with a real API and no terms to breach.
    let engine = settings.get("image_engine").and_then(|v| v.as_str()).unwrap_or("flux");
    // The paid APIs first: fal.ai, Leonardo, Ideogram and Recraft are all one shape (see
    // image_apis.rs), so they share a single arm rather than four near-identical ones.
    if crate::image_apis::is_paid_api(engine) {
        // The destination's shape and its negative prompt travel with the prompt itself, so a
        // Leonardo render honours the same choice a ComfyUI one does. Reading them back out of
        // settings here meant the imagery panel's decision reached the prompt and nothing else.
        return real_paid_image(engine, prompt, negative, aspect, settings, job_id, db, cancelled).await;
    }
    match engine {
        "comfyui" | "comfy" => real_comfy(prompt, ref_image, settings, job_id, db, cancelled).await,
        // Midjourney is named explicitly rather than being the fallback arm. It used to catch every
        // unrecognised id, which meant a typo or a since-renamed engine quietly drove the user's own
        // Midjourney account — the one outcome this whole arrangement exists to avoid.
        "midjourney" => real_mj(prompt, settings, job_id, db, cancelled).await,
        _ => real_flux(prompt, settings, job_id, db, cancelled).await,
    }
}

// ────────────────────────────────────────────────────────────────
// Real FFmpeg video composition
// ────────────────────────────────────────────────────────────────

/// Resolve the ffmpeg binary: configured path → system which → bundled resource.
fn resolve_ffmpeg(settings: &Value) -> Option<String> {
    let ff = settings.get("ffmpeg_path").and_then(|v| v.as_str()).unwrap_or("ffmpeg");
    let mut ff_path = ff.to_string();
    if which::which(&ff_path).is_err() {
        if let Ok(exe_path) = env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                for c in [parent.join("ffmpeg"), parent.join("ffmpeg.exe"),
                          parent.join("bin").join("ffmpeg"), parent.join("bin").join("ffmpeg.exe")] {
                    if c.is_file() { ff_path = c.to_string_lossy().to_string(); break; }
                }
            }
        }
    }
    if which::which(&ff_path).is_ok() { Some(ff_path) } else { None }
}

// ────────────────────────────────────────────────────────────────
// Companion overlay generation (Overlay Studio)
// ────────────────────────────────────────────────────────────────
// Renders an audio-reactive animation video from the song's audio, cheap enough for
// ~100 songs/day on CPU: ffmpeg's visualizer filters (default), or an external
// projectM command for the fancy OpenGL tier when the user has one configured.

fn overlay_filter_for(style: &str, w: i64, h: i64) -> String {
    // All styles render bright-on-black, which is exactly what the "screen" blend wants
    // (black disappears, the animation glows over the imagery).
    let vis = match style {
        "spectrum" => format!("showspectrum=s={w}x{h}:mode=combined:color=fire:slide=scroll"),
        "waves" => format!("showwaves=s={w}x{h}:mode=cline:colors=White"),
        "vectorscope" => format!("avectorscope=s={w}x{h}:draw=line:zoom=1.5"),
        "freqs" => format!("showfreqs=s={w}x{h}:mode=bar"),
        _ => format!("showcqt=s={w}x{h}"), // "cqt" default: musical, piano-roll-like
    };
    format!("[0:a]{vis},fps=24,format=yuv420p[v]")
}

async fn real_overlay(
    song: &Value,
    settings: &Value,
    job_id: &str,
    db: &crate::store::Db,
) -> Option<Value> {
    let song_id = song["id"].as_str()?;
    let audio_url = match song["audio_url"].as_str() {
        Some(u) if !u.is_empty() => u.to_string(),
        _ => { db_log(db, job_id, "overlay: song has no audio yet — generate music first").await; return None; }
    };
    let Some(ff_path) = resolve_ffmpeg(settings) else {
        db_log(db, job_id, "overlay: ffmpeg not found").await; return None;
    };

    let out_dir = std::path::PathBuf::from(format!("/tmp/studio_out/{song_id}"));
    let _ = tokio::fs::create_dir_all(&out_dir).await;
    let audio_path = out_dir.join("overlay_audio.mp3");
    let out_file = out_dir.join("overlay_companion.mp4");

    db_log(db, job_id, "overlay: downloading audio...").await;
    let client = reqwest::Client::new();
    match client.get(&audio_url).send().await {
        Ok(res) => match res.bytes().await {
            Ok(bytes) => {
                if tokio::fs::write(&audio_path, bytes).await.is_err() {
                    db_log(db, job_id, "overlay: failed to save audio").await; return None;
                }
            }
            Err(_) => { db_log(db, job_id, "overlay: failed to read audio").await; return None; }
        },
        Err(_) => { db_log(db, job_id, "overlay: failed to fetch audio").await; return None; }
    }

    let width = settings.get("video_width").and_then(|v| v.as_i64()).unwrap_or(1280).max(16);
    let height = settings.get("video_height").and_then(|v| v.as_i64()).unwrap_or(720).max(16);
    let style = settings.get("overlay_style").and_then(|v| v.as_str()).unwrap_or("cqt").to_string();
    let engine = settings.get("overlay_engine").and_then(|v| v.as_str()).unwrap_or("ffmpeg").to_string();
    let projectm = settings.get("projectm_path").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();

    let audio_s = audio_path.to_string_lossy().to_string();
    let out_s = out_file.to_string_lossy().to_string();

    // Opt-in projectM tier: run the user-configured external renderer; on any failure
    // fall through to the ffmpeg style so the job still produces an overlay.
    let mut done = false;
    if engine == "projectm" && !projectm.is_empty() {
        db_log(db, job_id, "overlay: rendering with projectM (external command)...").await;
        let cmdline = if projectm.contains("{audio}") || projectm.contains("{out}") {
            projectm.replace("{audio}", &audio_s).replace("{out}", &out_s)
        } else {
            format!("{} \"{}\" \"{}\"", projectm, audio_s, out_s)
        };
        let status = tokio::process::Command::new("sh").arg("-c").arg(&cmdline)
            .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
            .status().await;
        done = matches!(status, Ok(s) if s.success()) && out_file.is_file();
        if !done {
            db_log(db, job_id, "overlay: projectM command failed — falling back to the ffmpeg style").await;
        }
    }

    if !done {
        db_log(db, job_id, &format!("overlay: rendering {}x{} '{}' visualization (CPU)...", width, height, style)).await;
        let filter = overlay_filter_for(&style, width, height);
        let ff = ff_path.clone();
        let (audio_c, out_c) = (audio_s.clone(), out_s.clone());
        let ok: bool = tokio::task::spawn_blocking(move || {
            std::process::Command::new(&ff)
                .args(["-y", "-i", &audio_c,
                       "-filter_complex", &filter, "-map", "[v]", "-an",
                       "-c:v", "libx264", "-preset", "veryfast", "-crf", "27",
                       "-pix_fmt", "yuv420p", &out_c])
                .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
                .status().map(|s| s.success()).unwrap_or(false)
        }).await.unwrap_or(false);
        if !ok || !out_file.is_file() {
            db_log(db, job_id, "overlay: ffmpeg render failed").await;
            return None;
        }
    }

    let _ = tokio::fs::remove_file(&audio_path).await;
    db_log(db, job_id, "overlay: companion overlay ready").await;
    let _ = db.collection::<Document>("songs").update_one(
        doc! { "id": song_id },
        doc! { "$set": { "overlay_local_path": &out_s, "overlay_style": &style, "overlay_generated_at": now_iso() } },
    ).await;
    Some(serde_json::json!({ "overlay_local_path": out_s, "style": style, "real": true }))
}

async fn real_ffmpeg(
    song: &Value,
    sections: &[Value],
    settings: &Value,
    job_id: &str,
    db: &crate::store::Db,
) -> Option<Value> {
    let ff = settings.get("ffmpeg_path").and_then(|v|v.as_str()).unwrap_or("ffmpeg");
    // Resolve ffmpeg: prefer configured path, then system `which`, then bundled resource
    let mut ff_path = ff.to_string();
    if which::which(&ff_path).is_err() {
        // Try to find ffmpeg in the resource directory next to the executable
        if let Ok(exe_path) = env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                let candidates = [
                    parent.join("ffmpeg"),
                    parent.join("ffmpeg.exe"),
                    parent.join("bin").join("ffmpeg"),
                    parent.join("bin").join("ffmpeg.exe"),
                ];
                for c in &candidates {
                    if c.exists() && c.is_file() {
                        ff_path = c.to_string_lossy().to_string();
                        break;
                    }
                }
            }
        }
    }
    if which::which(&ff_path).is_err() {
        db_log(db, job_id, &format!("ffmpeg: '{}' not found", ff)).await;
        return None;
    }
    let song_id = song["id"].as_str()?;
    let out_dir = std::path::PathBuf::from(format!("/tmp/studio_out/{song_id}"));
    let _ = tokio::fs::create_dir_all(&out_dir).await;
    let out_file = out_dir.join("video.mp4");

    let audio_url = match song["audio_url"].as_str() {
        Some(u) if !u.is_empty() => u,
        _ => { db_log(db, job_id, "ffmpeg: missing audio_url").await; return None; }
    };
    let total_duration = song["duration"].as_f64().filter(|d| *d > 1.0).unwrap_or(120.0);

    // Order sections and keep only those that have a rendered asset.
    let mut ordered: Vec<&Value> = sections.iter().filter(|s| {
        s["image_url"].as_str().map(|u| !u.is_empty()).unwrap_or(false)
    }).collect();
    ordered.sort_by_key(|s| s["index"].as_i64().unwrap_or(0));
    if ordered.is_empty() {
        db_log(db, job_id, "ffmpeg: no section assets to compose").await;
        return None;
    }
    let n = ordered.len();

    // True when a URL/content-type points at a moving image (AnimateDiff/VHS clip) rather than a still.
    fn is_animated(url: &str, content_type: &str) -> bool {
        let ct = content_type.to_ascii_lowercase();
        if ct.starts_with("video/") || ct.contains("gif") { return true; }
        let u = url.to_ascii_lowercase();
        [".mp4", ".webm", ".mov", ".mkv", ".gif", ".m4v"].iter().any(|e| u.contains(e))
    }

    db_log(db, job_id, &format!("ffmpeg: downloading {} section assets...", n)).await;
    let client = reqwest::Client::new();

    // Transition config: whether to crossfade between scenes, the default transition, and its length.
    let transitions_enabled = settings.get("video_transitions_enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    let trans_dur = settings.get("video_transition_dur").and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))).unwrap_or(0.7).clamp(0.1, 3.0);
    let default_trans = settings.get("video_transition").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).unwrap_or("fade").to_string();

    // Downloaded segment sources: (local path, animated?, duration seconds, transition-into-this-scene).
    let mut segs: Vec<(String, bool, f64, String)> = Vec::new();
    for (i, sec) in ordered.iter().enumerate() {
        let url = sec["image_url"].as_str().unwrap_or("");
        let trans_in = sec["transition"].as_str().filter(|s| !s.is_empty()).unwrap_or(&default_trans).to_string();
        // Per-section duration from analysis start/end, else an even split of the song.
        let dur = {
            let start = sec["start"].as_f64().unwrap_or(f64::NAN);
            let end = sec["end"].as_f64().unwrap_or(f64::NAN);
            let d = end - start;
            if d.is_finite() && d > 0.4 { d } else { total_duration / n as f64 }
        }.max(1.0);

        match client.get(url).send().await {
            Ok(res) => {
                let ct = res.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
                let animated = is_animated(url, &ct);
                let ext = if animated { "mp4" } else { "jpg" };
                match res.bytes().await {
                    Ok(bytes) => {
                        let path = out_dir.join(format!("asset_{i:03}.{ext}"));
                        if tokio::fs::write(&path, &bytes).await.is_ok() {
                            segs.push((path.to_string_lossy().to_string(), animated, dur, trans_in.clone()));
                            db_log(db, job_id, &format!("ffmpeg: asset {}/{} ({}, {:.1}s)", i + 1, n, if animated { "animated" } else { "still" }, dur)).await;
                        } else {
                            db_log(db, job_id, &format!("ffmpeg: failed to save asset {}", i)).await;
                        }
                    }
                    Err(_) => db_log(db, job_id, &format!("ffmpeg: failed to read asset {}", i)).await,
                }
            }
            Err(_) => db_log(db, job_id, &format!("ffmpeg: failed to fetch asset {}", i)).await,
        }
        tokio::task::yield_now().await;
    }
    if segs.is_empty() {
        db_log(db, job_id, "ffmpeg: no assets downloaded").await;
        return None;
    }

    db_log(db, job_id, "ffmpeg: downloading audio...").await;
    let audio_path = out_dir.join("audio.mp3");
    match client.get(audio_url).send().await {
        Ok(res) => match res.bytes().await {
            Ok(bytes) => {
                if tokio::fs::write(&audio_path, bytes).await.is_err() {
                    db_log(db, job_id, "ffmpeg: failed to save audio").await; return None;
                }
            }
            Err(_) => { db_log(db, job_id, "ffmpeg: failed to read audio").await; return None; }
        },
        Err(_) => { db_log(db, job_id, "ffmpeg: failed to fetch audio").await; return None; }
    }

    // Optional looping overlay (light leaks / particles). Download it if configured.
    // With no global overlay set, fall back to the song's own companion overlay
    // (generated in Overlay Studio from this song's audio) when auto-use is on.
    let overlay_src = settings.get("video_overlay_url").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let overlay_path: Option<String> = if overlay_src.is_empty() {
        let auto = settings.get("overlay_auto_use").and_then(|v| v.as_bool()).unwrap_or(true);
        let companion = song.get("overlay_local_path").and_then(|v| v.as_str())
            .filter(|p| std::path::Path::new(p).is_file())
            .map(|s| s.to_string());
        match (auto, companion) {
            (true, Some(p)) => {
                db_log(db, job_id, "ffmpeg: using the song's companion audio-reactive overlay").await;
                Some(p)
            }
            _ => None,
        }
    } else if overlay_src.starts_with("http") {
        let p = out_dir.join("overlay.mp4");
        match client.get(&overlay_src).send().await {
            Ok(res) => match res.bytes().await {
                Ok(bytes) => {
                    if tokio::fs::write(&p, bytes).await.is_ok() {
                        Some(p.to_string_lossy().to_string())
                    } else {
                        db_log(db, job_id, "ffmpeg: overlay save failed — continuing without overlay").await; None
                    }
                }
                Err(_) => { db_log(db, job_id, "ffmpeg: overlay read failed — continuing without overlay").await; None }
            },
            Err(_) => { db_log(db, job_id, "ffmpeg: overlay fetch failed — continuing without overlay").await; None }
        }
    } else {
        Some(overlay_src.clone())
    };

    let width = settings.get("video_width").and_then(|v| v.as_i64()).unwrap_or(1280).max(16);
    let height = settings.get("video_height").and_then(|v| v.as_i64()).unwrap_or(720).max(16);
    let overlay_mode = settings.get("video_overlay_mode").and_then(|v| v.as_str()).unwrap_or("screen").to_string();
    let overlay_opacity = settings.get("video_overlay_opacity").and_then(|v| v.as_f64()).unwrap_or(0.35).clamp(0.0, 1.0);

    db_log(db, job_id, &format!("ffmpeg: composing {}x{} video ({} segments{})...", width, height, n,
        if overlay_path.is_some() { ", with overlay" } else { "" })).await;

    // All ffmpeg work runs in one blocking task; it returns (success, log lines).
    let ff_path = ff_path.clone();
    let out_dir_s = out_dir.to_string_lossy().to_string();
    let audio_path_s = audio_path.to_string_lossy().to_string();
    let output_path = out_file.to_string_lossy().to_string();

    let (ok, logs): (bool, Vec<String>) = tokio::task::spawn_blocking(move || {
        let mut logs: Vec<String> = Vec::new();
        let run = |args: &[String], logs: &mut Vec<String>| -> bool {
            match std::process::Command::new(&ff_path).args(args)
                .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped()).output() {
                Ok(o) if o.status.success() => true,
                Ok(o) => { let err = String::from_utf8_lossy(&o.stderr); let tail: String = err.lines().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" | "); logs.push(format!("ffmpeg: step failed: {}", tail)); false }
                Err(e) => { logs.push(format!("ffmpeg: spawn error: {}", e)); false }
            }
        };
        // Normalize every section into a uniform segment (still held for its duration, or animated
        // clip looped/trimmed to fill it). When transitions are on, each segment is built
        // `trans_dur` longer so the crossfade overlap nets back to the intended timeline.
        let vf = format!("scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black,setsar=1,fps=25,format=yuv420p", w = width, h = height);
        let extra = if transitions_enabled { trans_dur } else { 0.0 };
        // Built segments: (path, section-duration, transition-into-this-scene).
        let mut built: Vec<(String, f64, String)> = Vec::new();
        for (i, (src, animated, dur, trans)) in segs.iter().enumerate() {
            let seg_out = format!("{}/seg_{:03}.mp4", out_dir_s, i);
            let build_len = format!("{:.3}", dur + extra);
            let mut args: Vec<String> = vec!["-y".into()];
            if *animated {
                args.extend(["-stream_loop".into(), "-1".into(), "-i".into(), src.clone()]);
            } else {
                args.extend(["-loop".into(), "1".into(), "-i".into(), src.clone()]);
            }
            args.extend([
                "-t".into(), build_len,
                "-vf".into(), vf.clone(),
                "-an".into(),
                "-c:v".into(), "libx264".into(), "-preset".into(), "veryfast".into(), "-crf".into(), "20".into(),
                "-pix_fmt".into(), "yuv420p".into(),
                seg_out.clone(),
            ]);
            if !run(&args, &mut logs) {
                logs.push(format!("ffmpeg: segment {} failed, skipping", i));
                continue;
            }
            built.push((seg_out, *dur, trans.clone()));
        }
        if built.is_empty() { logs.push("ffmpeg: no segments built".into()); return (false, logs); }

        let concat_out = format!("{}/concat.mp4", out_dir_s);
        let mut have_concat = false;

        // Preferred path: chain xfade crossfades between consecutive scenes.
        if transitions_enabled && built.len() >= 2 {
            let mut args: Vec<String> = vec!["-y".into()];
            for (p, _, _) in &built { args.push("-i".into()); args.push(p.clone()); }
            let mut fc = String::new();
            let mut prev = "[0:v]".to_string();
            let mut offset = 0.0f64;
            let m = built.len();
            for k in 1..m {
                offset += built[k - 1].1; // start time of scene k
                let out_label = if k == m - 1 { "[vout]".to_string() } else { format!("[x{k}]") };
                let trans = sanitize_transition(&built[k].2);
                fc.push_str(&format!("{prev}[{k}:v]xfade=transition={trans}:duration={d:.3}:offset={o:.3}{out_label};",
                    prev = prev, k = k, trans = trans, d = trans_dur, o = offset, out_label = out_label));
                prev = out_label;
            }
            if fc.ends_with(';') { fc.pop(); }
            args.extend([
                "-filter_complex".into(), fc, "-map".into(), "[vout]".into(),
                "-c:v".into(), "libx264".into(), "-preset".into(), "veryfast".into(), "-crf".into(), "20".into(),
                "-pix_fmt".into(), "yuv420p".into(), "-r".into(), "25".into(), concat_out.clone(),
            ]);
            if run(&args, &mut logs) { have_concat = true; logs.push(format!("ffmpeg: applied {} crossfade transition(s)", m - 1)); }
            else { logs.push("ffmpeg: transition (xfade) pass failed — falling back to hard cuts".into()); }
        }

        // Fallback / no-transitions path: concat demuxer (re-encode for robustness).
        if !have_concat {
            let list_lines: Vec<String> = built.iter().map(|(p, _, _)| format!("file '{}'", p.replace('\'', "'\\''"))).collect();
            let list_path = format!("{}/segments.txt", out_dir_s);
            if std::fs::write(&list_path, list_lines.join("\n")).is_err() { logs.push("ffmpeg: could not write concat list".into()); return (false, logs); }
            let concat_args: Vec<String> = vec![
                "-y".into(), "-f".into(), "concat".into(), "-safe".into(), "0".into(), "-i".into(), list_path,
                "-c:v".into(), "libx264".into(), "-preset".into(), "veryfast".into(), "-crf".into(), "20".into(),
                "-pix_fmt".into(), "yuv420p".into(), "-r".into(), "25".into(), concat_out.clone(),
            ];
            if !run(&concat_args, &mut logs) { return (false, logs); }
        }

        // Mux audio onto the concatenated video.
        let base_out = if overlay_path.is_some() { format!("{}/base.mp4", out_dir_s) } else { output_path.clone() };
        let mux_args: Vec<String> = vec![
            "-y".into(), "-i".into(), concat_out, "-i".into(), audio_path_s,
            "-c:v".into(), "copy".into(), "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into(),
            "-shortest".into(), base_out.clone(),
        ];
        if !run(&mux_args, &mut logs) { return (false, logs); }

        // Optional overlay pass: composite a looping animation over the whole video.
        if let Some(ov) = overlay_path {
            let filter = if overlay_mode == "overlay" {
                format!("[1:v]scale={w}:{h},setsar=1,format=rgba,colorchannelmixer=aa={op:.3}[ov];[0:v][ov]overlay=0:0:shortest=1,format=yuv420p[v]", w = width, h = height, op = overlay_opacity)
            } else {
                // "screen": additive glow, ideal for light leaks / particles.
                format!("[1:v]scale={w}:{h},setsar=1,format=yuv420p[ovr];[0:v][ovr]blend=all_mode=screen:all_opacity={op:.3},format=yuv420p[v]", w = width, h = height, op = overlay_opacity)
            };
            let ov_args: Vec<String> = vec![
                "-y".into(), "-i".into(), base_out, "-stream_loop".into(), "-1".into(), "-i".into(), ov,
                "-filter_complex".into(), filter,
                "-map".into(), "[v]".into(), "-map".into(), "0:a".into(),
                "-c:v".into(), "libx264".into(), "-preset".into(), "veryfast".into(), "-crf".into(), "20".into(),
                "-c:a".into(), "copy".into(), "-shortest".into(), output_path.clone(),
            ];
            if !run(&ov_args, &mut logs) {
                logs.push("ffmpeg: overlay pass failed — the base video without overlay is available".into());
                // base.mp4 exists but final output_path may not; treat as failure so we don't publish a missing file.
                return (false, logs);
            }
        }
        (true, logs)
    }).await.unwrap_or((false, vec!["ffmpeg: blocking task panicked".into()]));

    for line in &logs { db_log(db, job_id, line).await; }

    if ok && out_file.exists() {
        db_log(db, job_id, "ffmpeg: video composed successfully").await;
        let local_path_str = out_file.to_str().unwrap_or("").to_string();
        let video_url = format!("/api/media/video/{song_id}.mp4");
        let _ = db.collection::<Document>("songs")
            .update_one(doc! { "id": song_id }, doc! { "$set": {
                "video_url": &video_url,
                "video_local_path": &local_path_str,
                "status": "video_ready"
            }}).await;
        return Some(serde_json::json!({
            "video_url": video_url,
            "local_path": local_path_str,
            "_real": true,
        }));
    }

    db_log(db, job_id, "ffmpeg: compose failed or output file missing").await;
    None
}

// ────────────────────────────────────────────────────────────────
// Real YouTube upload (token refresh + actual resumable session upload)
// ────────────────────────────────────────────────────────────────

async fn real_youtube_upload(
    upload: &Value,
    db: &crate::store::Db,
    job_id: &str,
    cancelled: &CancelSet,
) -> Option<Value> {
    let channel_id = upload["channel_id"].as_str()?;
    let song_id = upload["song_id"].as_str()?;
    
    // Fetch channel
    let channel = db.collection::<Document>("channels")
        .find_one(doc! { "id": channel_id }).await.ok()??.into_iter()
        .fold(serde_json::Map::new(), |mut m, (k, v)| {
            m.insert(k, bson::Bson::try_into(v).unwrap_or(Value::Null));
            m
        });
    let channel: Value = Value::Object(channel);
    let refresh_token = channel["refresh_token"].as_str()?;

    // Resolve OAuth client
    let oauth = pick_oauth_client(db, &channel, channel["oauth_client_id"].as_str()).await?;
    let client_id = oauth["client_id"].as_str()?;
    let client_secret = oauth["client_secret"].as_str()?;

    let http = reqwest::Client::new();
    
    // Step 1: Refresh access token
    db_log(db, job_id, "youtube: refreshing access token...").await;
    let tr = http.post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send().await.ok()?;
    if !tr.status().is_success() {
        db_log(db, job_id, &format!("youtube: token refresh failed ({}). Token may be invalid or revoked.", tr.status())).await;
        return None;
    }
    let tokens: Value = tr.json().await.ok()?;
    let access_token = tokens["access_token"].as_str()?;
    db_log(db, job_id, "youtube: access token refreshed successfully").await;

    // Step 2: Get song to find video file
    let song_doc = db.collection::<Document>("songs")
        .find_one(doc! { "id": song_id }).await.ok()??.into_iter()
        .fold(serde_json::Map::new(), |mut m, (k, v)| {
            m.insert(k, bson::Bson::try_into(v).unwrap_or(Value::Null));
            m
        });
    let song: Value = Value::Object(song_doc);
    let video_url = song["video_url"].as_str().unwrap_or("");
    let video_local_path = song["video_local_path"].as_str().unwrap_or("");
    
    let video_bytes = if !video_local_path.is_empty() {
        db_log(db, job_id, &format!("youtube: using local composed video file {}", video_local_path)).await;
        match std::fs::read(video_local_path) {
            Ok(bytes) => bytes,
            Err(e) => {
                db_log(db, job_id, &format!("youtube: failed to read local video file {}: {}", video_local_path, e)).await;
                return None;
            }
        }
    } else if !video_url.is_empty() && video_url.starts_with("http") {
        db_log(db, job_id, &format!("youtube: downloading video from {}", video_url)).await;
        match download_video(video_url, job_id, db).await {
            Some(bytes) => bytes,
            None => {
                db_log(db, job_id, "youtube: failed to download video file").await;
                return None;
            }
        }
    } else {
        db_log(db, job_id, "youtube: no valid video source available for upload").await;
        return None;
    };
    
    db_log(db, job_id, &format!("youtube: video file size: {:.1} MB", video_bytes.len() as f64 / (1024.0 * 1024.0))).await;

    // Step 4: Prepare upload metadata
    let title = upload["title"].as_str().unwrap_or("Untitled").to_string();
    let description = upload["description"].as_str().unwrap_or("").to_string();
    let tags: Vec<String> = upload["tags"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let category = upload["category"].as_str().unwrap_or("Music").to_string();
    let privacy = upload["privacy"].as_str().unwrap_or("private").to_string();

    // Convert privacy string to YouTube API value
    let privacy_status = match privacy.as_str() {
        "public" => "public",
        "unlisted" => "unlisted",
        _ => "private",
    };

    let metadata = serde_json::json!({
        "snippet": {
            "title": title,
            "description": description,
            "tags": tags,
            "categoryId": match category.as_str() {
                "Music" => "10",
                "Entertainment" => "24",
                "People" => "15",
                _ => "24",
            }
        },
        "status": {
            "privacyStatus": privacy_status,
            "madeForKids": false
        }
    });

    // Step 5: Initiate resumable upload session
    db_log(db, job_id, "youtube: initiating resumable upload session...").await;
    let session_res = http.post("https://www.googleapis.com/youtube/v3/videos")
        .query(&[("part", "snippet,status"), ("uploadType", "resumable")])
        .bearer_auth(access_token)
        .header("X-Goog-Upload-Protocol", "resumable")
        .header("X-Goog-Upload-Command", "start")
        .header("X-Goog-Upload-Content-Type", "video/mp4")
        .header("X-Goog-Upload-Content-Length", &video_bytes.len().to_string())
        .json(&metadata)
        .send().await.ok()?;

    if !session_res.status().is_success() {
        db_log(db, job_id, &format!("youtube: upload session init failed ({})", session_res.status())).await;
        return None;
    }

    let session_uri = session_res.headers().get("location")?.to_str().ok()?;
    db_log(db, job_id, "youtube: upload session created, starting file transfer...").await;

    // Step 6: Upload video bytes in chunks (resumable protocol)
    let chunk_size = 262144; // 256KB chunks
    let total_chunks = (video_bytes.len() + chunk_size - 1) / chunk_size;
    
    for (chunk_idx, chunk) in video_bytes.chunks(chunk_size).enumerate() {
        if is_cancelled(cancelled, job_id).await {
            db_log(db, job_id, "youtube: upload cancelled by user").await;
            return None;
        }

        let start = chunk_idx * chunk_size;
        let end = start + chunk.len();
        let is_final = end >= video_bytes.len();
        
        let range_header = format!("bytes {}-{}/{}", start, end - 1, video_bytes.len());
        
        let mut chunk_req = http.put(session_uri)
            .header("Content-Type", "video/mp4")
            .header("Content-Range", &range_header);
        
        chunk_req = if is_final {
            chunk_req
                .header("X-Goog-Upload-Command", "upload, finalize")
        } else {
            chunk_req
                .header("X-Goog-Upload-Command", "upload")
        };

        let chunk_res = chunk_req.body(chunk.to_vec()).send().await.ok()?;
        
        if is_final {
            if chunk_res.status().is_success() {
                if let Ok(result) = chunk_res.json::<Value>().await {
                    if let Some(video_id) = result["id"].as_str() {
                        db_log(db, job_id, &format!("youtube: upload complete! Video ID: {}", video_id)).await;
                        return Some(serde_json::json!({
                            "youtube_video_id": video_id,
                            "auth_ok": true,
                            "_real": true,
                            "privacy": privacy_status,
                        }));
                    }
                }
            } else {
                db_log(db, job_id, &format!("youtube: final upload chunk failed ({})", chunk_res.status())).await;
                return None;
            }
        } else {
            if !chunk_res.status().is_success() && chunk_res.status().as_u16() != 308 {
                db_log(db, job_id, &format!("youtube: chunk {} upload failed ({})", chunk_idx + 1, chunk_res.status())).await;
                return None;
            }
            set_progress(db, job_id, 15 + (chunk_idx as i32 * 70 / total_chunks as i32).min(70)).await;
            db_log(db, job_id, &format!("youtube: uploaded chunk {}/{}", chunk_idx + 1, total_chunks)).await;
        }
    }
    
    None
}

async fn download_video(url: &str, job_id: &str, db: &crate::store::Db) -> Option<Vec<u8>> {
    let client = reqwest::Client::new();
    
    let res = client.get(url).send().await.ok()?;
    if !res.status().is_success() {
        db_log(db, job_id, &format!("download_video: HTTP {} from {}", res.status(), url)).await;
        return None;
    }
    
    // If it's a local file:// URL, read from disk instead
    if url.starts_with("file://") || url.starts_with("/") {
        let path = if url.starts_with("file://") {
            url.trim_start_matches("file://")
        } else {
            url
        };
        match std::fs::read(path) {
            Ok(bytes) => return Some(bytes),
            Err(_) => {
                db_log(db, job_id, &format!("download_video: failed to read local file {}", path)).await;
                return None;
            }
        }
    }
    
    match res.bytes().await {
        Ok(bytes) => Some(bytes.to_vec()),
        Err(e) => {
            db_log(db, job_id, &format!("download_video: failed to read response body: {}", e)).await;
            None
        }
    }
}

// ────────────────────────────────────────────────────────────────
// OAuth client resolution (shared helper)
// ────────────────────────────────────────────────────────────────

pub async fn pick_oauth_client(
    db: &crate::store::Db,
    channel: &Value,
    forced_id: Option<&str>,
) -> Option<Value> {
    use futures_util::StreamExt;

    let coll = db.collection::<Document>("oauth_clients");

    if let Some(fid) = forced_id {
        if let Ok(Some(doc)) = coll.find_one(doc! { "id": fid }).await {
            return Some(bson_doc_to_value(doc));
        }
    }
    if let Some(oid) = channel["oauth_client_id"].as_str() {
        if let Ok(Some(doc)) = coll.find_one(doc! { "id": oid }).await {
            return Some(bson_doc_to_value(doc));
        }
    }
    if let Some(lang) = channel["language"].as_str() {
        if let Ok(Some(doc)) = coll.find_one(doc! { "languages": lang }).await {
            return Some(bson_doc_to_value(doc));
        }
    }

    let mut clients = Vec::new();
    if let Ok(mut cursor) = coll.find(doc! {}).sort(doc! { "created_at": -1 }).await {
        while let Some(Ok(doc)) = cursor.next().await {
            clients.push(bson_doc_to_value(doc));
        }
    }
    if let Some(client) = clients.iter().find(|client| {
        client["languages"]
            .as_array()
            .map_or(true, |languages| languages.is_empty())
    }) {
        return Some(client.clone());
    }
    if clients.len() == 1 {
        return clients.into_iter().next();
    }

    // Legacy settings fallback
    let s = db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.ok()??.into_iter()
        .fold(serde_json::Map::new(), |mut m, (k, v)| {
            if let Ok(jv) = bson::from_bson::<Value>(v) { m.insert(k, jv); }
            m
        });
    let s = Value::Object(s);
    // Accept legacy fallback when `google_client_id` is present. The client
    // secret may be empty (UI can supply it later); building the consent URL
    // only requires the client_id.
    let cid = s["google_client_id"].as_str().filter(|s| !s.is_empty())?;
    let csec = s["google_client_secret"].as_str().filter(|s| !s.is_empty()).unwrap_or("");
    Some(serde_json::json!({
        "id": "_legacy",
        "label": "Settings default",
        "client_id": cid,
        "client_secret": csec,
        "redirect_uri": s["google_redirect_uri"].as_str().unwrap_or(""),
        "languages": [],
    }))
}

fn bson_doc_to_value(doc: Document) -> Value {
    let mut m = serde_json::Map::new();
    for (k, v) in doc {
        if k == "_id" { continue; }
        if let Ok(jv) = bson::from_bson::<Value>(v) { m.insert(k, jv); }
    }
    Value::Object(m)
}

// ────────────────────────────────────────────────────────────────
// Job enqueue
// ────────────────────────────────────────────────────────────────

pub async fn enqueue(
    kind: &str,
    target_id: &str,
    state: &Arc<AppState>,
) -> anyhow::Result<Job> {
    let job = Job {
        id: Uuid::new_v4().to_string(),
        kind: kind.to_string(),
        target_id: target_id.to_string(),
        status: "queued".into(),
        progress: 0,
        logs: vec![],
        attempts: 0,
        created_at: now_iso(),
        updated_at: now_iso(),
        error: None,
        result: Value::Object(serde_json::Map::new()),
        stage: Some("queued".into()),
        stage_message: Some("Waiting for a worker…".into()),
        stage_started_at: Some(unix_secs()),
    };
    let bson_doc = bson::to_document(&job)?;
    state.db.collection::<Document>("jobs").insert_one(bson_doc).await?;

    let job_id = job.id.clone();
    let state_clone = Arc::clone(state);
    let semaphore = Arc::clone(&state.job_semaphore);
    tokio::spawn(async move {
        // Block here (job stays visibly "queued") until a concurrency slot frees up, instead of
        // running an unbounded number of jobs at once — each Midjourney job launches a full
        // browser and Suno/YouTube calls share account-level rate limits, so unbounded
        // concurrency was a real resource risk, not just a theoretical one.
        let _permit = semaphore.acquire_owned().await;
        run_job(&job_id, &state_clone).await;
    });
    Ok(job)
}

// ────────────────────────────────────────────────────────────────
// Job runner
// ────────────────────────────────────────────────────────────────

async fn get_job_settings(db: &crate::store::Db, kind: &str, tgt: &str) -> Value {
    let mut project_id: Option<String> = None;
    // For image-on-section jobs we also capture the song's language so we can apply that
    // channel's sticky style.
    let mut song_language: Option<String> = None;
    match kind {
        "music" | "analysis" | "video" | "overlay" => {
            if let Ok(Some(song)) = db.collection::<Document>("songs").find_one(doc! { "id": tgt }).await {
                if let Some(pid) = song.get_str("project_id").ok() {
                    project_id = Some(pid.to_string());
                }
            }
        }
        "image" => {
            if let Ok(Some(sec)) = db.collection::<Document>("sections").find_one(doc! { "id": tgt }).await {
                if let Some(song_id) = sec.get_str("song_id").ok() {
                    if let Ok(Some(song)) = db.collection::<Document>("songs").find_one(doc! { "id": song_id }).await {
                        if let Some(pid) = song.get_str("project_id").ok() {
                            project_id = Some(pid.to_string());
                        }
                        if let Some(lang) = song.get_str("language").ok() {
                            song_language = Some(lang.to_string());
                        }
                    }
                }
            } else if let Ok(Some(char_doc)) = db.collection::<Document>("characters").find_one(doc! { "id": tgt }).await {
                if let Some(pid) = char_doc.get_str("project_id").ok() {
                    project_id = Some(pid.to_string());
                }
            }
        }
        "upload" => {
            if let Ok(Some(up)) = db.collection::<Document>("uploads").find_one(doc! { "id": tgt }).await {
                if let Some(song_id) = up.get_str("song_id").ok() {
                    if let Ok(Some(song)) = db.collection::<Document>("songs").find_one(doc! { "id": song_id }).await {
                        if let Some(pid) = song.get_str("project_id").ok() {
                            project_id = Some(pid.to_string());
                        }
                    }
                }
            }
        }
        _ => {}
    }
    // Base: the global singleton. A per-project doc (if any) is merged ON TOP, never used
    // wholesale — a stale project doc would otherwise hide globally-saved keys (e.g. the
    // user's `music_engine` choice, silently falling back to Suno). Mirrors `get_settings`.
    let mut settings = db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.unwrap_or(None)
        .map(bson_doc_to_value).unwrap_or_default();
    if let Some(pid) = &project_id {
        if let Ok(Some(sdoc)) = db.collection::<Document>("settings").find_one(doc! { "_id": pid }).await {
            if let (Some(base), Value::Object(over)) = (settings.as_object_mut(), bson_doc_to_value(sdoc)) {
                for (k, v) in over { base.insert(k, v); }
            }
        }
    }

    // Overlay the per-channel sticky style (matched by the song's language) for section images.
    if let Some(lang) = song_language {
        if let Some(overrides) = crate::commands::styles::channel_style_overrides(db, &lang, project_id.as_deref()).await {
            if let (Some(base), Some(ov)) = (settings.as_object_mut(), overrides.as_object()) {
                for (k, v) in ov {
                    base.insert(k.clone(), v.clone());
                }
            }
        }
    }
    settings
}

pub async fn run_job(job_id: &str, state: &Arc<AppState>) {
    let db = &state.db;
    let ts = now_iso();
    let _ = db.collection::<Document>("jobs")
        .update_one(
            doc! { "id": job_id },
            doc! { "$set": { "status": "running", "updated_at": &ts },
                   "$push": { "logs": format!("[{ts}] starting") } },
        )
        .await;

    // Fetch job document
    let job_doc = match db.collection::<Document>("jobs")
        .find_one(doc! { "id": job_id })
        .await
    {
        Ok(Some(d)) => d,
        _ => return,
    };
    let job: Value = bson_doc_to_value(job_doc);

    // A cancellation requested in the brief window before this task got to run (or before it
    // got a chance to start any real work). The "running" update above may have just clobbered
    // the "cancelled" status cancel_job set, so restore it explicitly rather than only logging.
    if is_cancelled(&state.cancelled_jobs, job_id).await {
        state.cancelled_jobs.lock().await.remove(job_id);
        let ts = now_iso();
        let _ = db.collection::<Document>("jobs")
            .update_one(
                doc! { "id": job_id },
                doc! { "$set": { "status": "cancelled", "updated_at": &ts },
                       "$push": { "logs": format!("[{ts}] cancelled by user before it started") } },
            )
            .await;
        return;
    }

    // Progress ticks
    for p in [15, 35, 60, 85] {
        tokio::time::sleep(tokio::time::Duration::from_millis(600)).await;
        set_progress(db, job_id, p).await;
    }

    let kind = job["kind"].as_str().unwrap_or("");
    let tgt  = job["target_id"].as_str().unwrap_or("");

    // Fetch settings
    let settings_doc = get_job_settings(db, kind, tgt).await;

    let run_result: anyhow::Result<Value> = async {
        Ok(match kind {
            // ── MUSIC ──────────────────────────────────────────────────
            "music" => {
                let song = fetch_doc(db, "songs", "id", tgt).await;
                // HeartMuLa by default: open weights on a free GPU, nothing to breach and no account to lose.
                // Suno stays in the code and stays hidden — its terms may change, and an official API is
                // reportedly in progress.
                let engine = settings_doc.get("music_engine").and_then(|v| v.as_str()).unwrap_or("heartmula");

                // Try the chosen engine, then the configured fallback. The whole pipeline rides on
                // an unofficial Suno session cookie; when it expires overnight every queued song
                // fails, even though a free self-hosted engine may be sitting there ready. With
                // `music_engine_fallback` set, one engine's outage costs a retry instead of the
                // night's output. `none` (the default) keeps the old fail-loudly behaviour.
                let configured_fallback = settings_doc
                    .get("music_engine_fallback")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty() && *s != "none" && *s != engine);

                // A fallback may never take somebody from a free engine to one that charges. The
                // whole point of a fallback is that a tunnel dying overnight does not cost the
                // night's output — being quietly billed for it instead is a different and worse
                // failure, and one nobody would find until the invoice.
                let fallback = match configured_fallback {
                    Some(f) if music_engine_is_paid(f) && !music_engine_is_paid(engine) => {
                        db_log(db, job_id, &format!(
                            "Not falling back to {f}: it charges per track and {engine} does not.                              Pick {f} as the engine itself if that is what you want.")).await;
                        None
                    }
                    other => other,
                };

                let mut attempts: Vec<&str> = vec![engine];
                attempts.extend(fallback);

                let mut clips: Option<Vec<Value>> = None;
                let mut used_engine = engine.to_string();
                let mut failures: Vec<String> = Vec::new();
                for candidate in attempts {
                    if !failures.is_empty() {
                        db_log(db, job_id, &format!("Falling back to {candidate} after {engine} failed")).await;
                    }
                    let produced = match candidate {
                        "acestep" => real_acestep(&song, &settings_doc, job_id, db, &state.cancelled_jobs).await,
                        "riffusion" => real_riffusion(&song, &settings_doc, job_id, db, &state.cancelled_jobs).await,
                        "elevenlabs" => real_elevenlabs(&song, &settings_doc, job_id, db, &state.cancelled_jobs).await,
                        // Suno is named rather than being the fallback arm: as the catch-all it
                        // meant any unrecognised id drove the user's own Suno session, which is the
                        // one thing hiding the engine was meant to prevent.
                        "suno" => real_suno(&song, &settings_doc, job_id, db, &state.cancelled_jobs).await,
                        _ => real_heartmula(&song, &settings_doc, job_id, db, &state.cancelled_jobs).await,
                    };
                    match produced {
                        Some(c) => {
                            used_engine = candidate.to_string();
                            clips = Some(c);
                            break;
                        }
                        None => failures.push(match candidate {
                            "acestep" => "ACE-Step failed — check acestep_api_url is reachable, the Kaggle/Colab notebook is still running (share URLs expire), and the API key if one is required.".to_string(),
                            "riffusion" => "Riffusion failed — check riffusion_api_url is reachable and the notebook is still running.".to_string(),
                            "elevenlabs" => "ElevenLabs failed — check the API key and whether the account still has credit.".to_string(),
                            "suno" => "Suno failed — check the session cookie's validity, network connectivity and Suno's service status.".to_string(),
                            _ => "HeartMuLa failed — check heartmula_api_url is reachable, the notebook is still running, and the API key if required.".to_string(),
                        }),
                    }
                }

                let clips = clips.ok_or_else(|| {
                    anyhow::anyhow!("Music generation failed. {} See job logs for details.", failures.join(" "))
                })?;
                let engine = used_engine.as_str();

                let primary = &clips[0];
                let primary_url = primary["audio_url"].as_str().unwrap_or("").to_string();
                let primary_dur = primary["duration"].as_f64().unwrap_or(120.0);

                // A bare origin like `https://host/` carries no filename and is exactly what the
                // old empty-`file` harvest produced; it saves cleanly but 404s on playback. Reject
                // anything without a real path or query so that failure mode can't come back
                // silently through a different engine.
                let url_unusable = primary_url.is_empty()
                    || reqwest::Url::parse(&primary_url)
                        .map(|u| u.path() == "/" && u.query().is_none())
                        .unwrap_or(true);
                if url_unusable {
                    return Err(anyhow::anyhow!(
                        "{} returned an unusable audio URL ({:?}) — the render produced no file. \
                         Check the notebook log for a generation error.",
                        engine, primary_url
                    ));
                }

                let (alt_url, alt_dur) = if clips.len() > 1 {
                    let alt = &clips[1];
                    (alt["audio_url"].as_str().unwrap_or("").to_string(),
                     alt["duration"].as_f64().unwrap_or(120.0))
                } else {
                    ("".to_string(), 0.0)
                };

                db.collection::<Document>("songs").update_one(
                    doc! { "id": tgt },
                    doc! { "$set": { 
                        "audio_url": &primary_url, 
                        "duration": primary_dur,
                        "audio_url_primary": &primary_url,
                        "duration_primary": primary_dur,
                        "audio_url_alt": &alt_url,
                        "duration_alt": alt_dur,
                        "status": "music_ready"
                    } },
                ).await?;
                set_stage(db, job_id, "done", 100,
                          if clips.len() > 1 { "Done — 2 takes ready to preview." }
                          else { "Done — your track is ready to preview." }, true).await;
                serde_json::json!({ "audio_url": primary_url, "real": true })
            }

            // ── ANALYSIS ───────────────────────────────────────────────
            "analysis" => {
                let song = fetch_doc(db, "songs", "id", tgt).await;
                let annotations = song["annotations"].as_str().unwrap_or("");
                let lyrics      = song["lyrics"].as_str().unwrap_or("");
                let pairs = parse_annotations(annotations, lyrics);
                let n = pairs.len().max(1);
                let duration = song["duration"].as_f64().unwrap_or(110.0);
                let seg = duration / n as f64;

                db.collection::<Document>("sections").delete_many(doc! { "song_id": tgt }).await?;
                let moods: Vec<&'static str> = pairs.iter()
                    .map(|p| derive_mood(if p.image_prompt.is_empty() { &p.line } else { &p.image_prompt }))
                    .collect();

                let mut secs: Vec<bson::Document> = Vec::new();
                for (i, p) in pairs.iter().enumerate() {
                    let sec = Section {
                        id: Uuid::new_v4().to_string(),
                        song_id: tgt.to_string(),
                        index: i as i32,
                        start: (i as f64 * seg * 100.0).round() / 100.0,
                        end: ((i + 1) as f64 * seg * 100.0).round() / 100.0,
                        line: p.line.clone(),
                        image_prompt: p.image_prompt.clone(),
                        mood: moods[i].to_string(),
                        mood_prev: if i > 0 { moods[i-1].to_string() } else { String::new() },
                        mood_next: if i < n-1 { moods[i+1].to_string() } else { String::new() },
                        image_url: None,
                        image_variants: vec![],
                        is_video: false,
                        effects: suggest_effects(moods[i]),
                    };
                    secs.push(bson::to_document(&sec)?);
                }
                let count = secs.len();
                if !secs.is_empty() {
                    db.collection::<Document>("sections").insert_many(secs).await?;
                }
                db.collection::<Document>("songs").update_one(
                    doc! { "id": tgt }, doc! { "$set": { "status": "analyzed" } },
                ).await?;
                serde_json::json!({ "sections": count })
            }

            // ── CHARACTER IMAGE ────────────────────────────────────────
            "character_image" => {
                let character = fetch_doc(db, "characters", "id", tgt).await;
                let base_prompt = character["image_prompt"].as_str()
                    .filter(|s| !s.is_empty())
                    .or(character["description"].as_str())
                    .unwrap_or("")
                    .to_string();
                // Prepend the character's stable appearance tags so every generation and every
                // "Vary" shares the same visual anchors, instead of drifting each time only the
                // free-text image_prompt/description gets re-sent to Midjourney.
                let appearance_tags: Vec<String> = character["appearance_tags"].as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                let mut prompt = if appearance_tags.is_empty() {
                    base_prompt
                } else {
                    format!("{}, {}", appearance_tags.join(", "), base_prompt)
                };
                // Optional per-channel adaptation: blend the target channel's style + research into
                // the prompt so this take leans into that channel's visual culture while the
                // appearance tags + IP-Adapter reference keep the SAME character recognizable.
                let variant_channel = character["variant_channel_id"].as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
                if let Some(cid) = &variant_channel {
                    if let Ok(Some(chan)) = db.collection::<Document>("channels").find_one(doc! { "id": cid }).await {
                        let cstyles = chan.get_str("styles").unwrap_or("");
                        let cresearch = chan.get_str("research").unwrap_or("");
                        let clang = chan.get_str("language").unwrap_or("");
                        let mut extra = Vec::new();
                        if !cstyles.trim().is_empty() { extra.push(format!("visual style: {}", cstyles.trim())); }
                        if !clang.trim().is_empty() { extra.push(format!("styled for a {} audience", clang.trim())); }
                        if !cresearch.trim().is_empty() { extra.push(cresearch.trim().chars().take(200).collect::<String>()); }
                        if !extra.is_empty() {
                            prompt = format!("{}, {} — keep the SAME character identity and face, only adapt styling/atmosphere", prompt, extra.join(", "));
                            db_log(db, job_id, &format!("character: adapting to channel {} style", cid)).await;
                        }
                    }
                }
                // For ComfyUI character consistency, feed the character's existing avatar/example
                // image as the IP-Adapter reference so re-generations stay on-model.
                let char_ref = character["reference_image"].as_str()
                    .filter(|s| !s.is_empty())
                    .or_else(|| character["image_url"].as_str().filter(|s| !s.is_empty()))
                    .or_else(|| character["image_variants"].as_array().and_then(|a| a.first()).and_then(|v| v.as_str()))
                    .map(|s| s.to_string());
                let v = gen_images(&prompt, None, "1:1", char_ref.as_deref(), &settings_doc, job_id, db, &state.cancelled_jobs).await
                    .ok_or_else(|| anyhow::anyhow!(
                        "Character image generation failed. For Midjourney: check proxy/profile and token. For FLUX/ComfyUI: check the server URL is set and the Kaggle/Colab notebook is running. See job logs."
                    ))?;

                db_log(db, job_id, &format!("image: received {} character image variants", v.len())).await;
                // Update character with new variants (append to existing)
                let existing: Vec<String> = character["image_variants"].as_array()
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                let mut all_variants = existing.clone();
                for url in &v {
                    if !all_variants.contains(url) {
                        all_variants.push(url.clone());
                    }
                }
                let bson_variants: Vec<bson::Bson> = all_variants.iter().map(|s| bson::Bson::String(s.clone())).collect();
                let mut set = doc! {
                    "image_url": &v[0],
                    "image_variants": bson_variants,
                };
                // When this was a per-channel adaptation, also file the takes under
                // channel_variants.<cid> so the UI can show "this character, per channel", and clear
                // the transient request flag so a later plain generation isn't still channel-biased.
                let mut unset: Option<Document> = None;
                if let Some(cid) = &variant_channel {
                    let existing_cv: Vec<bson::Bson> = character["channel_variants"][cid.as_str()].as_array()
                        .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| bson::Bson::String(s.to_string()))).collect())
                        .unwrap_or_default();
                    let mut cv = existing_cv;
                    for url in &v { cv.push(bson::Bson::String(url.clone())); }
                    set.insert(format!("channel_variants.{}", cid), cv);
                    unset = Some(doc! { "variant_channel_id": "" });
                }
                let mut update = doc! { "$set": set };
                if let Some(u) = unset { update.insert("$unset", u); }
                db.collection::<Document>("characters").update_one(doc! { "id": tgt }, update).await?;
                set_stage(db, job_id, "done", 100, &format!("Done — {} character take(s) ready.", v.len()), true).await;
                serde_json::json!({ "variants": v.len(), "total_variants": all_variants.len(), "real": true })
            }

            // ── IMAGE ──────────────────────────────────────────────────
            "image" => {
                let sec = fetch_doc(db, "sections", "id", tgt).await;
                let subject = sec["image_prompt"].as_str()
                    .filter(|s| !s.is_empty())
                    .or(sec["line"].as_str())
                    .unwrap_or("")
                    .to_string();
                // The imagery choice the user made on the Images page (see imagery.rs) decides the
                // light, the mood, the restraint and the shape. Without one, the section's own
                // prompt goes through untouched — this must never be a wall in front of somebody
                // who has not opened that panel.
                let imagery = apply_imagery(&subject, &settings_doc, job_id, db).await;
                // Optional per-section character reference (if the section is bound to a character).
                let sec_ref = sec["reference_image"].as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
                let v = gen_images(&imagery.prompt, imagery.negative.as_deref(), &imagery.aspect,
                                   sec_ref.as_deref(), &settings_doc, job_id, db, &state.cancelled_jobs).await
                    .ok_or_else(|| anyhow::anyhow!(
                        "Image generation failed. For Midjourney: check proxy/profile and token. For FLUX/ComfyUI: check the server URL is set and the Kaggle/Colab notebook is running. See logs for details."
                    ))?;

                db_log(db, job_id, &format!("image: received {} image variants", v.len())).await;
                db.collection::<Document>("sections").update_one(
                    doc! { "id": tgt },
                    doc! { "$set": {
                        "image_url": &v[0],
                        "image_variants": v.iter().map(|s| bson::Bson::String(s.clone())).collect::<Vec<_>>()
                    }},
                ).await?;
                set_stage(db, job_id, "done", 100, &format!("Done — {} image(s) ready.", v.len()), true).await;
                serde_json::json!({ "variants": v.len(), "real": true })
            }

            // ── VIDEO ──────────────────────────────────────────────────
            "video" => {
                let song = fetch_doc(db, "songs", "id", tgt).await;
                // Proper cursor iteration
                let mut cursor = db.collection::<Document>("sections")
                    .find(doc! { "song_id": tgt }).await?;
                let mut secs_docs: Vec<Value> = Vec::new();
                use futures_util::StreamExt;
                while let Some(Ok(d)) = cursor.next().await {
                    secs_docs.push(bson_doc_to_value(d));
                }
                let real = real_ffmpeg(&song, &secs_docs, &settings_doc, job_id, db).await;
                let video_url = real.as_ref()
                    .and_then(|r| r["video_url"].as_str())
                    .unwrap_or(&format!("/api/media/video/{tgt}.mp4"))
                    .to_string();
                let is_real = real.is_some();
                db.collection::<Document>("songs").update_one(
                    doc! { "id": tgt },
                    doc! { "$set": { "status": "video_ready", "video_url": &video_url } },
                ).await?;
                serde_json::json!({ "video_url": video_url, "real": is_real })
            }

            // ── OVERLAY (companion audio-reactive animation) ───────────
            "overlay" => {
                let song = fetch_doc(db, "songs", "id", tgt).await;
                let real = real_overlay(&song, &settings_doc, job_id, db).await
                    .ok_or_else(|| anyhow::anyhow!(
                        "Overlay generation failed. Verify the song has audio and ffmpeg is available. See logs for details."
                    ))?;
                real
            }

            // ── UPLOAD ─────────────────────────────────────────────────
            "upload" => {
                let upload = fetch_doc(db, "uploads", "id", tgt).await;
                let yt_id = real_youtube_upload(&upload, db, job_id, &state.cancelled_jobs).await
                    .ok_or_else(|| anyhow::anyhow!(
                        "YouTube upload failed. Verify: (1) Channel has valid refresh token, (2) Video file exists, (3) OAuth credentials are active. See logs for details."
                    ))?
                    .get("youtube_video_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("YouTube API did not return video ID"))?
                    .to_string();
                
                let ts = now_iso();
                db.collection::<Document>("uploads").update_one(
                    doc! { "id": tgt },
                    doc! { "$set": { "status": "published", "youtube_video_id": &yt_id, "published_at": &ts } },
                ).await?;
                
                db_log(db, job_id, &format!("upload: completed! YouTube URL: https://youtu.be/{}", yt_id)).await;

                // The moment the video exists is the moment publicity can reference it, so draft the
                // per-platform pieces now — with the real URL in them — rather than making the user
                // come back for it. Drafts only, and only when the setting is on: nothing posts here.
                if let Some(song_id) = upload.get("song_id").and_then(|v| v.as_str()) {
                    let state_for_publicity = state.clone();
                    let song_id = song_id.to_string();
                    tauri::async_runtime::spawn(async move {
                        crate::commands::publicity::auto_author_after_upload(&state_for_publicity, &song_id).await;
                    });
                }
                serde_json::json!({ "youtube_video_id": yt_id, "url": format!("https://youtu.be/{yt_id}"), "real": true })
            }

            _ => serde_json::json!({}),
        })
    }.await;

    let ts = now_iso();

    // If cancellation was requested at any point during execution, the final state is always
    // "cancelled" — regardless of whether the underlying work happened to succeed or fail after
    // the cancel was noticed — since that's what the user asked for and it's the only outcome
    // the Jobs Monitor UI should show for a job the user explicitly stopped.
    if state.cancelled_jobs.lock().await.remove(job_id) {
        let _ = db.collection::<Document>("jobs")
            .update_one(
                doc! { "id": job_id },
                doc! { "$set": { "status": "cancelled", "updated_at": &ts },
                       "$push": { "logs": format!("[{ts}] cancelled") } },
            ).await;
        return;
    }

    match run_result {
        Ok(result) => {
            let bson_result = bson::to_bson(&result).unwrap_or(bson::Bson::Null);
            let _ = db.collection::<Document>("jobs")
                .update_one(
                    doc! { "id": job_id },
                    doc! { "$set": { "status": "done", "progress": 100, "result": bson_result, "updated_at": &ts },
                           "$push": { "logs": format!("[{ts}] done") } },
                ).await;
        }
        Err(e) => {
            let _ = db.collection::<Document>("jobs")
                .update_one(
                    doc! { "id": job_id },
                    doc! { "$set": { "status": "failed", "error": e.to_string(), "updated_at": &ts },
                           "$push": { "logs": format!("[{ts}] error: {e}") } },
                ).await;
        }
    }
}

async fn fetch_doc(db: &crate::store::Db, coll: &str, key: &str, val: &str) -> Value {
    db.collection::<Document>(coll)
        .find_one(doc! { key: val }).await.ok().flatten()
        .map(bson_doc_to_value)
        .unwrap_or_default()
}
