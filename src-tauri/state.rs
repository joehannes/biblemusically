use mongodb::{Client, Database};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Shared application state injected into every Tauri command via `tauri::State`.
pub struct AppState {
    pub db: Database,
    /// Simple in-memory queue of job IDs that are pending dispatch.
    /// The background worker drains this and calls `run_job`.
    pub job_queue: Arc<Mutex<Vec<String>>>,
}

impl AppState {
    pub async fn new() -> anyhow::Result<Self> {
        // Load .env from the crate root so dev overrides work out of the box.
        let _ = dotenvy::dotenv();

        let mongo_url = std::env::var("MONGO_URL")
            .unwrap_or_else(|_| "mongodb://localhost:27017".into());
        let db_name = std::env::var("DB_NAME")
            .unwrap_or_else(|_| "studio".into());

        let client = Client::with_uri_str(&mongo_url).await?;
        let db = client.database(&db_name);

        Ok(Self {
            db,
            job_queue: Arc::new(Mutex::new(Vec::new())),
        })
    }
}
