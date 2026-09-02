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
use serde_json::{json, Value};
use std::path::PathBuf;
use tauri::State;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

/// The global learnings directory, created if missing: <config>/studio-lightkid/learnings.
fn global_dir() -> Res<PathBuf> {
    let base = crate::paths::config_dir().ok_or("Could not locate the config directory")?;
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

/// Forget learnings — a whole store, one kind, or one key within a kind.
///
/// This exists because `update_*_learnings` cannot express deletion: `merge` is a deep merge, so a
/// patch can only add or overwrite. An empty object merges to nothing and a `null` writes a literal
/// null, which would leave the key present and the tally readable. Anything the app is going to feed
/// back into a generation prompt (see `learnings_prompt_block`) needs a real way out, not an
/// approximation of one — a personalisation store you cannot empty is one the user cannot refuse.
///
/// `scope` = "user" | "project". `kind` omitted clears the store; `key` omitted clears the kind.
#[tauri::command]
pub async fn forget_learnings(
    state: State<'_, AppState>,
    scope: String,
    project_id: Option<String>,
    kind: Option<String>,
    key: Option<String>,
) -> Res<Value> {
    let path = if scope == "project" {
        project_path(&state, project_id.as_deref().unwrap_or("")).await?
    } else {
        global_path()?
    };

    let Some(kind) = kind.filter(|k| !k.trim().is_empty()) else {
        // No kind: the whole store goes, envelope and all.
        write_data(&path, json!({}))?;
        return Ok(json!({}));
    };

    let mut data = read_data(&path);
    forget_in(&mut data, &kind, key.as_deref())?;
    write_data(&path, data.clone())?;
    Ok(data)
}

/// The removal itself, on the `data` object — pure, so the rules can be tested without a store.
///
/// `key` empty means the whole kind.
pub(crate) fn forget_in(data: &mut Value, kind: &str, key: Option<&str>) -> Res<()> {
    let obj = data.as_object_mut().ok_or("learnings root is not an object")?;

    // `preferences` is not a tally. It is the one thing the user stated in words, it lives at the
    // root rather than under `tally`/`signals`, and `learnings_prompt_block` gives it the last word
    // over everything counted — so withdrawing it must not require clearing the whole store, which
    // is what withdrawing it used to cost (this branch simply did not exist, so asking to forget it
    // silently did nothing at all).
    if kind == "preferences" {
        obj.remove("preferences");
        return Ok(());
    }

    match key.filter(|k| !k.trim().is_empty()) {
        // One key: drop its tally entry and every signal that named it.
        Some(key) => {
            if let Some(per_kind) = obj.get_mut("tally").and_then(|t| t.get_mut(kind)).and_then(|v| v.as_object_mut()) {
                per_kind.remove(key);
            }
            if let Some(arr) = obj.get_mut("signals").and_then(|s| s.get_mut(kind)).and_then(|v| v.as_array_mut()) {
                arr.retain(|sig| sig["key"].as_str() != Some(key));
            }
        }
        // The whole kind.
        None => {
            if let Some(m) = obj.get_mut("tally").and_then(|v| v.as_object_mut()) { m.remove(kind); }
            if let Some(m) = obj.get_mut("signals").and_then(|v| v.as_object_mut()) { m.remove(kind); }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Value {
        json!({
            "preferences": "warmer, less literal, no crowds",
            "tally": {
                "kept_image": { "gold leaf": 4, "neon": 1 },
                "genre_pick": { "liquid dnb": 3 },
            },
            "signals": {
                "kept_image": [ { "key": "gold leaf" }, { "key": "neon" } ],
                "genre_pick": [ { "key": "liquid dnb" } ],
            },
        })
    }

    #[test]
    fn a_stated_preference_can_be_withdrawn_on_its_own() {
        let mut d = store();
        forget_in(&mut d, "preferences", None).unwrap();
        assert!(d.get("preferences").is_none(), "the stated preference survived");
        // …and nothing else went with it: withdrawing what you said is not clearing the store.
        assert_eq!(d["tally"]["kept_image"]["gold leaf"], 4);
        assert_eq!(d["signals"]["genre_pick"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn merging_cannot_express_this_which_is_why_it_is_a_removal() {
        // `merge` is a deep merge, so an empty patch adds nothing and `""` would leave the key in
        // place — and `learnings_prompt_block` reads the key, not its emptiness.
        let mut d = store();
        merge(&mut d, &json!({ "preferences": "" }));
        assert_eq!(d["preferences"], "");
        assert!(d.get("preferences").is_some());
        forget_in(&mut d, "preferences", None).unwrap();
        assert!(d.get("preferences").is_none());
    }

    #[test]
    fn one_key_leaves_its_siblings_and_the_other_kinds_alone() {
        let mut d = store();
        forget_in(&mut d, "kept_image", Some("neon")).unwrap();
        assert!(d["tally"]["kept_image"].get("neon").is_none());
        assert_eq!(d["tally"]["kept_image"]["gold leaf"], 4);
        let sigs = d["signals"]["kept_image"].as_array().unwrap();
        assert_eq!(sigs.len(), 1);
        assert_eq!(sigs[0]["key"], "gold leaf");
        assert_eq!(d["tally"]["genre_pick"]["liquid dnb"], 3);
    }

    #[test]
    fn a_whole_kind_takes_its_tally_and_its_signals() {
        let mut d = store();
        forget_in(&mut d, "kept_image", None).unwrap();
        assert!(d["tally"].get("kept_image").is_none());
        assert!(d["signals"].get("kept_image").is_none());
        assert_eq!(d["tally"]["genre_pick"]["liquid dnb"], 3);
        // The words the user typed are not a tally and are not swept up with one.
        assert_eq!(d["preferences"], "warmer, less literal, no crowds");
    }

    #[test]
    fn forgetting_something_that_was_never_there_is_not_an_error() {
        let mut d = json!({});
        forget_in(&mut d, "kept_image", Some("gold leaf")).unwrap();
        forget_in(&mut d, "preferences", None).unwrap();
        assert_eq!(d, json!({}));
    }
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
