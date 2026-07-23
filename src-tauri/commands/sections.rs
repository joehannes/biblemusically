use crate::{
    helpers::EFFECT_PRESETS,
    jobs::enqueue,
    state::AppState,
};
use bson::{doc, Document};
use serde_json::Value;
use std::sync::Arc;
use tauri::State;

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
pub async fn list_sections(state: State<'_, AppState>, sid: String) -> Res<Vec<Value>> {
    use futures_util::StreamExt;
    let mut cursor = state.db.collection::<Document>("sections")
        .find(doc! { "song_id": &sid }).sort(doc! { "index": 1 })
        .await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }
    Ok(out)
}

/// Bulk-set the per-section transition (used by the Transitions editor / AI suggester).
/// `updates` is a list of `{ id, transition }`.
#[tauri::command]
pub async fn set_section_transitions(state: State<'_, AppState>, updates: Vec<Value>) -> Res<Value> {
    let coll = state.db.collection::<Document>("sections");
    let mut n = 0u64;
    for u in &updates {
        if let (Some(id), Some(tr)) = (u["id"].as_str(), u["transition"].as_str()) {
            let r = coll.update_one(doc! { "id": id }, doc! { "$set": { "transition": tr } }).await.map_err(e)?;
            n += r.modified_count;
        }
    }
    Ok(serde_json::json!({ "ok": true, "updated": n }))
}

#[tauri::command]
pub async fn update_section(
    state: State<'_, AppState>,
    secid: String,
    mut body: Value,
) -> Res<Value> {
    if let Some(obj) = body.as_object_mut() {
        obj.remove("id");
        obj.remove("_id");
    }
    let bson = bson::to_bson(&body).map_err(e)?;
    state.db.collection::<Document>("sections")
        .update_one(doc! { "id": &secid }, doc! { "$set": bson })
        .await.map_err(e)?;
    let doc = state.db.collection::<Document>("sections")
        .find_one(doc! { "id": &secid }).await.map_err(e)?
        .ok_or_else(|| "not found".to_string())?;
    Ok(bson_to_value(doc))
}

#[tauri::command]
pub async fn generate_section_image(
    state: State<'_, AppState>,
    state_arc: State<'_, Arc<AppState>>,
    secid: String,
) -> Res<Value> {
    state.db.collection::<Document>("sections")
        .find_one(doc! { "id": &secid }).await.map_err(e)?
        .ok_or_else(|| "section missing".to_string())?;
    let job = enqueue("image", &secid, &state_arc).await.map_err(e)?;
    Ok(serde_json::to_value(job).map_err(e)?)
}

#[tauri::command]
pub async fn batch_generate_images(
    state: State<'_, AppState>,
    state_arc: State<'_, Arc<AppState>>,
    sid: String,
) -> Res<Value> {
    use futures_util::StreamExt;
    let mut cursor = state.db.collection::<Document>("sections")
        .find(doc! { "song_id": &sid }).await.map_err(e)?;
    let mut sec_ids = Vec::new();
    while let Some(Ok(d)) = cursor.next().await {
        if let Some(id) = d.get_str("id").ok() { sec_ids.push(id.to_string()); }
    }
    let mut jobs = Vec::new();
    for sec_id in &sec_ids {
        let job = enqueue("image", sec_id, &state_arc).await.map_err(e)?;
        jobs.push(serde_json::to_value(job).map_err(e)?);
    }
    Ok(serde_json::json!({ "queued": jobs.len(), "jobs": jobs }))
}


/// Queues a Midjourney image job for every section, optionally scoped to one project.
/// Renamed from `bulk_generate_all_songs` (which was misleading — it generates *images*, not
/// music) and now takes an optional `project_id` so the Settings-page "Generate All Images"
/// button doesn't silently reach across every project in the database. Passing no project_id
/// preserves the old unscoped behavior for any caller that genuinely wants everything.
#[tauri::command]
pub async fn bulk_generate_all_images(
    state: State<'_, AppState>,
    state_arc: State<'_, Arc<AppState>>,
    project_id: Option<String>,
) -> Res<Value> {
    use futures_util::StreamExt;
    // Ensure Playwright profile configured
    let sdoc = state.db.collection::<Document>("settings").find_one(doc! { "_id": "singleton" }).await.map_err(e)?.ok_or_else(|| "settings not found".to_string())?;
    let s = bson_to_value(sdoc);
    let profile = s["mj_profile_dir"].as_str().unwrap_or("").trim().to_string();
    if profile.is_empty() {
        return Err("mj_profile_dir is not configured. Capture a Playwright profile first.".to_string());
    }

    let section_filter = if let Some(pid) = project_id.as_deref().filter(|p| !p.is_empty()) {
        let mut song_cursor = state.db.collection::<Document>("songs")
            .find(doc! { "project_id": pid }).await.map_err(e)?;
        let mut song_ids = Vec::new();
        while let Some(Ok(d)) = song_cursor.next().await {
            if let Ok(sid) = d.get_str("id") { song_ids.push(sid.to_string()); }
        }
        doc! { "song_id": { "$in": song_ids } }
    } else {
        doc! {}
    };

    let mut cursor = state.db.collection::<Document>("sections").find(section_filter).await.map_err(e)?;
    let mut jobs = Vec::new();
    while let Some(Ok(d)) = cursor.next().await {
        if let Ok(sec_id) = d.get_str("id") {
            if let Ok(job) = enqueue("image", sec_id, &state_arc).await {
                jobs.push(serde_json::to_value(job).map_err(e)?);
            }
        }
    }
    Ok(serde_json::json!({ "queued": jobs.len(), "jobs": jobs }))
}

#[tauri::command]
pub async fn get_effects_presets() -> Res<Value> {
    Ok(serde_json::to_value(EFFECT_PRESETS.iter().collect::<Vec<_>>()).map_err(e)?)
}
