use crate::state::AppState;
use bson::{doc, Document};
use serde_json::{json, Value};
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
// Updates, and the difference between an update and an upgrade
//
// A **minor or patch** release is an update: same major version, covered by every licence including
// lifetime, applied with one click.
//
// A **new major** version is an upgrade: a separate product and a separate purchase. It is announced,
// never applied. Nothing about the running version changes, nothing is withdrawn, and the announcement
// can be dismissed for good. Somebody who bought "for life" and then sees their app start nagging has
// been told a different story than the one they paid for.
//
// A trial gets neither. A trial is for deciding about the version that was downloaded.
//
// The download goes through the same one-time-code endpoint as a first install, which is what lets the
// builds live in a private repository: the server holds the GitHub token and streams the asset, so no
// user ever needs access to the repo.
//
// Applying it hands the file to the operating system rather than replacing the binary in place. A
// self-replacing updater has to get permissions, signatures and rollback right on three platforms; the
// OS installer already does, and a package manager that knows what it installed is worth more than one
// click saved.
// ────────────────────────────────────────────────────────────────

/// How often the app asks. Fifteen minutes, per the design — frequent enough to notice a release the day
/// it lands, rare enough that a thousand installs are a rounding error on the server.
pub const CHECK_INTERVAL_MINS: i64 = 15;

/// Which asset this platform wants.
///
/// Named per platform rather than guessed from the file list, so a release that happens to contain an
/// extra artefact cannot offer a Windows installer to a Linux user.
pub fn platform_asset() -> &'static str {
    match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "macos",
        "windows" => "windows",
        "android" => "android",
        "ios" => "ios",
        _ => "linux",
    }
}

/// Compare two `major.minor.patch` strings numerically.
///
/// String comparison gets this wrong in the one case that matters: "0.10.0" sorts before "0.9.0", so a
/// tenth minor release would look older than the ninth and nobody would be offered it.
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |v: &str| -> Vec<u64> {
        v.trim().trim_start_matches('v').split('.')
            .map(|p| p.chars().take_while(|c| c.is_ascii_digit()).collect::<String>()
                .parse().unwrap_or(0))
            .chain(std::iter::repeat(0))
            .take(3).collect()
    };
    parse(a).cmp(&parse(b))
}

pub fn major_of(v: &str) -> u64 {
    v.trim().trim_start_matches('v').split('.').next()
        .and_then(|p| p.parse().ok()).unwrap_or(0)
}

/// What to do about a newer version, given what the licence covers.
///
/// Returns `("none" | "update" | "upgrade" | "blocked", why)`. One function so the button, the startup
/// notice and the background check can never disagree.
pub fn decide(current: &str, latest: &str, status: &str, dismissed_major: u64) -> (&'static str, String) {
    if latest.is_empty() { return ("none", String::new()); }
    if compare_versions(latest, current) != std::cmp::Ordering::Greater {
        return ("none", String::new());
    }
    // A trial is for deciding about the version that was downloaded.
    if status == "trial" || status == "expired" || status == "none" {
        return ("blocked", "Updates come with a subscription. The version you have keeps working.".into());
    }
    if major_of(latest) == major_of(current) {
        return ("update", format!("Version {latest} is ready — one click, and your licence covers it."));
    }
    let newer_major = major_of(latest);
    if dismissed_major >= newer_major {
        // Announced once, dismissed for good. Not mentioned again.
        return ("none", String::new());
    }
    ("upgrade", format!(
        "Version {latest} is out — a new major version, and a separate purchase. Nothing changes about \
         the version you have; it keeps working exactly as it does now."))
}

async fn settings_of(state: &AppState) -> Value {
    state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.ok().flatten()
        .map(bson_to_value).unwrap_or_default()
}

fn http() -> Res<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        // Cloudflare's bot protection returns an error page rather than the Worker's answer to anything
        // without a browser User-Agent.
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) \
                     Chrome/126.0 Safari/537.36 StudioLightkid")
        .build().map_err(e)
}

