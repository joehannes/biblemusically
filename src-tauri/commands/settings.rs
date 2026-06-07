use crate::{models::Settings, state::AppState};
use bson::{doc, Document};
use serde_json::Value;
use tauri::State;
use std::env;
use std::path::PathBuf;
use tokio::fs;
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

fn locate_resource_file(name: &str) -> Option<PathBuf> {
    // 1. Tauri resource directory (used in built/release app)
    //    Resources are bundled under "packaging/" or directly.
    if let Ok(rd) = env::var("TAURI_RESOURCE_DIR") {
        let p = PathBuf::from(&rd).join(name);
        if p.exists() { return Some(p); }
        let p2 = PathBuf::from(&rd).join("packaging").join(name);
        if p2.exists() { return Some(p2); }
    }
    // 2. Current working directory (dev mode from project root)
    if let Ok(cwd) = env::current_dir() {
        let p = cwd.join("src-tauri").join("packaging").join(name);
        if p.exists() { return Some(p); }
        let p2 = cwd.join("src-tauri").join(name);
        if p2.exists() { return Some(p2); }
    }
    // 3. Walk up from executable parent directories.
    //    Tauri v2 bundles resources as: /exe/dir/_up_/src-tauri/packaging/<name>
    //    or during deb: /usr/lib/<app>/_up_/src-tauri/packaging/<name>
    if let Ok(exe) = env::current_exe() {
        let mut path = exe.parent();
        while let Some(dir) = path {
            // Check _up_/src-tauri/packaging/ (Tauri v2 default resource layout)
            let up = dir.join("_up_").join("src-tauri").join("packaging").join(name);
            if up.exists() { return Some(up); }
            // Check _up_/ packaging directly
            let up2 = dir.join("_up_").join("packaging").join(name);
            if up2.exists() { return Some(up2); }
            // Check packaging/ alongside the binary
            let p = dir.join("packaging").join(name);
            if p.exists() { return Some(p); }
            // Check src-tauri/packaging/ one level up
            if let Some(parent) = dir.parent() {
                let p2 = parent.join("src-tauri").join("packaging").join(name);
                if p2.exists() { return Some(p2); }
            }
            path = dir.parent();
        }
    }
    // 4. Fallback: check known .deb install paths
    let candidates = [
        format!("/usr/lib/AI Music Video Studio/_up_/src-tauri/packaging/{}", name),
        format!("/usr/lib/AI Music Video Studio/packaging/{}", name),
    ];
    for c in &candidates {
        if std::path::Path::new(c).exists() {
            return Some(std::path::PathBuf::from(c));
        }
    }
    None
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
    // Midjourney proxy autostart is deprecated. Use the visible Playwright
    // driven workflow and the Settings → Capture session flow to obtain a
    // Playwright profile directory (stored as `mj_profile_dir`). This function remains for API compatibility and no
    // service is installed.
    Ok(serde_json::json!({ "ok": true, "installed": false, "note": "midjourney-proxy autostart removed; use Playwright-based flow" }))
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
            "next_step": "1. Go to https://suno.com 2. Open DevTools (F12) 3. Cookies → suno.com → Copy 'studio-api_key' cookie 4. Paste in Settings",
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
        if let Ok(node) = which::which("node") {
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

#[tauri::command]
pub async fn capture_midjourney_session(state: State<'_, AppState>) -> Res<Value> {
    let script = locate_resource_file("midjourney-session-capture.js")
        .ok_or_else(|| "Midjourney capture script not found in resources".to_string())?;
    let node = which::which("node").map_err(|_| "Node.js is required for Midjourney session automation. Install Node.js and npm.".to_string())?;
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
    let node = which::which("node").map_err(|_| "Node.js is required for Suno session automation. Install Node.js and npm.".to_string())?;
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


#[tauri::command]
pub async fn generate_mj_now(state: State<'_, AppState>, prompt: String) -> Res<Value> {
    // Immediate generation via Playwright generator script. Returns saved image paths.
    let script = locate_resource_file("midjourney-generator.js").ok_or_else(|| "Generator script not found".to_string())?;
    let node = which::which("node").map_err(|_| "Node.js is required to run generator".to_string())?;

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
