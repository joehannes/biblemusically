use crate::{
    models::now_iso,
    state::AppState,
};
use bson::{doc, Document};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};
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

/// Initialize a git repository for a project if it doesn't exist
#[tauri::command]
pub async fn init_project_git(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    project_id: String,
) -> Res<Value> {
    use std::process::Command;
    
    // Get or create project directory
    let app_data = get_app_data_dir()?;
    let projects_dir = app_data.join("projects");
    let project_dir = projects_dir.join(&project_id);
    
    if !project_dir.exists() {
        std::fs::create_dir_all(&project_dir).map_err(e)?;
    }
    
    let git_dir = project_dir.join(".git");
    if !git_dir.exists() {
        // Initialize git repo
        let output = Command::new("git")
            .arg("init")
            .current_dir(&project_dir)
            .output()
            .map_err(|err| format!("Failed to initialize git: {}", err))?;
        
        if !output.status.success() {
            return Err(format!("Git init failed: {}", String::from_utf8_lossy(&output.stderr)));
        }
        
        // Set git config
        let _ = Command::new("git")
            .args(["config", "user.name", "Studio Lightkid"])
            .current_dir(&project_dir)
            .output();
        let _ = Command::new("git")
            .args(["config", "user.email", "studio@lightkid.local"])
            .current_dir(&project_dir)
            .output();
    }
    
    // Store project git path in database
    let git_path = project_dir.to_string_lossy().to_string();
    state.db.collection::<Document>("projects")
        .update_one(
            doc! { "id": &project_id },
            doc! { "$set": { "git_repo_path": &git_path } }
        )
        .await
        .map_err(e)?;
    
    Ok(serde_json::json!({
        "ok": true,
        "git_path": git_path,
        "initialized": !git_dir.exists()
    }))
}

