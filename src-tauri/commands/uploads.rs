use crate::{jobs::enqueue, models::now_iso, state::AppState};
use bson::{doc, Document};
use regex::Regex;
use serde_json::Value;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

fn bson_to_value(doc: Document) -> Value {
    let mut m = serde_json::Map::new();
    for (k, v) in doc {
        if k == "_id" { continue; }
        if let Ok(jv) = bson::from_bson::<Value>(v) { m.insert(k, jv); }
    }
    Value::Object(m)
}

#[tauri::command]
pub async fn list_uploads(state: State<'_, AppState>) -> Res<Vec<Value>> {
    use futures_util::StreamExt;
    let mut cursor = state.db.collection::<Document>("uploads")
        .find(doc! {}).sort(doc! { "created_at": -1 })
        .await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }
    Ok(out)
}

#[tauri::command]
pub async fn create_upload(state: State<'_, AppState>, body: Value) -> Res<Value> {
    let u = serde_json::json!({
        "id": Uuid::new_v4().to_string(),
        "song_id": body["song_id"],
        "channel_id": body["channel_id"],
        "title": body["title"].as_str().unwrap_or(""),
        "description": body["description"].as_str().unwrap_or(""),
        "tags": body["tags"].as_array().cloned().unwrap_or_default(),
        "category": body["category"].as_str().unwrap_or("Music"),
        "privacy": body["privacy"].as_str().unwrap_or("private"),
        "format": body["format"].as_str().unwrap_or("youtube"),
        "status": "pending",
        "created_at": now_iso(),
    });
    let bson = bson::to_document(&u).map_err(e)?;
    state.db.collection::<Document>("uploads").insert_one(bson).await.map_err(e)?;
    Ok(u)
}

#[tauri::command]
pub async fn publish_upload(
    state: State<'_, AppState>,
    state_arc: State<'_, Arc<AppState>>,
    uid: String,
) -> Res<Value> {
    state.db.collection::<Document>("uploads")
        .find_one(doc! { "id": &uid }).await.map_err(e)?
        .ok_or_else(|| "missing".to_string())?;
    state.db.collection::<Document>("uploads")
        .update_one(doc! { "id": &uid }, doc! { "$set": { "status": "uploading" } })
        .await.map_err(e)?;
    let job = enqueue("upload", &uid, &state_arc).await.map_err(e)?;
    Ok(serde_json::to_value(job).map_err(e)?)
}

#[tauri::command]
pub async fn publish_all_uploads(
    state: State<'_, AppState>,
    state_arc: State<'_, Arc<AppState>>,
) -> Res<Value> {
    use futures_util::StreamExt;
    let mut cursor = state.db.collection::<Document>("uploads")
        .find(doc! { "status": "pending" }).await.map_err(e)?;
    let mut ids = Vec::new();
    while let Some(Ok(d)) = cursor.next().await {
        if let Some(id) = d.get_str("id").ok() { ids.push(id.to_string()); }
    }
    let mut jobs = Vec::new();
    for uid in &ids {
        state.db.collection::<Document>("uploads")
            .update_one(doc! { "id": uid }, doc! { "$set": { "status": "uploading" } })
            .await.map_err(e)?;
        let job = enqueue("upload", uid, &state_arc).await.map_err(e)?;
        jobs.push(serde_json::to_value(job).map_err(e)?);
    }
    Ok(serde_json::json!({ "queued": jobs.len() }))
}

