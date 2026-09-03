//! Pipeline overview, performance feedback and quota visibility.
//!
//! Three mid-term backlog items that answer three questions the app could not answer before:
//!
//! * **"Where is everything?"** — `pipeline_overview` counts every song by stage, per project and
//!   per language/style, so someone running ten channels sees the whole board instead of clicking
//!   through songs one at a time.
//! * **"Is any of this working?"** — `refresh_upload_analytics` pulls view counts and likes back
//!   from the YouTube Data API (already OAuth-connected) and `performance_report` ranks
//!   channel/language/style combinations by median views, so weak combinations can be dropped
//!   instead of guessed at.
//! * **"What is this costing me?"** — `quota_report` gathers the limits the user currently
//!   discovers by hitting them: YouTube's daily upload quota, Kaggle's weekly GPU hours, the
//!   free-tier AI request counts, and local disk.
//!
//! Everything here is read-only aggregation over the JSON store plus one optional YouTube call.

use crate::state::AppState;
use bson::{doc, Document};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use tauri::State;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

/// The pipeline, in order. A song's `status` is one of these; anything unrecognized counts as a
/// draft so the totals always add up.
const STAGES: &[(&str, &str)] = &[
    ("draft", "Draft"),
    ("music_ready", "Music"),
    ("analyzed", "Analyzed"),
    ("images_ready", "Images"),
    ("video_ready", "Video"),
    ("uploaded", "Uploaded"),
];

fn stage_index(status: &str) -> usize {
    STAGES.iter().position(|(id, _)| *id == status).unwrap_or(0)
}

/// Every song bucketed by stage — per project, and broken down by language and style.
#[tauri::command]
pub async fn pipeline_overview(state: State<'_, AppState>, project_id: Option<String>) -> Res<Value> {
    use futures_util::StreamExt;

    let filter = match project_id.as_deref().filter(|s| !s.is_empty()) {
        Some(pid) => doc! { "project_id": pid },
        None => doc! {},
    };
    let mut cursor = state.db.collection::<Document>("songs").find(filter).await.map_err(e)?;

    // project → stage → count, plus the same split by language and by style.
    let mut per_project: HashMap<String, [u64; 6]> = HashMap::new();
    let mut per_language: HashMap<String, [u64; 6]> = HashMap::new();
    let mut per_style: HashMap<String, [u64; 6]> = HashMap::new();
    let mut totals = [0u64; 6];
    let mut stuck: Vec<Value> = Vec::new();
    let now = chrono::Utc::now();

    while let Some(Ok(d)) = cursor.next().await {
        let song = crate::store::doc_to_json(&d);
        let stage = stage_index(song["status"].as_str().unwrap_or("draft"));
        let project = song["project_id"].as_str().unwrap_or("(none)").to_string();
        let language = song["language"].as_str().unwrap_or("(none)").to_string();
        // A song's `styles` is a free-text list; the first entry is the one that identifies it.
        let style = song["styles"]
            .as_str()
            .unwrap_or("")
            .split(',')
            .next()
            .unwrap_or("(none)")
            .trim()
            .to_string();

        totals[stage] += 1;
        per_project.entry(project.clone()).or_insert([0; 6])[stage] += 1;
        per_language.entry(language).or_insert([0; 6])[stage] += 1;
        per_style.entry(if style.is_empty() { "(none)".into() } else { style }).or_insert([0; 6])[stage] += 1;

        // Anything sitting unfinished for over a week is worth surfacing: it usually means a job
        // failed quietly weeks ago and nobody noticed.
        if stage < STAGES.len() - 1 {
            if let Some(created) = song["created_at"].as_str().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()) {
                let days = (now - created.with_timezone(&chrono::Utc)).num_days();
                if days >= 7 {
                    stuck.push(json!({
                        "id": song["id"], "title": song["title"], "project_id": project,
                        "status": song["status"], "days": days,
                    }));
                }
            }
        }
    }

    stuck.sort_by(|a, b| b["days"].as_i64().unwrap_or(0).cmp(&a["days"].as_i64().unwrap_or(0)));
    stuck.truncate(25);

    let to_rows = |map: HashMap<String, [u64; 6]>| -> Vec<Value> {
        let mut rows: Vec<Value> = map
            .into_iter()
            .map(|(key, counts)| {
                json!({
                    "key": key,
                    "counts": counts.to_vec(),
                    "total": counts.iter().sum::<u64>(),
                    "done": counts[STAGES.len() - 1],
                })
            })
            .collect();
        rows.sort_by(|a, b| b["total"].as_u64().unwrap_or(0).cmp(&a["total"].as_u64().unwrap_or(0)));
        rows
    };

    Ok(json!({
        "stages": STAGES.iter().map(|(id, label)| json!({ "id": id, "label": label })).collect::<Vec<_>>(),
        "totals": totals.to_vec(),
        "by_project": to_rows(per_project),
        "by_language": to_rows(per_language),
        "by_style": to_rows(per_style),
        "stuck": stuck,
    }))
}


