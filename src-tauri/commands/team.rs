use crate::state::AppState;
use bson::{doc, Document};
use serde_json::{json, Map, Value};
use tauri::State;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

fn bson_to_value(doc: Document) -> Value {
    let mut m = serde_json::Map::new();
    for (k, v) in doc {
        if k == "_id" { continue; }
        if let Ok(jv) = bson::from_bson::<Value>(v) { m.insert(k, jv); }
    }
    Value::Object(m)
}

// ────────────────────────────────────────────────────────────────
// Users, shared projects, and merging
//
// Identity is a social sign-in plus a username chosen once. No passwords are stored here at all — not
// hashed, not salted, none — because a password this app kept would be one more thing to lose, and the
// user already has a Google account. The username exists so people are addressable by something they
// picked rather than by an email address.
//
// Sharing is a shared git remote. That is the honest architecture for this store: the project already
// *is* a git repo, so two people pulling and pushing the same repo is the mechanism, not a bolt-on.
//
// THE HONEST LIMIT, stated everywhere it matters: **roles here are advisory.** Anyone with push access
// to the remote can push anything, whatever this app displays. Real enforcement belongs to the host —
// GitHub teams, branch protection — and the app says so rather than implying a guarantee it cannot
// keep. What the app *can* do is refuse to let its own UI perform a write the local user's role does
// not cover, which stops accidents, not adversaries.
//
// Merging is three-way on the JSON itself. Last-writer-wins is not offered, because it loses somebody's
// work invisibly: whoever pushed second silently erases the other's afternoon. Where both sides changed
// the same field, that field becomes a conflict for a person to settle, and everything else merges.
//
// Rewinding never rewrites history. Restoring an old state writes a NEW commit that contains the old
// tree, so "someone destroyed something" is recoverable *and* the destruction stays visible — which is
// what makes it possible to talk about afterwards.
// ────────────────────────────────────────────────────────────────

/// What a role may do inside this app's own UI.
///
/// Coarse on purpose: three roles people can hold in their heads beat nine nobody reads.
pub fn role_allows(role: &str, action: &str) -> bool {
    match role {
        "owner" => true,
        "editor" => !matches!(action, "delete_project" | "manage_members" | "change_remote"),
        "viewer" => matches!(action, "read" | "pull" | "export"),
        // An unknown role is read-only rather than full access: failing closed is the only safe default
        // when the value came off a shared repo somebody else wrote.
        _ => matches!(action, "read"),
    }
}

/// Three-way merge of two JSON objects against their common ancestor.
///
/// Returns the merged value and the list of conflicting paths. A field only conflicts when **both**
/// sides changed it to *different* values — if only one side moved, that side wins, which is what makes
/// most collaboration merge cleanly without anybody being asked anything.
pub fn merge_json(base: &Value, mine: &Value, theirs: &Value) -> (Value, Vec<String>) {
    let mut conflicts = Vec::new();
    let merged = merge_at("", base, mine, theirs, &mut conflicts);
    (merged, conflicts)
}

fn merge_at(path: &str, base: &Value, mine: &Value, theirs: &Value, conflicts: &mut Vec<String>) -> Value {
    // Both objects: merge key by key, which is where the useful behaviour lives.
    if mine.is_object() && theirs.is_object() {
        let empty = Map::new();
        let bo = base.as_object().unwrap_or(&empty);
        let mo = mine.as_object().unwrap();
        let to = theirs.as_object().unwrap();
        let mut out = Map::new();
        let mut keys: Vec<&String> = mo.keys().chain(to.keys()).collect();
        keys.sort();
        keys.dedup();
        for k in keys {
            let child_path = if path.is_empty() { k.clone() } else { format!("{path}.{k}") };
            let b = bo.get(k).cloned().unwrap_or(Value::Null);
            let m = mo.get(k).cloned();
            let t = to.get(k).cloned();
            match (m, t) {
                (Some(m), Some(t)) => {
                    out.insert(k.clone(), merge_at(&child_path, &b, &m, &t, conflicts));
                }
                // Only one side has the key. Added by that side, or deleted by the other — either way
                // keeping it is the non-destructive answer, and a deletion someone wanted is one more
                // click while a deletion nobody wanted is lost work.
                (Some(m), None) => { out.insert(k.clone(), m); }
                (None, Some(t)) => { out.insert(k.clone(), t); }
                (None, None) => {}
            }
        }
        return Value::Object(out);
    }

    if mine == theirs { return mine.clone(); }
    if base == mine { return theirs.clone(); }      // only they moved
    if base == theirs { return mine.clone(); }      // only I moved

    // Both moved, differently. Keep mine in the tree so the file stays usable, and record the path so a
    // person settles it — never silently.
    conflicts.push(path.to_string());
    mine.clone()
}

