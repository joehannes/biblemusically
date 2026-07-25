// Live startup monitor for the Kaggle engine notebooks.
//
// The four engine notebooks print clean progress milestones as they boot a GPU batch run
// (install → download ~21 GB checkpoints → load pipeline → open a Cloudflare tunnel → serve),
// and on failure they print the papermill traceback. None of that was ever surfaced: the app
// only streamed the log for 25 s to scrape the tunnel URL and threw the rest away, so a startup
// that stalled or errored looked identical to one still working — the user just waited.
//
// This module runs `kaggle kernels logs -f <slug>` as a long-lived child (the CLI's follow mode
// proxies the live SSE feed for a running session and dumps the persisted blob for a finished one,
// printing one `data` line per event), classifies each line into a phase, keeps a ring buffer of
// the meaningful lines, extracts + liveness-probes the tunnel URL, and captures the fatal error
// with an actionable hint (e.g. flux's missing HF_TOKEN, GPU-quota exhaustion). The frontend polls
// `kaggle_progress` to render a real progress panel and to know the moment a live URL is ready.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bson::{doc, Document};
use serde::Serialize;
use serde_json::Value;
use tauri::State;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::settings::{kaggle_kernel_for, locate_kaggle};
use crate::state::AppState;

type Res<T> = Result<T, String>;

const LOG_RING: usize = 80; // lines kept for the UI log tail
const MONITOR_MAX_SECS: u64 = 20 * 60; // stop streaming after this even if the run keeps serving
const DOWNLOAD_THROTTLE_MS: u128 = 4000; // collapse the checkpoint-download line spam
const PROBE_EVERY_SECS: u64 = 4; // re-probe cadence for a printed-but-not-yet-routable tunnel
const TUNNEL_WARN_SECS: u64 = 45; // say something in the log if the tunnel is slow to answer
const TUNNEL_FAIL_SECS: u64 = 210; // give up on a URL the Cloudflare edge never routes
const STALE_STATUS_GRACE_SECS: u64 = 45; // ignore a terminal status this early (it's the *previous* session's)

#[derive(Clone, Serialize)]
pub struct LogLine {
    pub t: u64, // seconds since the monitor started, when this line was captured
    pub level: String, // "info" | "milestone" | "error"
    pub text: String,
}

#[derive(Clone, Serialize, Default)]
pub struct KaggleProgress {
    pub engine: String,
    /// queued | running | error | complete | cancelled | unknown | ""
    pub kernel_status: String,
    /// idle | queued | installing | downloading | loading | serving | error | stopped
    pub phase: String,
    pub url: Option<String>,
    pub url_live: bool,
    pub error: Option<String>,
    pub hint: Option<String>,
    pub log: VecDeque<LogLine>,
    pub elapsed_s: u64,
    pub running: bool,
    pub done: bool,
    #[serde(skip)]
    pub started_ms: u128,
}

