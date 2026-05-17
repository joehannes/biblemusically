use crate::{jobs::run_job, models::now_iso, state::AppState};
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
pub async fn list_jobs(state: State<'_, AppState>, limit: Option<i64>) -> Res<Vec<Value>> {
    use futures_util::StreamExt;
    let limit = limit.unwrap_or(200);
    let mut cursor = state.db.collection::<Document>("jobs")
        .find(doc! {}).sort(doc! { "created_at": -1 }).limit(limit)
        .await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }
    Ok(out)
}

#[tauri::command]
pub async fn get_job(state: State<'_, AppState>, jid: String) -> Res<Value> {
    let doc = state.db.collection::<Document>("jobs")
        .find_one(doc! { "id": &jid }).await.map_err(e)?
        .ok_or_else(|| "missing".to_string())?;
    Ok(bson_to_value(doc))
}

#[tauri::command]
pub async fn retry_job(
    state: State<'_, AppState>,
    state_arc: State<'_, Arc<AppState>>,
    jid: String,
) -> Res<Value> {
    state.db.collection::<Document>("jobs")
        .find_one(doc! { "id": &jid }).await.map_err(e)?
        .ok_or_else(|| "missing".to_string())?;
    let ts = now_iso();
    state.db.collection::<Document>("jobs")
        .update_one(
            doc! { "id": &jid },
            doc! {
                "$set": { "status": "queued", "progress": 0, "error": bson::Bson::Null, "updated_at": &ts },
                "$inc": { "attempts": 1 },
                "$push": { "logs": format!("[{ts}] retry requested") },
            },
        )
        .await.map_err(e)?;
    let jid_clone = jid.clone();
    let arc = Arc::clone(&*state_arc);
    tokio::spawn(async move { run_job(&jid_clone, &arc).await; });
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn cancel_job(state: State<'_, AppState>, jid: String) -> Res<Value> {
    state.db.collection::<Document>("jobs")
        .delete_one(doc! { "id": &jid }).await.map_err(e)?;
    Ok(serde_json::json!({ "ok": true }))
}
