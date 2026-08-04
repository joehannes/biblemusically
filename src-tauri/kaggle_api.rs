//! Talking to Kaggle over HTTP instead of through the `kaggle` command.
//!
//! # Why this exists
//!
//! Everything the app does with Kaggle currently shells out to the Python CLI: `locate_kaggle()`
//! hunts for it on PATH, and every operation spawns a subprocess. That works on a desktop and cannot
//! work on Android at all — since Android 10, an app targeting API 29 or later is forbidden by
//! SELinux from `exec()`ing anything in its own writable data directory (a W^X violation). It is why
//! Termux is pinned to targetSdk 28 and ships its binaries as APK native libraries rather than as
//! ordinary files. Shipping a Python interpreter plus the kaggle package as native libs is possible
//! and deeply unpleasant.
//!
//! It is also unnecessary, because the CLI is a thin wrapper over a public REST API. `kernels/status`
//! is a GET with two query parameters and basic auth. So the honest fix is not to make subprocesses
//! work on a phone; it is to stop needing one.
//!
//! # What this replaces, and what it adds
//!
//! Two of these endpoints do things the app currently believes are impossible:
//!
//! * **`kernels/quota`** returns real GPU hours used and allowed, with the reset time. The advisor
//!   has a `quota_minutes_left` input that nothing has ever populated — it was written expecting a
//!   number the app had no way to obtain. Now it does.
//! * **`kernels/cancel-session`** ends a running session directly. `supersede_kaggle_session` exists
//!   because "the Kaggle CLI/API has no stop-session call", which was true of the CLI and is not
//!   true of the API. The GPU-off supersede push can stay as the fallback it always should have been.
//!
//! # Credentials
//!
//! The same `~/.kaggle/kaggle.json` the CLI reads, as HTTP basic auth — username as the user, API
//! key as the password. Read from that file rather than from the settings store for the reason
//! `kaggle_owner` gives: that file is what the CLI authenticates as, so while both exist they must
//! not be allowed to disagree.

use serde::Serialize;
use serde_json::Value;

const BASE: &str = "https://www.kaggle.com/api/v1";

type Res<T> = Result<T, String>;

/// Username and API key, from the file the CLI uses.
pub async fn credentials() -> Option<(String, String)> {
    let home = std::env::var("HOME").ok()?;
    let raw = tokio::fs::read_to_string(std::path::PathBuf::from(home).join(".kaggle/kaggle.json"))
        .await.ok()?;
    let v: Value = serde_json::from_str(&raw).ok()?;
    let user = v["username"].as_str()?.trim().to_string();
    let key = v["key"].as_str()?.trim().to_string();
    (!user.is_empty() && !key.is_empty()).then_some((user, key))
}

async fn client() -> Res<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        // Kaggle's edge answers a request with no User-Agent with an HTML challenge rather than JSON.
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) BibleMusically")
        .build().map_err(|e| e.to_string())
}

/// GET a Kaggle endpoint with basic auth, returning parsed JSON.
async fn get(path: &str, query: &[(&str, &str)]) -> Res<Value> {
    let (user, key) = credentials().await
        .ok_or("No ~/.kaggle/kaggle.json — connect a Kaggle account first.")?;
    let res = client().await?
        .get(format!("{BASE}{path}"))
        .basic_auth(&user, Some(&key))
        .query(query)
        .send().await.map_err(|e| format!("Could not reach Kaggle: {e}"))?;
    let status = res.status();
    let text = res.text().await.unwrap_or_default();
    if !status.is_success() {
        // 401/403 is the one worth naming: it means the token is stale, which looks identical to a
        // network problem from the outside.
        return Err(match status.as_u16() {
            401 | 403 => "Kaggle rejected the API token. Create a fresh one at Kaggle → Settings → \
                          Create New API Token.".to_string(),
            _ => format!("Kaggle answered {status}: {}", text.chars().take(200).collect::<String>()),
        });
    }
    serde_json::from_str(&text)
        .map_err(|_| format!("Kaggle returned something unreadable: {}",
                             text.chars().take(200).collect::<String>()))
}

