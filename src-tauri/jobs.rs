use crate::{
    helpers::{derive_mood, parse_annotations, resolve_node_executable, suggest_effects},
    models::{now_iso, Job, Section},
    state::AppState,
};
use bson::{doc, Document};
use serde_json::Value;
use std::env;
use std::sync::Arc;
use uuid::Uuid;

// ────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────

async fn db_log(db: &mongodb::Database, job_id: &str, msg: &str) {
    let ts = now_iso();
    let entry = format!("[{ts}] {msg}");
    let _ = db.collection::<Document>("jobs")
        .update_one(
            doc! { "id": job_id },
            doc! { "$push": { "logs": &entry }, "$set": { "updated_at": &ts } },
        )
        .await;
}

async fn set_progress(db: &mongodb::Database, job_id: &str, p: i32) {
    let ts = now_iso();
    let _ = db.collection::<Document>("jobs")
        .update_one(
            doc! { "id": job_id },
            doc! { "$set": { "progress": p, "updated_at": &ts },
                   "$push": { "logs": format!("[{ts}] progress {p}%") } },
        )
        .await;
}

// ────────────────────────────────────────────────────────────────
// Real Suno integration
// ────────────────────────────────────────────────────────────────

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

async fn real_suno(
    song: &Value,
    settings: &Value,
    job_id: &str,
    db: &mongodb::Database,
) -> Option<Vec<Value>> {
    let raw_cookie = settings.get("suno_cookie")?.as_str()?.trim();
    let cookie = match normalize_suno_cookie(raw_cookie) {
        Some(c) => c,
        None => {
            db_log(db, job_id, "suno: cookie not configured").await;
            return None;
        }
    };

    // Pre-check cookie validity before attempting generation
    let precheck_client = reqwest::Client::new();
    let precheck = precheck_client
        .get("https://studio-api.suno.com/api/user/")
        .header("Cookie", &cookie)
        .header("Accept", "application/json, text/plain, */*")
        .header("User-Agent", "Mozilla/5.0")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;
    match precheck {
        Ok(r) if r.status() == 200 => { /* cookie is valid */ }
        Ok(r) if r.status() == 401 || r.status() == 403 => {
            db_log(db, job_id, "suno: cookie has expired or is invalid. Please renew it in Settings (F12 → Cookies → suno.com → copy studio-api_key). Generation cancelled.").await;
            return None;
        }
        Ok(_) => {
            db_log(db, job_id, "suno: pre-check HTTP error — cookie may still be valid, proceeding anyway.").await;
        }
        Err(e) => {
            db_log(db, job_id, &format!("suno: pre-check connection error ({}), proceeding anyway.", e)).await;
        }
    }
    
    db_log(db, job_id, "suno: submitting music generation request...").await;
    
    let payload = serde_json::json!({
        "gpt_description_prompt": format!("{} — {}",
            song.get("styles").and_then(|v|v.as_str()).unwrap_or(""),
            song.get("title").and_then(|v|v.as_str()).unwrap_or("")),
        "make_instrumental": false,
        "mv": "chirp-v3-5",
        "prompt": song.get("lyrics").and_then(|v|v.as_str()).unwrap_or(""),
        "title": song.get("title").and_then(|v|v.as_str()).unwrap_or(""),
    });
    let client = reqwest::Client::new();
    let res = client
        .post("https://studio-api.suno.com/api/generate/v2/")
        .header("Cookie", &cookie)
        .header("Accept", "application/json, text/plain, */*")
        .header("User-Agent", "Mozilla/5.0")
        .json(&payload)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await;
    
    let res = match res {
        Ok(r) => r,
        Err(e) => {
            db_log(db, job_id, &format!("suno: request failed: {}", e)).await;
            return None;
        }
    };
    
    if !res.status().is_success() {
        let status = res.status();
        if status == 401 || status == 403 {
            db_log(db, job_id, "suno: authentication failed - cookie may be expired or invalid").await;
        } else {
            db_log(db, job_id, &format!("suno: HTTP {} - service error", status)).await;
        }
        return None;
    }
    
    let data: Value = match res.json().await {
        Ok(d) => d,
        Err(e) => {
            db_log(db, job_id, &format!("suno: invalid response format: {}", e)).await;
            return None;
        }
    };
    
    let ids: Vec<String> = data["clips"]
        .as_array()?
        .iter()
        .filter_map(|c| c["id"].as_str().map(|s| s.to_string()))
        .collect();
    
    if ids.is_empty() {
        db_log(db, job_id, "suno: no clip IDs returned - generation may have failed").await;
        return None;
    }
    
    db_log(db, job_id, &format!("suno: generation submitted, polling for results ({} clips)...", ids.len())).await;
    
    // Poll for results (up to 200 seconds)
    for attempt in 0..40 {
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        
        let fr = client
            .get(format!("https://studio-api.suno.com/api/feed/?ids={}", ids.join(",")))
            .header("Cookie", &cookie)
            .header("Accept", "application/json, text/plain, */*")
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await;
        
        let fr = match fr {
            Ok(f) => f,
            Err(e) => {
                db_log(db, job_id, &format!("suno: poll request {}/{} failed: {}", attempt + 1, 40, e)).await;
                continue;
            }
        };
        
        if !fr.status().is_success() {
            db_log(db, job_id, &format!("suno: poll {}/{} returned HTTP {}", attempt + 1, 40, fr.status())).await;
            continue;
        }
        
        let clips: Value = match fr.json().await {
            Ok(c) => c,
            Err(e) => {
                db_log(db, job_id, &format!("suno: poll {}/{} invalid response: {}", attempt + 1, 40, e)).await;
                continue;
            }
        };
        
        if let Some(arr) = clips.as_array() {
            let ready: Vec<&Value> = arr.iter()
                .filter(|x| x["audio_url"].is_string()
                    && matches!(x["status"].as_str(), Some("complete") | Some("streaming")))
                .collect();
            
            if !ready.is_empty() {
                db_log(db, job_id, &format!("suno: generation complete! {} clips ready", ready.len())).await;
                let mut results = Vec::new();
                for clip in ready.iter().take(2) {
                    results.push(serde_json::json!({
                        "audio_url": clip["audio_url"],
                        "duration": clip["metadata"]["duration"].as_f64().unwrap_or(120.0),
                    }));
                }
                return Some(results);
            }
        }
        
        set_progress(db, job_id, 15 + (attempt as i32 * 70 / 40).min(70)).await;
    }
    
    db_log(db, job_id, "suno: timeout after 200 seconds - generation did not complete").await;
    None
}

