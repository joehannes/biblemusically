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

    let mut updated = 0usize;
    for u in &uploads {
        let Some(song_id) = u["song_id"].as_str() else { continue; };
        let song = state.db.collection::<Document>("songs")
            .find_one(doc! { "id": song_id }).await.map_err(e)?
            .map(bson_to_value).unwrap_or_default();
        let ch = if let Some(cid) = u["channel_id"].as_str() {
            state.db.collection::<Document>("channels")
                .find_one(doc! { "id": cid })
                .await
                .map_err(e)?
                .map(bson_to_value)
                .unwrap_or_default()
        } else {
            Value::default()
        };

        let lang  = song["language"].as_str().unwrap_or("English").to_string();
        let style = song["styles"].as_str().unwrap_or("").to_string();
        let lyrics = song["lyrics"].as_str().unwrap_or("").chars().take(400).collect::<String>();

        if !regenerate && !only_empty { continue; }
        let need_title = regenerate || u["title"].as_str().map_or(true, |s| s.is_empty());
        let need_desc  = regenerate || u["description"].as_str().map_or(true, |s| s.is_empty());
        let need_tags  = regenerate || u["tags"].as_array().map_or(true, |a| a.is_empty());

        let mut chunk = serde_json::Map::new();

        if need_title {
            chunk.insert("title".into(), song["title"].clone());
        }
        if need_desc {
            let ai_desc = qwen_text(
                &state.db,
                "You adapt YouTube video descriptions for different languages and music styles. Reply ONLY with the adapted description text — no preamble, no markdown.",
                &format!("Source description (English):\n{global_desc}\n\nAdapt for: language={lang}, style='{style}', channel='{}'. Keep tone, include the song title '{}' near top, end with up to 5 relevant hashtags.",
                    ch["name"].as_str().unwrap_or(""),
                    song["title"].as_str().unwrap_or("")),
            ).await;
            let desc = if ai_desc.is_empty() {
                format!("{}\n\nA {style} interpretation in {lang}.\n\n{global_desc}\n\n#AIMusicVideo #{lang} #{}", 
                    song["title"].as_str().unwrap_or(""), style.replace(' ', ""))
            } else {
                ai_desc
            };
            chunk.insert("description".into(), Value::String(desc));
        }
        if need_tags {
            let ai_tags = qwen_text(
                &state.db,
                "You generate a JSON array of 8-12 short, lowercase, hyphen-free YouTube tags (no leading #). Reply ONLY with the JSON array.",
                &format!("Music style: {style}\nLanguage: {lang}\nTitle: {}\nLyrics snippet: {lyrics}", song["title"].as_str().unwrap_or("")),
            ).await;
            let tags: Vec<Value> = if !ai_tags.is_empty() {
                let re = Regex::new(r"\[[\s\S]*\]").unwrap();
                re.find(&ai_tags)
                    .and_then(|m| serde_json::from_str::<Vec<Value>>(m.as_str()).ok())
                    .unwrap_or_default()
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

// ────────────────────────────────────────────────────────────────
// Internal: single Qwen call via OpenRouter
// ────────────────────────────────────────────────────────────────

async fn qwen_text(db: &mongodb::Database, system: &str, user: &str) -> String {
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

    let key = s["openrouter_api_key"].as_str().unwrap_or("").trim().to_string();
    if key.is_empty() { return String::new(); }
    let model = s["openrouter_model"].as_str().unwrap_or("qwen/qwen-2.5-72b-instruct:free").to_string();

    let client = reqwest::Client::new();
    let res = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .bearer_auth(&key)
        .header("HTTP-Referer", "https://lightkid.studio")
        .header("X-Title", "Lightkid AI Studio")
        .json(&serde_json::json!({
            "model": model,
            "temperature": 0.7,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user",   "content": user   },
            ]
        }))
        .send().await;
    match res {
        Ok(r) if r.status().is_success() => {
            r.json::<Value>().await.ok()
                .and_then(|v| v["choices"][0]["message"]["content"].as_str().map(|s| s.trim().to_string()))
                .unwrap_or_default()
        }
        _ => String::new(),
    }
}
