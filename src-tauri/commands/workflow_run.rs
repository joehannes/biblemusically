use crate::state::AppState;
use bson::{doc, Document};
use serde_json::{json, Value};
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

// ────────────────────────────────────────────────────────────────
// Workflow runs, on the backend
//
// The pipeline used to be sequenced by the /workflow page: a loop in a React component. That works
// until the moment it matters — switching project unmounts the component, and the sequence dies with
// it. Individual jobs survived (they are backend jobs), but nothing was left to start the next stage,
// so a run abandoned half way looked finished.
//
// So the run is a document now, and a backend tick advances it:
//
//   { project_id, steps[], cursor, status, dispatched[], log[] }
//
// Three consequences, all of them the point:
//
//   • Switching project does not stop the previous project's run. Nothing about the run lives in the
//     GUI, so there is nothing for a navigation to destroy. The view observes a run; it is not the run.
//   • A crash or a reload resumes. The cursor is on disk, and the tick picks it up again.
//   • Eligibility is read from the STORE, never from component state. A run asks "which songs in this
//     project have audio but no analysis" of the saved data, so it cannot be confused by whatever a
//     half-edited form happens to be showing — and cannot be affected by a form belonging to a
//     different project entirely.
//
// The tick is deliberately dumb: dispatch a step, wait for its jobs to finish, advance. Everything that
// does real work is an existing job kind, so this adds sequencing and nothing else.
// ────────────────────────────────────────────────────────────────

/// The pipeline, in order. Each id is either a job kind to enqueue or a named backend action.
pub const ALL_STEPS: &[&str] = &["lyrics", "music", "analysis", "images", "overlays", "video", "upload"];

/// The steps a run should perform.
///
/// `stop_after` is inclusive. Upload is excluded unless asked for, because publishing is the one step
/// that is public and irreversible — a "run everything" that quietly uploaded would be a trap.
pub fn step_order(stop_after: &str, include_upload: bool) -> Vec<String> {
    let mut out = Vec::new();
    for step in ALL_STEPS {
        if *step == "upload" && !include_upload { continue; }
        out.push(step.to_string());
        if *step == stop_after { break; }
    }
    out
}

/// Which songs a step still has work for.
///
/// These rules were closures inside the /workflow page. They belong here: they decide what a background
/// run does, and they are the kind of condition that is quietly wrong until something is skipped for a
/// week. `songs` are store documents.
pub fn eligible_songs(step: &str, songs: &[Value]) -> Vec<String> {
    let id = |s: &Value| s["id"].as_str().unwrap_or("").to_string();
    let has_audio = |s: &Value| {
        !s["audio_url"].as_str().unwrap_or("").is_empty()
            || !s["local_audio_path"].as_str().unwrap_or("").is_empty()
    };
    songs.iter().filter(|s| {
        let status = s["status"].as_str().unwrap_or("");
        match step {
            "music" => !has_audio(s),
            // Already analysed, or already past analysis — a video-ready song must not be re-analysed.
            "analysis" => has_audio(s) && status != "analyzed" && status != "video_ready",
            "overlays" => has_audio(s) && s["overlay_local_path"].as_str().unwrap_or("").is_empty(),
            "video" => !s["overlay_local_path"].as_str().unwrap_or("").is_empty() && status != "video_ready",
            _ => false,
        }
    })
    .map(id)
    .filter(|i| !i.is_empty())
    .collect()
}

/// What the tick should do with a step, given the jobs it dispatched.
///
/// Returns `("wait" | "advance", finished, failed)`. A failed job does not stall the run: the next step
/// simply has less to do, which is better than a run that stops overnight because one image of forty
/// failed. `stop_on_error` is honoured by the caller, which has the run document.
pub fn dispatch_verdict(job_states: &[String]) -> (&'static str, usize, usize) {
    let finished = job_states.iter().filter(|s| *s == "done").count();
    let failed = job_states.iter().filter(|s| *s == "failed" || *s == "cancelled").count();
    let outstanding = job_states.len() - finished - failed;
    if outstanding > 0 { ("wait", finished, failed) } else { ("advance", finished, failed) }
}

// ── Commands ────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct StartRequest {
    pub project_id: String,
    /// Last step to run, inclusive. Defaults to "video" — never upload by accident.
    #[serde(default)]
    pub stop_after: Option<String>,
    #[serde(default)]
    pub include_upload: Option<bool>,
    /// Halt the run when a step's jobs all fail. Default: keep going.
    #[serde(default)]
    pub stop_on_error: Option<bool>,
}

