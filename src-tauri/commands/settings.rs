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
    
    let proxy = doc["mj_proxy_url"].as_str().unwrap_or("").trim();
    let token = doc["mj_discord_token"].as_str().unwrap_or("").trim();
    let cookie = doc["mj_cookie"].as_str().unwrap_or("").trim();
    
    if proxy.is_empty() && token.is_empty() && cookie.is_empty() {
        return Ok(serde_json::json!({
            "ok": false,
            "status": "not_configured",
            "detail": "Provide either MJ proxy URL or Discord wrapper token"
        }));
    }
    
    // Test actual connectivity to proxy if available
    if !proxy.is_empty() {
        let client = reqwest::Client::new();
        let mut req = client.get(format!("{}/info", proxy.trim_end_matches('/')))
            .timeout(std::time::Duration::from_secs(10));
        if !token.is_empty() {
            req = req.bearer_auth(token);
        }
        
        match req.send().await {
            Ok(res) => {
                if res.status().is_success() {
                    return Ok(serde_json::json!({
                        "ok": true,
                        "status": "connected",
                        "detail": "Midjourney proxy reachable and responsive"
                    }));
                } else {
                    return Ok(serde_json::json!({
                        "ok": false,
                        "status": "auth_failed",
                        "detail": format!("Proxy returned HTTP {}: check token or credentials", res.status())
                    }));
                }
            }
            Err(err) => {
                let detail = if err.is_timeout() {
                    format!("Connection timeout ({}s) - proxy unreachable or too slow", 10)
                } else if err.is_connect() {
                    format!("Cannot connect to proxy: {}", err)
                } else {
                    format!("Connection error: {}", err)
                };
                return Ok(serde_json::json!({
                    "ok": false,
                    "status": "connection_error",
                    "detail": detail
                }));
            }
        }
    }
    
    // If only cookie/token present but no proxy configured, warn about missing proxy
    Ok(serde_json::json!({
        "ok": true,
        "status": "partial_config",
        "detail": "Credentials present but no proxy URL. Configure MJ proxy URL for actual image generation"
    }))
}

#[tauri::command]
pub async fn test_ffmpeg(state: State<'_, AppState>) -> Res<Value> {
    let doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let path = doc["ffmpeg_path"].as_str().unwrap_or("ffmpeg").to_string();
    // Prefer configured path, then system which, then bundled resource
    let mut resolved: Option<String> = which::which(&path).ok().map(|p| p.to_string_lossy().to_string());
    if resolved.is_none() {
        if let Some(res_dir) = tauri::api::path::resource_dir() {
            let candidates = [
                res_dir.join("ffmpeg"),
                res_dir.join("ffmpeg.exe"),
                res_dir.join("bin").join("ffmpeg"),
                res_dir.join("bin").join("ffmpeg.exe"),
            ];
            for c in &candidates {
                if c.exists() && c.is_file() {
                    resolved = Some(c.to_string_lossy().to_string());
                    break;
                }
            }
        }
    }
    Ok(serde_json::json!({ "ok": resolved.is_some(), "path": resolved.unwrap_or(path) }))
}
