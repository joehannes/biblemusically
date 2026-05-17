use crate::{models::Settings, state::AppState};
use bson::{doc, Document};
use mongodb::options::UpdateOptions;
use serde_json::Value;
use tauri::State;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

fn proj0() -> mongodb::options::FindOneOptions {
    mongodb::options::FindOneOptions::builder().projection(doc! { "_id": 0 }).build()
}

fn bson_to_value(doc: Document) -> Value {
    let mut m = serde_json::Map::new();
    for (k, v) in doc {
        if k == "_id" { continue; }
        if let Ok(jv) = bson::from_bson::<Value>(v) { m.insert(k, jv); }
    }
    Value::Object(m)
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Res<Value> {
    let doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" })
        .with_options(proj0())
        .await.map_err(e)?;
    Ok(doc.map(bson_to_value).unwrap_or_else(|| {
        serde_json::to_value(Settings::default()).unwrap()
    }))
}

#[tauri::command]
pub async fn update_settings(state: State<'_, AppState>, payload: Settings) -> Res<Value> {
    let bson = bson::to_document(&payload).map_err(e)?;
    state.db.collection::<Document>("settings")
        .update_one(doc! { "_id": "singleton" }, doc! { "$set": &bson })
        .with_options(UpdateOptions::builder().upsert(true).build())
        .await.map_err(e)?;
    Ok(serde_json::to_value(payload).map_err(e)?)
}

#[tauri::command]
pub async fn test_suno(state: State<'_, AppState>) -> Res<Value> {
    let doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let ok = doc["suno_cookie"].as_str().map_or(false, |s| !s.is_empty());
    Ok(serde_json::json!({
        "ok": ok,
        "detail": if ok { "Cookie present" } else { "No cookie configured. Paste a fresh studio-api.suno.com session cookie." }
    }))
}

#[tauri::command]
pub async fn test_mj(state: State<'_, AppState>) -> Res<Value> {
    let doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let ok = doc["mj_cookie"].as_str().map_or(false, |s| !s.is_empty())
        || doc["mj_discord_token"].as_str().map_or(false, |s| !s.is_empty());
    Ok(serde_json::json!({
        "ok": ok,
        "detail": if ok { "Connected" } else { "Provide either MJ cookie or Discord wrapper token." }
    }))
}

#[tauri::command]
pub async fn test_ffmpeg(state: State<'_, AppState>) -> Res<Value> {
    let doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let path = doc["ffmpeg_path"].as_str().unwrap_or("ffmpeg").to_string();
    let found = which::which(&path).ok().map(|p| p.to_string_lossy().to_string());
    Ok(serde_json::json!({ "ok": found.is_some(), "path": found.unwrap_or(path) }))
}
