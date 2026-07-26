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
// When a channel should publish
//
// A channel has an audience in a place, and that audience is awake at particular hours. Publishing at
// 3am local time to a Brazilian audience wastes the first hours of a video's life, which is when the
// algorithm decides what to do with it.
//
// Two sources, in order of how much they are worth:
//
//   1. **YouTube Analytics** — "when your viewers are on YouTube" is the only authoritative answer, and
//      it is specific to *this* channel's actual audience. It needs the `yt-analytics.readonly` scope,
//      which means re-consenting, so it is opt-in per channel rather than demanded of everyone.
//   2. **AI research** — a regional estimate with reasoning behind it, grounded in search. Not as good
//      as your own numbers, but available the moment a channel is imported, which is when the question
//      actually gets asked.
//
// The AI answer is stored with `source: "ai"` so it can never be mistaken for measured data, and it is
// a *suggestion* until accepted: a time that appeared by itself and started publishing would be a
// setting nobody chose.
// ────────────────────────────────────────────────────────────────

/// A weekday-and-time suggestion, in the channel's own local time.
///
/// Stored on the channel as `publish_time` (HH:MM), `publish_days` and `publish_timezone`.
#[derive(serde::Deserialize)]
pub struct SuggestRequest {
    pub channel_id: String,
    /// Skip the cache and ask again — used when the region or language changed.
    #[serde(default)]
    pub refresh: Option<bool>,
}

/// Normalise whatever the model returns into `HH:MM`.
///
/// Models answer "7 PM", "19:00", "19h", "around 7pm" — all of which have to become one comparable
/// string, or the schedule silently never matches.
pub fn normalise_time(raw: &str) -> Option<String> {
    let s = raw.trim().to_lowercase();
    let pm = s.contains("pm");
    let am = s.contains("am");
    let digits: String = s.chars().map(|c| if c.is_ascii_digit() { c } else { ' ' }).collect();
    let mut parts = digits.split_whitespace();
    let h: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    if m > 59 { return None; }
    let hour = match (h, pm, am) {
        (12, true, _) => 12,
        (h, true, _) if h < 12 => h + 12,
        (12, _, true) => 0,
        (h, _, _) if h < 24 => h,
        _ => return None,
    };
    Some(format!("{hour:02}:{m:02}"))
}

/// Weekday names → the 0-6 the scheduler uses (0 = Sunday, matching `schedule_config.day_of_week`).
pub fn normalise_days(raw: &Value) -> Vec<i64> {
    let names = ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"];
    let mut out = Vec::new();
    let mut push = |d: i64| { if (0..=6).contains(&d) && !out.contains(&d) { out.push(d); } };
    match raw {
        Value::Array(a) => {
            for item in a {
                if let Some(n) = item.as_i64() { push(n); continue; }
                if let Some(s) = item.as_str() {
                    let lower = s.trim().to_lowercase();
                    if let Some(i) = names.iter().position(|n| lower.starts_with(&n[..3])) {
                        push(i as i64);
                    }
                }
            }
        }
        Value::String(s) => {
            let lower = s.to_lowercase();
            for (i, n) in names.iter().enumerate() {
                if lower.contains(&n[..3]) { push(i as i64); }
            }
        }
        _ => {}
    }
    out.sort();
    out
}

