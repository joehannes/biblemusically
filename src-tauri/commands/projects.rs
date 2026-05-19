use crate::{
    models::{now_iso, Project, ProjectCreate},
    state::AppState,
};
use bson::{doc, Document};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};
use uuid::Uuid;

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

fn copy_media_if_local(url: &str, media_dir: &Path) -> Option<String> {
    if url.is_empty() { return None; }
    if url.starts_with("http://") || url.starts_with("https://") { return None; }
    let source_path = if url.starts_with("file://") {
        PathBuf::from(url.trim_start_matches("file://"))
    } else {
        PathBuf::from(url)
    };
    if !source_path.exists() || !source_path.is_file() {
        return None;
    }
    if let Some(filename) = source_path.file_name().and_then(|n| n.to_str()) {
        let mut dest_path = media_dir.join(filename);
        let mut suffix = 1;
        while dest_path.exists() {
            let name = source_path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
            let ext = source_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let file_name = if ext.is_empty() {
                format!("{}-{}", name, suffix)
            } else {
                format!("{}-{}.{}", name, suffix, ext)
            };
            dest_path = media_dir.join(file_name);
            suffix += 1;
        }
        if let Err(_) = std::fs::copy(&source_path, &dest_path) {
            return None;
        }
        return Some(dest_path.to_string_lossy().to_string());
    }
    None
}

fn normalize_relative_media_path(url: &str, source_dir: &Path) -> Option<String> {
    if url.is_empty() { return None; }
    if url.starts_with("http://") || url.starts_with("https://") { return None; }
    let path = PathBuf::from(url);
    let resolved = if path.is_absolute() {
        path
    } else {
        source_dir.join(path)
    };
    if resolved.exists() && resolved.is_file() {
        Some(resolved.to_string_lossy().to_string())
    } else {
        None
    }
}

#[tauri::command]
pub async fn list_projects(state: State<'_, AppState>) -> Res<Vec<Value>> {
    use futures_util::StreamExt;
    let mut cursor = state.db.collection::<Document>("projects")
        .find(doc! {}).sort(doc! { "created_at": -1 })
        .await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }
    Ok(out)
}

#[tauri::command]
pub async fn create_project(state: State<'_, AppState>, body: ProjectCreate) -> Res<Value> {
    let p = Project {
        id: Uuid::new_v4().to_string(),
        name: body.name,
        topic: body.topic,
        schedule: body.schedule,
        multi_language: true,
        multi_style: true,
        languages: vec![],
        styles: vec![],
        created_at: now_iso(),
    };
    let bson = bson::to_document(&p).map_err(e)?;
    state.db.collection::<Document>("projects").insert_one(bson).await.map_err(e)?;
    Ok(serde_json::to_value(&p).map_err(e)?)
}

#[tauri::command]
pub async fn get_project(state: State<'_, AppState>, pid: String) -> Res<Value> {
    let doc = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &pid }).await.map_err(e)?
        .ok_or_else(|| "not found".to_string())?;
    Ok(bson_to_value(doc))
}

#[tauri::command]
pub async fn update_project(state: State<'_, AppState>, pid: String, body: Value) -> Res<Value> {
    let mut body = body;
    if let Some(obj) = body.as_object_mut() {
        obj.remove("id");
        obj.remove("_id");
    }
    let bson = bson::to_bson(&body).map_err(e)?;
    state.db.collection::<Document>("projects")
        .update_one(doc! { "id": &pid }, doc! { "$set": bson })
        .await.map_err(e)?;
    let doc = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &pid }).await.map_err(e)?
        .ok_or_else(|| "not found".to_string())?;
    Ok(bson_to_value(doc))
}

