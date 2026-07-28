//! Stopping a free GPU server that nobody is using.
//!
//! Kaggle gives about thirty GPU hours a week. A session left running holds its slot and spends
//! those hours whether or not anything is generating — so the cost of forgetting to press Stop is
//! not a warning, it is next weekend's rendering. Nothing in the app noticed: a server started for
//! one picture stayed up until the user remembered it, or until Kaggle's own watchdog killed it
//! hours later.
//!
//! The rule here is deliberately timid, because the failure modes are not symmetric. Stopping a
//! server too eagerly costs a cold start — five to ten minutes of waiting the next time somebody
//! generates. Stopping one *mid-generation* costs the generation, and looks like the app broke. So:
//!
//!   * an engine is only ever a candidate once the app has actually used it in this session;
//!   * nothing is stopped while any job is queued or running, for any engine, because a job that
//!     has not reached its generation step yet is still about to need a server;
//!   * activity refreshes the clock rather than the sweep resetting it, so a long generation cannot
//!     age into the timeout while it is still going.
//!
//! Off is one setting away (`kaggle_idle_stop_minutes = 0`), and the default is generous enough
//! that a user thinking between prompts is not treated as a user who has gone home.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// How long an engine may sit unused before its session is ended. Twenty-five minutes: long enough
/// to write a prompt, read the result and decide, short enough that a forgotten server costs a
/// fraction of an hour rather than the rest of the day.
pub const DEFAULT_IDLE_MINUTES: u64 = 25;

fn seen() -> &'static Mutex<HashMap<String, Instant>> {
    static SEEN: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    SEEN.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Note that an engine's server is being used right now.
///
/// Called when a job talks to one. Both at the start and the end of the work, so a generation that
/// takes twenty minutes does not age into the timeout while it is still running.
pub fn touch(engine: &str) {
    if engine.trim().is_empty() { return; }
    if let Ok(mut m) = seen().lock() {
        m.insert(engine.trim().to_ascii_lowercase(), Instant::now());
    }
}

/// Forget an engine — after it has been stopped, so it is not a candidate again until something
/// starts and uses it.
pub fn forget(engine: &str) {
    if let Ok(mut m) = seen().lock() {
        m.remove(&engine.trim().to_ascii_lowercase());
    }
}

/// Engines untouched for longer than `idle_minutes`.
///
/// Split out from the sweep so the decision is testable without a Kaggle account, a network, or a
/// twenty-five minute wait.
pub fn stale(idle_minutes: u64, now: Instant) -> Vec<String> {
    let Ok(m) = seen().lock() else { return Vec::new() };
    m.iter()
        .filter(|(_, last)| now.duration_since(**last).as_secs() >= idle_minutes * 60)
        .map(|(engine, _)| engine.clone())
        .collect()
}

/// One pass: stop whatever has gone quiet, unless anything at all is still in flight.
pub async fn sweep(state: &crate::state::AppState) {
    use bson::{doc, Document};

    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.ok().flatten()
        .map(|d| crate::store::doc_to_json(&d))
        .unwrap_or_default();

    let minutes = settings["kaggle_idle_stop_minutes"].as_u64()
        .or_else(|| settings["kaggle_idle_stop_minutes"].as_str().and_then(|s| s.trim().parse().ok()))
        .unwrap_or(DEFAULT_IDLE_MINUTES);
    if minutes == 0 { return; }  // switched off.

    // Anything queued or running means somebody is mid-workflow. A job that has not reached its
    // generation step yet is still about to need a server, so this checks all jobs rather than only
    // the ones already talking to an engine.
    let busy = state.db.collection::<Document>("jobs")
        .count_documents(doc! { "status": { "$in": ["queued", "running"] } })
        .await.unwrap_or(0);
    if busy > 0 {
        // Not merely "do not stop" — the clock is pushed forward, so a long run cannot age out.
        if let Ok(m) = seen().lock() {
            let engines: Vec<String> = m.keys().cloned().collect();
            drop(m);
            for eng in engines { touch(&eng); }
        }
        return;
    }

    for engine in stale(minutes, Instant::now()) {
        match crate::commands::stop_kaggle_server(engine.clone()).await {
            Ok(r) if r["ok"].as_bool().unwrap_or(false) => {
                println!("idle: stopped the {engine} server after {minutes} idle minutes — its GPU slot is released.");
                forget(&engine);
            }
            Ok(r) => {
                eprintln!("idle: could not stop {engine}: {}",
                          r["detail"].as_str().unwrap_or("no detail"));
                // Left in the map on purpose: the next sweep tries again rather than leaving a
                // session running because one attempt failed.
            }
            Err(err) => eprintln!("idle: could not stop {engine}: {err}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn an_engine_nobody_has_used_is_never_a_candidate() {
        // Nothing touched: nothing to stop. The map starts empty, so a server somebody else started
        // outside this session is left alone rather than stopped from under them.
        assert!(stale(0, Instant::now()).iter().all(|e| e != "never-touched"));
    }

    #[test]
    fn a_recently_used_engine_is_left_alone() {
        touch("comfyui-recent");
        let out = stale(25, Instant::now());
        assert!(!out.contains(&"comfyui-recent".to_string()),
                "just used, so far from idle");
        forget("comfyui-recent");
    }

    #[test]
    fn an_engine_past_the_timeout_is_offered_up() {
        touch("acestep-old");
        // Ask as if half an hour had passed, rather than waiting half an hour.
        let later = Instant::now() + Duration::from_secs(30 * 60);
        assert!(stale(25, later).contains(&"acestep-old".to_string()));
        forget("acestep-old");
    }

    #[test]
    fn touching_again_resets_the_clock() {
        touch("flux-busy");
        let later = Instant::now() + Duration::from_secs(30 * 60);
        assert!(stale(25, later).contains(&"flux-busy".to_string()));
        touch("flux-busy");
        assert!(!stale(25, Instant::now()).contains(&"flux-busy".to_string()),
                "a fresh touch must save it — this is what keeps a long run from ageing out");
        forget("flux-busy");
    }
}
