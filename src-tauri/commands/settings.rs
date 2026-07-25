use crate::{helpers::{resolve_node_executable, locate_resource_file}, models::Settings, state::AppState};
use bson::{doc, Document};
use serde_json::Value;
use std::env;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
use tokio::fs;
use tokio::sync::mpsc;
use tokio::time::Duration;
use uuid::Uuid;
use warp::Filter;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

fn proj0() -> crate::store::FindOneOptions {
    crate::store::FindOneOptions::builder().projection(doc! { "_id": 0 }).build()
}

fn bson_to_value(doc: Document) -> Value {
    let mut m = serde_json::Map::new();
    for (k, v) in doc {
        if k == "_id" { continue; }
        if let Ok(jv) = bson::from_bson::<Value>(v) { m.insert(k, jv); }
    }
    Value::Object(m)
}


async fn probe_midjourney_proxy() -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .ok()?;
    for port in [8080u16, 8086u16, 8081u16, 8085u16] {
        let url = format!("http://127.0.0.1:{}", port);
        if let Ok(res) = client.get(format!("{}/info", url.trim_end_matches('/'))).send().await {
            if res.status().is_success() {
                return Some(url);
            }
        }
    }
    None
}

async fn get_settings_doc(db: &crate::store::Db) -> Result<Value, String> {
    let doc = db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    Ok(doc)
}

fn normalize_suno_cookie(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let raw = if raw.to_ascii_lowercase().starts_with("cookie:") {
        raw[7..].trim()
    } else {
        raw
    };
    let cookie = raw
        .split(';')
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    let cookie = cookie.trim();
    if cookie.is_empty() {
        return None;
    }
    if !cookie.contains('=') {
        return Some(format!("studio-api_key={cookie}"));
    }
    Some(cookie.to_string())
}

pub async fn validate_suno_cookie_internal(db: &crate::store::Db) -> Result<(), String> {
    let s = get_settings_doc(db).await?;
    let cookie_env = std::env::var("SUNO_COOKIE").ok();
    let raw_cookie = cookie_env.as_deref().unwrap_or_else(|| s["suno_cookie"].as_str().unwrap_or(""));
    let cookie = match normalize_suno_cookie(raw_cookie) {
        Some(c) => c,
        None => {
            let _ = db.collection::<Document>("settings")
                .update_one(doc! { "_id": "singleton" }, doc! { "$set": { "suno_cookie_valid": false, "suno_cookie_status": "not_configured", "suno_cookie_checked_at": chrono::Utc::now().to_rfc3339() } })
                .await;
            return Err("Suno cookie not configured".into());
        }
    };
    let client = reqwest::Client::new();
    let res = client
        .get("https://studio-api.suno.com/api/user/")
        .header("Cookie", cookie)
        .header("Accept", "application/json, text/plain, */*")
        .header("User-Agent", "Mozilla/5.0")
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|err| format!("Suno validation failed: {}", err))?;
    if res.status() == 200 {
        // Persist valid status
        let _ = db.collection::<Document>("settings")
            .update_one(doc! { "_id": "singleton" }, doc! { "$set": { "suno_cookie_valid": true, "suno_cookie_status": "valid", "suno_cookie_checked_at": chrono::Utc::now().to_rfc3339() } })
            .await;
        Ok(())
    } else {
        let status_str = if res.status() == 401 || res.status() == 403 { "expired" } else { "api_error" };
        // Persist invalid status
        let _ = db.collection::<Document>("settings")
            .update_one(doc! { "_id": "singleton" }, doc! { "$set": { "suno_cookie_valid": false, "suno_cookie_status": status_str, "suno_cookie_checked_at": chrono::Utc::now().to_rfc3339() } })
            .await;
        Err(format!("Suno validation failed: HTTP {}", res.status()))
    }
}

pub async fn validate_mj_token_internal(db: &crate::store::Db) -> Result<(), String> {
    let s = get_settings_doc(db).await?;
    // Prefer Playwright profile for authentication; if present and exists, consider valid
    let profile_env = std::env::var("MJ_PROFILE_DIR").ok();
    let profile_dir = profile_env.as_deref().unwrap_or_else(|| s["mj_profile_dir"].as_str().unwrap_or("")).trim();
    if !profile_dir.is_empty() {
        if std::path::Path::new(profile_dir).exists() {
            let _ = db.collection::<Document>("settings")
                .update_one(doc! { "_id": "singleton" }, doc! { "$set": { "mj_profile_valid": true, "mj_profile_checked_at": chrono::Utc::now().to_rfc3339() } })
                .await;
            return Ok(());
        } else {
            return Err("MJ Playwright profile directory not found".into());
        }
    }

    let proxy_env = std::env::var("MJ_PROXY_URL").ok();
    let proxy = proxy_env.as_deref().unwrap_or_else(|| s["mj_proxy_url"].as_str().unwrap_or("")).trim();
    let token_env = std::env::var("MJ_DISCORD_TOKEN").ok();
    let token = token_env.as_deref().unwrap_or_else(|| s["mj_discord_token"].as_str().unwrap_or("")).trim();
    if proxy.is_empty() || token.is_empty() {
        return Err("MJ proxy URL or Discord token missing".into());
    }
    let client = reqwest::Client::new();
    let res = client.get(format!("{}/info", proxy.trim_end_matches('/')))
        .bearer_auth(token)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|err| format!("MJ validation failed: {}", err))?;
    if res.status().is_success() {
        Ok(())
    } else {
        Err(format!("MJ validation failed: HTTP {}", res.status()))
    }
}

