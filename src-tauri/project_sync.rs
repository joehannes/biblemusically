//! Keeps each project's JSON data committed to that project's own git repository.
//!
//! The store (see `store.rs`) writes a project's songs/sections/characters/uploads as JSON files
//! inside `<project_folder>/data/`, and the project folder is a git repo. This module closes the
//! loop: it initializes that repo, and a background sweep commits whatever changed — so a project's
//! full history is `git log`, restoring an old state is `git checkout`, and the whole project
//! (data + media) is a directory you can copy, back up, or push to a remote (see
//! `commands/remote_sync.rs`).
//!
//! Commits are **debounced, not immediate**: the store flags a project dirty on every write, and the
//! sweep runs on a timer. A generation run touches the same files dozens of times a minute, and one
//! commit per write would bury the useful history in noise.

use crate::store::Db;
use bson::{doc, Document};
use std::path::{Path, PathBuf};

type Res<T> = std::result::Result<T, String>;

/// Media patterns tracked by git-LFS when LFS is available — the generated assets that make a
/// project repo big. Kept in sync with what `commands/remote_sync.rs` expects a remote to hold.
pub const LFS_PATTERNS: &[&str] = &[
    "*.mp3", "*.wav", "*.flac", "*.m4a", "*.ogg", "*.opus", "*.mp4", "*.mov", "*.mkv", "*.webm",
    "*.png", "*.jpg", "*.jpeg", "*.webp", "*.gif", "*.tif", "*.tiff", "*.psd", "*.zip", "*.tar",
    "*.tar.gz",
];

const GITIGNORE: &str = "\
# Scratch files the app writes while working — never worth versioning.
*.tmp
*.json.tmp
*.part
.DS_Store
Thumbs.db
tmp/
cache/
";

// ────────────────────────────────────────────────────────────────────────────
// git plumbing
// ────────────────────────────────────────────────────────────────────────────

