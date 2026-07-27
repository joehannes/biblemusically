use crate::state::AppState;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use tauri::State;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

// ────────────────────────────────────────────────────────────────
// Autosave, as actual commits
//
// The rule the app now follows, and the reason for each half:
//
//   • Leaving a field STAGES it. Every blur, every control change, writes the value into that feature's
//     JSON file inside the project's git repo and runs `git add` on just that file. Staging is cheap and
//     reversible, and it means the value is already on disk before anything else can go wrong.
//   • Changing view COMMITS what is staged. That is the moment a person believes their work is safe, so
//     it is the moment the history gets a real entry — with a message naming the view and the fields
//     that changed, because a log of "autosave" fifty times over is not a history.
//
// Two details that matter:
//
//   The commit here is deliberately NOT `git add -A`. It commits the index, so a half-finished file the
//   user never touched through the app does not get swept into an autosave commit alongside their edit.
//   `project_sync::commit_all` still exists for the sweeper, which does want everything.
//
//   An autosave with nothing staged is a no-op, not an empty commit. Navigating between views without
//   editing anything must leave the history untouched, or the log fills with noise and `git log` stops
//   being usable for the thing it is for.
// ────────────────────────────────────────────────────────────────

/// Where a feature's field state lives inside the project repo.
///
/// One file per feature rather than one per field: a commit then shows "composer.json changed" and the
/// diff shows which values, which is what a person reading history wants.
const FIELDS_DIR: &str = "data/fields";

/// A file name that cannot escape the fields directory.
///
/// Feature names come from the frontend, so `../../.git/config` is a name this must never turn into a
/// path. Anything but letters, digits, dash and underscore is collapsed.
pub fn safe_feature(name: &str) -> String {
    let cleaned: String = name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() { "misc".to_string() } else { trimmed.chars().take(60).collect() }
}

/// Set a possibly-dotted field path inside an object, creating the intermediate objects.
///
/// `brief.mood` is one field from the GUI's point of view and two levels in the file, and a flat
/// `"brief.mood"` key would make the JSON useless to everything else that reads it.
pub fn merge_field(root: &mut Value, path: &str, value: Value) {
    if !root.is_object() { *root = Value::Object(Map::new()); }
    let parts: Vec<&str> = path.split('.').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() { return; }
    let mut cur = root;
    for part in &parts[..parts.len() - 1] {
        let obj = cur.as_object_mut().expect("checked above");
        cur = obj.entry(part.to_string())
            .and_modify(|v| if !v.is_object() { *v = Value::Object(Map::new()); })
            .or_insert_with(|| Value::Object(Map::new()));
    }
    if let Some(obj) = cur.as_object_mut() {
        obj.insert(parts[parts.len() - 1].to_string(), value);
    }
}

/// The commit message for an autosave.
///
/// Names the view and the fields, because that is what makes the log readable months later. Long lists
/// are truncated rather than wrapped — a subject line is a subject line.
pub fn commit_message(view: &str, fields: &[String], reason: &str) -> String {
    let view = if view.trim().is_empty() { "app" } else { view.trim() };
    let head = match reason {
        "manual" => format!("save({view})"),
        "switch" => format!("autosave({view}): before switching project"),
        _ => format!("autosave({view})"),
    };
    if fields.is_empty() { return head; }
    let shown: Vec<&str> = fields.iter().take(4).map(|f| f.as_str()).collect();
    let rest = fields.len().saturating_sub(shown.len());
    if rest > 0 {
        format!("{head}: {} and {rest} more", shown.join(", "))
    } else {
        format!("{head}: {}", shown.join(", "))
    }
}

async fn folder(state: &AppState, project_id: &str) -> Res<PathBuf> {
    crate::project_sync::project_folder(&state.db, project_id).await
        .ok_or_else(|| "This project has no folder on disk yet — save it once to choose one.".to_string())
}

/// Read the current file for a feature, or an empty object.
fn read_feature(dir: &Path, feature: &str) -> Value {
    let path = dir.join(format!("{feature}.json"));
    std::fs::read_to_string(path).ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| Value::Object(Map::new()))
}

#[derive(serde::Deserialize)]
pub struct StageRequest {
    pub project_id: String,
    /// Which part of the app this value belongs to: "composer", "brief", "video"…
    pub feature: String,
    /// Field name, dotted for nesting.
    pub field: String,
    pub value: Value,
}