pub async fn validate_google_refresh_tokens_internal(db: &crate::store::Db) -> Result<Vec<String>, String> {
    use futures_util::StreamExt;
    let mut cursor = db.collection::<Document>("channels").find(doc! {}).await.map_err(e)?;
    let mut invalidated = Vec::new();
    while let Some(Ok(doc)) = cursor.next().await {
        let ch = bson_to_value(doc);
        let connected = ch["connected"].as_bool().unwrap_or(false);
        let refresh_token = ch["refresh_token"].as_str().unwrap_or("").trim().to_string();
        if !connected || refresh_token.is_empty() {
            continue;
        }
        let client = crate::jobs::pick_oauth_client(&db, &ch, ch["oauth_client_id"].as_str()).await;
        let client = match client {
            Some(c) => c,
            None => continue,
        };
        let cid = client["client_id"].as_str().unwrap_or("").to_string();
        let csec = client["client_secret"].as_str().unwrap_or("").to_string();
        if cid.is_empty() || csec.is_empty() {
            continue;
        }
        let http = reqwest::Client::new();
        let resp = http.post("https://oauth2.googleapis.com/token")
            .form(&[
                ("client_id", cid.as_str()),
                ("client_secret", csec.as_str()),
                ("refresh_token", refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .timeout(Duration::from_secs(10))
            .send()
            .await;
        match resp {
            Ok(res) => {
                if res.status().is_success() {
                    // Refresh token is valid.
                    continue;
                }
                if let Ok(body) = res.json::<Value>().await {
                    if body["error"].as_str().unwrap_or("") == "invalid_grant" {
                        let channel_id = ch["id"].as_str().unwrap_or("").to_string();
                        let _ = db.collection::<Document>("channels")
                            .update_one(doc! { "id": &channel_id }, doc! { "$set": { "connected": false } })
                            .await;
                        invalidated.push(channel_id);
                    }
                }
            }
            Err(err) => {
                eprintln!("Google refresh check failed for channel {}: {}", ch["id"].as_str().unwrap_or(""), err);
            }
        }
    }
    Ok(invalidated)
}

pub async fn ensure_mj_autostart_internal(db: &crate::store::Db) -> Res<Value> {
    // Midjourney proxy autostart is deprecated. Use the visible Playwright
    // driven workflow and the Settings → Capture session flow to obtain a
    // Playwright profile directory (stored as `mj_profile_dir`). This function remains for API compatibility and no
    // service is installed.
    Ok(serde_json::json!({ "ok": true, "installed": false, "note": "midjourney-proxy autostart removed; use Playwright-based flow" }))
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>, project_id: Option<String>) -> Res<Value> {
    let coll = state.db.collection::<Document>("settings");
    // Base: the global singleton (what all backend consumers — AI, jobs, uploads — read).
    let mut merged = coll.find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value)
        .unwrap_or_else(|| serde_json::to_value(Settings::default()).unwrap());
    // Project-scoped values override the singleton, never replace it wholesale —
    // otherwise a stale project doc hides globally-saved keys.
    if let Some(pid) = project_id.filter(|s| !s.is_empty()) {
        if let Some(d) = coll.find_one(doc! { "_id": &pid }).await.map_err(e)? {
            if let (Some(base), Value::Object(over)) = (merged.as_object_mut(), bson_to_value(d)) {
                for (k, v) in over { base.insert(k, v); }
            }
        }
    }
    Ok(merged)
}

#[tauri::command]
pub async fn update_settings(state: State<'_, AppState>, payload: Value, project_id: Option<String>) -> Res<Value> {
    let coll = state.db.collection::<Document>("settings");
    // Always write through to the singleton: every backend consumer (compose_lyrics,
    // job runners, uploads, test_* probes) reads `_id: "singleton"` directly, so a
    // project-only save would leave e.g. freshly-entered API keys invisible to them.
    let mut bson = bson::to_document(&payload).map_err(e)?;
    bson.insert("_id", "singleton");
    coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": &bson })
        .upsert(true)
        .await.map_err(e)?;
    // Additionally keep the per-project override doc (get_settings merges it on top).
    if let Some(pid) = project_id.filter(|s| !s.is_empty()) {
        let mut pdoc = bson::to_document(&payload).map_err(e)?;
        pdoc.insert("_id", &pid);
        coll.update_one(doc! { "_id": &pid }, doc! { "$set": &pdoc })
            .upsert(true)
            .await.map_err(e)?;
    }
    Ok(payload)
}

#[tauri::command]
pub async fn test_suno(state: State<'_, AppState>) -> Res<Value> {
    let doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let raw_cookie = doc["suno_cookie"].as_str().unwrap_or("");
    let cookie = match normalize_suno_cookie(raw_cookie) {
        Some(c) => c,
        None => {
            return Ok(serde_json::json!({
                "ok": false,
                "status": "not_configured",
                "detail": "Suno session cookie not configured.",
                "next_step": "1. Go to https://suno.com 2. Open DevTools (F12) 3. Cookies → suno.com → copy studio-api_key, studio-api_key_local, __session, or session_id 4. Paste the cookie string in Settings",
                "expires": "Cookie expires after ~24 hours of inactivity"
            }));
        }
    };
    
    // Test cookie validity with a lightweight API call
    let client = reqwest::Client::new();
    let test_res = client
        .get("https://studio-api.suno.com/api/user/")
        .header("Cookie", cookie)
        .header("Accept", "application/json, text/plain, */*")
        .header("User-Agent", "Mozilla/5.0")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;
    
    // Persist result to DB for frontend to read
    let coll = state.db.collection::<Document>("settings");
    let now_rfc = chrono::Utc::now().to_rfc3339();

    match test_res {
        Ok(res) => {
            if res.status() == 200 {
                let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "suno_cookie_valid": true, "suno_cookie_status": "valid", "suno_cookie_checked_at": &now_rfc } }).await;
                Ok(serde_json::json!({
                    "ok": true,
                    "status": "authenticated",
                    "detail": "Suno cookie is valid and working"
                }))
            } else if res.status() == 401 || res.status() == 403 {
                let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "suno_cookie_valid": false, "suno_cookie_status": "expired", "suno_cookie_checked_at": &now_rfc } }).await;
                Ok(serde_json::json!({
                    "ok": false,
                    "status": "cookie_invalid",
                    "detail": "Cookie is invalid, expired, or revoked.",
                    "next_step": "Get a fresh cookie from https://suno.com and update in Settings"
                }))
            } else {
                let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "suno_cookie_valid": false, "suno_cookie_status": "api_error", "suno_cookie_checked_at": &now_rfc } }).await;
                Ok(serde_json::json!({
                    "ok": false,
                    "status": "api_error",
                    "detail": format!("Suno API returned HTTP {} - service may be unavailable", res.status())
                }))
            }
        }
        Err(err) => {
            let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "suno_cookie_valid": false, "suno_cookie_status": "connection_error", "suno_cookie_checked_at": &now_rfc } }).await;
            let detail = if err.is_timeout() {
                "Connection timeout - Suno service unreachable".to_string()
            } else if err.is_connect() {
                "Cannot reach Suno service - check network connectivity".to_string()
            } else {
                format!("Connection error: {}", err)
            };
            Ok(serde_json::json!({
                "ok": false,
                "status": "connection_error",
                "detail": detail
            }))
        }
    }
}

#[tauri::command]
pub async fn test_acestep(state: State<'_, AppState>) -> Res<Value> {
    let doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let base = doc["acestep_api_url"].as_str().unwrap_or("").trim().trim_end_matches('/').to_string();
    if base.is_empty() {
        return Ok(serde_json::json!({
            "ok": false,
            "status": "not_configured",
            "detail": "ACE-Step API URL not configured.",
            "next_step": "Run the Kaggle/Colab ACE-Step notebook (or a local `acestep-api`), then paste its URL (e.g. https://xxxx.gradio.live or http://localhost:8001) into Settings."
        }));
    }
    let api_key = doc["acestep_api_key"].as_str().unwrap_or("").trim().to_string();

    // Reachability probe: an empty query_result call should return a valid JSON envelope
    // (code 200 / empty data) without generating anything.
    let client = reqwest::Client::new();
    let mut rb = client
        .post(format!("{}/query_result", base))
        .header("Accept", "application/json")
        .json(&serde_json::json!({ "task_id_list": [] }))
        .timeout(std::time::Duration::from_secs(15));
    if !api_key.is_empty() {
        rb = rb.header("Authorization", format!("Bearer {}", api_key));
    }

    let coll = state.db.collection::<Document>("settings");
    let now_rfc = chrono::Utc::now().to_rfc3339();

    match rb.send().await {
        Ok(res) if res.status().is_success() => {
            let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "acestep_valid": true, "acestep_status": "reachable", "acestep_checked_at": &now_rfc } }).await;
            Ok(serde_json::json!({
                "ok": true,
                "status": "reachable",
                "detail": format!("ACE-Step server responded at {}", base)
            }))
        }
        Ok(res) if res.status() == 401 || res.status() == 403 => {
            let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "acestep_valid": false, "acestep_status": "auth_error", "acestep_checked_at": &now_rfc } }).await;
            Ok(serde_json::json!({
                "ok": false,
                "status": "auth_error",
                "detail": "Server reachable but rejected the request — check the API key.",
                "next_step": "Set the same key the server was started with (ACESTEP_API_KEY), or clear it if the server has none."
            }))
        }
        Ok(res) => {
            let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "acestep_valid": false, "acestep_status": "http_error", "acestep_checked_at": &now_rfc } }).await;
            Ok(serde_json::json!({
                "ok": false,
                "status": "http_error",
                "detail": format!("Server returned HTTP {} — is this an ACE-Step REST server?", res.status())
            }))
        }
        Err(err) => {
            let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "acestep_valid": false, "acestep_status": "connection_error", "acestep_checked_at": &now_rfc } }).await;
            let detail = if err.is_timeout() {
                "Connection timeout — server unreachable (notebook stopped or share URL expired?)".to_string()
            } else if err.is_connect() {
                "Cannot connect — check the URL and that the notebook is still running".to_string()
            } else {
                format!("Connection error: {}", err)
            };
            Ok(serde_json::json!({ "ok": false, "status": "connection_error", "detail": detail }))
        }
    }
}

#[tauri::command]
pub async fn test_heartmula(state: State<'_, AppState>) -> Res<Value> {
    let doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let base = doc["heartmula_api_url"].as_str().unwrap_or("").trim().trim_end_matches('/').to_string();
    if base.is_empty() {
        return Ok(serde_json::json!({
            "ok": false, "status": "not_configured",
            "detail": "HeartMuLa server URL not configured.",
            "next_step": "Run the HeartMuLa Kaggle notebook, then paste its URL (e.g. https://xxxx.trycloudflare.com or http://localhost:8003)."
        }));
    }
    let api_key = doc["heartmula_api_key"].as_str().unwrap_or("").trim().to_string();
    let client = reqwest::Client::new();
    let mut rb = client.post(format!("{}/query_result", base))
        .header("Accept", "application/json")
        .json(&serde_json::json!({ "task_id_list": [] }))
        .timeout(std::time::Duration::from_secs(15));
    if !api_key.is_empty() { rb = rb.header("Authorization", format!("Bearer {}", api_key)); }
    let coll = state.db.collection::<Document>("settings");
    let now_rfc = chrono::Utc::now().to_rfc3339();
    match rb.send().await {
        Ok(res) if res.status().is_success() => {
            let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "heartmula_valid": true, "heartmula_status": "reachable", "heartmula_checked_at": &now_rfc } }).await;
            Ok(serde_json::json!({ "ok": true, "status": "reachable", "detail": format!("HeartMuLa server responded at {}", base) }))
        }
        Ok(res) if res.status() == 401 || res.status() == 403 => {
            Ok(serde_json::json!({ "ok": false, "status": "auth_error", "detail": "Server reachable but rejected the request — check the API key." }))
        }
        Ok(res) => Ok(serde_json::json!({ "ok": false, "status": "http_error", "detail": format!("Server returned HTTP {} — is the HeartMuLa notebook running?", res.status()) })),
        Err(err) => {
            let detail = if err.is_timeout() { "Connection timeout — server unreachable (notebook stopped or URL expired?)".to_string() }
                else if err.is_connect() { "Cannot connect — check the URL and that the notebook is running".to_string() }
                else { format!("Connection error: {}", err) };
            Ok(serde_json::json!({ "ok": false, "status": "connection_error", "detail": detail }))
        }
    }
}

