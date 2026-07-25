//! One place that answers "is the pipeline actually able to run right now?".
//!
//! The backend has always *recorded* service health — `validate_suno_cookie_internal` and friends
//! re-check every 15 minutes and stamp `*_valid` / `*_status` / `*_checked_at` onto the settings
//! document. But nothing ever showed it unless the user opened Settings and clicked "Test", so an
//! expired Suno cookie silently failed a whole night's queued jobs (BACKLOG "Health/expiry
//! surfacing", TODOS.md #16).
//!
//! The one design decision worth stating: **only the services you actually use are reported.** A
//! stale Midjourney profile is irrelevant when `image_engine` is ComfyUI, and nagging about it
//! trains people to ignore the banner. `relevant` marks the services selected by the current
//! engine settings; the UI only raises an alarm for those.

use crate::state::AppState;
use bson::{doc, Document};
use serde_json::{json, Value};
use tauri::State;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

/// How stale a check may be before we say so (the background validator runs every 15 minutes).
const STALE_AFTER_MINUTES: i64 = 45;

fn minutes_since(rfc3339: Option<&str>) -> Option<i64> {
    let parsed = chrono::DateTime::parse_from_rfc3339(rfc3339?).ok()?;
    Some((chrono::Utc::now() - parsed.with_timezone(&chrono::Utc)).num_minutes())
}

/// Turn a stored status code into something a human can act on.
fn explain(service: &str, status: &str) -> &'static str {
    match (service, status) {
        (_, "not_configured") => "Not set up yet.",
        ("suno", "expired") => "The Suno session cookie expired — sign in again to refresh it.",
        ("suno", "api_error") => "Suno rejected the request. The cookie may be for a different account.",
        (_, "connection_error") => "Could not reach the service — check the URL, or start the server.",
        (_, "auth_error") => "The service refused the API key.",
        (_, "http_error") => "The service answered with an error.",
        (_, "reachable") | (_, "valid") => "Working.",
        _ => "Unknown state — run a test from Settings.",
    }
}

fn service(
    settings: &Value,
    id: &str,
    label: &str,
    prefix: &str,
    relevant: bool,
    configured: bool,
) -> Value {
    let valid = settings[format!("{prefix}_valid")].as_bool();
    let status = settings[format!("{prefix}_status")]
        .as_str()
        .unwrap_or(if configured { "unknown" } else { "not_configured" })
        .to_string();
    let checked_at = settings[format!("{prefix}_checked_at")].as_str().map(|s| s.to_string());
    let age = minutes_since(checked_at.as_deref());
    json!({
        "id": id,
        "label": label,
        "relevant": relevant,
        "configured": configured,
        "ok": valid.unwrap_or(false),
        "unknown": valid.is_none(),
        "status": status,
        "explanation": explain(id, &status),
        "checked_at": checked_at,
        "checked_minutes_ago": age,
        "stale": age.map(|m| m > STALE_AFTER_MINUTES).unwrap_or(true),
    })
}

fn has(settings: &Value, key: &str) -> bool {
    settings[key].as_str().map(|s| !s.trim().is_empty()).unwrap_or(false)
}

/// Health of every integration, with `relevant` marking the ones the current engine choices use.
#[tauri::command]
pub async fn service_health(state: State<'_, AppState>) -> Res<Value> {
    let settings = state
        .db
        .collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" })
        .await
        .map_err(e)?
        .map(|d| crate::store::doc_to_json(&d))
        .unwrap_or_else(|| json!({}));

    let music_engine = settings["music_engine"].as_str().unwrap_or("suno").to_string();
    let image_engine = settings["image_engine"].as_str().unwrap_or("midjourney").to_string();
    let ai_provider = settings["ai_provider"].as_str().unwrap_or("openrouter").to_string();

    let services = vec![
        service(&settings, "suno", "Suno (music)", "suno_cookie",
                music_engine == "suno", has(&settings, "suno_cookie")),
        service(&settings, "acestep", "ACE-Step (music)", "acestep",
                music_engine == "acestep", has(&settings, "acestep_api_url")),
        service(&settings, "heartmula", "HeartMuLa (music)", "heartmula",
                music_engine == "heartmula", has(&settings, "heartmula_api_url")),
        service(&settings, "midjourney", "Midjourney (images)", "mj_profile",
                image_engine == "midjourney", has(&settings, "mj_profile_dir")),
        service(&settings, "flux", "FLUX (images)", "flux",
                image_engine == "flux", has(&settings, "flux_api_url")),
        service(&settings, "comfyui", "ComfyUI (images)", "comfyui",
                image_engine == "comfyui", has(&settings, "comfyui_api_url")),
    ];

    // The AI provider has no periodic validator; report configuration only, and say so.
    let ai_configured = match ai_provider.as_str() {
        "gemini" => has(&settings, "gemini_api_key"),
        _ => has(&settings, "openrouter_api_key"),
    };

    let blocking: Vec<Value> = services
        .iter()
        .filter(|s| s["relevant"].as_bool() == Some(true) && s["ok"].as_bool() != Some(true))
        .cloned()
        .collect();

    Ok(json!({
        "services": services,
        "ai": {
            "id": "ai", "label": format!("AI ({ai_provider})"),
            "relevant": true, "configured": ai_configured,
            "ok": ai_configured, "unknown": !ai_configured,
            "explanation": if ai_configured { "Key configured." } else { "No API key set — lyrics and assist features will fail." },
        },
        "engines": { "music": music_engine, "image": image_engine, "ai": ai_provider },
        // What the UI raises a banner for: the engines in use that aren't working.
        "blocking": blocking,
        "healthy": blocking.is_empty() && ai_configured,
        "fallback_engine": settings["music_engine_fallback"].as_str(),
    }))
}
