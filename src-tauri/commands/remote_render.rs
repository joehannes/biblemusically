use crate::state::AppState;
use bson::{doc, Document};
use serde_json::{json, Value};
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
// Remote rendering
//
// Composing a 5–10 minute video and pushing it to YouTube is CPU and bandwidth the user's own machine
// should not have to spend — at 50 channels it is ~750 videos, 50–100 core-hours and 150–300 GB of
// egress a month (the numbers are worked out in docs/REMOTE_RENDER.md). So the work is described as a
// job and handed to somebody else's computer.
//
// One job spec, one worker script (scripts/remote/render_worker.py), four launchers:
//
//   kaggle   free   a CPU notebook push; 12 h sessions, 5 concurrent, and the app already drives Kaggle
//   actions  free*  workflow_dispatch on the project's own repo; unmetered on a public repo
//   modal    paid   POST to a Modal web endpoint; inside its $30/month free credits at this volume
//   http     any    POST to any worker you host (a Hetzner box, a Pi, a colleague's machine)
//
// The spec carries URLs, never files: the worker fetches assets from the project's sync remote, so
// media travels host-to-host and the user's connection only carries a few kilobytes of JSON.
// ────────────────────────────────────────────────────────────────

const WORKER_SRC: &str = include_str!("../../scripts/remote/render_worker.py");

/// The providers, with the facts a user needs to choose between them. Surfaced in Settings.
#[tauri::command]
pub async fn list_render_providers() -> Res<Value> {
    Ok(json!({ "providers": [
        {
            "id": "local", "label": "This machine", "cost": "free",
            "detail": "Renders with the local ffmpeg and uploads over your own connection.",
            "needs": [],
        },
        {
            "id": "kaggle", "label": "Kaggle CPU session", "cost": "free",
            "detail": "12 h CPU sessions, 5 at a time, no GPU quota used. Needs the kaggle CLI and a token — the same one the music engines use.",
            "needs": ["kaggle CLI", "kaggle token", "project synced to a remote"],
        },
        {
            "id": "actions", "label": "GitHub Actions", "cost": "free on a public repo",
            "detail": "Unmetered minutes on public repos; 2,000 min/month on a private one (Free plan). Runs on the project's own repo.",
            "needs": ["project repo on GitHub", "github_token with workflow scope"],
        },
        {
            "id": "modal", "label": "Modal", "cost": "$30/month credits, then ~$0.05/core-hour",
            "detail": "Serverless containers, no idle cost. Deploy scripts/remote/modal_app.py once, then paste its web endpoint.",
            "needs": ["modal_endpoint", "modal_token (optional)"],
        },
        {
            "id": "http", "label": "Your own worker", "cost": "your hosting",
            "detail": "POSTs the job to any HTTP worker you run — a $4.59 VPS handles this volume with room to spare.",
            "needs": ["render_worker_url"],
        },
    ]}))
}

// ── Asset URL resolution ─────────────────────────────────────────

/// Turn a local asset reference into something a remote worker can fetch.
///
/// A worker on Kaggle cannot open `/home/you/Music/song.mp3`, and a job that silently renders without
/// its audio is worse than a job that refuses to start. So anything not already a public URL is
/// mapped onto the project's sync remote, and if that mapping is impossible the caller is told which
/// asset is unreachable and what to do about it.
fn public_asset_url(raw: &str, project_folder: Option<&std::path::Path>, remote: &RemoteRepo) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() { return Err("asset has no path".into()); }
    if raw.starts_with("http://") || raw.starts_with("https://") {
        let local_host = raw.contains("127.0.0.1") || raw.contains("localhost") || raw.contains("0.0.0.0");
        if !local_host { return Ok(raw.to_string()); }
        // A loopback URL is this app's own media server, which no remote worker can reach.
        return Err(format!(
            "'{raw}' is served by this app on localhost, so a remote worker cannot fetch it. \
             Sync the project so the asset exists in the remote repo, or render locally."
        ));
    }

    let path = std::path::Path::new(raw.trim_start_matches("file://"));
    let folder = project_folder.ok_or_else(|| {
        format!("'{raw}' is a local file and this project has no folder, so a remote worker cannot fetch it")
    })?;
    let rel = path.strip_prefix(folder).map_err(|_| {
        format!("'{raw}' lives outside the project folder, so it is not part of what gets synced to the remote")
    })?;
    remote.raw_url(&rel.to_string_lossy().replace('\\', "/"))
}