#[tauri::command]
pub async fn test_flux(state: State<'_, AppState>) -> Res<Value> {
    let doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let base = doc["flux_api_url"].as_str().unwrap_or("").trim().trim_end_matches('/').to_string();
    if base.is_empty() {
        return Ok(serde_json::json!({
            "ok": false,
            "status": "not_configured",
            "detail": "FLUX image server URL not configured.",
            "next_step": "Run the Kaggle/Colab FLUX notebook (or a local server), then paste its URL (e.g. https://xxxx.trycloudflare.com or http://localhost:8002) into Settings."
        }));
    }
    let api_key = doc["flux_api_key"].as_str().unwrap_or("").trim().to_string();

    // Probe the server's /health endpoint.
    let client = reqwest::Client::new();
    let mut rb = client.get(format!("{}/health", base)).timeout(std::time::Duration::from_secs(15));
    if !api_key.is_empty() {
        rb = rb.header("Authorization", format!("Bearer {}", api_key));
    }

    let coll = state.db.collection::<Document>("settings");
    let now_rfc = chrono::Utc::now().to_rfc3339();

    match rb.send().await {
        Ok(res) if res.status().is_success() => {
            let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "flux_valid": true, "flux_status": "reachable", "flux_checked_at": &now_rfc } }).await;
            Ok(serde_json::json!({ "ok": true, "status": "reachable", "detail": format!("FLUX image server responded at {}", base) }))
        }
        Ok(res) if res.status() == 401 || res.status() == 403 => {
            let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "flux_valid": false, "flux_status": "auth_error", "flux_checked_at": &now_rfc } }).await;
            Ok(serde_json::json!({ "ok": false, "status": "auth_error", "detail": "Server reachable but rejected the request — check the API key." }))
        }
        Ok(res) => {
            let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "flux_valid": false, "flux_status": "http_error", "flux_checked_at": &now_rfc } }).await;
            Ok(serde_json::json!({ "ok": false, "status": "http_error", "detail": format!("Server returned HTTP {} — is the FLUX notebook running?", res.status()) }))
        }
        Err(err) => {
            let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "flux_valid": false, "flux_status": "connection_error", "flux_checked_at": &now_rfc } }).await;
            let detail = if err.is_timeout() {
                "Connection timeout — server unreachable (notebook stopped or share URL expired?)".to_string()
            } else if err.is_connect() {
                "Cannot connect — check the URL and that the notebook is still running".to_string()
            } else {
                format!("Connection error: {}", err)
            };
            Ok(serde_json::json!({ "ok": false, "status": "connection_error", "detail": detail }))
        }
    }
}

#[tauri::command]
pub async fn test_comfy(state: State<'_, AppState>) -> Res<Value> {
    let doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let base = doc["comfyui_api_url"].as_str().unwrap_or("").trim().trim_end_matches('/').to_string();
    if base.is_empty() {
        return Ok(serde_json::json!({
            "ok": false, "status": "not_configured",
            "detail": "ComfyUI server URL not configured.",
            "next_step": "Run the ComfyUI Kaggle notebook, then paste its URL (e.g. https://xxxx.trycloudflare.com or http://localhost:8188)."
        }));
    }
    let api_key = doc["comfyui_api_key"].as_str().unwrap_or("").trim().to_string();
    let client = reqwest::Client::new();
    // ComfyUI exposes /system_stats as a lightweight JSON health endpoint.
    let mut rb = client.get(format!("{}/system_stats", base)).timeout(std::time::Duration::from_secs(15));
    if !api_key.is_empty() { rb = rb.header("Authorization", format!("Bearer {}", api_key)); }

    let coll = state.db.collection::<Document>("settings");
    let now_rfc = chrono::Utc::now().to_rfc3339();
    match rb.send().await {
        Ok(res) if res.status().is_success() => {
            let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "comfyui_valid": true, "comfyui_status": "reachable", "comfyui_checked_at": &now_rfc } }).await;
            Ok(serde_json::json!({ "ok": true, "status": "reachable", "detail": format!("ComfyUI responded at {}", base) }))
        }
        Ok(res) if res.status() == 401 || res.status() == 403 => {
            Ok(serde_json::json!({ "ok": false, "status": "auth_error", "detail": "Server reachable but rejected the request — check the API key." }))
        }
        Ok(res) => Ok(serde_json::json!({ "ok": false, "status": "http_error", "detail": format!("Server returned HTTP {} — is the ComfyUI notebook running?", res.status()) })),
        Err(err) => {
            let detail = if err.is_timeout() { "Connection timeout — server unreachable (notebook stopped or URL expired?)".to_string() }
                else if err.is_connect() { "Cannot connect — check the URL and that the notebook is running".to_string() }
                else { format!("Connection error: {}", err) };
            Ok(serde_json::json!({ "ok": false, "status": "connection_error", "detail": detail }))
        }
    }
}

#[tauri::command]
pub async fn test_mj(state: State<'_, AppState>) -> Res<Value> {
    let doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();

    // Prefer Playwright profile for authentication
    let profile_env = std::env::var("MJ_PROFILE_DIR").ok();
    let profile_dir = profile_env.as_deref().unwrap_or_else(|| doc["mj_profile_dir"].as_str().unwrap_or("")).trim();
    if !profile_dir.is_empty() {
        if std::path::Path::new(profile_dir).exists() {
            let coll = state.db.collection::<Document>("settings");
            let now_rfc = chrono::Utc::now().to_rfc3339();
            let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "mj_profile_valid": true, "mj_profile_checked_at": &now_rfc } }).await;
            return Ok(serde_json::json!({
                "ok": true,
                "status": "connected",
                "detail": "Midjourney profile directory present.",
                "method": "profile"
            }));
        } else {
            return Ok(serde_json::json!({
                "ok": false,
                "status": "profile_missing",
                "detail": "Configured Playwright profile directory not found on disk.",
                "next_step": "Capture a new session via the Settings → Capture session flow."
            }));
        }
    }

    // Legacy midjourney-proxy support removed. If no profile configured
    // instruct the user to capture a session via the Settings UI.
    return Ok(serde_json::json!({
        "ok": false,
        "status": "not_configured",
        "detail": "Midjourney session profile is not configured.",
        "next_step": "Use the browser capture button to open Midjourney and capture a Playwright profile.",
        "setup_guide": "https://www.midjourney.com"
    }));
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
        // Try to find ffmpeg in the resource directory next to the executable
        if let Ok(exe_path) = env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                let candidates = [
                    parent.join("ffmpeg"),
                    parent.join("ffmpeg.exe"),
                    parent.join("bin").join("ffmpeg"),
                    parent.join("bin").join("ffmpeg.exe"),
                ];
                for c in &candidates {
                    if c.exists() && c.is_file() {
                        resolved = Some(c.to_string_lossy().to_string());
                        break;
                    }
                }
            }
        }
    }
    Ok(serde_json::json!({ "ok": resolved.is_some(), "path": resolved.unwrap_or(path) }))
}

#[tauri::command]
pub async fn open_suno_login() -> Res<Value> {
    let url = "https://suno.com";
    open::that(url).map_err(|err| format!("Failed to open browser for Suno login: {}", err))?;
    Ok(serde_json::json!({ "ok": true, "url": url }))
}

#[tauri::command]
pub async fn open_midjourney_login() -> Res<Value> {
    let url = "https://www.midjourney.com";
    // Prefer launching the bundled Playwright-based visible browser flow when available
    if let Some(script) = locate_resource_file("midjourney-session-capture.js") {
        if let Some(node) = resolve_node_executable() {
            let profile_dir = env::temp_dir().join("biblemusically-midjourney-playwright-profile");
            let _ = fs::create_dir_all(&profile_dir).await;
            let mut cmd = tokio::process::Command::new(node);
            let _ = cmd
                .arg(script.to_string_lossy().to_string())
                .arg(profile_dir.to_string_lossy().to_string())
                .arg("300")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(|err| format!("Failed to launch Playwright for Midjourney login: {}", err))?;
            return Ok(serde_json::json!({ "ok": true, "url": url, "method": "playwright", "profile_dir": profile_dir.to_string_lossy() }));
        }
    }

    // Fallback: open system browser
    open::that(url).map_err(|err| format!("Failed to open browser for Midjourney login: {}", err))?;
    Ok(serde_json::json!({ "ok": true, "url": url, "method": "system" }))
}

// ── Kaggle server helpers ────────────────────────────────────────
// The free Suno/Midjourney-alternative engines run as notebooks on the user's
// Kaggle account and expose an ephemeral `*.trycloudflare.com` tunnel that
// rotates every run. These commands let the app open the notebook, open the
// Kaggle API-token page, and pull the current tunnel URL straight from the
// kernel's latest output — instead of the user hand-copying a rotating URL.

/// Maps a UI engine id → (Kaggle kernel slug, the settings key its live URL is stored in).
pub(crate) fn kaggle_kernel_for(engine: &str) -> Option<(&'static str, &'static str)> {
    match engine {
        "acestep" => Some(("joehannes/biblemusically-acestep-server", "acestep_api_url")),
        "heartmula" => Some(("joehannes/biblemusically-heartmula-server", "heartmula_api_url")),
        "comfyui" | "comfy" => Some(("joehannes/biblemusically-comfyui-server", "comfyui_api_url")),
        "flux" => Some(("joehannes/biblemusically-flux-server", "flux_api_url")),
        _ => None,
    }
}