/// Begin a run. One per project — starting again replaces the previous run's plan.
#[tauri::command]
pub async fn start_workflow_run(state: State<'_, AppState>, payload: StartRequest) -> Res<Value> {
    let stop_after = payload.stop_after.unwrap_or_else(|| "video".into());
    let include_upload = payload.include_upload.unwrap_or(false);
    let steps = step_order(&stop_after, include_upload);
    if steps.is_empty() {
        return Err(format!("'{stop_after}' is not a step in the pipeline"));
    }
    let run = json!({
        "_id": payload.project_id.clone(),        // one run per project, by construction
        "id": uuid::Uuid::new_v4().to_string(),
        "project_id": payload.project_id,
        "steps": steps,
        "cursor": 0,
        "status": "running",
        "stop_on_error": payload.stop_on_error.unwrap_or(false),
        "dispatched": Value::Null,                // set when the current step is dispatched
        "log": [],
        "started_at": crate::models::now_iso(),
        "updated_at": crate::models::now_iso(),
    });
    let d = bson::to_document(&run).map_err(e)?;
    state.db.collection::<Document>("workflow_runs")
        .update_one(doc! { "_id": run["project_id"].as_str().unwrap_or("") }, doc! { "$set": d })
        .upsert(true).await.map_err(e)?;
    Ok(run)
}

#[tauri::command]
pub async fn workflow_run_status(state: State<'_, AppState>, project_id: Option<String>) -> Res<Value> {
    use futures_util::StreamExt;
    let mut runs = Vec::new();
    let mut cursor = state.db.collection::<Document>("workflow_runs").find(doc! {}).await.map_err(e)?;
    while let Some(Ok(d)) = cursor.next().await { runs.push(bson_to_value(d)); }
    // Every run comes back, not just this project's: the whole point is that another project's run is
    // still going, and the user should be able to see that from wherever they are.
    let mine = project_id.as_deref().and_then(|p| {
        runs.iter().find(|r| r["project_id"].as_str() == Some(p)).cloned()
    });
    let others: Vec<Value> = runs.iter()
        .filter(|r| project_id.as_deref().map_or(true, |p| r["project_id"].as_str() != Some(p)))
        .filter(|r| r["status"].as_str() == Some("running"))
        .cloned().collect();
    Ok(json!({ "run": mine, "running_elsewhere": others, "all": runs }))
}

#[tauri::command]
pub async fn set_workflow_run_status(state: State<'_, AppState>, project_id: String, status: String) -> Res<Value> {
    let allowed = ["running", "paused", "cancelled"];
    if !allowed.contains(&status.as_str()) {
        return Err(format!("status must be one of {}", allowed.join(", ")));
    }
    state.db.collection::<Document>("workflow_runs")
        .update_one(doc! { "_id": &project_id },
                    doc! { "$set": { "status": &status, "updated_at": crate::models::now_iso() } })
        .await.map_err(e)?;
    Ok(json!({ "project_id": project_id, "status": status }))
}

// ── The tick ────────────────────────────────────────────────────────────────

/// Advance every running workflow. Called from the same loop as the scheduler tick.
pub async fn tick(state: &Arc<AppState>) {
    use futures_util::StreamExt;
    let mut runs = Vec::new();
    if let Ok(mut cursor) = state.db.collection::<Document>("workflow_runs").find(doc! {}).await {
        while let Some(Ok(d)) = cursor.next().await { runs.push(bson_to_value(d)); }
    }
    for run in runs {
        if run["status"].as_str() != Some("running") { continue; }
        if let Err(err) = advance_one(state, &run).await {
            let project_id = run["project_id"].as_str().unwrap_or("").to_string();
            log_and_set(state, &project_id, &format!("step failed: {err}"), None).await;
        }
    }
}