/// Parse `git status --porcelain` into the files git could not merge.
pub fn conflicted_files(porcelain: &str) -> Vec<String> {
    porcelain.lines().filter_map(|line| {
        if line.len() < 4 { return None; }
        let code = &line[..2];
        // The unmerged states: both added, both modified, and the delete/modify pairs.
        let unmerged = matches!(code, "AA" | "UU" | "DU" | "UD" | "AU" | "UA" | "DD");
        if unmerged { Some(line[3..].trim().to_string()) } else { None }
    }).collect()
}

/// A share invitation: everything the other person needs, in one pasteable line.
///
/// Deliberately not a URL to a server — there is no server. It is the repo, the project, and the role,
/// so accepting it is a clone rather than a request to something that could be down.
pub fn invite_payload(project_id: &str, project_name: &str, remote: &str, role: &str) -> String {
    let body = json!({
        "v": 1, "project_id": project_id, "name": project_name, "remote": remote, "role": role,
    });
    format!("bmstudio-invite:{}", base64_encode(body.to_string().as_bytes()))
}

pub fn parse_invite(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    let b64 = trimmed.strip_prefix("bmstudio-invite:")?;
    let bytes = base64_decode(b64)?;
    let parsed: Value = serde_json::from_slice(&bytes).ok()?;
    // A remote is the one field without which nothing can happen.
    if parsed["remote"].as_str().unwrap_or("").is_empty() { return None; }
    Some(parsed)
}

fn base64_encode(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a') as u32 + 26,
            b'0'..=b'9' => (c - b'0') as u32 + 52,
            b'+' => 62, b'/' => 63,
            _ => return None,
        })
    };
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for &c in s.trim().as_bytes() {
        if c == b'=' || c.is_ascii_whitespace() { continue; }
        acc = (acc << 6) | val(c)?;
        bits += 6;
        if bits >= 8 { bits -= 8; out.push(((acc >> bits) & 0xFF) as u8); }
    }
    Some(out)
}

// ── Identity ────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct SignInRequest {
    /// "google" | "github" — where the identity came from.
    pub provider: String,
    pub email: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
    /// The username this person picked. Unique within this install.
    pub username: String,
}

/// Record the signed-in user and make them current.
///
/// No password is taken or stored. The provider proved who they are; the username is what the team
/// calls them.
#[tauri::command]
pub async fn sign_in(state: State<'_, AppState>, payload: SignInRequest) -> Res<Value> {
    let username = payload.username.trim().to_lowercase();
    if username.len() < 2 {
        return Err("Pick a username of at least two characters.".into());
    }
    if !username.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.') {
        return Err("A username can use letters, digits, dash, underscore and dot.".into());
    }
    if payload.email.trim().is_empty() {
        return Err("The sign-in did not return an email address.".into());
    }

    // Taken by somebody else on this install?
    if let Ok(Some(d)) = state.db.collection::<Document>("users")
        .find_one(doc! { "username": &username }).await {
        let existing = bson_to_value(d);
        if existing["email"].as_str() != Some(payload.email.trim()) {
            return Err(format!("The username '{username}' is already used here by somebody else."));
        }
    }

    let user = json!({
        "id": format!("{}:{}", payload.provider, payload.email.trim().to_lowercase()),
        "provider": payload.provider,
        "email": payload.email.trim().to_lowercase(),
        "username": username,
        "name": payload.name.unwrap_or_default(),
        "avatar": payload.avatar.unwrap_or_default(),
        "last_seen": crate::models::now_iso(),
    });
    let d = bson::to_document(&user).map_err(e)?;
    state.db.collection::<Document>("users")
        .update_one(doc! { "id": user["id"].as_str().unwrap_or("") }, doc! { "$set": d })
        .upsert(true).await.map_err(e)?;

    // The current session — one per install, since this is a desktop app somebody is sitting at.
    state.db.collection::<Document>("session")
        .update_one(doc! { "_id": "current" },
                    doc! { "$set": { "user_id": user["id"].as_str().unwrap_or(""),
                                     "since": crate::models::now_iso() } })
        .upsert(true).await.map_err(e)?;
    Ok(user)
}