// ── Which way to talk to Kaggle ─────────────────────────────────────────────

/// How a Kaggle operation should be carried out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Transport {
    /// Spawn the `kaggle` CLI. Everything the app has always done, and still the default wherever it
    /// can run — it is the better-tested path, and its `kernels push` handles the multipart upload
    /// this module does not yet implement.
    Cli,
    /// Speak the REST API directly. The only option on Android, and the fallback anywhere the CLI is
    /// absent or the user would rather not install Python.
    Http,
}

/// Decide, from the platform and what is installed.
///
/// Mobile is not a preference — Android forbids `exec()` from an app's own data directory, so there
/// is no CLI to choose. On desktop the CLI wins when present, so nothing that works today changes;
/// HTTP is what a machine without it falls back to instead of failing with "could not run the kaggle
/// CLI", which is the error this removes.
pub fn transport(cli_present: bool) -> Transport {
    if cfg!(mobile) { return Transport::Http; }
    if cli_present { Transport::Cli } else { Transport::Http }
}

/// Whether HTTP can work at all: it needs a key file, since there is no cached CLI session to borrow.
pub async fn http_usable() -> bool { credentials().await.is_some() }

// ── Quota ───────────────────────────────────────────────────────────────────

/// GPU time used and allowed this week.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Quota {
    pub used_minutes: i64,
    pub allowed_minutes: i64,
    pub left_minutes: i64,
    /// When the weekly window resets, ISO-8601, as Kaggle stated it.
    pub resets_at: String,
}

/// Kaggle returns durations as protobuf `Duration`, which serialises as a string of seconds with a
/// trailing `s` ("108000s") — but has also appeared as a plain number and as an object. Parsed
/// defensively for that reason: a quota that reads as zero would make the advisor block every run.
pub fn parse_duration_minutes(v: &Value) -> Option<i64> {
    let secs = match v {
        Value::String(s) => s.trim().trim_end_matches('s').parse::<f64>().ok()?,
        Value::Number(n) => n.as_f64()?,
        Value::Object(o) => o.get("seconds")
            .and_then(|s| s.as_f64().or_else(|| s.as_str().and_then(|t| t.parse().ok())))?,
        _ => return None,
    };
    if !secs.is_finite() || secs < 0.0 { return None; }
    Some((secs / 60.0).round() as i64)
}

/// Read a quota response into the numbers the advisor wants.
///
/// Separated from the request so it can be tested against a captured payload — the shape is the part
/// most likely to change under us, and the part that would otherwise only be discovered in the wild.
pub fn parse_quota(v: &Value) -> Quota {
    // Both camelCase and snake_case appear depending on which serialiser answered.
    let gpu = if v["gpuQuota"].is_object() { &v["gpuQuota"] } else { &v["gpu_quota"] };
    let used = parse_duration_minutes(&gpu["timeUsed"])
        .or_else(|| parse_duration_minutes(&gpu["time_used"])).unwrap_or(0);
    let allowed = parse_duration_minutes(&gpu["totalTimeAllowed"])
        .or_else(|| parse_duration_minutes(&gpu["total_time_allowed"])).unwrap_or(0);
    let resets_at = v["quotaRefreshTime"].as_str()
        .or_else(|| v["quota_refresh_time"].as_str()).unwrap_or("").to_string();
    Quota {
        used_minutes: used,
        allowed_minutes: allowed,
        // Never negative: reserved time can push used past allowed, and a negative "left" would
        // read as nonsense in the advisor's arithmetic.
        left_minutes: (allowed - used).max(0),
        resets_at,
    }
}

/// This account's GPU quota, straight from Kaggle.
pub async fn quota() -> Res<Quota> {
    Ok(parse_quota(&get("/kernels/quota", &[]).await?))
}

// ── Status ──────────────────────────────────────────────────────────────────

