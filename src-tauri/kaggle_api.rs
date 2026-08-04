//! Talking to Kaggle over HTTP instead of through the `kaggle` command.
//!
//! # Why this exists
//!
//! Everything the app does with Kaggle currently shells out to the Python CLI: `locate_kaggle()`
//! hunts for it on PATH, and every operation spawns a subprocess. That works on a desktop and cannot
//! work on Android at all — since Android 10, an app targeting API 29 or later is forbidden by
//! SELinux from `exec()`ing anything in its own writable data directory (a W^X violation). It is why
//! Termux is pinned to targetSdk 28 and ships its binaries as APK native libraries rather than as
//! ordinary files. Shipping a Python interpreter plus the kaggle package as native libs is possible
//! and deeply unpleasant.
//!
//! It is also unnecessary, because the CLI is a thin wrapper over a public REST API. `kernels/status`
//! is a GET with two query parameters and basic auth. So the honest fix is not to make subprocesses
//! work on a phone; it is to stop needing one.
//!
//! # What this replaces, and what it adds
//!
//! Two of these endpoints do things the app currently believes are impossible:
//!
//! * **`kernels/quota`** returns real GPU hours used and allowed, with the reset time. The advisor
//!   has a `quota_minutes_left` input that nothing has ever populated — it was written expecting a
//!   number the app had no way to obtain. Now it does.
//! * **`kernels/cancel-session`** ends a running session directly. `supersede_kaggle_session` exists
//!   because "the Kaggle CLI/API has no stop-session call", which was true of the CLI and is not
//!   true of the API. The GPU-off supersede push can stay as the fallback it always should have been.
//!
//! # Credentials
//!
//! The same `~/.kaggle/kaggle.json` the CLI reads, as HTTP basic auth — username as the user, API
//! key as the password. Read from that file rather than from the settings store for the reason
//! `kaggle_owner` gives: that file is what the CLI authenticates as, so while both exist they must
//! not be allowed to disagree.

use serde::Serialize;
use serde_json::Value;

const BASE: &str = "https://www.kaggle.com/api/v1";

type Res<T> = Result<T, String>;

/// Username and API key, from the file the CLI uses.
pub async fn credentials() -> Option<(String, String)> {
    let home = std::env::var("HOME").ok()?;
    let raw = tokio::fs::read_to_string(std::path::PathBuf::from(home).join(".kaggle/kaggle.json"))
        .await.ok()?;
    let v: Value = serde_json::from_str(&raw).ok()?;
    let user = v["username"].as_str()?.trim().to_string();
    let key = v["key"].as_str()?.trim().to_string();
    (!user.is_empty() && !key.is_empty()).then_some((user, key))
}

async fn client() -> Res<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        // Kaggle's edge answers a request with no User-Agent with an HTML challenge rather than JSON.
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) BibleMusically")
        .build().map_err(|e| e.to_string())
}

/// GET a Kaggle endpoint with basic auth, returning parsed JSON.
async fn get(path: &str, query: &[(&str, &str)]) -> Res<Value> {
    let (user, key) = credentials().await
        .ok_or("No ~/.kaggle/kaggle.json — connect a Kaggle account first.")?;
    let res = client().await?
        .get(format!("{BASE}{path}"))
        .basic_auth(&user, Some(&key))
        .query(query)
        .send().await.map_err(|e| format!("Could not reach Kaggle: {e}"))?;
    let status = res.status();
    let text = res.text().await.unwrap_or_default();
    if !status.is_success() {
        // 401/403 is the one worth naming: it means the token is stale, which looks identical to a
        // network problem from the outside.
        return Err(match status.as_u16() {
            401 | 403 => "Kaggle rejected the API token. Create a fresh one at Kaggle → Settings → \
                          Create New API Token.".to_string(),
            _ => format!("Kaggle answered {status}: {}", text.chars().take(200).collect::<String>()),
        });
    }
    serde_json::from_str(&text)
        .map_err(|_| format!("Kaggle returned something unreadable: {}",
                             text.chars().take(200).collect::<String>()))
}

// ── Which way to talk to Kaggle ─────────────────────────────────────────────

/// How a Kaggle operation should be carried out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Transport {
    /// Spawn the `kaggle` CLI. Everything the app has always done, and still the default wherever it
    /// can run — it is the better-tested path, and its `kernels push` handles the multipart upload
    /// this module does not yet implement.
    Cli,
    /// Speak the REST API directly. The only option on Android, and the fallback anywhere the CLI is
    /// absent or the user would rather not install Python.
    Http,
}