impl KaggleProgress {
    fn new(engine: &str) -> Self {
        KaggleProgress {
            engine: engine.to_string(),
            phase: "queued".into(),
            log: VecDeque::new(),
            running: true,
            started_ms: now_ms(),
            ..Default::default()
        }
    }
    fn elapsed(&self) -> u64 { ((now_ms().saturating_sub(self.started_ms)) / 1000) as u64 }
    fn push(&mut self, level: &str, text: String) {
        self.log.push_back(LogLine { t: self.elapsed(), level: level.into(), text });
        while self.log.len() > LOG_RING { self.log.pop_front(); }
    }
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// One monitor per engine. `stop` lets a new run (or an explicit stop) cancel the streaming task.
#[derive(Default)]
pub struct KaggleMonitors {
    pub map: Mutex<HashMap<String, Arc<Mutex<KaggleProgress>>>>,
    pub stops: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

// ── line classification ────────────────────────────────────────────────
// Phase ranking so we never regress (a late "Downloading" line for a second model shouldn't
// bump us back from "loading" to "downloading").
fn phase_rank(p: &str) -> u8 {
    match p {
        "queued" => 0,
        "installing" => 1,
        "downloading" => 2,
        "loading" => 3,
        "tunneling" => 4,
        "serving" => 5,
        "error" => 6,
        _ => 0,
    }
}

/// A fatal error line from the batch run. Returns (summary, optional hint).
fn detect_error(line: &str) -> Option<(String, Option<String>)> {
    let l = line;
    if l.contains("No HF_TOKEN") || (l.contains("HF_TOKEN") && l.contains("gated")) {
        return Some((
            "FLUX needs a Hugging Face token: the FLUX.1-schnell repo is gated.".into(),
            Some("hf_token".into()),
        ));
    }
    if l.contains("PapermillExecutionError") {
        return Some(("A notebook cell raised an exception — the run aborted.".into(), None));
    }
    if l.contains("CUDA out of memory") || l.contains("OutOfMemoryError") {
        return Some(("The GPU ran out of memory while loading the model.".into(), Some("oom".into())));
    }
    // Kaggle accepted the run but handed us a CPU-only container. The app ALWAYS pushes with
    // enable_gpu=true, so this means Kaggle declined the GPU — almost always an exhausted weekly
    // quota. Without this the notebook just skips serving and the run ends "successfully", which
    // surfaced as the useless "the run ended before the server came up".
    if l.contains("NO GPU ON THIS RUN") {
        return Some((
            "Kaggle gave this run no GPU, so the server was not started. Your weekly GPU quota is most likely exhausted.".into(),
            Some("gpu_denied".into()),
        ));
    }
    if let Some(idx) = l.find("Error: ") {
        // A concrete exception line (e.g. "RuntimeError: ...."), not a pip warning.
        let rest = &l[idx..];
        if rest.len() > 9 && !l.contains("WARNING") {
            return Some((rest.trim().chars().take(200).collect(), None));
        }
    }
    None
}

/// Map a single log `data` line to a phase (if it's a recognizable milestone) and whether it's
/// worth keeping in the UI ring. Ordered most-advanced first.
fn classify(line: &str) -> Option<(&'static str, &'static str, bool)> {
    // (phase, level, keep_in_ring)
    let l = line;
    if l.contains("trycloudflare.com") || l.contains("gradio.live") {
        return Some(("serving", "milestone", true));
    }
    // Genuine "something is listening" signals: a registered tunnel connection, or the notebook's
    // own local-server handshake.
    if l.contains("Registered tunnel connection")
        || l.contains("Uvicorn running") || l.contains("Application startup complete")
        || l.contains("Running on") || l.contains("Serving on") || l.contains("server is live")
        || l.contains("Started server") || l.contains("Server is up") || l.contains("server up")
    {
        return Some(("serving", "milestone", true));
    }
    // cloudflared boot chatter — the version banner, "Cannot determine default configuration path",
    // the CONNECTIVITY PRE-CHECKS table, ICMP/metrics notices. It means the tunnel *process* started,
    // NOT that anything is reachable: the URL only appears several seconds later. Treating this as
    // "serving" is what used to light the Serve step (and suppress failure detection) far too early.
    // Kept out of the ring as well, so ~20 lines of pre-check table don't evict the real milestones.
    if l.contains("Requesting new quick Tunnel") {
        return Some(("tunneling", "milestone", true));
    }
    if l.contains("[tunnel]") || l.contains("Cloudflare Tunnel") {
        return Some(("tunneling", "info", false));
    }
    if l.contains("checkpoints downloaded") || l.contains("Loading pipeline")
        || l.contains("Loading checkpoint") || l.contains("Loading model")
        || l.contains("Loaded ") || l.contains("loading weights")
    {
        return Some(("loading", "milestone", true));
    }
    if l.contains("Downloading") || (l.contains("Fetching") && l.contains("files")) {
        return Some(("downloading", "info", true)); // throttled by the caller
    }
    if l.contains("environment OK") || l.contains("Successfully installed")
        || l.contains("installed —") || l.contains("Installing ") || l.contains("Collecting ")
    {
        return Some(("installing", "milestone", true));
    }
    None
}

fn extract_url(line: &str) -> Option<String> {
    // Tunnels are lowercase host + one of the two providers.
    let re = regex::Regex::new(r"https://[a-z0-9-]+\.(?:trycloudflare\.com|gradio\.live|lhr\.life|serveo\.net)").ok()?;
    re.find(line).map(|m| m.as_str().to_string())
}

fn kstatus_from(raw: &str) -> &'static str {
    if raw.contains("CANCEL") { "cancelled" }
    else if raw.contains("COMPLETE") { "complete" }
    else if raw.contains("RUNNING") { "running" }
    else if raw.contains("ERROR") { "error" }
    else if raw.contains("QUEUED") || raw.contains("PREPARING") { "queued" }
    else { "unknown" }
}

/// Is the tunnel actually routed to the notebook's server yet?
///
/// Any non-5xx answer counts: these engines expose a task API with no `/` route, so a 404 straight
/// from FastAPI is proof the origin is reachable. While the quick tunnel is still registering, the
/// Cloudflare edge answers 502/503/530 (or DNS/connect fails outright) — all correctly "not yet".
async fn probe_alive(url: &str) -> bool {
    match reqwest::Client::new().get(url).timeout(Duration::from_secs(8)).send().await {
        Ok(res) => res.status().as_u16() < 500,
        Err(_) => false,
    }
}

/// The background streaming task. Owns the child `kaggle kernels logs -f` process and a periodic
/// `kernels status` poll, writing everything into the shared `progress`.
async fn run_monitor(
    engine: String,
    slug: &'static str,
    settings_key: &'static str,
    kaggle: String,
    progress: Arc<Mutex<KaggleProgress>>,
    stop: Arc<AtomicBool>,
    db: crate::store::Db,
) {
    progress.lock().unwrap().push("info", format!("Watching {} boot on Kaggle…", engine));

    let mut status_iv = tokio::time::interval(Duration::from_secs(12));
    let mut probe_iv = tokio::time::interval(Duration::from_secs(PROBE_EVERY_SECS));
    let mut last_download_push: u128 = 0;
    let mut url_seen_ms: u128 = 0; // when the tunnel URL was first printed (drives the stall timeout)
    let mut warned_slow = false;
    let mut seen_active = false; // the kernel has been observed queued/running at least once
    let start = std::time::Instant::now();

    // Why an outer loop: `kaggle kernels logs -f` attaches to the *latest* session. Right after a
    // push that session is still QUEUED and the follow stream can close immediately with no data —
    // treating that end-of-stream as "the run is over" would be a false failure. So on a clean
    // stream end while the kernel is still queued/running we reconnect, and only finalize on a
    // terminal status, a live tunnel, a stop request, or the time budget.
    'outer: loop {
        if stop.load(Ordering::Relaxed) {
            let mut p = progress.lock().unwrap();
            if !p.done { p.phase = "stopped".into(); }
            break;
        }
        if start.elapsed().as_secs() > MONITOR_MAX_SECS {
            let mut p = progress.lock().unwrap();
            p.push("info", "Stopped watching after 20 min — the run may still be serving.".into());
            break;
        }

        // (Re)attach the follow stream. stderr → null so reconnect notices don't pollute stdout.
        let child = tokio::process::Command::new(&kaggle)
            .args(["kernels", "logs", "-f", slug])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(err) => {
                let mut p = progress.lock().unwrap();
                p.phase = "error".into();
                p.error = Some(format!("Could not run the kaggle CLI: {}", err));
                p.hint = Some("cli".into());
                break;
            }
        };
        let stdout = child.stdout.take();
        let mut lines = stdout.map(|s| BufReader::new(s).lines());