#[tauri::command]
pub async fn current_user(state: State<'_, AppState>) -> Res<Value> {
    let session = state.db.collection::<Document>("session")
        .find_one(doc! { "_id": "current" }).await.map_err(e)?
        .map(bson_to_value);
    let Some(uid) = session.as_ref().and_then(|s| s["user_id"].as_str()).map(|s| s.to_string()) else {
        return Ok(json!({ "signed_in": false }));
    };
    let user = state.db.collection::<Document>("users")
        .find_one(doc! { "id": &uid }).await.map_err(e)?
        .map(bson_to_value);
    match user {
        Some(u) => Ok(json!({ "signed_in": true, "user": u })),
        None => Ok(json!({ "signed_in": false })),
    }
}

#[tauri::command]
pub async fn sign_out(state: State<'_, AppState>) -> Res<Value> {
    state.db.collection::<Document>("session")
        .delete_one(doc! { "_id": "current" }).await.map_err(e)?;
    // The user record stays: signing out is not leaving, and their name still appears in history.
    Ok(json!({ "signed_in": false }))
}

// ── Members ─────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct MemberRequest {
    pub project_id: String,
    /// Email or username — whichever the inviter knows.
    pub who: String,
    /// "owner" | "editor" | "viewer"
    pub role: String,
}

#[tauri::command]
pub async fn add_project_member(state: State<'_, AppState>, payload: MemberRequest) -> Res<Value> {
    if !["owner", "editor", "viewer"].contains(&payload.role.as_str()) {
        return Err("Role must be owner, editor or viewer.".into());
    }
    let member = json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "project_id": payload.project_id,
        "who": payload.who.trim().to_lowercase(),
        "role": payload.role,
        "added_at": crate::models::now_iso(),
    });
    let d = bson::to_document(&member).map_err(e)?;
    state.db.collection::<Document>("project_members")
        .update_one(doc! { "project_id": member["project_id"].as_str().unwrap_or(""),
                           "who": member["who"].as_str().unwrap_or("") },
                    doc! { "$set": d })
        .upsert(true).await.map_err(e)?;
    Ok(json!({
        "member": member,
        "advisory": "This role is what the app shows and what it stops its own buttons from doing. It \
                     cannot stop somebody with push access to the remote — that is the git host's job \
                     (GitHub teams, branch protection).",
    }))
}

#[tauri::command]
pub async fn list_project_members(state: State<'_, AppState>, project_id: String) -> Res<Value> {
    use futures_util::StreamExt;
    let mut out = Vec::new();
    let mut cursor = state.db.collection::<Document>("project_members")
        .find(doc! { "project_id": &project_id }).await.map_err(e)?;
    while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }
    Ok(json!({ "members": out }))
}

#[tauri::command]
pub async fn remove_project_member(state: State<'_, AppState>, project_id: String, who: String) -> Res<Value> {
    state.db.collection::<Document>("project_members")
        .delete_one(doc! { "project_id": &project_id, "who": who.trim().to_lowercase() })
        .await.map_err(e)?;
    Ok(json!({ "removed": who }))
}

