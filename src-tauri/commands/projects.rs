use crate::{
    models::{now_iso, Project},
    state::AppState,
};
use bson::{doc, Document};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};
use uuid::Uuid;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use url::Url;
use warp::Filter;

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

/// Turn a name into a filesystem-safe slug ("My Bible Project!" -> "my-bible-project"). Also
/// used by commands::songs::save_generated_asset to name a channel's downloads subfolder.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut last_was_dash = false;
    for ch in name.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_was_dash = false;
        } else if !last_was_dash && !out.is_empty() {
            out.push('-');
            last_was_dash = true;
        }
    }
    while out.ends_with('-') { out.pop(); }
    if out.is_empty() { "project".to_string() } else { out }
}

#[tauri::command]
pub async fn create_project(state: State<'_, AppState>, body: Value) -> Res<Value> {
    let name = body["name"].as_str().unwrap_or("").trim().to_string();
    if name.is_empty() {
        return Err("Project name is required".into());
    }
    let topic = body["topic"].as_str().unwrap_or("").to_string();
    let schedule = if body["schedule"].is_null() {
        None
    } else {
        body["schedule"].as_str().map(|s| s.to_string())
    };
    let multi_language = body["multi_language"].as_bool().unwrap_or(true);
    let multi_style = body["multi_style"].as_bool().unwrap_or(true);
    let languages = body["languages"].as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_else(Vec::new);
    let styles = body["styles"].as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_else(Vec::new);

    let id = Uuid::new_v4().to_string();

    // A distinct ~/Documents folder per project, named after it, created right away so JSON
    // exports and other project detail files have somewhere sensible to land automatically
    // (see save_project_file) instead of falling back to the browser's Downloads folder.
    // The id-prefix suffix avoids collisions between same-named projects without needing a
    // "does this folder already exist" check-then-create race.
    let project_folder = crate::paths::document_dir().map(|docs| {
        let folder = docs.join("AI Music Video Studio").join(format!("{}-{}", slugify(&name), &id[..8]));
        if let Err(err) = std::fs::create_dir_all(&folder) {
            eprintln!("Failed to create project folder {}: {}", folder.display(), err);
        }
        // The folder is a git repository from the very first moment, because it is where this
        // project's JSON data lives from now on (store.rs routes songs/sections/characters/uploads
        // into <folder>/data/). Initializing it here — rather than lazily on the first "Save
        // version" — means the project's whole life is in its history, not just what happened after
        // someone remembered to press a button.
        if let Err(err) = crate::project_sync::ensure_repo(&folder) {
            eprintln!("Failed to initialize git for {}: {}", folder.display(), err);
        }
        // Track generated media with LFS when it's installed, so pushing the project to a remote
        // that offers large-file storage works without re-writing history later.
        let _ = crate::project_sync::ensure_lfs(&folder);
        folder.to_string_lossy().to_string()
    });

    let p = Project {
        id,
        name,
        topic,
        schedule,
        // Unset means "lyrics": a new project never starts publishing by itself.
        schedule_autonomy: None,
        multi_language,
        multi_style,
        languages,
        styles,
        local_path: None,
        project_folder,
        current_branch: None,
        git_status: None,
        created_at: now_iso(),
    };
    // Christian projects (default) get Bible Sources + the Jewish/Messianic musical layers; the
    // flag isn't on the typed Project struct, so it rides along on the stored doc directly.
    let is_christian = body["is_christian"].as_bool().unwrap_or(true);
    let mut bson = bson::to_document(&p).map_err(e)?;
    bson.insert("is_christian", is_christian);
    state.db.collection::<Document>("projects").insert_one(bson).await.map_err(e)?;
    let mut out = serde_json::to_value(&p).map_err(e)?;
    out["is_christian"] = Value::Bool(is_christian);
    Ok(out)
}

/// Write a file straight into the project's own folder (created at project creation, under
/// ~/Documents) — no save dialog, just a direct write, the same "no fuss" behaviour as a
/// browser download but landing next to the rest of the project's own files instead of
/// wherever the OS Downloads folder happens to be, under whatever name the caller gives it.
/// Falls back to creating the folder on the spot if an older project predates this feature and
/// has none yet.
#[tauri::command]
pub async fn save_project_file(
    state: State<'_, AppState>,
    pid: String,
    filename: String,
    content: String,
) -> Res<Value> {
    let doc = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &pid }).await.map_err(e)?
        .ok_or_else(|| "project missing".to_string())?;
    let project = bson_to_value(doc);

    let folder = match project["project_folder"].as_str() {
        Some(f) if !f.trim().is_empty() => PathBuf::from(f),
        _ => {
            let name = project["name"].as_str().unwrap_or("project");
            let docs = crate::paths::document_dir().ok_or("Could not locate the Documents folder")?;
            let folder = docs.join("AI Music Video Studio").join(format!("{}-{}", slugify(name), &pid[..8.min(pid.len())]));
            state.db.collection::<Document>("projects")
                .update_one(doc! { "id": &pid }, doc! { "$set": { "project_folder": folder.to_string_lossy().to_string() } })
                .await.map_err(e)?;
            folder
        }
    };
    std::fs::create_dir_all(&folder).map_err(e)?;

    // Guard against a filename that's actually a path (e.g. "../../etc/passwd") escaping the
    // project folder — keep only the final path segment.
    let safe_name = Path::new(&filename)
        .file_name()
        .ok_or("Invalid filename")?
        .to_string_lossy()
        .to_string();
    let path = folder.join(&safe_name);
    std::fs::write(&path, content).map_err(e)?;
    Ok(serde_json::json!({ "ok": true, "path": path.to_string_lossy(), "folder": folder.to_string_lossy() }))
}