/// Decide, from the platform and what is installed.
///
/// Mobile is not a preference — Android forbids `exec()` from an app's own data directory, so there
/// is no CLI to choose. On desktop the CLI wins when present, so nothing that works today changes;
/// HTTP is what a machine without it falls back to instead of failing with "could not run the kaggle
/// CLI", which is the error this removes.
pub fn transport(cli_present: bool) -> Transport {
    if cfg!(mobile) { return Transport::Http; }
    if cli_present { Transport::Cli } else { Transport::Http }
}

/// Whether HTTP can work at all: it needs a key file, since there is no cached CLI session to borrow.
pub async fn http_usable() -> bool { credentials().await.is_some() }

// ── Quota ───────────────────────────────────────────────────────────────────

/// GPU time used and allowed this week.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Quota {
    pub used_minutes: i64,
    pub allowed_minutes: i64,
    pub left_minutes: i64,
    /// When the weekly window resets, ISO-8601, as Kaggle stated it.
    pub resets_at: String,
}

/// Kaggle returns durations as protobuf `Duration`, which serialises as a string of seconds with a
/// trailing `s` ("108000s") — but has also appeared as a plain number and as an object. Parsed
/// defensively for that reason: a quota that reads as zero would make the advisor block every run.
pub fn parse_duration_minutes(v: &Value) -> Option<i64> {
    let secs = match v {
        Value::String(s) => s.trim().trim_end_matches('s').parse::<f64>().ok()?,
        Value::Number(n) => n.as_f64()?,
        Value::Object(o) => o.get("seconds")
            .and_then(|s| s.as_f64().or_else(|| s.as_str().and_then(|t| t.parse().ok())))?,
        _ => return None,
    };
    if !secs.is_finite() || secs < 0.0 { return None; }
    Some((secs / 60.0).round() as i64)
}

/// Read a quota response into the numbers the advisor wants.
///
/// Separated from the request so it can be tested against a captured payload — the shape is the part
/// most likely to change under us, and the part that would otherwise only be discovered in the wild.
pub fn parse_quota(v: &Value) -> Quota {
    // Both camelCase and snake_case appear depending on which serialiser answered.
    let gpu = if v["gpuQuota"].is_object() { &v["gpuQuota"] } else { &v["gpu_quota"] };
    let used = parse_duration_minutes(&gpu["timeUsed"])
        .or_else(|| parse_duration_minutes(&gpu["time_used"])).unwrap_or(0);
    let allowed = parse_duration_minutes(&gpu["totalTimeAllowed"])
        .or_else(|| parse_duration_minutes(&gpu["total_time_allowed"])).unwrap_or(0);
    let resets_at = v["quotaRefreshTime"].as_str()
        .or_else(|| v["quota_refresh_time"].as_str()).unwrap_or("").to_string();
    Quota {
        used_minutes: used,
        allowed_minutes: allowed,
        // Never negative: reserved time can push used past allowed, and a negative "left" would
        // read as nonsense in the advisor's arithmetic.
        left_minutes: (allowed - used).max(0),
        resets_at,
    }
}

/// This account's GPU quota, straight from Kaggle.
pub async fn quota() -> Res<Quota> {
    Ok(parse_quota(&get("/kernels/quota", &[]).await?))
}

// ── Status ──────────────────────────────────────────────────────────────────

/// A kernel's latest status, without spawning anything.
///
/// Takes the full `owner/name` slug the rest of the app uses and splits it, so callers do not have
/// to know that Kaggle's API wants the two halves as separate query parameters.
pub async fn kernel_status(slug: &str) -> Res<String> {
    let (owner, name) = slug.split_once('/')
        .ok_or_else(|| format!("'{slug}' is not an owner/name kernel slug."))?;
    let v = get("/kernels/status", &[("userName", owner), ("kernelSlug", name)]).await?;
    Ok(v["status"].as_str().unwrap_or("unknown").to_string())
}

// ── Pull and push ───────────────────────────────────────────────────────────

/// A kernel as Kaggle holds it: its metadata, and its source as one string.
#[derive(Debug, Clone)]
pub struct PulledKernel {
    /// The `owner/name` slug this came back under.
    pub slug: String,
    /// Kaggle's own metadata block, in the shape `kernel-metadata.json` uses.
    pub metadata: Value,
    /// The notebook or script, verbatim. For a notebook this is the `.ipynb` JSON as text.
    pub source: String,
}

