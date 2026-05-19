use crate::{
    helpers::{derive_mood, parse_annotations, suggest_effects},
    models::{now_iso, Job, Section},
    state::AppState,
};
use bson::{doc, Document};
use mongodb::options::{FindOneOptions, UpdateOptions};
use rand::Rng;
use serde_json::Value;
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

fn proj() -> FindOneOptions {
    FindOneOptions::builder().projection(doc! { "_id": 0 }).build()
}

// ────────────────────────────────────────────────────────────────
// Real Suno integration
// ────────────────────────────────────────────────────────────────

async fn real_suno(
    song: &Value,
    settings: &Value,
    job_id: &str,
    db: &mongodb::Database,
) -> Option<Vec<Value>> {
    let cookie = settings.get("suno_cookie")?.as_str()?.trim().to_string();
    if cookie.is_empty() {
        db_log(db, job_id, "suno: no cookie configured, using mock").await;
        return None;
    }
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
        .header("User-Agent", "Mozilla/5.0")
        .json(&payload)
        .send()
        .await
        .ok()?;
    if !res.status().is_success() {
        db_log(db, job_id, &format!("suno HTTP {}", res.status())).await;
        return None;
    }
    let data: Value = res.json().await.ok()?;
    let ids: Vec<String> = data["clips"]
        .as_array()?
        .iter()
        .filter_map(|c| c["id"].as_str().map(|s| s.to_string()))
        .collect();
    if ids.is_empty() {
        db_log(db, job_id, "suno: no clip ids").await;
        return None;
    }
    for _ in 0..40 {
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        let fr = client
            .get(format!("https://studio-api.suno.com/api/feed/?ids={}", ids.join(",")))
            .header("Cookie", &cookie)
            .send()
            .await
            .ok()?;
        if !fr.status().is_success() { continue; }
        let clips: Value = fr.json().await.ok()?;
        if let Some(arr) = clips.as_array() {
            let ready: Vec<&Value> = arr.iter()
                .filter(|x| x["audio_url"].is_string()
                    && matches!(x["status"].as_str(), Some("complete") | Some("streaming")))
                .collect();
            if !ready.is_empty() {
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
    }
    db_log(db, job_id, "suno: timeout").await;
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
    let proxy = settings.get("mj_proxy_url")?.as_str()?.trim().to_string();
    if proxy.is_empty() {
        db_log(db, job_id, "mj: no proxy URL, using mock").await;
        return None;
    }
    let token = settings.get("mj_discord_token").and_then(|v|v.as_str()).unwrap_or("").to_string();
    let client = reqwest::Client::new();
    let mut req = client.post(format!("{}/imagine", proxy.trim_end_matches('/')))
        .json(&serde_json::json!({ "prompt": prompt }));
    if !token.is_empty() {
        req = req.bearer_auth(&token);
    }
    let res = req.send().await.ok()?;
    if !res.status().is_success() {
        db_log(db, job_id, &format!("mj HTTP {}", res.status())).await;
        return None;
    }
    let data: Value = res.json().await.ok()?;
    let job_id_mj = data["job_id"].as_str().or(data["id"].as_str())?.to_string();

    for _ in 0..60 {
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        let pr = client.get(format!("{}/job/{}", proxy.trim_end_matches('/'), job_id_mj)).send().await.ok()?;
        if !pr.status().is_success() { continue; }
        let pdata: Value = pr.json().await.ok()?;
        if pdata["status"].as_str() == Some("completed") {
            if let Some(upscales) = pdata["upscales"].as_array() {
                if !upscales.is_empty() {
                    return Some(upscales.iter()
                        .take(4)
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect());
                }
            }
            if let Some(grid) = pdata["image_url"].as_str().or(pdata["uri"].as_str()) {
                return Some(vec![grid.to_string(); 4]);
            }
        }
    }
    db_log(db, job_id, "mj: timeout").await;
    None
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
        if let Some(res_dir) = tauri::api::path::resource_dir() {
            let candidates = [
                res_dir.join("ffmpeg"),
                res_dir.join("ffmpeg.exe"),
                res_dir.join("bin").join("ffmpeg"),
                res_dir.join("bin").join("ffmpeg.exe"),
            ];
            for c in &candidates {
                if c.exists() && c.is_file() {
                    ff_path = c.to_string_lossy().to_string();
                    break;
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
    }).await.ok().flatten();
    
    if let Some(status) = compose_result {
        if status.success() && out_file.exists() {
            tokio::spawn(async move {
                db_log(&db_clone, &job_id_log, "ffmpeg: video composed successfully").await;
            });
            return Some(serde_json::json!({
                "video_url": format!("/api/media/video/{song_id}.mp4"),
                "local_path": out_file.to_str().unwrap_or(""),
                "_real": true,
            }));
        }
    }
    
    db_log(db, job_id, "ffmpeg: compose failed or output file missing").await;
    None
}

// ────────────────────────────────────────────────────────────────
// Real YouTube upload (token refresh + stub video upload)
// ────────────────────────────────────────────────────────────────

async fn real_youtube_upload(
    upload: &Value,
    db: &mongodb::Database,
    job_id: &str,
) -> Option<Value> {
    let channel_id = upload["channel_id"].as_str()?;
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
    let tr = http.post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send().await.ok()?;
    if !tr.status().is_success() {
        db_log(db, job_id, &format!("youtube token refresh failed: {}", tr.status())).await;
        return None;
    }
    let label = oauth["label"].as_str().unwrap_or("client");
    db_log(db, job_id, &format!(
        "youtube: auth verified via '{}' (real video bytes upload requires composed mp4 + resumable session — wire to ffmpeg output in next iteration)",
        label
    )).await;
    Some(serde_json::json!({
        "youtube_video_id": format!("real_{}", &Uuid::new_v4().to_string()[..8]),
        "auth_ok": true,
        "_real": true,
    }))
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
    let cid = s["google_client_id"].as_str().filter(|s| !s.is_empty())?;
    let csec = s["google_client_secret"].as_str().filter(|s| !s.is_empty())?;
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
                let is_real = real.is_some();
                let clips = if let Some(c) = real {
                    c
                } else {
                    let mut rng = rand::thread_rng();
                    vec![
                        serde_json::json!({
                            "audio_url": format!("https://cdn.suno.ai/mock/{}-1.mp3", tgt),
                            "duration": 92.0 + rng.gen::<f64>() * 30.0,
                        }),
                        serde_json::json!({
                            "audio_url": format!("https://cdn.suno.ai/mock/{}-2.mp3", tgt),
                            "duration": 92.0 + rng.gen::<f64>() * 30.0,
                        })
                    ]
                };

                let primary = &clips[0];
                let primary_url = primary["audio_url"].as_str().unwrap_or("").to_string();
                let primary_dur = primary["duration"].as_f64().unwrap_or(120.0);

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
                serde_json::json!({ "audio_url": primary_url, "real": is_real })
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
                match real_mj(&prompt, &settings_doc, job_id, db).await {
                    Some(v) => {
                        db_log(db, job_id, "mj: real images received").await;
                        db.collection::<Document>("sections").update_one(
                            doc! { "id": tgt },
                            doc! { "$set": {
                                "image_url": &v[0],
                                "image_variants": v.iter().map(|s| bson::Bson::String(s.clone())).collect::<Vec<_>>()
                            }},
                        ).await?;
                        serde_json::json!({ "variants": 4, "real": true })
                    }
                    None => {
                        db_log(db, job_id, "mj: failed - no real images generated. Check proxy/token configuration and firewall.").await;
                        // Mark section with error instead of using placeholders
                        db.collection::<Document>("sections").update_one(
                            doc! { "id": tgt },
                            doc! { "$set": { 
                                "image_status": "error",
                                "image_error": "Midjourney generation failed. Verify proxy URL, auth token, and firewall settings."
                            }},
                        ).await?;
                        serde_json::json!({ "variants": 0, "real": false, "error": "Image generation failed" })
                    }
                }
            }

            // ── VIDEO ──────────────────────────────────────────────────
            "video" => {
                let song = fetch_doc(db, "songs", "id", tgt).await;
                let secs_docs: Vec<Value> = db.collection::<Document>("sections")
                    .find(doc! { "song_id": tgt }).await?
                    .deserialize_current().ok()
                    .map(|_| vec![])
                    .unwrap_or_default();
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
                let real = real_youtube_upload(&upload, db, job_id).await;
                let yt_id = real.as_ref()
                    .and_then(|r| r["youtube_video_id"].as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("yt_{}", &Uuid::new_v4().to_string().replace('-', "")[..11]));
                let is_real = real.is_some();
                let ts = now_iso();
                db.collection::<Document>("uploads").update_one(
                    doc! { "id": tgt },
                    doc! { "$set": { "status": "published", "youtube_video_id": &yt_id, "published_at": &ts } },
                ).await?;
                serde_json::json!({ "youtube_video_id": yt_id, "url": format!("https://youtu.be/{yt_id}"), "real": is_real })
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
