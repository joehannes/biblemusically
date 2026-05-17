use crate::{jobs::enqueue, state::AppState};
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
pub async fn list_songs(state: State<'_, AppState>, pid: String) -> Res<Vec<Value>> {
    use futures_util::StreamExt;
    let mut cursor = state.db.collection::<Document>("songs")
        .find(doc! { "project_id": &pid }).await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }
    Ok(out)
}

#[tauri::command]
pub async fn get_song(state: State<'_, AppState>, sid: String) -> Res<Value> {
    let doc = state.db.collection::<Document>("songs")
        .find_one(doc! { "id": &sid }).await.map_err(e)?
        .ok_or_else(|| "not found".to_string())?;
    Ok(bson_to_value(doc))
}

#[tauri::command]
pub async fn delete_song(state: State<'_, AppState>, sid: String) -> Res<Value> {
    state.db.collection::<Document>("sections")
        .delete_many(doc! { "song_id": &sid }).await.map_err(e)?;
    state.db.collection::<Document>("songs")
        .delete_one(doc! { "id": &sid }).await.map_err(e)?;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn generate_music(
    state: State<'_, AppState>,
    state_arc: State<'_, Arc<AppState>>,
    sid: String,
) -> Res<Value> {
    state.db.collection::<Document>("songs")
        .find_one(doc! { "id": &sid }).await.map_err(e)?
        .ok_or_else(|| "song missing".to_string())?;
    let job = enqueue("music", &sid, &state_arc).await.map_err(e)?;
    Ok(serde_json::to_value(job).map_err(e)?)
}

#[tauri::command]
pub async fn analyze_song(
    state: State<'_, AppState>,
    state_arc: State<'_, Arc<AppState>>,
    sid: String,
) -> Res<Value> {
    state.db.collection::<Document>("songs")
        .find_one(doc! { "id": &sid }).await.map_err(e)?
        .ok_or_else(|| "song missing".to_string())?;
    let job = enqueue("analysis", &sid, &state_arc).await.map_err(e)?;
    Ok(serde_json::to_value(job).map_err(e)?)
}

#[tauri::command]
pub async fn compose_video(
    state: State<'_, AppState>,
    state_arc: State<'_, Arc<AppState>>,
    sid: String,
) -> Res<Value> {
    state.db.collection::<Document>("songs")
        .find_one(doc! { "id": &sid }).await.map_err(e)?
        .ok_or_else(|| "song missing".to_string())?;
    let job = enqueue("video", &sid, &state_arc).await.map_err(e)?;
    Ok(serde_json::to_value(job).map_err(e)?)
}
