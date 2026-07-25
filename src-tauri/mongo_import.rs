//! One-time importer that lifts the legacy MongoDB data into the JSON store, then gets out of the
//! way forever.
//!
//! The app used to persist everything in a bundled `mongod` sidecar. That sidecar is gone (it's a
//! native x86_64 server binary — it can't ship to Android, and it made the app's data opaque). Users
//! who ran an older build still have a WiredTiger data directory at
//! `<app_data>/db`, so their projects, songs, settings and channels must be carried over.
//!
//! **This module deliberately does not depend on the `mongodb` driver.** Keeping that dependency
//! just for a one-time import would have defeated the point of removing it. Instead it starts the
//! legacy `mongod` binary (if one can still be found) on a scratch port and talks to it with ~120
//! lines of the MongoDB wire protocol: a single `OP_MSG` opcode carrying `find` / `getMore` /
//! `listCollections` commands as BSON. `bson` is a pure serialization crate that cross-compiles
//! anywhere, so nothing platform-specific comes back in through the side door.
//!
//! The import is idempotent and non-destructive:
//! * it skips any document whose `_id`/`id` already exists in the JSON store, so running it twice
//!   cannot duplicate data;
//! * `projects` are imported first (so the store knows each project's folder), then `songs`, so
//!   project-scoped documents land in the right project repo rather than the global shard;
//! * the legacy directory is **never deleted** — it's marked with `.migrated-to-json` and reported
//!   in the UI, and only an explicit user action (`purge_legacy_mongo`) removes it.

use crate::store::Db;
use bson::{doc, Bson, Document};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

type Res<T> = std::result::Result<T, String>;

const MARKER: &str = ".migrated-to-json";
/// Import order matters: a project must exist before its songs (to resolve the project folder), and
/// a song before its sections/uploads (they reference it by `song_id`). Anything not listed here is
/// imported afterwards in whatever order the server reports.
const PRIORITY: &[&str] = &["projects", "songs"];

// ────────────────────────────────────────────────────────────────────────────
// Discovery
// ────────────────────────────────────────────────────────────────────────────

/// Where the retired sidecar kept its data.
pub fn legacy_db_dir(app_data: &Path) -> PathBuf { app_data.join("db") }

/// Does this look like a real WiredTiger data directory with content in it?
pub fn has_legacy_data(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    let catalog = dir.join("_mdb_catalog.wt");
    if !catalog.exists() {
        return false;
    }
    // At least one user collection file (system collections exist even in an empty database).
    std::fs::read_dir(dir)
        .map(|entries| {
            entries.flatten().any(|e| {
                e.file_name().to_string_lossy().starts_with("collection-")
            })
        })
        .unwrap_or(false)
}

pub fn already_migrated(dir: &Path) -> bool { dir.join(MARKER).exists() }

/// Find a `mongod` binary: the bundled sidecar (dev tree or installed bundle), an explicit
/// `MONGOD_PATH`, or one on `PATH`. Returns `None` when there is nothing to import *with* — which is
/// not an error, just "this machine can't read the old format".
pub fn find_mongod() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("MONGOD_PATH") {
        let p = PathBuf::from(explicit);
        if p.is_file() {
            return Some(p);
        }
    }

    fn scan(dir: &Path) -> Option<PathBuf> {
        let entries = std::fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "mongod" || name.starts_with("mongod-") {
                let path = entry.path();
                if path.is_file() {
                    return Some(path);
                }
            }
        }
        None
    }

    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(|p| p.to_path_buf());
        while let Some(d) = dir {
            roots.push(d.clone());
            roots.push(d.join("binaries"));
            roots.push(d.join("resources"));
            roots.push(d.join("_up_").join("binaries"));
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd.join("binaries"));
        roots.push(cwd.join("src-tauri").join("binaries"));
        roots.push(cwd.join("..").join("binaries"));
    }
    for root in roots {
        if let Some(found) = scan(&root) {
            return Some(found);
        }
    }
    which::which("mongod").ok()
}

// ────────────────────────────────────────────────────────────────────────────
// Minimal MongoDB wire protocol (OP_MSG only)
// ────────────────────────────────────────────────────────────────────────────

const OP_MSG: i32 = 2013;

