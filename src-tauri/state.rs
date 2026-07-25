use crate::store::Db;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};

const DEFAULT_MAX_CONCURRENT_JOBS: usize = 2;

/// Shared application state injected into every Tauri command via `tauri::State`.
#[derive(Clone)]
pub struct AppState {
    /// The JSON file store (see `store.rs`). Named `db` because ~380 call sites read
    /// `state.db.collection::<Document>(…)`; it is a directory of JSON files, not a database
    /// server — the bundled `mongod` sidecar it replaced is gone.
    pub db: Db,
    /// Bounds how many jobs (Suno/Midjourney/FFmpeg/YouTube upload/etc.) run at once. Each job
    /// task acquires a permit before doing real work and holds it for its whole run; a job stays
    /// visibly "queued" in the Jobs Monitor until a permit frees up. Sized from the
    /// `max_concurrent_jobs` setting at startup (see `AppState::new`) — this was previously a
    /// `job_queue: Arc<Mutex<Vec<String>>>` field that nothing ever drained (jobs.rs::enqueue
    /// spawned immediately, unbounded); a `Semaphore` is the real implementation of what that
    /// field's doc comment always claimed to do.
    pub job_semaphore: Arc<Semaphore>,
    /// Job IDs a user has requested cancellation for. Checked by `jobs::run_job` (at start and
    /// after completion) and polled inside the long-running loops of the Suno/YouTube-upload
    /// integrations so a cancel takes effect quickly instead of only relabeling the result after
    /// the job would have finished anyway. Entries are removed once observed/consumed.
    pub cancelled_jobs: Arc<Mutex<HashSet<String>>>,
}

impl AppState {
    pub async fn new() -> anyhow::Result<Self> {
        // Load .env from the crate root so dev overrides work out of the box.
        let _ = dotenvy::dotenv();

        // All persistence is JSON files under one directory. `STUDIO_DATA_DIR` overrides it (useful
        // for tests, portable installs, or pointing several builds at the same data).
        let root = match std::env::var("STUDIO_DATA_DIR") {
            Ok(p) if !p.trim().is_empty() => PathBuf::from(p),
            _ => Db::default_root().map_err(|e| anyhow::anyhow!(e.to_string()))?,
        };
        let db = Db::open(root).map_err(|e| anyhow::anyhow!(e.to_string()))?;

        // Read the desired concurrency cap from settings (falls back to the default if unset,
        // non-numeric, or no settings document exists yet). Changing this setting takes effect
        // on the next app restart, since a Semaphore's permit count is fixed at construction.
        let max_concurrent = db
            .collection::<bson::Document>("settings")
            .find_one(bson::doc! { "_id": "singleton" })
            .await
            .ok()
            .flatten()
            .and_then(|d| crate::store::get_num(&d, "max_concurrent_jobs"))
            .filter(|v| *v > 0)
            .map(|v| v as usize)
            .unwrap_or(DEFAULT_MAX_CONCURRENT_JOBS);

        Ok(Self {
            db,
            job_semaphore: Arc::new(Semaphore::new(max_concurrent)),
            cancelled_jobs: Arc::new(Mutex::new(HashSet::new())),
        })
    }
}
