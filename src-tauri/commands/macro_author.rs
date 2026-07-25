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
// AI-authored browser macros
//
// Half the platforms worth posting to have no usable posting API — the rules are gated behind a
// partner review, the tier that allows writing costs money, or there simply is no endpoint. The
// honest answer for those is to drive the site the way a person does, which is what the macro player
// already does. What was missing is the authoring: recording a flow by hand is fine once and tedious
// at fifty channels.
//
// So: point the app at a page, say what you want done, and the AI writes the macro from the page's
// own DOM. It sees a *structural digest* — the interactive elements with their selectors, labels and
// roles — not the raw HTML, because raw HTML is mostly noise and blows the context window on a single
// page. Every step it proposes is validated against the player's real vocabulary before it is saved,
// so a hallucinated step type or a target with no selector never reaches playback.
// ────────────────────────────────────────────────────────────────

/// The step types the macro player actually understands (see lib/macroRecorder.js).
const STEP_TYPES: &[&str] = &["nav", "click", "contextmenu", "paste", "paste-clipboard", "fill", "enter", "wait", "download", "action", "connector"];

/// Validate and clean one proposed step. `None` drops it.
///
/// The rules mirror what the player needs to execute a step at all: a `nav` needs an href, anything
/// that touches an element needs a target it can resolve, and a `fill` needs a value or an explicit
/// parameter marker. Everything else is dropped rather than guessed at — a macro that half-works on
/// a live site is worse than one that refuses to save.
fn sanitise_step(step: &Value) -> Option<Value> {
    let kind = step["type"].as_str()?;
    if !STEP_TYPES.contains(&kind) { return None; }

    let mut out = serde_json::Map::new();
    out.insert("type".into(), json!(kind));

    match kind {
        "nav" => {
            let href = step["href"].as_str()?.trim();
            if !href.starts_with("http") { return None; }
            out.insert("href".into(), json!(href));
        }
        "action" => {
            let action = step["action"].as_str()?.trim();
            if action.is_empty() { return None; }
            out.insert("action".into(), json!(action));
        }
        "wait" => {
            // A wait with no condition is just a sleep; keep it bounded so a bad plan cannot hang.
            let condition = step["condition"].as_str().unwrap_or("appear");
            out.insert("condition".into(), json!(condition));
            if let Some(t) = step["target"].as_object() { out.insert("target".into(), json!(t)); }
            if let Some(c) = step["containerSelector"].as_str() { out.insert("containerSelector".into(), json!(c)); }
            if let Some(t) = step["text"].as_str() { out.insert("text".into(), json!(t)); }
            let quiet = step["quietMs"].as_i64().unwrap_or(1500).clamp(200, 60_000);
            out.insert("quietMs".into(), json!(quiet));
        }
        _ => {
            // Element-touching steps: the target must be resolvable by at least one strategy the
            // player supports — a stable selector, or button/link text.
            let t = step["target"].as_object()?;
            let has_selector = t.get("selector").and_then(|v| v.as_str()).map(|s| !s.trim().is_empty()).unwrap_or(false);
            let has_text = t.get("byText").and_then(|v| v.as_object())
                .map(|b| b.get("text").and_then(|x| x.as_str()).map(|s| !s.trim().is_empty()).unwrap_or(false))
                .unwrap_or(false);
            if !has_selector && !has_text { return None; }
            out.insert("target".into(), json!(t));
            if kind == "fill" || kind == "paste" {
                // `param: "text"` marks the field the caller fills at playback — that is what makes a
                // macro reusable across songs instead of hardcoding one post's text.
                if let Some(p) = step["param"].as_str() { out.insert("param".into(), json!(p)); }
                if let Some(v) = step["value"].as_str() { out.insert("value".into(), json!(v)); }
                if !out.contains_key("value") && !out.contains_key("param") { return None; }
            }
            if kind == "download" {
                out.insert("folderName".into(), json!(step["folderName"].as_str().unwrap_or("macro-download")));
            }
        }
    }
    if let Some(note) = step["note"].as_str().filter(|s| !s.trim().is_empty()) {
        out.insert("note".into(), json!(note.chars().take(120).collect::<String>()));
    }
    Some(Value::Object(out))
}