/// Where a project's files can be read from over HTTP, derived from its sync config.
struct RemoteRepo {
    provider: String,
    repo: String,      // "owner/name"
    branch: String,
}

impl RemoteRepo {
    fn raw_url(&self, rel: &str) -> Result<String, String> {
        let enc = rel.split('/').map(urlencoding::encode).collect::<Vec<_>>().join("/");
        match self.provider.as_str() {
            "huggingface" => Ok(format!("https://huggingface.co/datasets/{}/resolve/{}/{}", self.repo, self.branch, enc)),
            "github" => Ok(format!("https://raw.githubusercontent.com/{}/{}/{}", self.repo, self.branch, enc)),
            "gitlab" => Ok(format!("https://gitlab.com/{}/-/raw/{}/{}", self.repo, self.branch, enc)),
            "codeberg" => Ok(format!("https://codeberg.org/{}/raw/branch/{}/{}", self.repo, self.branch, enc)),
            other => Err(format!(
                "Sync provider '{other}' has no known raw-file URL, so a remote worker cannot fetch this project's assets. \
                 Sync to Hugging Face, GitHub, GitLab or Codeberg, or use the local renderer."
            )),
        }
    }
}

async fn remote_repo(state: &AppState, project_id: &str) -> Res<RemoteRepo> {
    let cfg = state.db.collection::<Document>("sync_configs")
        .find_one(doc! { "_id": project_id }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let repo = cfg["repo"].as_str().unwrap_or("").trim().to_string();
    if repo.is_empty() {
        return Err("This project is not synced to a remote yet. Set that up in Data & Sync — the remote is where the render worker reads the audio and images from.".into());
    }
    Ok(RemoteRepo {
        provider: cfg["provider"].as_str().unwrap_or("huggingface").to_string(),
        repo,
        branch: cfg["branch"].as_str().filter(|s| !s.is_empty()).unwrap_or("main").to_string(),
    })
}

// ── Job spec ─────────────────────────────────────────────────────

#[derive(serde::Deserialize, Default)]
pub struct RenderOptions {
    /// "local" | "kaggle" | "actions" | "modal" | "http" — falls back to the saved setting.
    #[serde(default)]
    pub provider: Option<String>,
    /// "hold" | "private" | "public" — "hold" renders without uploading.
    #[serde(default)]
    pub publish: Option<String>,
    /// Crossfade name, or "none" for hard cuts.
    #[serde(default)]
    pub transition: Option<String>,
}

/// Build the spec for one song: assets as fetchable URLs, encode settings, upload credentials.
///
/// Public so the UI can show exactly what would be sent (and why a song is not ready) without
/// launching anything.
#[tauri::command]
pub async fn build_render_spec(state: State<'_, AppState>, song_id: String, options: Option<RenderOptions>) -> Res<Value> {
    build_spec(&state, &song_id, options.unwrap_or_default()).await
}

async fn build_spec(state: &AppState, song_id: &str, opts: RenderOptions) -> Res<Value> {
    let song_id = song_id.to_string();
    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();

    let song = state.db.collection::<Document>("songs")
        .find_one(doc! { "id": &song_id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "song not found".to_string())?;
    let project_id = song["project_id"].as_str().unwrap_or("").to_string();

    let folder = crate::project_sync::project_folder(&state.db, &project_id).await;
    let remote = remote_repo(state, &project_id).await?;

    // Audio.
    let audio_raw = song["audio_url"].as_str().unwrap_or("");
    if audio_raw.trim().is_empty() {
        return Err("This song has no audio yet — generate the music before rendering a video.".into());
    }
    let audio_url = public_asset_url(audio_raw, folder.as_deref(), &remote)?;

    // Sections with images, in order. Their own duration drives the slideshow timing; anything
    // missing one shares out the remaining audio time on the worker side.
    use futures_util::StreamExt;
    let mut cursor = state.db.collection::<Document>("sections")
        .find(doc! { "song_id": &song_id }).await.map_err(e)?;
    let mut sections: Vec<Value> = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { sections.push(bson_to_value(d)); }
    sections.sort_by_key(|s| s["index"].as_i64().unwrap_or(0));

    let mut images = Vec::new();
    let mut unreachable: Vec<String> = Vec::new();
    for s in &sections {
        let Some(img) = s["image_url"].as_str().filter(|u| !u.trim().is_empty()) else { continue };
        match public_asset_url(img, folder.as_deref(), &remote) {
            Ok(url) => images.push(json!({
                "url": url,
                "duration": s["duration"].as_f64().unwrap_or(0.0),
                "section": s["name"].as_str().unwrap_or(""),
            })),
            Err(err) => unreachable.push(err),
        }
    }
    if images.is_empty() {
        return Err(if unreachable.is_empty() {
            "This song has no section images yet — run the analysis and image steps first.".to_string()
        } else {
            format!("None of this song's images can be fetched remotely:\n- {}", unreachable.join("\n- "))
        });
    }

    // Upload target: the channel's stored refresh token plus its OAuth client.
    let publish = opts.publish.clone().unwrap_or_else(|| "hold".to_string());
    let mut youtube = json!({ "upload": false });
    if publish != "hold" {
        let channel_id = song["channel_id"].as_str().unwrap_or("");
        let channel = state.db.collection::<Document>("channels")
            .find_one(doc! { "id": channel_id }).await.map_err(e)?
            .map(bson_to_value)
            .ok_or_else(|| "This song has no connected channel, so there is nowhere to upload it.".to_string())?;
        let refresh = channel["refresh_token"].as_str().unwrap_or("").trim().to_string();
        if refresh.is_empty() {
            return Err(format!(
                "Channel '{}' has no YouTube refresh token — authorize it in Channel Manager first.",
                channel["name"].as_str().unwrap_or(channel_id)
            ));
        }
        let client = crate::jobs::pick_oauth_client(&state.db, &channel, None).await
            .ok_or_else(|| "No OAuth client configured for this channel.".to_string())?;
        youtube = json!({
            "upload": true,
            "title": song["title"].as_str().unwrap_or("Untitled"),
            "description": song["description"].as_str().unwrap_or(""),
            "tags": song["tags"].clone(),
            "privacy": if publish == "public" { "public" } else { "private" },
            "category_id": "10",
            "client_id": client["client_id"].as_str().unwrap_or(""),
            "client_secret": client["client_secret"].as_str().unwrap_or(""),
            "refresh_token": refresh,
            // Idempotency: a retried job must not publish a second video for this song.
            "existing_video_id": song["youtube_video_id"].as_str(),
        });
    }

    let transition = opts.transition.clone().unwrap_or_else(|| {
        if settings["video_transitions_enabled"].as_bool().unwrap_or(false) {
            settings["video_transition"].as_str().unwrap_or("fade").to_string()
        } else { "none".to_string() }
    });
    let trans_dur = if transition == "none" { 0.0 } else {
        settings["video_transition_dur"].as_f64().unwrap_or(0.7).clamp(0.1, 3.0)
    };

    // The worker needs a token for private remotes; it is read from the vault, not from settings.
    let assets_token = crate::vault::get_quiet(&format!("remote.{}.token", remote.provider));

    Ok(json!({
        "job_id": uuid::Uuid::new_v4().to_string(),
        "song_id": song_id,
        "project_id": project_id,
        "created_at": crate::models::now_iso(),
        "provider": opts.provider.clone().unwrap_or_else(||
            settings["remote_render_provider"].as_str().unwrap_or("kaggle").to_string()),
        "video": {
            "width": settings["video_width"].as_i64().unwrap_or(1920),
            "height": settings["video_height"].as_i64().unwrap_or(1080),
            "fps": settings["video_fps"].as_i64().unwrap_or(30),
            "crf": settings["video_crf"].as_i64().unwrap_or(21),
            "preset": settings["video_preset"].as_str().unwrap_or("veryfast"),
            "audio_bitrate": "192k",
        },
        "audio": { "url": audio_url },
        "images": images,
        "transition": { "name": transition, "duration": trans_dur },
        "subtitles": Value::Null,
        "youtube": youtube,
        "assets_token": assets_token,
        "warnings": unreachable,
    }))
}

// ── Launchers ────────────────────────────────────────────────────

/// Kaggle: push a private CPU kernel that carries the worker and the job inline.
///
/// Per-job pushes rather than a long-lived poller, because that is what Kaggle's model rewards — a
/// batch kernel run is exactly one unit of work, and five can run at once.
async fn launch_kaggle(spec: &Value, settings: &Value) -> Res<Value> {
    use tokio::fs;
    let kaggle = crate::commands::settings::locate_kaggle();
    // Kernel ids are namespaced by account, and the active account is the one whose token the CLI
    // is currently using.
    let username = settings["kaggle_active"].as_str()
        .or_else(|| settings["kaggle_username"].as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "No Kaggle account is active. Add your token in Settings → Kaggle first.".to_string())?
        .to_string();

    let job_id = spec["job_id"].as_str().unwrap_or("job");
    let slug = format!("bm-render-{}", &job_id[..8.min(job_id.len())]);
    let dir = std::env::temp_dir().join(format!("bm-render-{slug}"));
    let _ = fs::remove_dir_all(&dir).await;
    fs::create_dir_all(&dir).await.map_err(e)?;

    // The job travels base64-encoded inside the script: one file, no dataset to attach, and the
    // credentials never appear in a readable diff of a private kernel.
    let payload = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(spec).map_err(e)?)
    };
    let script = format!(
        "# Auto-generated by Bible Musically — renders one song and uploads it.\n\
         # Runs on a Kaggle CPU session: no GPU quota is consumed.\n\
         import base64, os, sys\n\
         os.environ['BM_JOB_B64'] = {payload:?}\n\
         WORKER = base64.b64decode({worker:?}).decode('utf-8')\n\
         open('/kaggle/working/render_worker.py', 'w').write(WORKER)\n\
         sys.argv = ['render_worker.py']\n\
         exec(compile(WORKER, 'render_worker.py', 'exec'), {{'__name__': '__main__'}})\n",
        payload = payload,
        worker = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(WORKER_SRC.as_bytes())
        },
    );
    fs::write(dir.join("render.py"), script).await.map_err(e)?;
    fs::write(dir.join("kernel-metadata.json"), serde_json::to_string_pretty(&json!({
        "id": format!("{username}/{slug}"),
        "title": format!("BM render {}", &job_id[..8.min(job_id.len())]),
        "code_file": "render.py",
        "language": "python",
        "kernel_type": "script",
        "is_private": true,
        "enable_gpu": false,
        "enable_internet": true,
        "dataset_sources": [],
        "competition_sources": [],
        "kernel_sources": [],
    })).map_err(e)?).await.map_err(e)?;

    let out = tokio::process::Command::new(&kaggle)
        .args(["kernels", "push", "-p"]).arg(&dir)
        .output().await
        .map_err(|err| format!("Could not run the kaggle CLI: {err}. Install it with `pipx install kaggle`."))?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if !text.to_lowercase().contains("successfully pushed") {
        return Err(format!("Kaggle refused the render job: {}", text.trim()));
    }
    Ok(json!({
        "provider": "kaggle",
        "kernel": format!("{username}/{slug}"),
        "monitor": format!("https://www.kaggle.com/code/{username}/{slug}"),
        "detail": "Kaggle is running the render on a CPU session. It uploads to YouTube itself when it finishes.",
    }))
}