/// Locate the `kaggle` CLI. A desktop-launched app inherits a minimal PATH that
/// usually misses ~/.local/bin (pipx shim) and linuxbrew, so probe those first.
pub(crate) fn locate_kaggle() -> String {
    if let Ok(home) = env::var("HOME") {
        let p = PathBuf::from(&home).join(".local/bin/kaggle");
        if p.is_file() { return p.to_string_lossy().into_owned(); }
    }
    for p in ["/home/linuxbrew/.linuxbrew/bin/kaggle", "/usr/local/bin/kaggle"] {
        if PathBuf::from(p).is_file() { return p.to_string(); }
    }
    "kaggle".to_string() // last resort: let the OS resolve it on PATH
}

/// Scan every file in a directory for the last-printed public tunnel URL.
async fn scan_dir_for_tunnel_url(dir: &std::path::Path) -> Option<String> {
    let re = regex::Regex::new(r"https://[a-z0-9-]+\.(?:trycloudflare\.com|gradio\.live|lhr\.life|serveo\.net)").ok()?;
    let mut last: Option<String> = None;
    let mut rd = fs::read_dir(dir).await.ok()?;
    while let Ok(Some(entry)) = rd.next_entry().await {
        if let Ok(bytes) = fs::read(entry.path()).await {
            let text = String::from_utf8_lossy(&bytes);
            for m in re.find_iter(&text) { last = Some(m.as_str().to_string()); }
        }
    }
    last
}

/// Stream `kaggle kernels logs -f <slug>` for up to `secs` seconds, returning the first
/// public tunnel URL printed by the CURRENT run. This is the only way to read a run that
/// is still serving — `kernels output` returns data only after a run completes.
async fn stream_logs_for_tunnel_url(kaggle: &str, slug: &str, secs: u64) -> Option<String> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let re = regex::Regex::new(r"https://[a-z0-9-]+\.(?:trycloudflare\.com|gradio\.live|lhr\.life|serveo\.net)").ok()?;
    let mut child = tokio::process::Command::new(kaggle)
        .args(["kernels", "logs", "-f", slug])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn().ok()?;
    let stdout = child.stdout.take()?;
    let mut lines = BufReader::new(stdout).lines();
    let found = tokio::time::timeout(Duration::from_secs(secs), async {
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(m) = re.find(&line) { return Some(m.as_str().to_string()); }
        }
        None
    }).await.ok().flatten();
    let _ = child.kill().await;
    found
}

#[tauri::command]
pub async fn open_kaggle_notebook(engine: String) -> Res<Value> {
    let (slug, _) = kaggle_kernel_for(&engine).ok_or_else(|| format!("Unknown engine '{}'.", engine))?;
    let url = format!("https://www.kaggle.com/code/{}", slug);
    open::that(&url).map_err(|err| format!("Failed to open Kaggle notebook: {}", err))?;
    Ok(serde_json::json!({ "ok": true, "url": url }))
}

/// Return an engine's Kaggle notebook URL WITHOUT opening it, so the UI can load it in the
/// in-app browser instead of launching an external browser window.
#[tauri::command]
pub async fn kaggle_notebook_url(engine: String) -> Res<Value> {
    let (slug, _) = kaggle_kernel_for(&engine).ok_or_else(|| format!("Unknown engine '{}'.", engine))?;
    Ok(serde_json::json!({ "ok": true, "url": format!("https://www.kaggle.com/code/{}", slug) }))
}

#[tauri::command]
pub async fn open_kaggle_token_page() -> Res<Value> {
    // The "Create New API Token" button lives on the account settings page.
    let url = "https://www.kaggle.com/settings";
    open::that(url).map_err(|err| format!("Failed to open Kaggle settings: {}", err))?;
    Ok(serde_json::json!({ "ok": true, "url": url }))
}

/// Open the Kaggle sign-in page in the SYSTEM browser. Used by the onboarding wizard's guided
/// Kaggle step — the user can sign in (Google is fine there, unlike the embedded webview which
/// Google blocks) and then create an API token.
#[tauri::command]
pub async fn open_kaggle_login() -> Res<Value> {
    let url = "https://www.kaggle.com/account/login";
    open::that(url).map_err(|err| format!("Failed to open Kaggle sign-in: {}", err))?;
    Ok(serde_json::json!({ "ok": true, "url": url }))
}

/// Write a kaggle.json to `~/.kaggle/kaggle.json` (0600 — the CLI refuses a world-readable token).
async fn write_kaggle_json(username: &str, key: &str) -> Res<String> {
    let home = env::var("HOME").map_err(|_| "Could not locate your home directory.".to_string())?;
    let dir = PathBuf::from(&home).join(".kaggle");
    fs::create_dir_all(&dir).await.map_err(e)?;
    let path = dir.join("kaggle.json");
    fs::write(&path, serde_json::json!({ "username": username, "key": key }).to_string()).await.map_err(e)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(path.to_string_lossy().into_owned())
}

/// Lightweight CLI auth check → (verified, detail).
async fn verify_kaggle_auth() -> (bool, String) {
    let kaggle = locate_kaggle();
    match tokio::process::Command::new(&kaggle)
        .args(["kernels", "list", "--mine", "--page-size", "1"]).output().await
    {
        Ok(o) => {
            let out = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
            let low = out.to_lowercase();
            let bad = low.contains("401") || low.contains("403") || low.contains("unauthorized") || low.contains("forbidden");
            (o.status.success() && !bad, out.chars().take(300).collect())
        }
        Err(err) => (false, format!("could not run the kaggle CLI: {err}")),
    }
}

/// Read the stored Kaggle accounts (`[{username, key}]`) from the settings singleton.
async fn stored_kaggle_accounts(db: &crate::store::Db) -> Vec<Document> {
    db.collection::<Document>("settings").find_one(doc! { "_id": "singleton" }).await.ok().flatten()
        .and_then(|d| d.get_array("kaggle_accounts").ok().cloned())
        .map(|arr| arr.iter().filter_map(|b| b.as_document().cloned()).collect())
        .unwrap_or_default()
}

/// Save a pasted kaggle.json: write it, verify, and record it in the multi-account store as the
/// ACTIVE account. Kaggle's free GPU quota is per-account, so the app keeps every account the user
/// connects and can rotate to the next when one is exhausted (see `rotate_kaggle_account`).
/// Keys live in the local settings DB only, alongside the app's other credentials, and are never
/// returned to the UI.
#[tauri::command]
pub async fn save_kaggle_token(state: State<'_, AppState>, token_json: String) -> Res<Value> {
    let parsed: Value = serde_json::from_str(token_json.trim()).map_err(|_|
        "That doesn't look like a valid kaggle.json — it should be {\"username\":\"…\",\"key\":\"…\"}.".to_string())?;
    let username = parsed["username"].as_str().unwrap_or("").trim().to_string();
    let key = parsed["key"].as_str().unwrap_or("").trim().to_string();
    if username.is_empty() || key.is_empty() {
        return Err("The token is missing \"username\" or \"key\". Download a fresh one from Kaggle → Settings → Create New API Token.".into());
    }
    let path = write_kaggle_json(&username, &key).await?;
    let (verified, detail) = verify_kaggle_auth().await;

    // Upsert into the account list (dedupe by username), then mark it active.
    let coll = state.db.collection::<Document>("settings");
    let mut accts = stored_kaggle_accounts(&state.db).await;
    accts.retain(|a| a.get_str("username").ok() != Some(username.as_str()));
    accts.push(doc! { "username": &username, "key": &key });
    let bson_accts = bson::to_bson(&accts).map_err(e)?;
    coll.update_one(
        doc! { "_id": "singleton" },
        doc! { "$set": { "kaggle_accounts": bson_accts, "kaggle_active": &username,
                         "kaggle_connected": true, "kaggle_username": &username } },
    ).with_options(crate::store::UpdateOptions::builder().upsert(true).build()).await.map_err(e)?;

    Ok(serde_json::json!({ "ok": true, "username": username, "verified": verified, "detail": detail,
        "path": path, "account_count": accts.len() }))
}

/// List connected Kaggle accounts (usernames + which is active). Never returns keys.
#[tauri::command]
pub async fn list_kaggle_accounts(state: State<'_, AppState>) -> Res<Value> {
    let doc = state.db.collection::<Document>("settings").find_one(doc! { "_id": "singleton" }).await.ok().flatten();
    let active = doc.as_ref().and_then(|d| d.get_str("kaggle_active").ok()).unwrap_or("").to_string();
    let accts = stored_kaggle_accounts(&state.db).await;
    let list: Vec<Value> = accts.iter().filter_map(|a| a.get_str("username").ok().map(|u| {
        serde_json::json!({ "username": u, "active": u == active })
    })).collect();
    Ok(serde_json::json!({ "accounts": list, "active": active }))
}

