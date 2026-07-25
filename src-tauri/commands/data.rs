//! Data-transparency commands: where the app's JSON lives, what's in it, and the state of the
//! one-time carry-over from the retired MongoDB sidecar.
//!
//! The point of the JSON store is that a user can *see* their data. These commands back the
//! Settings → Data panel: the folder paths (global + per project), per-collection document counts,
//! the migration report, and the two explicit actions — re-run the import, or delete the legacy
//! database directory once its contents are safely in JSON.

use crate::state::AppState;
use crate::{mongo_import, project_sync};
use serde_json::{json, Value};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

type Res<T> = Result<T, String>;

fn app_data_dir(app: &AppHandle) -> Res<PathBuf> {
    app.path().app_data_dir().map_err(|err| err.to_string())
}

/// Everything the Data panel needs in one call.
#[tauri::command]
pub async fn store_info(state: State<'_, AppState>, app: AppHandle) -> Res<Value> {
    let locations = state.db.locations().await;
    let counts = state.db.counts().await.map_err(|err| err.to_string())?;
    let report = std::fs::read_to_string(state.db.global_root().join("migration-report.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok());

    let legacy = match app_data_dir(&app) {
        Ok(dir) => {
            let legacy_dir = mongo_import::legacy_db_dir(&dir);
            let present = mongo_import::has_legacy_data(&legacy_dir);
            json!({
                "present": present,
                "migrated": mongo_import::already_migrated(&legacy_dir),
                "path": legacy_dir.to_string_lossy(),
                "size_bytes": if present { mongo_import::dir_size(&legacy_dir) } else { 0 },
                "mongod_available": mongo_import::find_mongod().map(|p| p.to_string_lossy().to_string()),
            })
        }
        Err(err) => json!({ "present": false, "error": err }),
    };

    Ok(json!({
        "engine": "json-files",
        "locations": locations,
        "counts": counts,
        "migration_report": report,
        "legacy_database": legacy,
        "git": {
            "available": project_sync::git_available(),
            "lfs_available": project_sync::lfs_available(),
        },
    }))
}

/// Re-run the legacy import by hand (Settings → Data → "Import legacy database").
///
/// Safe to press repeatedly: the importer skips documents whose `_id`/`id` already exists.
#[tauri::command]
pub async fn run_legacy_migration(state: State<'_, AppState>, app: AppHandle, force: Option<bool>) -> Res<Value> {
    let dir = app_data_dir(&app)?;
    let legacy_dir = mongo_import::legacy_db_dir(&dir);
    if !mongo_import::has_legacy_data(&legacy_dir) {
        return Ok(json!({ "status": "nothing_to_migrate", "path": legacy_dir.to_string_lossy() }));
    }
    #[cfg(desktop)]
    {
        if force.unwrap_or(false) || !mongo_import::already_migrated(&legacy_dir) {
            let report = mongo_import::import(&state.db, &legacy_dir).await?;
            state.db.invalidate_cache().await;
            return Ok(report);
        }
        Ok(mongo_import::import_if_needed(&state.db, &dir).await)
    }
    #[cfg(not(desktop))]
    {
        let _ = force;
        Ok(json!({ "status": "not_applicable" }))
    }
}

/// Delete the legacy MongoDB directory. Only allowed after a successful import — the importer
/// leaves a marker file, and this refuses to run without it.
#[tauri::command]
pub async fn purge_legacy_data(app: AppHandle) -> Res<Value> {
    let legacy_dir = mongo_import::legacy_db_dir(&app_data_dir(&app)?);
    let freed = mongo_import::purge(&legacy_dir)?;
    Ok(json!({ "ok": true, "freed_bytes": freed, "path": legacy_dir.to_string_lossy() }))
}

/// Commit a project's JSON (and any new media) to its own git repo right now, instead of waiting
/// for the background sweep.
#[tauri::command]
pub async fn commit_project_data(
    state: State<'_, AppState>,
    project_id: String,
    message: Option<String>,
) -> Res<Value> {
    match project_sync::commit_project(&state.db, &project_id, message).await? {
        Some(hash) => Ok(json!({ "ok": true, "commit": hash })),
        None => Ok(json!({ "ok": true, "commit": null, "message": "No changes to commit" })),
    }
}

/// Commit every project with pending changes (the same sweep the timer runs).
#[tauri::command]
pub async fn commit_all_project_data(state: State<'_, AppState>) -> Res<Value> {
    Ok(json!({ "ok": true, "committed": project_sync::sweep(&state.db).await }))
}

/// Open a folder in the desktop file manager, so "your data is here" is one click from being true.
#[tauri::command]
pub async fn reveal_in_file_manager(path: String) -> Res<Value> {
    open::that(&path).map_err(|err| format!("could not open {path}: {err}"))?;
    Ok(json!({ "ok": true }))
}
