use crate::{jobs::enqueue, state::AppState};
use bson::{doc, Document};
use serde_json::Value;
use std::sync::Arc;
use tauri::State;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

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
    state.db.collection::<Document>("songs")
        .find_one(doc! { "id": &sid }).await.map_err(e)?
        .ok_or_else(|| "song missing".to_string())?;
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
    let client = reqwest::Client::new();
    let res = client.get(&audio_url).send().await.map_err(|err| format!("HTTP request failed: {}", err))?;
    let bytes = res.bytes().await.map_err(|err| format!("Failed to read audio bytes: {}", err))?;
    
    let temp_dir = std::env::temp_dir();
    let unique_id = uuid::Uuid::new_v4().to_string();
    let mp3_path = temp_dir.join(format!("temp_src_{}.mp3", unique_id));
    tokio::fs::write(&mp3_path, bytes).await.map_err(|err| format!("Failed to write temp MP3: {}", err))?;

    let settings_doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(|err| format!("DB lookup failed: {}", err))?
        .unwrap_or_default();
    let ff = settings_doc.get("ffmpeg_path").and_then(|v| v.as_str()).unwrap_or("ffmpeg").to_string();

    let out_ext = format.to_lowercase();
    let converted_path = temp_dir.join(format!("converted_{}.{}", unique_id, out_ext));

    let status = if out_ext == "wav" {
        tokio::process::Command::new(&ff)
            .args(["-y", "-i", mp3_path.to_str().unwrap(),
                   "-c:a", "pcm_s16le", "-ar", "44100", "-ac", "2",
                   converted_path.to_str().unwrap()])
            .status().await.map_err(|err| format!("FFmpeg failed to start: {}", err))?
    } else if out_ext == "flac" {
        tokio::process::Command::new(&ff)
            .args(["-y", "-i", mp3_path.to_str().unwrap(),
                   "-c:a", "flac", "-ar", "44100", "-ac", "2",
                   converted_path.to_str().unwrap()])
            .status().await.map_err(|err| format!("FFmpeg failed to start: {}", err))?
    } else {
        tokio::process::Command::new(&ff)
            .args(["-y", "-i", mp3_path.to_str().unwrap(),
                   "-c:a", "copy",
                   converted_path.to_str().unwrap()])
            .status().await.map_err(|err| format!("FFmpeg failed to start: {}", err))?
    };

    if !status.success() {
        let _ = tokio::fs::remove_file(&mp3_path).await;
        return Err("Audio conversion process failed".to_string());
    }

    use tauri_plugin_dialog::DialogExt;
    
    let file_path_opt = app_handle.dialog()
        .file()
        .set_file_name(&filename)
        .add_filter("Audio File", &[&out_ext])
        .blocking_save_file();

    if let Some(file_path) = file_path_opt {
        let dest = file_path.into_path().map_err(|err| format!("{:?}", err))?;
        tokio::fs::copy(&converted_path, &dest).await.map_err(|err| format!("Failed to copy file to destination: {}", err))?;
        
        let _ = tokio::fs::remove_file(&mp3_path).await;
        let _ = tokio::fs::remove_file(&converted_path).await;
        
        Ok(dest.to_string_lossy().to_string())
    } else {
        let _ = tokio::fs::remove_file(&mp3_path).await;
        let _ = tokio::fs::remove_file(&converted_path).await;
        Err("Save dialog cancelled".to_string())
    }
}

#[tauri::command]
pub async fn download_all_songs(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    songs: Vec<SongDownloadInfo>,
) -> Result<u32, String> {
    use tauri_plugin_dialog::DialogExt;
    
    let dir_path_opt = app_handle.dialog()
        .file()
        .blocking_pick_folder();
        
    let dir_path = match dir_path_opt {
        Some(path) => path.into_path().map_err(|err| format!("{:?}", err))?,
        None => return Err("Folder picker cancelled".to_string()),
    };

    let settings_doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(|err| format!("DB lookup failed: {}", err))?
        .unwrap_or_default();
    let ff = settings_doc.get("ffmpeg_path").and_then(|v| v.as_str()).unwrap_or("ffmpeg").to_string();

    let client = reqwest::Client::new();
    let mut count = 0;

    for s in songs {
        if s.audio_url.is_empty() { continue; }
        
        if let Ok(res) = client.get(&s.audio_url).send().await {
            if let Ok(bytes) = res.bytes().await {
                let temp_dir = std::env::temp_dir();
                let unique_id = uuid::Uuid::new_v4().to_string();
                let mp3_path = temp_dir.join(format!("temp_src_{}.mp3", unique_id));
                if tokio::fs::write(&mp3_path, bytes).await.is_err() { continue; }
                
                let out_ext = s.format.to_lowercase();
                let safe_title = s.title.chars()
                    .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
                    .collect::<String>();
                let dest_filename = format!("{}.{}", safe_title.trim(), out_ext);
                let dest_path = dir_path.join(dest_filename);
                
                let status = if out_ext == "wav" {
                    tokio::process::Command::new(&ff)
                        .args(["-y", "-i", mp3_path.to_str().unwrap(),
                               "-c:a", "pcm_s16le", "-ar", "44100", "-ac", "2",
                               dest_path.to_str().unwrap()])
                        .status().await
                } else if out_ext == "flac" {
                    tokio::process::Command::new(&ff)
                        .args(["-y", "-i", mp3_path.to_str().unwrap(),
                               "-c:a", "flac", "-ar", "44100", "-ac", "2",
                               dest_path.to_str().unwrap()])
                        .status().await
                } else {
                    tokio::process::Command::new(&ff)
                        .args(["-y", "-i", mp3_path.to_str().unwrap(),
                               "-c:a", "copy",
                               dest_path.to_str().unwrap()])
                        .status().await
                };
                
                let _ = tokio::fs::remove_file(&mp3_path).await;
                
                if let Ok(st) = status {
                    if st.success() {
                        count += 1;
                    }
                }
            }
        }
    }
    
    Ok(count)
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