async fn advance_one(state: &Arc<AppState>, run: &Value) -> Res<()> {
    let project_id = run["project_id"].as_str().unwrap_or("").to_string();
    let steps: Vec<String> = run["steps"].as_array().map(|a| a.iter()
        .filter_map(|s| s.as_str().map(|s| s.to_string())).collect()).unwrap_or_default();
    let cursor = run["cursor"].as_i64().unwrap_or(0).max(0) as usize;

    if cursor >= steps.len() {
        set_fields(state, &project_id, json!({ "status": "done" })).await;
        return Ok(());
    }
    let step = steps[cursor].clone();

    // Already dispatched: wait for its jobs, then move on.
    if let Some(ids) = run["dispatched"].as_array() {
        let job_ids: Vec<String> = ids.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
        let mut states = Vec::new();
        for id in &job_ids {
            let s = state.db.collection::<Document>("jobs")
                .find_one(doc! { "id": id }).await.ok().flatten()
                .and_then(|d| d.get_str("status").ok().map(|s| s.to_string()))
                // A job document that vanished counts as finished rather than blocking forever.
                .unwrap_or_else(|| "done".to_string());
            states.push(s);
        }
        let (verdict, finished, failed) = dispatch_verdict(&states);
        if verdict == "wait" { return Ok(()); }

        let note = format!("{step}: {finished} done, {failed} failed");
        if failed > 0 && finished == 0 && run["stop_on_error"].as_bool() == Some(true) && !job_ids.is_empty() {
            log_and_set(state, &project_id, &format!("{note} — stopping"), Some("failed")).await;
            return Ok(());
        }
        log_and_set(state, &project_id, &note, None).await;
        set_fields(state, &project_id, json!({
            "cursor": (cursor + 1) as i64,
            "dispatched": Value::Null,
        })).await;
        return Ok(());
    }

    // Not dispatched yet: do the step.
    let dispatched = dispatch_step(state, &project_id, &step).await?;
    log_and_set(state, &project_id, &format!("{step}: queued {}", dispatched.len()), None).await;
    set_fields(state, &project_id, json!({ "dispatched": dispatched })).await;
    Ok(())
}

/// Perform one step, returning the job ids to wait on.
///
/// Everything reads the store rather than anything the GUI holds, which is what makes a background run
/// safe while the user works on another project.
async fn dispatch_step(state: &Arc<AppState>, project_id: &str, step: &str) -> Res<Vec<String>> {
    use futures_util::StreamExt;

    // The project's songs, from disk.
    let mut songs = Vec::new();
    if let Ok(mut cursor) = state.db.collection::<Document>("songs")
        .find(doc! { "project_id": project_id }).await {
        while let Some(Ok(d)) = cursor.next().await { songs.push(bson_to_value(d)); }
    }

    let mut ids = Vec::new();
    match step {
        "lyrics" => {
            // Synchronous and already backend-side: it generates the next scheduled chapter's song.
            crate::commands::scheduler::run_scheduled_generation(state, project_id).await?;
        }
        "images" => {
            // One image job per section that has no image yet, across the project's songs.
            let song_ids: Vec<String> = songs.iter()
                .filter_map(|s| s["id"].as_str().map(|i| i.to_string())).collect();
            for song_id in song_ids {
                if let Ok(mut cursor) = state.db.collection::<Document>("sections")
                    .find(doc! { "song_id": &song_id }).await {
                    while let Some(Ok(d)) = cursor.next().await {
                        let sec = bson_to_value(d);
                        if !sec["image_url"].as_str().unwrap_or("").is_empty() { continue; }
                        if sec["image_prompt"].as_str().unwrap_or("").trim().is_empty() { continue; }
                        if let Some(sid) = sec["id"].as_str() {
                            if let Ok(job) = crate::jobs::enqueue("image", sid, state).await {
                                ids.push(job.id);
                            }
                        }
                    }
                }
            }
        }
        "upload" => {
            // Only rows that already exist and are waiting. Creating rows and enriching their metadata
            // stays in the Upload view: those are decisions, not work.
            if let Ok(mut cursor) = state.db.collection::<Document>("uploads")
                .find(doc! { "status": "pending" }).await {
                while let Some(Ok(d)) = cursor.next().await {
                    let up = bson_to_value(d);
                    if let Some(uid) = up["id"].as_str() {
                        if let Ok(job) = crate::jobs::enqueue("upload", uid, state).await {
                            ids.push(job.id);
                        }
                    }
                }
            }
        }
        // music, analysis, overlays, video: one job per eligible song.
        other => {
            let kind = match other {
                "overlays" => "overlay",
                k => k,
            };
            for target in eligible_songs(other, &songs) {
                if let Ok(job) = crate::jobs::enqueue(kind, &target, state).await {
                    ids.push(job.id);
                }
            }
        }
    }
    Ok(ids)
}

async fn set_fields(state: &Arc<AppState>, project_id: &str, fields: Value) {
    let mut set = Document::new();
    if let Some(obj) = fields.as_object() {
        for (k, v) in obj {
            if let Ok(b) = bson::to_bson(v) { set.insert(k.clone(), b); }
        }
    }
    set.insert("updated_at", crate::models::now_iso());
    let _ = state.db.collection::<Document>("workflow_runs")
        .update_one(doc! { "_id": project_id }, doc! { "$set": set }).await;
}