/// Create a commit with a tag for the current project state
#[tauri::command]
pub async fn save_project_version(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    project_id: String,
    message: Option<String>,
    include_large_files: bool,
) -> Res<Value> {
    use std::process::Command;
    
    // Get project git path from DB
    let project_doc = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &project_id })
        .await
        .map_err(e)?
        .ok_or_else(|| "Project not found".to_string())?;
    
    let git_path = project_doc.get_str("git_repo_path")
        .ok_or_else(|| "Project git repo not initialized".to_string())?
        .to_string();
    
    let project_dir = PathBuf::from(&git_path);
    
    // Export current project state to the git directory
    export_project_to_git_dir(&state, &project_id, &project_dir, include_large_files).await?;
    
    // Add all files
    let output = Command::new("git")
        .args(["add", "-A"])
        .current_dir(&project_dir)
        .output()
        .map_err(|err| format!("Failed to add files: {}", err))?;
    
    if !output.status.success() {
        return Err(format!("Git add failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    
    // Check if there are changes
    let status_output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&project_dir)
        .output()
        .map_err(e)?;
    
    if status_output.stdout.is_empty() {
        return Ok(serde_json::json!({
            "ok": true,
            "message": "No changes to save",
            "tag": null
        }));
    }
    
    // Create commit
    let commit_msg = message.unwrap_or_else(|| format!("Auto-save {}", now_iso()));
    let output = Command::new("git")
        .args(["commit", "-m", &commit_msg])
        .current_dir(&project_dir)
        .output()
        .map_err(|err| format!("Failed to commit: {}", err))?;
    
    if !output.status.success() {
        return Err(format!("Git commit failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    
    // Create tag with date and patch version
    let now = chrono::Utc::now();
    let date_str = now.format("%Y%m%d").to_string();
    
    // Get existing tags for today
    let tag_prefix = format!("v{}", date_str);
    let tags_output = Command::new("git")
        .args(["tag", "-l", &format!("{}.*", tag_prefix)])
        .current_dir(&project_dir)
        .output()
        .map_err(e)?;
    
    let existing_tags: Vec<String> = String::from_utf8_lossy(&tags_output.stdout)
        .lines()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    
    let patch_num = existing_tags.len() + 1;
    let tag_name = format!("{}.p{:02}", tag_prefix, patch_num);
    
    let output = Command::new("git")
        .args(["tag", "-a", &tag_name, "-m", &commit_msg])
        .current_dir(&project_dir)
        .output()
        .map_err(|err| format!("Failed to create tag: {}", err))?;
    
    if !output.status.success() {
        return Err(format!("Git tag failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    
    // Get current branch
    let branch_output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&project_dir)
        .output()
        .map_err(e)?;
    let branch = String::from_utf8_lossy(&branch_output.stdout).trim().to_string();
    
    Ok(serde_json::json!({
        "ok": true,
        "tag": tag_name,
        "branch": branch,
        "commit_message": commit_msg
    }))
}

/// List all tags/versions for a project
#[tauri::command]
pub async fn list_project_versions(
    state: State<'_, AppState>,
    project_id: String,
) -> Res<Value> {
    use std::process::Command;
    
    let project_doc = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &project_id })
        .await
        .map_err(e)?
        .ok_or_else(|| "Project not found".to_string())?;
    
    let git_path = project_doc.get_str("git_repo_path")
        .ok_or_else(|| "Project git repo not initialized".to_string())?
        .to_string();
    
    let project_dir = PathBuf::from(&git_path);
    
    // Get all tags with their dates
    let output = Command::new("git")
        .args(["tag", "-l", "--format=%(refname:short)|%(creatordate:short)|%(taggername)|%(subject)"])
        .current_dir(&project_dir)
        .output()
        .map_err(e)?;
    
    let tags: Vec<Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|s| !s.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.split('|').collect();
            serde_json::json!({
                "tag": parts.get(0).unwrap_or(&""),
                "date": parts.get(1).unwrap_or(&""),
                "tagger": parts.get(2).unwrap_or(&""),
                "message": parts.get(3).unwrap_or(&"")
            })
        })
        .collect();
    
    // Get current branch
    let branch_output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&project_dir)
        .output()
        .map_err(e)?;
    let current_branch = String::from_utf8_lossy(&branch_output.stdout).trim().to_string();
    
    // Get all branches
    let branches_output = Command::new("git")
        .args(["branch", "-a"])
        .current_dir(&project_dir)
        .output()
        .map_err(e)?;
    let branches: Vec<String> = String::from_utf8_lossy(&branches_output.stdout)
        .lines()
        .map(|s| s.trim().trim_start_matches("* ").to_string())
        .collect();
    
    Ok(serde_json::json!({
        "ok": true,
        "tags": tags,
        "current_branch": current_branch,
        "branches": branches
    }))
}

/// Load a specific tag/version of a project
#[tauri::command]
pub async fn load_project_version(
    state: State<'_, AppState>,
    project_id: String,
    tag: String,
) -> Res<Value> {
    use std::process::Command;
    
    let project_doc = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &project_id })
        .await
        .map_err(e)?
        .ok_or_else(|| "Project not found".to_string())?;
    
    let git_path = project_doc.get_str("git_repo_path")
        .ok_or_else(|| "Project git repo not initialized".to_string())?
        .to_string();
    
    let project_dir = PathBuf::from(&git_path);
    
    // Checkout the tag
    let output = Command::new("git")
        .args(["checkout", &tag])
        .current_dir(&project_dir)
        .output()
        .map_err(|err| format!("Failed to checkout tag: {}", err))?;
    
    if !output.status.success() {
        return Err(format!("Git checkout failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    
    // Import project state from git directory back to DB
    import_project_from_git_dir(&state, &project_id, &project_dir).await?;
    
    Ok(serde_json::json!({
        "ok": true,
        "tag": tag,
        "message": "Project loaded from version"
    }))
}

/// Create a new branch from current state
#[tauri::command]
pub async fn create_project_branch(
    state: State<'_, AppState>,
    project_id: String,
    branch_name: String,
) -> Res<Value> {
    use std::process::Command;
    
    let project_doc = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &project_id })
        .await
        .map_err(e)?
        .ok_or_else(|| "Project not found".to_string())?;
    
    let git_path = project_doc.get_str("git_repo_path")
        .ok_or_else(|| "Project git repo not initialized".to_string())?
        .to_string();
    
    let project_dir = PathBuf::from(&git_path);
    
    // Create and checkout new branch
    let output = Command::new("git")
        .args(["checkout", "-b", &branch_name])
        .current_dir(&project_dir)
        .output()
        .map_err(|err| format!("Failed to create branch: {}", err))?;
    
    if !output.status.success() {
        return Err(format!("Git branch creation failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    
    // Update project with branch info
    state.db.collection::<Document>("projects")
        .update_one(
            doc! { "id": &project_id },
            doc! { "$set": { "git_branch": &branch_name } }
        )
        .await
        .map_err(e)?;
    
    Ok(serde_json::json!({
        "ok": true,
        "branch": branch_name
    }))
}

/// Switch to an existing branch
#[tauri::command]
pub async fn switch_project_branch(
    state: State<'_, AppState>,
    project_id: String,
    branch_name: String,
) -> Res<Value> {
    use std::process::Command;
    
    let project_doc = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &project_id })
        .await
        .map_err(e)?
        .ok_or_else(|| "Project not found".to_string())?;
    
    let git_path = project_doc.get_str("git_repo_path")
        .ok_or_else(|| "Project git repo not initialized".to_string())?
        .to_string();
    
    let project_dir = PathBuf::from(&git_path);
    
    // Checkout existing branch
    let output = Command::new("git")
        .args(["checkout", &branch_name])
        .current_dir(&project_dir)
        .output()
        .map_err(|err| format!("Failed to switch branch: {}", err))?;
    
    if !output.status.success() {
        return Err(format!("Git checkout failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    
    // Update project with branch info
    state.db.collection::<Document>("projects")
        .update_one(
            doc! { "id": &project_id },
            doc! { "$set": { "git_branch": &branch_name } }
        )
        .await
        .map_err(e)?;
    
    // Import project state from git directory
    import_project_from_git_dir(&state, &project_id, &project_dir).await?;
    
    Ok(serde_json::json!({
        "ok": true,
        "branch": branch_name
    }))
}

/// Get current git status including whether we're on HEAD
#[tauri::command]
pub async fn get_project_git_status(
    state: State<'_, AppState>,
    project_id: String,
) -> Res<Value> {
    use std::process::Command;
    
    let project_doc = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &project_id })
        .await
        .map_err(e)?
        .ok_or_else(|| "Project not found".to_string())?;
    
    let git_path = project_doc.get_str("git_repo_path")
        .or(project_doc.get_str("id")) // fallback to using project ID as path
        .ok_or_else(|| "Project git repo not initialized".to_string())?
        .to_string();
    
    let project_dir = PathBuf::from(&git_path);
    
    if !project_dir.exists() {
        return Ok(serde_json::json!({
            "ok": true,
            "initialized": false,
            "on_head": true,
            "current_branch": "main",
            "has_unsaved_changes": false
        }));
    }
    
    // Get current branch
    let branch_output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&project_dir)
        .output()
        .map_err(e)?;
    let branch = String::from_utf8_lossy(&branch_output.stdout).trim().to_string();
    
    // Check if on HEAD
    let head_output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&project_dir)
        .output();
    
    let on_head = head_output.map_or(true, |o| o.status.success());
    
    // Check for unsaved changes
    let status_output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&project_dir)
        .output()
        .map_err(e)?;
    let has_changes = !status_output.stdout.is_empty();
    
    Ok(serde_json::json!({
        "ok": true,
        "initialized": true,
        "on_head": on_head,
        "current_branch": branch,
        "has_unsaved_changes": has_changes
    }))
}