/// GitHub Actions: dispatch the workflow in the project's own repo.
async fn launch_actions(state: &AppState, spec: &Value, project_id: &str) -> Res<Value> {
    let cfg = state.db.collection::<Document>("sync_configs")
        .find_one(doc! { "_id": project_id }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    if cfg["provider"].as_str() != Some("github") {
        return Err("GitHub Actions needs this project synced to GitHub. Switch the sync provider in Data & Sync, or pick another render provider.".into());
    }
    let repo = cfg["repo"].as_str().unwrap_or("").to_string();
    let token = crate::vault::get_quiet("remote.github.token")
        .ok_or_else(|| "No GitHub token stored. Add one in Data & Sync (it needs the `workflow` scope to dispatch runs).".to_string())?;
    let branch = cfg["branch"].as_str().filter(|s| !s.is_empty()).unwrap_or("main").to_string();

    let payload = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(spec).map_err(e)?)
    };
    let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(45)).build().map_err(e)?;
    let url = format!("https://api.github.com/repos/{repo}/actions/workflows/bm-render.yml/dispatches");
    let r = http.post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "BibleMusically")
        .json(&json!({ "ref": branch, "inputs": { "job": payload } }))
        .send().await.map_err(e)?;
    if !r.status().is_success() {
        let status = r.status();
        let body = r.text().await.unwrap_or_default();
        let hint = if status.as_u16() == 404 {
            "\n\nThe workflow file is probably missing — run `write_render_workflow` (Settings → Remote rendering → \"Install the workflow\") and push the project once."
        } else { "" };
        return Err(format!("GitHub refused the dispatch (HTTP {status}): {}{hint}", body.chars().take(300).collect::<String>()));
    }
    Ok(json!({
        "provider": "actions",
        "monitor": format!("https://github.com/{repo}/actions/workflows/bm-render.yml"),
        "detail": "GitHub Actions is running the render. Watch it on the Actions tab.",
    }))
}