/// Exchange a channel's stored refresh token for an access token, reusing the OAuth client the
/// upload path already picks for that channel (several clients exist so channels can be spread
/// across projects to dodge per-project quota).
async fn access_token_for_channel(db: &crate::store::Db, channel_id: &str) -> Res<String> {
    let channel = db
        .collection::<Document>("channels")
        .find_one(doc! { "id": channel_id })
        .await
        .map_err(e)?
        .map(|d| crate::store::doc_to_json(&d))
        .ok_or("channel not found")?;
    let refresh_token = channel["refresh_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("channel is not connected to YouTube")?;

    let oauth = crate::jobs::pick_oauth_client(db, &channel, channel["oauth_client_id"].as_str())
        .await
        .ok_or("no OAuth client is configured for this channel")?;

    let resp = reqwest::Client::new()
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", oauth["client_id"].as_str().unwrap_or("")),
            ("client_secret", oauth["client_secret"].as_str().unwrap_or("")),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(e)?;
    if !resp.status().is_success() {
        return Err(format!("token refresh failed ({}) — the channel may need reconnecting", resp.status()));
    }
    let tokens: Value = resp.json().await.map_err(e)?;
    tokens["access_token"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "token refresh returned no access token".to_string())
}

// ────────────────────────────────────────────────────────────────────────────
// Upload analytics
// ────────────────────────────────────────────────────────────────────────────

/// Fetch view/like/comment counts for published uploads from the YouTube Data API and store them
/// on the upload document, so the performance report can rank without re-fetching.
///
/// Uses the per-channel OAuth refresh token the Channel Manager already holds. Videos are fetched
/// in batches of 50 (the API's per-call limit) to stay inside the daily quota.
#[tauri::command]
pub async fn refresh_upload_analytics(state: State<'_, AppState>, channel_id: Option<String>) -> Res<Value> {
    use futures_util::StreamExt;

    let filter = match channel_id.as_deref().filter(|s| !s.is_empty()) {
        Some(cid) => doc! { "channel_id": cid, "status": "published" },
        None => doc! { "status": "published" },
    };
    let mut cursor = state.db.collection::<Document>("uploads").find(filter).await.map_err(e)?;

    // Group video ids by the channel that owns them — each channel has its own OAuth token.
    let mut by_channel: HashMap<String, Vec<String>> = HashMap::new();
    while let Some(Ok(d)) = cursor.next().await {
        let upload = crate::store::doc_to_json(&d);
        let (Some(video_id), Some(cid)) = (
            upload["youtube_video_id"].as_str().filter(|s| !s.is_empty()),
            upload["channel_id"].as_str().filter(|s| !s.is_empty()),
        ) else {
            continue;
        };
        by_channel.entry(cid.to_string()).or_default().push(video_id.to_string());
    }

    let http = reqwest::Client::new();
    let mut updated = 0u64;
    let mut errors: Vec<String> = Vec::new();

    for (cid, video_ids) in by_channel {
        let token = match access_token_for_channel(&state.db, &cid).await {
            Ok(t) => t,
            Err(err) => {
                errors.push(format!("channel {cid}: {err}"));
                continue;
            }
        };
        for chunk in video_ids.chunks(50) {
            let url = format!(
                "https://www.googleapis.com/youtube/v3/videos?part=statistics,snippet&id={}",
                chunk.join(",")
            );
            let resp = match http.get(&url).bearer_auth(&token).send().await {
                Ok(r) => r,
                Err(err) => {
                    errors.push(format!("channel {cid}: {err}"));
                    continue;
                }
            };
            let body: Value = resp.json().await.unwrap_or(Value::Null);
            for item in body["items"].as_array().cloned().unwrap_or_default() {
                let video_id = item["id"].as_str().unwrap_or("").to_string();
                let stats = &item["statistics"];
                let as_num = |key: &str| -> i64 {
                    stats[key].as_str().and_then(|s| s.parse().ok()).unwrap_or(0)
                };
                let r = state
                    .db
                    .collection::<Document>("uploads")
                    .update_one(
                        doc! { "youtube_video_id": &video_id },
                        doc! { "$set": {
                            "views": as_num("viewCount"),
                            "likes": as_num("likeCount"),
                            "comments": as_num("commentCount"),
                            "published_at_yt": item["snippet"]["publishedAt"].as_str().unwrap_or(""),
                            "stats_checked_at": chrono::Utc::now().to_rfc3339(),
                        }},
                    )
                    .await
                    .map_err(e)?;
                updated += r.modified_count;
            }
        }
    }

    Ok(json!({ "ok": true, "updated": updated, "errors": errors }))
}

/// Below this many videos, a row is a coincidence with a number next to it, and is not reported to
/// anything that would act on it. Between this and `THIN_ROW` it is reported and labelled thin.
const MIN_ROW_VIDEOS: usize = 3;
const THIN_ROW: usize = 5;
/// Below this many measured videos in total there is no report at all — ranking four uploads against
/// each other is how a first lucky video becomes a strategy.
const MIN_MEASURED: usize = 6;

// ────────────────────────────────────────────────────────────────────────────
// The axes
// ────────────────────────────────────────────────────────────────────────────
//
// The app is combinatorial by construction — a target is channel × language × style, an image is
// section × style pack, a publish is video × hour — and until now the report crossed exactly three
// of those. So the studio scheduled by hour and could not say whether the hour mattered, and style
// packs, the most visible creative choice in the app, were not a dimension at all.
//
// Everything below is a pure function from one upload to one bucket key, which is what makes the
// axes testable and what keeps the rules for "which bucket" in one place rather than inline in a
// loop. Each returns `None` where the answer is unknown, and an unknown answer is left out of the
// dimension rather than bucketed as "(none)" — a bucket of everything whose duration was never
// recorded is not a finding about duration.

/// Three-hour bands in the channel's own timezone.
///
/// Bands rather than 24 hours, because a creator with thirty measured videos and twenty-four buckets
/// has one video per bucket and a ranking made of coincidences. Local rather than UTC because the
/// question "does the hour matter" is only meaningful in the audience's own day — the same instant
/// is breakfast for one channel and midnight for another, and `publish_time` already resolves and
/// stores each channel's zone.
pub fn hour_band(published_at: &str, timezone: &str) -> Option<String> {
    use chrono::Timelike;
    let when = chrono::DateTime::parse_from_rfc3339(published_at.trim()).ok()?
        .with_timezone(&chrono::Utc);
    let tz: chrono_tz::Tz = timezone.trim().parse().ok()?;
    let hour = when.with_timezone(&tz).hour();
    let start = (hour / 3) * 3;
    Some(format!("{start:02}:00–{:02}:00", start + 3))
}

/// The weekday it went out, in the channel's own timezone.
pub fn weekday(published_at: &str, timezone: &str) -> Option<String> {
    use chrono::Datelike;
    let when = chrono::DateTime::parse_from_rfc3339(published_at.trim()).ok()?
        .with_timezone(&chrono::Utc);
    let tz: chrono_tz::Tz = timezone.trim().parse().ok()?;
    Some(when.with_timezone(&tz).weekday().to_string())
}

/// The first entry of a comma-separated list, which is how this app writes a primary choice.
fn first_of(csv: &str) -> Option<String> {
    let first = csv.split(',').next()?.trim();
    (!first.is_empty()).then(|| first.to_string())
}

/// How long the song runs, in the bands people actually choose between.
///
/// `pick_duration` already places a length in a range from the lyric's line count; nothing ever
/// checked whether the ranges that get watched are the ones being asked for.
pub fn length_band(seconds: f64) -> Option<String> {
    if !(seconds.is_finite()) || seconds < 1.0 { return None; }
    Some(match seconds {
        s if s < 90.0 => "under 1:30",
        s if s < 150.0 => "1:30–2:30",
        s if s < 210.0 => "2:30–3:30",
        s if s < 300.0 => "3:30–5:00",
        _ => "over 5:00",
    }.to_string())
}

/// How many sections a song was cut into, in the bands that are actually a choice.
///
/// A section is one image and one stretch of song, so this is simultaneously "how many pictures does
/// this video have" and "how fast does it move" — which makes it the one axis here that a creator
/// can change on the next song without changing anything else about it.
pub fn section_band(n: usize) -> Option<String> {
    if n == 0 { return None; }
    Some(match n {
        1..=4 => "up to 4 sections",
        5..=6 => "5–6 sections",
        7..=9 => "7–9 sections",
        _ => "10 or more sections",
    }.to_string())
}

/// How long the title is, in the bands a search result actually cuts at.
///
/// YouTube truncates around 60 characters in most surfaces and around 40 on a phone's home feed, so
/// the bands are where the text stops being visible rather than round numbers.
pub fn title_length_band(title: &str) -> Option<String> {
    let n = title.trim().chars().count();
    if n == 0 { return None; }
    Some(match n {
        0..=40 => "≤40 chars",
        41..=60 => "41–60 chars",
        _ => "over 60 chars",
    }.to_string())
}

/// What kind of title it is. One label per title, so the buckets are disjoint and a row means
/// something; the order is the order of how strongly each form shapes a click.
pub fn title_form(title: &str) -> Option<String> {
    let t = title.trim();
    if t.is_empty() { return None; }
    Some(if t.contains('?') { "a question" }
        else if t.chars().any(|c| c.is_ascii_digit()) { "has a number" }
        else if t.contains('|') || t.contains('—') || t.contains(" - ") { "two parts" }
        else if t.contains(':') { "a colon" }
        else { "plain" }.to_string())
}

/// Rank channel / language / style combinations by how they actually performed.
///
/// Median, not mean: one video that went unexpectedly wide would otherwise make its whole
/// combination look like a strategy.
#[tauri::command]
pub async fn performance_report(state: State<'_, AppState>, project_id: Option<String>) -> Res<Value> {
    performance_report_inner(&state, project_id.as_deref().unwrap_or("")).await
}

async fn performance_report_inner(state: &AppState, project_id: &str) -> Res<Value> {
    use futures_util::StreamExt;
    let project_id = (!project_id.trim().is_empty()).then(|| project_id.to_string());

    // Songs first, so an upload can be attributed to a language/style.
    let song_filter = match project_id.as_deref().filter(|s| !s.is_empty()) {
        Some(pid) => doc! { "project_id": pid },
        None => doc! {},
    };
    let mut songs = HashMap::new();
    let mut cursor = state.db.collection::<Document>("songs").find(song_filter).await.map_err(e)?;
    while let Some(Ok(d)) = cursor.next().await {
        let song = crate::store::doc_to_json(&d);
        if let Some(id) = song["id"].as_str() {
            songs.insert(id.to_string(), song);
        }
    }

    // Sections per song. Loaded here rather than left out, which is why "how many pictures does this
    // video have" was the one axis in the plan that could be crossed with views and never was.
    let mut section_counts: HashMap<String, usize> = HashMap::new();
    let mut sec_cursor = state.db.collection::<Document>("sections").find(doc! {}).await.map_err(e)?;
    while let Some(Ok(d)) = sec_cursor.next().await {
        if let Ok(sid) = d.get_str("song_id") {
            if songs.contains_key(sid) { *section_counts.entry(sid.to_string()).or_default() += 1; }
        }
    }

    let mut channel_names: HashMap<String, String> = HashMap::new();
    // The channel's own zone, so an hour band means the audience's morning rather than the server's.
    let mut channel_zones: HashMap<String, String> = HashMap::new();
    let mut ch_cursor = state.db.collection::<Document>("channels").find(doc! {}).await.map_err(e)?;
    while let Some(Ok(d)) = ch_cursor.next().await {
        if let (Ok(id), Ok(name)) = (d.get_str("id"), d.get_str("name")) {
            channel_names.insert(id.to_string(), name.to_string());
            channel_zones.insert(id.to_string(),
                d.get_str("publish_timezone").unwrap_or("UTC").to_string());
        }
    }

    let mut buckets: HashMap<String, Vec<i64>> = HashMap::new();
    let mut per_video: Vec<Value> = Vec::new();
    let mut up_cursor = state
        .db
        .collection::<Document>("uploads")
        .find(doc! { "status": "published" })
        .await
        .map_err(e)?;
    while let Some(Ok(d)) = up_cursor.next().await {
        let upload = crate::store::doc_to_json(&d);
        let song_id = upload["song_id"].as_str().unwrap_or("");
        let Some(song) = songs.get(song_id) else { continue };
        let views = upload["views"].as_i64().unwrap_or(-1);
        if views < 0 {
            continue; // never had its stats fetched
        }
        let channel_id = upload["channel_id"].as_str().unwrap_or("").to_string();
        let zone = channel_zones.get(&channel_id).cloned().unwrap_or_else(|| "UTC".into());
        let channel = channel_names.get(&channel_id).cloned().unwrap_or(channel_id);
        let language = song["language"].as_str().unwrap_or("(none)").to_string();
        let style = song["styles"].as_str().unwrap_or("").split(',').next().unwrap_or("(none)").trim().to_string();

        buckets.entry(format!("channel::{channel}")).or_default().push(views);
        buckets.entry(format!("language::{language}")).or_default().push(views);
        buckets.entry(format!("style::{style}")).or_default().push(views);
        buckets.entry(format!("combo::{channel} · {language} · {style}")).or_default().push(views);

        // The axes the app already varies and never crossed. An unknown answer contributes to no
        // bucket rather than to a "(none)" one: a row made of everything whose duration was never
        // recorded is not a finding about duration.
        let published_at = upload["published_at"].as_str()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| upload["created_at"].as_str().unwrap_or(""));
        let title = upload["title"].as_str()
            .filter(|t| !t.trim().is_empty())
            .or_else(|| song["title"].as_str())
            .unwrap_or("");
        for (dimension, key) in [
            ("hour", hour_band(published_at, &zone)),
            ("weekday", weekday(published_at, &zone)),
            ("image_style", first_of(song["image_styles"].as_str().unwrap_or(""))),
            ("length", length_band(song["duration"].as_f64().unwrap_or(0.0))),
            ("sections", section_band(section_counts.get(song_id).copied().unwrap_or(0))),
            ("title_length", title_length_band(title)),
            ("title_form", title_form(title)),
        ] {
            if let Some(k) = key {
                buckets.entry(format!("{dimension}::{k}")).or_default().push(views);
            }
        }

        per_video.push(json!({
            "title": song["title"], "channel": channel, "language": language, "style": style,
            "views": views, "likes": upload["likes"], "video_id": upload["youtube_video_id"],
        }));
    }

    fn median(values: &mut Vec<i64>) -> i64 {
        if values.is_empty() { return 0; }
        values.sort_unstable();
        let mid = values.len() / 2;
        if values.len() % 2 == 0 { (values[mid - 1] + values[mid]) / 2 } else { values[mid] }
    }

    let mut grouped: Map<String, Value> = Map::new();
    for dimension in ["channel", "language", "style", "combo",
                      "hour", "weekday", "image_style", "length", "sections",
                      "title_length", "title_form"] {
        let mut rows: Vec<Value> = buckets
            .iter()
            .filter_map(|(key, values)| {
                let (kind, name) = key.split_once("::")?;
                if kind != dimension { return None; }
                let mut v = values.clone();
                Some(json!({
                    "key": name,
                    "videos": v.len(),
                    "median_views": median(&mut v),
                    "total_views": values.iter().sum::<i64>(),
                    "best": values.iter().max().copied().unwrap_or(0),
                }))
            })
            .collect();
        rows.sort_by(|a, b| b["median_views"].as_i64().unwrap_or(0).cmp(&a["median_views"].as_i64().unwrap_or(0)));
        grouped.insert(dimension.to_string(), Value::Array(rows));
    }

    per_video.sort_by(|a, b| b["views"].as_i64().unwrap_or(0).cmp(&a["views"].as_i64().unwrap_or(0)));
    // Counted before the list is trimmed for display: `measured_videos` is how many uploads carry
    // real numbers, and truncating first made it stop at 50 however many had actually been measured.
    let measured = per_video.len();
    per_video.truncate(50);

    let report = json!({
        "ok": true,
        "measured_videos": measured,
        "by": Value::Object(grouped),
        "top_videos": per_video,
        "note": "Ranked by median views — one runaway video shouldn't look like a strategy. Combinations with fewer than ~5 videos are noise, not signal.",
    });
    // Attached rather than computed separately, so the interface and the guide are looking at the
    // same suggestion from the same numbers.
    let mut report = report;
    report["experiment"] = experiment_suggestion(&report).unwrap_or(Value::Null);
    Ok(report)
}