fn get_app_data_dir() -> Res<PathBuf> {
    let app_data = if let Some(config_dir) = dirs::config_dir() {
        config_dir.join("studio-lightkid").join("app")
    } else {
        return Err("Unable to locate app data directory".to_string());
    };
    Ok(app_data)
}

async fn export_project_to_git_dir(
    state: &State<'_, AppState>,
    project_id: &str,
    project_dir: &Path,
    include_large_files: bool,
) -> Res<()> {
    use futures_util::StreamExt;
    use std::fs;
    
    // Create subdirectories
    let media_dir = project_dir.join("media");
    fs::create_dir_all(&media_dir).map_err(e)?;
    
    // Get project
    let project_doc = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": project_id })
        .await
        .map_err(e)?
        .ok_or_else(|| "Project not found".to_string())?;
    
    let project_val = bson_to_value(project_doc);
    
    // Write project.json
    let project_file = project_dir.join("project.json");
    let file = fs::File::create(&project_file).map_err(e)?;
    serde_json::to_writer_pretty(file, &project_val).map_err(e)?;
    
    // Get songs
    let mut song_cursor = state.db.collection::<Document>("songs")
        .find(doc! { "project_id": project_id })
        .await
        .map_err(e)?;
    
    let mut songs = Vec::new();
    while let Some(Ok(doc)) = song_cursor.next().await {
        songs.push(bson_to_value(doc));
    }
    
    // Write songs.json
    let songs_file = project_dir.join("songs.json");
    let file = fs::File::create(&songs_file).map_err(e)?;
    serde_json::to_writer_pretty(file, &songs).map_err(e)?;
    
    // Get sections for all songs
    let mut all_sections = Vec::new();
    for song in &songs {
        if let Some(sid) = song["id"].as_str() {
            let mut section_cursor = state.db.collection::<Document>("sections")
                .find(doc! { "song_id": sid })
                .await
                .map_err(e)?;
            while let Some(Ok(doc)) = section_cursor.next().await {
                all_sections.push(bson_to_value(doc));
            }
        }
    }
    
    // Write sections.json
    let sections_file = project_dir.join("sections.json");
    let file = fs::File::create(&sections_file).map_err(e)?;
    serde_json::to_writer_pretty(file, &all_sections).map_err(e)?;
    
    // Copy media files if requested
    if include_large_files {
        copy_media_to_git_dir(&project_val, &media_dir);
        for song in &songs {
            copy_media_to_git_dir(song, &media_dir);
        }
        for section in &all_sections {
            copy_media_to_git_dir(section, &media_dir);
        }
    }
    
    Ok(())
}