/// What the signed-in user may do in this project, as this app's UI sees it.
#[tauri::command]
pub async fn my_project_role(state: State<'_, AppState>, project_id: String) -> Res<Value> {
    let me = current_user(state.clone()).await?;
    if me["signed_in"].as_bool() != Some(true) {
        // Nobody signed in: a single-user install, which is the normal case and gets full access.
        return Ok(json!({ "role": "owner", "reason": "no sign-in on this install" }));
    }
    let email = me["user"]["email"].as_str().unwrap_or("").to_string();
    let username = me["user"]["username"].as_str().unwrap_or("").to_string();
    let member = state.db.collection::<Document>("project_members")
        .find_one(doc! { "project_id": &project_id,
                         "$or": [ { "who": &email }, { "who": &username } ] })
        .await.map_err(e)?.map(bson_to_value);
    // Not listed means owner: the person who created the project locally is not a guest in it.
    let role = member.and_then(|m| m["role"].as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "owner".to_string());
    Ok(json!({
        "role": role,
        "can": {
            "read": role_allows(&role, "read"),
            "write": role_allows(&role, "write"),
            "manage_members": role_allows(&role, "manage_members"),
            "delete_project": role_allows(&role, "delete_project"),
        },
    }))
}

// ── Sharing ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn share_project(state: State<'_, AppState>, project_id: String, role: String) -> Res<Value> {
    let project = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &project_id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "project not found".to_string())?;
    let folder = crate::project_sync::project_folder(&state.db, &project_id).await
        .ok_or("This project has no folder on disk yet — save it once first.")?;
    let remote = crate::project_sync::git(&folder, &["remote", "get-url", "origin"])
        .unwrap_or_default().trim().to_string();
    if remote.is_empty() {
        return Err("This project has no git remote. Set one up in Data & Sync — sharing is a shared \
                    repo, so there has to be one for the other person to reach.".into());
    }
    let invite = invite_payload(
        &project_id,
        project["name"].as_str().unwrap_or("project"),
        &remote,
        &role,
    );
    Ok(json!({
        "invite": invite,
        "remote": remote,
        "role": role,
        "how": "Send that line to them. They paste it into Team → Accept an invitation, and the app \
                clones the repo.",
        "advisory": "They also need push access on the host itself if they are to write. The role in the \
                     invitation is what this app shows and enforces in its own UI; the host decides who \
                     can actually push.",
    }))
}

#[derive(serde::Deserialize)]
pub struct AcceptRequest {
    pub invite: String,
    /// Where to clone it. A folder that does not exist yet.
    pub folder: String,
}

/// Accept an invitation: clone the shared repo and register the project locally.
#[tauri::command]
pub async fn accept_invite(state: State<'_, AppState>, payload: AcceptRequest) -> Res<Value> {
    let invite = parse_invite(&payload.invite)
        .ok_or("That does not look like an invitation from this app.")?;
    let remote = invite["remote"].as_str().unwrap_or("").to_string();
    let target = std::path::PathBuf::from(payload.folder.trim());
    if target.exists() && std::fs::read_dir(&target).map(|mut d| d.next().is_some()).unwrap_or(false) {
        return Err("That folder already has something in it. Pick an empty one — cloning into it would \
                    mix two projects.".into());
    }
    std::fs::create_dir_all(&target).map_err(e)?;

    let parent = target.parent().unwrap_or(std::path::Path::new(".")).to_path_buf();
    let name = target.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    crate::project_sync::git(&parent, &["clone", &remote, &name])
        .map_err(|err| format!("Could not clone the shared repo: {err}"))?;

    let project = json!({
        "id": invite["project_id"].as_str().unwrap_or("").to_string(),
        "name": invite["name"].as_str().unwrap_or("Shared project"),
        "project_folder": target.to_string_lossy(),
        "shared": true,
        "my_role": invite["role"].as_str().unwrap_or("editor"),
        "created_at": crate::models::now_iso(),
    });
    let d = bson::to_document(&project).map_err(e)?;
    state.db.collection::<Document>("projects")
        .update_one(doc! { "id": project["id"].as_str().unwrap_or("") }, doc! { "$set": d })
        .upsert(true).await.map_err(e)?;
    // The clone brought its own JSON shards; the cache must not keep serving what was here before.
    state.db.invalidate_cache().await;

    Ok(json!({
        "project": project,
        "note": "Their history came with it. Pull before you start working, and again before you push.",
    }))
}

