//! Push a whole project — its JSON data *and* its generated media — to a free remote, so a project
//! survives a dead laptop and can be picked up on another machine.
//!
//! Since the store keeps each project in its own git repo (see `project_sync.rs`), "backup" is just
//! "push". The only real question is where the *large files* go, because generated audio, images and
//! video are what actually consume storage. Two strategies, both free, selectable per project:
//!
//! * **`lfs`** — media stays in the repo, tracked with git-LFS, and the host stores it.
//! * **`offload`** — media is uploaded to an asset host, the repo keeps only a manifest
//!   (`data/assets.json`: path → remote URL + SHA-256 + size), and the media is git-ignored. This
//!   keeps the repo tiny and works even on hosts with strict LFS quotas.
//!
//! # Free tiers, as of July 2026 (checked against each provider's own docs)
//!
//! | Provider | Free storage | LFS | Notes |
//! |---|---|---|---|
//! | Hugging Face | 100 GB private, "best-effort" (generous) public | native, 500 GB/file hard cap | the default: by far the largest free large-file allowance, and git-LFS is how the Hub works |
//! | GitLab.com | 10 GiB per project (repo + LFS combined) | yes | project goes read-only past the limit |
//! | GitHub | repo itself is free; LFS only 1 GB storage + 1 GB/month bandwidth | yes, small | fine for `offload` mode, tight for `lfs` |
//! | Codeberg | 750 MiB git + 1.5 GiB LFS/packages | yes | non-profit FOSS host — use `offload` and keep media elsewhere |
//! | Internet Archive | free, effectively unlimited — **public** | n/a (asset host) | for media you're happy to publish |
//! | Custom | whatever you self-host | maybe | any git URL, e.g. a Forgejo/Gitea box |
//!
//! Tokens never touch the JSON store or `.git/config`: they live in the encrypted vault
//! (`vault.rs`) and are handed to git through a credential helper reading an environment variable,
//! so they stay out of the repo, out of command lines visible to `ps`, and out of logs.

use crate::state::AppState;
use crate::{project_sync, vault};
use bson::{doc, Document};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tauri::State;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

/// Vault key holding a provider's access token.
fn token_key(provider: &str) -> String { format!("remote.{provider}.token") }
/// Vault key holding an asset host's secret (Internet Archive's S3 secret key).
fn asset_secret_key(provider: &str) -> String { format!("asset.{provider}.secret") }
fn asset_access_key(provider: &str) -> String { format!("asset.{provider}.access") }

// ────────────────────────────────────────────────────────────────────────────
// Catalog
// ────────────────────────────────────────────────────────────────────────────