#[tauri::command]
pub async fn get_project(state: State<'_, AppState>, pid: String) -> Res<Value> {
    let doc = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &pid }).await.map_err(e)?
        .ok_or_else(|| "not found".to_string())?;
    Ok(bson_to_value(doc))
}

/// One-line English rendering of a `schedule_config`, e.g.
/// "Daily at 06:00 — Psalms, English (Cinematic), next chapter 12".
///
/// The project used to carry two schedules: this structured config (which drives the actual
/// automation) and a free-text `schedule` string shown on the dashboard card, which nothing read
/// and which drifted until the card claimed "weekly Sunday 9am" while the scheduler ran daily at
/// 06:00 (TODOS.md #17). Rather than keep two sources of truth, `schedule` is now *derived* here on
/// every save, so the card can only ever say what the automation will actually do.
pub fn describe_schedule(cfg: &Value) -> String {
    if !cfg.is_object() {
        return String::new();
    }
    if cfg["enabled"].as_bool() != Some(true) {
        return "Automation off".to_string();
    }
    let time = cfg["time"].as_str().filter(|s| !s.is_empty()).unwrap_or("06:00");
    let cadence = match cfg["frequency"].as_str().unwrap_or("daily") {
        "weekly" => {
            const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
            let day = cfg["day_of_week"].as_i64().unwrap_or(0).rem_euclid(7) as usize;
            format!("Weekly on {} at {time}", DAYS[day])
        }
        _ => format!("Daily at {time}"),
    };

    let mut parts = vec![cadence];
    if let Some(book) = cfg["book"].as_str().filter(|s| !s.is_empty()) {
        let mut chars = book.chars();
        let pretty = match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => book.to_string(),
        };
        parts.push(pretty);
    }
    let list = |key: &str| -> Option<String> {
        let items: Vec<&str> = cfg[key].as_array()?.iter().filter_map(|v| v.as_str()).collect();
        if items.is_empty() { None } else { Some(items.join(", ")) }
    };
    if let Some(langs) = list("languages") {
        parts.push(langs);
    }
    if let Some(styles) = list("styles") {
        parts.push(format!("({styles})"));
    }
    if let Some(next) = cfg["next_chapter"].as_i64() {
        parts.push(format!("next chapter {next}"));
    }
    parts.join(" — ")
}

#[tauri::command]
pub async fn update_project(state: State<'_, AppState>, pid: String, body: Value) -> Res<Value> {
    let mut body = body;
    if let Some(obj) = body.as_object_mut() {
        obj.remove("id");
        obj.remove("_id");
        // Keep the displayed schedule text in lockstep with the automation that actually runs.
        if let Some(cfg) = obj.get("schedule_config").cloned() {
            obj.insert("schedule".to_string(), Value::String(describe_schedule(&cfg)));
        }
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
    crate::commands::subscription::require(&state, "export").await?;
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

    let dir_path = crate::helpers::pick_folder_blocking(&app_handle)
        .map_err(|err| if err == "Cancelled" { "Export cancelled".to_string() } else { err })?;

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
    state.db.collection::<Document>("projects").insert_one(project_doc).await.map_err(e)?;

    let source_dir_path = source_dir.map(PathBuf::from);
    // Get app data directory using standard approach
    let app_data = if let Some(config_dir) = crate::paths::config_dir() {
        config_dir.join("studio-lightkid").join("app")
    } else {
        return Err("Unable to locate app data directory".to_string());
    };
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
            state.db.collection::<Document>("songs").insert_one(bson).await.map_err(e)?;
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
            state.db.collection::<Document>("sections").insert_one(bson).await.map_err(e)?;
        }
    }

    Ok(serde_json::json!({ "ok": true, "project_id": new_project_id }))
}