/// Modal, or any HTTP worker: POST the job and let it answer when it is done.
async fn launch_http(spec: &Value, endpoint: &str, token: Option<String>) -> Res<Value> {
    if endpoint.trim().is_empty() {
        return Err("No worker endpoint configured. Set it in Settings → Remote rendering.".into());
    }
    let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(60)).build().map_err(e)?;
    let mut req = http.post(endpoint).json(spec);
    if let Some(t) = token.filter(|t| !t.trim().is_empty()) {
        req = req.header("Authorization", format!("Bearer {t}"));
    }
    let r = req.send().await.map_err(|err| format!("Could not reach the render worker at {endpoint}: {err}"))?;
    let status = r.status();
    let body = r.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("The render worker refused the job (HTTP {status}): {}", body.chars().take(300).collect::<String>()));
    }
    Ok(json!({
        "provider": "http",
        "endpoint": endpoint,
        "response": serde_json::from_str::<Value>(&body).unwrap_or(Value::String(body)),
        "detail": "The worker accepted the job.",
    }))
}

/// Submit one song for remote rendering. Records the job so the UI can follow it.
#[tauri::command]
pub async fn submit_remote_render(state: State<'_, AppState>, song_id: String, options: Option<RenderOptions>) -> Res<Value> {
    let spec = build_spec(&state, &song_id, options.unwrap_or_default()).await?;
    let provider = spec["provider"].as_str().unwrap_or("kaggle").to_string();
    let project_id = spec["project_id"].as_str().unwrap_or("").to_string();
    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();

    let launched = match provider.as_str() {
        "kaggle" => launch_kaggle(&spec, &settings).await,
        "actions" => launch_actions(&state, &spec, &project_id).await,
        "modal" => launch_http(&spec, settings["modal_endpoint"].as_str().unwrap_or(""),
                               crate::vault::get_quiet("modal_token")).await,
        "http" => launch_http(&spec, settings["render_worker_url"].as_str().unwrap_or(""),
                              crate::vault::get_quiet("render_worker_token")).await,
        "local" => Err("Provider is 'local' — use the normal video job for that.".to_string()),
        other => Err(format!("Unknown render provider '{other}'.")),
    };

    // Record the attempt either way: a failed launch is exactly what the user needs to see.
    let job_id = spec["job_id"].as_str().unwrap_or("").to_string();
    let record = json!({
        "id": job_id,
        "song_id": song_id,
        "project_id": project_id,
        "provider": provider,
        "status": if launched.is_ok() { "running" } else { "failed" },
        "created_at": crate::models::now_iso(),
        "images": spec["images"].as_array().map(|a| a.len()).unwrap_or(0),
        "uploads": spec["youtube"]["upload"].as_bool().unwrap_or(false),
        "launch": match &launched { Ok(v) => v.clone(), Err(err) => json!({ "error": err }) },
    });
    if let Ok(doc) = bson::to_document(&record) {
        let _ = state.db.collection::<Document>("render_jobs").insert_one(doc).await;
    }
    let mut out = launched?;
    if let Some(obj) = out.as_object_mut() {
        obj.insert("ok".into(), json!(true));
        obj.insert("job_id".into(), json!(job_id));
    }
    Ok(out)
}

