//! On-demand style/genre SAMPLE generation.
//!
//! Not a bundled library — the user generates a small example for a given art style or music genre
//! using THEIR currently-configured engine, so they can see/hear what a style (or a genre mix)
//! actually looks or sounds like before committing. Samples are saved in the global app folder:
//!
//!   <config>/studio-lightkid/style-samples/{image,music}/<slug>.{png,mp3}
//!   <config>/studio-lightkid/style-samples/manifest.json   (key -> { kind,label,path,created_at })
//!
//! The style pickers (Sound Studio, Style Studio) read the manifest and show the sample next to
//! each style's name whenever one exists.

use crate::state::AppState;
use bson::{doc, Document};
use serde_json::{json, Value};
use std::path::PathBuf;
use tauri::State;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

fn bson_to_value(d: Document) -> Value {
    let mut m = serde_json::Map::new();
    for (k, v) in d { if k == "_id" { continue; } if let Ok(jv) = bson::from_bson::<Value>(v) { m.insert(k, jv); } }
    Value::Object(m)
}

fn base_dir() -> Res<PathBuf> {
    let base = crate::paths::config_dir().ok_or("Could not locate the config directory")?;
    let dir = base.join("studio-lightkid").join("style-samples");
    std::fs::create_dir_all(&dir).map_err(e)?;
    Ok(dir)
}

fn manifest_path() -> Res<PathBuf> { Ok(base_dir()?.join("manifest.json")) }

fn read_manifest() -> Value {
    manifest_path().ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .unwrap_or_else(|| json!({}))
}

fn slugify(s: &str) -> String {
    let out: String = s.chars().map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    out.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-").chars().take(64).collect()
}

/// The saved samples as a { "<kind>:<key>": entry } map. Entries carry the absolute file `path`;
/// the frontend turns it into a loadable src via Tauri's convertFileSrc.
#[tauri::command]
pub async fn list_style_samples() -> Res<Value> {
    // Prune entries whose file was deleted so the UI never shows a broken sample.
    let mut m = read_manifest();
    if let Some(obj) = m.as_object_mut() {
        obj.retain(|_, v| v["path"].as_str().map(|p| std::path::Path::new(p).exists()).unwrap_or(false));
    }
    Ok(m)
}

/// Generate one sample for a style/genre with the current engine and cache it.
/// `kind` = "image" | "music". `spec` is the actual style/genre descriptor sent to the engine;
/// `key`/`label` identify it in the manifest and UI.
#[tauri::command]
pub async fn generate_style_sample(
    state: State<'_, AppState>,
    kind: String,
    key: String,
    label: String,
    spec: String,
) -> Res<Value> {
    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let cancelled = state.cancelled_jobs.clone();

    let url = if kind == "music" {
        crate::jobs::sample_music(&spec, &settings, &state.db, &cancelled).await
            .ok_or_else(|| "Music sample failed — is the music engine connected and running?".to_string())?
    } else {
        crate::jobs::sample_image(&spec, &settings, &state.db, &cancelled).await
            .ok_or_else(|| "Image sample failed — is the image engine (ComfyUI) connected and running?".to_string())?
    };

    // Download the produced media and save it under the global samples folder.
    let bytes = reqwest::Client::new().get(&url).timeout(std::time::Duration::from_secs(60))
        .send().await.map_err(e)?.bytes().await.map_err(e)?;
    let ext = if kind == "music" { "mp3" } else { "png" };
    let dir = base_dir()?.join(if kind == "music" { "music" } else { "image" });
    std::fs::create_dir_all(&dir).map_err(e)?;
    let file = dir.join(format!("{}.{ext}", slugify(&key)));
    std::fs::write(&file, &bytes).map_err(e)?;

    let entry = json!({
        "kind": kind, "key": key, "label": label,
        "path": file.to_string_lossy(), "created_at": chrono::Utc::now().to_rfc3339(),
    });
    let mut manifest = read_manifest();
    manifest.as_object_mut().ok_or("manifest not an object")?
        .insert(format!("{kind}:{key}"), entry.clone());
    std::fs::write(manifest_path()?, serde_json::to_string_pretty(&manifest).map_err(e)?).map_err(e)?;
    Ok(entry)
}

/// Delete a saved sample (file + manifest entry).
#[tauri::command]
pub async fn delete_style_sample(kind: String, key: String) -> Res<Value> {
    let mut manifest = read_manifest();
    if let Some(obj) = manifest.as_object_mut() {
        if let Some(entry) = obj.remove(&format!("{kind}:{key}")) {
            if let Some(p) = entry["path"].as_str() { let _ = std::fs::remove_file(p); }
            std::fs::write(manifest_path()?, serde_json::to_string_pretty(&manifest).map_err(e)?).map_err(e)?;
        }
    }
    Ok(json!({ "ok": true }))
}
