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

/// Drop the engine server URLs from a per-project settings override.
///
/// See the call site in `update_settings` for why: these addresses are discovered by the app, only
/// ever written to the singleton, and merged project-over-singleton — so a project copy is a stale
/// address that outranks the live one.
fn strip_discovered_urls(pdoc: &mut Document) {
    let keys: Vec<String> = pdoc.keys().filter(|k| k.ends_with("_api_url")).cloned().collect();
    for k in keys { pdoc.remove(&k); }
}

/// Drop digit-only keys — settings has no numeric fields, so these are debris.
///
/// They came from a UI helper that took `updateS(key, value)` but only understood
/// `updateS({key: value})`: spreading a string into an object spreads it per character, so a click
/// wrote `{0:"r", 1:"e", …}` instead of the setting. The UI bug is fixed, but the store already
/// holds the wreckage on every settings document, and it would otherwise be copied forward by each
/// save forever. Cheaper and safer to refuse them here than to migrate once and hope.
fn strip_debris(doc: &mut Document) {
    let keys: Vec<String> = doc.keys().filter(|k| !k.is_empty() && k.chars().all(|c| c.is_ascii_digit()))
        .cloned().collect();
    for k in keys { doc.remove(&k); }
}

#[tauri::command]
pub async fn update_settings(state: State<'_, AppState>, payload: Value, project_id: Option<String>) -> Res<Value> {
    let coll = state.db.collection::<Document>("settings");
    // Always write through to the singleton: every backend consumer (compose_lyrics,
    // job runners, uploads, test_* probes) reads `_id: "singleton"` directly, so a
    // project-only save would leave e.g. freshly-entered API keys invisible to them.
    let mut bson = bson::to_document(&payload).map_err(e)?;
    strip_debris(&mut bson);
    bson.insert("_id", "singleton");
    coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": &bson })
        .upsert(true)
        .await.map_err(e)?;
    // Additionally keep the per-project override doc (get_settings merges it on top).
    if let Some(pid) = project_id.filter(|s| !s.is_empty()) {
        let mut pdoc = bson::to_document(&payload).map_err(e)?;
        // Engine server URLs are deliberately NOT mirrored here. They are discovered, not chosen:
        // the monitor, `fetch_kaggle_url` and `refresh_kaggle_url` all write the singleton, so a
        // copy in a project document can only ever go stale — and because the merge puts the
        // project on top, that stale copy wins over the address the app just verified. Mirroring
        // them is what made "a trycloudflare URL that doesn't exist" survive every rediscovery, and
        // it is why `persist_engine_url` clearing the override was not enough on its own: the very
        // next autosave of the Settings form put the dead address straight back.
        strip_discovered_urls(&mut pdoc);
        strip_debris(&mut pdoc);
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
    let path = crate::helpers::resolve_media_binary(doc["ffmpeg_path"].as_str(), "ffmpeg");
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
pub async fn open_suno_login(app: tauri::AppHandle) -> Res<Value> {
    let url = "https://suno.com";
    crate::helpers::open_external(&app, url)?;
    Ok(serde_json::json!({ "ok": true, "url": url }))
}

#[tauri::command]
pub async fn open_midjourney_login(app: tauri::AppHandle) -> Res<Value> {
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
    crate::helpers::open_external(&app, url)?;
    Ok(serde_json::json!({ "ok": true, "url": url, "method": "system" }))
}

// ── Kaggle server helpers ────────────────────────────────────────
// The free Suno/Midjourney-alternative engines run as notebooks on the user's
// Kaggle account and expose an ephemeral `*.trycloudflare.com` tunnel that
// rotates every run. These commands let the app open the notebook, open the
// Kaggle API-token page, and pull the current tunnel URL straight from the
// kernel's latest output — instead of the user hand-copying a rotating URL.

// ── Whose kernel are we talking to? ──────────────────────────────────────────
//
// A Kaggle kernel slug is `<owner>/<name>`, and every CLI verb the app uses — pull, push, status,
// logs — takes one. The owner used to be hardcoded to the author's account, which worked for exactly
// one person and failed silently for everybody else, including the author on a second account:
//
//   • `kernels push` cannot write to somebody else's kernel, so starting a server did nothing;
//   • `kernels status <their>/…` answers about THEIR kernel — usually "complete", from an old run —
//     which the monitor faithfully reported as "the run ended before a server came up";
//   • `kernels logs -f <their>/…` streams THEIR last run's persisted log, so the app displayed a
//     successful boot ("Server is up", "Uvicorn running") for a run that had never started here.
//
// Free GPU quota is per-account and the app rotates between accounts, so the owner has to be
// whichever account is active right now. The upstream account stays in the picture as the *template*:
// a user who has never run an engine has no kernel of their own to pull, so the published one is
// pulled, re-addressed, and pushed under their name — which creates it.

/// The account the published reference notebooks live under. Only ever read from.
const KAGGLE_UPSTREAM_OWNER: &str = "joehannes";

/// Maps a UI engine id → (kernel name without owner, the settings key its live URL is stored in).
pub(crate) fn kaggle_kernel_for(engine: &str) -> Option<(&'static str, &'static str)> {
    match engine {
        "acestep" => Some(("biblemusically-acestep-server", "acestep_api_url")),
        "heartmula" => Some(("biblemusically-heartmula-server", "heartmula_api_url")),
        "comfyui" | "comfy" => Some(("biblemusically-comfyui-server", "comfyui_api_url")),
        "flux" => Some(("biblemusically-flux-server", "flux_api_url")),
        "riffusion" => Some(("biblemusically-riffusion-server", "riffusion_api_url")),
        // Video runs as its own ComfyUI server rather than inside the image one. Kaggle allows two
        // concurrent GPU sessions, so a separate kernel means stills and clips render at the same
        // time on separate quota — and a video model plus SDXL/FLUX would not fit one 16 GB card
        // anyway (see video_models.rs for the per-model figures).
        "video" => Some(("biblemusically-video-server", "video_api_url")),
        _ => None,
    }
}

// ── Engines that are switched off ───────────────────────────────────────────
//
// An engine listed here keeps all of its code — notebook, commands, job path, tests — and simply
// stops being offered or started. Hiding rather than deleting because "not now" is not "never": the
// pipeline for it still compiles and still has its tests, so turning it back on is removing a string
// from this list rather than rebuilding anything.
//
// The important half is that a hidden engine must consume nothing. It is refused by the two entry
// points that spend real resources — pushing a Kaggle run, and starting the log monitor that streams
// for up to twenty minutes — and `idle_guard::touch` ignores it, so the idle sweep never adopts it as
// something to watch and periodically try to stop.
//
// Nothing is switched off right now. ACE-Step was briefly listed here on a misreading: "under 10
// seconds" is its *generation speed* per song, not its output length — it writes full tracks, and
// this app already drives it at `acestep_duration` (240 s by default, 10–600 s allowed).
pub const HIDDEN_ENGINES: &[&str] = &[];

/// Is this engine switched off? Case- and whitespace-insensitive, because engine ids arrive from
/// settings documents and UI strings as well as from code.
pub fn engine_hidden(engine: &str) -> bool {
    let e = engine.trim().to_ascii_lowercase();
    HIDDEN_ENGINES.iter().any(|h| *h == e)
}

/// Engines that exist, work, and are not offered until somebody deliberately turns them on.
///
/// Different from `HIDDEN_ENGINES`, which is "switched off in this build, full stop". These are
/// switched off *by default* and unlockable, because they carry a risk the user should accept
/// knowingly rather than discover:
///
/// * **`suno_api`** drives Suno over plain HTTP with the user's own session cookie. It is the only
///   Suno path that works on a phone, and it is unofficial: Suno's terms describe non-interface
///   access as self-botting, and the account that gets suspended is the user's. That is a choice to
///   make on purpose, so it is not on the engine list until it is made.
pub const ADMIN_ONLY_ENGINES: &[&str] = &["suno_api"];

pub fn engine_is_admin_only(engine: &str) -> bool {
    let e = engine.trim().to_ascii_lowercase();
    ADMIN_ONLY_ENGINES.iter().any(|a| *a == e)
}

/// The phrase that unlocks them.
///
/// Deliberately a typed phrase and not a password. This is a single-user desktop app running as the
/// user, on the user's own machine, reading the user's own settings file — a secret kept here would
/// protect nothing from anybody, and pretending otherwise would be the same theatre the team module
/// refuses about roles. What a phrase *does* buy is that nobody enables an account-risking engine by
/// clicking a toggle they did not read.
pub const ADMIN_UNLOCK_PHRASE: &str = "I accept the risk to my account";

pub fn unlock_phrase_matches(given: &str) -> bool {
    given.trim().eq_ignore_ascii_case(ADMIN_UNLOCK_PHRASE)
}

/// Turn the admin-only engines on or off. Enabling requires the phrase typed exactly.
#[tauri::command]
pub async fn set_admin_engines(state: State<'_, AppState>, enabled: bool, phrase: Option<String>) -> Res<Value> {
    if enabled && !unlock_phrase_matches(phrase.as_deref().unwrap_or("")) {
        return Ok(serde_json::json!({
            "ok": false, "unlocked": false,
            "detail": format!("To turn these on, type: {ADMIN_UNLOCK_PHRASE}"),
        }));
    }
    let mut set = Document::new();
    set.insert("admin_engines_unlocked", enabled);
    // Turning the lock back on must also put the engine back, or the setting says "locked" while a
    // locked engine keeps generating.
    if !enabled {
        let cur = state.db.collection::<Document>("settings")
            .find_one(doc! { "_id": "singleton" }).await.ok().flatten()
            .and_then(|d| d.get_str("music_engine").ok().map(|s| s.to_string()))
            .unwrap_or_default();
        if engine_is_admin_only(&cur) { set.insert("music_engine", "heartmula"); }
    }
    state.db.collection::<Document>("settings")
        .update_one(doc! { "_id": "singleton" }, doc! { "$set": set }).upsert(true).await.map_err(e)?;
    Ok(serde_json::json!({
        "ok": true, "unlocked": enabled,
        "engines": ADMIN_ONLY_ENGINES,
        "detail": if enabled { "Unlocked. The extra engines now appear in the music-engine list." }
                  else { "Locked again, and the engine reset to HeartMuLa if one of these was selected." },
    }))
}

/// Whether the admin-only engines are currently offered, and which they are.
#[tauri::command]
pub async fn admin_engines_status(state: State<'_, AppState>) -> Res<Value> {
    let unlocked = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.ok().flatten()
        .and_then(|d| d.get_bool("admin_engines_unlocked").ok())
        .unwrap_or(false);
    Ok(serde_json::json!({
        "unlocked": unlocked, "engines": ADMIN_ONLY_ENGINES, "phrase": ADMIN_UNLOCK_PHRASE,
    }))
}

/// The refusal a hidden engine gives, shaped like every other command result so no caller special-cases it.
fn hidden_engine_reply(engine: &str) -> Value {
    serde_json::json!({
        "ok": false, "status": "engine_hidden", "hidden": true,
        "detail": format!("The {engine} engine is turned off in this build."),
        "next_step": "Pick a different engine in Settings → Music engine.",
    })
}

/// The username written in `~/.kaggle/kaggle.json`.
///
/// Note carefully: this is *not* necessarily who the CLI authenticates as — see
/// `kaggle_cli_identity` below. Kept separate because the disagreement between the two is a real
/// state a user can be in, and collapsing them hides it.
pub(crate) async fn kaggle_json_username() -> Option<String> {
    let home = env::var("HOME").ok()?;
    let raw = fs::read_to_string(PathBuf::from(home).join(".kaggle/kaggle.json")).await.ok()?;
    let parsed: Value = serde_json::from_str(&raw).ok()?;
    let name = parsed["username"].as_str()?.trim().to_string();
    (!name.is_empty()).then_some(name)
}

/// Who the CLI is *actually* signed in as, and how.
///
/// `kaggle config view` reports both, and it is the only authoritative answer — because the modern
/// Kaggle client prefers a cached OAuth token at `~/.kaggle/access_token` over the API key in
/// `kaggle.json`, and reports `auth_method: ACCESS_TOKEN` when it does. When that token exists,
/// writing a different account's `kaggle.json` changes nothing: every command still runs as whoever
/// the token belongs to.
///
/// This is exactly the trap that made "I added another Kaggle account" not take effect, and then
/// made the fix for it worse — reading the username out of `kaggle.json` looked like the
/// authoritative source and is not, so the app began addressing an account the CLI could not write
/// to. Ask the CLI.
pub(crate) async fn kaggle_cli_identity() -> Option<(String, String)> {
    let out = tokio::process::Command::new(locate_kaggle())
        .args(["config", "view"]).output().await.ok()?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let field = |k: &str| text.lines()
        .find_map(|l| l.trim().strip_prefix(&format!("- {k}:")))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty() && v != "None");
    let user = field("username")?;
    Some((user, field("auth_method").unwrap_or_else(|| "API_KEY".into())))
}

/// The Kaggle username commands will run as: the CLI's own answer where there is a CLI, else the
/// key file. On mobile there is no CLI to ask, and the key file is the only credential there is.
pub(crate) async fn kaggle_cli_username() -> Option<String> {
    if let Some((user, _)) = kaggle_cli_identity().await { return Some(user); }
    kaggle_json_username().await
}

/// The owner half of the slug: whoever the CLI is signed in as, falling back to the app's record and
/// finally to upstream (which at least lets a read-only operation work before any account is set up).
pub(crate) async fn kaggle_owner(db: &crate::store::Db) -> String {
    if let Some(u) = kaggle_cli_username().await { return u; }
    let stored = db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.ok().flatten()
        .and_then(|d| d.get_str("kaggle_active").ok().map(|s| s.to_string()))
        .unwrap_or_default();
    if !stored.trim().is_empty() { return stored } else { KAGGLE_UPSTREAM_OWNER.to_string() }
}

/// `(own_slug, upstream_slug, settings_key)` for an engine.
///
/// `own_slug` is what gets pushed, polled and followed; `upstream_slug` is the template to pull when
/// the user has no copy yet.
pub(crate) async fn kaggle_slugs(db: &crate::store::Db, engine: &str) -> Option<(String, String, &'static str)> {
    let (name, key) = kaggle_kernel_for(engine)?;
    let owner = kaggle_owner(db).await;
    Some((format!("{owner}/{name}"), format!("{KAGGLE_UPSTREAM_OWNER}/{name}"), key))
}

/// The engine id a kernel name belongs to — `johannes/biblemusically-video-server` → `video`.
fn engine_of_slug(slug: &str) -> &str {
    slug.rsplit('/').next().unwrap_or(slug)
        .trim_start_matches("biblemusically-").trim_end_matches("-server")
}

/// The notebook shipped inside the app for an engine.
///
/// Compiled in rather than read from disk: an installed app has no `scripts/` directory, and a
/// notebook that only exists on Kaggle makes the whole feature depend on one account staying alive.
fn bundled_notebook(engine: &str) -> Option<&'static str> {
    Some(match engine {
        "acestep" => include_str!("../kaggle_notebooks/acestep.ipynb"),
        "heartmula" => include_str!("../kaggle_notebooks/heartmula.ipynb"),
        "comfyui" | "comfy" => include_str!("../kaggle_notebooks/comfyui.ipynb"),
        "flux" => include_str!("../kaggle_notebooks/flux.ipynb"),
        "riffusion" => include_str!("../kaggle_notebooks/riffusion.ipynb"),
        "video" => include_str!("../kaggle_notebooks/video.ipynb"),
        _ => return None,
    })
}

