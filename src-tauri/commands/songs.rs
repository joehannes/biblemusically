use crate::{commands::projects::slugify, jobs::enqueue, state::AppState};
use bson::{doc, Document};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::State;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

fn ffmpeg_path_from(settings_doc: &Document) -> String {
    settings_doc.get("ffmpeg_path").and_then(|v| v.as_str()).unwrap_or("ffmpeg").to_string()
}

/// Fetch `audio_url`, convert it with ffmpeg straight to `dest` (creating its parent folder if
/// needed), and clean up the temp source file. Shared by every download/save path below so the
/// wav/flac/copy conversion args only live in one place.
async fn fetch_and_convert(client: &reqwest::Client, ff: &str, audio_url: &str, out_ext: &str, dest: &Path) -> Result<(), String> {
    let res = client.get(audio_url).send().await.map_err(|err| format!("HTTP request failed: {}", err))?;
    let bytes = res.bytes().await.map_err(|err| format!("Failed to read audio bytes: {}", err))?;

    let temp_dir = std::env::temp_dir();
    let unique_id = uuid::Uuid::new_v4().to_string();
    let mp3_path = temp_dir.join(format!("temp_src_{}.mp3", unique_id));
    tokio::fs::write(&mp3_path, bytes).await.map_err(|err| format!("Failed to write temp MP3: {}", err))?;

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|err| format!("Failed to create destination folder: {}", err))?;
    }

    let dest_str = dest.to_str().ok_or_else(|| "Destination path is not valid UTF-8".to_string())?;
    let status = if out_ext == "wav" {
        tokio::process::Command::new(ff)
            .args(["-y", "-i", mp3_path.to_str().unwrap(), "-c:a", "pcm_s16le", "-ar", "44100", "-ac", "2", dest_str])
            .status().await
    } else if out_ext == "flac" {
        tokio::process::Command::new(ff)
            .args(["-y", "-i", mp3_path.to_str().unwrap(), "-c:a", "flac", "-ar", "44100", "-ac", "2", dest_str])
            .status().await
    } else {
        tokio::process::Command::new(ff)
            .args(["-y", "-i", mp3_path.to_str().unwrap(), "-c:a", "copy", dest_str])
            .status().await
    }.map_err(|err| format!("FFmpeg failed to start: {}", err))?;

    let _ = tokio::fs::remove_file(&mp3_path).await;

    if !status.success() {
        return Err("Audio conversion process failed".to_string());
    }
    Ok(())
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
pub async fn list_songs(state: State<'_, AppState>, pid: String) -> Res<Vec<Value>> {
    use futures_util::StreamExt;
    let mut cursor = state.db.collection::<Document>("songs")
        .find(doc! { "project_id": &pid }).await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }
    Ok(out)
}

#[tauri::command]
pub async fn get_song(state: State<'_, AppState>, sid: String) -> Res<Value> {
    let doc = state.db.collection::<Document>("songs")
        .find_one(doc! { "id": &sid }).await.map_err(e)?
        .ok_or_else(|| "not found".to_string())?;
    Ok(bson_to_value(doc))
}

/// Apply a (fused) genre/style string to one or more songs. Used by the Sound Studio to push a
/// mixed genre onto selected songs before regenerating their music.
#[tauri::command]
pub async fn update_song_styles(state: State<'_, AppState>, song_ids: Vec<String>, styles: String) -> Res<Value> {
    let coll = state.db.collection::<Document>("songs");
    let mut updated = 0;
    for sid in &song_ids {
        let r = coll.update_one(doc! { "id": sid }, doc! { "$set": { "styles": &styles } }).await.map_err(e)?;
        updated += r.modified_count;
    }
    Ok(serde_json::json!({ "ok": true, "updated": updated }))
}