// ────────────────────────────────────────────────────────────────
// Real Midjourney (proxy) integration
// ────────────────────────────────────────────────────────────────

async fn real_mj(
    prompt: &str,
    settings: &Value,
    job_id: &str,
    db: &mongodb::Database,
) -> Option<Vec<String>> {
    // New Playwright-driven flow: spawn a Node helper which controls a visible
    // browser to submit the prompt and download resulting images. The helper
    // will print a JSON array of saved image paths to stdout on success.
    // Prefer a persistent Playwright profile captured earlier instead of cookies
    let mj_profile = settings.get("mj_profile_dir").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if mj_profile.is_empty() {
        db_log(db, job_id, "mj: mj_profile_dir not configured - open Settings → Capture session").await;
        return None;
    }

    db_log(db, job_id, "mj: launching Playwright generator (visible browser)...").await;

    let out_dir = format!("/tmp/studio_mj_{}", Uuid::new_v4());
    let _ = tokio::fs::create_dir_all(&out_dir).await;

    let node = resolve_node_executable();
    let node = match node {
        Some(p) => p,
        None => {
            db_log(db, job_id, "mj: 'node' runtime not found in PATH or bundled resources").await;
            return None;
        }
    };

    let script = std::path::Path::new("src-tauri").join("packaging").join("midjourney-generator.js");
    if !script.exists() {
        db_log(db, job_id, "mj: generator script missing").await;
        return None;
    }

    let mut cmd = tokio::process::Command::new(node);
    cmd.arg(script.to_string_lossy().to_string())
        .arg("--prompt")
        .arg(prompt.to_string())
        .arg("--profile")
        .arg(mj_profile.clone())
        .arg("--outdir")
        .arg(out_dir.clone())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            db_log(db, job_id, &format!("mj: failed to spawn generator: {}", e)).await;
            return None;
        }
    };

    // Capture PID before moving `child` into wait_with_output
    let child_pid = child.id();

    // Wait for generator with a 6 minute timeout
    let timeout = tokio::time::Duration::from_secs(360);
    let wait = tokio::time::timeout(timeout, child.wait_with_output());
    match wait.await {
        Ok(Ok(output)) => {
            if !output.status.success() {
                let err = String::from_utf8_lossy(&output.stderr);
                db_log(db, job_id, &format!("mj: generator failed: {}", err)).await;
                return None;
            }
            let out = String::from_utf8_lossy(&output.stdout).to_string();
            let parsed: Result<Vec<String>, _> = serde_json::from_str(&out);
            match parsed {
                Ok(v) if !v.is_empty() => {
                    db_log(db, job_id, &format!("mj: generator returned {} images", v.len())).await;
                    return Some(v);
                }
                Ok(_) => {
                    db_log(db, job_id, "mj: generator returned no images").await;
                    return None;
                }
                Err(e) => {
                    db_log(db, job_id, &format!("mj: failed parsing generator output: {}", e)).await;
                    return None;
                }
            }
        }
        Ok(Err(e)) => {
            db_log(db, job_id, &format!("mj: generator process error: {}", e)).await;
            return None;
        }
        Err(_) => {
            db_log(db, job_id, "mj: generator timed out").await;
            // Try to kill by pid if available
            if let Some(pid) = child_pid {
                let _ = std::process::Command::new("kill").arg("-9").arg(pid.to_string()).spawn();
            }
            return None;
        }
    }
}