/// Ask the AI when this channel's audience is most likely to be watching.
///
/// Grounded in search where the provider supports it, because "best time to post on YouTube in Brazil"
/// is exactly the kind of question whose answer moves and whose stale answer looks identical to a fresh
/// one.
#[tauri::command]
pub async fn suggest_publish_time(state: State<'_, AppState>, payload: SuggestRequest) -> Res<Value> {
    let channel = state.db.collection::<Document>("channels")
        .find_one(doc! { "id": &payload.channel_id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "channel not found".to_string())?;

    // Already suggested and not asked to redo it: don't spend a request on the same answer.
    if payload.refresh != Some(true) {
        if let Some(existing) = channel["publish_time"].as_str().filter(|t| !t.is_empty()) {
            return Ok(json!({
                "time": existing,
                "days": channel["publish_days"].clone(),
                "timezone": channel["publish_timezone"].clone(),
                "source": channel["publish_time_source"].clone(),
                "reason": channel["publish_time_reason"].clone(),
                "cached": true,
            }));
        }
    }

    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();

    let region = channel["region"].as_str().unwrap_or("").trim();
    let language = channel["language"].as_str().unwrap_or("").trim();
    let name = channel["name"].as_str().unwrap_or("this channel");

    let system = "You advise on YouTube publishing schedules.\n\
        Answer ONLY with a JSON object: {\"time\": \"HH:MM\", \"days\": [\"Friday\", \"Saturday\"], \
        \"timezone\": \"IANA name\", \"reason\": \"one or two sentences\"}\n\
        `time` is local time in the audience's own timezone, on a 24-hour clock. Aim for 2-4 hours \
        BEFORE the audience's peak viewing window, because a video needs time to be indexed and shown. \
        `days` are the two or three best days. Be specific to the region and the kind of content; do \
        not give a global average.";
    let user = format!(
        "Channel: {name}\nRegion: {region}\nAudience language: {language}\n\
         Content: music videos setting scripture to music — devotional, often watched in the evening \
         and on weekends.\n\nWhen should it publish?"
    );

    let (content, model) = crate::commands::ai::provider_chat(&settings, system, &user, 0.4, true).await?;
    let parsed = crate::commands::ai::extract_json_value(&content)
        .ok_or("the model did not answer with usable JSON — try again")?;

    let time = parsed["time"].as_str().and_then(normalise_time)
        .ok_or("the model did not give a readable time")?;
    let days = normalise_days(&parsed["days"]);
    let timezone = parsed["timezone"].as_str().unwrap_or("").to_string();
    let reason = parsed["reason"].as_str().unwrap_or("").to_string();

    // Stored, but marked as a suggestion: a time that appeared by itself and started publishing would
    // be a setting nobody chose.
    state.db.collection::<Document>("channels")
        .update_one(doc! { "id": &payload.channel_id }, doc! { "$set": {
            "publish_time_suggested": &time,
            "publish_days_suggested": bson::to_bson(&days).unwrap_or(bson::Bson::Null),
            "publish_timezone": &timezone,
            "publish_time_source": "ai",
            "publish_time_reason": &reason,
            "publish_time_model": &model,
        }}).await.map_err(e)?;

    Ok(json!({
        "time": time, "days": days, "timezone": timezone,
        "reason": reason, "source": "ai", "model": model, "cached": false,
        "accepted": false,
        "note": "A suggestion until you accept it. Your own YouTube Analytics — \"when your viewers are \
                 on YouTube\" — is the only authoritative answer, and needs the analytics scope.",
    }))
}

#[derive(serde::Deserialize)]
pub struct SetRequest {
    pub channel_id: String,
    /// `HH:MM` in the channel's local time. Empty clears it.
    pub time: String,
    #[serde(default)]
    pub days: Vec<i64>,
    #[serde(default)]
    pub timezone: Option<String>,
}

/// Set the channel's publication time by hand, or accept a suggestion.
#[tauri::command]
pub async fn set_publish_time(state: State<'_, AppState>, payload: SetRequest) -> Res<Value> {
    let time = if payload.time.trim().is_empty() {
        String::new()
    } else {
        normalise_time(&payload.time).ok_or("that is not a readable time — try 19:30 or 7:30 pm")?
    };
    let days: Vec<i64> = payload.days.into_iter().filter(|d| (0..=6).contains(d)).collect();
    state.db.collection::<Document>("channels")
        .update_one(doc! { "id": &payload.channel_id }, doc! { "$set": {
            "publish_time": &time,
            "publish_days": bson::to_bson(&days).unwrap_or(bson::Bson::Null),
            "publish_timezone": payload.timezone.unwrap_or_default(),
            "publish_time_source": if time.is_empty() { "none" } else { "chosen" },
        }}).await.map_err(e)?;
    Ok(json!({ "channel_id": payload.channel_id, "time": time, "days": days }))
}

/// Every channel's publication time, for the schedule view and for the guide's nudges.
#[tauri::command]
pub async fn list_publish_times(state: State<'_, AppState>) -> Res<Value> {
    use futures_util::StreamExt;
    let mut out = Vec::new();
    let mut cursor = state.db.collection::<Document>("channels").find(doc! {}).await.map_err(e)?;
    while let Some(Ok(d)) = cursor.next().await {
        let c = bson_to_value(d);
        out.push(json!({
            "channel_id": c["id"],
            "name": c["name"],
            "region": c["region"],
            "language": c["language"],
            "time": c["publish_time"],
            "days": c["publish_days"],
            "timezone": c["publish_timezone"],
            "source": c["publish_time_source"],
            "reason": c["publish_time_reason"],
            "suggested": c["publish_time_suggested"],
            "suggested_days": c["publish_days_suggested"],
        }));
    }
    // Channels with nothing set at all are what the guide should mention, so they come first.
    out.sort_by_key(|c| c["time"].as_str().unwrap_or("").is_empty() as i64 * -1);
    Ok(json!({ "channels": out }))
}

/// Channels that have no publication time yet — the guide's cue to offer one.
///
/// Kept as data rather than a nudge the guide invents, so the assistant only raises it when there is
/// genuinely something to raise.
#[tauri::command]
pub async fn channels_missing_publish_time(state: State<'_, AppState>) -> Res<Value> {
    use futures_util::StreamExt;
    let mut missing = Vec::new();
    let mut cursor = state.db.collection::<Document>("channels").find(doc! {}).await.map_err(e)?;
    while let Some(Ok(d)) = cursor.next().await {
        let c = bson_to_value(d);
        let chosen = c["publish_time"].as_str().unwrap_or("");
        if chosen.is_empty() {
            missing.push(json!({
                "channel_id": c["id"], "name": c["name"], "region": c["region"],
                "has_suggestion": c["publish_time_suggested"].as_str().map(|s| !s.is_empty()).unwrap_or(false),
            }));
        }
    }
    Ok(json!({ "missing": missing, "count": missing.len() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shape_a_model_answers_with_becomes_one_comparable_time() {
        // A schedule that never matches because "7 PM" was stored verbatim is a feature that silently
        // does nothing, which is the worst kind.
        assert_eq!(normalise_time("19:00").as_deref(), Some("19:00"));
        assert_eq!(normalise_time("7 PM").as_deref(), Some("19:00"));
        assert_eq!(normalise_time("around 7:30pm").as_deref(), Some("19:30"));
        assert_eq!(normalise_time("07:30").as_deref(), Some("07:30"));
        assert_eq!(normalise_time("9am").as_deref(), Some("09:00"));
    }

    #[test]
    fn midnight_and_noon_do_not_collapse_into_each_other() {
        // The classic 12-hour-clock bug: 12am is 00:00 and 12pm is 12:00, not the other way round.
        assert_eq!(normalise_time("12 am").as_deref(), Some("00:00"));
        assert_eq!(normalise_time("12 pm").as_deref(), Some("12:00"));
        assert_eq!(normalise_time("12:30 am").as_deref(), Some("00:30"));
    }

    #[test]
    fn nonsense_is_rejected_rather_than_stored() {
        assert_eq!(normalise_time("whenever"), None);
        assert_eq!(normalise_time(""), None);
        assert_eq!(normalise_time("25:00"), None);
        assert_eq!(normalise_time("19:75"), None, "there is no 75th minute");
    }

    #[test]
    fn day_names_numbers_and_prose_all_reach_the_same_indices() {
        assert_eq!(normalise_days(&json!(["Friday", "Saturday"])), vec![5, 6]);
        assert_eq!(normalise_days(&json!(["sun", "Wednesday"])), vec![0, 3]);
        assert_eq!(normalise_days(&json!([5, 6])), vec![5, 6]);
        assert_eq!(normalise_days(&json!("Fridays and Sundays work best")), vec![0, 5]);
        assert_eq!(normalise_days(&json!(["Friday", "friday", 5])), vec![5], "no duplicates");
        assert_eq!(normalise_days(&json!(["nonsense"])), Vec::<i64>::new());
        assert_eq!(normalise_days(&json!([9, -1])), Vec::<i64>::new(), "out of range is dropped");
    }
}