struct Wire {
    stream: TcpStream,
    next_request_id: i32,
}

impl Wire {
    async fn connect(port: u16) -> Res<Self> {
        let stream = TcpStream::connect(("127.0.0.1", port))
            .await
            .map_err(|e| format!("cannot reach the legacy database on port {port}: {e}"))?;
        Ok(Wire { stream, next_request_id: 1 })
    }

    /// Send one command document and read one reply document.
    async fn command(&mut self, command: Document) -> Res<Document> {
        let body = bson::to_vec(&command).map_err(|e| format!("encoding command failed: {e}"))?;
        let total = 16 + 4 + 1 + body.len();
        let mut msg = Vec::with_capacity(total);
        msg.extend_from_slice(&(total as i32).to_le_bytes()); // messageLength
        msg.extend_from_slice(&self.next_request_id.to_le_bytes()); // requestID
        msg.extend_from_slice(&0i32.to_le_bytes()); // responseTo
        msg.extend_from_slice(&OP_MSG.to_le_bytes()); // opCode
        msg.extend_from_slice(&0u32.to_le_bytes()); // flagBits
        msg.push(0u8); // section kind 0: a single body document
        msg.extend_from_slice(&body);
        self.next_request_id = self.next_request_id.wrapping_add(1);

        self.stream.write_all(&msg).await.map_err(|e| format!("wire write failed: {e}"))?;
        self.stream.flush().await.map_err(|e| format!("wire flush failed: {e}"))?;

        let mut header = [0u8; 16];
        self.stream.read_exact(&mut header).await.map_err(|e| format!("wire read failed: {e}"))?;
        let length = i32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
        if length < 21 || length > 64 * 1024 * 1024 {
            return Err(format!("implausible reply length {length} from the legacy database"));
        }
        let mut rest = vec![0u8; length - 16];
        self.stream.read_exact(&mut rest).await.map_err(|e| format!("wire read failed: {e}"))?;

        // rest = flagBits(4) + payloadType(1) + document
        if rest.len() < 5 {
            return Err("truncated reply from the legacy database".into());
        }
        let doc = Document::from_reader(&rest[5..])
            .map_err(|e| format!("decoding reply failed: {e}"))?;
        let ok = doc.get_f64("ok").unwrap_or_else(|_| doc.get_i32("ok").map(|v| v as f64).unwrap_or(0.0));
        if ok != 1.0 {
            let msg = doc.get_str("errmsg").unwrap_or("unknown error");
            return Err(format!("legacy database rejected the command: {msg}"));
        }
        Ok(doc)
    }

    /// Read every document of a collection, following the server's cursor.
    async fn read_all(&mut self, db_name: &str, collection: &str) -> Res<Vec<Document>> {
        let mut out: Vec<Document> = Vec::new();
        let first = self
            .command(doc! {
                "find": collection,
                "filter": {},
                "batchSize": 500i32,
                "$db": db_name,
            })
            .await?;

        let mut cursor = first
            .get_document("cursor")
            .map_err(|e| format!("unexpected find reply: {e}"))?
            .clone();
        loop {
            let batch_key = if out.is_empty() && cursor.contains_key("firstBatch") {
                "firstBatch"
            } else {
                "nextBatch"
            };
            if let Ok(batch) = cursor.get_array(batch_key) {
                for item in batch {
                    if let Bson::Document(d) = item {
                        out.push(d.clone());
                    }
                }
            }
            let cursor_id = cursor.get_i64("id").unwrap_or(0);
            if cursor_id == 0 {
                break;
            }
            let more = self
                .command(doc! {
                    "getMore": cursor_id,
                    "collection": collection,
                    "batchSize": 500i32,
                    "$db": db_name,
                })
                .await?;
            cursor = more
                .get_document("cursor")
                .map_err(|e| format!("unexpected getMore reply: {e}"))?
                .clone();
        }
        Ok(out)
    }

