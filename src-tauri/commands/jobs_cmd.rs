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
    // Clear any stale cancellation flag: a job can be cancelled and then retried before the
    // in-flight task noticed the flag, in which case the retried run must not immediately be
    // treated as cancelled too.
    state.cancelled_jobs.lock().await.remove(&jid);
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
    let semaphore = Arc::clone(&state.job_semaphore);
    tokio::spawn(async move {
        let _permit = semaphore.acquire_owned().await;
        run_job(&jid_clone, &arc).await;
    });
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn cancel_job(state: State<'_, AppState>, jid: String) -> Res<Value> {
    // Signal the running task (checked in jobs::run_job and inside the Suno/YouTube-upload
    // polling loops) instead of deleting the document — deleting let the spawned tokio task
    // keep running invisibly in the background (still polling Suno / uploading to YouTube)
    // with no way for the user to see it was still active, and its eventual DB writes would
    // silently target a document that no longer existed.
    state.cancelled_jobs.lock().await.insert(jid.clone());
    let ts = now_iso();
    state.db.collection::<Document>("jobs")
        .update_one(
            doc! { "id": &jid },
            doc! {
                "$set": { "status": "cancelled", "updated_at": &ts },
                "$push": { "logs": format!("[{ts}] cancellation requested by user") },
            },
        )
        .await.map_err(e)?;
    Ok(serde_json::json!({ "ok": true }))
}