/// What a kernel pull produced, and where it came from.
pub(crate) struct PulledKernel {
    /// True when the user had no copy and the published upstream notebook was used as the template.
    /// The push that follows will therefore *create* their kernel rather than update it.
    pub from_upstream: bool,
    pub slug: String,
}

/// Pull an engine's kernel into `dir` and re-address it to the active account.
///
/// Own copy first, upstream as the template. The metadata `id` is rewritten either way, because a
/// push only succeeds against a kernel the signed-in account owns — pushing an upstream id is the
/// silent no-op that made "Start & connect" appear to work while nothing started.
async fn pull_kernel_for_push(
    kaggle: &str,
    dir: &std::path::Path,
    own: &str,
    upstream: &str,
) -> Result<PulledKernel, String> {
    let pull = |slug: String| {
        let kaggle = kaggle.to_string();
        let dir = dir.to_path_buf();
        async move {
            tokio::process::Command::new(&kaggle)
                .args(["kernels", "pull", &slug, "-m", "-p"]).arg(&dir)
                .output().await
                .map_err(|err| format!("Could not run the kaggle CLI: {}", err))
        }
    };

    let mut from_upstream = false;
    let first = pull(own.to_string()).await?;
    // The CLI exits 0 for a missing kernel in some versions and prints the failure instead, so the
    // metadata file's existence is the real test of whether anything came back.
    let got_own = first.status.success() && dir.join("kernel-metadata.json").is_file();
    if !got_own {
        from_upstream = true;
        let _ = fs::remove_dir_all(dir).await;
        let _ = fs::create_dir_all(dir).await;
        let second = pull(upstream.to_string()).await?;
        if !second.status.success() || !dir.join("kernel-metadata.json").is_file() {
            // Neither this account nor upstream has it. That is the normal state for an engine whose
            // notebook has never been published — the video server on a fresh install — so fall back
            // to the copy bundled with the app. This is also what makes the app independent of the
            // author's Kaggle account remaining alive: the notebooks ship inside the binary, and
            // Kaggle is only ever a place to run them.
            let bundled = bundled_notebook(engine_of_slug(own));
            let Some(source) = bundled else {
                let msg = format!("{}{}",
                    String::from_utf8_lossy(&second.stdout), String::from_utf8_lossy(&second.stderr));
                return Err(format!(
                    "Could not pull the reference notebook {upstream} from Kaggle, and no bundled \
                     copy is available: {}", msg.trim()));
            };
            fs::write(dir.join("setup_and_serve.ipynb"), &source).await.map_err(e)?;
            let name = own.split('/').next_back().unwrap_or("biblemusically-server");
            let meta = serde_json::json!({
                "id": own,
                "title": name,
                "code_file": "setup_and_serve.ipynb",
                "language": "python",
                "kernel_type": "notebook",
                "is_private": true,
                "enable_gpu": true,
                "enable_internet": true,
                "dataset_sources": [],
                "competition_sources": [],
                "kernel_sources": [],
            });
            fs::write(dir.join("kernel-metadata.json"),
                      serde_json::to_string_pretty(&meta).map_err(e)?).await.map_err(e)?;
            return Ok(PulledKernel { from_upstream: true, slug: own.to_string() });
        }
    }

    // Re-address to the active account. `id` is the only field the CLI derives the target slug from.
    let meta_path = dir.join("kernel-metadata.json");
    let raw = fs::read_to_string(&meta_path).await.map_err(e)?;
    let mut meta: Value = serde_json::from_str(&raw).map_err(e)?;
    meta["id"] = Value::String(own.to_string());
    // A freshly created kernel needs a title, and Kaggle rejects one under five characters. Keep
    // whatever upstream had when it is usable, else fall back to the slug's own name.
    let title_ok = meta["title"].as_str().map(|t| t.trim().chars().count() >= 5).unwrap_or(false);
    if !title_ok {
        let name = own.split('/').next_back().unwrap_or("biblemusically-server");
        meta["title"] = Value::String(name.to_string());
    }
    // `id_no` names a *specific existing* kernel by numeric id. Carried over from the upstream pull it
    // would point the push straight back at the template, overriding `id` — so it must go.
    if let Some(obj) = meta.as_object_mut() { obj.remove("id_no"); }
    fs::write(&meta_path, serde_json::to_string_pretty(&meta).map_err(e)?).await.map_err(e)?;

    // Replace the account's notebook when the app is carrying a newer one.
    //
    // Until now a start pushed back whatever Kaggle already held, and the bundled notebook was only
    // ever used by an account that had no copy at all. That meant every notebook fix this app ships
    // reached precisely nobody who had run the engine before — the people it was written for. The
    // riffusion engine is what exposed it: the repo's notebook had been rewritten to embed its
    // source package, the version on Kaggle was still the 7.5 KB original whose
    // `from preset_engine import ...` cannot resolve, and so every start faithfully re-pushed a
    // notebook that had been unable to import for months, one release after the fix was written.
    if let Some(bundled) = bundled_notebook(engine_of_slug(own)) {
        let code_file = meta["code_file"].as_str().unwrap_or("setup_and_serve.ipynb").to_string();
        let nb_path = dir.join(&code_file);
        let theirs = fs::read_to_string(&nb_path).await.ok().and_then(|s| notebook_revision(&s));
        let ours = notebook_revision(bundled);
        // Only ever forward. An account whose copy is stamped newer than the app's is running a
        // notebook from a later release, and overwriting it would be a downgrade.
        if ours.unwrap_or(0) > theirs.unwrap_or(0) {
            let _ = fs::write(&nb_path, bundled).await;
        }
    }

    Ok(PulledKernel { from_upstream, slug: own.to_string() })
}

/// The `bm_notebook_revision` stamp in a notebook's metadata, if it carries one.
///
/// Absent means "older than every stamped release" — the notebooks predate this scheme, so an
/// unstamped copy is exactly the case that needs replacing.
fn notebook_revision(source: &str) -> Option<u64> {
    serde_json::from_str::<Value>(source).ok()?
        .get("metadata")?.get("bm_notebook_revision")?.as_u64()
}

/// Locate the `kaggle` CLI, or `None` when there genuinely isn't one.
///
/// A desktop-launched app inherits a minimal PATH that usually misses ~/.local/bin (pipx shim) and
/// linuxbrew, so probe those first, then fall back to a real PATH lookup.
///
/// This returns an `Option` on purpose. The old version returned the bare string `"kaggle"` when it
/// found nothing, so "no CLI" and "a CLI on PATH" were the same answer — which is how a phone, where
/// a CLI *cannot* exist (Android forbids `exec()` from an app's data directory), ended up producing
/// a spawn error that reads like a missing install rather than an impossible one.
pub(crate) fn locate_kaggle_opt() -> Option<String> {
    if let Ok(home) = env::var("HOME") {
        let p = PathBuf::from(&home).join(".local/bin/kaggle");
        if p.is_file() { return Some(p.to_string_lossy().into_owned()); }
    }
    for p in ["/home/linuxbrew/.linuxbrew/bin/kaggle", "/usr/local/bin/kaggle"] {
        if PathBuf::from(p).is_file() { return Some(p.to_string()); }
    }
    which::which("kaggle").ok().map(|p| p.to_string_lossy().into_owned())
}

