use crate::{models::Settings, state::AppState};
use bson::{doc, Document};
use serde_json::Value;
use tauri::State;
use std::env;
use tokio::sync::mpsc;
use tokio::time::Duration;
use uuid::Uuid;
use warp::Filter;

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

async fn get_settings_doc(db: &mongodb::Database) -> Result<Value, String> {
    let doc = db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    Ok(doc)
}

pub async fn validate_suno_cookie_internal(db: &mongodb::Database) -> Result<(), String> {
    let s = get_settings_doc(db).await?;
    let cookie_env = std::env::var("SUNO_COOKIE").ok();
    let cookie = cookie_env.as_deref().unwrap_or_else(|| s["suno_cookie"].as_str().unwrap_or("")).trim();
    if cookie.is_empty() {
        // Persist invalid status
        let _ = db.collection::<Document>("settings")
            .update_one(doc! { "_id": "singleton" }, doc! { "$set": { "suno_cookie_valid": false, "suno_cookie_status": "not_configured", "suno_cookie_checked_at": chrono::Utc::now().to_rfc3339() } })
            .await;
        return Err("Suno cookie not configured".into());
    }
    let client = reqwest::Client::new();
    let res = client
        .get("https://studio-api.suno.com/api/user/")
        .header("Cookie", cookie)
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

pub async fn validate_mj_token_internal(db: &mongodb::Database) -> Result<(), String> {
    let s = get_settings_doc(db).await?;
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

pub async fn validate_google_refresh_tokens_internal(db: &mongodb::Database) -> Result<Vec<String>, String> {
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

pub async fn ensure_mj_autostart_internal(db: &mongodb::Database) -> Res<Value> {
    let coll = db.collection::<Document>("settings");
    let sdoc = coll.find_one(doc! { "_id": "singleton" }).await.map_err(e)?.unwrap_or_default();
    let s = bson_to_value(sdoc);
    if s["mj_autostart_installed"].as_bool().unwrap_or(false) {
        return Ok(serde_json::json!({ "ok": true, "installed": true }));
    }

    let resource_dir = if let Ok(rd) = std::env::var("TAURI_RESOURCE_DIR") {
        std::path::PathBuf::from(rd)
    } else if let Ok(exe) = std::env::current_exe() {
        exe.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| std::path::PathBuf::from("."))
    } else {
        std::path::PathBuf::from(".")
    };

    let mut scripts_dir = resource_dir.join("midjourney-proxy").join("scripts");
    if !scripts_dir.exists() {
        scripts_dir = resource_dir.join("scripts");
    }
    let packaging_dir = resource_dir.join("packaging");
    if !scripts_dir.exists() && !packaging_dir.exists() {
        return Ok(serde_json::json!({ "ok": false, "error": "scripts_missing", "path": resource_dir.to_string_lossy() }));
    }

    let os = std::env::consts::OS;
    let mut result = serde_json::Map::new();
    if os == "linux" {
        let install_sh = packaging_dir.join("install_midjourney_service.sh");
        let fallback_sh = scripts_dir.join("linux_install.sh");
        let run_sh = scripts_dir.join("run_app.sh");
        if install_sh.exists() {
            let _ = std::process::Command::new("sh").arg(install_sh.to_string_lossy().to_string()).arg(resource_dir.to_string_lossy().to_string()).spawn();
            result.insert("action".into(), "started_packaging_install_sh".into());
        } else if fallback_sh.exists() {
            let _ = std::process::Command::new("sh").arg(fallback_sh.to_string_lossy().to_string()).spawn();
            result.insert("action".into(), "started_fallback_linux_install".into());
        } else if run_sh.exists() {
            let _ = std::process::Command::new("sh").arg(run_sh.to_string_lossy().to_string()).spawn();
            result.insert("action".into(), "started_run_sh".into());
        } else {
            result.insert("error".into(), "no_script_found".into());
        }
    } else if os == "macos" {
        let install_sh = packaging_dir.join("install_midjourney_service.sh");
        let fallback_sh = scripts_dir.join("run_app_osx.sh");
        if install_sh.exists() {
            let _ = std::process::Command::new("sh").arg(install_sh.to_string_lossy().to_string()).arg(resource_dir.to_string_lossy().to_string()).spawn();
            result.insert("action".into(), "started_packaging_install_sh_mac".into());
        } else if fallback_sh.exists() {
            let _ = std::process::Command::new("sh").arg(fallback_sh.to_string_lossy().to_string()).spawn();
            result.insert("action".into(), "started_fallback_run_sh_mac".into());
        } else {
            result.insert("error".into(), "no_script_found".into());
        }
    } else if os == "windows" {
        let ps = packaging_dir.join("install_midjourney_service.ps1");
        if ps.exists() {
            let _ = std::process::Command::new("powershell").args(&["-ExecutionPolicy", "Bypass", "-File", &ps.to_string_lossy()]).spawn();
            result.insert("action".into(), "started_powershell".into());
        } else {
            result.insert("error".into(), "no_ps_found".into());
        }
    } else {
        result.insert("error".into(), "unsupported_os".into());
    }

    if result.get("action").is_some() {
        if let Some(url) = probe_midjourney_proxy().await {
            let _ = std::env::set_var("MJ_PROXY_URL", url.clone());
            let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "mj_proxy_url": &url } }).await;
            result.insert("proxy_url".into(), url.into());
        }
    }

    let installed = !result.contains_key("error");
    if installed {
        let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "mj_autostart_installed": true } }).await;
    }

    let mut out = serde_json::json!({ "ok": installed });
    if !result.is_empty() { out.as_object_mut().unwrap().extend(result); }
    Ok(out)
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
        .await.map_err(e)?;
    Ok(serde_json::to_value(payload).map_err(e)?)
}