/// Fetch a kernel's metadata and source.
///
/// `GET /kernels/pull` — the same call `kaggle kernels pull -m` makes. Returns `Ok(None)` when the
/// kernel does not exist, which is not an error: the first run on a fresh account legitimately has
/// no copy yet, and the caller falls back to upstream or to the bundled notebook.
pub async fn kernel_pull(slug: &str) -> Res<Option<PulledKernel>> {
    let (owner, name) = slug.split_once('/')
        .ok_or_else(|| format!("'{slug}' is not an owner/name kernel slug."))?;
    let v = match get("/kernels/pull", &[("userName", owner), ("kernelSlug", name)]).await {
        Ok(v) => v,
        // A missing kernel comes back as a 404 and reads here as "Kaggle answered 404 Not Found".
        // Distinguished from a real failure so a first run is not reported as broken.
        Err(msg) if msg.contains("404") => return Ok(None),
        Err(msg) => return Err(msg),
    };
    let source = v["blob"]["source"].as_str().unwrap_or("").to_string();
    if source.is_empty() { return Ok(None); }
    Ok(Some(PulledKernel {
        slug: v["metadata"]["ref"].as_str().unwrap_or(slug).to_string(),
        metadata: v["metadata"].clone(),
        source,
    }))
}

/// Prepare a notebook's source the way Kaggle's own client does before pushing it.
///
/// Two transformations, both load-bearing and neither obvious:
///
///   * **Outputs are stripped.** Pushing a notebook with cell outputs re-uploads every image and log
///     line the last run produced, which for these engine notebooks is megabytes of install chatter.
///   * **A cell's `source` is joined into one string.** The `.ipynb` spec allows a list of lines and
///     Kaggle's server accepts only a single string — the client's own comment says so. A notebook
///     pulled from Kaggle comes back already joined, but the *bundled* notebooks in this repo are
///     ordinary files written by hand and by Jupyter, so they have the list form.
///
/// A non-notebook, or anything that does not parse as JSON, is passed through untouched.
pub fn prepare_notebook_source(source: &str, kernel_type: &str) -> String {
    if !kernel_type.eq_ignore_ascii_case("notebook") { return source.to_string(); }
    let Ok(mut nb) = serde_json::from_str::<Value>(source) else { return source.to_string() };
    if let Some(cells) = nb["cells"].as_array_mut() {
        for cell in cells.iter_mut() {
            if cell["cell_type"] == "code" { cell["outputs"] = Value::Array(vec![]); }
            if let Some(list) = cell["source"].as_array() {
                let joined: String = list.iter().filter_map(|l| l.as_str()).collect();
                cell["source"] = Value::String(joined);
            }
        }
    }
    serde_json::to_string(&nb).unwrap_or_else(|_| source.to_string())
}

/// The body `POST /kernels/push` expects, built from a metadata block and a source string.
///
/// Kaggle's client sends the source as **plain text in `text`** — `request.text = script_body`,
/// where `script_body` is the file read straight off disk. It is not base64, and it is not a
/// multipart upload despite the CLI calling the operation "push"; TODOS.md said base64 and was
/// wrong, which is the kind of detail that costs an afternoon.
pub fn push_body(metadata: &Value, source: &str) -> Value {
    let s = |k: &str| metadata[k].as_str().unwrap_or("");
    let b = |k: &str, d: bool| metadata[k].as_bool().unwrap_or(d);
    let arr = |k: &str| metadata[k].as_array().cloned().unwrap_or_default();
    let kernel_type = if s("kernel_type").is_empty() { "notebook" } else { s("kernel_type") };

    serde_json::json!({
        "id": s("id"),
        "newTitle": s("title"),
        "text": prepare_notebook_source(source, kernel_type),
        "language": if s("language").is_empty() { "python" } else { s("language") },
        "kernelType": kernel_type,
        "isPrivate": b("is_private", true),
        "enableGpu": b("enable_gpu", false),
        "enableTpu": b("enable_tpu", false),
        "enableInternet": b("enable_internet", true),
        "datasetDataSources": arr("dataset_sources"),
        "competitionDataSources": arr("competition_sources"),
        "kernelDataSources": arr("kernel_sources"),
        "modelDataSources": arr("model_sources"),
        "categoryIds": arr("keywords"),
    })
}

