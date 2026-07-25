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

/// Rank channel / language / style combinations by how they actually performed.
///
/// Median, not mean: one video that went unexpectedly wide would otherwise make its whole
/// combination look like a strategy.
#[tauri::command]
pub async fn performance_report(state: State<'_, AppState>, project_id: Option<String>) -> Res<Value> {
    use futures_util::StreamExt;

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

    let mut channel_names: HashMap<String, String> = HashMap::new();
    let mut ch_cursor = state.db.collection::<Document>("channels").find(doc! {}).await.map_err(e)?;
    while let Some(Ok(d)) = ch_cursor.next().await {
        if let (Ok(id), Ok(name)) = (d.get_str("id"), d.get_str("name")) {
            channel_names.insert(id.to_string(), name.to_string());
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
        let channel = channel_names.get(&channel_id).cloned().unwrap_or(channel_id);
        let language = song["language"].as_str().unwrap_or("(none)").to_string();
        let style = song["styles"].as_str().unwrap_or("").split(',').next().unwrap_or("(none)").trim().to_string();

        buckets.entry(format!("channel::{channel}")).or_default().push(views);
        buckets.entry(format!("language::{language}")).or_default().push(views);
        buckets.entry(format!("style::{style}")).or_default().push(views);
        buckets.entry(format!("combo::{channel} · {language} · {style}")).or_default().push(views);

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
    for dimension in ["channel", "language", "style", "combo"] {
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
    per_video.truncate(50);

    Ok(json!({
        "ok": true,
        "measured_videos": per_video.len(),
        "by": Value::Object(grouped),
        "top_videos": per_video,
        "note": "Ranked by median views — one runaway video shouldn't look like a strategy. Combinations with fewer than ~5 videos are noise, not signal.",
    }))
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
        "ai": {
            "label": "AI requests",
            "provider": settings["ai_provider"],
            "jobs_today": ai_jobs_today,
            "failed_today": failed_today,
            "limit_note": "OpenRouter free models allow ~50 requests/day (1,000 with credits); Gemini's free tier is per-minute and per-day per model. Repeated failures today usually mean the limit, not a bug.",
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
}
