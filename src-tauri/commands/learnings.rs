//! JSON-file learnings store — deliberately NOT MongoDB.
//!
//! The app is moving toward pure-JSON persistence (so it can run on mobile, where the mongod
//! sidecar can't). "Learnings" — what the app observes about the user's taste and each project's
//! working preferences — are the first data to live purely as JSON files, in clear, hand-readable
//! structures:
//!
//!   GLOBAL (per user):   <config>/studio-lightkid/learnings/user-learnings.json
//!   PER PROJECT:         <project_folder>/learnings.json   (falls back under the global folder
//!                        keyed by project id when a project has no folder yet)
//!
//! Both files share the same envelope so a future migration can treat them uniformly:
//!   { "version": 1, "updated_at": "<rfc3339>", "data": { … } }
//!
//! `data` is free-form so the shape can grow without a schema change: taste tags, liked/disliked
//! styles, kept-image notes, channel leanings, etc. Callers merge into `data` via `record_*`.

use crate::state::AppState;
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use tauri::State;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

/// The global learnings directory, created if missing: <config>/studio-lightkid/learnings.
fn global_dir() -> Res<PathBuf> {
    let base = dirs::config_dir().ok_or("Could not locate the config directory")?;
    let dir = base.join("studio-lightkid").join("learnings");
    std::fs::create_dir_all(&dir).map_err(e)?;
    Ok(dir)
}

fn now() -> String { chrono::Utc::now().to_rfc3339() }

/// Read a learnings file, returning its `data` object (empty object if the file is missing/corrupt
/// — learnings are best-effort and must never hard-fail a caller).
fn read_data(path: &PathBuf) -> Value {
    match std::fs::read_to_string(path) {
        Ok(txt) => serde_json::from_str::<Value>(&txt)
            .ok()
            .and_then(|v| v.get("data").cloned())
            .unwrap_or_else(|| json!({})),
        Err(_) => json!({}),
    }
}

/// Write the envelope { version, updated_at, data } atomically-ish (write + rename).
fn write_data(path: &PathBuf, data: Value) -> Res<()> {
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent).map_err(e)?; }
    let envelope = json!({ "version": 1, "updated_at": now(), "data": data });
    let pretty = serde_json::to_string_pretty(&envelope).map_err(e)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, pretty).map_err(e)?;
    std::fs::rename(&tmp, path).map_err(e)?;
    Ok(())
}

/// Deep-ish merge: object keys merge recursively; arrays and scalars from `patch` replace. Small
/// helper so `record_*` can accumulate without clobbering sibling keys.
fn merge(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, pv) in p {
                merge(b.entry(k.clone()).or_insert(Value::Null), pv);
            }
        }
        (b, p) => *b = p.clone(),
    }
}

fn global_path() -> Res<PathBuf> { Ok(global_dir()?.join("user-learnings.json")) }

/// A project's learnings.json — inside its own folder when it has one, else a global fallback
/// keyed by id (so learnings survive even before a project folder exists).
async fn project_path(state: &AppState, project_id: &str) -> Res<PathBuf> {
    if project_id.trim().is_empty() { return Err("project_id is required".into()); }
    let folder = state.db.collection::<bson::Document>("projects")
        .find_one(bson::doc! { "id": project_id }).await.ok().flatten()
        .and_then(|d| d.get_str("project_folder").ok().map(|s| s.to_string()))
        .filter(|s| !s.is_empty());
    match folder {
        Some(f) => Ok(PathBuf::from(f).join("learnings.json")),
        None => Ok(global_dir()?.join("projects").join(format!("{project_id}.json"))),
    }
}

// ── Commands ─────────────────────────────────────────────────────────────────

/// The global (per-user) learnings `data` object.
#[tauri::command]
pub async fn get_user_learnings() -> Res<Value> {
    Ok(read_data(&global_path()?))
}

/// Merge `patch` into the global learnings and stamp it.
#[tauri::command]
pub async fn update_user_learnings(patch: Value) -> Res<Value> {
    let path = global_path()?;
    let mut data = read_data(&path);
    merge(&mut data, &patch);
    write_data(&path, data.clone())?;
    Ok(data)
}

/// A project's learnings `data` object.
#[tauri::command]
pub async fn get_project_learnings(state: State<'_, AppState>, project_id: String) -> Res<Value> {
    Ok(read_data(&project_path(&state, &project_id).await?))
}

/// Merge `patch` into a project's learnings.
#[tauri::command]
pub async fn update_project_learnings(state: State<'_, AppState>, project_id: String, patch: Value) -> Res<Value> {
    let path = project_path(&state, &project_id).await?;
    let mut data = read_data(&path);
    merge(&mut data, &patch);
    write_data(&path, data.clone())?;
    Ok(data)
}