/// The provider facts, surfaced to the UI so the user picks with the trade-offs in front of them
/// rather than from a bare dropdown.
#[tauri::command]
pub async fn list_sync_providers() -> Res<Value> {
    Ok(json!({
        "git": [
            {
                "id": "huggingface", "label": "Hugging Face", "host": "huggingface.co",
                "recommended": true,
                "free_storage": "100 GB private (free account); public repos are best-effort and generous",
                "lfs": "Native — the Hub is git-LFS. Hard cap 500 GB per file, <200 GB recommended.",
                "token_url": "https://huggingface.co/settings/tokens",
                "token_hint": "Create a token with WRITE permission.",
                "best_for": "Everything, including media in LFS — the largest free large-file allowance.",
                "caveats": "Public repos are meant to be useful to others; keep personal project data private."
            },
            {
                "id": "gitlab", "label": "GitLab.com", "host": "gitlab.com",
                "free_storage": "10 GiB per project (repository + LFS combined)",
                "lfs": "Yes, inside the same 10 GiB.",
                "token_url": "https://gitlab.com/-/user_settings/personal_access_tokens",
                "token_hint": "Personal access token with the `api` and `write_repository` scopes.",
                "best_for": "A private mirror with a comfortable, predictable quota.",
                "caveats": "The project becomes read-only once it exceeds 10 GiB."
            },
            {
                "id": "github", "label": "GitHub", "host": "github.com",
                "free_storage": "Repositories are free; git-LFS is only 1 GB storage + 1 GB/month bandwidth",
                "lfs": "Yes, but the free LFS quota is small.",
                "token_url": "https://github.com/settings/tokens",
                "token_hint": "Fine-grained token with Contents: read & write on the target repo.",
                "best_for": "JSON-only sync with media offloaded elsewhere.",
                "caveats": "Do not choose `lfs` mode here unless you have paid LFS data packs."
            },
            {
                "id": "codeberg", "label": "Codeberg", "host": "codeberg.org",
                "free_storage": "750 MiB git + 1.5 GiB LFS/packages/releases",
                "lfs": "Yes, within 1.5 GiB.",
                "token_url": "https://codeberg.org/user/settings/applications",
                "token_hint": "Access token with repository read/write.",
                "best_for": "A non-profit, ad-free home for the project's JSON.",
                "caveats": "A volunteer-run FOSS host — offload media rather than filling their storage."
            },
            {
                "id": "custom", "label": "Custom git remote", "host": "(your own)",
                "free_storage": "Whatever your server has",
                "lfs": "If your server supports it.",
                "token_hint": "Optional username + token/password for HTTPS auth.",
                "best_for": "Self-hosted Forgejo/Gitea/GitLab, or a NAS.",
                "caveats": null
            }
        ],
        "assets": [
            {
                "id": "internet_archive", "label": "Internet Archive", "host": "archive.org",
                "free_storage": "Free, effectively unlimited — but everything uploaded is PUBLIC",
                "token_url": "https://archive.org/account/s3.php",
                "token_hint": "Your archive.org S3-style access key and secret key.",
                "best_for": "Finished, publishable audio/video/artwork.",
                "caveats": "Public and hard to fully delete — never send drafts or anything private."
            },
            {
                "id": "lfs", "label": "Keep in the repo (git-LFS)", "host": "(your git provider)",
                "free_storage": "Your git provider's LFS quota",
                "best_for": "Hugging Face, where LFS is generous.",
                "caveats": "Uses your git host's storage."
            },
            {
                "id": "none", "label": "Local only", "host": "(this machine)",
                "free_storage": "Your disk",
                "best_for": "Sync the JSON only; media never leaves this computer.",
                "caveats": "Media is not backed up anywhere."
            }
        ]
    }))
}

// ────────────────────────────────────────────────────────────────────────────
// Config
// ────────────────────────────────────────────────────────────────────────────

/// Per-project sync settings live in the global `sync_configs` collection (`_id` = project id, or
/// `"global"` for the defaults a new project inherits). Secrets are *not* here — they're in the vault.
async fn load_config(state: &AppState, project_id: &str) -> Res<Value> {
    let coll = state.db.collection::<Document>("sync_configs");
    let global = coll.find_one(doc! { "_id": "global" }).await.map_err(e)?;
    let own = coll.find_one(doc! { "_id": project_id }).await.map_err(e)?;
    let mut merged = json!({
        "provider": "huggingface",
        "asset_provider": "lfs",
        "repo": Value::Null,
        "private": true,
        "auto_sync": false,
        "branch": "main",
    });
    for source in [global, own].into_iter().flatten() {
        let v = crate::store::doc_to_json(&source);
        if let (Some(dst), Some(src)) = (merged.as_object_mut(), v.as_object()) {
            for (k, val) in src {
                if k != "_id" && !val.is_null() {
                    dst.insert(k.clone(), val.clone());
                }
            }
        }
    }
    Ok(merged)
}

#[tauri::command]
pub async fn get_sync_config(state: State<'_, AppState>, project_id: String) -> Res<Value> {
    let mut cfg = load_config(&state, &project_id).await?;
    let provider = cfg["provider"].as_str().unwrap_or("huggingface").to_string();
    let asset_provider = cfg["asset_provider"].as_str().unwrap_or("lfs").to_string();
    cfg["has_token"] = json!(vault::get_quiet(&token_key(&provider)).is_some());
    cfg["has_asset_keys"] = json!(vault::get_quiet(&asset_secret_key(&asset_provider)).is_some());
    // Show what the repo currently knows, so the panel reflects reality and not just intent.
    if let Some(folder) = project_sync::project_folder(&state.db, &project_id).await {
        cfg["folder"] = json!(folder.to_string_lossy());
        cfg["remote_url"] = json!(current_remote(&folder));
        cfg["unpushed_commits"] = json!(unpushed_count(&folder));
    }
    Ok(cfg)
}