    async fn collection_names(&mut self, db_name: &str) -> Res<Vec<String>> {
        let reply = self
            .command(doc! { "listCollections": 1, "nameOnly": true, "$db": db_name })
            .await?;
        let mut names = Vec::new();
        let mut cursor = reply.get_document("cursor").map_err(|e| e.to_string())?.clone();
        loop {
            for key in ["firstBatch", "nextBatch"] {
                if let Ok(batch) = cursor.get_array(key) {
                    for item in batch {
                        if let Bson::Document(d) = item {
                            if let Ok(name) = d.get_str("name") {
                                if !name.starts_with("system.") {
                                    names.push(name.to_string());
                                }
                            }
                        }
                    }
                }
            }
            let id = cursor.get_i64("id").unwrap_or(0);
            if id == 0 {
                break;
            }
            let more = self
                .command(doc! { "getMore": id, "collection": "$cmd.listCollections", "$db": db_name })
                .await?;
            cursor = more.get_document("cursor").map_err(|e| e.to_string())?.clone();
        }
        names.sort();
        Ok(names)
    }

    async fn database_names(&mut self) -> Res<Vec<String>> {
        let reply = self.command(doc! { "listDatabases": 1, "nameOnly": true, "$db": "admin" }).await?;
        let mut names = Vec::new();
        if let Ok(list) = reply.get_array("databases") {
            for item in list {
                if let Bson::Document(d) = item {
                    if let Ok(name) = d.get_str("name") {
                        if !matches!(name, "admin" | "local" | "config") {
                            names.push(name.to_string());
                        }
                    }
                }
            }
        }
        Ok(names)
    }
}

// ────────────────────────────────────────────────────────────────────────────
// The import
// ────────────────────────────────────────────────────────────────────────────

/// Run the import if — and only if — there's legacy data that hasn't been imported yet.
/// Returns a report either way; `status` says what happened.
#[cfg(desktop)]
pub async fn import_if_needed(db: &Db, app_data: &Path) -> Value {
    let dir = legacy_db_dir(app_data);
    if !has_legacy_data(&dir) {
        return json!({ "status": "nothing_to_migrate", "legacy_dir": dir.to_string_lossy() });
    }
    if already_migrated(&dir) {
        let previous = std::fs::read_to_string(dir.join(MARKER))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok());
        return json!({
            "status": "already_migrated",
            "legacy_dir": dir.to_string_lossy(),
            "report": previous,
        });
    }
    match import(db, &dir).await {
        Ok(report) => report,
        Err(err) => json!({
            "status": "failed",
            "error": err,
            "legacy_dir": dir.to_string_lossy(),
            "hint": "The old data is untouched. Fix the cause and retry from Settings → Data.",
        }),
    }
}

#[cfg(not(desktop))]
pub async fn import_if_needed(_db: &Db, _app_data: &Path) -> Value {
    // Mobile builds never had a `mongod` sidecar to import from.
    json!({ "status": "not_applicable" })
}

/// Import every legacy database/collection into the JSON store.
#[cfg(desktop)]
pub async fn import(db: &Db, dir: &Path) -> Res<Value> {
    let mongod = find_mongod().ok_or_else(|| {
        "Legacy database files exist, but no `mongod` binary was found to read them. Set MONGOD_PATH \
         to a mongod executable and retry, or keep using an older build to export your data."
            .to_string()
    })?;

    let port = free_port()?;
    let mut child = std::process::Command::new(&mongod)
        .arg("--dbpath")
        .arg(dir)
        .arg("--port")
        .arg(port.to_string())
        .arg("--bind_ip")
        .arg("127.0.0.1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start {} for the one-time import: {e}", mongod.display()))?;

    let outcome = drain_legacy(db, port).await;

    // Always stop the server again, whatever happened.
    let _ = shutdown(port).await;
    std::thread::sleep(std::time::Duration::from_millis(400));
    let _ = child.kill();
    let _ = child.wait();

    let mut report = outcome?;
    report["legacy_dir"] = json!(dir.to_string_lossy());
    report["mongod_used"] = json!(mongod.to_string_lossy());
    report["status"] = json!("migrated");

    // Leave a marker so this never runs twice, and so the UI can explain what happened.
    let _ = std::fs::write(
        dir.join(MARKER),
        serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".into()),
    );
    Ok(report)
}