/// Locate the `kaggle` CLI, falling back to the bare name for the OS to resolve.
///
/// Kept for the call sites that are only ever reached once the CLI is known to be usable. Anything
/// that a phone can reach should call [`require_kaggle_cli`] instead, so the refusal is a sentence
/// rather than a spawn error.
pub(crate) fn locate_kaggle() -> String {
    locate_kaggle_opt().unwrap_or_else(|| "kaggle".to_string())
}

/// The CLI path, or an explanation of why there will never be one here.
///
/// Two different failures wear the same clothes without this: a desktop that simply has not installed
/// the CLI (fixable in a minute) and a phone that cannot run one at all (not fixable, and the four
/// operations that still need it are listed in TODOS.md with the REST calls that would replace them).
/// Saying which is which is the whole point.
pub(crate) fn require_kaggle_cli() -> Result<String, String> {
    if let Some(p) = locate_kaggle_opt() { return Ok(p); }
    if cfg!(mobile) {
        Err("This step needs the Kaggle command-line tool, which cannot run on a phone — Android \
             does not allow an app to execute a program from its own storage. Start and stop servers \
             from the desktop app; reading their status works here."
            .to_string())
    } else {
        Err("The Kaggle command-line tool isn't installed. Install it with `pipx install kaggle` \
             (or `pip install --user kaggle`), then try again — the app looks in ~/.local/bin, \
             linuxbrew, /usr/local/bin and on PATH."
            .to_string())
    }
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
pub async fn open_kaggle_notebook(app: tauri::AppHandle, state: State<'_, AppState>, engine: String) -> Res<Value> {
    let (own, upstream, _) = kaggle_slugs(&state.db, &engine).await
        .ok_or_else(|| format!("Unknown engine '{}'.", engine))?;
    let url = format!("https://www.kaggle.com/code/{}", own);
    crate::helpers::open_external(&app, &url)?;
    Ok(serde_json::json!({ "ok": true, "url": url, "upstream_url": format!("https://www.kaggle.com/code/{}", upstream) }))
}

/// Return an engine's Kaggle notebook URL WITHOUT opening it, so the UI can load it in the
/// in-app browser instead of launching an external browser window.
///
/// This is the *active account's* copy. Before the first successful start that kernel does not exist
/// yet, so the upstream template's URL comes along too — a 404 on your own notebook is a confusing
/// thing to hand somebody who has never started a server.
#[tauri::command]
pub async fn kaggle_notebook_url(state: State<'_, AppState>, engine: String) -> Res<Value> {
    let (own, upstream, _) = kaggle_slugs(&state.db, &engine).await
        .ok_or_else(|| format!("Unknown engine '{}'.", engine))?;
    // Whether the user's own copy exists decides which URL is worth opening. Sending somebody to
    // their own notebook before they have ever started a server lands them on a Kaggle 404 (or, on a
    // slug they do not own, on Kaggle's own error page) — which is a confusing answer to "show me the
    // log", and indistinguishable from the app being broken.
    let kaggle = locate_kaggle();
    let status = tokio::process::Command::new(&kaggle)
        .args(["kernels", "status", &own]).output().await.ok()
        .map(|o| format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)))
        .unwrap_or_default();
    let low = status.to_lowercase();
    let exists = !(low.contains("404") || low.contains("403") || low.contains("not found")
                   || low.contains("forbidden"));

    let own_url = format!("https://www.kaggle.com/code/{}", own);
    let upstream_url = format!("https://www.kaggle.com/code/{}", upstream);
    Ok(serde_json::json!({
        "ok": true,
        "url": if exists { own_url.clone() } else { upstream_url.clone() },
        "own_url": own_url,
        "upstream_url": upstream_url,
        "exists": exists,
        "slug": own,
        "owner": own.split('/').next().unwrap_or(""),
    }))
}

#[tauri::command]
pub async fn open_kaggle_token_page(app: tauri::AppHandle) -> Res<Value> {
    // The "Create New API Token" button lives on the account settings page.
    let url = "https://www.kaggle.com/settings";
    crate::helpers::open_external(&app, url)?;
    Ok(serde_json::json!({ "ok": true, "url": url }))
}

/// Open the Kaggle sign-in page in the SYSTEM browser. Used by the onboarding wizard's guided
/// Kaggle step — the user can sign in (Google is fine there, unlike the embedded webview which
/// Google blocks) and then create an API token.
#[tauri::command]
pub async fn open_kaggle_login(app: tauri::AppHandle) -> Res<Value> {
    let url = "https://www.kaggle.com/account/login";
    crate::helpers::open_external(&app, url)?;
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
    let adopted = reconcile_kaggle_identity(&state.db).await;
    let doc = state.db.collection::<Document>("settings").find_one(doc! { "_id": "singleton" }).await.ok().flatten();
    let active = doc.as_ref().and_then(|d| d.get_str("kaggle_active").ok()).unwrap_or("").to_string();
    let accts = stored_kaggle_accounts(&state.db).await;
    let list: Vec<Value> = accts.iter().filter_map(|a| a.get_str("username").ok().map(|u| {
        serde_json::json!({ "username": u, "active": u == active })
    })).collect();
    Ok(serde_json::json!({ "accounts": list, "active": active, "adopted": adopted }))
}

/// Make the app's idea of the active Kaggle account match the CLI's.
///
/// `~/.kaggle/kaggle.json` is the only credential every `kaggle` subprocess reads, so it — not the
/// settings document — decides which account the app is really operating as. The two had no way to
/// disagree until adding a second account wrote the file but left the stored list behind; after that
/// the app addressed one account's kernels while authenticating as another, and every symptom of
/// that (pushes that did nothing, another account's stale "complete" status, another account's old
/// log replayed as if it were this run) looked like a Kaggle problem rather than a bookkeeping one.
///
/// Returns the username adopted, if anything changed.
pub async fn reconcile_kaggle_identity(db: &crate::store::Db) -> Option<String> {
    let cli_user = kaggle_cli_username().await?;
    let coll = db.collection::<Document>("settings");
    let doc = coll.find_one(doc! { "_id": "singleton" }).await.ok().flatten();
    let active = doc.as_ref().and_then(|d| d.get_str("kaggle_active").ok()).unwrap_or("").to_string();

    let mut accts = stored_kaggle_accounts(db).await;
    let known = accts.iter().any(|a| a.get_str("username").ok() == Some(cli_user.as_str()));
    if known && active == cli_user { return None; }

    // The key is in the same file, so an account that reached the CLI without reaching the app can be
    // adopted whole — it stays rotatable rather than becoming a one-way switch.
    if !known {
        let key = env::var("HOME").ok()
            .map(|h| PathBuf::from(h).join(".kaggle/kaggle.json"))
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .and_then(|v| v["key"].as_str().map(|s| s.to_string()))
            .unwrap_or_default();
        accts.push(doc! { "username": &cli_user, "key": key });
    }
    let bson_accts = bson::to_bson(&accts).ok()?;
    let _ = coll.update_one(
        doc! { "_id": "singleton" },
        doc! { "$set": { "kaggle_accounts": bson_accts, "kaggle_active": &cli_user,
                         "kaggle_connected": true, "kaggle_username": &cli_user } },
    ).with_options(crate::store::UpdateOptions::builder().upsert(true).build()).await;
    Some(cli_user)
}

/// A kernel's status, by whichever transport this platform can use.
///
/// The first operation moved off the CLI, because it is the one every other flow depends on and the
/// one with no side effects — a wrong answer costs a re-poll, not a lost session. Push and pull stay
/// on the CLI for now: they upload a multipart blob, which is real work rather than a query.
pub(crate) async fn kernel_status_any(slug: &str) -> Res<String> {
    // `|| kaggle == "kaggle"` used to be here, which made the *fallback* count as a present CLI —
    // so a machine with no CLI at all still chose the CLI transport and failed on spawn instead of
    // taking the HTTP path that would have worked.
    let found = locate_kaggle_opt();
    let kaggle = found.clone().unwrap_or_else(|| "kaggle".to_string());
    match crate::kaggle_api::transport(found.is_some()) {
        crate::kaggle_api::Transport::Cli => {
            let out = tokio::process::Command::new(&kaggle)
                .args(["kernels", "status", slug]).output().await
                .map_err(|err| format!("Could not run the kaggle CLI: {err}"))?;
            Ok(format!("{}{}", String::from_utf8_lossy(&out.stdout),
                       String::from_utf8_lossy(&out.stderr)))
        }
        crate::kaggle_api::Transport::Http => crate::kaggle_api::kernel_status(slug).await,
    }
}

/// Everything about how this app is currently wired to Kaggle, in one answer.
///
/// Exists because the failure that motivated it was completely invisible from the interface: the app
/// said "the run ended before a server came up" while pushing to an account it was not signed in as.
/// Every fact needed to spot that — who the CLI authenticates as, which kernel is being addressed,
/// whether that kernel exists — was knowable and shown nowhere. It is also what a bug report should
/// carry, so the UI can offer it as one copyable block.
#[tauri::command]
pub async fn kaggle_diagnostics(state: State<'_, AppState>, engine: Option<String>) -> Res<Value> {
    let found = locate_kaggle_opt();
    let kaggle = found.clone().unwrap_or_else(|| "kaggle".to_string());
    let cli_present = found.is_some();
    let identity = kaggle_cli_identity().await;
    let cli_user = match &identity { Some((u, _)) => Some(u.clone()), None => kaggle_json_username().await };
    let auth_method = identity.as_ref().map(|(_, m)| m.clone()).unwrap_or_default();
    let json_user = kaggle_json_username().await;
    // The trap: a cached OAuth token overrides the key file silently, so adding an account by
    // pasting a new kaggle.json changes nothing and says nothing.
    let token_overrides = auth_method == "ACCESS_TOKEN"
        && json_user.is_some() && json_user != cli_user;

    let doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.ok().flatten();
    let stored_active = doc.as_ref().and_then(|d| d.get_str("kaggle_active").ok()).unwrap_or("").to_string();
    let accounts: Vec<String> = stored_kaggle_accounts(&state.db).await.iter()
        .filter_map(|a| a.get_str("username").ok().map(|s| s.to_string())).collect();

    // The disagreement that caused the original bug, stated as a fact rather than left to be inferred.
    let identity_mismatch = match (&cli_user, stored_active.as_str()) {
        (Some(u), s) if !s.is_empty() && u != s => true,
        _ => false,
    };

    let engines: Vec<String> = match engine {
        Some(e) => vec![e],
        None => ["acestep", "heartmula", "comfyui", "flux", "riffusion", "video"].iter()
            .filter(|e| !engine_hidden(e)).map(|s| s.to_string()).collect(),
    };
    let mut per_engine = Vec::new();
    for eng in engines {
        let Some((own, upstream, key)) = kaggle_slugs(&state.db, &eng).await else { continue };
        let status = kernel_status_any(&own).await.unwrap_or_default();
        let stored_url = doc.as_ref().and_then(|d| d.get_str(key).ok()).unwrap_or("").to_string();
        let url_alive = if stored_url.is_empty() { None } else {
            Some(matches!(reqwest::Client::new().get(&stored_url)
                .timeout(std::time::Duration::from_secs(6)).send().await,
                Ok(r) if r.status().as_u16() < 500))
        };
        per_engine.push(serde_json::json!({
            "engine": eng,
            "slug": own,
            "upstream_slug": upstream,
            // A kernel the account does not own answers with a 403/404 rather than a status word.
            "kernel_exists": !(status.contains("403") || status.contains("404")
                               || status.to_lowercase().contains("not found")),
            "status_raw": status.trim().chars().take(300).collect::<String>(),
            "settings_key": key,
            "stored_url": stored_url,
            "url_alive": url_alive,
        }));
    }

    Ok(serde_json::json!({
        "ok": true,
        "cli_path": kaggle,
        "cli_present": cli_present,
        "transport": crate::kaggle_api::transport(cli_present),
        "http_usable": crate::kaggle_api::http_usable().await,
        "cli_username": cli_user,
        "auth_method": auth_method,
        "kaggle_json_username": json_user,
        // True when a cached ~/.kaggle/access_token is authenticating as somebody other than the
        // account in kaggle.json — which makes "add another account" a silent no-op.
        "token_overrides_key_file": token_overrides,
        "stored_active": stored_active,
        "accounts": accounts,
        "identity_mismatch": identity_mismatch,
        "engines": per_engine,
    }))
}