#[tauri::command]
pub async fn save_sync_config(
    state: State<'_, AppState>,
    project_id: String,
    config: Value,
) -> Res<Value> {
    let mut doc = bson::to_document(&config).map_err(e)?;
    doc.remove("_id");
    doc.remove("has_token");
    doc.remove("has_asset_keys");
    // A token that arrives in the config payload is moved straight into the vault and stripped.
    if let Some(token) = config.get("token").and_then(|t| t.as_str()).filter(|t| !t.trim().is_empty()) {
        let provider = config.get("provider").and_then(|p| p.as_str()).unwrap_or("huggingface");
        vault::put(&token_key(provider), token.trim())?;
        doc.remove("token");
    }
    state
        .db
        .collection::<Document>("sync_configs")
        .update_one(doc! { "_id": &project_id }, doc! { "$set": doc })
        .upsert(true)
        .await
        .map_err(e)?;
    get_sync_config(state, project_id).await
}

/// Store an asset host's key pair (Internet Archive's are two separate values).
#[tauri::command]
pub async fn save_asset_credentials(provider: String, access_key: String, secret_key: String) -> Res<Value> {
    vault::put(&asset_access_key(&provider), access_key.trim())?;
    vault::put(&asset_secret_key(&provider), secret_key.trim())?;
    Ok(json!({ "ok": true, "provider": provider }))
}

// ────────────────────────────────────────────────────────────────────────────
// Provider HTTP
// ────────────────────────────────────────────────────────────────────────────

fn client() -> Res<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("studio-lightkid")
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(e)
}

/// Who does this token belong to? Also the connection test — a provider that answers this is
/// reachable and the token is valid.
pub async fn whoami(provider: &str, token: &str) -> Res<Value> {
    let http = client()?;
    let (url, req) = match provider {
        "huggingface" => (
            "https://huggingface.co/api/whoami-v2".to_string(),
            http.get("https://huggingface.co/api/whoami-v2").bearer_auth(token),
        ),
        "gitlab" => (
            "https://gitlab.com/api/v4/user".to_string(),
            http.get("https://gitlab.com/api/v4/user").header("PRIVATE-TOKEN", token),
        ),
        "github" => (
            "https://api.github.com/user".to_string(),
            http.get("https://api.github.com/user")
                .bearer_auth(token)
                .header("Accept", "application/vnd.github+json"),
        ),
        "codeberg" => (
            "https://codeberg.org/api/v1/user".to_string(),
            http.get("https://codeberg.org/api/v1/user").header("Authorization", format!("token {token}")),
        ),
        "custom" => return Ok(json!({ "name": "(custom remote)", "provider": "custom" })),
        other => return Err(format!("unknown provider '{other}'")),
    };
    let resp = req.send().await.map_err(|err| format!("{url}: {err}"))?;
    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        let msg = body
            .get("error")
            .or_else(|| body.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("check the token and its scopes");
        return Err(format!("{provider} rejected the token ({status}): {msg}"));
    }
    let name = body
        .get("name")
        .or_else(|| body.get("username"))
        .or_else(|| body.get("login"))
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string();
    Ok(json!({ "ok": true, "provider": provider, "name": name, "raw": body }))
}

#[tauri::command]
pub async fn test_sync_connection(provider: String, token: Option<String>) -> Res<Value> {
    let token = match token.filter(|t| !t.trim().is_empty()) {
        Some(t) => {
            // Testing a freshly typed token also saves it — otherwise "test" and "use" could
            // disagree about which credential is in play.
            vault::put(&token_key(&provider), t.trim())?;
            t
        }
        None => vault::get(&token_key(&provider))?
            .ok_or_else(|| format!("No token stored for {provider} yet."))?,
    };
    whoami(&provider, &token).await
}