/// Activate a stored account: write its kaggle.json and mark it active.
#[tauri::command]
pub async fn activate_kaggle_account(state: State<'_, AppState>, username: String) -> Res<Value> {
    let accts = stored_kaggle_accounts(&state.db).await;
    let acct = accts.iter().find(|a| a.get_str("username").ok() == Some(username.as_str()))
        .ok_or_else(|| format!("No stored account '{}'.", username))?;
    let key = acct.get_str("key").map_err(|_| "Stored account is missing its key.".to_string())?;
    write_kaggle_json(&username, key).await?;
    let (verified, _) = verify_kaggle_auth().await;
    state.db.collection::<Document>("settings").update_one(
        doc! { "_id": "singleton" },
        doc! { "$set": { "kaggle_active": &username, "kaggle_username": &username, "kaggle_connected": true } },
    ).await.map_err(e)?;
    Ok(serde_json::json!({ "ok": true, "username": username, "verified": verified }))
}

/// Remove a stored account. If it was active, the first remaining account becomes active.
#[tauri::command]
pub async fn remove_kaggle_account(state: State<'_, AppState>, username: String) -> Res<Value> {
    let coll = state.db.collection::<Document>("settings");
    let mut accts = stored_kaggle_accounts(&state.db).await;
    accts.retain(|a| a.get_str("username").ok() != Some(username.as_str()));
    let new_active = accts.first().and_then(|a| a.get_str("username").ok().map(|s| s.to_string()));
    let bson_accts = bson::to_bson(&accts).map_err(e)?;
    coll.update_one(
        doc! { "_id": "singleton" },
        doc! { "$set": { "kaggle_accounts": bson_accts, "kaggle_active": new_active.clone().unwrap_or_default() } },
    ).await.map_err(e)?;
    if let Some(u) = &new_active {
        // Re-point the CLI token at whatever is now active.
        if let Some(acct) = accts.iter().find(|a| a.get_str("username").ok() == Some(u.as_str())) {
            if let Ok(key) = acct.get_str("key") { let _ = write_kaggle_json(u, key).await; }
        }
    }
    Ok(serde_json::json!({ "ok": true, "active": new_active }))
}

/// Rotate to the next stored account after the active one (wraps). Returns the new active username,
/// or `ok:false` with `only_one:true` when there's nothing to rotate to — the app then prompts the
/// user to connect another free account. Called automatically when a run is denied a GPU (quota
/// exhausted), since Kaggle's quota is per-account.
#[tauri::command]
pub async fn rotate_kaggle_account(state: State<'_, AppState>) -> Res<Value> {
    let doc = state.db.collection::<Document>("settings").find_one(doc! { "_id": "singleton" }).await.ok().flatten();
    let active = doc.as_ref().and_then(|d| d.get_str("kaggle_active").ok()).unwrap_or("").to_string();
    let accts = stored_kaggle_accounts(&state.db).await;
    if accts.len() < 2 {
        return Ok(serde_json::json!({ "ok": false, "only_one": true,
            "detail": "Only one Kaggle account is connected — connect another free account to rotate to it when quota runs out." }));
    }
    let idx = accts.iter().position(|a| a.get_str("username").ok() == Some(active.as_str())).unwrap_or(0);
    let next = &accts[(idx + 1) % accts.len()];
    let next_user = next.get_str("username").map_err(|_| "next account missing username".to_string())?.to_string();
    let key = next.get_str("key").map_err(|_| "next account missing key".to_string())?;
    write_kaggle_json(&next_user, key).await?;
    state.db.collection::<Document>("settings").update_one(
        doc! { "_id": "singleton" },
        doc! { "$set": { "kaggle_active": &next_user, "kaggle_username": &next_user, "kaggle_connected": true } },
    ).await.map_err(e)?;
    Ok(serde_json::json!({ "ok": true, "username": next_user, "rotated_from": active }))
}

/// Native folder picker (used by onboarding to choose where project files/exports live).
/// Returns the chosen absolute path, or null if the user cancelled.
#[tauri::command]
pub async fn pick_directory(title: Option<String>) -> Res<Option<String>> {
    let mut dlg = rfd::AsyncFileDialog::new();
    if let Some(t) = title.filter(|s| !s.is_empty()) { dlg = dlg.set_title(t); }
    Ok(dlg.pick_folder().await.map(|f| f.path().to_string_lossy().to_string()))
}

#[tauri::command]
pub async fn start_kaggle_server(engine: String) -> Res<Value> {
    let (slug, _) = match kaggle_kernel_for(&engine) {
        Some(v) => v,
        None => return Ok(serde_json::json!({ "ok": false, "detail": format!("Unknown engine '{}'.", engine) })),
    };
    let kaggle = locate_kaggle();

    // Pull the kernel (code + metadata) from Kaggle so this works from the installed app,
    // which doesn't ship the notebook sources.
    let tmp = env::temp_dir().join(format!("bm-kaggle-start-{}", engine));
    let _ = fs::remove_dir_all(&tmp).await;
    let _ = fs::create_dir_all(&tmp).await;
    let pull = tokio::process::Command::new(&kaggle)
        .args(["kernels", "pull", slug, "-m", "-p"]).arg(&tmp)
        .output().await.map_err(|err| format!("Could not run the kaggle CLI: {}", err))?;
    if !pull.status.success() {
        let msg = format!("{}{}", String::from_utf8_lossy(&pull.stdout), String::from_utf8_lossy(&pull.stderr));
        return Ok(serde_json::json!({
            "ok": false, "status": "pull_failed",
            "detail": format!("Could not pull the kernel from Kaggle: {}", msg.trim()),
            "next_step": "Check that ~/.kaggle/kaggle.json is valid (Kaggle → Settings → Create New API Token)."
        }));
    }

    // A serving run needs the GPU: the notebook's batch guard only opens the tunnel
    // when a GPU is present (GPU-less batch runs are cheap source-update pushes).
    let meta_path = tmp.join("kernel-metadata.json");
    let meta_raw = fs::read_to_string(&meta_path).await.map_err(e)?;
    let mut meta: Value = serde_json::from_str(&meta_raw).map_err(e)?;
    meta["enable_gpu"] = Value::Bool(true);
    meta["enable_internet"] = Value::Bool(true);
    fs::write(&meta_path, serde_json::to_string_pretty(&meta).map_err(e)?).await.map_err(e)?;

    let push = tokio::process::Command::new(&kaggle)
        .args(["kernels", "push", "-p"]).arg(&tmp)
        .output().await.map_err(|err| format!("Could not run the kaggle CLI: {}", err))?;
    let out = format!("{}{}", String::from_utf8_lossy(&push.stdout), String::from_utf8_lossy(&push.stderr));
    let low = out.to_lowercase();
    if out.contains("successfully pushed") {
        Ok(serde_json::json!({
            "ok": true, "status": "starting",
            "detail": format!("{} server starting on Kaggle (GPU batch run). It needs ~8-10 min to install, download models and open the tunnel.", engine),
            "next_step": "Watch the live log below. The run serves until Kaggle's ~9-12 h batch limit."
        }))
    } else if low.contains("session count") || low.contains("concurrent")
        || low.contains("maximum number") || low.contains("too many")
        || low.contains("reached the maximum") || low.contains("active session")
    {
        // The 2-concurrent-GPU-batch-session cap. A kernel whose tunnel died (idle-watchdog killed
        // the server, or it crashed) still shows RUNNING and still holds its GPU slot until the
        // ~9-12 h batch limit — pushing a fresh run for that SAME kernel does not free it first.
        // Neither the kaggle CLI nor Kaggle's API expose a way to stop a session remotely; the only
        // way to free a slot early is the "Stop Session" button on the notebook's own edit page.
        Ok(serde_json::json!({
            "ok": false, "status": "gpu_slots_full",
            "detail": "Kaggle allows only 2 concurrent GPU batch sessions and both are in use.",
            "next_step": "Open each engine's notebook (Open notebook button) and look for one still showing a running session with a dead tunnel — click Stop Session there, then retry. The kaggle CLI can't stop a session remotely, so this step needs the notebook page itself."
        }))
    } else if low.contains("quota") || low.contains("exceeded") {
        // Weekly GPU-hour quota (30 h) is spent. Resets Saturdays UTC.
        Ok(serde_json::json!({
            "ok": false, "status": "gpu_quota",
            "detail": "Your weekly Kaggle GPU quota (30 h) looks exhausted — a GPU batch run can't start.",
            "next_step": "Wait for the weekly reset (Saturdays UTC), or run the engine on another free GPU host (Lightning.ai / Colab) and paste its URL above."
        }))
    } else if low.contains("403") || low.contains("forbidden") || low.contains("401")
        || low.contains("unauthorized") || low.contains("invalid") && low.contains("token")
    {
        Ok(serde_json::json!({
            "ok": false, "status": "auth_error",
            "detail": "Kaggle rejected the push — the API token is missing or invalid.",
            "next_step": "Click Kaggle API token, Create New API Token, and save it as ~/.kaggle/kaggle.json, then retry."
        }))
    } else {
        Ok(serde_json::json!({
            "ok": false, "status": "push_failed",
            "detail": out.trim().chars().take(400).collect::<String>(),
            "next_step": "Open the notebook (Open notebook) and check the kernel there. If it looks fine, retry Start & connect."
        }))
    }
}