// ────────────────────────────────────────────────────────────────
// Real FFmpeg video composition
// ────────────────────────────────────────────────────────────────

async fn real_ffmpeg(
    song: &Value,
    sections: &[Value],
    settings: &Value,
    job_id: &str,
    db: &mongodb::Database,
) -> Option<Value> {
    let ff = settings.get("ffmpeg_path").and_then(|v|v.as_str()).unwrap_or("ffmpeg");
    // Resolve ffmpeg: prefer configured path, then system `which`, then bundled resource
    let mut ff_path = ff.to_string();
    if which::which(&ff_path).is_err() {
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
                        ff_path = c.to_string_lossy().to_string();
                        break;
                    }
                }
            }
        }
    }
    if which::which(&ff_path).is_err() {
        db_log(db, job_id, &format!("ffmpeg: '{}' not found", ff)).await;
        return None;
    }
    let song_id = song["id"].as_str()?;
    let out_dir = std::path::PathBuf::from(format!("/tmp/studio_out/{song_id}"));
    let _ = tokio::fs::create_dir_all(&out_dir).await;
    let out_file = out_dir.join("video.mp4");

    let images: Vec<&str> = sections.iter()
        .filter_map(|s| s["image_url"].as_str())
        .collect();
    let audio_url = song["audio_url"].as_str();
    if images.is_empty() || audio_url.is_none() {
        db_log(db, job_id, "ffmpeg: missing images or audio_url").await;
        return None;
    }
    
    db_log(db, job_id, &format!("ffmpeg: downloading {} images...", images.len())).await;
    
    let client = reqwest::Client::new();
    for (i, url) in images.iter().enumerate() {
        // Non-blocking download with progress updates
        match client.get(*url).send().await {
            Ok(res) => {
                match res.bytes().await {
                    Ok(bytes) => {
                        let path = out_dir.join(format!("img_{i:03}.jpg"));
                        if let Err(_) = tokio::fs::write(&path, bytes).await {
                            db_log(db, job_id, &format!("ffmpeg: failed to save image {}", i)).await;
                        } else {
                            db_log(db, job_id, &format!("ffmpeg: downloaded image {}/{}", i+1, images.len())).await;
                        }
                    }
                    Err(_) => db_log(db, job_id, &format!("ffmpeg: failed to read image {}", i)).await,
                }
            }
            Err(_) => db_log(db, job_id, &format!("ffmpeg: failed to fetch image {}", i)).await,
        }
        // Yield control to prevent blocking
        tokio::task::yield_now().await;
    }
    
    db_log(db, job_id, "ffmpeg: downloading audio...").await;
    if let Ok(res) = client.get(audio_url.unwrap()).send().await {
        if let Ok(bytes) = res.bytes().await {
            if let Err(_) = tokio::fs::write(out_dir.join("audio.mp3"), bytes).await {
                db_log(db, job_id, "ffmpeg: failed to save audio").await;
                return None;
            }
            db_log(db, job_id, "ffmpeg: audio downloaded").await;
        } else {
            db_log(db, job_id, "ffmpeg: failed to read audio").await;
            return None;
        }
    } else {
        db_log(db, job_id, "ffmpeg: failed to fetch audio").await;
        return None;
    }
    
    db_log(db, job_id, "ffmpeg: composing video (this may take a minute)...").await;
    let duration = song["duration"].as_f64().unwrap_or(120.0);
    let per = (duration / images.len().max(1) as f64).max(2.0) as u64;
    
    // Use tokio spawn_blocking to prevent blocking the async runtime
    let ff_path = ff.to_string();
    let img_pattern = out_dir.join("img_%03d.jpg").to_string_lossy().to_string();
    let audio_path = out_dir.join("audio.mp3").to_string_lossy().to_string();
    let output_path = out_file.to_string_lossy().to_string();
    let job_id_log = job_id.to_string();
    let db_clone = db.clone();
    
    let compose_result = tokio::task::spawn_blocking(move || {
        std::process::Command::new(&ff_path)
            .args(["-y", "-framerate", &format!("1/{per}"),
                   "-i", &img_pattern,
                   "-i", &audio_path,
                   "-c:v", "libx264", "-pix_fmt", "yuv420p",
                   "-c:a", "aac", "-shortest",
                   &output_path])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .status()
    }).await.ok().and_then(|r| r.ok());
    
    if let Some(status) = compose_result {
        if status.success() && out_file.exists() {
            tokio::spawn(async move {
                db_log(&db_clone, &job_id_log, "ffmpeg: video composed successfully").await;
            });
            let local_path_str = out_file.to_str().unwrap_or("").to_string();
            let video_url = format!("/api/media/video/{song_id}.mp4");
            let _ = db.collection::<Document>("songs")
                .update_one(doc! { "id": song_id }, doc! { "$set": {
                    "video_url": &video_url,
                    "video_local_path": &local_path_str,
                    "status": "video_ready"
                }}).await;
            return Some(serde_json::json!({
                "video_url": video_url,
                "local_path": local_path_str,
                "_real": true,
            }));
        }
    }

    db_log(db, job_id, "ffmpeg: compose failed or output file missing").await;
    None
}