// ── Pull, merge, rewind ─────────────────────────────────────────────────────

/// Pull the shared repo and report what could not be merged automatically.
#[tauri::command]
pub async fn team_pull(state: State<'_, AppState>, project_id: String) -> Res<Value> {
    let folder = crate::project_sync::project_folder(&state.db, &project_id).await
        .ok_or("This project has no folder on disk.")?;

    // Commit local work first. Pulling over uncommitted changes is how people lose an afternoon.
    let staged = crate::project_sync::commit_all(&folder, "data: before pulling the team's changes")?;

    // A merge, not a rebase: rebasing rewrites local commits, and someone else's history is not ours
    // to rewrite.
    let pulled = crate::project_sync::git(&folder, &["pull", "--no-rebase", "--no-edit"]);
    let conflicts = conflicted_files(
        &crate::project_sync::git(&folder, &["status", "--porcelain"]).unwrap_or_default());

    state.db.invalidate_cache().await;
    match pulled {
        Ok(out) if conflicts.is_empty() => Ok(json!({
            "ok": true, "committed_first": staged, "output": out.trim(), "conflicts": [],
        })),
        _ => Ok(json!({
            "ok": false,
            "committed_first": staged,
            "conflicts": conflicts,
            "note": "Both sides changed the same files. Resolve them here — nothing is lost either way, \
                     because your work was committed before the pull.",
        })),
    }
}

#[derive(serde::Deserialize)]
pub struct ResolveRequest {
    pub project_id: String,
    pub file: String,
    /// "merge" (three-way on the JSON), "mine", or "theirs".
    pub how: String,
}

/// Settle one conflicted file.
///
/// `merge` is the interesting one: it reads all three versions out of git and merges the JSON field by
/// field, so only the fields both sides actually changed are left for a person. `mine`/`theirs` are the
/// blunt instruments, offered because sometimes that genuinely is the answer.
#[tauri::command]
pub async fn resolve_conflict(state: State<'_, AppState>, payload: ResolveRequest) -> Res<Value> {
    let folder = crate::project_sync::project_folder(&state.db, &payload.project_id).await
        .ok_or("This project has no folder on disk.")?;
    let file = payload.file.trim();
    if file.is_empty() || file.contains("..") {
        return Err("That is not a file in this project.".into());
    }

    let read_stage = |stage: &str| -> Option<Value> {
        crate::project_sync::git(&folder, &["show", &format!("{stage}:{file}")])
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
    };

    match payload.how.as_str() {
        "mine" => { crate::project_sync::git(&folder, &["checkout", "--ours", "--", file])?; }
        "theirs" => { crate::project_sync::git(&folder, &["checkout", "--theirs", "--", file])?; }
        "merge" => {
            // :1 = base, :2 = ours, :3 = theirs — git's own conflict stages.
            let base = read_stage(":1").unwrap_or(Value::Null);
            let mine = read_stage(":2").ok_or("Your version of this file is not JSON, so it cannot be \
                                               merged field by field. Choose yours or theirs.")?;
            let theirs = read_stage(":3").ok_or("Their version of this file is not JSON, so it cannot \
                                                be merged field by field. Choose yours or theirs.")?;
            let (merged, conflicts) = merge_json(&base, &mine, &theirs);
            let path = folder.join(file);
            std::fs::write(&path, serde_json::to_string_pretty(&merged).map_err(e)?).map_err(e)?;
            crate::project_sync::git(&folder, &["add", "--", file])?;
            state.db.invalidate_cache().await;
            return Ok(json!({
                "file": file, "how": "merge", "field_conflicts": conflicts,
                "note": if conflicts.is_empty() {
                    "Merged cleanly — the two of you changed different fields."
                } else {
                    "Merged, but these fields were changed by both of you and kept YOUR value. Check them."
                },
            }));
        }
        other => return Err(format!("'{other}' is not a way to resolve a conflict")),
    }
    crate::project_sync::git(&folder, &["add", "--", file])?;
    state.db.invalidate_cache().await;
    Ok(json!({ "file": file, "how": payload.how }))
}