/// `lyrics` should always be a plain string (see the fixed prompt in commands/ai.rs), but this
/// stays defensive against the array-of-{section,lines} shape an older prompt used to produce
/// (and against any future model that doesn't follow instructions) — silently dropping to an
/// empty string here is exactly the bug that made imported songs show up with no lyrics at all.
fn lyrics_as_string(v: &Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    if let Some(arr) = v.as_array() {
        return arr.iter()
            .filter_map(|item| {
                let section = item.get("section").and_then(|s| s.as_str()).unwrap_or("");
                let lines = item.get("lines").and_then(|s| s.as_str()).unwrap_or("");
                if section.is_empty() && lines.is_empty() { return None; }
                Some(format!("[{}]\n{}", section, lines))
            })
            .collect::<Vec<_>>()
            .join("\n\n");
    }
    String::new()
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
            lyrics: lyrics_as_string(&it["lyrics"]),
            annotations: it["annotations"].as_str().unwrap_or("").to_string(),
            image_styles: it["image_styles"].as_str().unwrap_or("").to_string(),
            audio_url: None,
            video_url: None,
            duration: 0.0,
            channel_id: it["channel_id"].as_str().map(|s| s.to_string()),
            local_audio_path: None,
            local_audio_path_alt: None,
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

// Helper to sync from checked-out project.json to MongoDB
async fn sync_git_to_db(db: &crate::store::Db, repo_path: &Path, project_id: &str) -> Res<()> {
    let json_path = repo_path.join("project.json");
    if !json_path.exists() {
        return Err("project.json missing in repository".to_string());
    }
    
    let file = std::fs::File::open(&json_path).map_err(e)?;
    let payload: Value = serde_json::from_reader(file).map_err(e)?;
    
    let proj_val = payload["project"].as_object().ok_or_else(|| "project data missing".to_string())?;
    let songs_val = payload["songs"].as_array().ok_or_else(|| "songs data missing".to_string())?;
    let sections_val = payload["sections"].as_array().cloned().unwrap_or_default();
    
    // 1. Update Project in DB
    let mut project_obj = proj_val.clone();
    project_obj.insert("id".to_string(), serde_json::Value::String(project_id.to_string()));
    // preserve local_path
    project_obj.insert("local_path".to_string(), serde_json::Value::String(repo_path.to_string_lossy().to_string()));
    
    let bson_proj = bson::to_document(&project_obj).map_err(e)?;
    db.collection::<Document>("projects")
        .update_one(doc! { "id": project_id }, doc! { "$set": bson_proj })
        .upsert(true)
        .await.map_err(e)?;
        
    // 2. Delete old songs and sections from DB
    use futures_util::StreamExt;
    let mut song_cursor = db.collection::<Document>("songs")
        .find(doc! { "project_id": project_id }).await.map_err(e)?;
    let mut old_song_ids = Vec::new();
    while let Some(Ok(doc)) = song_cursor.next().await {
        if let Some(id) = doc.get_str("id").ok() {
            old_song_ids.push(id.to_string());
        }
    }
    
    if !old_song_ids.is_empty() {
        db.collection::<Document>("sections")
            .delete_many(doc! { "song_id": { "$in": &old_song_ids } }).await.map_err(e)?;
    }
    db.collection::<Document>("songs")
        .delete_many(doc! { "project_id": project_id }).await.map_err(e)?;
        
    // 3. Insert new songs with absolute paths resolved
    for song_val in songs_val {
        if let Some(mut song_obj) = song_val.as_object().cloned() {
            song_obj.insert("project_id".to_string(), serde_json::Value::String(project_id.to_string()));
            song_obj.remove("_id");
            
            if let Some(url) = song_obj.get("audio_url").and_then(|v| v.as_str()) {
                if url.starts_with("media/") {
                    let resolved = repo_path.join(url).to_string_lossy().to_string();
                    song_obj.insert("audio_url".to_string(), serde_json::Value::String(resolved));
                }
            }
            if let Some(url) = song_obj.get("video_url").and_then(|v| v.as_str()) {
                if url.starts_with("media/") {
                    let resolved = repo_path.join(url).to_string_lossy().to_string();
                    song_obj.insert("video_url".to_string(), serde_json::Value::String(resolved));
                }
            }
            
            let bson_song = bson::to_document(&song_obj).map_err(e)?;
            db.collection::<Document>("songs").insert_one(bson_song).await.map_err(e)?;
        }
    }
    
    // 4. Insert new sections with absolute paths resolved
    for section_val in sections_val {
        if let Some(mut section_obj) = section_val.as_object().cloned() {
            section_obj.remove("_id");
            
            if let Some(url) = section_obj.get("image_url").and_then(|v| v.as_str()) {
                if url.starts_with("media/") {
                    let resolved = repo_path.join(url).to_string_lossy().to_string();
                    section_obj.insert("image_url".to_string(), serde_json::Value::String(resolved));
                }
            }
            if let Some(arr) = section_obj.get("image_variants").and_then(|v| v.as_array()) {
                let new_arr = arr.iter().map(|v| {
                    if let Some(url) = v.as_str() {
                        if url.starts_with("media/") {
                            let resolved = repo_path.join(url).to_string_lossy().to_string();
                            return serde_json::Value::String(resolved);
                        }
                    }
                    v.clone()
                }).collect();
                section_obj.insert("image_variants".to_string(), serde_json::Value::Array(new_arr));
            }
            
            let bson_sec = bson::to_document(&section_obj).map_err(e)?;
            db.collection::<Document>("sections").insert_one(bson_sec).await.map_err(e)?;
        }
    }
    
    Ok(())
}

fn add_dir_to_tar<W: std::io::Write>(
    builder: &mut tar::Builder<W>,
    base_dir: &Path,
    current_dir: &Path,
    exclude_media: bool,
) -> Result<(), String> {
    for entry in std::fs::read_dir(current_dir).map_err(e)? {
        let entry = entry.map_err(e)?;
        let path = entry.path();
        let rel_path = path.strip_prefix(base_dir).map_err(e)?;

        // Always skip the git internals: the local repo (browsable/checkout-able in place via
        // the Dashboard's Version History panel) is the actual history store. Including it in
        // exported/uploaded tars would (a) silently defeat `exclude_media` — git's object store
        // is content-addressed, so every media file ever committed stays reachable from
        // `.git/objects` even once the current `media/` dir is excluded from the tar listing —
        // and (b) make every tar grow to contain the *entire* accumulated history instead of a
        // snapshot of current state. Verified via a real tar dump before this fix: a
        // "gdrive_exclude" save still smuggled a previously-committed media blob in via .git.
        if rel_path.starts_with(".git") {
            continue;
        }

        if exclude_media && rel_path.starts_with("media") {
            continue;
        }

        if path.is_dir() {
            add_dir_to_tar(builder, base_dir, &path, exclude_media)?;
        } else if path.is_file() {
            let mut file = std::fs::File::open(&path).map_err(e)?;
            builder.append_file(rel_path, &mut file).map_err(e)?;
        }
    }
    Ok(())
}

// Google Drive OAuth token refresh
async fn refresh_gdrive_token(db: &crate::store::Db, project_id: &str) -> Result<String, String> {
    let settings = db.collection::<Document>("settings")
        .find_one(doc! { "_id": project_id }).await.map_err(e)?
        .ok_or_else(|| "Project settings not found. Configure Google OAuth in settings first.".to_string())?;
    
    let refresh_token = settings.get("gdrive_refresh_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Google Drive not connected. Go to Settings and authorize your Google Account.".to_string())?;
    
    let ch = serde_json::json!({ "google_oauth_client_id": settings.get("google_oauth_client_id").and_then(|v| v.as_str()) });
    let oauth = crate::jobs::pick_oauth_client(db, &ch, settings.get("google_oauth_client_id").and_then(|v| v.as_str())).await
        .ok_or_else(|| "No Google OAuth client configured.".to_string())?;
    
    let client_id = oauth["client_id"].as_str().ok_or("Missing client_id")?;
    let client_secret = oauth["client_secret"].as_str().ok_or("Missing client_secret")?;
    
    let http = reqwest::Client::new();
    let tr = http.post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send().await.map_err(e)?;
        
    if !tr.status().is_success() {
        return Err(format!("Google token refresh failed: {}", tr.status()));
    }
    
    let tokens: Value = tr.json().await.map_err(e)?;
    let access_token = tokens["access_token"].as_str()
        .ok_or("No access token returned")?.to_string();
        
    Ok(access_token)
}

// Google Drive multipart upload
async fn upload_to_gdrive(access_token: &str, file_name: &str, file_data: &[u8]) -> Result<String, String> {
    let http = reqwest::Client::new();
    let boundary = "foo_bar_baz_boundary";
    
    let metadata = serde_json::json!({
        "name": file_name,
        "mimeType": "application/x-tar"
    });
    
    let metadata_str = serde_json::to_string(&metadata).map_err(e)?;
    
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(b"Content-Type: application/json; charset=UTF-8\r\n\r\n");
    body.extend_from_slice(metadata_str.as_bytes());
    body.extend_from_slice(b"\r\n");
    
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(b"Content-Type: application/x-tar\r\n\r\n");
    body.extend_from_slice(file_data);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
    
    let res = http.post("https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart")
        .bearer_auth(access_token)
        .header("Content-Type", format!("multipart/related; boundary={}", boundary))
        .body(body)
        .send().await.map_err(e)?;
        
    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        return Err(format!("Google Drive upload failed ({}): {}", status, err_text));
    }
    
    let json: Value = res.json().await.map_err(e)?;
    let id = json["id"].as_str().ok_or("No file ID returned from Google Drive")?.to_string();
    Ok(id)
}