        // Inner read loop for this stream attachment. Exits are all explicit: `break 'outer` on a
        // terminal outcome, or `continue 'outer` (after a short pause) when the follow stream just
        // dropped while the run is still booting.
        loop {
            if stop.load(Ordering::Relaxed) {
                { let mut p = progress.lock().unwrap(); if !p.done { p.phase = "stopped".into(); } }
                let _ = child.kill().await;
                break 'outer;
            }
            if start.elapsed().as_secs() > MONITOR_MAX_SECS {
                let _ = child.kill().await;
                break 'outer;
            }

            tokio::select! {
                _ = status_iv.tick() => {
                    if let Ok(out) = tokio::process::Command::new(&kaggle)
                        .args(["kernels", "status", slug]).output().await
                    {
                        let raw = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
                        let ks = kstatus_from(&raw);
                        if matches!(ks, "queued" | "running") { seen_active = true; }
                        // Right after a push, `kernels status` can still describe the *previous*
                        // session — usually "complete" — so a terminal status is only believed once
                        // we've seen this run alive, or after a short grace period.
                        let trustworthy = seen_active || start.elapsed().as_secs() > STALE_STATUS_GRACE_SECS;
                        // Scope the guard so it is dropped before any .await below (a std::sync
                        // MutexGuard held across await makes the whole task non-Send).
                        let terminal_fail = {
                            let mut p = progress.lock().unwrap();
                            p.kernel_status = ks.to_string();
                            // A terminal status with no *verified* tunnel means the run died before
                            // serving. Deliberately not gated on phase != "serving": cloudflared's
                            // boot chatter used to set that phase, which silently disabled this
                            // check for every run that got as far as starting the tunnel.
                            if matches!(ks, "error" | "complete" | "cancelled") && !p.url_live && trustworthy {
                                if p.error.is_none() {
                                    p.error = Some(match ks {
                                        "error" => "The run ERRORED before a server came up — see the log below.".to_string(),
                                        _ => format!("The run ended ({}) before a server came up.", ks),
                                    });
                                }
                                p.phase = "error".into();
                                true
                            } else { false }
                        };
                        if terminal_fail {
                            let _ = child.kill().await;
                            break 'outer;
                        }
                    }
                }
                // Liveness polling. A quick tunnel is not reachable the moment cloudflared prints
                // its URL — the edge only routes it once a connection is registered, typically a
                // few seconds later — so we keep asking instead of judging it on one early try.
                _ = probe_iv.tick() => {
                    let pending = {
                        let p = progress.lock().unwrap();
                        match (&p.url, p.url_live) { (Some(u), false) => Some(u.clone()), _ => None }
                    };
                    if let Some(u) = pending {
                        if probe_alive(&u).await {
                            {
                                let mut p = progress.lock().unwrap();
                                p.url_live = true;
                                if phase_rank("serving") >= phase_rank(&p.phase) { p.phase = "serving".into(); }
                                p.push("milestone", "Tunnel is answering — server is up.".into());
                            }
                            // Persist the verified URL so job code picks it up without a manual Fetch.
                            let mut set = Document::new();
                            set.insert(settings_key, u.clone());
                            let _ = db.collection::<Document>("settings")
                                .update_one(doc! { "_id": "singleton" }, doc! { "$set": set }).await;
                            // A verified live tunnel is the goal — nothing more to watch here.
                            let _ = child.kill().await;
                            break 'outer;
                        }
                        let waited = ((now_ms().saturating_sub(url_seen_ms)) / 1000) as u64;
                        if waited >= TUNNEL_WARN_SECS && !warned_slow {
                            warned_slow = true;
                            progress.lock().unwrap().push(
                                "info",
                                format!("Tunnel printed {}s ago but not answering yet — still retrying…", waited),
                            );
                        }
                        if waited >= TUNNEL_FAIL_SECS {
                            {
                                let mut p = progress.lock().unwrap();
                                p.phase = "error".into();
                                p.error = Some(format!(
                                    "The notebook opened {} but it never started answering ({}s). The Cloudflare quick tunnel did not come up.",
                                    u, waited
                                ));
                                p.hint = Some("tunnel_dead".into());
                            }
                            let _ = child.kill().await;
                            break 'outer;
                        }
                    }
                }
                maybe_line = async { match &mut lines { Some(l) => l.next_line().await, None => Ok(None) } } => {
                    match maybe_line {
                        Ok(Some(line)) => {
                            let line = line.trim_end().to_string();
                            if line.is_empty() { continue; }

                            // Fatal error?
                            if let Some((summary, hint)) = detect_error(&line) {
                                let mut p = progress.lock().unwrap();
                                if p.error.is_none() { p.error = Some(summary); }
                                if hint.is_some() { p.hint = hint; }
                                if phase_rank("error") >= phase_rank(&p.phase) { p.phase = "error".into(); }
                                p.push("error", line.chars().take(240).collect());
                                continue;
                            }

                            // URL? Only *record* it here — liveness is decided by the probe branch
                            // below, which keeps retrying. (Probing once, inline, at the instant the
                            // URL is printed is exactly the bug this replaces: cloudflared prints
                            // "Visit it at (it may take some time to be reachable)" seconds before
                            // the edge routes the hostname, so that single probe almost always got a
                            // 530/DNS failure — and since a repeated URL line short-circuits on
                            // `already`, url_live could then never flip true and the UI sat on
                            // "Serve" until the frontend's 14-minute timeout.)
                            if let Some(u) = extract_url(&line) {
                                let is_new = {
                                    let mut p = progress.lock().unwrap();
                                    if p.url.as_deref() == Some(u.as_str()) {
                                        false
                                    } else {
                                        p.url = Some(u.clone());
                                        p.push("milestone", format!("Tunnel URL: {}", u));
                                        p.push("info", "Waiting for the Cloudflare edge to route it…".into());
                                        true
                                    }
                                };
                                if is_new {
                                    url_seen_ms = now_ms();
                                    warned_slow = false;
                                }
                                continue;
                            }

                            // Milestone / phase line?
                            if let Some((phase, level, keep)) = classify(&line) {
                                let mut p = progress.lock().unwrap();
                                // phase_rank alone prevents regression (and keeps "error" sticky).
                                if phase_rank(phase) >= phase_rank(&p.phase) {
                                    p.phase = phase.to_string();
                                }
                                if keep {
                                    if phase == "downloading" {
                                        let n = now_ms();
                                        if n.saturating_sub(last_download_push) >= DOWNLOAD_THROTTLE_MS {
                                            last_download_push = n;
                                            p.push("info", "Downloading model checkpoints…".into());
                                        }
                                    } else {
                                        p.push(level, line.chars().take(200).collect());
                                    }
                                }
                            }
                        }
                        Ok(None) | Err(_) => {
                            // This stream attachment ended. If we already have a live tunnel or the
                            // kernel is terminal, we're done; otherwise the follow stream just
                            // dropped (common while queued) — reconnect after a short pause.
                            // Only a *verified* tunnel (or a terminal kernel status) ends the watch.
                            // `phase == "serving"` used to count too, which meant a follow stream
                            // that dropped any time after cloudflared started was reported as "the
                            // run ended before a server came up" while the run was in fact fine.
                            let terminal = {
                                let p = progress.lock().unwrap();
                                p.url_live
                                    || matches!(p.kernel_status.as_str(), "error" | "complete" | "cancelled")
                            };
                            if terminal {
                                {
                                    let mut p = progress.lock().unwrap();
                                    if !p.url_live && p.error.is_none() {
                                        p.phase = "error".into();
                                        p.error = Some(format!("The run ended ({}) without opening a live tunnel.", p.kernel_status));
                                    }
                                } // guard dropped before the await
                                let _ = child.kill().await;
                                break 'outer;
                            }
                            // Not terminal — the follow stream just dropped (common while the
                            // session is still queued). Reconnect after a short pause.
                            let _ = child.kill().await;
                            tokio::time::sleep(Duration::from_secs(3)).await;
                            continue 'outer;
                        }
                    }
                }
            }
        }
    }

    let mut p = progress.lock().unwrap();
    p.running = false;
    p.done = true;
}

