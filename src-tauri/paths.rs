//! Where the app is allowed to keep its own files.
//!
//! Each of these was a direct `dirs::*` call. That is correct on a desktop and fatal on a phone.
//! Android has no XDG layout and no home directory: `dirs::config_dir()` resolves through a passwd
//! entry whose home is `/`, so the store's data directory came out as `/.config/studio-lightkid/data`
//! — a path under the read-only root, which no application may create. Opening the store was the
//! first thing startup did, the failure was read as "the data folder is unusable", and `run()`
//! answered it with `std::process::exit(1)`.
//!
//! That is why the first Android build installed and then did nothing when tapped. There was no
//! crash dialog and no "app keeps stopping", because nothing crashed — the process decided, in its
//! first few milliseconds, that it had nowhere to write, and left. From the launcher it is
//! indistinguishable from an app that never started.
//!
//! So the platform's own answer is installed over the top at startup; see [`install`], called from
//! the setup hook before anything reads a path. Desktop installs nothing and keeps resolving through
//! `dirs`, deliberately: those are the directories every existing installation already keeps its
//! data in, and quietly answering differently would look exactly like data loss.

use std::path::PathBuf;
use std::sync::OnceLock;

static CONFIG: OnceLock<PathBuf> = OnceLock::new();
static CACHE: OnceLock<PathBuf> = OnceLock::new();
static DOCUMENTS: OnceLock<PathBuf> = OnceLock::new();
static DOWNLOADS: OnceLock<PathBuf> = OnceLock::new();

/// Point the lookups below at directories the platform has granted this app.
///
/// Only the first call counts, and the setup hook is the only caller. Later calls are ignored
/// rather than rejected — a second one would mean the store had already opened somewhere else,
/// and moving the answer underneath it is worse than keeping it.
pub fn install(config: PathBuf, cache: PathBuf, documents: PathBuf, downloads: PathBuf) {
    let _ = CONFIG.set(config);
    let _ = CACHE.set(cache);
    let _ = DOCUMENTS.set(documents);
    let _ = DOWNLOADS.set(downloads);
}

/// Long-lived app-private data: the JSON store, the vault, learnings, style samples.
pub fn config_dir() -> Option<PathBuf> {
    CONFIG.get().cloned().or_else(dirs::config_dir)
}

/// Regenerable files the OS may delete when it needs the space.
pub fn cache_dir() -> Option<PathBuf> {
    CACHE.get().cloned().or_else(dirs::cache_dir)
}

/// Where a project folder goes — the one location here the user is expected to open themselves.
pub fn document_dir() -> Option<PathBuf> {
    DOCUMENTS.get().cloned().or_else(dirs::document_dir)
}

/// Where a downloaded update lands.
pub fn download_dir() -> Option<PathBuf> {
    DOWNLOADS.get().cloned().or_else(dirs::download_dir)
}