/// Record a discrete "signal" (an observed preference) by appending to a capped list under
/// `data.signals.<kind>`, and bump a tally under `data.tally.<kind>.<key>`. This is the raw
/// material the taste profile is built from — e.g. kind="kept_image", key=a style tag.
///
/// `scope` = "user" | "project". Kept lists are capped so the JSON stays small and readable.
#[tauri::command]
pub async fn record_learning_signal(
    state: State<'_, AppState>,
    scope: String,
    project_id: Option<String>,
    kind: String,
    key: String,
    detail: Option<Value>,
) -> Res<Value> {
    let path = if scope == "project" {
        project_path(&state, project_id.as_deref().unwrap_or("")).await?
    } else {
        global_path()?
    };
    let mut data = read_data(&path);
    let obj = data.as_object_mut().ok_or("learnings root is not an object")?;

    // Append to signals.<kind> (cap 200 newest).
    let signals = obj.entry("signals").or_insert_with(|| json!({}));
    let list = signals.as_object_mut().unwrap()
        .entry(kind.clone()).or_insert_with(|| json!([]));
    if let Some(arr) = list.as_array_mut() {
        arr.push(json!({ "key": key, "detail": detail, "at": now() }));
        let overflow = arr.len().saturating_sub(200);
        if overflow > 0 { arr.drain(0..overflow); }
    }

    // Bump tally.<kind>.<key>.
    let tally = obj.entry("tally").or_insert_with(|| json!({}));
    let per_kind = tally.as_object_mut().unwrap()
        .entry(kind.clone()).or_insert_with(|| json!({}));
    if let Some(m) = per_kind.as_object_mut() {
        let n = m.get(&key).and_then(|v| v.as_i64()).unwrap_or(0) + 1;
        m.insert(key.clone(), json!(n));
    }

    write_data(&path, data.clone())?;
    Ok(data)
}

/// The recorded signals of one kind, newest first, merged across the user and project stores.
///
/// Exposed for callers that need the raw history rather than the tally summary — the guided flow
/// reads `guided_choice` this way to recommend what this user actually picked last time, which is a
/// question about recency, not frequency.
pub async fn recent_signals(state: &AppState, project_id: &str, kind: &str, limit: usize) -> Vec<Value> {
    let mut all: Vec<Value> = Vec::new();
    let user = global_path().ok().map(|p| read_data(&p)).unwrap_or_else(|| json!({}));
    let proj = if project_id.trim().is_empty() { json!({}) } else {
        match project_path(state, project_id).await { Ok(p) => read_data(&p), Err(_) => json!({}) }
    };
    for data in [&user, &proj] {
        if let Some(arr) = data.pointer(&format!("/signals/{kind}")).and_then(|v| v.as_array()) {
            all.extend(arr.iter().cloned());
        }
    }
    // `at` is an RFC3339 timestamp, so a plain string sort is chronological.
    all.sort_by(|a, b| b["at"].as_str().unwrap_or("").cmp(a["at"].as_str().unwrap_or("")));
    all.truncate(limit);
    all
}

/// A compact prompt-ready summary of the strongest learnings, for injection into generation
/// prompts (e.g. "The user tends to prefer: warm lighting (7), acoustic (5), …"). Combines user +
/// project tallies, taking the top few keys per kind. Empty string when there's nothing learned.
pub async fn learnings_prompt_block(state: &AppState, project_id: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let user = global_path().ok().map(|p| read_data(&p)).unwrap_or_else(|| json!({}));
    let proj = if project_id.trim().is_empty() { json!({}) } else {
        match project_path(state, project_id).await { Ok(p) => read_data(&p), Err(_) => json!({}) }
    };
    for (label, data) in [("Across projects", &user), ("This project", &proj)] {
        if let Some(tally) = data.get("tally").and_then(|t| t.as_object()) {
            for (kind, keys) in tally {
                if let Some(km) = keys.as_object() {
                    let mut pairs: Vec<(&String, i64)> = km.iter()
                        .map(|(k, v)| (k, v.as_i64().unwrap_or(0))).collect();
                    pairs.sort_by(|a, b| b.1.cmp(&a.1));
                    let top: Vec<String> = pairs.iter().take(6).map(|(k, _)| (*k).clone()).collect();
                    if !top.is_empty() {
                        lines.push(format!("- {label} / {kind}: {}", top.join(", ")));
                    }
                }
            }
        }
        // Free-form declared preferences take precedence — the user said it explicitly.
        if let Some(prefs) = data.get("preferences").and_then(|p| p.as_str()).filter(|s| !s.trim().is_empty()) {
            lines.push(format!("- {label} (stated): {}", prefs.trim()));
        }
    }
    if lines.is_empty() { return String::new(); }
    format!("LEARNED USER TASTE (bias choices toward these unless the current request overrides them):\n{}\n", lines.join("\n"))
}

/// Where the learnings live on disk — surfaced so the UI can show/open the folder.
#[tauri::command]
pub async fn learnings_locations(state: State<'_, AppState>, project_id: Option<String>) -> Res<Value> {
    let global = global_path()?.to_string_lossy().to_string();
    let project = match project_id.filter(|s| !s.is_empty()) {
        Some(pid) => project_path(&state, &pid).await.ok().map(|p| p.to_string_lossy().to_string()),
        None => None,
    };
    Ok(json!({ "global": global, "project": project }))
}