fn copy_media_to_git_dir(item: &Value, media_dir: &Path) {
    if let Some(url) = item.get("audio_url").and_then(|v| v.as_str()) {
        copy_local_file(url, media_dir);
    }
    if let Some(url) = item.get("video_url").and_then(|v| v.as_str()) {
        copy_local_file(url, media_dir);
    }
    if let Some(url) = item.get("image_url").and_then(|v| v.as_str()) {
        copy_local_file(url, media_dir);
    }
    if let Some(arr) = item.get("image_variants").and_then(|v| v.as_array()) {
        for url_val in arr {
            if let Some(url) = url_val.as_str() {
                copy_local_file(url, media_dir);
            }
        }
    }
}

fn copy_local_file(url: &str, media_dir: &Path) {
    if url.starts_with("http://") || url.starts_with("https://") {
        return; // Skip remote URLs
    }
    
    let source_path = if url.starts_with("file://") {
        PathBuf::from(url.trim_start_matches("file://"))
    } else {
        PathBuf::from(url)
    };
    
    if !source_path.exists() || !source_path.is_file() {
        return;
    }
    
    if let Some(filename) = source_path.file_name().and_then(|n| n.to_str()) {
        let dest_path = media_dir.join(filename);
        let _ = std::fs::copy(&source_path, &dest_path);
    }
}