#[cfg(desktop)]
pub fn git(dir: &Path, args: &[&str]) -> Res<String> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|err| format!("could not run git: {err}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(not(desktop))]
pub fn git(_dir: &Path, _args: &[&str]) -> Res<String> {
    // Mobile builds have no git binary; project history stays on the desktop instance.
    Err("git is not available in this build".into())
}

/// Is a `git` binary usable at all? Everything here degrades to a no-op when it isn't, because a
/// missing git must never break saving data — the JSON files are the source of truth, the repo is
/// history on top of them.
pub fn git_available() -> bool {
    #[cfg(desktop)]
    {
        std::process::Command::new("git")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(not(desktop))]
    {
        false
    }
}

pub fn lfs_available() -> bool {
    #[cfg(desktop)]
    {
        std::process::Command::new("git")
            .args(["lfs", "version"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(not(desktop))]
    {
        false
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Repo setup
// ────────────────────────────────────────────────────────────────────────────

/// Make `folder` a git repository with the app's conventions, idempotently. Safe to call on every
/// project touch.
pub fn ensure_repo(folder: &Path) -> Res<bool> {
    if !git_available() {
        return Ok(false);
    }
    std::fs::create_dir_all(folder).map_err(|err| err.to_string())?;
    let fresh = !folder.join(".git").exists();
    if fresh {
        git(folder, &["init"])?;
        // A stable default branch name, so remotes don't disagree about master/main.
        let _ = git(folder, &["symbolic-ref", "HEAD", "refs/heads/main"]);
        let _ = git(folder, &["config", "user.name", "Studio Lightkid"]);
        let _ = git(folder, &["config", "user.email", "studio@lightkid.local"]);
    }

    let gitignore = folder.join(".gitignore");
    if !gitignore.exists() {
        let _ = std::fs::write(&gitignore, GITIGNORE);
    }
    Ok(fresh)
}

/// Track the generated media patterns with git-LFS, so pushing a project to a remote that offers
/// LFS storage doesn't try to stuff multi-megabyte audio into ordinary git objects. No-op without
/// git-lfs installed: declaring an `lfs` filter that git can't run would break `git add` outright.
pub fn ensure_lfs(folder: &Path) -> Res<bool> {
    if !lfs_available() {
        return Ok(false);
    }
    let _ = git(folder, &["lfs", "install", "--local"]);
    let attributes = folder.join(".gitattributes");
    let existing = std::fs::read_to_string(&attributes).unwrap_or_default();
    let mut lines: Vec<String> = existing.lines().map(|l| l.to_string()).collect();
    let mut added = false;
    for pattern in LFS_PATTERNS {
        let already = lines.iter().any(|l| l.split_whitespace().next() == Some(pattern));
        if !already {
            lines.push(format!("{pattern} filter=lfs diff=lfs merge=lfs -text"));
            added = true;
        }
    }
    if added {
        std::fs::write(&attributes, format!("{}\n", lines.join("\n"))).map_err(|err| err.to_string())?;
    }
    Ok(added)
}

// ────────────────────────────────────────────────────────────────────────────
// Committing
// ────────────────────────────────────────────────────────────────────────────

/// Stage everything and commit, unless nothing changed. Returns the new commit's short hash, or
/// `None` when the tree was already clean.
pub fn commit_all(folder: &Path, message: &str) -> Res<Option<String>> {
    if !git_available() || !folder.join(".git").exists() {
        return Ok(None);
    }
    git(folder, &["add", "-A"])?;
    let status = git(folder, &["status", "--porcelain"])?;
    if status.trim().is_empty() {
        return Ok(None);
    }
    git(folder, &["commit", "-m", message])?;
    let hash = git(folder, &["rev-parse", "--short", "HEAD"]).unwrap_or_default();
    Ok(Some(hash.trim().to_string()))
}

/// Resolve a project's folder from the store.
pub async fn project_folder(db: &Db, project_id: &str) -> Option<PathBuf> {
    db.collection::<Document>("projects")
        .find_one(doc! { "id": project_id })
        .await
        .ok()
        .flatten()
        .and_then(|d| d.get_str("project_folder").ok().map(|s| s.to_string()))
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
}

/// Commit one project's pending data changes now.
pub async fn commit_project(db: &Db, project_id: &str, message: Option<String>) -> Res<Option<String>> {
    let Some(folder) = project_folder(db, project_id).await else {
        return Ok(None);
    };
    ensure_repo(&folder)?;
    let message = message.unwrap_or_else(|| {
        format!("data: autosave {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"))
    });
    commit_all(&folder, &message)
}

/// Commit every project the store flagged as changed since the last sweep. Called on a timer from
/// `lib.rs`, and directly after bulk operations that want their result versioned immediately.
pub async fn sweep(db: &Db) -> Vec<serde_json::Value> {
    let mut results = Vec::new();
    for project_id in db.take_dirty_projects().await {
        match commit_project(db, &project_id, None).await {
            Ok(Some(hash)) => results.push(serde_json::json!({ "project_id": project_id, "commit": hash })),
            Ok(None) => {}
            Err(err) => {
                // A failed commit must not lose the flag, or the change would never be versioned.
                db.mark_project_dirty(&project_id).await;
                results.push(serde_json::json!({ "project_id": project_id, "error": err }));
            }
        }
    }
    results
}

/// Start the background sweep. One commit per project per interval at most.
pub fn spawn_sweeper(db: Db, interval_secs: u64) {
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        tick.tick().await; // the first tick fires immediately; skip it
        loop {
            tick.tick().await;
            let done = sweep(&db).await;
            for entry in done {
                if let Some(err) = entry.get("error").and_then(|e| e.as_str()) {
                    eprintln!("project autosave failed: {err}");
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_repo_is_idempotent_and_writes_conventions() {
        if !git_available() {
            return; // nothing to assert on a machine without git
        }
        let dir = std::env::temp_dir().join(format!("lightkid-sync-{}", uuid::Uuid::new_v4()));
        assert!(ensure_repo(&dir).unwrap(), "first call initializes");
        assert!(dir.join(".git").exists());
        assert!(dir.join(".gitignore").exists());
        assert!(!ensure_repo(&dir).unwrap(), "second call is a no-op");

        std::fs::create_dir_all(dir.join("data")).unwrap();
        std::fs::write(dir.join("data").join("songs.json"), "{\"documents\":[]}").unwrap();
        let commit = commit_all(&dir, "test: first").unwrap();
        assert!(commit.is_some(), "a change produces a commit");
        assert!(commit_all(&dir, "test: nothing").unwrap().is_none(), "a clean tree does not");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