#[tauri::command]
pub async fn get_project_git_info(state: State<'_, AppState>, project_id: String) -> Res<Value> {
    let project_doc = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &project_id }).await.map_err(e)?
        .ok_or_else(|| "Project not found".to_string())?;
    let project = bson_to_value(project_doc);
    let local_path = project["local_path"].as_str().unwrap_or("").trim().to_string();
    
    if local_path.is_empty() {
        return Ok(serde_json::json!({
            "local_path": null,
            "is_git": false,
            "current_branch": null,
            "git_status": null,
            "branches": [],
            "tags": [],
        }));
    }
    
    let path = Path::new(&local_path);
    if !path.exists() || !path.join(".git").exists() {
        return Ok(serde_json::json!({
            "local_path": local_path,
            "is_git": false,
            "current_branch": null,
            "git_status": null,
            "branches": [],
            "tags": [],
        }));
    }
    
    let branch_out = std::process::Command::new("git")
        .args(["symbolic-ref", "--short", "-q", "HEAD"])
        .current_dir(path)
        .output();
        
    let current_branch = match branch_out {
        Ok(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        }
        _ => {
            let desc = std::process::Command::new("git")
                .args(["describe", "--tags", "--always"])
                .current_dir(path)
                .output();
            match desc {
                Ok(out) if out.status.success() => {
                    format!("(detached HEAD at {})", String::from_utf8_lossy(&out.stdout).trim())
                }
                _ => "(detached HEAD)".to_string()
            }
        }
    };
    
    let is_detached = current_branch.contains("detached");
    
    let diff = std::process::Command::new("git")
        .args(["diff", "--quiet"])
        .current_dir(path)
        .status();
    let is_dirty = match diff {
        Ok(status) => !status.success(),
        _ => false,
    };
    
    let git_status = if is_detached {
        "detached"
    } else if is_dirty {
        "dirty"
    } else {
        "clean"
    };
    
    let branch_list_out = std::process::Command::new("git")
        .args(["branch", "--format=%(refname:short)"])
        .current_dir(path)
        .output();
    let mut branches = Vec::new();
    if let Ok(out) = branch_list_out {
        if out.status.success() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let name = line.trim();
                if !name.is_empty() {
                    branches.push(name.to_string());
                }
            }
        }
    }
    
    let tag_list_out = std::process::Command::new("git")
        .args(["for-each-ref", "--sort=-creatordate", "--format=%(refname:short)|%(creatordate:iso)|%(subject)", "refs/tags"])
        .current_dir(path)
        .output();
    
    let mut tags = Vec::new();
    if let Ok(out) = tag_list_out {
        if out.status.success() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 3 {
                    tags.push(serde_json::json!({
                        "tag": parts[0].trim(),
                        "date": parts[1].trim(),
                        "subject": parts[2].trim(),
                    }));
                }
            }
        }
    }
    
    Ok(serde_json::json!({
        "local_path": local_path,
        "is_git": true,
        "current_branch": current_branch,
        "is_detached": is_detached,
        "git_status": git_status,
        "branches": branches,
        "tags": tags,
    }))
}