/// Create the remote repository if it doesn't exist, and return `(clone_url, web_url, owner)`.
async fn ensure_remote_repo(
    provider: &str,
    token: &str,
    repo: &str,
    private: bool,
) -> Res<(String, String, String)> {
    let http = client()?;
    // `repo` may be "name" or "owner/name".
    let (owner_hint, name) = match repo.split_once('/') {
        Some((o, n)) => (Some(o.to_string()), n.to_string()),
        None => (None, repo.to_string()),
    };
    let owner = match owner_hint {
        Some(o) => o,
        None => whoami(provider, token)
            .await?
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string(),
    };
    if owner.is_empty() {
        return Err("Could not determine the account name for the remote repository.".into());
    }

    match provider {
        "huggingface" => {
            // Datasets are the right repo type for "a pile of files with LFS".
            let resp = http
                .post("https://huggingface.co/api/repos/create")
                .bearer_auth(token)
                .json(&json!({ "name": name, "type": "dataset", "private": private }))
                .send()
                .await
                .map_err(e)?;
            let status = resp.status();
            let body: Value = resp.json().await.unwrap_or(Value::Null);
            // 409 = it already exists, which is exactly what we want on every run after the first.
            if !status.is_success() && status.as_u16() != 409 {
                return Err(format!(
                    "Hugging Face refused to create the repo ({status}): {}",
                    body.get("error").and_then(|x| x.as_str()).unwrap_or("unknown")
                ));
            }
            let url = format!("https://huggingface.co/datasets/{owner}/{name}");
            Ok((url.clone(), url, owner))
        }
        "gitlab" => {
            let resp = http
                .post("https://gitlab.com/api/v4/projects")
                .header("PRIVATE-TOKEN", token)
                .json(&json!({
                    "name": name,
                    "path": name,
                    "visibility": if private { "private" } else { "public" },
                }))
                .send()
                .await
                .map_err(e)?;
            let status = resp.status();
            let body: Value = resp.json().await.unwrap_or(Value::Null);
            if status.is_success() {
                let http_url = body
                    .get("http_url_to_repo")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .to_string();
                let web = body.get("web_url").and_then(|u| u.as_str()).unwrap_or("").to_string();
                return Ok((http_url, web, owner));
            }
            // Already taken → assume it's ours and carry on.
            let url = format!("https://gitlab.com/{owner}/{name}.git");
            Ok((url, format!("https://gitlab.com/{owner}/{name}"), owner))
        }
        "github" => {
            let resp = http
                .post("https://api.github.com/user/repos")
                .bearer_auth(token)
                .header("Accept", "application/vnd.github+json")
                .json(&json!({ "name": name, "private": private, "auto_init": false }))
                .send()
                .await
                .map_err(e)?;
            let _ = resp.status();
            let url = format!("https://github.com/{owner}/{name}.git");
            Ok((url, format!("https://github.com/{owner}/{name}"), owner))
        }
        "codeberg" => {
            let resp = http
                .post("https://codeberg.org/api/v1/user/repos")
                .header("Authorization", format!("token {token}"))
                .json(&json!({ "name": name, "private": private, "auto_init": false }))
                .send()
                .await
                .map_err(e)?;
            let _ = resp.status();
            let url = format!("https://codeberg.org/{owner}/{name}.git");
            Ok((url, format!("https://codeberg.org/{owner}/{name}"), owner))
        }
        "custom" => Ok((repo.to_string(), repo.to_string(), owner)),
        other => Err(format!("unknown provider '{other}'")),
    }
}

// ────────────────────────────────────────────────────────────────────────────
// git with credentials that stay out of everything
// ────────────────────────────────────────────────────────────────────────────