#[tauri::command]
pub async fn delete_project(state: State<'_, AppState>, pid: String) -> Res<Value> {
    state.db.collection::<Document>("projects")
        .delete_one(doc! { "id": &pid }).await.map_err(e)?;
    // Cascade delete songs + sections
    use futures_util::StreamExt;
    let mut cursor = state.db.collection::<Document>("songs")
        .find(doc! { "project_id": &pid }).await.map_err(e)?;
    let mut song_ids = Vec::new();
    while let Some(Ok(d)) = cursor.next().await {
        if let Some(id) = d.get_str("id").ok() { song_ids.push(id.to_string()); }
    }
    for sid in &song_ids {
        state.db.collection::<Document>("sections")
            .delete_many(doc! { "song_id": sid }).await.map_err(e)?;
    }
    state.db.collection::<Document>("songs")
        .delete_many(doc! { "project_id": &pid }).await.map_err(e)?;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn export_project(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    pid: String,
) -> Res<Value> {
    let project = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &pid }).await.map_err(e)?
        .ok_or_else(|| "project missing".to_string())?;
    let project_val = bson_to_value(project);

    use futures_util::StreamExt;
    let mut song_cursor = state.db.collection::<Document>("songs")
        .find(doc! { "project_id": &pid }).await.map_err(e)?;
    let mut songs = Vec::new();
    while let Some(Ok(doc)) = song_cursor.next().await { songs.push(bson_to_value(doc)); }

    let mut sections = Vec::new();
    for song in &songs {
        if let Some(sid) = song["id"].as_str() {
            let mut section_cursor = state.db.collection::<Document>("sections")
                .find(doc! { "song_id": sid }).await.map_err(e)?;
            while let Some(Ok(doc)) = section_cursor.next().await { sections.push(bson_to_value(doc)); }
        }
    }

    use tauri_plugin_dialog::DialogExt;
    let dir_path_opt = app_handle.dialog().file().blocking_pick_folder();
    let dir_path = dir_path_opt.ok_or_else(|| "Export cancelled".to_string())?
        .into_path().map_err(|err| e(err))?;

    let media_dir = dir_path.join("media");
    std::fs::create_dir_all(&media_dir).map_err(e)?;

    let mut project_out = project_val.clone();
    let mut songs_out = songs.clone();
    let mut sections_out = sections.clone();

    if let Some(url) = project_out["image_url"].as_str() {
        if let Some(copied) = copy_media_if_local(url, &media_dir) {
            project_out["image_url"] = serde_json::Value::String(copied);
        }
    }

    for song in &mut songs_out {
        if let Some(url) = song["audio_url"].as_str() {
            if let Some(copied) = copy_media_if_local(url, &media_dir) {
                song["audio_url"] = serde_json::Value::String(copied);
            }
        }
        if let Some(url) = song["video_url"].as_str() {
            if let Some(copied) = copy_media_if_local(url, &media_dir) {
                song["video_url"] = serde_json::Value::String(copied);
            }
        }
    }

    for section in &mut sections_out {
        if let Some(url) = section["image_url"].as_str() {
            if let Some(copied) = copy_media_if_local(url, &media_dir) {
                section["image_url"] = serde_json::Value::String(copied);
            }
        }
        if let Some(arr) = section["image_variants"].as_array() {
            let new_arr = arr.iter().map(|v| {
                if let Some(url) = v.as_str() {
                    if let Some(copied) = copy_media_if_local(url, &media_dir) {
                        serde_json::Value::String(copied)
                    } else {
                        v.clone()
                    }
                } else { v.clone() }
            }).collect();
            section["image_variants"] = serde_json::Value::Array(new_arr);
        }
    }

    let export_file = dir_path.join("project.json");
    let export_payload = serde_json::json!({
        "project": project_out,
        "songs": songs_out,
        "sections": sections_out,
        "exported_at": now_iso(),
    });
    let file = std::fs::File::create(&export_file).map_err(e)?;
    serde_json::to_writer_pretty(file, &export_payload).map_err(e)?;

    Ok(serde_json::json!({ "ok": true, "export_folder": dir_path.to_string_lossy() }))
}