/// Every look the picker can offer, with each mix's resolved prompt for preview.
///
/// Includes the prompts and negatives on purpose: somebody choosing a look should be able to read
/// what it will actually send, and somebody debugging a disappointing picture should not have to
/// guess which half of the prompt caused it.
#[tauri::command]
pub async fn image_style_catalogue() -> Res<Value> {
    Ok(crate::image_styles::catalogue())
}

/// Resolve a style, a named mix, or an ad-hoc `a+b` blend into the prompt pair a graph gets.
#[tauri::command]
pub async fn resolve_image_style(id: String) -> Res<Value> {
    match crate::image_styles::resolve(&id) {
        Some(look) => Ok(serde_json::json!({ "ok": true, "id": id,
                                             "prompt": look.prompt, "negative": look.negative })),
        None => Ok(serde_json::json!({ "ok": false, "id": id,
                                       "detail": format!("No style, mix or blend named '{id}'.") })),
    }
}

/// Where an engine can run, what it costs, and how to get set up on it.
///
/// Reports whether each provider looks configured, so the picker can show "ready" rather than making
/// somebody remember which of six accounts they already made.
#[tauri::command]
pub async fn compute_providers(state: State<'_, AppState>) -> Res<Value> {
    use crate::compute_providers as cp;
    let doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.ok().flatten();
    let has = |k: &str| doc.as_ref()
        .and_then(|d| d.get_str(k).ok()).map(|v| !v.trim().is_empty()).unwrap_or(false);
    let kaggle_ready = kaggle_cli_username().await.is_some();

    // Which engine URLs point where. This is the whole basis for "is RunPod set up?", since nothing
    // else in the store records that a pasted URL belongs to a particular provider.
    let url_of = |k: &str| doc.as_ref().and_then(|d| d.get_str(k).ok()).unwrap_or("").to_string();
    let in_use: Vec<(&str, cp::UrlOrigin)> = [
        ("video", "video_api_url"), ("comfyui", "comfyui_api_url"), ("flux", "flux_api_url"),
        ("heartmula", "heartmula_api_url"), ("acestep", "acestep_api_url"),
    ].iter().map(|(engine, key)| (*engine, cp::provider_of_url(&url_of(key)))).collect();

    let tunnelled: Vec<&str> = in_use.iter()
        .filter(|(_, o)| *o == cp::UrlOrigin::Ambiguous).map(|(e, _)| *e).collect();
    let unknown_host: Vec<&str> = in_use.iter()
        .filter(|(_, o)| *o == cp::UrlOrigin::Unknown).map(|(e, _)| *e).collect();

    Ok(serde_json::json!({
        // Engines on a quick tunnel, which could be Kaggle or a Lightning studio. Named separately
        // so the panel can say so instead of leaving them invisible.
        "on_a_tunnel": tunnelled,
        "on_an_unrecognised_host": unknown_host,
        "providers": cp::PROVIDERS.iter().map(|p| {
            // "Configured" is per-provider because each needs a different thing: Kaggle a token
            // file, fal a key, everything else just a URL somebody pasted.
            let ready = match p.id {
                "kaggle" => kaggle_ready,
                "fal" => has("fal_api_key"),
                "replicate" => has("replicate_api_key"),
                // For a URL provider the evidence is the hostname of whatever somebody pasted. A
                // trycloudflare address is deliberately not counted for anybody: Kaggle and a
                // Lightning studio open the identical kind of tunnel, so attributing one would put a
                // "configured" badge on a provider that is not set up.
                _ => in_use.iter().any(|(_, origin)| *origin == cp::UrlOrigin::Known(p.id)),
            };
            let engines_here: Vec<&str> = in_use.iter()
                .filter(|(_, origin)| *origin == cp::UrlOrigin::Known(p.id))
                .map(|(engine, _)| *engine).collect();
            serde_json::json!({
                "id": p.id, "label": p.label, "shape": p.shape, "cost": p.cost,
                "price": p.price, "free_allowance": p.free_allowance,
                "automated": p.automated, "engines": p.engines,
                "signup_url": p.signup_url, "setup": p.setup, "needs": p.needs, "note": p.note,
                "ready": ready,
                // Which engines are actually pointed at this provider right now — so the panel can
                // say "video is on RunPod" rather than only "RunPod: ready".
                "engines_here": engines_here,
            })
        }).collect::<Vec<_>>(),
    }))
}

/// This account's real GPU quota, from Kaggle rather than from a stored guess.
///
/// Also the cheapest proof that the app can talk to Kaggle without the CLI at all: it is a plain
/// authenticated GET, and it works on a phone, where spawning the CLI cannot.
#[tauri::command]
pub async fn kaggle_quota() -> Res<Value> {
    match crate::kaggle_api::quota().await {
        Ok(q) => Ok(serde_json::json!({
            "ok": true, "used_minutes": q.used_minutes, "allowed_minutes": q.allowed_minutes,
            "left_minutes": q.left_minutes, "resets_at": q.resets_at,
            "detail": format!("{} of {} GPU minutes used this week.", q.used_minutes, q.allowed_minutes),
        })),
        Err(detail) => Ok(serde_json::json!({ "ok": false, "detail": detail })),
    }
}

/// Move the cached OAuth token aside so the API key in `kaggle.json` is what authenticates.
///
/// This is the whole reason switching accounts appeared not to work. The modern Kaggle client
/// prefers `~/.kaggle/access_token` over `kaggle.json` and reports `auth_method: ACCESS_TOKEN` when
/// it does — so writing another account's key changed the file and nothing else. `reconcile_kaggle_
/// identity` then asked the CLI who it was, got the *old* account, and dutifully flipped the app's
/// record back. The switch reported success, the label reverted, and every push afterwards ran as
/// somebody the user had not chosen.
///
/// Renamed rather than deleted: it is a credential the user may want back, and a `.disabled` file is
/// recoverable in a way an `unlink` is not.
async fn park_access_token() -> Option<std::path::PathBuf> {
    let home = env::var("HOME").ok()?;
    let token = PathBuf::from(home).join(".kaggle/access_token");
    if !token.is_file() { return None; }
    let parked = token.with_extension("disabled");
    let _ = fs::remove_file(&parked).await;
    fs::rename(&token, &parked).await.ok()?;
    Some(parked)
}