#[tauri::command]
pub async fn finish_merge(state: State<'_, AppState>, project_id: String) -> Res<Value> {
    let folder = crate::project_sync::project_folder(&state.db, &project_id).await
        .ok_or("This project has no folder on disk.")?;
    let left = conflicted_files(
        &crate::project_sync::git(&folder, &["status", "--porcelain"]).unwrap_or_default());
    if !left.is_empty() {
        return Err(format!("{} file(s) are still unresolved: {}", left.len(), left.join(", ")));
    }
    crate::project_sync::git(&folder, &["commit", "--no-edit"])
        .or_else(|_| crate::project_sync::git(&folder, &["commit", "-m", "data: merged the team's changes"]))?;
    state.db.invalidate_cache().await;
    Ok(json!({ "merged": true }))
}

/// The project's history, for looking back and for rewinding.
#[tauri::command]
pub async fn project_history(state: State<'_, AppState>, project_id: String, limit: Option<i64>) -> Res<Value> {
    let folder = crate::project_sync::project_folder(&state.db, &project_id).await
        .ok_or("This project has no folder on disk.")?;
    let n = limit.unwrap_or(50).clamp(1, 500).to_string();
    let log = crate::project_sync::git(&folder, &[
        "log", &format!("-{n}"), "--pretty=format:%h\x1f%an\x1f%ar\x1f%s",
    ]).unwrap_or_default();
    let commits: Vec<Value> = log.lines().filter_map(|l| {
        let mut p = l.split('\x1f');
        Some(json!({
            "hash": p.next()?, "author": p.next().unwrap_or(""),
            "when": p.next().unwrap_or(""), "subject": p.next().unwrap_or(""),
        }))
    }).collect();
    Ok(json!({ "commits": commits }))
}

