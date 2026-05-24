use crate::{models::Settings, state::AppState};
use bson::{doc, Document};
use mongodb::options::UpdateOptions;
use serde_json::Value;
use tauri::State;
use std::env;
use std::path::PathBuf;

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
    
    match test_res {
        Ok(res) => {
            if res.status() == 200 {
                Ok(serde_json::json!({
                    "ok": true,
                    "status": "authenticated",
                    "detail": "Suno cookie is valid and working"
                }))
            } else if res.status() == 401 || res.status() == 403 {
                Ok(serde_json::json!({
                    "ok": false,
                    "status": "cookie_invalid",
                    "detail": "Cookie is invalid, expired, or revoked.",
                    "next_step": "Get a fresh cookie from https://studio.suno.ai and update in Settings"
                }))
            } else {
                Ok(serde_json::json!({
                    "ok": false,
                    "status": "api_error",
                    "detail": format!("Suno API returned HTTP {} - service may be unavailable", res.status())
                }))
            }
        }
        Err(err) => {
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
    // Idempotent: check settings flag and skip if already installed
    let coll = state.db.collection::<Document>("settings");
    let sdoc = coll.find_one(doc! { "_id": "singleton" }).await.map_err(e)?.unwrap_or_default();
    let s = bson_to_value(sdoc);
    if s["mj_autostart_installed"].as_bool().unwrap_or(false) {
        return Ok(serde_json::json!({ "ok": true, "installed": true }));
    }

    // Determine resource paths
    // Use Tauri handle to get resource dir in commands context: read env var TAURI_RESOURCE_DIR if set, otherwise try common locations
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

    let installed = !result.contains_key("error");
    if installed {
        let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "mj_autostart_installed": true } }).await;
    }

    let mut out = serde_json::json!({ "ok": installed });
    if !result.is_empty() { out.as_object_mut().unwrap().extend(result); }
    Ok(out)
}