fn base_of(settings: &Value) -> String {
    let raw = settings["subs_base_url"].as_str().unwrap_or("").trim();
    if raw.is_empty() { "https://studio-lightkid.johannes-neugschwentner.workers.dev".to_string() }
    else { raw.trim_end_matches('/').to_string() }
}

/// Ask whether there is anything newer, and what it means for this licence.
#[tauri::command]
pub async fn check_update(state: State<'_, AppState>) -> Res<Value> {
    let settings = settings_of(&state).await;
    let base = base_of(&settings);
    let current = env!("CARGO_PKG_VERSION").to_string();

    let r = http()?.get(format!("{base}/v1/updates?version={current}")).send().await.map_err(e)?;
    let answer: Value = r.json().await.map_err(|_| "The update service answered unreadably.".to_string())?;

    let latest = answer["latest"].as_str().unwrap_or("").to_string();
    let status = crate::commands::subscription::current(&state).await
        .and_then(|p| p["status"].as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "none".into());
    let dismissed = settings["update_dismissed_major"].as_i64().unwrap_or(0) as u64;
    let (kind, message) = decide(&current, &latest, &status, dismissed);

    let _ = state.db.collection::<Document>("settings")
        .update_one(doc! { "_id": "singleton" },
                    doc! { "$set": { "update_checked_at": crate::models::now_iso() } }).await;

    Ok(json!({
        "current": current,
        "latest": latest,
        "kind": kind,
        "message": message,
        "notes": answer["update"]["notes"].as_str()
            .or_else(|| answer["upgrade"]["notes"].as_str()).unwrap_or(""),
        // Whatever image the release notes carry — the "nice news" picture for a major version.
        "image": answer["upgrade"]["image"].as_str().unwrap_or(""),
        "platform": platform_asset(),
        "interval_minutes": CHECK_INTERVAL_MINS,
    }))
}

/// Stop mentioning this major version.
#[tauri::command]
pub async fn dismiss_upgrade(state: State<'_, AppState>, major: i64) -> Res<Value> {
    state.db.collection::<Document>("settings")
        .update_one(doc! { "_id": "singleton" },
                    doc! { "$set": { "update_dismissed_major": major } }).await.map_err(e)?;
    Ok(json!({ "dismissed_major": major }))
}