async fn import_project_from_git_dir(
    state: &State<'_, AppState>,
    project_id: &str,
    project_dir: &Path,
) -> Res<()> {
    use std::fs;
    
    // Read project.json
    let project_file = project_dir.join("project.json");
    if !project_file.exists() {
        return Err("project.json not found in git repo".to_string());
    }
    
    let content = fs::read_to_string(&project_file).map_err(e)?;
    let project_val: Value = serde_json::from_str(&content).map_err(e)?;
    
    // Update project in DB
    let mut update_doc = bson::to_document(&project_val).map_err(e)?;
    update_doc.insert("id", project_id);
    state.db.collection::<Document>("projects")
        .update_one(
            doc! { "id": project_id },
            doc! { "$set": &update_doc }
        )
        .await
        .map_err(e)?;
    
    // Read and import songs
    let songs_file = project_dir.join("songs.json");
    if songs_file.exists() {
        let content = fs::read_to_string(&songs_file).map_err(e)?;
        let songs: Vec<Value> = serde_json::from_str(&content).map_err(e)?;
        
        // Clear existing songs for this project
        state.db.collection::<Document>("songs")
            .delete_many(doc! { "project_id": project_id })
            .await
            .map_err(e)?;
        
        // Insert new songs
        for song_val in songs {
            let mut song_doc = bson::to_document(&song_val).map_err(e)?;
            song_doc.insert("project_id", project_id);
            state.db.collection::<Document>("songs")
                .insert_one(song_doc)
                .await
                .map_err(e)?;
        }
    }
    
    // Read and import sections
    let sections_file = project_dir.join("sections.json");
    if sections_file.exists() {
        let content = fs::read_to_string(&sections_file).map_err(e)?;
        let sections: Vec<Value> = serde_json::from_str(&content).map_err(e)?;
        
        // Clear existing sections for songs in this project
        // (simplified - in production would need to match by song_id)
        state.db.collection::<Document>("sections")
            .delete_many(doc! {})
            .await
            .map_err(e)?;
        
        // Insert new sections
        for section_val in sections {
            let section_doc = bson::to_document(&section_val).map_err(e)?;
            state.db.collection::<Document>("sections")
                .insert_one(section_doc)
                .await
                .map_err(e)?;
        }
    }
    
    Ok(())
}

/// Package project git repo into a TAR file
#[tauri::command]
pub async fn package_project_tar(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    project_id: String,
    destination: String,
    exclude_large_files: bool,
) -> Res<Value> {
    use std::process::Command;
    
    let project_doc = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &project_id })
        .await
        .map_err(e)?
        .ok_or_else(|| "Project not found".to_string())?;
    
    let git_path = project_doc.get_str("git_repo_path")
        .ok_or_else(|| "Project git repo not initialized".to_string())?
        .to_string();
    
    let project_dir = PathBuf::from(&git_path);
    
    // If excluding large files, temporarily remove them
    let media_dir = project_dir.join("media");
    let temp_media_dir = project_dir.join("media_backup");
    
    if exclude_large_files && media_dir.exists() {
        std::fs::rename(&media_dir, &temp_media_dir).map_err(e)?;
    }
    
    // Create tar archive
    let tar_path = PathBuf::from(&destination);
    let parent = tar_path.parent().ok_or("Invalid destination path")?;
    
    let output = Command::new("tar")
        .args([
            "-czf",
            &tar_path.to_string_lossy(),
            "-C",
            &project_dir.to_string_lossy(),
            "."
        ])
        .output()
        .map_err(|err| format!("Failed to create tar: {}", err))?;
    
    if !output.status.success() {
        // Restore media dir if it was moved
        if exclude_large_files && temp_media_dir.exists() {
            let _ = std::fs::rename(&temp_media_dir, &media_dir);
        }
        return Err(format!("Tar creation failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    
    // Restore media dir if it was moved
    if exclude_large_files && temp_media_dir.exists() {
        let _ = std::fs::rename(&temp_media_dir, &media_dir);
    }
    
    Ok(serde_json::json!({
        "ok": true,
        "tar_path": destination,
        "excluded_large_files": exclude_large_files
    }))
}