#[derive(serde::Deserialize)]
pub struct AuthorMacroRequest {
    /// What the macro should accomplish, in the user's words.
    pub goal: String,
    /// The page it starts on.
    pub url: String,
    /// A structural digest of that page, collected in the browser (see lib/macroAuthor.js).
    #[serde(default)]
    pub page: Value,
    /// Optional name; one is derived from the goal otherwise.
    #[serde(default)]
    pub name: Option<String>,
}

/// Write a macro for a goal on a page, from that page's own structure.
#[tauri::command]
pub async fn author_macro(state: State<'_, AppState>, payload: AuthorMacroRequest) -> Res<Value> {
    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    if payload.goal.trim().is_empty() { return Err("Say what the macro should do.".into()); }

    let elements = payload.page["elements"].as_array().cloned().unwrap_or_default();
    if elements.is_empty() {
        return Err("The page digest is empty — open the page in the app's browser and scan it first.".into());
    }

    let system = "You write browser automation macros for a studio app. You are given a GOAL and a \
         structural digest of the page the macro starts on: its interactive elements, each with a \
         stable `selector`, its visible `label`, its `role` and whether it accepts text.\n\n\
         Return a list of steps the app's macro player can execute. The vocabulary is fixed:\n\
         - {\"type\":\"nav\",\"href\":\"https://…\"} — go to a URL.\n\
         - {\"type\":\"click\",\"target\":{…}} — click an element.\n\
         - {\"type\":\"fill\",\"target\":{…},\"value\":\"…\"} — type a fixed value.\n\
         - {\"type\":\"fill\",\"target\":{…},\"param\":\"text\"} — type whatever the caller passes at \
           playback. USE THIS for the post body, title or caption: it is what makes the macro reusable.\n\
         - {\"type\":\"paste\",\"target\":{…},\"param\":\"text\"} — paste instead of typing (faster on long text).\n\
         - {\"type\":\"enter\",\"target\":{…}} — press Enter in a field.\n\
         - {\"type\":\"wait\",\"condition\":\"appear\",\"target\":{…},\"quietMs\":1500} — wait for something.\n\n\
         A `target` is {\"selector\":\"<css from the digest>\"} and may also carry \
         {\"byText\":{\"tag\":\"button\",\"text\":\"Post\"}} as a fallback. Only use selectors that appear \
         in the digest — never invent one.\n\n\
         Think about what a person actually does on this page, in order, including the confirmation \
         click at the end. Add a short `note` to any step whose purpose is not obvious.\n\n\
         Return ONLY: {\"name\":\"short name\",\"steps\":[…],\"caution\":\"what could break this macro, in one sentence\"}";

    let digest = serde_json::to_string_pretty(&json!({
        "url": payload.url,
        "title": payload.page["title"],
        "elements": elements.iter().take(120).collect::<Vec<_>>(),
    })).unwrap_or_default();
    let user = format!("GOAL: {}\n\nPAGE DIGEST:\n{digest}", payload.goal.trim());

    let (content, model) = crate::commands::ai::provider_chat(&settings, system, &user, 0.2, true).await?;
    let parsed = crate::commands::ai::extract_json_value(&content)
        .ok_or_else(|| format!("The AI returned no usable JSON (raw: {})", content.chars().take(200).collect::<String>()))?;

    let mut steps: Vec<Value> = Vec::new();
    let mut dropped = 0;
    // The macro always starts by navigating to the page it was written against, whether or not the
    // model remembered to say so — replaying from a different page is the most common failure.
    steps.push(json!({ "type": "nav", "href": payload.url }));
    for s in parsed["steps"].as_array().cloned().unwrap_or_default() {
        if s["type"].as_str() == Some("nav") && s["href"].as_str() == Some(payload.url.as_str()) { continue; }
        match sanitise_step(&s) { Some(ok) => steps.push(ok), None => dropped += 1 }
    }
    if steps.len() < 2 {
        return Err("None of the proposed steps were usable — try describing the goal more concretely.".into());
    }

    let macro_doc = json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "name": payload.name.clone()
            .or_else(|| parsed["name"].as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| payload.goal.chars().take(40).collect()),
        "goal": payload.goal.trim(),
        "url": payload.url,
        "steps": steps,
        "caution": parsed["caution"].as_str().unwrap_or("").chars().take(200).collect::<String>(),
        "authored_by": model,
        "dropped_steps": dropped,
        "created_at": crate::models::now_iso(),
    });
    if let Ok(d) = bson::to_document(&macro_doc) {
        state.db.collection::<Document>("authored_macros").insert_one(d).await.map_err(e)?;
    }
    Ok(macro_doc)
}