/// Stop `engine`'s Kaggle session so it stops consuming the free weekly GPU quota.
///
/// A running GPU batch session bills quota for its whole life, used or not — so the app shuts a
/// server down as soon as the workflow no longer needs it, after 15 min of app inactivity, and on
/// exit. Mechanically this is the same GPU-off supersede push used for zombie recovery: pushing a
/// new kernel version ends the one session Kaggle allows per kernel, and a GPU-off run needs no
/// GPU slot. (The Kaggle CLI/API has no "stop session" call.)
#[tauri::command]
pub async fn stop_kaggle_server(engine: String) -> Res<Value> {
    let r = supersede_kaggle_session(engine.clone()).await?;
    let ok = r["ok"].as_bool().unwrap_or(false);
    Ok(serde_json::json!({
        "ok": ok,
        "engine": engine,
        "detail": if ok { format!("{} stopped — its GPU slot is released.", engine) }
                  else { r["detail"].as_str().unwrap_or("Could not stop the session.").to_string() },
    }))
}

/// Auto-recover a stuck/zombie session for `engine` by pushing a GPU-OFF version of its kernel.
///
/// The Kaggle CLI/API has no "stop session" — but pushing a new kernel version SUPERSEDES the one
/// running session Kaggle allows per kernel, and a GPU-off run needs no GPU slot, so this ends a
/// dead-tunnel zombie and frees the GPU slot it was holding without any manual "Stop Session"
/// click. The app calls this automatically before retrying a start that failed because the
/// engine's own zombie held the last slot (see kaggleServerPipeline.js).
#[tauri::command]
pub async fn supersede_kaggle_session(engine: String) -> Res<Value> {
    let (slug, _) = match kaggle_kernel_for(&engine) {
        Some(v) => v,
        None => return Ok(serde_json::json!({ "ok": false, "detail": format!("Unknown engine '{}'.", engine) })),
    };
    let kaggle = locate_kaggle();
    let tmp = env::temp_dir().join(format!("bm-kaggle-stop-{}", engine));
    let _ = fs::remove_dir_all(&tmp).await;
    let _ = fs::create_dir_all(&tmp).await;
    let pull = tokio::process::Command::new(&kaggle)
        .args(["kernels", "pull", slug, "-m", "-p"]).arg(&tmp)
        .output().await.map_err(|err| format!("Could not run the kaggle CLI: {}", err))?;
    if !pull.status.success() {
        let msg = format!("{}{}", String::from_utf8_lossy(&pull.stdout), String::from_utf8_lossy(&pull.stderr));
        return Ok(serde_json::json!({ "ok": false, "detail": format!("Could not pull the kernel to supersede it: {}", msg.trim()) }));
    }
    let meta_path = tmp.join("kernel-metadata.json");
    if let Ok(raw) = fs::read_to_string(&meta_path).await {
        if let Ok(mut meta) = serde_json::from_str::<Value>(&raw) {
            meta["enable_gpu"] = Value::Bool(false); // GPU-off: supersede without needing a slot
            let pretty = serde_json::to_string_pretty(&meta).unwrap_or(raw);
            let _ = fs::write(&meta_path, pretty).await;
        }
    }
    let push = tokio::process::Command::new(&kaggle)
        .args(["kernels", "push", "-p"]).arg(&tmp)
        .output().await.map_err(|err| format!("Could not run the kaggle CLI: {}", err))?;
    let out = format!("{}{}", String::from_utf8_lossy(&push.stdout), String::from_utf8_lossy(&push.stderr));
    if out.to_lowercase().contains("successfully pushed") {
        Ok(serde_json::json!({ "ok": true,
            "detail": format!("Ended {}'s stuck session with a GPU-off push — its GPU slot should free within a minute.", engine) }))
    } else {
        Ok(serde_json::json!({ "ok": false,
            "detail": format!("Supersede push didn't confirm: {}", out.trim().chars().take(200).collect::<String>()) }))
    }
}

/// Re-discover and persist `engine`'s live tunnel URL with no UI-facing diagnostics — the same
/// discover-then-verify steps as `fetch_kaggle_url` below, minus the kernel-status messaging,
/// so a job runner can call this mid-run when the cached URL turns out to be a dead tunnel
/// (Cloudflare tunnels rotate every run — see jobs.rs's `generate_song_api`) instead of failing
/// outright and making the user manually re-run "Fetch live URL" in Settings.
pub async fn refresh_kaggle_url(db: &crate::store::Db, engine: &str) -> Option<String> {
    let (slug, settings_key) = kaggle_kernel_for(engine)?;
    let kaggle = locate_kaggle();

    let tmp = env::temp_dir().join(format!("bm-kaggle-{}", engine));
    let _ = fs::create_dir_all(&tmp).await;
    let _ = tokio::process::Command::new(&kaggle)
        .args(["kernels", "output", slug, "-p"]).arg(&tmp).output().await;
    let mut url = scan_dir_for_tunnel_url(&tmp).await;
    if url.is_none() {
        url = stream_logs_for_tunnel_url(&kaggle, slug, 25).await;
    }
    let u = url?;

    let alive = match reqwest::Client::new().get(&u).timeout(std::time::Duration::from_secs(8)).send().await {
        Ok(res) => res.status().as_u16() < 500,
        Err(_) => false,
    };
    if !alive {
        return None;
    }

    let mut set = Document::new();
    set.insert(settings_key, u.clone());
    let _ = db.collection::<Document>("settings")
        .update_one(doc! { "_id": "singleton" }, doc! { "$set": set }).await;
    Some(u)
}

#[tauri::command]
pub async fn fetch_kaggle_url(state: State<'_, AppState>, engine: String) -> Res<Value> {
    let (slug, settings_key) = match kaggle_kernel_for(&engine) {
        Some(v) => v,
        None => return Ok(serde_json::json!({ "ok": false, "detail": format!("Unknown engine '{}'.", engine) })),
    };
    let kaggle = locate_kaggle();

    // 1) Kernel status — so we can give an actionable message when there's no URL.
    let status_str = match tokio::process::Command::new(&kaggle)
        .args(["kernels", "status", slug]).output().await
    {
        Ok(o) => format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)),
        Err(err) => return Ok(serde_json::json!({
            "ok": false, "status": "no_cli",
            "detail": format!("Could not run the kaggle CLI: {}", err),
            "next_step": "Install it with `pipx install kaggle` and place your token at ~/.kaggle/kaggle.json (Kaggle → Settings → Create New API Token)."
        })),
    };
    let kstatus = if status_str.contains("CANCEL") { "cancelled" }
        else if status_str.contains("COMPLETE") { "complete" }
        else if status_str.contains("RUNNING") { "running" }
        else if status_str.contains("ERROR") { "error" }
        else if status_str.contains("QUEUED") || status_str.contains("PREPARING") { "queued" }
        else if status_str.to_lowercase().contains("403") || status_str.to_lowercase().contains("forbidden") {
            return Ok(serde_json::json!({
                "ok": false, "status": "auth_error",
                "detail": "Kaggle rejected the request — missing or invalid API token.",
                "next_step": "Open the Kaggle token page, Create New API Token, and save it as ~/.kaggle/kaggle.json."
            }));
        }
        else { "unknown" };

    // 2) Pull the kernel's latest output/log and grep for the tunnel URL.
    //    `kernels output` only has data for COMPLETED runs; a RUNNING server (the case
    //    that matters) is only visible by streaming `kernels logs -f` for a bit.
    let tmp = env::temp_dir().join(format!("bm-kaggle-{}", engine));
    let _ = fs::create_dir_all(&tmp).await;
    let _ = tokio::process::Command::new(&kaggle)
        .args(["kernels", "output", slug, "-p"]).arg(&tmp).output().await;
    let mut url = scan_dir_for_tunnel_url(&tmp).await;

    if url.is_none() && matches!(kstatus, "running" | "queued" | "unknown") {
        url = stream_logs_for_tunnel_url(&kaggle, slug, 25).await;
    }

    // 3) Liveness probe: logs (and `output` of finished runs) happily yield the URL of a
    //    DEAD tunnel — e.g. after the batch time limit or a cancel. Only save a URL that
    //    actually answers; otherwise tell the user the server needs a restart.
    if let Some(u) = &url {
        let alive = match reqwest::Client::new().get(u.clone())
            .timeout(std::time::Duration::from_secs(8)).send().await
        {
            Ok(res) => res.status().as_u16() < 500,
            Err(_) => false,
        };
        if !alive {
            return Ok(serde_json::json!({
                "ok": false, "status": "stale_url", "kernel_status": kstatus,
                "detail": format!("Found {} in the kernel log, but that tunnel is no longer alive (run ended: {}).", u, kstatus),
                "next_step": "Click Start server to launch a fresh run, wait ~8-10 min, then Fetch live URL again."
            }));
        }
    }

    match url {
        Some(u) => {
            // Persist it into the engine's URL setting (dynamic key → build the doc by hand).
            let mut set = Document::new();
            set.insert(settings_key, u.clone());
            let _ = state.db.collection::<Document>("settings")
                .update_one(doc! { "_id": "singleton" }, doc! { "$set": set }).await;
            Ok(serde_json::json!({
                "ok": true, "url": u, "kernel_status": kstatus,
                "detail": format!("Live URL pulled from the {} kernel — saved. Now click Test connection.", engine)
            }))
        }
        None => {
            let (detail, next_step): (String, &str) = match kstatus {
                "running" | "queued" => (
                    format!("Kernel is {} but hasn't printed a tunnel URL yet — servers need ~8-10 min to come up.", kstatus),
                    "Wait a few minutes, then click Fetch live URL again."),
                "error" => (
                    "The kernel's last run ERRORED, so no server is serving.".to_string(),
                    "Click Start server (or open the notebook and Run All), wait ~8-10 min, then Fetch again."),
                "cancelled" | "complete" => (
                    "The kernel's last run has ended — its server and tunnel are gone.".to_string(),
                    "Click Start server to launch a fresh run, wait ~8-10 min, then Fetch live URL again."),
                _ => (
                    "No tunnel URL found in the kernel's latest output.".to_string(),
                    "Click Start server (or open the notebook and Run All with GPU + Internet), then Fetch again."),
            };
            Ok(serde_json::json!({ "ok": false, "kernel_status": kstatus, "detail": detail, "next_step": next_step }))
        }
    }
}

