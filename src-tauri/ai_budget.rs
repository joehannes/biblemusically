//! What the free AI tiers have left today, and which provider to ask next.
//!
//! OpenRouter's free tier is **50 requests a day for the whole account** — not per model, not per
//! key, per account. Gemini's free tier is generous per day but tight per minute and answers "the
//! model is overloaded" when it is not. Both numbers are small enough that an app which calls the
//! AI once per item in a loop runs out before lunch, and the way it runs out is the worst kind:
//! every remaining call fails, so a workflow started at request 48 dies half-finished.
//!
//! The app had no idea any of this was happening. `quota_report` counted *jobs* as a proxy for AI
//! calls, which is wrong twice over — most AI calls never create a job (lyrics, styles, translation,
//! the guide, upload enrichment), and most jobs are not AI calls (music, images, video, uploads).
//! So the one number the user was shown about their scarcest resource was unrelated to it.
//!
//! This module is the ledger and the policy:
//!
//!   * **The ledger** counts every provider call, per provider, per UTC day, in a small JSON file
//!     next to the learnings store. A file rather than the document store because `provider_chat`
//!     is called from thirty-five places that do not all hold a `Db`, and threading one through
//!     every call site to count to fifty would be a worse cure than the disease.
//!   * **The policy** answers "who should take this call?". A provider with nothing left is not
//!     asked — an exhausted key produces a 429 that costs the user a wait and tells them nothing,
//!     and trying the next provider instead is free.
//!
//! Rotation is deliberately across *providers*, not models. The previous fallback swapped one
//! OpenRouter model for another OpenRouter model, which does nothing when the limit is per account:
//! it burned a second request out of the same fifty to receive the same 429.

use serde_json::{json, Value};
use std::path::PathBuf;

/// OpenRouter's free tier, and the number this whole module exists because of.
pub const OPENROUTER_FREE_DAILY: u64 = 50;

/// Gemini's free tier is limited per minute rather than per day; the daily ceiling is high enough
/// that treating it as unlimited would be honest, but a number lets the planner reason about a mixed
/// budget and warn before the user is surprised. Deliberately conservative.
pub const GEMINI_FREE_DAILY: u64 = 200;

/// Where the ledger lives. Beside the learnings store, for the same reason: it is small, it is the
/// user's own data, and it should survive a reinstall.
fn ledger_path() -> Option<PathBuf> {
    let base = crate::paths::config_dir()?;
    let dir = base.join("studio-lightkid");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("ai-usage.json"))
}

fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

fn read_all() -> Value {
    let Some(p) = ledger_path() else { return json!({}) };
    std::fs::read_to_string(p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!({}))
}

/// Today's counts, provider → requests. Days other than today are kept for a short history so the
/// user can see a pattern rather than a single number with no context.
pub fn usage_today() -> Value {
    read_all()
        .get(today())
        .cloned()
        .unwrap_or_else(|| json!({}))
}

/// Count one call against a provider.
///
/// Called whether or not the request succeeded, because a rate-limited request still counts against
/// the limit that rejected it — counting only successes is how an app talks itself past a quota it
/// has already spent.
pub fn record(provider: &str) {
    let Some(path) = ledger_path() else { return };
    let mut all = read_all();
    let day = today();

    // Keep a fortnight. Enough to see "I always run out on Sundays", short enough to stay a file
    // nobody has to think about.
    if let Some(obj) = all.as_object_mut() {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(14))
            .format("%Y-%m-%d")
            .to_string();
        obj.retain(|k, _| k.as_str() >= cutoff.as_str());
    }

    let entry = all
        .as_object_mut()
        .map(|o| o.entry(day).or_insert_with(|| json!({})));
    if let Some(entry) = entry {
        let n = entry.get(provider).and_then(|v| v.as_u64()).unwrap_or(0);
        entry[provider] = json!(n + 1);
    }
    if let Ok(text) = serde_json::to_string_pretty(&all) {
        let _ = std::fs::write(path, text);
    }
}

/// The free-tier daily ceiling for a provider, or `None` when calls are billed and therefore not
/// rationed. A paid key has no budget to run out of, only a bill — which is the user's business.
pub fn daily_cap(provider: &str, settings: &Value) -> Option<u64> {
    match provider {
        // A key with credit on it is not on the free tier any more. The user says so in settings
        // rather than the app guessing from a balance endpoint that may not exist.
        "openrouter" if settings["openrouter_paid"].as_bool() == Some(true) => None,
        "openrouter" => Some(OPENROUTER_FREE_DAILY),
        "gemini" if settings["gemini_paid"].as_bool() == Some(true) => None,
        "gemini" => Some(GEMINI_FREE_DAILY),
        _ => None,
    }
}

/// How many calls this provider has left today. `None` means "not rationed".
pub fn remaining(provider: &str, settings: &Value) -> Option<u64> {
    let cap = daily_cap(provider, settings)?;
    let used = usage_today()
        .get(provider)
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    Some(cap.saturating_sub(used))
}

/// Does this provider have a usable key configured?
pub fn configured(provider: &str, settings: &Value) -> bool {
    let field = match provider {
        "openrouter" => "openrouter_api_key",
        "gemini" => "gemini_api_key",
        "anthropic" => "anthropic_api_key",
        "openai" => "openai_api_key",
        _ => return false,
    };
    settings[field].as_str().map(|s| !s.trim().is_empty()).unwrap_or(false)
}