// ── commands ───────────────────────────────────────────────────────────

/// Start (or restart) the live monitor for an engine. Idempotent: if one is already running it is
/// left alone unless `fresh` is set, in which case the old one is cancelled and a clean one starts
/// (used right after pushing a new run so stale progress from the previous run is cleared).
#[tauri::command]
pub async fn kaggle_start_monitor(
    engine: String,
    fresh: Option<bool>,
    state: State<'_, AppState>,
    monitors: State<'_, KaggleMonitors>,
) -> Res<Value> {
    let (slug, settings_key) = match kaggle_kernel_for(&engine) {
        Some(v) => v,
        None => return Ok(serde_json::json!({ "ok": false, "detail": format!("Unknown engine '{}'.", engine) })),
    };
    let fresh = fresh.unwrap_or(false);

    // Already monitoring and caller doesn't want a reset → no-op.
    {
        let map = monitors.map.lock().unwrap();
        if let Some(p) = map.get(&engine) {
            let running = p.lock().unwrap().running;
            if running && !fresh {
                return Ok(serde_json::json!({ "ok": true, "detail": "already monitoring" }));
            }
        }
    }
    // Cancel any existing task for this engine.
    if let Some(old) = monitors.stops.lock().unwrap().get(&engine) {
        old.store(true, Ordering::Relaxed);
    }

    let progress = Arc::new(Mutex::new(KaggleProgress::new(&engine)));
    let stop = Arc::new(AtomicBool::new(false));
    monitors.map.lock().unwrap().insert(engine.clone(), progress.clone());
    monitors.stops.lock().unwrap().insert(engine.clone(), stop.clone());

    let kaggle = locate_kaggle();
    let db = state.db.clone();
    let engine_moved = engine.clone();
    tauri::async_runtime::spawn(async move {
        run_monitor(engine_moved, slug, settings_key, kaggle, progress, stop, db).await;
    });

    Ok(serde_json::json!({ "ok": true, "detail": "monitor started" }))
}

#[tauri::command]
pub async fn kaggle_progress(engine: String, monitors: State<'_, KaggleMonitors>) -> Res<Value> {
    let snapshot = {
        let map = monitors.map.lock().unwrap();
        map.get(&engine).map(|p| {
            let mut c = p.lock().unwrap().clone();
            c.elapsed_s = c.elapsed();
            c
        })
    };
    match snapshot {
        Some(p) => Ok(serde_json::to_value(p).map_err(|e| e.to_string())?),
        None => Ok(serde_json::json!({ "engine": engine, "phase": "idle", "running": false, "done": false, "log": [] })),
    }
}

#[tauri::command]
pub async fn kaggle_stop_monitor(engine: String, monitors: State<'_, KaggleMonitors>) -> Res<Value> {
    if let Some(stop) = monitors.stops.lock().unwrap().get(&engine) {
        stop.store(true, Ordering::Relaxed);
    }
    Ok(serde_json::json!({ "ok": true }))
}