#[cfg(desktop)]
async fn drain_legacy(db: &Db, port: u16) -> Res<Value> {
    wait_until_ready(port).await?;
    let mut wire = Wire::connect(port).await?;

    let mut db_names = wire.database_names().await?;
    // The app's own database first, in case a stray one exists.
    let primary = std::env::var("DB_NAME").unwrap_or_else(|_| "studio".into());
    db_names.sort_by_key(|n| if *n == primary { 0 } else { 1 });

    let mut imported = serde_json::Map::new();
    let mut skipped = serde_json::Map::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut total = 0u64;

    for db_name in &db_names {
        let mut names = wire.collection_names(db_name).await?;
        names.sort_by_key(|n| PRIORITY.iter().position(|p| p == n).unwrap_or(PRIORITY.len()));

        for coll in names {
            let docs = match wire.read_all(db_name, &coll).await {
                Ok(d) => d,
                Err(err) => {
                    warnings.push(format!("{db_name}.{coll}: {err}"));
                    continue;
                }
            };
            let target = db.collection::<Document>(&coll);

            // Existing identities, so a re-run can't duplicate anything.
            let existing: std::collections::HashSet<String> = {
                let mut set = std::collections::HashSet::new();
                if let Ok(mut cursor) = target.find(Document::new()).await {
                    use futures_util::StreamExt;
                    while let Some(Ok(d)) = cursor.next().await {
                        if let Some(key) = identity(&d) {
                            set.insert(key);
                        }
                    }
                }
                set
            };

            let mut wrote = 0u64;
            let mut dupes = 0u64;
            for legacy in docs {
                match identity(&legacy) {
                    Some(key) if existing.contains(&key) => {
                        dupes += 1;
                        continue;
                    }
                    _ => {}
                }
                match target.insert_one(&legacy).await {
                    Ok(_) => {
                        wrote += 1;
                        total += 1;
                    }
                    Err(err) => warnings.push(format!("{db_name}.{coll}: {err}")),
                }
            }
            let key = if db_name == &primary { coll.clone() } else { format!("{db_name}.{coll}") };
            imported.insert(key.clone(), json!(wrote));
            if dupes > 0 {
                skipped.insert(key, json!(dupes));
            }
        }
    }

    Ok(json!({
        "databases": db_names,
        "documents_imported": total,
        "per_collection": imported,
        "already_present_skipped": skipped,
        "warnings": warnings,
        "finished_at": chrono::Utc::now().to_rfc3339(),
    }))
}

/// A document's stable identity for de-duplication: `_id` first (Mongo's own key), else `id`.
fn identity(d: &Document) -> Option<String> {
    for key in ["_id", "id"] {
        if let Some(v) = d.get(key) {
            return Some(match v {
                Bson::String(s) => s.clone(),
                Bson::ObjectId(o) => o.to_hex(),
                other => other.to_string(),
            });
        }
    }
    None
}

#[cfg(desktop)]
fn free_port() -> Res<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("could not reserve a local port: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    drop(listener);
    Ok(port)
}

#[cfg(desktop)]
async fn wait_until_ready(port: u16) -> Res<()> {
    // A cold WiredTiger directory can need a while to recover its journal.
    for _ in 0..120 {
        if let Ok(mut wire) = Wire::connect(port).await {
            if wire.command(doc! { "ping": 1, "$db": "admin" }).await.is_ok() {
                return Ok(());
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    Err("the legacy database did not become reachable within 60s".into())
}

#[cfg(desktop)]
async fn shutdown(port: u16) -> Res<()> {
    let mut wire = Wire::connect(port).await?;
    // The server closes the socket instead of replying, so an error here is the expected outcome.
    let _ = wire.command(doc! { "shutdown": 1, "force": true, "$db": "admin" }).await;
    Ok(())
}

/// Delete the legacy directory. Only ever called from an explicit user action in Settings → Data,
/// and only after a successful import.
pub fn purge(dir: &Path) -> Res<u64> {
    if !already_migrated(dir) {
        return Err("refusing to delete legacy data that has not been imported yet".into());
    }
    let freed = dir_size(dir);
    std::fs::remove_dir_all(dir).map_err(|e| format!("could not remove {}: {e}", dir.display()))?;
    Ok(freed)
}

pub fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            match entry.metadata() {
                Ok(m) if m.is_dir() => total += dir_size(&entry.path()),
                Ok(m) => total += m.len(),
                Err(_) => {}
            }
        }
    }
    total
}