/// Jobs submitted so far, newest first.
#[tauri::command]
pub async fn list_render_jobs(state: State<'_, AppState>, limit: Option<i64>) -> Res<Vec<Value>> {
    use futures_util::StreamExt;
    let mut cursor = state.db.collection::<Document>("render_jobs")
        .find(doc! {}).sort(doc! { "created_at": -1 }).limit(limit.unwrap_or(25))
        .await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }
    Ok(out)
}

/// Record a worker's result. Called by the app when it reads a `BM_RESULT` line, and reachable by a
/// worker that was given a callback URL.
#[tauri::command]
pub async fn record_render_result(state: State<'_, AppState>, result: Value) -> Res<Value> {
    let job_id = result["job_id"].as_str().unwrap_or("").to_string();
    let song_id = result["song_id"].as_str().unwrap_or("").to_string();
    let status = result["status"].as_str().unwrap_or("unknown").to_string();

    if !job_id.is_empty() {
        let _ = state.db.collection::<Document>("render_jobs").update_one(
            doc! { "id": &job_id },
            doc! { "$set": {
                "status": &status,
                "finished_at": crate::models::now_iso(),
                "result": bson::to_bson(&result).unwrap_or(bson::Bson::Null),
            } },
        ).await;
    }

    // A finished upload is the song's publish state — write it where the rest of the app looks.
    if !song_id.is_empty() {
        if let Some(vid) = result["video_id"].as_str().filter(|v| !v.is_empty()) {
            let _ = state.db.collection::<Document>("songs").update_one(
                doc! { "id": &song_id },
                doc! { "$set": {
                    "youtube_video_id": vid,
                    "status": "published",
                    "video_url": format!("https://www.youtube.com/watch?v={vid}"),
                } },
            ).await;
        } else if status == "rendered" {
            let _ = state.db.collection::<Document>("songs").update_one(
                doc! { "id": &song_id },
                doc! { "$set": { "status": "video_ready" } },
            ).await;
        }
    }
    Ok(json!({ "ok": true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn hf() -> RemoteRepo {
        RemoteRepo { provider: "huggingface".into(), repo: "me/proj".into(), branch: "main".into() }
    }

    #[test]
    fn maps_project_files_onto_the_sync_remote() {
        let folder = Path::new("/home/me/Music/proj");
        let url = public_asset_url("/home/me/Music/proj/assets/img 01.png", Some(folder), &hf()).unwrap();
        // Spaces have to survive as %20 or the worker's fetch 404s.
        assert_eq!(url, "https://huggingface.co/datasets/me/proj/resolve/main/assets/img%2001.png");
    }

    #[test]
    fn passes_through_public_urls() {
        let url = public_asset_url("https://cdn.example.com/a.png", None, &hf()).unwrap();
        assert_eq!(url, "https://cdn.example.com/a.png");
    }

    #[test]
    fn refuses_loopback_urls() {
        // The app's own media server is invisible to a remote worker; failing loudly here is the
        // whole point — a render that silently drops its audio is worse.
        let err = public_asset_url("http://127.0.0.1:1420/api/media/audio/x.mp3", None, &hf()).unwrap_err();
        assert!(err.contains("localhost"), "{err}");
    }

    #[test]
    fn refuses_files_outside_the_project() {
        let folder = Path::new("/home/me/Music/proj");
        let err = public_asset_url("/tmp/scratch/x.png", Some(folder), &hf()).unwrap_err();
        assert!(err.contains("outside the project folder"), "{err}");
    }

    #[test]
    fn knows_each_providers_raw_url_shape() {
        let cases = [
            ("github", "https://raw.githubusercontent.com/me/proj/main/a.png"),
            ("gitlab", "https://gitlab.com/me/proj/-/raw/main/a.png"),
            ("codeberg", "https://codeberg.org/me/proj/raw/branch/main/a.png"),
        ];
        for (provider, expected) in cases {
            let repo = RemoteRepo { provider: provider.into(), repo: "me/proj".into(), branch: "main".into() };
            assert_eq!(repo.raw_url("a.png").unwrap(), expected, "{provider}");
        }
        let unknown = RemoteRepo { provider: "dropbox".into(), repo: "me/proj".into(), branch: "main".into() };
        assert!(unknown.raw_url("a.png").is_err(), "an unknown host must not guess a URL");
    }
}

/// Write the GitHub Actions workflow into the project repo, so `submit_remote_render` can dispatch it.
#[tauri::command]
pub async fn write_render_workflow(state: State<'_, AppState>, project_id: String) -> Res<Value> {
    let folder = crate::project_sync::project_folder(&state.db, &project_id).await
        .ok_or_else(|| "This project has no folder yet.".to_string())?;
    let dir = folder.join(".github").join("workflows");
    tokio::fs::create_dir_all(&dir).await.map_err(e)?;
    let workflow = dir.join("bm-render.yml");
    tokio::fs::write(&workflow, include_str!("../../scripts/remote/bm-render.yml")).await.map_err(e)?;
    // The worker travels with the repo so the runner needs nothing but python and ffmpeg.
    let worker_dir = folder.join(".bm");
    tokio::fs::create_dir_all(&worker_dir).await.map_err(e)?;
    tokio::fs::write(worker_dir.join("render_worker.py"), WORKER_SRC).await.map_err(e)?;
    Ok(json!({
        "ok": true,
        "workflow": workflow.to_string_lossy(),
        "next_step": "Sync the project (Data & Sync → Sync now) so GitHub has the workflow, then submit a render.",
    }))
}