/// Restore the project's files as they were at a commit — as a NEW commit.
///
/// History is never rewritten. Someone destroying something is recoverable *and* still visible, which is
/// what makes it possible to talk about afterwards. A reset would hide both.
#[tauri::command]
pub async fn rewind_project(state: State<'_, AppState>, project_id: String, commit: String) -> Res<Value> {
    let folder = crate::project_sync::project_folder(&state.db, &project_id).await
        .ok_or("This project has no folder on disk.")?;
    let hash = commit.trim();
    if hash.is_empty() || !hash.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("That is not a commit id.".into());
    }
    // Keep whatever is uncommitted, so a rewind cannot eat work in progress either.
    let saved = crate::project_sync::commit_all(&folder, "data: before rewinding")?;
    crate::project_sync::git(&folder, &["checkout", hash, "--", "."])
        .map_err(|err| format!("Could not read that commit: {err}"))?;
    let restored = crate::project_sync::commit_all(
        &folder, &format!("data: restore the state from {hash}"))?;
    state.db.invalidate_cache().await;
    Ok(json!({
        "saved_first": saved,
        "restored_as": restored,
        "note": "Restored as a new commit. Nothing was rewritten, so the state you rewound *from* is \
                 still in the history.",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_fields_both_sides_changed_become_conflicts() {
        // The behaviour that makes collaboration bearable: two people working on different fields of
        // the same file must not be asked anything at all.
        let base = json!({ "title": "Genesis 1", "mood": "calm", "tempo": 90 });
        let mine = json!({ "title": "Genesis 1", "mood": "reverent", "tempo": 90 });
        let theirs = json!({ "title": "Genesis 1", "mood": "calm", "tempo": 104 });
        let (merged, conflicts) = merge_json(&base, &mine, &theirs);
        assert!(conflicts.is_empty(), "{conflicts:?}");
        assert_eq!(merged["mood"], json!("reverent"), "my change survives");
        assert_eq!(merged["tempo"], json!(104), "their change survives");
    }

    #[test]
    fn the_same_field_changed_differently_is_reported_never_silently_picked() {
        // Last-writer-wins would erase somebody's afternoon without telling anyone.
        let base = json!({ "mood": "calm" });
        let mine = json!({ "mood": "reverent" });
        let theirs = json!({ "mood": "urgent" });
        let (merged, conflicts) = merge_json(&base, &mine, &theirs);
        assert_eq!(conflicts, vec!["mood"]);
        assert_eq!(merged["mood"], json!("reverent"), "mine is kept so the file stays usable");
    }

    #[test]
    fn nested_conflicts_are_reported_by_path() {
        let base = json!({ "brief": { "mood": "calm", "voice": "warm" } });
        let mine = json!({ "brief": { "mood": "reverent", "voice": "warm" } });
        let theirs = json!({ "brief": { "mood": "urgent", "voice": "warm" } });
        let (_, conflicts) = merge_json(&base, &mine, &theirs);
        assert_eq!(conflicts, vec!["brief.mood"], "a bare 'mood' would be ambiguous in a big file");
    }

    #[test]
    fn a_field_only_one_side_has_is_kept_rather_than_dropped() {
        // Added by them, or deleted by me — keeping it is the non-destructive reading, and a deletion
        // somebody wanted costs one more click while a deletion nobody wanted is lost work.
        let base = json!({ "a": 1 });
        let mine = json!({ "a": 1 });
        let theirs = json!({ "a": 1, "b": 2 });
        let (merged, conflicts) = merge_json(&base, &mine, &theirs);
        assert_eq!(merged["b"], json!(2));
        assert!(conflicts.is_empty());
    }

    #[test]
    fn an_unknown_role_fails_closed() {
        // Roles arrive off a shared repo somebody else wrote, so a value we do not recognise must not
        // mean full access.
        assert!(role_allows("owner", "delete_project"));
        assert!(role_allows("editor", "write"));
        assert!(!role_allows("editor", "delete_project"));
        assert!(!role_allows("editor", "manage_members"));
        assert!(role_allows("viewer", "read"));
        assert!(!role_allows("viewer", "write"));
        assert!(!role_allows("administrator", "write"), "not a role we know");
        assert!(!role_allows("", "write"));
    }

    #[test]
    fn an_invitation_round_trips_and_rejects_anything_else() {
        let text = invite_payload("p1", "Psalms", "https://github.com/me/psalms.git", "editor");
        assert!(text.starts_with("bmstudio-invite:"));
        let back = parse_invite(&text).expect("parsed");
        assert_eq!(back["project_id"], json!("p1"));
        assert_eq!(back["remote"], json!("https://github.com/me/psalms.git"));
        assert_eq!(back["role"], json!("editor"));
        // Surrounding whitespace from a chat message is normal and must not break it.
        assert!(parse_invite(&format!("  {text}\n")).is_some());
        assert!(parse_invite("https://example.com").is_none());
        assert!(parse_invite("bmstudio-invite:not-base64!!").is_none());
    }

    #[test]
    fn an_invitation_without_a_remote_is_useless_and_rejected() {
        let empty = format!("bmstudio-invite:{}",
            base64_encode(json!({ "v": 1, "project_id": "p", "remote": "" }).to_string().as_bytes()));
        assert!(parse_invite(&empty).is_none(), "there would be nothing to clone");
    }

    #[test]
    fn only_genuinely_unmerged_states_count_as_conflicts() {
        let porcelain = "UU data/fields/composer.json\n M data/fields/brief.json\n?? notes.txt\nAA both.json\n D gone.json";
        let files = conflicted_files(porcelain);
        assert_eq!(files, vec!["data/fields/composer.json", "both.json"]);
        // A plain local modification is not a conflict, and neither is an untracked file.
        assert!(!files.iter().any(|f| f.contains("brief")));
        assert!(!files.iter().any(|f| f.contains("notes")));
    }
}