/// Write one field and stage it. Called on blur and on any control change that alters a saved value.
#[tauri::command]
pub async fn stage_field(state: State<'_, AppState>, payload: StageRequest) -> Res<Value> {
    let dir_root = folder(&state, &payload.project_id).await?;
    crate::project_sync::ensure_repo(&dir_root)?;
    let dir = dir_root.join(FIELDS_DIR);
    std::fs::create_dir_all(&dir).map_err(e)?;

    let feature = safe_feature(&payload.feature);
    let mut doc = read_feature(&dir, &feature);
    merge_field(&mut doc, &payload.field, payload.value.clone());
    // The updated_at goes in the file rather than only in the commit, so a run reading the JSON knows
    // how fresh it is without asking git.
    merge_field(&mut doc, "_updated_at", json!(crate::models::now_iso()));

    let rel = format!("{FIELDS_DIR}/{feature}.json");
    let path = dir_root.join(&rel);
    std::fs::write(&path, serde_json::to_string_pretty(&doc).map_err(e)?).map_err(e)?;

    // Staged, not committed. That is the whole point: the value is safe on disk and in the index, and
    // the commit happens when the user leaves the view.
    let staged = crate::project_sync::git(&dir_root, &["add", "--", &rel]).is_ok();
    Ok(json!({ "staged": staged, "file": rel, "field": payload.field }))
}