/// Run git with the token supplied through the environment, via an inline credential helper.
///
/// The token is deliberately **not** put in the remote URL (that would persist it in `.git/config`)
/// and **not** in an argument (that would expose it to any `ps` on the machine). The helper script
/// contains only variable names; the values arrive as environment variables, which on Linux are
/// readable only by the same user.
#[cfg(desktop)]
fn git_authed(dir: &Path, user: &str, token: &str, args: &[&str]) -> Res<String> {
    let helper = "!f() { echo username=$GIT_SYNC_USER; echo password=$GIT_SYNC_TOKEN; }; f";
    let mut cmd = std::process::Command::new("git");
    cmd.arg("-c").arg(format!("credential.helper={helper}"))
        .args(args)
        .current_dir(dir)
        .env("GIT_SYNC_USER", user)
        .env("GIT_SYNC_TOKEN", token)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "true");
    let out = cmd.output().map_err(|err| format!("could not run git: {err}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        // Belt and braces: never let a token reach a log or the UI, whatever git printed.
        Err(stderr.replace(token, "«token»"))
    }
}

#[cfg(not(desktop))]
fn git_authed(_dir: &Path, _user: &str, _token: &str, _args: &[&str]) -> Res<String> {
    Err("git is not available in this build".into())
}

fn current_remote(folder: &Path) -> Option<String> {
    project_sync::git(folder, &["remote", "get-url", "origin"])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// How many commits exist locally that the remote hasn't got. `None` when there's no tracking info.
fn unpushed_count(folder: &Path) -> Option<u64> {
    project_sync::git(folder, &["rev-list", "--count", "@{u}..HEAD"])
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

fn set_remote(folder: &Path, url: &str) -> Res<()> {
    if current_remote(folder).is_some() {
        project_sync::git(folder, &["remote", "set-url", "origin", url])?;
    } else {
        project_sync::git(folder, &["remote", "add", "origin", url])?;
    }
    Ok(())
}

// ────────────────────────────────────────────────────────────────────────────
// Sync
// ────────────────────────────────────────────────────────────────────────────

/// Push a project to its configured remote, creating the repository on first use.
#[tauri::command]
pub async fn sync_project_now(state: State<'_, AppState>, project_id: String) -> Res<Value> {
    let mut log: Vec<String> = Vec::new();
    let cfg = load_config(&state, &project_id).await?;
    let provider = cfg["provider"].as_str().unwrap_or("huggingface").to_string();
    let branch = cfg["branch"].as_str().unwrap_or("main").to_string();
    let private = cfg["private"].as_bool().unwrap_or(true);
    let asset_mode = cfg["asset_provider"].as_str().unwrap_or("lfs").to_string();

    let folder = project_sync::project_folder(&state.db, &project_id)
        .await
        .ok_or("This project has no folder on disk yet.")?;
    project_sync::ensure_repo(&folder)?;

    // Media handling has to be decided *before* committing, or the wrong things get staged.
    if asset_mode == "lfs" {
        if project_sync::ensure_lfs(&folder)? {
            log.push("Tracked media patterns with git-LFS.".into());
        }
    } else if asset_mode != "none" {
        let offloaded = offload_assets(&state, &project_id, &folder, &asset_mode).await?;
        log.push(format!(
            "Offloaded {} asset(s) to {asset_mode}; the repo keeps the manifest.",
            offloaded
        ));
        ignore_media(&folder)?;
    }

    if let Some(hash) = project_sync::commit_all(&folder, "sync: project data")? {
        log.push(format!("Committed {hash}."));
    } else {
        log.push("Nothing new to commit.".into());
    }

    let token = vault::get(&token_key(&provider))?.unwrap_or_default();
    if provider != "custom" && token.is_empty() {
        return Err(format!("No {provider} token stored yet — add one in Settings → Sync."));
    }

    let repo_name = cfg["repo"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| default_repo_name(&folder));

    let (clone_url, web_url, owner) = ensure_remote_repo(&provider, &token, &repo_name, private).await?;
    set_remote(&folder, &clone_url)?;
    log.push(format!("Remote: {clone_url}"));

    let user = match provider.as_str() {
        "gitlab" => "oauth2".to_string(),
        "github" => "x-access-token".to_string(),
        _ => owner.clone(),
    };
    let push_out = git_authed(&folder, &user, &token, &["push", "-u", "origin", &branch])
        .or_else(|err| {
            // A brand-new repo may not have the branch yet, or the default branch may differ.
            if err.contains("src refspec") || err.contains("does not match any") {
                git_authed(&folder, &user, &token, &["push", "-u", "origin", "HEAD"])
            } else {
                Err(err)
            }
        })?;
    if !push_out.trim().is_empty() {
        log.push(push_out.trim().to_string());
    }
    log.push("Pushed.".into());

    let now = chrono::Utc::now().to_rfc3339();
    state
        .db
        .collection::<Document>("sync_configs")
        .update_one(
            doc! { "_id": &project_id },
            doc! { "$set": { "last_sync": &now, "last_error": Option::<String>::None,
                             "remote_url": &web_url, "repo": &repo_name } },
        )
        .upsert(true)
        .await
        .map_err(e)?;

    Ok(json!({ "ok": true, "log": log, "remote_url": web_url, "last_sync": now }))
}

/// Restore/refresh from the remote — the other half of "my laptop died".
#[tauri::command]
pub async fn pull_project_now(state: State<'_, AppState>, project_id: String) -> Res<Value> {
    let cfg = load_config(&state, &project_id).await?;
    let provider = cfg["provider"].as_str().unwrap_or("huggingface").to_string();
    let branch = cfg["branch"].as_str().unwrap_or("main").to_string();
    let folder = project_sync::project_folder(&state.db, &project_id)
        .await
        .ok_or("This project has no folder on disk yet.")?;
    let token = vault::get(&token_key(&provider))?.unwrap_or_default();
    let user = match provider.as_str() {
        "gitlab" => "oauth2".to_string(),
        "github" => "x-access-token".to_string(),
        _ => cfg["repo"].as_str().unwrap_or("").split('/').next().unwrap_or("user").to_string(),
    };
    let out = git_authed(&folder, &user, &token, &["pull", "--ff-only", "origin", &branch])?;
    // The pull rewrote data/*.json under us.
    state.db.invalidate_cache().await;
    Ok(json!({ "ok": true, "output": out.trim() }))
}

fn default_repo_name(folder: &Path) -> String {
    folder
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("studio-project")
        .to_string()
}

/// Tell git to leave the big files alone once they live on an asset host.
fn ignore_media(folder: &Path) -> Res<()> {
    let path = folder.join(".gitignore");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    if existing.contains("# offloaded media") {
        return Ok(());
    }
    let mut patterns = vec!["".to_string(), "# offloaded media (see data/assets.json)".to_string()];
    patterns.extend(project_sync::LFS_PATTERNS.iter().map(|p| p.to_string()));
    std::fs::write(&path, format!("{existing}{}\n", patterns.join("\n"))).map_err(e)?;
    Ok(())
}

// ────────────────────────────────────────────────────────────────────────────
// Asset offloading
// ────────────────────────────────────────────────────────────────────────────

fn sha256_file(path: &Path) -> Res<String> {
    let bytes = std::fs::read(path).map_err(e)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn media_files(folder: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let exts: Vec<&str> = project_sync::LFS_PATTERNS
        .iter()
        .filter_map(|p| p.strip_prefix("*."))
        .collect();
    fn walk(dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if name == ".git" || name == "data" {
                    continue;
                }
                walk(&path, exts, out);
            } else if let Some(ext) = path.extension().and_then(|x| x.to_str()) {
                if exts.iter().any(|e| e.eq_ignore_ascii_case(ext)) {
                    out.push(path);
                }
            }
        }
    }
    walk(folder, &exts, &mut out);
    out
}

/// Upload every not-yet-uploaded media file and record it in the project's `assets` manifest.
async fn offload_assets(
    state: &AppState,
    project_id: &str,
    folder: &Path,
    provider: &str,
) -> Res<usize> {
    let coll = state.db.collection::<Document>("assets");
    let mut uploaded = 0usize;
    for path in media_files(folder) {
        let rel = path.strip_prefix(folder).unwrap_or(&path).to_string_lossy().to_string();
        let digest = sha256_file(&path)?;
        // Already uploaded, unchanged? Then there is nothing to do.
        let existing = coll
            .find_one(doc! { "project_id": project_id, "path": &rel })
            .await
            .map_err(e)?;
        if let Some(prev) = &existing {
            if prev.get_str("sha256").unwrap_or("") == digest {
                continue;
            }
        }
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let url = match provider {
            "internet_archive" => upload_to_internet_archive(project_id, &rel, &path).await?,
            other => return Err(format!("unknown asset provider '{other}'")),
        };
        coll.update_one(
            doc! { "project_id": project_id, "path": &rel },
            doc! { "$set": {
                "project_id": project_id,
                "path": &rel,
                "provider": provider,
                "url": &url,
                "sha256": &digest,
                "size": size as i64,
                "uploaded_at": chrono::Utc::now().to_rfc3339(),
            }},
        )
        .upsert(true)
        .await
        .map_err(e)?;
        uploaded += 1;
    }
    Ok(uploaded)
}

/// Internet Archive's S3-compatible endpoint. Its "LOW" authorization scheme is a plain header pair,
/// not an AWS SigV4 signature, which is why this is a dozen lines rather than a hundred.
async fn upload_to_internet_archive(project_id: &str, rel_path: &str, file: &Path) -> Res<String> {
    let access = vault::get(&asset_access_key("internet_archive"))?
        .ok_or("No Internet Archive access key stored (Settings → Sync).")?;
    let secret = vault::get(&asset_secret_key("internet_archive"))?
        .ok_or("No Internet Archive secret key stored (Settings → Sync).")?;
    // One archive.org item per project; files keep their relative paths inside it.
    let identifier = format!("studio-lightkid-{}", &project_id[..project_id.len().min(24)]);
    let url = format!(
        "https://s3.us.archive.org/{identifier}/{}",
        urlencoding::encode(rel_path)
    );
    let bytes = std::fs::read(file).map_err(e)?;
    let resp = client()?
        .put(&url)
        .header("authorization", format!("LOW {access}:{secret}"))
        .header("x-archive-auto-make-bucket", "1")
        .header("x-archive-meta-mediatype", "data")
        .header("x-archive-meta-collection", "opensource_media")
        .header("x-archive-meta-title", format!("Studio Lightkid project {project_id}"))
        .body(bytes)
        .send()
        .await
        .map_err(e)?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Internet Archive upload failed ({status}): {}", body.trim()));
    }
    Ok(format!("https://archive.org/download/{identifier}/{rel_path}"))
}