/// Append to the run's log, and optionally change its status.
async fn log_and_set(state: &Arc<AppState>, project_id: &str, message: &str, status: Option<&str>) {
    let entry = json!({ "at": crate::models::now_iso(), "message": message });
    if let Ok(b) = bson::to_bson(&entry) {
        let _ = state.db.collection::<Document>("workflow_runs")
            .update_one(doc! { "_id": project_id }, doc! { "$push": { "log": b } }).await;
    }
    if let Some(s) = status {
        set_fields(state, project_id, json!({ "status": s })).await;
    } else {
        set_fields(state, project_id, json!({})).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_is_never_included_unless_asked_for() {
        // Publishing is public and irreversible. A "run everything" that quietly uploaded would be a
        // trap, so the default order stops before it even when asked to stop after upload.
        let default = step_order("video", false);
        assert!(!default.contains(&"upload".to_string()), "{default:?}");
        assert_eq!(default, vec!["lyrics", "music", "analysis", "images", "overlays", "video"]);

        let with = step_order("upload", true);
        assert_eq!(with.last().unwrap(), "upload");
    }

    #[test]
    fn stop_after_is_inclusive_and_cuts_the_rest() {
        assert_eq!(step_order("music", false), vec!["lyrics", "music"]);
        assert_eq!(step_order("lyrics", false), vec!["lyrics"]);
        // An unknown step yields the whole list rather than nothing — better to run than to silently
        // do nothing, and the caller rejects an empty plan.
        assert!(step_order("nonsense", false).len() >= 6);
    }

    #[test]
    fn a_song_is_not_analysed_twice() {
        let songs = vec![
            json!({ "id": "a", "audio_url": "http://x/a.mp3", "status": "draft" }),
            json!({ "id": "b", "audio_url": "http://x/b.mp3", "status": "analyzed" }),
            json!({ "id": "c", "audio_url": "http://x/c.mp3", "status": "video_ready" }),
            json!({ "id": "d", "status": "draft" }),
        ];
        assert_eq!(eligible_songs("analysis", &songs), vec!["a"]);
        // And a song already past analysis must not be dragged back through it.
        assert!(!eligible_songs("analysis", &songs).contains(&"c".to_string()));
    }

    #[test]
    fn music_is_queued_only_for_songs_with_no_audio_at_all() {
        let songs = vec![
            json!({ "id": "remote", "audio_url": "http://x/a.mp3" }),
            json!({ "id": "local", "local_audio_path": "/tmp/a.mp3" }),
            json!({ "id": "neither" }),
            json!({ "id": "empty", "audio_url": "", "local_audio_path": "" }),
        ];
        let out = eligible_songs("music", &songs);
        assert_eq!(out, vec!["neither", "empty"], "an empty string is not audio");
    }

    #[test]
    fn video_waits_for_an_overlay_and_skips_what_is_done() {
        let songs = vec![
            json!({ "id": "ready", "overlay_local_path": "/tmp/o.mp4", "status": "analyzed" }),
            json!({ "id": "done", "overlay_local_path": "/tmp/o.mp4", "status": "video_ready" }),
            json!({ "id": "no-overlay", "status": "analyzed" }),
        ];
        assert_eq!(eligible_songs("video", &songs), vec!["ready"]);
        assert_eq!(eligible_songs("overlays", &songs), Vec::<String>::new(),
                   "none of these lacks an overlay while having audio");
    }

    #[test]
    fn a_step_waits_while_any_job_is_outstanding() {
        assert_eq!(dispatch_verdict(&["done".into(), "running".into()]).0, "wait");
        assert_eq!(dispatch_verdict(&["queued".into()]).0, "wait");
        // All resolved, in any mix, means move on.
        let (verdict, finished, failed) = dispatch_verdict(&["done".into(), "failed".into(), "done".into()]);
        assert_eq!(verdict, "advance");
        assert_eq!((finished, failed), (2, 1));
    }

    #[test]
    fn a_step_with_nothing_to_do_advances_immediately() {
        // Otherwise a project whose songs all have audio would sit forever on the music step.
        let (verdict, finished, failed) = dispatch_verdict(&[]);
        assert_eq!(verdict, "advance");
        assert_eq!((finished, failed), (0, 0));
    }

    #[test]
    fn one_failure_among_many_does_not_stall_the_run() {
        // A run that stops overnight because one image of forty failed is worse than a run that
        // carries on with thirty-nine.
        let mut states: Vec<String> = (0..39).map(|_| "done".to_string()).collect();
        states.push("failed".into());
        let (verdict, finished, failed) = dispatch_verdict(&states);
        assert_eq!(verdict, "advance");
        assert_eq!(finished, 39);
        assert_eq!(failed, 1);
    }
}