/// Create or update a kernel and start its run.
///
/// `POST /kernels/push`. This is the call that makes a phone able to *start* a server rather than
/// only watch one — the last CLI-only operation on the critical path.
pub async fn kernel_push(metadata: &Value, source: &str) -> Res<Value> {
    let (user, key) = credentials().await
        .ok_or("No ~/.kaggle/kaggle.json — connect a Kaggle account first.")?;
    let res = client().await?
        .post(format!("{BASE}/kernels/push"))
        .basic_auth(&user, Some(&key))
        .json(&push_body(metadata, source))
        .send().await.map_err(|e| format!("Could not reach Kaggle: {e}"))?;
    let status = res.status();
    let text = res.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 | 403 => "Kaggle rejected the API token. Create a fresh one at Kaggle → Settings → \
                          Create New API Token.".to_string(),
            _ => format!("Kaggle answered {status}: {}", text.chars().take(300).collect::<String>()),
        });
    }
    let v: Value = serde_json::from_str(&text)
        .map_err(|_| format!("Kaggle returned something unreadable: {}",
                             text.chars().take(200).collect::<String>()))?;
    // A push can answer 200 and still have refused: `error` carries the reason, and it is the field
    // the CLI prints. Treating a 200 as success regardless is how "it said it worked" happens.
    if let Some(err) = v["error"].as_str().filter(|s| !s.trim().is_empty()) {
        return Err(err.to_string());
    }
    Ok(v)
}

// ── Output ──────────────────────────────────────────────────────────────────

/// The last public tunnel URL printed in some text, if any.
///
/// The notebooks announce their address by printing it, so finding a server means reading its
/// output. The *last* match wins: a run that reconnected has printed several, and the newest is the
/// only live one. Shared by both transports so a phone and a desktop cannot disagree about which
/// address is current.
pub fn find_tunnel_url(text: &str) -> Option<String> {
    let re = regex::Regex::new(
        r"https://[a-z0-9-]+\.(?:trycloudflare\.com|gradio\.live|lhr\.life|serveo\.net)").ok()?;
    re.find_iter(text).last().map(|m| m.as_str().to_string())
}

/// A finished or running kernel's output: its log, and whatever files it wrote.
#[derive(Debug, Clone, Default)]
pub struct KernelOutput {
    /// The run's console log. This is where a notebook's printed tunnel URL ends up.
    pub log: String,
    /// `(file name, download url)` for each output file.
    pub files: Vec<(String, String)>,
}

/// Fetch a kernel's session output.
///
/// `GET /kernels/output` — the same call `kaggle kernels output` makes. Note that the log comes back
/// *in the response body*, not as one of the files: the CLI writes it out as `<slug>.log` afterwards,
/// which is why scanning a downloaded folder finds it. Over HTTP there is nothing to download for
/// the common case, since the log is already here.
pub async fn kernel_output(slug: &str) -> Res<KernelOutput> {
    let (owner, name) = slug.split_once('/')
        .ok_or_else(|| format!("'{slug}' is not an owner/name kernel slug."))?;
    let v = get("/kernels/output", &[("userName", owner), ("kernelSlug", name)]).await?;
    let files = v["files"].as_array().map(|a| a.iter().filter_map(|f| {
        let name = f["fileName"].as_str().or_else(|| f["file_name"].as_str())?;
        let url = f["url"].as_str()?;
        Some((name.to_string(), url.to_string()))
    }).collect()).unwrap_or_default();
    Ok(KernelOutput { log: v["log"].as_str().unwrap_or("").to_string(), files })
}