#[tauri::command]
pub async fn save_project_version(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    project_id: String,
    save_type: String,
    branch_to_create: Option<String>
) -> Res<Value> {
    // Saving a named version out of the app is the "keep a copy" promise the terms describe.
    crate::commands::subscription::require(&state, "save_copies").await?;
    let project_doc = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &project_id }).await.map_err(e)?
        .ok_or_else(|| "Project not found".to_string())?;
    let project = bson_to_value(project_doc);
    let local_path = project["local_path"].as_str().unwrap_or("").trim().to_string();
    if local_path.is_empty() {
        return Ok(serde_json::json!({ "status": "needs_path" }));
    }
    
    let path = Path::new(&local_path);
    if !path.exists() {
        std::fs::create_dir_all(path).map_err(e)?;
    }
    
    let git_dir = path.join(".git");
    if !git_dir.exists() {
        let _ = std::process::Command::new("git").arg("init").current_dir(path).output();
        let _ = std::process::Command::new("git").args(["config", "user.name", "BibleMusically"]).current_dir(path).output();
        let _ = std::process::Command::new("git").args(["config", "user.email", "biblemusically@local"]).current_dir(path).output();
    }
    
    let branch_out = std::process::Command::new("git")
        .args(["symbolic-ref", "--short", "-q", "HEAD"])
        .current_dir(path)
        .output();
        
    let is_detached = match branch_out {
        Ok(out) => !out.status.success(),
        Err(_) => true,
    };
    
    if is_detached {
        if let Some(new_branch) = branch_to_create {
            let checkout_out = std::process::Command::new("git")
                .args(["checkout", "-b", &new_branch])
                .current_dir(path)
                .output()
                .map_err(|err| format!("Failed to create branch: {}", err))?;
            if !checkout_out.status.success() {
                let err_msg = String::from_utf8_lossy(&checkout_out.stderr).trim().to_string();
                return Err(format!("Failed to create branch: {}", err_msg));
            }
        } else {
            return Ok(serde_json::json!({ "status": "needs_branch_creation" }));
        }
    }
    
    use futures_util::StreamExt;
    let mut song_cursor = state.db.collection::<Document>("songs")
        .find(doc! { "project_id": &project_id }).await.map_err(e)?;
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
    
    let media_dir = path.join("media");
    std::fs::create_dir_all(&media_dir).map_err(e)?;
    
    let mut project_out = project.clone();
    let mut songs_out = songs.clone();
    let mut sections_out = sections.clone();
    
    if let Some(url) = project_out["image_url"].as_str() {
        if let Some(copied) = copy_media_if_local(url, &media_dir) {
            let filename = Path::new(&copied).file_name().unwrap().to_str().unwrap();
            project_out["image_url"] = serde_json::Value::String(format!("media/{}", filename));
        }
    }
    
    for song in &mut songs_out {
        if let Some(url) = song["audio_url"].as_str() {
            if let Some(copied) = copy_media_if_local(url, &media_dir) {
                let filename = Path::new(&copied).file_name().unwrap().to_str().unwrap();
                song["audio_url"] = serde_json::Value::String(format!("media/{}", filename));
            }
        }
        if let Some(url) = song["video_url"].as_str() {
            if let Some(copied) = copy_media_if_local(url, &media_dir) {
                let filename = Path::new(&copied).file_name().unwrap().to_str().unwrap();
                song["video_url"] = serde_json::Value::String(format!("media/{}", filename));
            }
        }
    }
    
    for section in &mut sections_out {
        if let Some(url) = section["image_url"].as_str() {
            if let Some(copied) = copy_media_if_local(url, &media_dir) {
                let filename = Path::new(&copied).file_name().unwrap().to_str().unwrap();
                section["image_url"] = serde_json::Value::String(format!("media/{}", filename));
            }
        }
        if let Some(arr) = section["image_variants"].as_array() {
            let new_arr = arr.iter().map(|v| {
                if let Some(url) = v.as_str() {
                    if let Some(copied) = copy_media_if_local(url, &media_dir) {
                        let filename = Path::new(&copied).file_name().unwrap().to_str().unwrap();
                        serde_json::Value::String(format!("media/{}", filename))
                    } else {
                        v.clone()
                    }
                } else { v.clone() }
            }).collect();
            section["image_variants"] = serde_json::Value::Array(new_arr);
        }
    }
    
    let project_file = path.join("project.json");
    let export_payload = serde_json::json!({
        "project": project_out,
        "songs": songs_out,
        "sections": sections_out,
        "exported_at": now_iso(),
    });
    
    let file = std::fs::File::create(&project_file).map_err(e)?;
    serde_json::to_writer_pretty(file, &export_payload).map_err(e)?;
    
    let _ = std::process::Command::new("git").arg("add").arg("-A").current_dir(path).output();
    let commit_msg = format!("Save: {}", chrono::Local::now().to_rfc3339());
    let _ = std::process::Command::new("git").args(["commit", "-m", &commit_msg]).current_dir(path).output();
    
    let date_prefix = chrono::Local::now().format("%Y-%m-%d").to_string();
    let tag_pattern = format!("{}.*", date_prefix);
    
    let existing_tags_out = std::process::Command::new("git")
        .args(["tag", "-l", &tag_pattern])
        .current_dir(path)
        .output();
        
    let mut next_suffix = 1;
    if let Ok(out) = existing_tags_out {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if let Some(suffix_str) = line.trim().strip_prefix(&format!("{}.", date_prefix)) {
                    if let Ok(num) = suffix_str.parse::<i32>() {
                        if num >= next_suffix {
                            next_suffix = num + 1;
                        }
                    }
                }
            }
        }
    }
    
    let new_tag = format!("{}.{}", date_prefix, next_suffix);
    let _ = std::process::Command::new("git").args(["tag", "-a", &new_tag, "-m", &commit_msg]).current_dir(path).output();
    
    let mut tar_data = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_data);
        let exclude_media = save_type == "gdrive_exclude";
        add_dir_to_tar(&mut builder, path, path, exclude_media)?;
        builder.into_inner().map_err(e)?;
    }
    
    if save_type == "local" {
        if let Some(parent) = path.parent() {
            let tar_name = format!("{}.tar", path.file_name().unwrap().to_str().unwrap());
            let tar_dest = parent.join(tar_name);
            std::fs::write(&tar_dest, &tar_data).map_err(e)?;
        }
    } else {
        let access_token = refresh_gdrive_token(&state.db, &project_id).await?;
        let tar_name = if save_type == "gdrive_exclude" {
            format!("{}-no-media.tar", project["name"].as_str().unwrap_or("project"))
        } else {
            format!("{}.tar", project["name"].as_str().unwrap_or("project"))
        };
        upload_to_gdrive(&access_token, &tar_name, &tar_data).await?;
    }
    
    Ok(serde_json::json!({
        "status": "success",
        "tag": new_tag,
    }))
}