/// Edit a single song's own title / styles / lyrics — the per-song fields the Music Studio's
/// expandable card sections and the active-song title field in its top bar write through.
/// Only the fields present in `patch` are touched.
#[tauri::command]
pub async fn update_song(state: State<'_, AppState>, sid: String, patch: Value) -> Res<Value> {
    let coll = state.db.collection::<Document>("songs");
    let mut set = Document::new();
    if let Some(v) = patch.get("title").and_then(|v| v.as_str()) { set.insert("title", v); }
    if let Some(v) = patch.get("styles").and_then(|v| v.as_str()) { set.insert("styles", v); }
    if let Some(v) = patch.get("lyrics").and_then(|v| v.as_str()) { set.insert("lyrics", v); }
    // Written by the Suno browser-automation flow (Browser.jsx captureSunoResult / genPipeline)
    // once it scrapes a finished clip's URL off the page — the API-driven engines set these same
    // fields themselves from jobs.rs's "music" job.
    if let Some(v) = patch.get("audio_url").and_then(|v| v.as_str()) { set.insert("audio_url", v); }
    if let Some(v) = patch.get("duration").and_then(|v| v.as_f64()) { set.insert("duration", v); }
    // Suno generates two clips per run; jobs.rs's API path already writes these directly, this
    // lets the webview automation path (genPipeline.js) set the same fields.
    if let Some(v) = patch.get("audio_url_primary").and_then(|v| v.as_str()) { set.insert("audio_url_primary", v); }
    if let Some(v) = patch.get("duration_primary").and_then(|v| v.as_f64()) { set.insert("duration_primary", v); }
    if let Some(v) = patch.get("audio_url_alt").and_then(|v| v.as_str()) { set.insert("audio_url_alt", v); }
    if let Some(v) = patch.get("duration_alt").and_then(|v| v.as_f64()) { set.insert("duration_alt", v); }
    // The destination channel for this song (auto-suggested by language match, overridable —
    // see genPipeline.js), and the on-disk path(s) save_generated_asset wrote to.
    if let Some(v) = patch.get("channel_id") {
        if v.is_null() { set.insert("channel_id", bson::Bson::Null); }
        else if let Some(s) = v.as_str() { set.insert("channel_id", s); }
    }
    if let Some(v) = patch.get("local_audio_path").and_then(|v| v.as_str()) { set.insert("local_audio_path", v); }
    if let Some(v) = patch.get("local_audio_path_alt").and_then(|v| v.as_str()) { set.insert("local_audio_path_alt", v); }
    if set.is_empty() {
        return Err("No valid fields to update (expected title/styles/lyrics/audio_url/duration/audio_url_primary/duration_primary/audio_url_alt/duration_alt/channel_id/local_audio_path/local_audio_path_alt)".to_string());
    }
    coll.update_one(doc! { "id": &sid }, doc! { "$set": set }).await.map_err(e)?;
    let updated = coll.find_one(doc! { "id": &sid }).await.map_err(e)?
        .ok_or_else(|| "Song not found after update".to_string())?;
    Ok(bson_to_value(updated))
}