#[tauri::command]
pub async fn list_authored_macros(state: State<'_, AppState>) -> Res<Vec<Value>> {
    use futures_util::StreamExt;
    let mut cursor = state.db.collection::<Document>("authored_macros")
        .find(doc! {}).sort(doc! { "created_at": -1 }).await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }
    Ok(out)
}

#[tauri::command]
pub async fn delete_authored_macro(state: State<'_, AppState>, id: String) -> Res<Value> {
    state.db.collection::<Document>("authored_macros").delete_one(doc! { "id": &id }).await.map_err(e)?;
    Ok(json!({ "ok": true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_step_types_are_dropped() {
        // The player has a fixed vocabulary; a step it cannot execute would fail mid-run on a live
        // site, which is worse than never saving it.
        assert!(sanitise_step(&json!({ "type": "hover", "target": { "selector": "#a" } })).is_none());
        assert!(sanitise_step(&json!({ "type": "screenshot" })).is_none());
    }

    #[test]
    fn a_target_must_be_resolvable() {
        assert!(sanitise_step(&json!({ "type": "click", "target": {} })).is_none());
        assert!(sanitise_step(&json!({ "type": "click" })).is_none());
        assert!(sanitise_step(&json!({ "type": "click", "target": { "selector": "button#post" } })).is_some());
        assert!(sanitise_step(&json!({
            "type": "click", "target": { "byText": { "tag": "button", "text": "Post" } }
        })).is_some(), "button text alone is a strategy the player supports");
    }

    #[test]
    fn a_fill_needs_a_value_or_a_playback_parameter() {
        assert!(sanitise_step(&json!({ "type": "fill", "target": { "selector": "#t" } })).is_none());
        let param = sanitise_step(&json!({ "type": "fill", "target": { "selector": "#t" }, "param": "text" })).unwrap();
        assert_eq!(param["param"], "text", "the parameter marker is what makes a macro reusable");
        assert!(sanitise_step(&json!({ "type": "fill", "target": { "selector": "#t" }, "value": "hello" })).is_some());
    }

    #[test]
    fn nav_requires_an_absolute_url() {
        assert!(sanitise_step(&json!({ "type": "nav", "href": "/submit" })).is_none());
        assert!(sanitise_step(&json!({ "type": "nav", "href": "https://reddit.com/submit" })).is_some());
    }

    #[test]
    fn wait_timeouts_are_clamped() {
        let s = sanitise_step(&json!({ "type": "wait", "condition": "appear", "quietMs": 9_000_000 })).unwrap();
        assert_eq!(s["quietMs"], 60_000, "an unbounded wait would hang the run");
        let s2 = sanitise_step(&json!({ "type": "wait", "quietMs": 1 })).unwrap();
        assert_eq!(s2["quietMs"], 200);
    }
}