/// The manifest, for the UI (and for a fresh clone that needs to fetch its media back).
#[tauri::command]
pub async fn list_project_assets(state: State<'_, AppState>, project_id: String) -> Res<Vec<Value>> {
    use futures_util::StreamExt;
    let mut cursor = state
        .db
        .collection::<Document>("assets")
        .find(doc! { "project_id": &project_id })
        .sort(doc! { "path": 1 })
        .await
        .map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await {
        out.push(crate::store::doc_to_json(&d));
    }
    Ok(out)
}

/// Download any manifest entry that isn't on disk — how a fresh clone gets its media back.
#[tauri::command]
pub async fn restore_project_assets(state: State<'_, AppState>, project_id: String) -> Res<Value> {
    let folder = project_sync::project_folder(&state.db, &project_id)
        .await
        .ok_or("This project has no folder on disk yet.")?;
    let assets = list_project_assets(state, project_id).await?;
    let http = client()?;
    let (mut restored, mut skipped, mut failed) = (0usize, 0usize, Vec::new());
    for asset in assets {
        let (Some(rel), Some(url)) = (asset["path"].as_str(), asset["url"].as_str()) else {
            continue;
        };
        let target = folder.join(rel);
        if target.exists() {
            skipped += 1;
            continue;
        }
        match http.get(url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let bytes = resp.bytes().await.map_err(e)?;
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(e)?;
                }
                std::fs::write(&target, &bytes).map_err(e)?;
                restored += 1;
            }
            Ok(resp) => failed.push(format!("{rel}: HTTP {}", resp.status())),
            Err(err) => failed.push(format!("{rel}: {err}")),
        }
    }
    Ok(json!({ "ok": true, "restored": restored, "already_present": skipped, "failed": failed }))
}