/// What is staged right now, as file names.
async fn staged_files(dir: &Path) -> Vec<String> {
    crate::project_sync::git(dir, &["diff", "--cached", "--name-only"])
        .map(|out| out.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}

#[derive(serde::Deserialize)]
pub struct CommitRequest {
    pub project_id: String,
    /// The view being left — goes in the message.
    #[serde(default)]
    pub view: Option<String>,
    /// The fields touched, for the message.
    #[serde(default)]
    pub fields: Vec<String>,
    /// "autosave" | "manual" | "switch"
    #[serde(default)]
    pub reason: Option<String>,
}

/// Commit what is staged. A no-op when nothing is.
#[tauri::command]
pub async fn autosave_commit(state: State<'_, AppState>, payload: CommitRequest) -> Res<Value> {
    let dir = folder(&state, &payload.project_id).await?;
    if !crate::project_sync::git_available() || !dir.join(".git").exists() {
        return Ok(json!({ "committed": false, "reason": "this project is not a git repository" }));
    }
    let files = staged_files(&dir).await;
    if files.is_empty() {
        // Navigating without editing must leave the history alone.
        return Ok(json!({ "committed": false, "reason": "nothing staged" }));
    }
    let reason = payload.reason.unwrap_or_else(|| "autosave".into());
    let message = commit_message(payload.view.as_deref().unwrap_or(""), &payload.fields, &reason);
    // No `-a`: only the index, so an unrelated working-tree change is not swept in.
    crate::project_sync::git(&dir, &["commit", "-m", &message])?;
    let hash = crate::project_sync::git(&dir, &["rev-parse", "--short", "HEAD"])
        .unwrap_or_default().trim().to_string();
    Ok(json!({ "committed": true, "commit": hash, "message": message, "files": files }))
}

/// Is there anything to save? Drives the save button's dot.
#[tauri::command]
pub async fn autosave_status(state: State<'_, AppState>, project_id: String) -> Res<Value> {
    let Ok(dir) = folder(&state, &project_id).await else {
        return Ok(json!({ "repo": false, "staged": [], "dirty": false }));
    };
    if !crate::project_sync::git_available() || !dir.join(".git").exists() {
        return Ok(json!({ "repo": false, "staged": [], "dirty": false }));
    }
    let staged = staged_files(&dir).await;
    let dirty = crate::project_sync::git(&dir, &["status", "--porcelain"])
        .map(|s| !s.trim().is_empty()).unwrap_or(false);
    let branch = crate::project_sync::git(&dir, &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_default().trim().to_string();
    let has_remote = crate::project_sync::git(&dir, &["remote"])
        .map(|s| !s.trim().is_empty()).unwrap_or(false);
    Ok(json!({
        "repo": true, "staged": staged, "dirty": dirty,
        "branch": branch, "has_remote": has_remote,
    }))
}

/// Commit everything pending and push, when a remote is configured.
///
/// Separate from the plain save because pushing is a different promise: it puts the work somewhere the
/// user does not control, so it is an explicit choice in the save menu rather than a side effect.
#[tauri::command]
pub async fn save_and_push(state: State<'_, AppState>, project_id: String, view: Option<String>) -> Res<Value> {
    let dir = folder(&state, &project_id).await?;
    crate::project_sync::ensure_repo(&dir)?;

    // Everything, not just the index: a manual save is the user asking for all of it.
    let committed = crate::project_sync::commit_all(
        &dir,
        &commit_message(view.as_deref().unwrap_or(""), &[], "manual"),
    )?;

    // Only the *push* half is gated, and only after the commit has happened. Committing locally is
    // how this app saves at all: refusing it would mean a trial that loses work rather than a trial
    // that cannot send work elsewhere.
    if let Err(reason) = super::subscription::require(&state, "remote_sync").await {
        return Ok(json!({ "committed": committed, "pushed": false, "reason": reason }));
    }

    let has_remote = crate::project_sync::git(&dir, &["remote"])
        .map(|s| !s.trim().is_empty()).unwrap_or(false);
    if !has_remote {
        return Ok(json!({
            "committed": committed,
            "pushed": false,
            "reason": "No git remote is configured for this project. Set one up in Data & Sync, then \
                       Save & push will send it there.",
        }));
    }
    // The sync path already knows how to reach the remote with a token that never touches
    // .git/config — reusing it means there is one place to be right about credentials.
    let synced = crate::commands::remote_sync::sync_project_now(state, project_id).await?;
    Ok(json!({ "committed": committed, "pushed": true, "sync": synced }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_feature_name_cannot_escape_the_fields_directory() {
        // Feature names come from the frontend. This one must never become a path.
        assert_eq!(safe_feature("../../.git/config"), "git-config");
        assert_eq!(safe_feature("composer"), "composer");
        assert_eq!(safe_feature("Brief/Mood"), "Brief-Mood");
        assert_eq!(safe_feature(""), "misc");
        assert_eq!(safe_feature("///"), "misc");
        assert!(!safe_feature("a/b/c").contains('/'));
    }

    #[test]
    fn a_dotted_field_nests_instead_of_becoming_a_flat_key() {
        let mut doc = json!({});
        merge_field(&mut doc, "brief.mood", json!("reverent"));
        merge_field(&mut doc, "brief.tempo", json!(92));
        merge_field(&mut doc, "title", json!("Genesis 1"));
        assert_eq!(doc["brief"]["mood"], json!("reverent"));
        assert_eq!(doc["brief"]["tempo"], json!(92));
        assert_eq!(doc["title"], json!("Genesis 1"));
        assert!(doc.get("brief.mood").is_none(), "a flat key would be useless to every other reader");
    }

    #[test]
    fn merging_into_a_non_object_replaces_it_rather_than_failing() {
        // The file on disk may hold anything, including a value where an object is now needed.
        let mut doc = json!({ "brief": "just a string" });
        merge_field(&mut doc, "brief.mood", json!("warm"));
        assert_eq!(doc["brief"]["mood"], json!("warm"));

        let mut not_an_object = json!(42);
        merge_field(&mut not_an_object, "a", json!(1));
        assert_eq!(not_an_object["a"], json!(1));
    }

    #[test]
    fn the_message_names_the_view_and_the_fields() {
        let fields = vec!["title".to_string(), "styles".to_string()];
        let m = commit_message("composer", &fields, "autosave");
        assert_eq!(m, "autosave(composer): title, styles");
        // A log of "autosave" fifty times over is not a history, so the fields have to be in there.
        assert!(commit_message("composer", &[], "manual").starts_with("save(composer)"));
        assert!(commit_message("video", &[], "switch").contains("before switching project"));
    }

    #[test]
    fn a_long_field_list_is_truncated_not_wrapped() {
        let many: Vec<String> = (1..=9).map(|i| format!("field{i}")).collect();
        let m = commit_message("composer", &many, "autosave");
        assert!(m.contains("and 5 more"), "{m}");
        assert!(!m.contains('\n'), "a subject line stays one line");
        assert!(m.len() < 90, "got {} chars: {m}", m.len());
    }

    #[test]
    fn an_empty_view_still_produces_a_usable_message() {
        assert_eq!(commit_message("", &[], "autosave"), "autosave(app)");
        assert_eq!(commit_message("   ", &["x".into()], "autosave"), "autosave(app): x");
    }
}