#[tauri::command]
pub async fn bulk_uploads_from_videos(state: State<'_, AppState>, body: Value) -> Res<Value> {
    use futures_util::StreamExt;
    let pid = body["project_id"].as_str().map(|s| s.to_string());
    let formats: Vec<String> = body["formats"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_else(|| vec!["youtube".into()]);
    let privacy = body["privacy"].as_str().unwrap_or("public").to_string();

    let mut q = doc! { "status": "video_ready" };
    if let Some(ref pid) = pid { q.insert("project_id", pid.as_str()); }
    let mut cursor = state.db.collection::<Document>("songs").find(q).await.map_err(e)?;
    let mut songs = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { songs.push(bson_to_value(d)); }

    let mut cursor = state.db.collection::<Document>("channels").find(doc! {}).await.map_err(e)?;
    let mut channels = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { channels.push(bson_to_value(d)); }

    let mut created = Vec::new();
    for song in &songs {
        let mut matching: Vec<&Value> = channels.iter().filter(|c| {
            c["language"].as_str().map(|l| l.to_lowercase())
                == song["language"].as_str().map(|l| l.to_lowercase())
        }).collect();
        if matching.is_empty() { matching = channels.iter().collect(); }
        for ch in &matching {
            for fmt in &formats {
                let exists = state.db.collection::<Document>("uploads")
                    .find_one(doc! {
                        "song_id": song["id"].as_str().unwrap_or(""),
                        "channel_id": ch["id"].as_str().unwrap_or(""),
                        "format": fmt.as_str(),
                    }).await.map_err(e)?.is_some();
                if exists { continue; }
                let u = serde_json::json!({
                    "id": Uuid::new_v4().to_string(),
                    "song_id": song["id"],
                    "channel_id": ch["id"],
                    "title": song["title"].as_str().unwrap_or(""),
                    "description": "",
                    "tags": [],
                    "category": "Music",
                    "privacy": &privacy,
                    "format": fmt,
                    "status": "pending",
                    "created_at": now_iso(),
                });
                let bson = bson::to_document(&u).map_err(e)?;
                state.db.collection::<Document>("uploads").insert_one(bson).await.map_err(e)?;
                created.push(u);
            }
        }
    }
    Ok(serde_json::json!({ "created": created.len(), "songs": songs.len(), "channels": channels.len() }))
}

#[tauri::command]
pub async fn uploads_preflight(state: State<'_, AppState>) -> Res<Value> {
    use futures_util::StreamExt;
    let mut cursor = state.db.collection::<Document>("uploads")
        .find(doc! { "status": "pending" }).await.map_err(e)?;
    let mut pending = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { pending.push(bson_to_value(d)); }
    let ch_ids: std::collections::HashSet<String> = pending.iter()
        .filter_map(|u| u["channel_id"].as_str().map(|s| s.to_string()))
        .collect();
    let mut need = Vec::new();
    let mut ready = Vec::new();
    for cid in &ch_ids {
        let ch_doc = state.db.collection::<Document>("channels")
            .find_one(doc! { "id": cid.as_str() }).await.map_err(e)?;
        let Some(ch_doc) = ch_doc else { continue; };
        let ch = bson_to_value(ch_doc);
        if ch["connected"].as_bool().unwrap_or(false) && ch["refresh_token"].is_string() {
            ready.push(serde_json::json!({ "channel_id": cid, "name": ch["name"] }));
            continue;
        }
        let client = crate::jobs::pick_oauth_client(&state.db, &ch, ch["oauth_client_id"].as_str()).await;
        let Some(client) = client else {
            need.push(serde_json::json!({ "channel_id": cid, "name": ch["name"], "url": "", "label": null, "error": "no oauth client matches this channel's language" }));
            continue;
        };
        let scope = "https://www.googleapis.com/auth/youtube.upload https://www.googleapis.com/auth/youtube.readonly";
        let url = format!(
            "https://accounts.google.com/o/oauth2/v2/auth?response_type=code&client_id={}&redirect_uri={}&scope={}&access_type=offline&prompt=consent&state={cid}",
            client["client_id"].as_str().unwrap_or(""),
            client["redirect_uri"].as_str().unwrap_or(""),
            scope.replace(' ', "%20"),
        );
        state.db.collection::<Document>("channels")
            .update_one(doc! { "id": cid.as_str() },
                doc! { "$set": { "oauth_client_id": client["id"].as_str().unwrap_or("") } })
            .await.map_err(e)?;
        need.push(serde_json::json!({ "channel_id": cid, "name": ch["name"], "url": url, "label": client["label"] }));
    }
    Ok(serde_json::json!({ "need_oauth": need, "ready": ready, "pending_uploads": pending.len() }))
}

#[tauri::command]
pub async fn ai_enrich_uploads(state: State<'_, AppState>, body: Value) -> Res<Value> {
    use futures_util::StreamExt;
    let global_desc = body["global_description"].as_str().unwrap_or("").trim().to_string();
    let only_empty  = body["only_empty"].as_bool().unwrap_or(true);
    let regenerate  = body["regenerate"].as_bool().unwrap_or(false);

    let mut cursor = state.db.collection::<Document>("uploads")
        .find(doc! { "status": "pending" }).await.map_err(e)?;
    let mut uploads = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { uploads.push(bson_to_value(d)); }

    // ── Pass one: what needs saying, and what the model needs to know to say it ──────────────
    //
    // Gathered before anything is asked, so the AI can be called once for the whole set instead of
    // twice per upload. See enrich_batch — this split is the whole reason a day's free allowance
    // now covers a workflow it previously could not.
    let mut ask: Vec<Value> = Vec::new();
    for u in &uploads {
        let Some(song_id) = u["song_id"].as_str() else { continue };
        if !regenerate && !only_empty { continue; }
        let needs_desc = regenerate || u["description"].as_str().map_or(true, |s| s.is_empty());
        let needs_tags = regenerate || u["tags"].as_array().map_or(true, |a| a.is_empty());
        if !needs_desc && !needs_tags { continue; }

        let song = state.db.collection::<Document>("songs")
            .find_one(doc! { "id": song_id }).await.map_err(e)?
            .map(bson_to_value).unwrap_or_default();
        let ch = if let Some(cid) = u["channel_id"].as_str() {
            state.db.collection::<Document>("channels")
                .find_one(doc! { "id": cid }).await.map_err(e)?
                .map(bson_to_value).unwrap_or_default()
        } else { Value::default() };

        ask.push(serde_json::json!({
            "id": u["id"].as_str().unwrap_or(""),
            "title": song["title"].as_str().unwrap_or(""),
            "language": song["language"].as_str().unwrap_or("English"),
            "style": song["styles"].as_str().unwrap_or(""),
            "channel": ch["name"].as_str().unwrap_or(""),
            "lyrics": song["lyrics"].as_str().unwrap_or("").chars().take(300).collect::<String>(),
        }));
    }
    let enriched = enrich_batch(&state.db, &ask, &global_desc).await;

    // ── Pass two: write it, falling back per upload for anything the batch did not answer ─────
    let mut updated = 0usize;
    for u in &uploads {
        let Some(song_id) = u["song_id"].as_str() else { continue; };
        let song = state.db.collection::<Document>("songs")
            .find_one(doc! { "id": song_id }).await.map_err(e)?
            .map(bson_to_value).unwrap_or_default();
        // The channel document and the lyric excerpt used to be read here for a per-upload AI call.
        // Batching moved both into pass one (`ask`), and this pass now only writes what came back —
        // so re-reading the channel was a database lookup per upload whose result went nowhere.

        let lang  = song["language"].as_str().unwrap_or("English").to_string();
        let style = song["styles"].as_str().unwrap_or("").to_string();

        if !regenerate && !only_empty { continue; }
        let need_title = regenerate || u["title"].as_str().map_or(true, |s| s.is_empty());
        let need_desc  = regenerate || u["description"].as_str().map_or(true, |s| s.is_empty());
        let need_tags  = regenerate || u["tags"].as_array().map_or(true, |a| a.is_empty());

        let mut chunk = serde_json::Map::new();

        if need_title {
            chunk.insert("title".into(), song["title"].clone());
        }
        if need_desc {
            // From the one batched request, not a request of its own.
            let ai_desc = enriched
                .get(u["id"].as_str().unwrap_or(""))
                .and_then(|v| v["description"].as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let desc = if ai_desc.is_empty() {
                format!("{}\n\nA {style} interpretation in {lang}.\n\n{global_desc}\n\n#AIMusicVideo #{lang} #{}", 
                    song["title"].as_str().unwrap_or(""), style.replace(' ', ""))
            } else {
                ai_desc
            };
            chunk.insert("description".into(), Value::String(desc));
        }
        if need_tags {
            // Likewise from the batch.
            let ai_tags: Vec<Value> = enriched
                .get(u["id"].as_str().unwrap_or(""))
                .and_then(|v| v["tags"].as_array().cloned())
                .unwrap_or_default();
            let tags: Vec<Value> = if !ai_tags.is_empty() {
                ai_tags
                    .into_iter()
                    .filter_map(|v| v.as_str().map(|s| Value::String(s.trim().trim_start_matches('#').to_string())))
                    .take(12)
                    .collect()
            } else {
                let mut t = vec![lang.to_lowercase(), "music video".into(), "ai music".into(), style.to_lowercase()];
                t.extend(style.to_lowercase().split_whitespace().filter(|w| w.len() > 2).map(|s| s.to_string()));
                t.into_iter().take(12).map(Value::String).collect()
            };
            chunk.insert("tags".into(), Value::Array(tags));
        }
        if u["privacy"].as_str().map_or(true, |s| s.is_empty()) {
            chunk.insert("privacy".into(), Value::String("public".into()));
        }

        if !chunk.is_empty() {
            let bson = bson::to_bson(&Value::Object(chunk)).map_err(e)?;
            state.db.collection::<Document>("uploads")
                .update_one(doc! { "id": u["id"].as_str().unwrap_or("") }, doc! { "$set": bson })
                .await.map_err(e)?;
            updated += 1;
        }
    }
    Ok(serde_json::json!({ "updated": updated, "total_pending": uploads.len() }))
}

/// How many uploads go into one enrichment request.
///
/// Eight rather than "all of them": a prompt carrying forty songs invites a truncated answer, and a
/// truncated answer loses the whole batch rather than one entry. Eight keeps the reply comfortably
/// inside a default token budget while still turning what used to be sixteen requests into one.
const ENRICH_BATCH: usize = 8;

/// Description and tags for a batch of uploads, in one request instead of two per upload.
///
/// This function is the difference between "a full workflow fits in a day's free allowance" and "it
/// does not". The loop it replaces called the AI twice for every pending upload — once for the
/// description, once for the tags — so ten uploads spent twenty of OpenRouter's fifty daily
/// requests on metadata alone, and a batch of forty could not complete at all.
///
/// Returns a map from upload id to whatever the model produced. A missing entry is not an error:
/// the caller keeps its own non-AI fallback for anything not answered, so a partial reply degrades
/// to the previous behaviour for the stragglers rather than failing the batch.
async fn enrich_batch(db: &crate::store::Db, items: &[Value], global_desc: &str) -> serde_json::Map<String, Value> {
    let mut out = serde_json::Map::new();
    if items.is_empty() { return out; }

    for chunk in items.chunks(ENRICH_BATCH) {
        let list = Value::Array(chunk.to_vec());
        let reply = qwen_text(
            db,
            "You write YouTube metadata for music videos. You are given a JSON array of songs. \
             Reply with ONLY a JSON array, same length and same order, each element \
             {\"id\": \"<the id you were given>\", \"description\": \"...\", \"tags\": [\"...\"]}. \
             Each description keeps the source tone, names the song title near the top, is written \
             in that entry's language, and ends with up to 5 relevant hashtags. Each tags array has \
             8-12 short lowercase tags with no leading '#'. No preamble, no markdown.",
            &format!(
                "Source description (English), to adapt for every entry:\n{global_desc}\n\nSongs:\n{}",
                serde_json::to_string(&list).unwrap_or_default()
            ),
        ).await;

        if reply.trim().is_empty() { continue; }
        // The model is asked for a bare array; some wrap it in prose anyway, so take the first array
        // in the reply rather than trusting the whole string to parse.
        let Some(found) = Regex::new(r"\[[\s\S]*\]").ok().and_then(|re| re.find(&reply).map(|m| m.as_str().to_string())) else { continue };
        let Ok(parsed) = serde_json::from_str::<Vec<Value>>(&found) else { continue };

        for entry in parsed {
            // Keyed by the id the model was given, not by position: an answer that drops or reorders
            // an entry would otherwise write one song's description onto another's upload, which is
            // worse than no enrichment at all and would not look like a bug until it was published.
            let Some(id) = entry["id"].as_str().filter(|s| !s.is_empty()) else { continue };
            if !chunk.iter().any(|c| c["id"].as_str() == Some(id)) { continue; }
            out.insert(id.to_string(), entry);
        }
    }
    out
}

// ────────────────────────────────────────────────────────────────
// Internal: single AI text call for upload metadata
//
// Routed through the shared `provider_chat` so it honours the configured provider, the timeout and
// token knobs, and the automatic fallback when the primary is overloaded. Metadata polish is
// best-effort — an empty string means "keep what the caller already had".
// ────────────────────────────────────────────────────────────────

async fn qwen_text(db: &crate::store::Db, system: &str, user: &str) -> String {
    let s = db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.ok().flatten()
        .map(|d| {
            let mut m = serde_json::Map::new();
            for (k, v) in d {
                if k == "_id" { continue; }
                if let Ok(jv) = bson::from_bson::<Value>(v) { m.insert(k, jv); }
            }
            Value::Object(m)
        })
        .unwrap_or_default();

    match crate::commands::ai::provider_chat(&s, system, user, 0.7, false).await {
        Ok((text, _model)) => text.trim().to_string(),
        Err(err) => { eprintln!("[uploads] metadata AI call failed: {err}"); String::new() }
    }
}