#[tauri::command]
pub async fn capture_midjourney_session(state: State<'_, AppState>) -> Res<Value> {
    let script = locate_resource_file("midjourney-session-capture.js")
        .ok_or_else(|| "Midjourney capture script not found in resources".to_string())?;
    let node = resolve_node_executable().ok_or_else(|| "Node.js is required for Midjourney session automation. Install Node.js and npm.".to_string())?;
    let profile_dir = env::temp_dir().join("biblemusically-midjourney-playwright-profile");
    fs::create_dir_all(&profile_dir).await.map_err(e)?;

    let output = tokio::process::Command::new(node)
        .arg(script.clone())
        .arg(profile_dir.to_string_lossy().to_string())
        .arg("300")
        .output()
        .await
        .map_err(|err| format!("Failed to launch Midjourney browser automation: {}", err))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if !output.status.success() {
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(format!("Midjourney session capture failed: {}", detail));
    }

    let result: Value = serde_json::from_str(&stdout)
        .map_err(|err| format!("Failed to parse Midjourney capture output: {}\nstdout={}\nstderr={}", err, stdout, stderr))?;
    if !result["ok"].as_bool().unwrap_or(false) {
        let detail = result["detail"].as_str().unwrap_or("Midjourney capture failed");
        return Err(detail.to_string());
    }

    if let Some(profile) = result["profile_dir"].as_str() {
        let coll = state.db.collection::<Document>("settings");
        let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "mj_profile_dir": profile.to_string() } }).await;
    }

    Ok(result)
}


#[tauri::command]
pub async fn capture_suno_session(state: State<'_, AppState>) -> Res<Value> {
    let script = locate_resource_file("suno-session-capture.js")
        .ok_or_else(|| "Suno capture script not found in resources".to_string())?;
    let node = resolve_node_executable().ok_or_else(|| "Node.js is required for Suno session automation. Install Node.js and npm.".to_string())?;
    let output = tokio::process::Command::new(node)
        .arg(script.clone())
        .arg("--timeout")
        .arg("300")
        .output()
        .await
        .map_err(|err| format!("Failed to launch Suno browser automation: {}", err))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if !output.status.success() {
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(format!("Suno session capture failed: {}", detail));
    }

    let result: Value = serde_json::from_str(&stdout)
        .map_err(|err| format!("Failed to parse Suno capture output: {}\nstdout={}\nstderr={}", err, stdout, stderr))?;
    if !result["ok"].as_bool().unwrap_or(false) {
        let detail = result["detail"].as_str().unwrap_or("Suno capture failed");
        return Err(detail.to_string());
    }

    if let Some(cookie) = result["cookie"].as_str() {
        let coll = state.db.collection::<Document>("settings");
        let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "suno_cookie": cookie.to_string() } }).await;
    }

    Ok(result)
}

// ── Webview-based session capture ────────────────────────────────────────────
// Local-webview versions of the Playwright "remote chrome" capture flows above: the user
// signs in inside the embedded browser (Web Browser view), and these commands lift the
// session cookies straight out of the webview's cookie store — no second visible browser.
// The Midjourney variant then seeds the same Playwright persistent profile the existing
// automation scripts (midjourney-generator.js, the proxy autostart) drive, so remote-chrome
// automation and the in-app webview share one login.

/// Collect deduped cookies visible to the embedded webview for the given URLs
/// (includes HttpOnly/secure cookies, which page JS could never read).
fn webview_cookies_for(
    mgr: &super::webview::WebviewManager,
    urls: &[&str],
) -> Res<Vec<tauri::webview::Cookie<'static>>> {
    let wv = mgr.primary_webview()?;
    let mut out: Vec<tauri::webview::Cookie<'static>> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for u in urls {
        let url = match url::Url::parse(u) { Ok(x) => x, Err(_) => continue };
        if let Ok(cookies) = wv.cookies_for_url(url) {
            for c in cookies {
                let key = (
                    c.name().to_string(),
                    c.domain().unwrap_or("").to_string(),
                    c.path().unwrap_or("").to_string(),
                );
                if seen.insert(key) {
                    out.push(c);
                }
            }
        }
    }
    Ok(out)
}

fn cookie_header(cookies: &[tauri::webview::Cookie<'static>]) -> String {
    cookies
        .iter()
        .map(|c| format!("{}={}", c.name(), c.value()))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Capture the Suno session from the embedded webview and store it as `suno_cookie` —
/// same outcome as `capture_suno_session`, without launching a separate browser.
#[tauri::command]
pub async fn webview_capture_suno_session(
    mgr: State<'_, super::webview::WebviewManager>,
    state: State<'_, AppState>,
) -> Res<Value> {
    let urls = [
        "https://suno.com/",
        "https://studio-api.suno.ai/",
        "https://studio-api.suno.com/",
        "https://accounts.suno.com/",
        "https://clerk.suno.com/",
    ];
    let cookies = webview_cookies_for(&mgr, &urls)?;
    let names: Vec<String> = cookies.iter().map(|c| c.name().to_string()).collect();

    // Primary target mirrors the Playwright capture script; the Clerk `__client` JWT is the
    // fallback marker for Suno's current auth (send the full header so `__session` rides along).
    let primary = cookies
        .iter()
        .find(|c| c.name() == "studio-api_key" || c.name() == "studio-api_key_local");
    let (cookie_str, marker) = if let Some(c) = primary {
        (format!("{}={}", c.name(), c.value()), c.name().to_string())
    } else if let Some(c) = cookies.iter().find(|c| c.name() == "__client") {
        (cookie_header(&cookies), c.name().to_string())
    } else {
        return Ok(serde_json::json!({
            "ok": false,
            "detail": "No Suno session cookie in the webview yet — finish signing in on the Suno page first.",
            "cookie_names": names,
        }));
    };

    let coll = state.db.collection::<Document>("settings");
    coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "suno_cookie": cookie_str.clone() } })
        .await
        .map_err(e)?;
    Ok(serde_json::json!({
        "ok": true,
        "method": "webview",
        "cookie": cookie_str,
        "marker": marker,
        "cookie_names": names,
    }))
}