#[tauri::command]
pub async fn import_project(
    state: State<'_, AppState>,
    payload: Value,
    source_dir: Option<String>,
) -> Res<Value> {
    let project_val = payload["project"].as_object().ok_or_else(|| "project data missing".to_string())?;
    let songs_val = payload["songs"].as_array().ok_or_else(|| "songs data missing".to_string())?;
    let sections_val = payload["sections"].as_array().cloned().unwrap_or_default();

    let new_project_id = Uuid::new_v4().to_string();
    let mut project_obj = project_val.clone();
    project_obj.insert("id".to_string(), serde_json::Value::String(new_project_id.clone()));
    project_obj.remove("_id");

    let mut project_doc = bson::to_document(&project_obj).map_err(e)?;
    state.db.collection::<Document>("projects").insert_one(project_doc, None).await.map_err(e)?;

    let source_dir_path = source_dir.map(PathBuf::from);
    let app_data = tauri::api::path::app_data_dir().ok_or_else(|| "Unable to locate app data directory".to_string())?;
    let import_media_dir = app_data.join("project_imports").join(&new_project_id).join("media");
    std::fs::create_dir_all(&import_media_dir).map_err(e)?;

    let mut song_map = std::collections::HashMap::new();
    for song_val in songs_val {
        if let Some(mut song_obj) = song_val.as_object().cloned() {
            let old_id = song_obj.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let new_song_id = Uuid::new_v4().to_string();
            song_map.insert(old_id.clone(), new_song_id.clone());
            song_obj.insert("id".to_string(), serde_json::Value::String(new_song_id.clone()));
            song_obj.insert("project_id".to_string(), serde_json::Value::String(new_project_id.clone()));
            song_obj.remove("_id");

            if let Some(url) = song_obj.get("audio_url").and_then(|v| v.as_str()) {
                if let Some(source) = source_dir_path.as_deref().and_then(|base| normalize_relative_media_path(url, base)) {
                    if let Some(copied) = copy_media_if_local(&source, &import_media_dir) {
                        song_obj.insert("audio_url".to_string(), serde_json::Value::String(copied));
                    }
                }
            }
            if let Some(url) = song_obj.get("video_url").and_then(|v| v.as_str()) {
                if let Some(source) = source_dir_path.as_deref().and_then(|base| normalize_relative_media_path(url, base)) {
                    if let Some(copied) = copy_media_if_local(&source, &import_media_dir) {
                        song_obj.insert("video_url".to_string(), serde_json::Value::String(copied));
                    }
                }
            }

            let bson = bson::to_document(&song_obj).map_err(e)?;
            state.db.collection::<Document>("songs").insert_one(bson, None).await.map_err(e)?;
        }
    }

    for section_val in sections_val {
        if let Some(mut section_obj) = section_val.as_object().cloned() {
            if let Some(old_song_id) = section_obj.get("song_id").and_then(|v| v.as_str()) {
                if let Some(new_song_id) = song_map.get(old_song_id) {
                    section_obj.insert("song_id".to_string(), serde_json::Value::String(new_song_id.clone()));
                }
            }
            section_obj.remove("_id");

            if let Some(url) = section_obj.get("image_url").and_then(|v| v.as_str()) {
                if let Some(source) = source_dir_path.as_deref().and_then(|base| normalize_relative_media_path(url, base)) {
                    if let Some(copied) = copy_media_if_local(&source, &import_media_dir) {
                        section_obj.insert("image_url".to_string(), serde_json::Value::String(copied));
                    }
                }
            }
            if let Some(arr) = section_obj.get("image_variants").and_then(|v| v.as_array()) {
                let new_arr = arr.iter().map(|v| {
                    if let Some(url) = v.as_str() {
                        if let Some(source) = source_dir_path.as_deref().and_then(|base| normalize_relative_media_path(url, base)) {
                            if let Some(copied) = copy_media_if_local(&source, &import_media_dir) {
                                return serde_json::Value::String(copied);
                            }
                        }
                    }
                    v.clone()
                }).collect();
                section_obj.insert("image_variants".to_string(), serde_json::Value::Array(new_arr));
            }

            let bson = bson::to_document(&section_obj).map_err(e)?;
            state.db.collection::<Document>("sections").insert_one(bson, None).await.map_err(e)?;
        }
    }

    Ok(serde_json::json!({ "ok": true, "project_id": new_project_id }))
}

#[tauri::command]
pub async fn import_lyrics(state: State<'_, AppState>, pid: String, body: Value) -> Res<Value> {
    let project = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &pid }).await.map_err(e)?
        .ok_or_else(|| "project missing".to_string())?;

    let proj_val = bson_to_value(project);
    let mut languages: std::collections::HashSet<String> = proj_val["languages"]
        .as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let mut styles: std::collections::HashSet<String> = proj_val["styles"]
        .as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();

    let items = body["items"].as_array().ok_or("expected items: list")?.clone();
    let mut created = Vec::new();
    for it in &items {
        use crate::models::Song;
        let song = Song {
            id: Uuid::new_v4().to_string(),
            project_id: pid.clone(),
            title: it["title"].as_str().unwrap_or("Untitled").to_string(),
            language: it["language"].as_str().unwrap_or("English").to_string(),
            styles: it["styles"].as_str().unwrap_or("").to_string(),
            lyrics: it["lyrics"].as_str().unwrap_or("").to_string(),
            annotations: it["annotations"].as_str().unwrap_or("").to_string(),
            image_styles: it["image_styles"].as_str().unwrap_or("").to_string(),
            audio_url: None,
            video_url: None,
            duration: 0.0,
            status: "draft".into(),
            created_at: now_iso(),
        };
        languages.insert(song.language.clone());
        styles.insert(song.styles.clone());
        let bson = bson::to_document(&song).map_err(e)?;
        state.db.collection::<Document>("songs").insert_one(bson).await.map_err(e)?;
        created.push(serde_json::to_value(&song).map_err(e)?);
    }

    let mut langs_sorted: Vec<String> = languages.into_iter().collect();
    langs_sorted.sort();
    let mut styles_sorted: Vec<String> = styles.into_iter().collect();
    styles_sorted.sort();
    state.db.collection::<Document>("projects")
        .update_one(doc! { "id": &pid },
            doc! { "$set": { "languages": langs_sorted, "styles": styles_sorted } })
        .await.map_err(e)?;
    Ok(serde_json::json!({ "created": created.len(), "songs": created }))
}