/// This engine's live tunnel URL, over HTTP.
///
/// The log alone answers it almost always. Falling back to the output files covers a notebook that
/// wrote its address to a file rather than printing it — cheap, since the listing is already in hand,
/// and only text-shaped files are read.
pub async fn tunnel_url(slug: &str) -> Res<Option<String>> {
    let out = kernel_output(slug).await?;
    if let Some(u) = find_tunnel_url(&out.log) { return Ok(Some(u)); }
    let client = client().await?;
    for (name, url) in out.files.iter().take(20) {
        if !name.ends_with(".txt") && !name.ends_with(".log") && !name.ends_with(".json") { continue; }
        if let Ok(res) = client.get(url).send().await {
            if let Ok(body) = res.text().await {
                if let Some(u) = find_tunnel_url(&body) { return Ok(Some(u)); }
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Nothing that works on a desktop today may change: with a CLI present, the CLI is used.
    #[test]
    fn the_cli_stays_the_desktop_default() {
        if cfg!(mobile) { return; }
        assert_eq!(transport(true), Transport::Cli);
    }

    /// A desktop without the CLI falls back rather than failing — that fallback is the whole point.
    #[test]
    fn a_desktop_without_the_cli_falls_back_to_http() {
        if cfg!(mobile) { return; }
        assert_eq!(transport(false), Transport::Http);
    }

    /// The server accepts one string per cell, not the list the .ipynb spec allows — Kaggle's own
    /// client joins them with a comment saying exactly that, and a bundled notebook written by
    /// Jupyter has the list form. Getting this wrong is a push that is accepted and runs nothing.
    #[test]
    fn a_notebooks_cell_source_is_joined_into_one_string() {
        let nb = json!({ "cells": [
            { "cell_type": "code", "source": ["import os\n", "print(os.getcwd())\n"], "outputs": [] },
        ]}).to_string();
        let out: Value = serde_json::from_str(&prepare_notebook_source(&nb, "notebook")).unwrap();
        assert_eq!(out["cells"][0]["source"], "import os\nprint(os.getcwd())\n");
    }

    /// Outputs are megabytes of install chatter on these notebooks, and re-uploading them on every
    /// start would make each push slower than the run it triggers.
    #[test]
    fn outputs_are_stripped_before_pushing() {
        let nb = json!({ "cells": [
            { "cell_type": "code", "source": "x=1", "outputs": [{ "text": "noise" }] },
            { "cell_type": "markdown", "source": "# title" },
        ]}).to_string();
        let out: Value = serde_json::from_str(&prepare_notebook_source(&nb, "notebook")).unwrap();
        assert_eq!(out["cells"][0]["outputs"], json!([]));
        // A markdown cell has no outputs key to clear, and must not gain one.
        assert!(out["cells"][1].get("outputs").is_none());
    }

    /// A script, or anything that is not JSON, goes through untouched rather than being mangled.
    #[test]
    fn a_script_is_passed_through_unchanged() {
        assert_eq!(prepare_notebook_source("print('hi')", "script"), "print('hi')");
        assert_eq!(prepare_notebook_source("not json at all", "notebook"), "not json at all");
    }

    /// A run that reconnected has printed several addresses and only the newest is live. Handing
    /// back the first would point the app at a tunnel that closed an hour ago — which fails as a
    /// timeout, i.e. indistinguishable from the server never having started.
    #[test]
    fn the_newest_tunnel_url_wins() {
        let log = "opening tunnel\n\
                   https://old-address-here.trycloudflare.com\n\
                   tunnel died, retrying\n\
                   https://new-address-here.trycloudflare.com\n\
                   ready";
        assert_eq!(find_tunnel_url(log).as_deref(), Some("https://new-address-here.trycloudflare.com"));
    }

    /// Every tunnel the notebooks fall back through, since one provider being down is the reason
    /// there is a chain at all.
    #[test]
    fn every_tunnel_provider_in_the_fallback_chain_is_recognised() {
        for host in ["trycloudflare.com", "gradio.live", "lhr.life", "serveo.net"] {
            let line = format!("serving at https://abc-123.{host} now");
            assert_eq!(find_tunnel_url(&line).as_deref(), Some(&*format!("https://abc-123.{host}")));
        }
    }

    /// No address is None, not an empty string — the caller distinguishes "not up yet" from "up at
    /// nowhere", and only the first is worth waiting through.
    #[test]
    fn a_log_with_no_address_yields_nothing() {
        assert!(find_tunnel_url("installing packages...\nno url here").is_none());
        assert!(find_tunnel_url("").is_none());
        // A bare mention of the domain without a scheme is not an address.
        assert!(find_tunnel_url("see trycloudflare.com for details").is_none());
    }

    /// The source travels as plain text. Kaggle's client sets `request.text = script_body` straight
    /// off the file — not base64, and not a multipart upload despite the CLI's name for the verb.
    #[test]
    fn the_push_body_carries_plain_source_and_kaggles_own_field_names() {
        let meta = json!({
            "id": "someone/engine-video", "title": "Video engine", "code_file": "engine.ipynb",
            "language": "python", "kernel_type": "notebook", "is_private": true,
            "enable_gpu": true, "enable_internet": true,
        });
        let body = push_body(&meta, "print('hi')");
        assert_eq!(body["id"], "someone/engine-video");
        assert_eq!(body["newTitle"], "Video engine");
        assert_eq!(body["text"], "print('hi')", "the source must travel verbatim, not encoded");
        assert_eq!(body["enableGpu"], true);
        assert_eq!(body["enableInternet"], true);
        assert_eq!(body["kernelType"], "notebook");
        // Absent list fields become empty arrays rather than null, which the server rejects.
        assert_eq!(body["datasetDataSources"], json!([]));
        assert_eq!(body["categoryIds"], json!([]));
    }

    /// Defaults matter: a metadata block missing these must not silently push a public kernel or one
    /// with no internet, which would fail minutes later inside the run rather than at the push.
    #[test]
    fn push_body_defaults_are_the_safe_ones() {
        let body = push_body(&json!({ "id": "a/b" }), "src");
        assert_eq!(body["isPrivate"], true);
        assert_eq!(body["enableInternet"], true);
        assert_eq!(body["enableGpu"], false);
        assert_eq!(body["language"], "python");
        assert_eq!(body["kernelType"], "notebook");
    }

    /// On Android there is no choice to make: exec() from the app's data directory is forbidden, so
    /// "is the CLI installed" is not a question that can have a useful answer.
    #[test]
    fn mobile_is_always_http_whatever_is_installed() {
        if !cfg!(mobile) { return; }
        assert_eq!(transport(true), Transport::Http);
        assert_eq!(transport(false), Transport::Http);
    }

    /// Kaggle has serialised these durations three different ways. Reading any of them as zero would
    /// make the advisor report no quota left and block every run.
    #[test]
    fn durations_parse_in_every_shape_kaggle_uses() {
        assert_eq!(parse_duration_minutes(&json!("108000s")), Some(1800));
        assert_eq!(parse_duration_minutes(&json!("108000")), Some(1800));
        assert_eq!(parse_duration_minutes(&json!(108000)), Some(1800));
        assert_eq!(parse_duration_minutes(&json!({ "seconds": 108000 })), Some(1800));
        assert_eq!(parse_duration_minutes(&json!({ "seconds": "108000" })), Some(1800));
    }

    #[test]
    fn nonsense_durations_are_rejected_rather_than_guessed() {
        assert_eq!(parse_duration_minutes(&json!("not a duration")), None);
        assert_eq!(parse_duration_minutes(&json!(null)), None);
        assert_eq!(parse_duration_minutes(&json!(-5)), None);
        assert_eq!(parse_duration_minutes(&json!([1, 2])), None);
    }

    #[test]
    fn a_quota_response_becomes_minutes() {
        let body = json!({
            "quotaRefreshTime": "2026-08-08T00:00:00Z",
            "gpuQuota": { "timeUsed": "3600s", "totalTimeAllowed": "108000s" },
        });
        let q = parse_quota(&body);
        assert_eq!(q.used_minutes, 60);
        assert_eq!(q.allowed_minutes, 1800);   // Kaggle's 30 h weekly allowance
        assert_eq!(q.left_minutes, 1740);
        assert_eq!(q.resets_at, "2026-08-08T00:00:00Z");
    }

    #[test]
    fn snake_case_answers_parse_too() {
        let body = json!({
            "quota_refresh_time": "2026-08-08T00:00:00Z",
            "gpu_quota": { "time_used": "600s", "total_time_allowed": "108000s" },
        });
        assert_eq!(parse_quota(&body).used_minutes, 10);
        assert_eq!(parse_quota(&body).left_minutes, 1790);
    }

    /// Reserved time can push usage past the allowance. A negative "left" would be arithmetic the
    /// advisor then reports as a blocker with a nonsense number in it.
    #[test]
    fn using_more_than_allowed_leaves_zero_not_a_negative() {
        let body = json!({ "gpuQuota": { "timeUsed": "200000s", "totalTimeAllowed": "108000s" } });
        assert_eq!(parse_quota(&body).left_minutes, 0);
    }

    /// An empty or unexpected body must not read as "plenty of quota".
    #[test]
    fn an_empty_answer_claims_nothing() {
        let q = parse_quota(&json!({}));
        assert_eq!(q, Quota::default());
        assert_eq!(q.left_minutes, 0);
    }
}