#[tauri::command]
pub async fn test_suno(state: State<'_, AppState>) -> Res<Value> {
    let doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let cookie = doc["suno_cookie"].as_str().unwrap_or("").trim();
    
    if cookie.is_empty() {
        return Ok(serde_json::json!({
            "ok": false,
            "status": "not_configured",
            "detail": "Suno session cookie not configured.",
            "next_step": "1. Go to https://studio.suno.ai 2. Open DevTools (F12) 3. Cookies → studio.suno.ai → Copy 'studio-api_key' cookie 4. Paste in Settings",
            "expires": "Cookie expires after ~24 hours of inactivity"
        }));
    }
    
    // Test cookie validity with a lightweight API call
    let client = reqwest::Client::new();
    let test_res = client
        .get("https://studio-api.suno.com/api/user/")
        .header("Cookie", cookie)
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
                    "next_step": "Get a fresh cookie from https://studio.suno.ai and update in Settings"
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
pub async fn test_mj(state: State<'_, AppState>) -> Res<Value> {
    let doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    
    // Prefer auto-started proxy via env var
    let proxy_env = std::env::var("MJ_PROXY_URL").ok();
    let proxy = proxy_env.as_deref().unwrap_or_else(|| doc["mj_proxy_url"].as_str().unwrap_or("")).trim();
    let token_env = std::env::var("MJ_DISCORD_TOKEN").ok();
    let token = token_env.as_deref().unwrap_or_else(|| doc["mj_discord_token"].as_str().unwrap_or("")).trim();
    
    if proxy.is_empty() {
        return Ok(serde_json::json!({
            "ok": false,
            "status": "not_configured",
            "detail": "Midjourney proxy URL not configured. Required for image generation.",
            "next_step": "Enter Midjourney proxy URL in Settings (e.g., http://localhost:8086)",
            "setup_guide": "https://github.com/trueai-org/midjourney-proxy/blob/main/docs/install.md"
        }));
    }
    
    if token.is_empty() {
        return Ok(serde_json::json!({
            "ok": false,
            "status": "token_missing",
            "detail": "Midjourney proxy URL is set but Discord token is missing.",
            "next_step": "Add Discord user token in Settings",
            "info": "Token is used to authenticate with Discord (obtained from browser console)"
        }));
    }
    
    // Test actual connectivity to proxy
    let client = reqwest::Client::new();
    let mut req = client.get(format!("{}/info", proxy.trim_end_matches('/')))
        .timeout(std::time::Duration::from_secs(10));
    req = req.bearer_auth(token);
    
    match req.send().await {
        Ok(res) => {
            match res.status().as_u16() {
                200 => {
                    Ok(serde_json::json!({
                        "ok": true,
                        "status": "connected",
                        "detail": "Midjourney proxy is reachable and authenticated",
                        "proxy_url": proxy,
                        "note": "If the app auto-started the bundled proxy, it sets MJ_PROXY_URL. Obtain a Discord user token per the proxy docs (open browser DevTools, inspect websocket auth, or follow the proxy README)"
                    }))
                }
                401 | 403 => {
                    Ok(serde_json::json!({
                        "ok": false,
                        "status": "auth_failed",
                        "detail": format!("Proxy returned HTTP {} - Discord token is invalid or expired", res.status()),
                        "next_step": "Verify Discord user token is correct and still valid"
                    }))
                }
                _ => {
                    Ok(serde_json::json!({
                        "ok": false,
                        "status": "proxy_error",
                        "detail": format!("Proxy returned HTTP {} - check proxy logs", res.status())
                    }))
                }
            }
        }
        Err(err) => {
            let detail = if err.is_timeout() {
                format!("Connection timeout (10s) - proxy at {} is unreachable or too slow", proxy)
            } else if err.is_connect() {
                format!("Cannot connect to proxy at {} - check: (1) URL is correct, (2) proxy is running, (3) firewall allows connection", proxy)
            } else {
                format!("Connection error: {} - check network and proxy status", err)
            };
            Ok(serde_json::json!({
                "ok": false,
                "status": "connection_error",
                "detail": detail,
                "proxy_url": proxy
            }))
        }
    }
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