/// What actually performed, in a few lines, for the guide to weigh when it proposes a run.
///
/// The studio has measured which channel/language/style combinations do well since the analytics
/// feedback loop landed, and it has recommended combinations since the guide landed — but the two
/// never met. The guide read the brief, the learnings store and the user's past picks, which between
/// them describe *habits*. A habit and a result are different things, and only one of them is
/// evidence.
///
/// Deliberately conservative about what counts as evidence, because a recommendation carries more
/// weight than a chart: nothing at all below `MIN_MEASURED` measured videos, no row below
/// `MIN_ROW_VIDEOS`, and every row states its own sample size so a thin one can be discounted rather
/// than believed. The worst row is included too — knowing what to stop doing is the cheaper half.
pub async fn performance_prompt_block(state: &AppState, project_id: &str) -> String {
    match performance_report_inner(state, project_id).await {
        Ok(report) => performance_block_from(&report),
        Err(_) => String::new(),
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Varying something on purpose
// ────────────────────────────────────────────────────────────────────────────

/// The values an axis can take, where the app knows the whole vocabulary.
///
/// Only the closed axes are here. Style, image style and language are open — the creator invents
/// them — so the app can say "you have only ever used one" but must not invent the alternative.
const KNOWN_VALUES: &[(&str, &[&str])] = &[
    ("hour", &["00:00–03:00", "03:00–06:00", "06:00–09:00", "09:00–12:00",
               "12:00–15:00", "15:00–18:00", "18:00–21:00", "21:00–24:00"]),
    ("weekday", &["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]),
    ("length", &["under 1:30", "1:30–2:30", "2:30–3:30", "3:30–5:00", "over 5:00"]),
    ("sections", &["up to 4 sections", "5–6 sections", "7–9 sections", "10 or more sections"]),
    ("title_length", &["≤40 chars", "41–60 chars", "over 60 chars"]),
    ("title_form", &["a question", "has a number", "two parts", "a colon", "plain"]),
];

/// An axis where one value accounts for at least this share of everything published has not been
/// tested; it has been assumed. Four-fifths rather than "all", because one stray video does not
/// make a habit into an experiment.
const CONCENTRATED: f64 = 0.8;

/// What to vary next, and why.
///
/// Every combination this app produces today is *chosen* — from the brief, from habit, and since the
/// evidence loop landed, from what has worked. None of it is varied on purpose, so the ranking can
/// only ever rank what was already going to be made. That is the difference between measuring
/// history and learning: an axis the creator has only ever set one way has no data behind it and
/// never will, however many videos they publish.
///
/// Pure, over a `performance_report` value. It proposes the *most concentrated* axis — the one where
/// the creator's own record says least — and names a specific alternative where the app knows the
/// vocabulary. It says nothing at all when there is not enough published to know, and nothing when
/// every axis already has spread, because at that point the ranking is the better advice.
pub fn experiment_suggestion(report: &Value) -> Option<Value> {
    if (report["measured_videos"].as_u64().unwrap_or(0) as usize) < MIN_MEASURED { return None; }

    let mut best: Option<(f64, String, String, u64, u64)> = None;   // share, axis, value, top, total
    for (axis, _) in KNOWN_VALUES.iter().chain(
        [("style", &[] as &[&str]), ("image_style", &[]), ("language", &[])].iter())
    {
        let rows = report["by"][*axis].as_array().cloned().unwrap_or_default();
        if rows.is_empty() { continue; }
        let total: u64 = rows.iter().map(|r| r["videos"].as_u64().unwrap_or(0)).sum();
        if (total as usize) < MIN_MEASURED { continue; }
        // The largest row by count, which is not the same as the best-performing one: this is about
        // what has been done most, not what has done best.
        let top = rows.iter().max_by_key(|r| r["videos"].as_u64().unwrap_or(0))?;
        let n = top["videos"].as_u64().unwrap_or(0);
        let share = n as f64 / total as f64;
        if share < CONCENTRATED { continue; }
        let value = top["key"].as_str().unwrap_or("").to_string();
        if best.as_ref().is_none_or(|(s, ..)| share > *s) {
            best = Some((share, (*axis).to_string(), value, n, total));
        }
    }

    let (share, axis, value, n, total) = best?;
    // A specific alternative where the app knows the vocabulary; silence where the creator invents
    // it, because inventing a musical style for somebody is not a suggestion, it is a guess.
    let alternatives: Vec<String> = KNOWN_VALUES.iter()
        .find(|(a, _)| *a == axis)
        .map(|(_, vs)| vs.iter().filter(|v| **v != value).map(|v| v.to_string()).collect())
        .unwrap_or_default();

    Some(json!({
        "axis": axis,
        "current": value,
        "videos": n,
        "of": total,
        "share": (share * 100.0).round() as i64,
        "try": alternatives.first().cloned(),
        "alternatives": alternatives,
        "why": format!(
            "{n} of your {total} measured videos are the same on this axis, so the ranking has \
             nothing to compare it against — however many more you publish."),
    }))
}

/// The axes about the work itself, as opposed to how it was packaged and when it went out. They get
/// more room in the block, because a creator can act on them and because a block that reads as
/// scheduling advice is weighed as scheduling advice.
const CREATIVE: &[&str] = &["combo", "language", "style", "image_style", "length", "sections"];

/// The block itself, from a `performance_report` value — pure, so the evidence rules are testable.
fn performance_block_from(report: &Value) -> String {
    let measured = report["measured_videos"].as_u64().unwrap_or(0) as usize;
    if measured < MIN_MEASURED { return String::new(); }

    let describe = |row: &Value| {
        let videos = row["videos"].as_u64().unwrap_or(0);
        format!(
            "{} — {} median views over {videos} video{}{}",
            row["key"].as_str().unwrap_or("?"),
            row["median_views"].as_i64().unwrap_or(0),
            if videos == 1 { "" } else { "s" },
            if (videos as usize) < THIN_ROW { " (thin — treat as a hint, not a finding)" } else { "" },
        )
    };

    let mut lines: Vec<String> = Vec::new();
    // The creative axes first, then the ones about packaging and timing. Two rows each for the
    // latter rather than three: they are weaker signals and a block that is mostly scheduling advice
    // is a block the model weighs as scheduling advice.
    for dimension in ["combo", "language", "style", "image_style", "length", "sections",
                      "hour", "weekday", "title_form", "title_length"] {
        let rows: Vec<&Value> = report["by"][dimension].as_array()
            .map(|a| a.iter().filter(|r| r["videos"].as_u64().unwrap_or(0) as usize >= MIN_ROW_VIDEOS).collect())
            .unwrap_or_default();
        let creative = CREATIVE.contains(&dimension);
        // On a packaging axis, one row is a tautology: a creator who has only ever published at
        // seven in the evening gets "best hour: 18:00–21:00", which restates their habit in the
        // voice of evidence and is worse than silence. A creative axis with one row still carries
        // its magnitude — what a normal result looks like here — so it stays.
        if !creative && rows.len() < 2 { continue; }
        // One best and one worst on a packaging axis: that pair *is* the comparison, and a
        // second-place row labelled "best" reads as a recommendation of both.
        let top = if creative { 3 } else { 1 };
        for row in rows.iter().take(top) {
            lines.push(format!("- best {dimension}: {}", describe(row)));
        }
        // Only worth naming when there is something to compare it against.
        if rows.len() > top {
            lines.push(format!("- weakest {dimension}: {}", describe(rows[rows.len() - 1])));
        }
    }
    if lines.is_empty() { return String::new(); }

    // What has never been tested belongs in the same block as what has: an axis with one value is a
    // hole in the evidence, and the model proposing a run is the one thing in the app positioned to
    // fill it.
    let untested = match experiment_suggestion(report) {
        Some(s) => format!(
            "\nNEVER VARIED: {} — {} of {} measured videos are \"{}\". {}Proposing something different \
             here is worth more than another confirmation of what is already known, so long as the \
             rest of the run stays the same.\n",
            s["axis"].as_str().unwrap_or(""),
            s["videos"].as_u64().unwrap_or(0), s["of"].as_u64().unwrap_or(0),
            s["current"].as_str().unwrap_or(""),
            match s["try"].as_str() {
                Some(t) => format!("Try \"{t}\". "),
                None => String::new(),
            },
        ),
        None => String::new(),
    };

    format!(
        "WHAT THIS CREATOR'S PUBLISHED VIDEOS ACTUALLY DID ({measured} measured). This is evidence, not \
         habit — prefer it over past choices when the two disagree, and say so in your reason. Ranked \
         by median views, so one runaway video is not a strategy:\n{}\n{untested}",
        lines.join("\n"),
    )
}

// ────────────────────────────────────────────────────────────────────────────
// Quotas
// ────────────────────────────────────────────────────────────────────────────

/// The limits the user currently discovers by hitting them, gathered in one place.
///
/// Where an exact figure isn't knowable without a paid API call (YouTube's actual quota consumption,
/// Kaggle's GPU-hours), this reports what the app itself observed — uploads today, sessions started
/// this week — and says so, rather than inventing precision.
#[tauri::command]
pub async fn quota_report(state: State<'_, AppState>) -> Res<Value> {
    use futures_util::StreamExt;

    let today = chrono::Utc::now().date_naive();
    let week_ago = chrono::Utc::now() - chrono::Duration::days(7);

    let mut uploads_today = 0u64;
    let mut uploads_week = 0u64;
    let mut cursor = state
        .db
        .collection::<Document>("uploads")
        .find(doc! { "status": "published" })
        .await
        .map_err(e)?;
    while let Some(Ok(d)) = cursor.next().await {
        let Some(published) = d.get_str("published_at").ok() else { continue };
        let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(published) else { continue };
        let parsed = parsed.with_timezone(&chrono::Utc);
        if parsed.date_naive() == today { uploads_today += 1; }
        if parsed > week_ago { uploads_week += 1; }
    }

    // AI calls the app made, from the job log — a proxy for free-tier request consumption.
    let mut ai_jobs_today = 0u64;
    let mut failed_today = 0u64;
    let mut jobs_cursor = state.db.collection::<Document>("jobs").find(doc! {}).await.map_err(e)?;
    while let Some(Ok(d)) = jobs_cursor.next().await {
        let Some(created) = d.get_str("created_at").ok() else { continue };
        let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(created) else { continue };
        if parsed.with_timezone(&chrono::Utc).date_naive() != today { continue; }
        ai_jobs_today += 1;
        if d.get_str("status").unwrap_or("") == "failed" { failed_today += 1; }
    }

    let settings = state
        .db
        .collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" })
        .await
        .map_err(e)?
        .map(|d| crate::store::doc_to_json(&d))
        .unwrap_or_else(|| json!({}));

    let kaggle_accounts = settings["kaggle_accounts"].as_array().map(|a| a.len()).unwrap_or(0);
    let disk = disk_free(state.db.global_root());

    // Computed before the json! below rather than inside it: the macro cannot parse a method chain
    // hanging off an array literal in value position.
    let ai_used_today = crate::ai_budget::usage_today();
    let ai_free_remaining: u64 = ["openrouter", "gemini"].iter()
        .filter(|p| crate::ai_budget::configured(p, &settings))
        .filter_map(|p| crate::ai_budget::remaining(p, &settings))
        .sum();
    let ai_rotation = crate::ai_budget::rotation(&settings);

    Ok(json!({
        "youtube": {
            "label": "YouTube uploads",
            "used_today": uploads_today,
            "used_this_week": uploads_week,
            "limit": 6,
            "limit_note": "A new API project gets ~10,000 quota units/day and an upload costs ~1,600 — roughly 6 uploads a day. The figure here counts what this app published, not Google's own accounting.",
        },
        "kaggle": {
            "label": "Kaggle GPU",
            "accounts": kaggle_accounts,
            "limit_note": "30 GPU hours per account per week, resetting Saturday 00:00 UTC. Add accounts to rotate when one runs out.",
            "active": settings["kaggle_active"],
        },
        // Real counts, from the ledger every provider call is recorded in — not the job count this
        // used to report. That proxy was wrong in both directions: most AI calls never create a job
        // (lyrics, styles, translation, the guide, upload metadata), and most jobs are not AI calls
        // (music, images, video, uploads). `jobs_today` is kept because it is a true and separate
        // fact worth seeing next to it, but it no longer pretends to be the AI number.
        "ai": {
            "label": "AI requests",
            "provider": settings["ai_provider"],
            "used_today": ai_used_today,
            "free_remaining": ai_free_remaining,
            "rotation": ai_rotation,
            "jobs_today": ai_jobs_today,
            "failed_today": failed_today,
            "limit_note": "OpenRouter's free tier is 50 requests a day for the whole account (1,000 with credits); Gemini's free tier is per-minute and per-day per model. The app rotates to another configured provider when one is spent, and stops asking one that has nothing left.",
        },
        "disk": disk,
    }))
}

/// Free space on the volume holding the data directory. Uses `statvfs` via `df` rather than adding
/// a dependency for one number.
fn disk_free(path: &std::path::Path) -> Value {
    #[cfg(desktop)]
    {
        let out = std::process::Command::new("df")
            .arg("-k")
            .arg(path)
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        if let Some(line) = out.lines().nth(1) {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() >= 4 {
                let total_kb: u64 = cols[1].parse().unwrap_or(0);
                let avail_kb: u64 = cols[3].parse().unwrap_or(0);
                return json!({
                    "label": "Disk",
                    "free_bytes": avail_kb * 1024,
                    "total_bytes": total_kb * 1024,
                    "path": path.to_string_lossy(),
                    "limit_note": "Generated audio and video are the bulk of this. Offload assets in Data & Sync to reclaim space.",
                });
            }
        }
    }
    json!({ "label": "Disk", "free_bytes": Value::Null, "path": path.to_string_lossy() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_song_status_maps_into_the_pipeline() {
        assert_eq!(stage_index("draft"), 0);
        assert_eq!(stage_index("uploaded"), STAGES.len() - 1);
        assert!(stage_index("music_ready") < stage_index("video_ready"));
        // An unknown or empty status must not vanish from the totals.
        assert_eq!(stage_index("something-new"), 0);
        assert_eq!(stage_index(""), 0);
    }

    // ── the axes ────────────────────────────────────────────────────────────

    #[test]
    fn an_hour_band_is_the_audiences_hour_and_not_the_servers() {
        // The same instant. 15:00 UTC is mid-afternoon in Berlin and breakfast in Los Angeles, and
        // "does the hour matter" is only a question about the audience's own day.
        let at = "2026-07-25T15:00:00Z";
        assert_eq!(hour_band(at, "Europe/Berlin").unwrap(), "15:00–18:00");
        assert_eq!(hour_band(at, "America/Los_Angeles").unwrap(), "06:00–09:00");
        assert_eq!(hour_band(at, "UTC").unwrap(), "15:00–18:00");
    }

    #[test]
    fn three_hour_bands_because_twenty_four_buckets_over_thirty_videos_is_noise() {
        for (hour, band) in [(0, "00:00–03:00"), (2, "00:00–03:00"), (3, "03:00–06:00"),
                             (21, "21:00–24:00"), (23, "21:00–24:00")] {
            let at = format!("2026-07-25T{hour:02}:30:00Z");
            assert_eq!(hour_band(&at, "UTC").unwrap(), band, "hour {hour}");
        }
    }

    #[test]
    fn a_timestamp_or_a_zone_that_cannot_be_read_contributes_to_no_bucket() {
        // Left out of the dimension rather than bucketed as unknown: a row made of everything the
        // app failed to parse is not a finding about anything.
        assert!(hour_band("", "UTC").is_none());
        assert!(hour_band("last Tuesday", "UTC").is_none());
        assert!(hour_band("2026-07-25T15:00:00Z", "Mars/Olympus").is_none());
        assert!(weekday("2026-07-25T15:00:00Z", "").is_none());
    }

    #[test]
    fn the_weekday_crosses_midnight_with_the_channels_zone() {
        // 23:00 Saturday UTC is already Sunday in Tokyo.
        let at = "2026-07-25T23:00:00Z";   // a Saturday
        assert_eq!(weekday(at, "UTC").unwrap(), "Sat");
        assert_eq!(weekday(at, "Asia/Tokyo").unwrap(), "Sun");
    }

    #[test]
    fn song_length_lands_in_the_bands_people_choose_between() {
        assert_eq!(length_band(75.0).unwrap(), "under 1:30");
        assert_eq!(length_band(120.0).unwrap(), "1:30–2:30");
        assert_eq!(length_band(200.0).unwrap(), "2:30–3:30");
        assert_eq!(length_band(280.0).unwrap(), "3:30–5:00");
        assert_eq!(length_band(600.0).unwrap(), "over 5:00");
        // A duration that was never measured is not a band.
        assert!(length_band(0.0).is_none());
        assert!(length_band(f64::NAN).is_none());
    }

    #[test]
    fn section_counts_land_in_the_bands_that_are_actually_a_choice() {
        assert_eq!(section_band(3).unwrap(), "up to 4 sections");
        assert_eq!(section_band(6).unwrap(), "5–6 sections");
        assert_eq!(section_band(8).unwrap(), "7–9 sections");
        assert_eq!(section_band(14).unwrap(), "10 or more sections");
        // A song that was never cut into sections is not a finding about section counts.
        assert!(section_band(0).is_none());
    }

    #[test]
    fn title_bands_sit_where_the_text_actually_gets_cut_off() {
        assert_eq!(title_length_band("Psalm 23").unwrap(), "≤40 chars");
        assert_eq!(title_length_band(&"x".repeat(50)).unwrap(), "41–60 chars");
        assert_eq!(title_length_band(&"x".repeat(75)).unwrap(), "over 60 chars");
        assert!(title_length_band("   ").is_none());
        // Counted in characters, not bytes: a title in Japanese is not three times as long.
        assert_eq!(title_length_band(&"あ".repeat(30)).unwrap(), "≤40 chars");
    }

    #[test]
    fn every_title_gets_exactly_one_form_so_the_rows_mean_something() {
        assert_eq!(title_form("Who shall dwell?").unwrap(), "a question");
        assert_eq!(title_form("Psalm 23").unwrap(), "has a number");
        assert_eq!(title_form("The Light | Lightkid").unwrap(), "two parts");
        assert_eq!(title_form("Genesis: a beginning").unwrap(), "a colon");
        assert_eq!(title_form("The light has come").unwrap(), "plain");
        assert!(title_form("  ").is_none());
        // A title that is several forms at once still gets one bucket, and the same one every time.
        assert_eq!(title_form("Psalm 23: who shall dwell?").unwrap(), "a question");
    }

    // ── the evidence block the guide reads ──────────────────────────────────

    fn row(key: &str, videos: u64, median: i64) -> Value {
        json!({ "key": key, "videos": videos, "median_views": median, "total_views": median * videos as i64, "best": median })
    }
    fn report(measured: u64, combos: Vec<Value>) -> Value {
        json!({ "measured_videos": measured, "by": { "combo": combos, "language": [], "style": [] } })
    }

    #[test]
    fn too_little_measured_says_nothing_at_all() {
        // Four uploads ranked against each other is how one lucky video becomes a strategy.
        let r = report(4, vec![row("A · English · dnb", 3, 900), row("B · German · folk", 1, 10)]);
        assert_eq!(performance_block_from(&r), "");
    }

    #[test]
    fn a_row_with_almost_no_videos_is_not_reported() {
        let r = report(9, vec![row("A · English · dnb", 6, 500), row("B · German · folk", 2, 4000)]);
        let block = performance_block_from(&r);
        assert!(block.contains("A · English · dnb"));
        // The two-video row has the higher median and is still not evidence of anything.
        assert!(!block.contains("B · German · folk"), "{block}");
    }

    #[test]
    fn a_thin_row_is_reported_but_labelled_as_thin() {
        let r = report(9, vec![row("A · English · dnb", 3, 500)]);
        let block = performance_block_from(&r);
        assert!(block.contains("over 3 videos (thin"), "{block}");
    }

    #[test]
    fn a_row_with_enough_behind_it_is_stated_plainly() {
        let r = report(12, vec![row("A · English · dnb", 7, 500)]);
        let block = performance_block_from(&r);
        assert!(block.contains("over 7 videos"), "{block}");
        assert!(!block.contains("thin"), "{block}");
    }

    #[test]
    fn the_weakest_is_named_only_when_there_is_a_field_to_be_last_in() {
        let three = report(12, vec![row("a", 5, 900), row("b", 5, 500), row("c", 5, 100)]);
        assert!(!performance_block_from(&three).contains("weakest"));

        let four = report(20, vec![row("a", 5, 900), row("b", 5, 500), row("c", 5, 300), row("d", 5, 20)]);
        let block = performance_block_from(&four);
        assert!(block.contains("weakest combo: d"), "{block}");
        // …and the top three are still the top three, not four.
        assert_eq!(block.matches("best combo").count(), 3);
    }

    #[test]
    fn one_row_on_a_packaging_axis_is_a_habit_wearing_the_voice_of_evidence() {
        // A creator who has only ever published at seven would otherwise be told that seven is best.
        let mut r = report(20, vec![row("A · English · dnb", 8, 500)]);
        r["by"]["hour"] = json!([row("18:00–21:00", 8, 500)]);
        let block = performance_block_from(&r);
        assert!(!block.contains("best hour"), "{block}");
        // It is not silence, though: an axis with one value is a hole in the evidence, and saying so
        // is the useful thing to say about it.
        assert!(block.contains("NEVER VARIED: hour"), "{block}");
        // Two bands is a comparison, and then it is worth saying.
        r["by"]["hour"] = json!([row("18:00–21:00", 5, 900), row("06:00–09:00", 5, 100)]);
        let block = performance_block_from(&r);
        assert!(block.contains("best hour: 18:00–21:00"), "{block}");
        assert!(block.contains("weakest hour: 06:00–09:00"), "{block}");
    }

    #[test]
    fn a_packaging_axis_gets_less_room_than_a_creative_one() {
        // A block that reads as scheduling advice is weighed as scheduling advice.
        let four = |n| json!([row("a", n, 900), row("b", n, 700), row("c", n, 500), row("d", n, 20)]);
        let mut r = report(30, vec![]);
        r["by"]["style"] = four(5);
        r["by"]["title_form"] = four(5);
        let block = performance_block_from(&r);
        assert_eq!(block.matches("best style").count(), 3);
        assert_eq!(block.matches("best title_form").count(), 1);
        assert_eq!(block.matches("weakest style").count(), 1);
        assert_eq!(block.matches("weakest title_form").count(), 1);
    }

    // ── varying something on purpose ────────────────────────────────────────

    #[test]
    fn the_axis_proposed_is_the_one_the_creators_own_record_says_least_about() {
        let mut r = report(20, vec![row("A · English · dnb", 20, 500)]);
        // Hours are spread; every title is the same shape.
        r["by"]["hour"] = json!([row("18:00–21:00", 10, 700), row("09:00–12:00", 10, 300)]);
        r["by"]["title_form"] = json!([row("plain", 19, 500), row("a question", 1, 800)]);
        let s = experiment_suggestion(&r).expect("something is untested here");
        assert_eq!(s["axis"], "title_form");
        assert_eq!(s["current"], "plain");
        assert_eq!(s["videos"], 19);
        assert_eq!(s["share"], 95);
    }

    #[test]
    fn it_proposes_by_how_often_a_value_was_used_and_not_by_how_well_it_did() {
        // The point is the hole in the evidence, not the ranking — the dominant value being the
        // worst performer would make it more worth varying, not less.
        let mut r = report(20, vec![]);
        r["by"]["length"] = json!([row("over 5:00", 2, 9000), row("2:30–3:30", 18, 100)]);
        let s = experiment_suggestion(&r).unwrap();
        assert_eq!(s["current"], "2:30–3:30", "the most used, not the best");
    }

    #[test]
    fn an_axis_that_already_has_spread_is_not_an_experiment_waiting_to_happen() {
        let mut r = report(20, vec![]);
        r["by"]["hour"] = json!([row("18:00–21:00", 11, 700), row("09:00–12:00", 9, 300)]);
        assert!(experiment_suggestion(&r).is_none(), "at that point the ranking is the advice");
    }

    #[test]
    fn nothing_is_proposed_before_there_is_enough_published_to_know() {
        let mut r = report(4, vec![]);
        r["by"]["title_form"] = json!([row("plain", 4, 500)]);
        assert!(experiment_suggestion(&r).is_none());
    }

    #[test]
    fn a_specific_alternative_where_the_app_knows_the_vocabulary_and_none_where_it_does_not() {
        let mut closed = report(20, vec![]);
        closed["by"]["title_form"] = json!([row("plain", 20, 500)]);
        let s = experiment_suggestion(&closed).unwrap();
        assert!(s["try"].is_string(), "the title forms are a closed set");
        assert!(!s["alternatives"].as_array().unwrap().iter().any(|v| v == "plain"),
                "it must not propose what is already being done");

        // Musical style is invented by the creator, so naming one for them would be a guess wearing
        // the voice of a suggestion.
        let mut open = report(20, vec![]);
        open["by"]["style"] = json!([row("liquid dnb", 20, 500)]);
        let s = experiment_suggestion(&open).unwrap();
        assert_eq!(s["axis"], "style");
        assert!(s["try"].is_null(), "{s}");
        assert!(s["alternatives"].as_array().unwrap().is_empty());
    }

    #[test]
    fn every_known_vocabulary_matches_the_bands_the_report_actually_produces() {
        // A suggestion naming a value the bucketing can never produce would never be measurable.
        let hours: &[&str] = KNOWN_VALUES.iter().find(|(a, _)| *a == "hour").unwrap().1;
        for h in 0..24u32 {
            let at = format!("2026-07-25T{h:02}:00:00Z");
            let band = hour_band(&at, "UTC").unwrap();
            assert!(hours.contains(&band.as_str()), "{band} is not in the offered set");
        }
        let lengths: &[&str] = KNOWN_VALUES.iter().find(|(a, _)| *a == "length").unwrap().1;
        for secs in [30.0, 100.0, 180.0, 250.0, 400.0] {
            let band = length_band(secs).unwrap();
            assert!(lengths.contains(&band.as_str()), "{band} is not in the offered set");
        }
        let sections: &[&str] = KNOWN_VALUES.iter().find(|(a, _)| *a == "sections").unwrap().1;
        for n in [1usize, 4, 5, 6, 7, 9, 10, 25] {
            let band = section_band(n).unwrap();
            assert!(sections.contains(&band.as_str()), "{band} is not in the offered set");
        }
        let forms: &[&str] = KNOWN_VALUES.iter().find(|(a, _)| *a == "title_form").unwrap().1;
        for t in ["Who?", "Psalm 23", "A | B", "A: b", "plain words"] {
            let f = title_form(t).unwrap();
            assert!(forms.contains(&f.as_str()), "{f} is not in the offered set");
        }
    }

    #[test]
    fn every_row_carries_its_own_sample_size_so_it_can_be_discounted() {
        let r = report(12, vec![row("a", 5, 900), row("b", 4, 800)]);
        let block = performance_block_from(&r);
        assert!(block.contains("over 5 videos"));
        assert!(block.contains("over 4 videos (thin"));
        assert!(block.contains("12 measured"));
    }

    #[test]
    fn no_reportable_rows_means_no_block_even_with_plenty_measured() {
        // Fifty uploads spread one-per-combination is still nothing anyone should act on.
        let r = report(50, (0..50).map(|i| row(&format!("c{i}"), 1, 100)).collect());
        assert_eq!(performance_block_from(&r), "");
    }
}