#[tauri::command]
pub async fn checkout_project_git_tag(state: State<'_, AppState>, project_id: String, tag: String) -> Res<Value> {
    let project_doc = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &project_id }).await.map_err(e)?
        .ok_or_else(|| "Project not found".to_string())?;
    let project = bson_to_value(project_doc);
    let local_path = project["local_path"].as_str().unwrap_or("").trim().to_string();
    if local_path.is_empty() {
        return Err("Project is not linked to a local directory".into());
    }
    
    let path = Path::new(&local_path);
    let checkout_out = std::process::Command::new("git")
        .args(["checkout", &tag])
        .current_dir(path)
        .output()
        .map_err(|err| format!("Failed to run git checkout: {}", err))?;
        
    if !checkout_out.status.success() {
        let err_msg = String::from_utf8_lossy(&checkout_out.stderr).trim().to_string();
        return Err(format!("Git checkout failed: {}", err_msg));
    }
    
    sync_git_to_db(&state.db, path, &project_id).await?;
    // The checkout replaced files on disk (including this project's data/*.json), so drop the
    // store's in-memory shards and let the next read parse what git just put there.
    state.db.invalidate_cache().await;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn checkout_project_git_branch(state: State<'_, AppState>, project_id: String, branch: String) -> Res<Value> {
    let project_doc = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &project_id }).await.map_err(e)?
        .ok_or_else(|| "Project not found".to_string())?;
    let project = bson_to_value(project_doc);
    let local_path = project["local_path"].as_str().unwrap_or("").trim().to_string();
    if local_path.is_empty() {
        return Err("Project is not linked to a local directory".into());
    }
    
    let path = Path::new(&local_path);
    let checkout_out = std::process::Command::new("git")
        .args(["checkout", &branch])
        .current_dir(path)
        .output()
        .map_err(|err| format!("Failed to run git checkout: {}", err))?;
        
    if !checkout_out.status.success() {
        let err_msg = String::from_utf8_lossy(&checkout_out.stderr).trim().to_string();
        return Err(format!("Git checkout failed: {}", err_msg));
    }
    
    sync_git_to_db(&state.db, path, &project_id).await?;
    // The checkout replaced files on disk (including this project's data/*.json), so drop the
    // store's in-memory shards and let the next read parse what git just put there.
    state.db.invalidate_cache().await;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn create_project_git_branch(state: State<'_, AppState>, project_id: String, branch_name: String) -> Res<Value> {
    let project_doc = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &project_id }).await.map_err(e)?
        .ok_or_else(|| "Project not found".to_string())?;
    let project = bson_to_value(project_doc);
    let local_path = project["local_path"].as_str().unwrap_or("").trim().to_string();
    if local_path.is_empty() {
        return Err("Project is not linked to a local directory".into());
    }
    
    let path = Path::new(&local_path);
    let checkout_out = std::process::Command::new("git")
        .args(["checkout", "-b", &branch_name])
        .current_dir(path)
        .output()
        .map_err(|err| format!("Failed to create branch: {}", err))?;
        
    if !checkout_out.status.success() {
        let err_msg = String::from_utf8_lossy(&checkout_out.stderr).trim().to_string();
        return Err(format!("Git branch creation failed: {}", err_msg));
    }
    
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn authorize_project_gdrive(
    state: State<'_, AppState>,
    project_id: String,
) -> Res<Value> {
    let settings_doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": &project_id }).await.map_err(e)?
        .map(bson_to_value)
        .unwrap_or_default();
        
    let client = crate::jobs::pick_oauth_client(
        &state.db,
        &serde_json::json!({ "google_oauth_client_id": settings_doc.get("google_oauth_client_id").and_then(|v| v.as_str()) }),
        settings_doc.get("google_oauth_client_id").and_then(|v| v.as_str())
    ).await.ok_or_else(|| "No Google OAuth client configured.".to_string())?;
    
    let cid_g = client["client_id"].as_str().unwrap_or("").to_string();
    let csec = client["client_secret"].as_str().unwrap_or("").to_string();
    let redirect = client["redirect_uri"].as_str().unwrap_or("").to_string();
    
    if cid_g.is_empty() || csec.is_empty() || redirect.is_empty() {
        return Err("OAuth client missing client_id/client_secret/redirect_uri".into());
    }
    
    let parsed = Url::parse(&redirect).map_err(e)?;
    let host = parsed.host_str().unwrap_or("");
    if host != "127.0.0.1" && host != "localhost" {
        return Err("OAuth client redirect_uri must be a loopback address (127.0.0.1 or localhost)".into());
    }
    
    let (tx, mut rx) = mpsc::channel::<HashMap<String, String>>(1);
    let tx_filter = warp::any().map(move || tx.clone());
    
    let callback_route = warp::get()
        .and(warp::path::full())
        .and(warp::query::<HashMap<String, String>>())
        .and(tx_filter.clone())
        .and_then(|_full: warp::path::FullPath, qs: HashMap<String, String>, tx: mpsc::Sender<HashMap<String, String>>| async move {
            if qs.contains_key("code") || qs.contains_key("error") {
                let _ = tx.try_send(qs);
            }
            let body = "<html><body style='font-family:sans-serif;text-align:center;padding:60px'><h2>✓ Google Drive Authorization complete</h2><p>You can close this window and return to the app.</p></body></html>";
            Ok::<_, std::convert::Infallible>(warp::reply::html(body))
        });
        
    let port = parsed.port().unwrap_or(0);
    let bind_addr: SocketAddr = format!("127.0.0.1:{}", port).parse().map_err(e)?;
    let tcp = TcpListener::bind(bind_addr).await
        .map_err(|err| format!("Cannot bind OAuth callback port {}: {}", port, err))?;
    let bound_port = tcp.local_addr().map_err(e)?.port();
    let std_listener = tcp.into_std().map_err(e)?;
    
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server = warp::serve(callback_route)
        .serve_incoming_with_graceful_shutdown(
            tokio_stream::wrappers::TcpListenerStream::new(
                TcpListener::from_std(std_listener).map_err(e)?
            ),
            async move { shutdown_rx.await.ok(); },
        );
    let server_task = tokio::task::spawn(server);
    
    let mut redirect_used = parsed.clone();
    redirect_used.set_port(Some(bound_port)).ok();
    let redirect_used_str = redirect_used.to_string();
    
    let scope = "https://www.googleapis.com/auth/drive.file https://www.googleapis.com/auth/userinfo.email";
    let state_param = project_id.clone();
    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?response_type=code&client_id={}&redirect_uri={}&scope={}&access_type=offline&prompt=consent&state={}",
        cid_g,
        urlencoding::encode(&redirect_used_str),
        urlencoding::encode(scope),
        state_param
    );
    
    let _ = open::that(auth_url);
    
    let result = match tokio::time::timeout(std::time::Duration::from_secs(120), rx.recv()).await {
        Ok(Some(qs)) => {
            if let Some(err) = qs.get("error") {
                let _ = shutdown_tx.send(());
                let _ = server_task.await;
                return Err(format!("OAuth error: {}", err));
            }
            let code = qs.get("code").cloned().unwrap_or_default();
            let returned_state = qs.get("state").cloned().unwrap_or_default();
            if code.is_empty() || returned_state != state_param {
                let _ = shutdown_tx.send(());
                let _ = server_task.await;
                return Err("Missing code or invalid state in callback".into());
            }
            
            let http = reqwest::Client::new();
            let tr = http.post("https://oauth2.googleapis.com/token")
                .form(&[
                    ("code", code.as_str()),
                    ("client_id", cid_g.as_str()),
                    ("client_secret", csec.as_str()),
                    ("redirect_uri", redirect_used_str.as_str()),
                    ("grant_type", "authorization_code"),
                ])
                .send().await.map_err(e)?;
                
            if !tr.status().is_success() {
                let error_body = tr.text().await.unwrap_or_default();
                let _ = shutdown_tx.send(());
                let _ = server_task.await;
                return Err(format!("Token exchange failed: {}", error_body));
            }
            
            let tokens: Value = tr.json().await.map_err(e)?;
            let refresh = tokens["refresh_token"].as_str().unwrap_or("").to_string();
            let access = tokens["access_token"].as_str().unwrap_or("").to_string();
            
            let mut email = "Connected".to_string();
            if !access.is_empty() {
                let ur = http.get("https://www.googleapis.com/oauth2/v2/userinfo")
                    .bearer_auth(&access)
                    .send().await;
                if let Ok(ur) = ur {
                    if ur.status().is_success() {
                        if let Ok(info) = ur.json::<Value>().await {
                            if let Some(em) = info["email"].as_str() {
                                email = em.to_string();
                            }
                        }
                    }
                }
            }
            
            let mut update_fields = doc! {
                "gdrive_connected": true,
                "gdrive_email": &email,
            };
            if !refresh.is_empty() {
                update_fields.insert("gdrive_refresh_token", &refresh);
            }
            
            state.db.collection::<Document>("settings")
                .update_one(doc! { "_id": &project_id }, doc! { "$set": update_fields })
                .upsert(true)
                .await.map_err(e)?;
                
            let _ = shutdown_tx.send(());
            let _ = server_task.await;
            Ok(serde_json::json!({
                "ok": true,
                "email": email,
            }))
        }
        _ => {
            let _ = shutdown_tx.send(());
            let _ = server_task.await;
            Err("OAuth authorization timed out or cancelled".into())
        }
    };
    result
}