// ────────────────────────────────────────────────────────────────
// Real YouTube upload (token refresh + actual resumable session upload)
// ────────────────────────────────────────────────────────────────

async fn real_youtube_upload(
    upload: &Value,
    db: &mongodb::Database,
    job_id: &str,
) -> Option<Value> {
    let channel_id = upload["channel_id"].as_str()?;
    let song_id = upload["song_id"].as_str()?;
    
    // Fetch channel
    let channel = db.collection::<Document>("channels")
        .find_one(doc! { "id": channel_id }).await.ok()??.into_iter()
        .fold(serde_json::Map::new(), |mut m, (k, v)| {
            m.insert(k, bson::Bson::try_into(v).unwrap_or(Value::Null));
            m
        });
    let channel: Value = Value::Object(channel);
    let refresh_token = channel["refresh_token"].as_str()?;

    // Resolve OAuth client
    let oauth = pick_oauth_client(db, &channel, channel["oauth_client_id"].as_str()).await?;
    let client_id = oauth["client_id"].as_str()?;
    let client_secret = oauth["client_secret"].as_str()?;

    let http = reqwest::Client::new();
    
    // Step 1: Refresh access token
    db_log(db, job_id, "youtube: refreshing access token...").await;
    let tr = http.post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send().await.ok()?;
    if !tr.status().is_success() {
        db_log(db, job_id, &format!("youtube: token refresh failed ({}). Token may be invalid or revoked.", tr.status())).await;
        return None;
    }
    let tokens: Value = tr.json().await.ok()?;
    let access_token = tokens["access_token"].as_str()?;
    db_log(db, job_id, "youtube: access token refreshed successfully").await;

    // Step 2: Get song to find video file
    let song_doc = db.collection::<Document>("songs")
        .find_one(doc! { "id": song_id }).await.ok()??.into_iter()
        .fold(serde_json::Map::new(), |mut m, (k, v)| {
            m.insert(k, bson::Bson::try_into(v).unwrap_or(Value::Null));
            m
        });
    let song: Value = Value::Object(song_doc);
    let video_url = song["video_url"].as_str().unwrap_or("");
    let video_local_path = song["video_local_path"].as_str().unwrap_or("");
    
    let video_bytes = if !video_local_path.is_empty() {
        db_log(db, job_id, &format!("youtube: using local composed video file {}", video_local_path)).await;
        match std::fs::read(video_local_path) {
            Ok(bytes) => bytes,
            Err(e) => {
                db_log(db, job_id, &format!("youtube: failed to read local video file {}: {}", video_local_path, e)).await;
                return None;
            }
        }
    } else if !video_url.is_empty() && video_url.starts_with("http") {
        db_log(db, job_id, &format!("youtube: downloading video from {}", video_url)).await;
        match download_video(video_url, job_id, db).await {
            Some(bytes) => bytes,
            None => {
                db_log(db, job_id, "youtube: failed to download video file").await;
                return None;
            }
        }
    } else {
        db_log(db, job_id, "youtube: no valid video source available for upload").await;
        return None;
    };
    
    db_log(db, job_id, &format!("youtube: video file size: {:.1} MB", video_bytes.len() as f64 / (1024.0 * 1024.0))).await;

    // Step 4: Prepare upload metadata
    let title = upload["title"].as_str().unwrap_or("Untitled").to_string();
    let description = upload["description"].as_str().unwrap_or("").to_string();
    let tags: Vec<String> = upload["tags"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let category = upload["category"].as_str().unwrap_or("Music").to_string();
    let privacy = upload["privacy"].as_str().unwrap_or("private").to_string();

    // Convert privacy string to YouTube API value
    let privacy_status = match privacy.as_str() {
        "public" => "public",
        "unlisted" => "unlisted",
        _ => "private",
    };

    let metadata = serde_json::json!({
        "snippet": {
            "title": title,
            "description": description,
            "tags": tags,
            "categoryId": match category.as_str() {
                "Music" => "10",
                "Entertainment" => "24",
                "People" => "15",
                _ => "24",
            }
        },
        "status": {
            "privacyStatus": privacy_status,
            "madeForKids": false
        }
    });

    // Step 5: Initiate resumable upload session
    db_log(db, job_id, "youtube: initiating resumable upload session...").await;
    let session_res = http.post("https://www.googleapis.com/youtube/v3/videos")
        .query(&[("part", "snippet,status"), ("uploadType", "resumable")])
        .bearer_auth(access_token)
        .header("X-Goog-Upload-Protocol", "resumable")
        .header("X-Goog-Upload-Command", "start")
        .header("X-Goog-Upload-Content-Type", "video/mp4")
        .header("X-Goog-Upload-Content-Length", &video_bytes.len().to_string())
        .json(&metadata)
        .send().await.ok()?;

    if !session_res.status().is_success() {
        db_log(db, job_id, &format!("youtube: upload session init failed ({})", session_res.status())).await;
        return None;
    }

    let session_uri = session_res.headers().get("location")?.to_str().ok()?;
    db_log(db, job_id, "youtube: upload session created, starting file transfer...").await;

    // Step 6: Upload video bytes in chunks (resumable protocol)
    let chunk_size = 262144; // 256KB chunks
    let total_chunks = (video_bytes.len() + chunk_size - 1) / chunk_size;
    
    for (chunk_idx, chunk) in video_bytes.chunks(chunk_size).enumerate() {
        let start = chunk_idx * chunk_size;
        let end = start + chunk.len();
        let is_final = end >= video_bytes.len();
        
        let range_header = format!("bytes {}-{}/{}", start, end - 1, video_bytes.len());
        
        let mut chunk_req = http.put(session_uri)
            .header("Content-Type", "video/mp4")
            .header("Content-Range", &range_header);
        
        chunk_req = if is_final {
            chunk_req
                .header("X-Goog-Upload-Command", "upload, finalize")
        } else {
            chunk_req
                .header("X-Goog-Upload-Command", "upload")
        };

        let chunk_res = chunk_req.body(chunk.to_vec()).send().await.ok()?;
        
        if is_final {
            if chunk_res.status().is_success() {
                if let Ok(result) = chunk_res.json::<Value>().await {
                    if let Some(video_id) = result["id"].as_str() {
                        db_log(db, job_id, &format!("youtube: upload complete! Video ID: {}", video_id)).await;
                        return Some(serde_json::json!({
                            "youtube_video_id": video_id,
                            "auth_ok": true,
                            "_real": true,
                            "privacy": privacy_status,
                        }));
                    }
                }
            } else {
                db_log(db, job_id, &format!("youtube: final upload chunk failed ({})", chunk_res.status())).await;
                return None;
            }
        } else {
            if !chunk_res.status().is_success() && chunk_res.status().as_u16() != 308 {
                db_log(db, job_id, &format!("youtube: chunk {} upload failed ({})", chunk_idx + 1, chunk_res.status())).await;
                return None;
            }
            set_progress(db, job_id, 15 + (chunk_idx as i32 * 70 / total_chunks as i32).min(70)).await;
            db_log(db, job_id, &format!("youtube: uploaded chunk {}/{}", chunk_idx + 1, total_chunks)).await;
        }
    }
    
    None
}

async fn download_video(url: &str, job_id: &str, db: &mongodb::Database) -> Option<Vec<u8>> {
    let client = reqwest::Client::new();
    
    let res = client.get(url).send().await.ok()?;
    if !res.status().is_success() {
        db_log(db, job_id, &format!("download_video: HTTP {} from {}", res.status(), url)).await;
        return None;
    }
    
    // If it's a local file:// URL, read from disk instead
    if url.starts_with("file://") || url.starts_with("/") {
        let path = if url.starts_with("file://") {
            url.trim_start_matches("file://")
        } else {
            url
        };
        match std::fs::read(path) {
            Ok(bytes) => return Some(bytes),
            Err(_) => {
                db_log(db, job_id, &format!("download_video: failed to read local file {}", path)).await;
                return None;
            }
        }
    }
    
    match res.bytes().await {
        Ok(bytes) => Some(bytes.to_vec()),
        Err(e) => {
            db_log(db, job_id, &format!("download_video: failed to read response body: {}", e)).await;
            None
        }
    }
}

// ────────────────────────────────────────────────────────────────
// OAuth client resolution (shared helper)
// ────────────────────────────────────────────────────────────────

pub async fn pick_oauth_client(
    db: &mongodb::Database,
    channel: &Value,
    forced_id: Option<&str>,
) -> Option<Value> {
    let coll = db.collection::<Document>("oauth_clients");

    if let Some(fid) = forced_id {
        if let Ok(Some(doc)) = coll.find_one(doc! { "id": fid }).await {
            return Some(bson_doc_to_value(doc));
        }
    }
    if let Some(oid) = channel["oauth_client_id"].as_str() {
        if let Ok(Some(doc)) = coll.find_one(doc! { "id": oid }).await {
            return Some(bson_doc_to_value(doc));
        }
    }
    if let Some(lang) = channel["language"].as_str() {
        if let Ok(Some(doc)) = coll.find_one(doc! { "languages": lang }).await {
            return Some(bson_doc_to_value(doc));
        }
    }
    // Legacy settings fallback
    let s = db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.ok()??.into_iter()
        .fold(serde_json::Map::new(), |mut m, (k, v)| {
            if let Ok(jv) = bson::from_bson::<Value>(v) { m.insert(k, jv); }
            m
        });
    let s = Value::Object(s);
    // Accept legacy fallback when `google_client_id` is present. The client
    // secret may be empty (UI can supply it later); building the consent URL
    // only requires the client_id.
    let cid = s["google_client_id"].as_str().filter(|s| !s.is_empty())?;
    let csec = s["google_client_secret"].as_str().filter(|s| !s.is_empty()).unwrap_or("");
    Some(serde_json::json!({
        "id": "_legacy",
        "label": "Settings default",
        "client_id": cid,
        "client_secret": csec,
        "redirect_uri": s["google_redirect_uri"].as_str().unwrap_or(""),
        "languages": [],
    }))
}

fn bson_doc_to_value(doc: Document) -> Value {
    let mut m = serde_json::Map::new();
    for (k, v) in doc {
        if k == "_id" { continue; }
        if let Ok(jv) = bson::from_bson::<Value>(v) { m.insert(k, jv); }
    }
    Value::Object(m)
}

// ────────────────────────────────────────────────────────────────
// Job enqueue
// ────────────────────────────────────────────────────────────────

pub async fn enqueue(
    kind: &str,
    target_id: &str,
    state: &Arc<AppState>,
) -> anyhow::Result<Job> {
    let job = Job {
        id: Uuid::new_v4().to_string(),
        kind: kind.to_string(),
        target_id: target_id.to_string(),
        status: "queued".into(),
        progress: 0,
        logs: vec![],
        attempts: 0,
        created_at: now_iso(),
        updated_at: now_iso(),
        error: None,
        result: Value::Object(serde_json::Map::new()),
    };
    let bson_doc = bson::to_document(&job)?;
    state.db.collection::<Document>("jobs").insert_one(bson_doc).await?;

    let job_id = job.id.clone();
    let state_clone = Arc::clone(state);
    tokio::spawn(async move {
        run_job(&job_id, &state_clone).await;
    });
    Ok(job)
}

// ────────────────────────────────────────────────────────────────
// Job runner
// ────────────────────────────────────────────────────────────────

pub async fn run_job(job_id: &str, state: &Arc<AppState>) {
    let db = &state.db;
    let ts = now_iso();
    let _ = db.collection::<Document>("jobs")
        .update_one(
            doc! { "id": job_id },
            doc! { "$set": { "status": "running", "updated_at": &ts },
                   "$push": { "logs": format!("[{ts}] starting") } },
        )
        .await;

    // Fetch job document
    let job_doc = match db.collection::<Document>("jobs")
        .find_one(doc! { "id": job_id })
        .await
    {
        Ok(Some(d)) => d,
        _ => return,
    };
    let job: Value = bson_doc_to_value(job_doc);

    // Fetch settings
    let settings_doc = db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.unwrap_or(None)
        .map(bson_doc_to_value)
        .unwrap_or_default();

    // Progress ticks
    for p in [15, 35, 60, 85] {
        tokio::time::sleep(tokio::time::Duration::from_millis(600)).await;
        set_progress(db, job_id, p).await;
    }

    let kind = job["kind"].as_str().unwrap_or("");
    let tgt  = job["target_id"].as_str().unwrap_or("");

    let run_result: anyhow::Result<Value> = async {
        Ok(match kind {
            // ── MUSIC ──────────────────────────────────────────────────
            "music" => {
                let song = fetch_doc(db, "songs", "id", tgt).await;
                let real = real_suno(&song, &settings_doc, job_id, db).await;
                let clips = real.ok_or_else(|| anyhow::anyhow!(
                    "Suno music generation failed. Check: (1) Cookie validity, (2) Network connectivity, (3) Suno service status. See job logs for details."
                ))?;

                let primary = &clips[0];
                let primary_url = primary["audio_url"].as_str().unwrap_or("").to_string();
                let primary_dur = primary["duration"].as_f64().unwrap_or(120.0);

                if primary_url.is_empty() {
                    return Err(anyhow::anyhow!("Suno returned empty audio URL"));
                }

                let (alt_url, alt_dur) = if clips.len() > 1 {
                    let alt = &clips[1];
                    (alt["audio_url"].as_str().unwrap_or("").to_string(),
                     alt["duration"].as_f64().unwrap_or(120.0))
                } else {
                    ("".to_string(), 0.0)
                };

                db.collection::<Document>("songs").update_one(
                    doc! { "id": tgt },
                    doc! { "$set": { 
                        "audio_url": &primary_url, 
                        "duration": primary_dur,
                        "audio_url_primary": &primary_url,
                        "duration_primary": primary_dur,
                        "audio_url_alt": &alt_url,
                        "duration_alt": alt_dur,
                        "status": "music_ready" 
                    } },
                ).await?;
                serde_json::json!({ "audio_url": primary_url, "real": true })
            }

            // ── ANALYSIS ───────────────────────────────────────────────
            "analysis" => {
                let song = fetch_doc(db, "songs", "id", tgt).await;
                let annotations = song["annotations"].as_str().unwrap_or("");
                let lyrics      = song["lyrics"].as_str().unwrap_or("");
                let pairs = parse_annotations(annotations, lyrics);
                let n = pairs.len().max(1);
                let duration = song["duration"].as_f64().unwrap_or(110.0);
                let seg = duration / n as f64;

                db.collection::<Document>("sections").delete_many(doc! { "song_id": tgt }).await?;
                let moods: Vec<&'static str> = pairs.iter()
                    .map(|p| derive_mood(if p.image_prompt.is_empty() { &p.line } else { &p.image_prompt }))
                    .collect();

                let mut secs: Vec<bson::Document> = Vec::new();
                for (i, p) in pairs.iter().enumerate() {
                    let sec = Section {
                        id: Uuid::new_v4().to_string(),
                        song_id: tgt.to_string(),
                        index: i as i32,
                        start: (i as f64 * seg * 100.0).round() / 100.0,
                        end: ((i + 1) as f64 * seg * 100.0).round() / 100.0,
                        line: p.line.clone(),
                        image_prompt: p.image_prompt.clone(),
                        mood: moods[i].to_string(),
                        mood_prev: if i > 0 { moods[i-1].to_string() } else { String::new() },
                        mood_next: if i < n-1 { moods[i+1].to_string() } else { String::new() },
                        image_url: None,
                        image_variants: vec![],
                        is_video: false,
                        effects: suggest_effects(moods[i]),
                    };
                    secs.push(bson::to_document(&sec)?);
                }
                let count = secs.len();
                if !secs.is_empty() {
                    db.collection::<Document>("sections").insert_many(secs).await?;
                }
                db.collection::<Document>("songs").update_one(
                    doc! { "id": tgt }, doc! { "$set": { "status": "analyzed" } },
                ).await?;
                serde_json::json!({ "sections": count })
            }

            // ── IMAGE ──────────────────────────────────────────────────
            "image" => {
                let sec = fetch_doc(db, "sections", "id", tgt).await;
                let prompt = sec["image_prompt"].as_str()
                    .filter(|s| !s.is_empty())
                    .or(sec["line"].as_str())
                    .unwrap_or("")
                    .to_string();
                let v = real_mj(&prompt, &settings_doc, job_id, db).await
                    .ok_or_else(|| anyhow::anyhow!(
                        "Midjourney image generation failed. Verify: (1) Proxy URL is set and accessible, (2) Discord token is valid, (3) Firewall allows outbound HTTPS. See logs for details."
                    ))?;
                
                db_log(db, job_id, &format!("mj: received {} image variants", v.len())).await;
                db.collection::<Document>("sections").update_one(
                    doc! { "id": tgt },
                    doc! { "$set": {
                        "image_url": &v[0],
                        "image_variants": v.iter().map(|s| bson::Bson::String(s.clone())).collect::<Vec<_>>()
                    }},
                ).await?;
                serde_json::json!({ "variants": v.len(), "real": true })
            }

            // ── VIDEO ──────────────────────────────────────────────────
            "video" => {
                let song = fetch_doc(db, "songs", "id", tgt).await;
                // Proper cursor iteration
                let mut cursor = db.collection::<Document>("sections")
                    .find(doc! { "song_id": tgt }).await?;
                let mut secs_docs: Vec<Value> = Vec::new();
                use futures_util::StreamExt;
                while let Some(Ok(d)) = cursor.next().await {
                    secs_docs.push(bson_doc_to_value(d));
                }
                let real = real_ffmpeg(&song, &secs_docs, &settings_doc, job_id, db).await;
                let video_url = real.as_ref()
                    .and_then(|r| r["video_url"].as_str())
                    .unwrap_or(&format!("/api/media/video/{tgt}.mp4"))
                    .to_string();
                let is_real = real.is_some();
                db.collection::<Document>("songs").update_one(
                    doc! { "id": tgt },
                    doc! { "$set": { "status": "video_ready", "video_url": &video_url } },
                ).await?;
                serde_json::json!({ "video_url": video_url, "real": is_real })
            }

            // ── UPLOAD ─────────────────────────────────────────────────
            "upload" => {
                let upload = fetch_doc(db, "uploads", "id", tgt).await;
                let yt_id = real_youtube_upload(&upload, db, job_id).await
                    .ok_or_else(|| anyhow::anyhow!(
                        "YouTube upload failed. Verify: (1) Channel has valid refresh token, (2) Video file exists, (3) OAuth credentials are active. See logs for details."
                    ))?
                    .get("youtube_video_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("YouTube API did not return video ID"))?
                    .to_string();
                
                let ts = now_iso();
                db.collection::<Document>("uploads").update_one(
                    doc! { "id": tgt },
                    doc! { "$set": { "status": "published", "youtube_video_id": &yt_id, "published_at": &ts } },
                ).await?;
                
                db_log(db, job_id, &format!("upload: completed! YouTube URL: https://youtu.be/{}", yt_id)).await;
                serde_json::json!({ "youtube_video_id": yt_id, "url": format!("https://youtu.be/{yt_id}"), "real": true })
            }

            _ => serde_json::json!({}),
        })
    }.await;

    let ts = now_iso();
    match run_result {
        Ok(result) => {
            let bson_result = bson::to_bson(&result).unwrap_or(bson::Bson::Null);
            let _ = db.collection::<Document>("jobs")
                .update_one(
                    doc! { "id": job_id },
                    doc! { "$set": { "status": "done", "progress": 100, "result": bson_result, "updated_at": &ts },
                           "$push": { "logs": format!("[{ts}] done") } },
                ).await;
        }
        Err(e) => {
            let _ = db.collection::<Document>("jobs")
                .update_one(
                    doc! { "id": job_id },
                    doc! { "$set": { "status": "failed", "error": e.to_string(), "updated_at": &ts },
                           "$push": { "logs": format!("[{ts}] error: {e}") } },
                ).await;
        }
    }
}

async fn fetch_doc(db: &mongodb::Database, coll: &str, key: &str, val: &str) -> Value {
    db.collection::<Document>(coll)
        .find_one(doc! { key: val }).await.ok().flatten()
        .map(bson_doc_to_value)
        .unwrap_or_default()
}