#[tauri::command]
pub async fn delete_song(state: State<'_, AppState>, sid: String) -> Res<Value> {
    state.db.collection::<Document>("sections")
        .delete_many(doc! { "song_id": &sid }).await.map_err(e)?;
    state.db.collection::<Document>("songs")
        .delete_one(doc! { "id": &sid }).await.map_err(e)?;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn generate_music(
    state: State<'_, AppState>,
    state_arc: State<'_, Arc<AppState>>,
    sid: String,
) -> Res<Value> {
    let song = state.db.collection::<Document>("songs")
        .find_one(doc! { "id": &sid }).await.map_err(e)?
        .ok_or_else(|| "song missing".to_string())?;
    // Every music engine needs lyrics to work from; without this check a blank-lyrics song
    // still gets enqueued and only fails deep inside the engine-specific job code with a
    // generic "generation failed" error that gives no hint the actual problem was upstream —
    // an empty lyrics field never written back by AI Composer/import.
    let lyrics = song.get_str("lyrics").unwrap_or("");
    if lyrics.trim().is_empty() {
        let title = song.get_str("title").unwrap_or("This song");
        return Err(format!("\"{}\" has no lyrics yet — add lyrics (expand the song card) before generating music.", title));
    }
    let job = enqueue("music", &sid, &state_arc).await.map_err(e)?;
    Ok(serde_json::to_value(job).map_err(e)?)
}

#[tauri::command]
pub async fn analyze_song(
    state: State<'_, AppState>,
    state_arc: State<'_, Arc<AppState>>,
    sid: String,
) -> Res<Value> {
    state.db.collection::<Document>("songs")
        .find_one(doc! { "id": &sid }).await.map_err(e)?
        .ok_or_else(|| "song missing".to_string())?;
    let job = enqueue("analysis", &sid, &state_arc).await.map_err(e)?;
    Ok(serde_json::to_value(job).map_err(e)?)
}

#[tauri::command]
pub async fn compose_video(
    state: State<'_, AppState>,
    state_arc: State<'_, Arc<AppState>>,
    sid: String,
) -> Res<Value> {
    state.db.collection::<Document>("songs")
        .find_one(doc! { "id": &sid }).await.map_err(e)?
        .ok_or_else(|| "song missing".to_string())?;
    let job = enqueue("video", &sid, &state_arc).await.map_err(e)?;
    Ok(serde_json::to_value(job).map_err(e)?)
}

#[tauri::command]
pub async fn generate_overlay(
    state: State<'_, AppState>,
    state_arc: State<'_, Arc<AppState>>,
    sid: String,
) -> Res<Value> {
    state.db.collection::<Document>("songs")
        .find_one(doc! { "id": &sid }).await.map_err(e)?
        .ok_or_else(|| "song missing".to_string())?;
    let job = enqueue("overlay", &sid, &state_arc).await.map_err(e)?;
    Ok(serde_json::to_value(job).map_err(e)?)
}

/// Queue a companion-overlay job for every song (of the project, if given) that has audio
/// but no generated overlay yet. `force` re-renders existing overlays too.
#[tauri::command]
pub async fn generate_overlays_bulk(
    state: State<'_, AppState>,
    state_arc: State<'_, Arc<AppState>>,
    pid: Option<String>,
    force: Option<bool>,
) -> Res<Value> {
    let mut filter = doc! { "audio_url": { "$type": "string", "$ne": "" } };
    if let Some(p) = pid.filter(|s| !s.is_empty()) {
        filter.insert("project_id", p);
    }
    if !force.unwrap_or(false) {
        filter.insert("overlay_local_path", doc! { "$exists": false });
    }
    let mut cursor = state.db.collection::<Document>("songs").find(filter).await.map_err(e)?;
    let mut queued = 0u32;
    use futures_util::StreamExt;
    while let Some(Ok(d)) = cursor.next().await {
        if let Ok(sid) = d.get_str("id") {
            if enqueue("overlay", sid, &state_arc).await.is_ok() { queued += 1; }
        }
    }
    Ok(serde_json::json!({ "queued": queued }))
}

#[derive(serde::Deserialize)]
pub struct SongDownloadInfo {
    pub audio_url: String,
    pub title: String,
    pub format: String,
}

#[tauri::command]
pub async fn download_and_convert_audio(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    audio_url: String,
    format: String,
    filename: String,
) -> Result<String, String> {
    use tauri_plugin_dialog::DialogExt;
    let out_ext = format.to_lowercase();
    let file_path_opt = app_handle.dialog()
        .file()
        .set_file_name(&filename)
        .add_filter("Audio File", &[&out_ext])
        .blocking_save_file();
    let dest = match file_path_opt {
        Some(p) => p.into_path().map_err(|err| format!("{:?}", err))?,
        None => return Err("Save dialog cancelled".to_string()),
    };

    let settings_doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(|err| format!("DB lookup failed: {}", err))?
        .unwrap_or_default();
    let ff = ffmpeg_path_from(&settings_doc);
    let client = reqwest::Client::new();
    fetch_and_convert(&client, &ff, &audio_url, &out_ext, &dest).await?;
    Ok(dest.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn download_all_songs(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    songs: Vec<SongDownloadInfo>,
) -> Result<u32, String> {
    let dir_path = crate::helpers::pick_folder_blocking(&app_handle)
        .map_err(|err| if err == "Cancelled" { "Folder picker cancelled".to_string() } else { err })?;

    let settings_doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(|err| format!("DB lookup failed: {}", err))?
        .unwrap_or_default();
    let ff = ffmpeg_path_from(&settings_doc);
    let client = reqwest::Client::new();
    let mut count = 0;

    for s in songs {
        if s.audio_url.is_empty() { continue; }
        let out_ext = s.format.to_lowercase();
        let safe_title = s.title.chars()
            .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
            .collect::<String>();
        let dest_path = dir_path.join(format!("{}.{}", safe_title.trim(), out_ext));
        if fetch_and_convert(&client, &ff, &s.audio_url, &out_ext, &dest_path).await.is_ok() {
            count += 1;
        }
    }

    Ok(count)
}

/// Dialog-free save, for the AI generation pipeline: fetches `audio_url`, converts it, and
/// writes it straight to `<project.project_folder>/auto/<channel-slug>/<filename>` — no native
/// save dialog, so a bulk/unattended run can download every clip without a human at the
/// keyboard. `channel_id` resolves to the channel's own name slug (falls back to "unsorted"
/// when absent) so a project's downloads land pre-sorted by destination channel.
#[tauri::command]
pub async fn save_generated_asset(
    state: State<'_, AppState>,
    audio_url: String,
    project_id: String,
    channel_id: Option<String>,
    filename: String,
    format: String,
) -> Result<String, String> {
    let project = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &project_id }).await.map_err(|err| format!("DB lookup failed: {}", err))?
        .ok_or_else(|| "project missing".to_string())?;
    let project_folder = project.get_str("project_folder")
        .map_err(|_| "This project has no project_folder set".to_string())?;

    let channel_slug = match channel_id.filter(|s| !s.is_empty()) {
        Some(cid) => state.db.collection::<Document>("channels")
            .find_one(doc! { "id": &cid }).await.map_err(|err| format!("DB lookup failed: {}", err))?
            .and_then(|c| c.get_str("name").ok().map(slugify))
            .unwrap_or_else(|| "unsorted".to_string()),
        None => "unsorted".to_string(),
    };

    let safe_filename: String = filename.chars()
        .map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' || c == ' ' { c } else { '_' })
        .collect();
    let dest = PathBuf::from(project_folder).join("auto").join(&channel_slug).join(safe_filename.trim());

    let settings_doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(|err| format!("DB lookup failed: {}", err))?
        .unwrap_or_default();
    let ff = ffmpeg_path_from(&settings_doc);
    let client = reqwest::Client::new();
    fetch_and_convert(&client, &ff, &audio_url, &format.to_lowercase(), &dest).await?;
    Ok(dest.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn select_song_variant(
    state: State<'_, AppState>,
    sid: String,
    variant: i32,
) -> Result<String, String> {
    let song = state.db.collection::<Document>("songs")
        .find_one(doc! { "id": &sid }).await.map_err(|err| format!("DB lookup failed: {}", err))?
        .ok_or_else(|| "song missing".to_string())?;

    let audio_url_primary = song.get("audio_url_primary").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let duration_primary = song.get("duration_primary").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let audio_url_alt = song.get("audio_url_alt").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let duration_alt = song.get("duration_alt").and_then(|v| v.as_f64()).unwrap_or(0.0);

    let (active_url, active_dur) = if variant == 2 && !audio_url_alt.is_empty() {
        (audio_url_alt, duration_alt)
    } else {
        (audio_url_primary, duration_primary)
    };

    if active_url.is_empty() {
        return Err("Selected variant has no audio URL".to_string());
    }

    state.db.collection::<Document>("songs").update_one(
        doc! { "id": &sid },
        doc! { "$set": {
            "audio_url": &active_url,
            "duration": active_dur,
        } }
    ).await.map_err(|err| format!("DB update failed: {}", err))?;

    Ok(active_url)
}