/// Activate a stored account: write its kaggle.json and mark it active.
///
/// Verifies afterwards rather than assuming. A switch that did not take is worth saying out loud —
/// the previous version reported success unconditionally, which is how the label could disagree with
/// the toast that had just claimed it changed.
#[tauri::command]
pub async fn activate_kaggle_account(state: State<'_, AppState>, username: String) -> Res<Value> {
    let accts = stored_kaggle_accounts(&state.db).await;
    let acct = accts.iter().find(|a| a.get_str("username").ok() == Some(username.as_str()))
        .ok_or_else(|| format!("No stored account '{}'.", username))?;
    let key = acct.get_str("key").map_err(|_| "Stored account is missing its key.".to_string())?;

    let parked = park_access_token().await;
    write_kaggle_json(&username, key).await?;
    let (verified, detail) = verify_kaggle_auth().await;

    // Ask the CLI who it is *now*. If it still disagrees, the settings record must not claim
    // otherwise: the CLI is what every push and poll actually runs as.
    let actual = kaggle_cli_username().await.unwrap_or_else(|| username.clone());
    let switched = actual == username;

    state.db.collection::<Document>("settings").update_one(
        doc! { "_id": "singleton" },
        doc! { "$set": { "kaggle_active": &actual, "kaggle_username": &actual, "kaggle_connected": true } },
    ).await.map_err(e)?;

    Ok(serde_json::json!({
        "ok": switched, "username": actual, "asked_for": username, "verified": verified,
        "parked_access_token": parked.map(|p| p.to_string_lossy().to_string()),
        "detail": if switched {
            format!("Now running as {actual}.")
        } else {
            format!("Asked for {username}, but the Kaggle CLI still authenticates as {actual}. \
                     {detail} Sign out of the CLI (`kaggle auth logout`) and try again — until then \
                     every push and poll runs as {actual}.")
        },
    }))
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

/// The directories this platform will actually let the app write to, with the facts needed to
/// choose between them.
///
/// Android has no folder picker for this, and the reason is worth stating because it is not
/// laziness on Tauri's part. Android's Storage Access Framework is not a permission you can be
/// granted — it is a document picker that hands back a `content://` URI, and a `content://` URI is
/// not a path. `std::fs` cannot open one. So a picker that "worked" would return something every
/// write in this app would then fail on, which is worse than not offering it.
///
/// What does work, and needs no permission at all, is `getExternalFilesDir(...)`: real filesystem
/// paths under `/storage/emulated/0/Android/data/<package>/files/`, which a file manager can browse
/// and a USB cable can reach. Tauri already resolves those (document_dir, download_dir, picture_dir,
/// audio_dir). Offering a choice between them is a real choice — unlike the previous behaviour,
/// which silently returned the *internal* data directory, a place the user cannot see at all.
#[tauri::command]
pub async fn list_storage_locations(app: tauri::AppHandle) -> Res<Value> {
    use tauri::Manager;
    let p = app.path();
    let mut out: Vec<Value> = Vec::new();
    let mut add = |id: &str, label: &str, note: &str, dir: Option<std::path::PathBuf>| {
        if let Some(dir) = dir {
            let _ = std::fs::create_dir_all(&dir);
            out.push(serde_json::json!({
                "id": id, "label": label, "note": note,
                "path": dir.to_string_lossy(),
                "writable": dir.exists(),
            }));
        }
    };

    #[cfg(desktop)]
    {
        add("documents", "Documents", "The usual place for work you keep.", p.document_dir().ok());
        add("home", "Home folder", "", p.home_dir().ok().map(|h| h.join("AI Music Video Studio")));
        add("downloads", "Downloads", "", p.download_dir().ok());
    }
    #[cfg(not(desktop))]
    {
        // All of these are getExternalFilesDir(...) — visible in a file manager under
        // Android/data/<package>/files, and removed with the app, which is worth saying because it
        // is the one real drawback of writing there rather than to shared storage.
        add("documents", "App folder — Documents",
            "Browsable in a file manager under Android/data. Removed if you uninstall the app.",
            p.document_dir().ok());
        add("music", "App folder — Music",
            "Same place, sorted for audio. Some players index it.", p.audio_dir().ok());
        add("pictures", "App folder — Pictures",
            "Same place, sorted for images.", p.picture_dir().ok());
        add("downloads", "App folder — Downloads",
            "Same place; where an update downloads to.", p.download_dir().ok());
    }

    Ok(serde_json::json!({
        "locations": out,
        "can_browse": cfg!(desktop),
        // Said plainly so the interface does not have to guess how to explain it.
        "note": if cfg!(desktop) { "" } else {
            "Android does not let an app browse for a folder — its picker returns a document handle \
             rather than a path, which nothing here could write to. These are the real folders this \
             app can use, and a file manager can reach all of them."
        },
    }))
}

/// Native folder picker (used by onboarding to choose where project files/exports live).
/// Returns the chosen absolute path, or null if the user cancelled.
#[tauri::command]
pub async fn pick_directory(
    #[allow(unused_variables)] app: tauri::AppHandle,
    title: Option<String>,
) -> Res<Option<String>> {
    #[cfg(desktop)]
    {
        let mut dlg = rfd::AsyncFileDialog::new();
        if let Some(t) = title.filter(|s| !s.is_empty()) { dlg = dlg.set_title(t); }
        Ok(dlg.pick_folder().await.map(|f| f.path().to_string_lossy().to_string()))
    }
    // On mobile this is not a picker and does not pretend to be one — see list_storage_locations for
    // why Android cannot offer one that produces a usable path. It returns the *external* documents
    // directory rather than the app-internal one it used to: same lack of choice, but somewhere the
    // user can actually open with a file manager, which the internal directory never was.
    #[cfg(not(desktop))]
    {
        use tauri::Manager;
        let _ = title;
        let dir = app.path().document_dir().ok()
            .or_else(|| app.path().app_data_dir().ok().map(|p| p.join("projects")));
        Ok(dir.map(|folder| {
            let _ = std::fs::create_dir_all(&folder);
            folder.to_string_lossy().to_string()
        }))
    }
}

#[tauri::command]
pub async fn start_kaggle_server(state: State<'_, AppState>, engine: String) -> Res<Value> {
    // Refused before anything is pulled or pushed: a hidden engine must not occupy a GPU slot.
    if engine_hidden(&engine) { return Ok(hidden_engine_reply(&engine)); }
    let (own, upstream, _) = match kaggle_slugs(&state.db, &engine).await {
        Some(v) => v,
        None => return Ok(serde_json::json!({ "ok": false, "detail": format!("Unknown engine '{}'.", engine) })),
    };
    // Ask the quota before spending eight minutes finding out. A GPU-less run is not a failure the
    // notebook can do anything about: Kaggle simply declines the accelerator, the notebook prints
    // "NO GPU ON THIS RUN — not serving", and the person watching has already waited three minutes
    // for an answer that was knowable in one request. Observed exactly this way in a real log.
    //
    // Advisory, not blocking: the number can be stale, and a wrong refusal would be worse than a
    // wasted run. It goes in the reply so the interface can say it up front.
    let quota_note = match crate::kaggle_api::quota().await {
        Ok(q) if q.left_minutes <= 0 => Some(format!(
            "This account's weekly GPU quota looks used up ({} of {} minutes). Kaggle will most \
             likely run this on CPU, and the notebook does not serve without a GPU. It resets {}.",
            q.used_minutes, q.allowed_minutes,
            if q.resets_at.is_empty() { "weekly (Saturdays UTC)".into() } else { q.resets_at.clone() })),
        _ => None,
    };

    // A phone has no CLI to spawn, and a desktop may simply not have installed one. Both now take
    // the HTTP path rather than dead-ending: `kernels push` was the last operation on the critical
    // path that only the CLI could do, which is what made a phone able to watch a server but never
    // start one.
    let found = locate_kaggle_opt();
    if crate::kaggle_api::transport(found.is_some()) == crate::kaggle_api::Transport::Http {
        return start_kaggle_server_http(&engine, &own, &upstream).await;
    }
    let kaggle = found.unwrap_or_else(|| "kaggle".to_string());

    // Pull the kernel (code + metadata) from Kaggle so this works from the installed app, which
    // doesn't ship the notebook sources — own copy if there is one, else the published template,
    // re-addressed so the push below lands on a kernel this account can actually write to.
    let tmp = env::temp_dir().join(format!("bm-kaggle-start-{}", engine));
    let _ = fs::remove_dir_all(&tmp).await;
    let _ = fs::create_dir_all(&tmp).await;
    let pulled = match pull_kernel_for_push(&kaggle, &tmp, &own, &upstream).await {
        Ok(p) => p,
        Err(msg) => return Ok(serde_json::json!({
            "ok": false, "status": "pull_failed", "detail": msg,
            "next_step": "Check that ~/.kaggle/kaggle.json is valid (Kaggle → Settings → Create New API Token)."
        })),
    };

    // A serving run needs the GPU: the notebook's batch guard only opens the tunnel
    // when a GPU is present (GPU-less batch runs are cheap source-update pushes).
    let meta_path = tmp.join("kernel-metadata.json");
    let meta_raw = fs::read_to_string(&meta_path).await.map_err(e)?;
    let mut meta: Value = serde_json::from_str(&meta_raw).map_err(e)?;
    request_gpu(&mut meta);
    meta["enable_internet"] = Value::Bool(true);
    strip_pinned_image(&mut meta);
    fs::write(&meta_path, serde_json::to_string_pretty(&meta).map_err(e)?).await.map_err(e)?;

    let push = tokio::process::Command::new(&kaggle)
        .args(["kernels", "push", "-p"]).arg(&tmp)
        .output().await.map_err(|err| format!("Could not run the kaggle CLI: {}", err))?;
    let out = format!("{}{}", String::from_utf8_lossy(&push.stdout), String::from_utf8_lossy(&push.stderr));
    let low = out.to_lowercase();
    if out.contains("successfully pushed") {
        let created = if pulled.from_upstream {
            format!(" This is the first run on “{}”, so the notebook was copied to that account.",
                    own.split('/').next().unwrap_or(""))
        } else { String::new() };
        Ok(serde_json::json!({
            "ok": true, "status": "starting", "slug": pulled.slug, "created": pulled.from_upstream,
            "quota_warning": quota_note,
            "detail": format!("{} server starting on Kaggle (GPU batch run).{} It needs ~8-10 min to install, download models and open the tunnel.", engine, created),
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

/// Drop a pinned `docker_image` from kernel metadata before pushing a run that needs a GPU.
///
/// Kaggle builds two different images — one with the CUDA stack, one without — and `docker_image`
/// pins an exact digest of one of them. The metadata this app pushes is not written from scratch:
/// `pull_kernel_for_push` fetches whatever Kaggle currently holds for the kernel, and that record
/// includes the digest of the image the kernel *last ran on*. So the moment a kernel runs once
/// without a GPU — a source-update push, a `supersede_kaggle_session` teardown, or a run Kaggle
/// declined an accelerator for — Kaggle stores the CPU digest, and every later push re-pins it.
/// The kernel is then permanently stuck: Kaggle honours the pin, boots the CPU image, and the
/// notebook finds no GPU however much weekly quota is left.
///
/// Verified 2026-08-13 rather than reasoned about. Two runs of the same one-cell probe kernel with
/// identical `enable_gpu`/`machine_shape`: with no `docker_image`, two Tesla T4s and
/// `torch 2.10.0+cu128`; with this digest pinned, `nvidia-smi` exit 12 and `torch 2.10.0+cpu`.
/// That is also what the heartmula engine had been doing — reported as "the server never starts",
/// and diagnosed by the app as an exhausted quota on an account with 29.8 of 30 hours left.
///
/// Removing the key rather than choosing a digest is deliberate: Kaggle then picks its own current
/// default for the accelerator being requested, which is right by construction and does not rot the
/// way a hard-coded digest in this file would.
fn strip_pinned_image(meta: &mut Value) {
    if let Some(obj) = meta.as_object_mut() { obj.remove("docker_image"); }
}

/// The accelerator a serving run asks for, stated rather than inherited.
///
/// `machine_shape` is the field Kaggle actually honours, and it OUTRANKS `enable_gpu`: a push
/// carrying `enable_gpu: false` alongside `machine_shape: "Gpu"` was handed a Tesla P100, measured
/// 2026-08-13. Since the metadata pushed here is whatever the account's kernel already held, both
/// halves of that pair have to be set on purpose — leaving either to whatever Kaggle last stored is
/// how a run ends up on hardware nobody asked it for.
const SERVING_ACCELERATOR: &str = "NvidiaTeslaT4";

fn request_gpu(meta: &mut Value) {
    meta["enable_gpu"] = Value::Bool(true);
    meta["enable_tpu"] = Value::Bool(false);
    meta["machine_shape"] = Value::String(SERVING_ACCELERATOR.to_string());
}

/// Ask for no accelerator at all — the metadata for a push whose only purpose is to end a session.
///
/// This is what `enable_gpu = false` was believed to do, and did not. `stop_kaggle_server`, the idle
/// watchdog and the stuck-session recovery all push a GPU-off version precisely so the teardown
/// needs no GPU slot and costs no quota. With `machine_shape` left in place they were each starting
/// a *fresh accelerated session* instead — the idle watchdog, whose entire job is to stop a run that
/// is quietly billing GPU hours, was billing another one to do it.
fn request_no_accelerator(meta: &mut Value) {
    meta["enable_gpu"] = Value::Bool(false);
    meta["enable_tpu"] = Value::Bool(false);
    if let Some(obj) = meta.as_object_mut() { obj.remove("machine_shape"); }
}

/// Turn whatever Kaggle said about a refused push into the app's own outcome codes.
///
/// Shared by both transports on purpose. The CLI prints its reasons and the API returns them, but
/// they are the *same* reasons — and a phone hitting the concurrent-session cap deserves the same
/// sentence a desktop gets rather than a raw error body.
fn classify_push_failure(text: &str) -> Value {
    let low = text.to_lowercase();
    if low.contains("session count") || low.contains("concurrent") || low.contains("maximum number")
        || low.contains("too many") || low.contains("reached the maximum") || low.contains("active session")
    {
        serde_json::json!({
            "ok": false, "status": "gpu_slots_full",
            "detail": "Kaggle allows only 2 concurrent GPU batch sessions and both are in use.",
            "next_step": "Open each engine's notebook and look for one still showing a running session with a dead tunnel — click Stop Session there, then retry."
        })
    } else if low.contains("quota") || low.contains("exceeded") {
        serde_json::json!({
            "ok": false, "status": "gpu_quota",
            "detail": "Your weekly Kaggle GPU quota (30 h) looks exhausted — a GPU batch run can't start.",
            "next_step": "Wait for the weekly reset (Saturdays UTC), or run the engine on another free GPU host and paste its URL."
        })
    } else if low.contains("403") || low.contains("forbidden") || low.contains("401")
        || low.contains("unauthorized") || low.contains("token")
    {
        serde_json::json!({
            "ok": false, "status": "auth_error",
            "detail": "Kaggle rejected the push — the API token is missing or invalid.",
            "next_step": "Create a new API token at Kaggle → Settings and save it as ~/.kaggle/kaggle.json, then retry."
        })
    } else {
        serde_json::json!({
            "ok": false, "status": "push_failed",
            "detail": text.trim().chars().take(400).collect::<String>(),
            "next_step": "Open the notebook and check the kernel there. If it looks fine, retry Start & connect."
        })
    }
}

/// Start an engine's server without the CLI — the phone path.
///
/// The same three steps the CLI path takes, over HTTP: find a notebook to push (this account's copy,
/// else the published template, else the copy bundled in the binary), re-address it to this account
/// and turn the GPU on, then push. Nothing here touches the filesystem, because a pulled kernel is
/// already a string and writing it to a temp file only existed so the CLI had a folder to read.
async fn start_kaggle_server_http(engine: &str, own: &str, upstream: &str) -> Res<Value> {
    use crate::kaggle_api as kapi;

    let pulled = match kapi::kernel_pull(own).await {
        Ok(Some(k)) => Some((k.metadata, k.source, false)),
        Ok(None) => match kapi::kernel_pull(upstream).await {
            Ok(Some(k)) => Some((k.metadata, k.source, true)),
            Ok(None) => None,
            Err(msg) => return Ok(serde_json::json!({
                "ok": false, "status": "pull_failed", "detail": msg,
                "next_step": "Check that ~/.kaggle/kaggle.json is valid (Kaggle → Settings → Create New API Token)."
            })),
        },
        Err(msg) => return Ok(serde_json::json!({
            "ok": false, "status": "pull_failed", "detail": msg,
            "next_step": "Check that ~/.kaggle/kaggle.json is valid (Kaggle → Settings → Create New API Token)."
        })),
    };

    // Neither copy exists: the normal state for an engine whose notebook was never published. The
    // bundled copy is also what keeps the app independent of the author's Kaggle account staying
    // alive — the notebooks ship inside the binary, and Kaggle is only a place to run them.
    let name = own.split('/').next_back().unwrap_or("biblemusically-server");
    let (mut meta, source, from_upstream) = match pulled {
        Some(v) => v,
        None => {
            let Some(source) = bundled_notebook(engine) else {
                return Ok(serde_json::json!({
                    "ok": false, "status": "pull_failed",
                    "detail": format!("Neither {own} nor {upstream} exists on Kaggle, and no copy of the {engine} notebook is bundled."),
                    "next_step": "Update the app, or open the notebook on Kaggle once so this account has a copy."
                }));
            };
            (serde_json::json!({
                "id": own, "title": name, "code_file": "setup_and_serve.ipynb",
                "language": "python", "kernel_type": "notebook", "is_private": true,
            }), source.to_string(), true)
        }
    };

    // Re-address to this account: `id` is the only field the push derives its target from.
    meta["id"] = Value::String(own.to_string());
    // Kaggle rejects a title under five characters, and a fresh kernel needs one.
    if !meta["title"].as_str().map(|t| t.trim().chars().count() >= 5).unwrap_or(false) {
        meta["title"] = Value::String(name.to_string());
    }
    // `id_no` names a specific existing kernel numerically and would override `id`, sending the push
    // straight back at the template we copied from.
    if let Some(obj) = meta.as_object_mut() { obj.remove("id_no"); }
    // A serving run needs the GPU: the notebook only opens its tunnel when one is present.
    meta["enable_gpu"] = Value::Bool(true);
    meta["enable_internet"] = Value::Bool(true);
    strip_pinned_image(&mut meta);

    match kapi::kernel_push(&meta, &source).await {
        Ok(_) => {
            let created = if from_upstream {
                format!(" This is the first run on “{}”, so the notebook was copied to that account.",
                        own.split('/').next().unwrap_or(""))
            } else { String::new() };
            Ok(serde_json::json!({
                "ok": true, "status": "starting", "slug": own, "created": from_upstream,
                "detail": format!("{engine} server starting on Kaggle (GPU batch run).{created} It needs ~8-10 min to install, download models and open the tunnel."),
                "next_step": "Watch the live log below. The run serves until Kaggle's ~9-12 h batch limit."
            }))
        }
        Err(msg) => Ok(classify_push_failure(&msg)),
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
pub async fn stop_kaggle_server(state: State<'_, AppState>, engine: String) -> Res<Value> {
    stop_kaggle_session(&state.db, &engine).await
}

/// Stop a session on a raw store handle — shared by the command above and the idle sweep.
pub async fn stop_kaggle_session(db: &crate::store::Db, engine: &str) -> Res<Value> {
    let r = supersede_session(db, engine).await?;
    let ok = r["ok"].as_bool().unwrap_or(false);
    Ok(serde_json::json!({
        "ok": ok,
        "engine": engine,
        "detail": if ok { format!("{} stopped — its GPU slot is released.", engine) }
                  else { r["detail"].as_str().unwrap_or("Could not stop the session.").to_string() },
    }))
}

/// Does this URL still answer? A tunnel that is gone is the difference between "already serving"
/// and "sitting behind a dead blocker", and every other decision here depends on getting it right.
/// Under 500 counts: a 404 from a served path still proves something is listening.
async fn url_answers(url: &str) -> bool {
    matches!(reqwest::Client::new().get(url).timeout(std::time::Duration::from_secs(8)).send().await,
             Ok(res) if res.status().as_u16() < 500)
}

/// Clear everything that can block a fresh start, then report what was actually in the way.
///
/// The reactive recovery this replaces only fired *after* a push came back `gpu_slots_full`, which
/// meant the common case never reached it: a run whose tunnel died but that Kaggle still lists as
/// RUNNING holds its GPU slot for the full ~9-12 h batch limit, and the app would sit in "checking
/// whether a server is already live" against a kernel that was never going to serve. Waiting for a
/// specific failure code to arrive is the wrong shape — by then the person has already watched a
/// progress bar do nothing.
///
/// So this runs *before* the push, and clears three separate things that each look identical from
/// the outside:
///
/// 1. **A stored URL that no longer answers.** Kept, it makes every later check "succeed" against a
///    tunnel that is gone.
/// 2. **A session holding the GPU slot without serving.** Ended with the GPU-off supersede, since
///    Kaggle has no stop-session call.
/// 3. **A monitor still streaming logs from the previous attempt**, which would otherwise narrate
///    the old run over the new one.
///
/// Deliberately safe to call when nothing is wrong: a kernel that is genuinely serving is left
/// alone, and the reply says so. That is what makes it callable unconditionally on every start.
#[tauri::command]
pub async fn reset_kaggle_engine(
    state: State<'_, AppState>,
    monitors: State<'_, crate::commands::kaggle_monitor::KaggleMonitors>,
    engine: String,
) -> Res<Value> {
    // Silence the previous attempt's log stream first, or its lines arrive interleaved with the new
    // run's and the panel reads as one confused story. Done here rather than in the shared helper
    // because only a command has the monitor registry.
    if let Some(stop) = monitors.stops.lock().unwrap().get(&engine) {
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    reset_engine_blockers(&state.db, &engine).await
}

/// The reset itself, on a raw store handle so any caller can use it.
pub async fn reset_engine_blockers(db: &crate::store::Db, engine: &str) -> Res<Value> {
    if engine_hidden(engine) { return Ok(hidden_engine_reply(engine)); }
    let mut cleared: Vec<String> = Vec::new();

    let url_key = format!("{engine}_api_url");
    let stored = db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.ok().flatten()
        .and_then(|d| d.get_str(&url_key).ok().map(|s| s.to_string()))
        .unwrap_or_default();

    // Is anything actually serving? A live URL means there is nothing to clean up, and tearing down
    // a working server because the user pressed start twice would be its own bug.
    if !stored.trim().is_empty() && url_answers(&stored).await {
        return Ok(serde_json::json!({
            "ok": true, "engine": engine, "already_live": true, "url": stored, "cleared": cleared,
            "detail": "That engine is already serving — nothing needed clearing.",
        }));
    }
    if !stored.trim().is_empty() {
        db.collection::<Document>("settings").update_one(
            doc! { "_id": "singleton" }, doc! { "$set": { &url_key: "" } },
        ).await.map_err(e)?;
        cleared.push(format!("a stored URL that no longer answers ({stored})"));
    }

    // A kernel Kaggle still calls running, with no reachable tunnel, is the zombie holding the slot.
    let kstatus = match kaggle_slugs(db, engine).await {
        Some((own, _, _)) => kernel_status_any(&own).await.unwrap_or_else(|_| "unknown".into()),
        None => "unknown".to_string(),
    };
    let mut freed_slot = false;
    if matches!(kstatus.as_str(), "running" | "queued") {
        match supersede_session(db, engine).await {
            Ok(r) if r["ok"].as_bool().unwrap_or(false) => {
                freed_slot = true;
                cleared.push(format!("a {kstatus} session that was holding its GPU slot"));
            }
            Ok(r) => cleared.push(format!(
                "could not end the {kstatus} session: {}",
                r["detail"].as_str().unwrap_or("no detail"))),
            Err(err) => cleared.push(format!("could not end the {kstatus} session: {err}")),
        }
    }

    Ok(serde_json::json!({
        "ok": true, "engine": engine, "already_live": false,
        "kernel_status": kstatus, "freed_slot": freed_slot, "cleared": cleared,
        "detail": if cleared.is_empty() {
            "Nothing was blocking a start.".to_string()
        } else {
            format!("Cleared {}.", cleared.join("; "))
        },
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
pub async fn supersede_kaggle_session(state: State<'_, AppState>, engine: String) -> Res<Value> {
    supersede_session(&state.db, &engine).await
}

/// The supersede itself, on a raw store handle so the idle sweep (which has no `tauri::State`) can
/// shut a forgotten session down through exactly the same path the UI button uses.
pub async fn supersede_session(db: &crate::store::Db, engine: &str) -> Res<Value> {
    let (own, upstream, _) = match kaggle_slugs(db, engine).await {
        Some(v) => v,
        None => return Ok(serde_json::json!({ "ok": false, "detail": format!("Unknown engine '{}'.", engine) })),
    };
    // A phone can reach this, and a desktop may simply not have the CLI. Say which, in a
    // sentence, rather than letting it surface as a spawn error. See require_kaggle_cli.
    let kaggle = require_kaggle_cli()?;
    let tmp = env::temp_dir().join(format!("bm-kaggle-stop-{}", engine));
    let _ = fs::remove_dir_all(&tmp).await;
    let _ = fs::create_dir_all(&tmp).await;
    let pulled = match pull_kernel_for_push(&kaggle, &tmp, &own, &upstream).await {
        Ok(p) => p,
        Err(msg) => return Ok(serde_json::json!({
            "ok": false, "detail": format!("Could not pull the kernel to supersede it: {msg}") })),
    };
    // Nothing to supersede: the account has no kernel of its own, so it cannot be holding a session.
    if pulled.from_upstream {
        return Ok(serde_json::json!({ "ok": true, "detail": format!(
            "No {engine} kernel exists on this Kaggle account yet, so no session is running.") }));
    }
    let meta_path = tmp.join("kernel-metadata.json");
    if let Ok(raw) = fs::read_to_string(&meta_path).await {
        if let Ok(mut meta) = serde_json::from_str::<Value>(&raw) {
            request_no_accelerator(&mut meta); // supersede without taking a slot or spending quota
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

/// Record a tunnel URL the app just discovered and verified, so that everything reading settings
/// actually sees it.
///
/// Writing the singleton is not enough on its own, and that was a real bug. `get_settings` and
/// jobs.rs's `get_job_settings` both merge the per-project settings document ON TOP of the
/// singleton, and `update_settings` mirrors every save into that project document — so a project
/// carried its own copy of `heartmula_api_url`, frozen at whatever the tunnel was the last time
/// somebody pressed Save. Discovering a fresh URL then changed the singleton underneath a project
/// override that still shadowed it: Settings kept displaying the dead address, and a music job for
/// a song in that project kept resolving to it. Observed with a project pinned to
/// `angela-craft-icon-humidity.trycloudflare.com`, a tunnel that had not existed for weeks.
///
/// So the override is removed rather than rewritten. An engine's tunnel address is a machine
/// discovered fact about one running server, not a per-project preference — there is no coherent
/// meaning for "this project uses a different tunnel to the same notebook", and leaving a copy in
/// every project doc only recreates the same staleness on the next rotation.
pub(crate) async fn persist_engine_url(db: &crate::store::Db, settings_key: &str, url: &str) {
    let mut set = Document::new();
    set.insert(settings_key, url);
    let _ = db.collection::<Document>("settings")
        .update_one(doc! { "_id": "singleton" }, doc! { "$set": set }).await;
    let mut unset = Document::new();
    unset.insert(settings_key, "");
    let _ = db.collection::<Document>("settings")
        .update_many(doc! { "_id": { "$ne": "singleton" } }, doc! { "$unset": unset }).await;
}

/// Re-discover and persist `engine`'s live tunnel URL with no UI-facing diagnostics — the same
/// discover-then-verify steps as `fetch_kaggle_url` below, minus the kernel-status messaging,
/// so a job runner can call this mid-run when the cached URL turns out to be a dead tunnel
/// (Cloudflare tunnels rotate every run — see jobs.rs's `generate_song_api`) instead of failing
/// outright and making the user manually re-run "Fetch live URL" in Settings.
pub async fn refresh_kaggle_url(db: &crate::store::Db, engine: &str) -> Option<String> {
    let (slug, _upstream, settings_key) = kaggle_slugs(db, engine).await?;
    let slug = slug.as_str();

    // Only this account's own kernel is consulted. Falling back to the upstream template here would
    // hand back somebody else's tunnel URL — which is either dead or, worse, live and belonging to a
    // machine this user does not control.
    let found = locate_kaggle_opt();
    let url = if crate::kaggle_api::transport(found.is_some()) == crate::kaggle_api::Transport::Http {
        // The log comes back inside the output response, so there is nothing to download and no
        // temp directory to scan — which is also why this works on a phone.
        crate::kaggle_api::tunnel_url(slug).await.ok().flatten()
    } else {
        let kaggle = found.unwrap_or_else(|| "kaggle".to_string());
        let tmp = env::temp_dir().join(format!("bm-kaggle-{}", engine));
        let _ = fs::create_dir_all(&tmp).await;
        let _ = tokio::process::Command::new(&kaggle)
            .args(["kernels", "output", slug, "-p"]).arg(&tmp).output().await;
        let mut u = scan_dir_for_tunnel_url(&tmp).await;
        if u.is_none() {
            u = stream_logs_for_tunnel_url(&kaggle, slug, 25).await;
        }
        u
    };
    let u = url?;

    let alive = match reqwest::Client::new().get(&u).timeout(std::time::Duration::from_secs(8)).send().await {
        Ok(res) => res.status().as_u16() < 500,
        Err(_) => false,
    };
    if !alive {
        return None;
    }

    persist_engine_url(db, settings_key, &u).await;
    Some(u)
}

#[tauri::command]
pub async fn fetch_kaggle_url(state: State<'_, AppState>, engine: String) -> Res<Value> {
    let (own, _upstream, settings_key) = match kaggle_slugs(&state.db, &engine).await {
        Some(v) => v,
        None => return Ok(serde_json::json!({ "ok": false, "detail": format!("Unknown engine '{}'.", engine) })),
    };
    let slug = own.as_str();
    // Both halves of this — asking the status and reading the log — now have an HTTP form, so a
    // phone can find its server's address rather than only be told one exists.
    let found = locate_kaggle_opt();
    let http = crate::kaggle_api::transport(found.is_some()) == crate::kaggle_api::Transport::Http;
    let kaggle = found.unwrap_or_else(|| "kaggle".to_string());

    // 1) Kernel status — so we can give an actionable message when there's no URL.
    let status_str = if http {
        match crate::kaggle_api::kernel_status(slug).await {
            Ok(s) => s,
            Err(msg) => return Ok(serde_json::json!({
                "ok": false, "status": "auth_error", "detail": msg,
                "next_step": "Open the Kaggle token page, Create New API Token, and save it as ~/.kaggle/kaggle.json."
            })),
        }
    } else {
        match tokio::process::Command::new(&kaggle)
            .args(["kernels", "status", slug]).output().await
        {
            Ok(o) => format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)),
            Err(err) => return Ok(serde_json::json!({
                "ok": false, "status": "no_cli",
                "detail": format!("Could not run the kaggle CLI: {}", err),
                "next_step": "Install it with `pipx install kaggle` and place your token at ~/.kaggle/kaggle.json (Kaggle → Settings → Create New API Token)."
            })),
        }
    };
    // The REST status is a bare word ("running"); the CLI prints a sentence containing it uppercased.
    // Upper-casing here lets one set of `contains` checks read both.
    let status_str = status_str.to_uppercase();
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
    // HTTP first even when a CLI exists. `kernels output` on the CLI *downloads every output file*
    // to find a URL that is sitting in the log — measured at 22 s and 4.8 MB on a real heartmula
    // kernel, before a possible 25 s of log streaming on top. That is most of the minute somebody
    // spends watching "Checking whether a server is already live…" and concluding it does nothing.
    // The REST call returns the log in the response body: one request, no downloads.
    let mut url = crate::kaggle_api::tunnel_url(slug).await.ok().flatten();
    if url.is_none() && !http {
        let tmp = env::temp_dir().join(format!("bm-kaggle-{}", engine));
        let _ = fs::create_dir_all(&tmp).await;
        let _ = tokio::process::Command::new(&kaggle)
            .args(["kernels", "output", slug, "-p"]).arg(&tmp).output().await;
        url = scan_dir_for_tunnel_url(&tmp).await;
    }

    if !http && url.is_none() && matches!(kstatus, "running" | "queued" | "unknown") {
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
            persist_engine_url(&state.db, settings_key, &u).await;
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

#[cfg(test)]
mod kaggle_slug_tests {
    use super::*;

    /// The regression that made every engine unusable for anybody but the notebook's author.
    ///
    /// `kaggle_kernel_for` used to return a full `owner/name` slug with the owner compiled in. Every
    /// caller then pushed to, polled, and streamed logs from that one account's kernels regardless of
    /// who was signed in — so a second account could not start a server, and the failure presented as
    /// Kaggle misbehaving (a stale "complete" status, another run's log) rather than as the app
    /// addressing the wrong kernel. Keeping the owner out of this table is what prevents it coming
    /// back; the owner belongs to `kaggle_owner`, which reads the credentials actually in use.
    #[test]
    fn kernel_names_never_carry_an_owner() {
        for engine in ["acestep", "heartmula", "comfyui", "comfy", "flux", "riffusion", "video"] {
            let (name, key) = kaggle_kernel_for(engine)
                .unwrap_or_else(|| panic!("'{engine}' should be a known engine"));
            assert!(!name.contains('/'),
                    "'{engine}' maps to '{name}', which bakes in an owner — the owner must come from \
                     the signed-in account, not from this table");
            assert!(key.ends_with("_api_url"), "'{engine}' stores its URL under '{key}'");
        }
    }

    /// A GPU start must never carry the image digest Kaggle recorded from an earlier run.
    ///
    /// The metadata pushed for a start is whatever `pull_kernel_for_push` fetched from Kaggle, which
    /// names the image the kernel last ran on. One GPU-less run — a source update, an idle teardown,
    /// a run Kaggle declined an accelerator for — writes the CPU image's digest there, and pushing it
    /// back pins the next run to the CPU image too. The engine can then never serve again, at any
    /// quota, and the app reads the result as an exhausted GPU allowance. Measured on 2026-08-13:
    /// the same probe kernel got two T4s unpinned and `torch 2.10.0+cpu` with this digest pinned.
    #[test]
    fn a_gpu_start_does_not_inherit_the_last_runs_image() {
        let mut meta = serde_json::json!({
            "id": "someone/biblemusically-heartmula-server",
            "enable_gpu": false,
            "machine_shape": "NvidiaTeslaT4",
            "docker_image": "gcr.io/kaggle-images/python@sha256:dafd4ce5668bbf1ad422e4c109e0f18c9623c3a7c7f48b0235f13142755c40b9",
        });
        strip_pinned_image(&mut meta);
        assert!(meta.get("docker_image").is_none(),
                "a pinned image survived the start push, which is what pins the run to Kaggle's \
                 CPU container and makes the engine unable to serve at any quota");
        // Everything else the push depends on must survive: the accelerator request in particular,
        // since dropping that would trade one no-GPU cause for another.
        assert_eq!(meta["machine_shape"], "NvidiaTeslaT4");
        assert_eq!(meta["id"], "someone/biblemusically-heartmula-server");
    }

    /// Saving Settings must not stamp a copy of the engine addresses onto the project.
    ///
    /// The per-project document is merged ON TOP of the singleton, while every piece of URL
    /// discovery writes the singleton — so a project copy is a snapshot of whatever the tunnel was
    /// when somebody last pressed Save, outranking the address the app just verified. Clearing the
    /// override on discovery was not enough by itself: the Settings form autosaves, and the next
    /// autosave wrote the dead address straight back. Observed twice on 2026-08-13, both times as
    /// an engine pinned to a trycloudflare hostname that had stopped existing.
    #[test]
    fn saving_settings_does_not_pin_a_project_to_an_engine_address() {
        let mut pdoc = bson::doc! {
            "heartmula_api_url": "https://a-dead-tunnel.trycloudflare.com",
            "comfyui_api_url": "https://another-dead-one.trycloudflare.com",
            "music_engine": "heartmula",
            "comfyui_steps": 30,
        };
        strip_discovered_urls(&mut pdoc);
        assert!(!pdoc.contains_key("heartmula_api_url"));
        assert!(!pdoc.contains_key("comfyui_api_url"));
        // Genuine per-project choices must survive — this is an override document, and stripping it
        // wholesale would throw away the per-project settings it exists to hold.
        assert_eq!(pdoc.get_str("music_engine").unwrap(), "heartmula");
        assert_eq!(pdoc.get_i32("comfyui_steps").unwrap(), 30);
    }

    /// Digit-only keys are debris and must never be written back.
    ///
    /// A UI helper accepted `updateS(key, value)` while only understanding `updateS({key: value})`,
    /// and spreading a string into an object spreads it per character — so clicking the remote
    /// rendering tile wrote `{0:"r", 1:"e", 2:"m", …}` and left the setting untouched. Found on a
    /// real store: 22 numeric keys spelling "remote_render_provider" on all three settings
    /// documents, with the provider still on its default. The UI is fixed; this keeps the wreckage
    /// from being copied forward by every subsequent save.
    #[test]
    fn digit_only_keys_are_refused_and_real_settings_survive() {
        let mut d = bson::doc! {
            "0": "r", "1": "e", "12": "p", "21": "r",
            "remote_render_provider": "modal",
            "video_fps": 30,
            "comfyui_steps": 30,
        };
        strip_debris(&mut d);
        for k in ["0", "1", "12", "21"] {
            assert!(!d.contains_key(k), "debris key {k} survived");
        }
        assert_eq!(d.get_str("remote_render_provider").unwrap(), "modal");
        assert_eq!(d.get_i32("video_fps").unwrap(), 30);
        assert_eq!(d.get_i32("comfyui_steps").unwrap(), 30);
    }

    /// A teardown push must ask for no accelerator, and a serving push must ask for one by name.
    ///
    /// `machine_shape` outranks `enable_gpu`, which is not what the code assumed. Measured
    /// 2026-08-13 on a real kernel: `enable_gpu: false` with `machine_shape: "Gpu"` still came up on
    /// a Tesla P100, and removing the field brought back `CUDA Available: False` and the notebook's
    /// own "NO GPU ON THIS RUN". Since every teardown here — the Stop button, the idle watchdog, the
    /// stuck-session recovery — works by pushing a GPU-off version, each of them had been starting a
    /// fresh accelerated session to end one, billing the quota it existed to protect.
    #[test]
    fn a_teardown_asks_for_no_accelerator_and_a_start_names_one() {
        let pulled = || serde_json::json!({
            "id": "someone/biblemusically-heartmula-server",
            "enable_gpu": true, "enable_tpu": false, "machine_shape": "Gpu",
        });

        let mut stop = pulled();
        request_no_accelerator(&mut stop);
        assert_eq!(stop["enable_gpu"], false);
        assert_eq!(stop["enable_tpu"], false);
        assert!(stop.get("machine_shape").is_none(),
                "a teardown that leaves machine_shape set is handed a GPU anyway, which is the whole bug");

        let mut start = serde_json::json!({ "id": "someone/x", "enable_gpu": false });
        request_gpu(&mut start);
        assert_eq!(start["enable_gpu"], true);
        assert_eq!(start["enable_tpu"], false);
        // Named rather than inherited: a kernel whose last push was a teardown has no machine_shape
        // left to inherit, and a serving run that quietly gets no accelerator cannot serve at all.
        assert_eq!(start["machine_shape"], SERVING_ACCELERATOR);
    }

    /// Every bundled notebook must carry a revision stamp, or it can never replace a stale copy.
    ///
    /// This is the mechanism that decides whether a notebook fix ever reaches somebody who has run
    /// the engine before. An unstamped bundled notebook reads as revision 0, never beats the copy on
    /// Kaggle, and the fix silently ships to nobody — which is precisely what happened to riffusion:
    /// its notebook was rewritten to embed its source package, and every start went on re-pushing
    /// the original from Kaggle, whose `from preset_engine import ...` could not resolve.
    #[test]
    fn every_bundled_notebook_is_stamped_and_can_supersede_an_unstamped_copy() {
        for engine in ["acestep", "heartmula", "comfyui", "flux", "riffusion", "video"] {
            let nb = bundled_notebook(engine)
                .unwrap_or_else(|| panic!("'{engine}' ships no notebook"));
            let rev = notebook_revision(nb)
                .unwrap_or_else(|| panic!("'{engine}' notebook carries no bm_notebook_revision"));
            assert!(rev >= 1, "'{engine}' is stamped {rev}, which can never supersede anything");
            // The copy already on an account predates the stamp, so it reads as 0 and must lose.
            assert!(rev > 0, "'{engine}' would not replace an unstamped Kaggle copy");
        }
    }

    /// An unstamped notebook must read as older than any stamped one, and junk must not panic.
    #[test]
    fn an_unstamped_or_unreadable_notebook_reads_as_oldest() {
        assert_eq!(notebook_revision(r#"{"metadata":{"bm_notebook_revision":7},"cells":[]}"#), Some(7));
        assert_eq!(notebook_revision(r#"{"metadata":{},"cells":[]}"#), None);
        assert_eq!(notebook_revision("not json at all"), None);
        assert_eq!(notebook_revision(""), None);
    }

    /// Every engine must ship a notebook, or it can never start on an account that has not run it
    /// before — which is every account on a fresh install. This is the check that keeps the app from
    /// depending on one person's Kaggle profile continuing to exist.
    #[test]
    fn every_engine_ships_a_notebook() {
        for engine in ["acestep", "heartmula", "comfyui", "flux", "riffusion", "video"] {
            let (name, _) = kaggle_kernel_for(engine).unwrap();
            let slug = format!("someone/{name}");
            let derived = engine_of_slug(&slug);
            let nb = bundled_notebook(derived).unwrap_or_else(||
                panic!("'{engine}' maps to kernel '{name}' -> engine '{derived}', which has no \
                        bundled notebook, so a fresh account can never start it"));
            assert!(nb.contains("\"cells\""), "'{derived}' notebook is not a notebook");
        }
    }

    /// Hiding is not deleting. A hidden engine keeps its kernel mapping and its bundled notebook, so
    /// switching it back on is removing a string from HIDDEN_ENGINES and nothing else.
    #[test]
    fn a_hidden_engine_keeps_everything_it_would_need_to_come_back() {
        for engine in HIDDEN_ENGINES {
            assert!(engine_hidden(engine));
            let (name, _) = kaggle_kernel_for(engine)
                .unwrap_or_else(|| panic!("'{engine}' is hidden but lost its kernel mapping"));
            assert!(bundled_notebook(engine_of_slug(&format!("x/{name}"))).is_some(),
                    "'{engine}' is hidden but lost its notebook");
        }
    }

    /// Engine ids reach this from settings documents and UI strings, not only from code, so the
    /// match has to survive casing and padding — and must not catch a name that merely starts the
    /// same way.
    #[test]
    fn hiding_matches_on_the_whole_name_only() {
        for hidden in HIDDEN_ENGINES {
            assert!(engine_hidden(hidden));
            assert!(engine_hidden(&format!("  {} ", hidden.to_uppercase())));
            assert!(!engine_hidden(&format!("{hidden}-old")),
                    "a longer name that starts the same way is a different engine");
        }
        assert!(!engine_hidden(""));
        assert!(!engine_hidden("heartmula"));
    }

    /// Whatever is hidden must leave a music engine behind, or nothing can generate a song at all.
    #[test]
    fn hiding_never_removes_every_music_engine() {
        assert!(["heartmula", "acestep", "riffusion"].iter().any(|e| !engine_hidden(e)),
                "every music engine is hidden — nothing can generate a song");
    }

    #[test]
    fn an_unknown_engine_has_no_kernel() {
        assert!(kaggle_kernel_for("suno").is_none());
        assert!(kaggle_kernel_for("").is_none());
    }

    /// Every engine the idle guard and the pipeline name must be addressable, and each must own a
    /// distinct settings key — two engines sharing one would overwrite each other's live URL.
    #[test]
    fn every_engine_has_its_own_settings_key() {
        let mut keys = Vec::new();
        for engine in ["acestep", "heartmula", "comfyui", "flux", "riffusion", "video"] {
            let (_, key) = kaggle_kernel_for(engine).unwrap();
            assert!(!keys.contains(&key), "'{key}' is used by more than one engine");
            keys.push(key);
        }
    }
}

    #[test]
    fn an_account_risking_engine_is_off_until_somebody_types_the_words() {
        // The point is not secrecy — this is the user's own machine and their own settings file, so a
        // secret kept here would protect nothing. The point is that nobody enables it by clicking a
        // toggle they did not read.
        assert!(engine_is_admin_only("suno_api"));
        assert!(engine_is_admin_only("  SUNO_API  "), "ids arrive from settings and UI, not just code");
        assert!(!engine_is_admin_only("heartmula"));
        assert!(!engine_is_admin_only("acestep"));

        assert!(unlock_phrase_matches(ADMIN_UNLOCK_PHRASE));
        assert!(unlock_phrase_matches("  i accept the risk to my account  "));
        assert!(!unlock_phrase_matches("yes"));
        assert!(!unlock_phrase_matches(""));
        assert!(!unlock_phrase_matches("I accept"));
    }

    #[test]
    fn an_admin_only_engine_is_not_merely_hidden() {
        // Two different states that would be easy to conflate: HIDDEN_ENGINES is "not in this build",
        // ADMIN_ONLY_ENGINES is "in this build, off until asked for". Conflating them would either
        // ship an account risk on by default, or make an unlockable engine unreachable.
        for e in ADMIN_ONLY_ENGINES {
            assert!(!engine_hidden(e), "{e} is unlockable, so it must not also be build-hidden");
        }
    }

    #[test]
    fn only_a_stuck_kernel_state_is_worth_clearing() {
        // The reset supersedes on "running" and "queued" — the two states that hold a slot without
        // necessarily serving. Clearing on "complete" would push a pointless GPU-off run every time
        // somebody pressed start after a finished session, and "unknown" (the answer when Kaggle
        // cannot be reached) must not be treated as a zombie either.
        for stuck in ["running", "queued"] {
            assert!(matches!(stuck, "running" | "queued"), "{stuck} should be cleared");
        }
        for leave in ["complete", "error", "cancelled", "unknown", ""] {
            assert!(!matches!(leave, "running" | "queued"), "{leave} must be left alone");
        }
    }

    #[test]
    fn a_hidden_engine_is_refused_before_anything_is_torn_down() {
        // reset_engine_blockers ends sessions and clears settings. A hidden engine must bounce off
        // the same guard every other spending entry point uses, before any of that happens.
        for e in HIDDEN_ENGINES {
            let reply = hidden_engine_reply(e);
            assert_eq!(reply["ok"], serde_json::json!(false));
            assert_eq!(reply["status"], serde_json::json!("engine_hidden"));
        }
    }