/// Fetch the installer and hand it to the operating system.
///
/// Two steps because the builds live in a private repository: the server issues a one-time code, then
/// streams the asset using its own GitHub token. No user ever needs repository access, and the code is
/// spent on use.
#[tauri::command]
pub async fn download_update(state: State<'_, AppState>) -> Res<Value> {
    // Only for people the update is actually for. The server would serve it anyway — the download is not
    // the product — but offering a button that then does nothing useful is worse than not offering it.
    crate::commands::subscription::require(&state, "read").await?;
    let status = crate::commands::subscription::current(&state).await
        .and_then(|p| p["status"].as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "none".into());
    if !matches!(status.as_str(), "active" | "lifetime") {
        return Err("Updating is part of a subscription. The version you have keeps working.".into());
    }

    let settings = settings_of(&state).await;
    let base = base_of(&settings);
    let email = crate::commands::subscription::current(&state).await
        .and_then(|p| p["email"].as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    let otp: Value = http()?.post(format!("{base}/v1/download/otp"))
        .json(&json!({ "email": email })).send().await.map_err(e)?
        .json().await.map_err(|_| "The download could not be prepared.".to_string())?;
    let code = otp["code"].as_str().ok_or("No download code was issued.")?;

    let platform = platform_asset();
    let r = http()?.get(format!("{base}/v1/download?code={code}&platform={platform}"))
        .send().await.map_err(e)?;
    if !r.status().is_success() {
        return Err(format!("The build could not be downloaded ({}).", r.status()));
    }
    // The server sets the real filename; falling back to something sensible rather than "download".
    let name = r.headers().get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split("filename=").nth(1))
        .map(|v| v.trim_matches('"').to_string())
        .unwrap_or_else(|| format!("studio-lightkid-update-{platform}"));

    let bytes = r.bytes().await.map_err(e)?;
    let dir = crate::paths::download_dir().unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&dir).map_err(e)?;
    let path = dir.join(&name);
    std::fs::write(&path, &bytes).map_err(e)?;

    Ok(json!({
        "path": path.to_string_lossy(),
        "bytes": bytes.len(),
        "name": name,
        // Handed to the OS rather than applied in place: a self-replacing updater has to get
        // permissions, signatures and rollback right on three platforms, and the OS installer already
        // does. A package manager that knows what it installed is worth more than one click saved.
        "next": match std::env::consts::OS {
            "linux" => "Open it with your package manager, or: sudo dpkg -i the file in Downloads.",
            "macos" => "Open the .dmg and drag the app over the old one. Right-click → Open the first time.",
            "windows" => "Run the installer. Windows will warn that it is unsigned — More info → Run anyway.",
            // Android does not use this: the interface hands the file straight to the system
            // installer (see apk_install.rs). Kept as the answer for anyone reading the raw result,
            // because "open the downloaded file" is not something a phone user can act on — the
            // download lands in app-specific storage they cannot browse to.
            "android" => "Android will ask you to confirm the install.",
            _ => "Open the downloaded file to install it.",
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn versions_compare_numerically_not_as_text() {
        // The case that matters: as text, "0.10.0" sorts before "0.9.0", so nobody would ever be offered
        // the tenth minor release.
        assert_eq!(compare_versions("0.10.0", "0.9.0"), Ordering::Greater);
        assert_eq!(compare_versions("1.0.0", "0.99.99"), Ordering::Greater);
        assert_eq!(compare_versions("0.88.0", "0.88.0"), Ordering::Equal);
        assert_eq!(compare_versions("v0.88.1", "0.88.0"), Ordering::Greater, "a v prefix is not a version");
    }

    #[test]
    fn a_minor_release_is_an_update_and_a_major_one_is_not() {
        let (kind, msg) = decide("0.88.0", "0.89.0", "active", 0);
        assert_eq!(kind, "update");
        assert!(msg.contains("licence covers it"));

        let (kind, msg) = decide("0.88.0", "1.0.0", "active", 0);
        assert_eq!(kind, "upgrade");
        // The reassurance is the point: somebody who bought "for life" must not think it is being taken.
        assert!(msg.contains("keeps working"), "{msg}");
    }

    #[test]
    fn a_lifetime_licence_gets_minor_updates_free() {
        assert_eq!(decide("1.2.0", "1.3.0", "lifetime", 0).0, "update");
        // …and is told about the next major version rather than being pushed into it.
        assert_eq!(decide("1.2.0", "2.0.0", "lifetime", 0).0, "upgrade");
    }

    #[test]
    fn a_dismissed_major_is_never_mentioned_again() {
        assert_eq!(decide("1.0.0", "2.0.0", "active", 2).0, "none");
        // But the one after it is still news.
        assert_eq!(decide("1.0.0", "3.0.0", "active", 2).0, "upgrade");
    }

    #[test]
    fn a_trial_is_not_offered_updates_and_is_told_why_kindly() {
        for status in ["trial", "expired", "none"] {
            let (kind, msg) = decide("0.88.0", "0.89.0", status, 0);
            assert_eq!(kind, "blocked", "{status}");
            assert!(msg.contains("keeps working"), "{status}: {msg}");
        }
    }

    #[test]
    fn nothing_newer_means_nothing_to_say() {
        assert_eq!(decide("0.88.0", "0.88.0", "active", 0).0, "none");
        assert_eq!(decide("0.88.0", "0.87.0", "active", 0).0, "none");
        assert_eq!(decide("0.88.0", "", "active", 0).0, "none");
    }

    #[test]
    fn the_asset_asked_for_matches_this_platform() {
        // A release containing every platform's installer must not offer the wrong one.
        let a = platform_asset();
        assert!(["linux", "macos", "windows", "android", "ios"].contains(&a));
        #[cfg(target_os = "linux")]
        assert_eq!(a, "linux");
    }
}