/// Every provider this install could actually call, primary first, then the rest as rotation
/// targets — skipping any that is configured but already spent for the day.
///
/// The order is the point. A user who chose Gemini gets Gemini until it has nothing left, and only
/// then somebody else; the app does not silently spend a paid key because a free one was busy for a
/// minute. Paid providers sort last for exactly that reason.
pub fn rotation(settings: &Value) -> Vec<String> {
    let primary = settings["ai_provider"].as_str().unwrap_or("openrouter").trim().to_string();
    let mut out: Vec<String> = Vec::new();

    let consider = |p: &str, out: &mut Vec<String>| {
        if out.iter().any(|x| x == p) { return; }
        if !configured(p, settings) { return; }
        // Nothing left today is not a candidate — asking anyway buys a 429 and a wait.
        if remaining(p, settings) == Some(0) { return; }
        out.push(p.to_string());
    };

    consider(&primary, &mut out);
    // Free first, then billed: a rotation that reaches for the credit card on its own is a
    // surprise on somebody's statement.
    for p in ["openrouter", "gemini"] { consider(p, &mut out); }
    for p in ["anthropic", "openai"] { consider(p, &mut out); }
    out
}

/// What one full pass of the workflow costs in requests, stage by stage.
///
/// Written down rather than measured because the user needs it *before* spending anything — "this
/// run needs 14 and you have 9 left" is actionable, and the same fact discovered at request 9 is
/// half a song and a wasted evening.
///
/// The numbers assume the batched call sites: one request that returns every section's image prompt,
/// not one request per section. Where a stage still loops per item that is called out, because the
/// honest cost there depends on how many items there are.
pub fn workflow_cost() -> Value {
    json!({
        "stages": [
            { "id": "brief",     "label": "Project brief / creative DNA", "requests": 1, "once": true,
              "note": "Once per project, not per song." },
            { "id": "lyrics",    "label": "Lyrics from the chapter",      "requests": 1, "once": false },
            { "id": "styles",    "label": "Music style + genre mix",      "requests": 1, "once": false },
            { "id": "sections",  "label": "Section image prompts",        "requests": 1, "once": false,
              "note": "One request for the whole song when batched." },
            { "id": "characters","label": "Character proposals",          "requests": 1, "once": true },
            { "id": "upload",    "label": "Title, description, tags",     "requests": 1, "once": false,
              "note": "One request per eight uploads, not per upload — descriptions and tags for the \
                       whole batch come back together. Counted as 1 here so the estimate errs high." },
            { "id": "publicity", "label": "Publicity pieces",             "requests": 1, "once": false,
              "optional": true },
            { "id": "guide",     "label": "The guide answering questions","requests": 2, "once": false,
              "optional": true, "note": "Varies with how much you ask it." }
        ],
        // A song end to end, without the optional stages.
        "per_song_core": 4,
        "per_project_once": 2,
        "notes": "A first song in a new project costs about 6 requests; each song after it about 4. \
                  On OpenRouter's 50-a-day that is roughly ten songs, or twelve once the project's \
                  one-off stages are done."
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(provider: &str, keys: &[&str]) -> Value {
        let mut s = json!({ "ai_provider": provider });
        for k in keys {
            let field = match *k {
                "openrouter" => "openrouter_api_key",
                "gemini" => "gemini_api_key",
                "anthropic" => "anthropic_api_key",
                _ => "openai_api_key",
            };
            s[field] = json!("k");
        }
        s
    }

    #[test]
    fn rotation_puts_the_chosen_provider_first() {
        let s = settings("gemini", &["gemini", "openrouter"]);
        assert_eq!(rotation(&s).first().map(String::as_str), Some("gemini"));
    }

    #[test]
    fn rotation_skips_providers_with_no_key() {
        let s = settings("openrouter", &["openrouter"]);
        let r = rotation(&s);
        assert!(r.contains(&"openrouter".to_string()));
        assert!(!r.contains(&"gemini".to_string()), "no key means not a candidate");
    }

    #[test]
    fn a_paid_provider_is_never_reached_for_before_a_free_one() {
        let s = settings("openrouter", &["openrouter", "gemini", "anthropic"]);
        let r = rotation(&s);
        let gemini = r.iter().position(|p| p == "gemini").unwrap();
        let anthropic = r.iter().position(|p| p == "anthropic").unwrap();
        assert!(gemini < anthropic, "a free tier must be exhausted before a bill is started");
    }

    #[test]
    fn a_billed_key_has_no_daily_ceiling() {
        let mut s = settings("openrouter", &["openrouter"]);
        assert_eq!(daily_cap("openrouter", &s), Some(OPENROUTER_FREE_DAILY));
        s["openrouter_paid"] = json!(true);
        assert_eq!(daily_cap("openrouter", &s), None, "credit removes the free-tier limit");
    }

    #[test]
    fn the_cost_table_adds_up_to_what_it_claims() {
        let c = workflow_cost();
        let core: u64 = c["stages"].as_array().unwrap().iter()
            .filter(|s| s["optional"].as_bool() != Some(true) && s["once"].as_bool() != Some(true))
            .map(|s| s["requests"].as_u64().unwrap_or(0))
            .sum();
        assert_eq!(core, c["per_song_core"].as_u64().unwrap(),
                   "the per-song figure must match the stages it is made of");
    }
}
