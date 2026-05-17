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

#[tauri::command]
pub async fn get_effects_presets() -> Res<Value> {
    Ok(serde_json::to_value(EFFECT_PRESETS.iter().collect::<Vec<_>>()).map_err(e)?)
}
