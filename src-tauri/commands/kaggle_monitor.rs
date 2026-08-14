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

use serde::Serialize;
use serde_json::Value;
use tauri::State;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::settings::{kaggle_slugs, locate_kaggle};
use crate::state::AppState;

type Res<T> = Result<T, String>;

const LOG_RING: usize = 80; // lines kept for the UI log tail
const MONITOR_MAX_SECS: u64 = 20 * 60; // stop streaming after this even if the run keeps serving
const DOWNLOAD_THROTTLE_MS: u128 = 4000; // collapse the checkpoint-download line spam
const PROBE_EVERY_SECS: u64 = 4; // re-probe cadence for a printed-but-not-yet-routable tunnel
const TUNNEL_WARN_SECS: u64 = 45; // say something in the log if the tunnel is slow to answer
/// How long to keep probing a printed tunnel before we stop waiting on it in the foreground.
///
/// Was 210s, which declared a perfectly healthy run dead. A trycloudflare hostname is brand new,
/// and how long it takes to become resolvable and routable *from a particular machine* has nothing
/// to do with the run: measured 2026-08-13, a URL that this laptop could not reach at 210s — TLS
/// connected, HTTP/2 stream opened, no response — answered 404 (alive) about ten minutes after it
/// was printed, on both address families, with the notebook serving perfectly the whole time.
const TUNNEL_FAIL_SECS: u64 = 12 * 60;
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
    // No internet in the session. Every engine clones its code at run time, so this is the first
    // thing that fails and it fails as a bare `CalledProcessError ... exit status 128` — which says
    // nothing about the cause. Kaggle grants notebook internet only to phone-verified accounts, and
    // it declines silently: the push asks for `enable_internet: true`, Kaggle stores it, and the
    // session comes up without a network anyway. Seen on a freshly added second account whose
    // metadata read `enable_internet: True` while git could not resolve github.com.
    if l.contains("Could not resolve host") || l.contains("Temporary failure in name resolution")
        || (l.contains("unable to access") && l.contains("github.com"))
    {
        return Some((
            "This Kaggle account has no internet in its notebooks, so the engine could not download \
             its own code. Kaggle only grants that to phone-verified accounts.".into(),
            Some("no_internet".into()),
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
    // The notebook exhausted every tunnel route (cloudflared/QUIC → cloudflared/HTTP2 →
    // localhost.run) and gave up. Without this the cell simply ends, the kernel finishes cleanly, and
    // the only thing the app could say was the uselessly generic "the run ended before a server came
    // up" — for a run whose model loaded fine and whose server was listening the whole time.
    if l.contains("no working public tunnel") {
        return Some((
            "The engine started and its server was listening, but no public tunnel could be opened \
             after three attempts — so nothing outside Kaggle could reach it.".into(),
            Some("tunnel_dead".into()),
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
    // One tunnel route gave up and the notebook is moving to the next (QUIC → HTTP/2 →
    // localhost.run). Kept in the ring: three attempts take a couple of minutes, and without these
    // lines that stretch looks like a stall rather than a retry that is working as designed.
    if l.contains("never answered within") || l.contains("no URL within")
        || l.contains("exited before printing a URL") || l.contains("attempt ")
    {
        return Some(("tunneling", "info", true));
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
    // The upstream revision this run is built from. Kept in the ring deliberately: every engine
    // clones the default branch fresh on each start, so when one of them breaks overnight without
    // anything here changing, this line is the only evidence of what actually did.
    if l.contains("[upstream]") {
        return Some(("installing", "milestone", true));
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
/// Watch a run without the CLI, by polling.
///
/// There is no REST equivalent of `kernels logs -f`: Kaggle's follow mode is the CLI proxying an SSE
/// feed, and nothing public replaces it. TODOS.md asked the right question rather than assuming one
/// had to be built — whether a phone needs the live boot log at all, or whether the run's *state* is
/// enough. It is. What the boot log is actually watched for is one transition, "is it serving yet",
/// and `kernels/output` carries the run's log in its response body, so the printed tunnel address is
/// readable without following anything.
///
/// So this keeps the monitor's shape and drops only its line-by-line narration: same
/// `KaggleProgress`, same phases, same liveness probe, same terminal conditions. The interface needs
/// no changes and cannot tell which transport answered, beyond a quieter log.
async fn run_monitor_http(
    engine: String,
    slug: String,
    settings_key: &'static str,
    progress: Arc<Mutex<KaggleProgress>>,
    stop: Arc<AtomicBool>,
    db: crate::store::Db,
) {
    let start = std::time::Instant::now();
    let mut url_seen_ms: u128 = 0;
    let mut seen_active = false;
    let mut warned_slow = false;
    progress.lock().unwrap().push(
        "info",
        "Watching over the Kaggle API (no command-line tool here), so this log shows the run's state \
         rather than its output.".into());

    loop {
        if stop.load(Ordering::Relaxed) {
            let mut p = progress.lock().unwrap();
            if !p.done { p.phase = "stopped".into(); }
            return;
        }
        if start.elapsed().as_secs() > MONITOR_MAX_SECS {
            progress.lock().unwrap()
                .push("info", "Stopped watching after 20 min — the run may still be serving.".into());
            return;
        }

        // State first: it is one cheap call and it decides whether waiting is still worthwhile.
        if let Ok(raw) = crate::kaggle_api::kernel_status(&slug).await {
            let ks = kstatus_from(&raw.to_uppercase());
            if matches!(ks, "queued" | "running") { seen_active = true; }
            let trustworthy = seen_active || start.elapsed().as_secs() > STALE_STATUS_GRACE_SECS;
            let terminal_fail = {
                let mut p = progress.lock().unwrap();
                p.kernel_status = ks.to_string();
                if phase_rank("queued") >= phase_rank(&p.phase) && ks == "running" {
                    p.phase = "installing".into();
                }
                // Same rule the streaming monitor uses: a terminal status with no *verified* tunnel
                // means the run died before serving, and right after a push the status can still
                // describe the previous session.
                if matches!(ks, "error" | "complete" | "cancelled") && !p.url_live && trustworthy {
                    if p.error.is_none() {
                        p.error = Some(match ks {
                            "error" => format!("The run on {slug} ERRORED before a server came up — open the notebook on Kaggle to see why."),
                            _ => format!("The run on {slug} ended ({ks}) before a server came up."),
                        });
                    }
                    p.phase = "error".into();
                    true
                } else { false }
            };
            if terminal_fail { return; }
        }

        // Then the address, which is in the output response rather than behind a stream.
        let known = { progress.lock().unwrap().url.clone() };
        if known.is_none() {
            if let Ok(Some(u)) = crate::kaggle_api::tunnel_url(&slug).await {
                url_seen_ms = now_ms();
                let mut p = progress.lock().unwrap();
                p.push("info", format!("Server address found: {u}"));
                p.url = Some(u);
                p.phase = "serving".into();
            }
        }

        // A quick tunnel is not routable the instant it is printed, so keep asking rather than
        // judging it on one early try — the same reason the streaming monitor probes on a timer.
        let pending = {
            let p = progress.lock().unwrap();
            match (&p.url, p.url_live) { (Some(u), false) => Some(u.clone()), _ => None }
        };
        if let Some(u) = pending {
            if probe_alive(&u).await {
                {
                    let mut p = progress.lock().unwrap();
                    p.url_live = true;
                    p.phase = "serving".into();
                    p.done = true;
                    p.push("info", "Server is answering.".into());
                }
                crate::commands::settings::persist_engine_url(&db, settings_key, &u).await;
                return;
            }
            let waited = ((now_ms().saturating_sub(url_seen_ms)) / 1000) as u64;
            if waited >= TUNNEL_WARN_SECS && !warned_slow {
                warned_slow = true;
                progress.lock().unwrap().push(
                    "info", format!("Tunnel printed {waited}s ago but not answering yet — still retrying…"));
            }
            if waited >= TUNNEL_FAIL_SECS {
                let mut p = progress.lock().unwrap();
                p.phase = "error".into();
                p.error = Some(format!(
                    "The notebook opened {u} but it never started answering ({waited}s). The Cloudflare quick tunnel did not come up."));
                p.hint = Some("tunnel_dead".into());
                return;
            }
        }

        // Slower than the stream it replaces, because each tick is two HTTPS round trips against a
        // rate-limited API rather than a line already in a pipe. A boot takes minutes; ten seconds
        // of latency on noticing it finished is not worth being rude to Kaggle for.
        let _ = engine; // named for the log message above; the loop keys off the slug
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}

async fn run_monitor(
    engine: String,
    slug: String,
    settings_key: &'static str,
    kaggle: String,
    progress: Arc<Mutex<KaggleProgress>>,
    stop: Arc<AtomicBool>,
    db: crate::store::Db,
) {
    let slug = slug.as_str();
    progress.lock().unwrap().push("info", format!("Watching {} ({}) boot on Kaggle…", engine, slug));

    // `kaggle kernels logs -f` dumps the whole persisted log before it starts following, so every
    // reconnect replays everything already seen. Left unchecked that re-pushed old milestones into
    // the ring in a fresh order — which is what made the log read as a *successful* boot interleaved
    // out of sequence ("Server is up" before "installed — environment OK") for a run that had not
    // started. Counting consumed lines and skipping the replayed prefix keeps the log a log.
    let mut consumed: usize = 0;

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
        // Position within THIS attachment's stream, against the replayed prefix counted above.
        let mut seen_this_attach: usize = 0;

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
                                    // Name the kernel. When the app was addressing an account it was
                                    // not signed in as, this sentence was the only thing the user
                                    // ever saw — and it described a run on somebody else's kernel
                                    // without ever saying whose.
                                    p.error = Some(match ks {
                                        "error" => format!("The run on {slug} ERRORED before a server came up — see the log below."),
                                        _ => format!("The run on {slug} ended ({ks}) before a server came up."),
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
                            // Via the shared helper because writing the singleton alone leaves any
                            // per-project override shadowing it — see `persist_engine_url`.
                            crate::commands::settings::persist_engine_url(&db, settings_key, &u).await;
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
                            // Whether this is a failure depends entirely on whether the run is
                            // still alive, and the previous version never asked. It reported a
                            // dead tunnel, and the interface told the user to press Start & connect
                            // again — which supersedes a serving GPU run, spends another eight
                            // minutes and more of the weekly quota, and arrives at a brand-new
                            // hostname with exactly the same propagation problem. That advice was
                            // worse than doing nothing.
                            //
                            // A run that is still RUNNING has a server on it: cloudflared
                            // registered an edge connection and the notebook's own probe reached
                            // the origin through the tunnel before it printed the URL. What has not
                            // happened is this machine being able to route the hostname yet, and
                            // that resolves itself. So the URL is saved (Test connection, and any
                            // job's own retry, can pick it up the moment routing catches up) and
                            // this ends as a distinct `tunnel_slow` outcome rather than an error.
                            let ks = match tokio::process::Command::new(&kaggle)
                                .args(["kernels", "status", slug]).output().await
                            {
                                Ok(out) => kstatus_from(&format!("{}{}",
                                    String::from_utf8_lossy(&out.stdout),
                                    String::from_utf8_lossy(&out.stderr))),
                                Err(_) => "unknown",
                            };
                            let still_serving = matches!(ks, "running" | "queued");
                            if still_serving {
                                crate::commands::settings::persist_engine_url(&db, settings_key, &u).await;
                            }
                            {
                                let mut p = progress.lock().unwrap();
                                p.done = true;
                                if still_serving {
                                    p.phase = "tunneling".into();
                                    p.hint = Some("tunnel_slow".into());
                                    p.error = Some(format!(
                                        "The server is running and its tunnel is open, but {} is not reachable from this \
                                         computer yet ({}s). A new trycloudflare address can take several minutes to \
                                         route. The address has been saved — do NOT restart the run; press Test \
                                         connection in a few minutes instead.",
                                        u, waited));
                                } else {
                                    p.phase = "error".into();
                                    p.hint = Some("tunnel_dead".into());
                                    p.error = Some(format!(
                                        "The notebook opened {} but it never started answering ({}s), and the run has \
                                         since ended ({}).", u, waited, ks));
                                }
                            }
                            let _ = child.kill().await;
                            break 'outer;
                        }
                    }
                }
                maybe_line = async { match &mut lines { Some(l) => l.next_line().await, None => Ok(None) } } => {
                    match maybe_line {
                        Ok(Some(line)) => {
                            // Skip the replayed prefix this attachment has already shown us.
                            seen_this_attach += 1;
                            if seen_this_attach <= consumed { continue; }
                            consumed = seen_this_attach;

                            let line = line.trim_end().to_string();
                            if line.is_empty() { continue; }

                            // Fatal error?
                            if let Some((summary, hint)) = detect_error(&line) {
                                // "Kaggle gave this run no GPU" is one symptom with two very
                                // different causes, and the app used to assume the worse one: it
                                // announced that the weekly quota was spent and asked the user to
                                // connect a second account. It said exactly that to an account
                                // holding 29.8 of its 30 hours, on a run where Kaggle had simply
                                // had no free T4 that minute — a probe kernel pushed six minutes
                                // later was given two. So ask Kaggle what the quota really is
                                // before naming a cause. `gpu_unavailable` is the same denial with
                                // hours still on the clock, and the answer to it is to try again,
                                // not to go and find another account.
                                let (summary, hint) = match (hint.as_deref(), crate::kaggle_api::quota().await) {
                                    (Some("gpu_denied"), Ok(q)) if q.left_minutes > 0 => (
                                        format!(
                                            "Kaggle gave this run no GPU, so the server never started — but this \
                                             account still has {} of its {} GPU minutes left this week, so the \
                                             quota is not the reason. Kaggle just had none free for this session.",
                                            q.left_minutes, q.allowed_minutes),
                                        Some("gpu_unavailable".to_string()),
                                    ),
                                    _ => (summary, hint),
                                };
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
    // A hidden engine gets no monitor. This is the long-lived one — it holds a `kaggle kernels
    // logs -f` child process and polls status for up to twenty minutes — so refusing here is most of
    // what "consumes nothing" means in practice.
    if crate::commands::settings::engine_hidden(&engine) {
        return Ok(serde_json::json!({ "ok": false, "hidden": true,
            "detail": format!("The {engine} engine is turned off in this build.") }));
    }
    let (slug, _upstream, settings_key) = match kaggle_slugs(&state.db, &engine).await {
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

    // No CLI to follow means polling instead — same progress object, same phases, quieter log.
    // See run_monitor_http for why a phone does not need the streamed boot output.
    let found = super::settings::locate_kaggle_opt();
    let http = crate::kaggle_api::transport(found.is_some()) == crate::kaggle_api::Transport::Http;
    let db = state.db.clone();
    let engine_moved = engine.clone();
    let slug_moved = slug.clone();
    if http {
        tauri::async_runtime::spawn(async move {
            run_monitor_http(engine_moved, slug_moved, settings_key, progress, stop, db).await;
        });
    } else {
        let kaggle = found.unwrap_or_else(|| "kaggle".to_string());
        tauri::async_runtime::spawn(async move {
            run_monitor(engine_moved, slug_moved, settings_key, kaggle, progress, stop, db).await;
        });
    }

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