/// Sync every project that has auto-sync enabled. Called on a timer from `lib.rs`.
pub async fn auto_sync_sweep(state: &AppState) {
    use futures_util::StreamExt;
    let Ok(mut cursor) = state.db.collection::<Document>("sync_configs").find(doc! { "auto_sync": true }).await
    else {
        return;
    };
    let mut ids = Vec::new();
    while let Some(Ok(d)) = cursor.next().await {
        if let Ok(id) = d.get_str("_id") {
            if id != "global" {
                ids.push(id.to_string());
            }
        }
    }
    for project_id in ids {
        // Deliberately sequential: several concurrent LFS pushes would saturate an ordinary
        // connection and the providers rate-limit anyway.
        let result = do_auto_sync(state, &project_id).await;
        if let Err(err) = result {
            let _ = state
                .db
                .collection::<Document>("sync_configs")
                .update_one(doc! { "_id": &project_id }, doc! { "$set": { "last_error": &err } })
                .await;
            eprintln!("auto-sync for project {project_id} failed: {err}");
        }
    }
}

/// The body of an auto-sync, minus the Tauri `State` wrapper the command version needs.
async fn do_auto_sync(state: &AppState, project_id: &str) -> Res<()> {
    let folder = project_sync::project_folder(&state.db, project_id)
        .await
        .ok_or("no project folder")?;
    // Nothing local to send? Then don't wake the network at all.
    if project_sync::commit_all(&folder, "sync: project data")?.is_none()
        && unpushed_count(&folder) == Some(0)
    {
        return Ok(());
    }
    let cfg = load_config(state, project_id).await?;
    let provider = cfg["provider"].as_str().unwrap_or("huggingface").to_string();
    let branch = cfg["branch"].as_str().unwrap_or("main").to_string();
    let token = vault::get_quiet(&token_key(&provider)).unwrap_or_default();
    if token.is_empty() && provider != "custom" {
        return Err("no token stored".into());
    }
    let user = match provider.as_str() {
        "gitlab" => "oauth2",
        "github" => "x-access-token",
        _ => "user",
    };
    git_authed(&folder, user, &token, &["push", "origin", &branch])?;
    let now = chrono::Utc::now().to_rfc3339();
    let _ = state
        .db
        .collection::<Document>("sync_configs")
        .update_one(
            doc! { "_id": project_id },
            doc! { "$set": { "last_sync": &now, "last_error": Option::<String>::None } },
        )
        .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_walk_finds_assets_but_skips_git_and_data() {
        let root = std::env::temp_dir().join(format!("lightkid-assets-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("auto/english")).unwrap();
        std::fs::create_dir_all(root.join(".git/objects")).unwrap();
        std::fs::create_dir_all(root.join("data")).unwrap();
        std::fs::write(root.join("auto/english/song.mp3"), b"audio").unwrap();
        std::fs::write(root.join("auto/english/cover.png"), b"image").unwrap();
        std::fs::write(root.join("auto/english/notes.txt"), b"text").unwrap();
        std::fs::write(root.join(".git/objects/blob.mp3"), b"nope").unwrap();
        std::fs::write(root.join("data/songs.json"), b"{}").unwrap();

        let mut found: Vec<String> = media_files(&root)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        found.sort();
        assert_eq!(found, vec!["cover.png", "song.mp3"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sha256_is_content_addressed() {
        let dir = std::env::temp_dir().join(format!("lightkid-sha-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.bin");
        let b = dir.join("b.bin");
        std::fs::write(&a, b"same").unwrap();
        std::fs::write(&b, b"same").unwrap();
        assert_eq!(sha256_file(&a).unwrap(), sha256_file(&b).unwrap());
        std::fs::write(&b, b"different").unwrap();
        assert_ne!(sha256_file(&a).unwrap(), sha256_file(&b).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