/// A kernel's latest status, without spawning anything.
///
/// Takes the full `owner/name` slug the rest of the app uses and splits it, so callers do not have
/// to know that Kaggle's API wants the two halves as separate query parameters.
pub async fn kernel_status(slug: &str) -> Res<String> {
    let (owner, name) = slug.split_once('/')
        .ok_or_else(|| format!("'{slug}' is not an owner/name kernel slug."))?;
    let v = get("/kernels/status", &[("userName", owner), ("kernelSlug", name)]).await?;
    Ok(v["status"].as_str().unwrap_or("unknown").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Nothing that works on a desktop today may change: with a CLI present, the CLI is used.
    #[test]
    fn the_cli_stays_the_desktop_default() {
        if cfg!(mobile) { return; }
        assert_eq!(transport(true), Transport::Cli);
    }

    /// A desktop without the CLI falls back rather than failing — that fallback is the whole point.
    #[test]
    fn a_desktop_without_the_cli_falls_back_to_http() {
        if cfg!(mobile) { return; }
        assert_eq!(transport(false), Transport::Http);
    }

    /// On Android there is no choice to make: exec() from the app's data directory is forbidden, so
    /// "is the CLI installed" is not a question that can have a useful answer.
    #[test]
    fn mobile_is_always_http_whatever_is_installed() {
        if !cfg!(mobile) { return; }
        assert_eq!(transport(true), Transport::Http);
        assert_eq!(transport(false), Transport::Http);
    }

    /// Kaggle has serialised these durations three different ways. Reading any of them as zero would
    /// make the advisor report no quota left and block every run.
    #[test]
    fn durations_parse_in_every_shape_kaggle_uses() {
        assert_eq!(parse_duration_minutes(&json!("108000s")), Some(1800));
        assert_eq!(parse_duration_minutes(&json!("108000")), Some(1800));
        assert_eq!(parse_duration_minutes(&json!(108000)), Some(1800));
        assert_eq!(parse_duration_minutes(&json!({ "seconds": 108000 })), Some(1800));
        assert_eq!(parse_duration_minutes(&json!({ "seconds": "108000" })), Some(1800));
    }

    #[test]
    fn nonsense_durations_are_rejected_rather_than_guessed() {
        assert_eq!(parse_duration_minutes(&json!("not a duration")), None);
        assert_eq!(parse_duration_minutes(&json!(null)), None);
        assert_eq!(parse_duration_minutes(&json!(-5)), None);
        assert_eq!(parse_duration_minutes(&json!([1, 2])), None);
    }

    #[test]
    fn a_quota_response_becomes_minutes() {
        let body = json!({
            "quotaRefreshTime": "2026-08-08T00:00:00Z",
            "gpuQuota": { "timeUsed": "3600s", "totalTimeAllowed": "108000s" },
        });
        let q = parse_quota(&body);
        assert_eq!(q.used_minutes, 60);
        assert_eq!(q.allowed_minutes, 1800);   // Kaggle's 30 h weekly allowance
        assert_eq!(q.left_minutes, 1740);
        assert_eq!(q.resets_at, "2026-08-08T00:00:00Z");
    }

    #[test]
    fn snake_case_answers_parse_too() {
        let body = json!({
            "quota_refresh_time": "2026-08-08T00:00:00Z",
            "gpu_quota": { "time_used": "600s", "total_time_allowed": "108000s" },
        });
        assert_eq!(parse_quota(&body).used_minutes, 10);
        assert_eq!(parse_quota(&body).left_minutes, 1790);
    }

    /// Reserved time can push usage past the allowance. A negative "left" would be arithmetic the
    /// advisor then reports as a blocker with a nonsense number in it.
    #[test]
    fn using_more_than_allowed_leaves_zero_not_a_negative() {
        let body = json!({ "gpuQuota": { "timeUsed": "200000s", "totalTimeAllowed": "108000s" } });
        assert_eq!(parse_quota(&body).left_minutes, 0);
    }

    /// An empty or unexpected body must not read as "plenty of quota".
    #[test]
    fn an_empty_answer_claims_nothing() {
        let q = parse_quota(&json!({}));
        assert_eq!(q, Quota::default());
        assert_eq!(q.left_minutes, 0);
    }
}