/// Capture the Midjourney session from the embedded webview. With `probe_only` it just
/// reports whether an auth cookie exists (cheap, pollable after login). The full run
/// converts the cookies to Playwright format and seeds the persistent Chromium profile
/// used by the existing automation scripts, then stores `mj_profile_dir`.
#[tauri::command]
pub async fn webview_capture_mj_session(
    mgr: State<'_, super::webview::WebviewManager>,
    state: State<'_, AppState>,
    probe_only: Option<bool>,
) -> Res<Value> {
    let urls = ["https://www.midjourney.com/", "https://midjourney.com/"];
    let cookies = webview_cookies_for(&mgr, &urls)?;
    let names: Vec<String> = cookies.iter().map(|c| c.name().to_string()).collect();

    // Same auth-cookie heuristics as midjourney-session-capture.js.
    let auth_re = regex::Regex::new(
        r"(?i)next-auth\.session-token|__Secure-next-auth\.session-token|session-token|midjourney|mj_session",
    )
    .map_err(e)?;
    let auth = cookies.iter().find(|c| auth_re.is_match(c.name()));
    let Some(auth) = auth else {
        return Ok(serde_json::json!({
            "ok": false,
            "detail": "No Midjourney auth cookie in the webview yet — finish signing in on the Midjourney page first.",
            "cookie_names": names,
        }));
    };
    if probe_only.unwrap_or(false) {
        return Ok(serde_json::json!({
            "ok": true,
            "probe": true,
            "auth_cookie_name": auth.name(),
            "cookie_names": names,
        }));
    }

    // Playwright addCookies() format.
    let pw_cookies: Vec<Value> = cookies
        .iter()
        .map(|c| {
            let expires = c
                .expires()
                .and_then(|x| x.datetime())
                .map(|dt| dt.unix_timestamp() as f64)
                .unwrap_or(-1.0);
            serde_json::json!({
                "name": c.name(),
                "value": c.value(),
                "domain": c.domain().map(|d| d.to_string()).unwrap_or_else(|| ".midjourney.com".to_string()),
                "path": c.path().unwrap_or("/"),
                "expires": expires,
                "httpOnly": c.http_only().unwrap_or(false),
                "secure": c.secure().unwrap_or(true),
                "sameSite": c.same_site().map(|s| s.to_string()).unwrap_or_else(|| "Lax".to_string()),
            })
        })
        .collect();

    let script = locate_resource_file("inject-cookies.js")
        .ok_or_else(|| "inject-cookies.js not found in resources".to_string())?;
    let node = resolve_node_executable()
        .ok_or_else(|| "Node.js is required to seed the Midjourney automation profile.".to_string())?;

    let profile_dir = env::temp_dir().join("biblemusically-midjourney-playwright-profile");
    fs::create_dir_all(&profile_dir).await.map_err(e)?;
    let cookies_path = env::temp_dir().join(format!("bm-mj-cookies-{}.json", Uuid::new_v4()));
    fs::write(&cookies_path, serde_json::to_vec(&pw_cookies).map_err(e)?)
        .await
        .map_err(e)?;

    let output = tokio::process::Command::new(node)
        .arg(script.to_string_lossy().to_string())
        .arg(profile_dir.to_string_lossy().to_string())
        .arg(cookies_path.to_string_lossy().to_string())
        .output()
        .await
        .map_err(|err| format!("Failed to run cookie injection: {}", err))?;
    let _ = fs::remove_file(&cookies_path).await;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !output.status.success() {
        return Err(format!(
            "Seeding the Midjourney profile failed: {}",
            if stderr.is_empty() { stdout } else { stderr }
        ));
    }
    let injected: Value = serde_json::from_str(&stdout)
        .map_err(|err| format!("Failed to parse inject output: {}\nstdout={}\nstderr={}", err, stdout, stderr))?;
    if !injected["ok"].as_bool().unwrap_or(false) {
        return Err(injected["detail"].as_str().unwrap_or("cookie injection failed").to_string());
    }

    let coll = state.db.collection::<Document>("settings");
    coll.update_one(
        doc! { "_id": "singleton" },
        doc! { "$set": { "mj_profile_dir": profile_dir.to_string_lossy().to_string() } },
    )
    .await
    .map_err(e)?;

    Ok(serde_json::json!({
        "ok": true,
        "method": "webview",
        "auth_cookie_name": auth.name(),
        "cookie_names": names,
        "profile_dir": profile_dir.to_string_lossy(),
        "injected": injected,
    }))
}

#[tauri::command]
pub async fn probe_node() -> Res<Value> {
    // Return the resolved node executable path if found
    if let Some(p) = resolve_node_executable() {
        return Ok(serde_json::json!({ "ok": true, "path": p }));
    }
    Ok(serde_json::json!({ "ok": false, "error": "Node.js not found" }))
}

#[tauri::command]
pub async fn generate_mj_now(state: State<'_, AppState>, prompt: String) -> Res<Value> {
    // Immediate generation via Playwright generator script. Returns saved image paths.
    let script = locate_resource_file("midjourney-generator.js").ok_or_else(|| "Generator script not found".to_string())?;
    let node = resolve_node_executable().ok_or_else(|| "Node.js is required to run generator".to_string())?;

    // Read mj_profile_dir from settings (use Playwright persistent profile)
    let coll = state.db.collection::<Document>("settings");
    let sdoc = coll.find_one(doc! { "_id": "singleton" }).await.map_err(e)?.unwrap_or_default();
    let s = bson_to_value(sdoc);
    let mj_profile = s["mj_profile_dir"].as_str().unwrap_or("").to_string();
    if mj_profile.trim().is_empty() {
        return Err("mj_profile_dir is not configured. Capture a session first.".to_string());
    }

    // Create an outputs directory in current working dir
    let out_base = std::env::current_dir().map(|d| d.join("outputs").join("midjourney")).unwrap_or_else(|_| std::path::PathBuf::from("./outputs/midjourney"));
    let out_dir = out_base.join(Uuid::new_v4().to_string());
    let _ = tokio::fs::create_dir_all(&out_dir).await;

    let mut cmd = tokio::process::Command::new(node);
    cmd.arg(script.to_string_lossy().to_string())
        .arg("--prompt").arg(&prompt)
        .arg("--profile").arg(mj_profile)
        .arg("--outdir").arg(out_dir.to_string_lossy().to_string())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let output = cmd.output().await.map_err(|err| format!("Failed to spawn generator: {}", err))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!("Generator failed: {}", stderr));
    }
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let parsed: Result<Vec<String>, _> = serde_json::from_str(&stdout);
    match parsed {
        Ok(v) => Ok(serde_json::json!({ "ok": true, "paths": v })),
        Err(e) => Err(format!("Failed to parse generator output: {}", e)),
    }
}

#[tauri::command]
pub async fn ensure_mj_autostart(state: State<'_, AppState>) -> Res<Value> {
    ensure_mj_autostart_internal(&state.db).await
}

#[tauri::command]
pub async fn mj_auto_login(state: State<'_, AppState>, login_account: String, login_password: String, login_2fa: String) -> Res<Value> {
    // Determine proxy URL (env > settings)
    let coll = state.db.collection::<Document>("settings");
    let sdoc = coll.find_one(doc! { "_id": "singleton" }).await.map_err(e)?.unwrap_or_default();
    let s = bson_to_value(sdoc);
    let proxy_env = std::env::var("MJ_PROXY_URL").ok();
    let proxy = proxy_env.as_deref().unwrap_or_else(|| s["mj_proxy_url"].as_str().unwrap_or("")).trim().to_string();
    if proxy.is_empty() {
        return Ok(serde_json::json!({ "ok": false, "error": "proxy_missing", "detail": "Midjourney proxy URL not configured or not auto-started." }));
    }

    // Start a temporary local callback server to receive the proxy notification
    let secret = Uuid::new_v4().to_string();
    let callback_secret = secret.clone();
    let (tx, mut rx) = mpsc::channel::<serde_json::Value>(1);
    let tx_filter = warp::any().map(move || tx.clone());

    let notify_route = warp::post()
        .and(warp::path("mj")).and(warp::path("admin")).and(warp::path("account-login-notify"))
        .and(warp::body::json())
        .and(tx_filter.clone())
        .and_then(move |body: serde_json::Value, tx: mpsc::Sender<serde_json::Value>| {
            let secret = callback_secret.clone();
            async move {
                if body.get("Secret").and_then(|v| v.as_str()) != Some(&secret) {
                    // ignore callbacks that do not match the request secret
                    return Ok::<_, std::convert::Infallible>(warp::reply::with_status("OK", warp::http::StatusCode::OK));
                }
                let _ = tx.try_send(body);
                Ok::<_, std::convert::Infallible>(warp::reply::with_status("OK", warp::http::StatusCode::OK))
            }
        });

    // Bind to an ephemeral port on localhost
    let (addr, server) = warp::serve(notify_route).bind_ephemeral(([127, 0, 0, 1], 0));
    tokio::task::spawn(server);
    let port = addr.port();
    let notify_hook = format!("http://127.0.0.1:{}", port);

    // Prepare auto-login payload
    let body = serde_json::json!({
        "LoginAccount": login_account,
        "LoginPassword": login_password,
        "Login2fa": login_2fa,
        "State": "app_mj_autologin",
        "Secret": secret,
        "NotifyHook": notify_hook
    });

    let client = reqwest::Client::new();
    let url = format!("{}/login/auto", proxy.trim_end_matches('/'));
    let resp = client.post(&url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            // wait up to 120s for callback
            match tokio::time::timeout(std::time::Duration::from_secs(120), rx.recv()).await {
                Ok(Some(payload)) => {
                    // if success and token present, persist
                    let success = payload.get("Success").and_then(|v| v.as_bool()).unwrap_or(false);
                    let token = payload.get("Token").and_then(|v| v.as_str()).map(|s| s.to_string());
                    if success && token.is_some() {
                        let tok = token.unwrap();
                        let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "mj_discord_token": tok.clone() } }).await;
                        return Ok(serde_json::json!({ "ok": true, "token_stored": true }));
                    } else {
                        return Ok(serde_json::json!({ "ok": false, "detail": payload }));
                    }
                }
                Ok(None) => {
                    return Ok(serde_json::json!({ "ok": false, "error": "no_callback", "detail": "Proxy did not POST back within timeout." }));
                }
                Err(_) => {
                    return Ok(serde_json::json!({ "ok": false, "error": "timeout", "detail": "Timed out waiting for proxy callback." }));
                }
            }
        }
        Ok(r) => {
            return Ok(serde_json::json!({ "ok": false, "status": r.status().as_u16(), "detail": "Proxy returned non-OK" }));
        }
        Err(err) => {
            return Ok(serde_json::json!({ "ok": false, "error": "request_failed", "detail": format!("{:#}", err) }));
        }
    }
}
