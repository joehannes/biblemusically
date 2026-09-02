use crate::state::AppState;
use bson::{doc, Document};
use serde_json::Value;
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
// Guided workflow proposals
//
// The frontend renders a page as a short sequence of questions (see components/GuidedFlow.jsx). This
// is where the answers get *proposed*: given the flow's own steps and options, the project's brief,
// today's topic, the learnings store and the capabilities of the selected engines, the AI picks one
// option per step and says why in one line.
//
// Deliberate properties:
//   • It only ever picks from the options the UI already offers — the model cannot invent a control
//     that does not exist, so a bad answer is a bad suggestion, never a broken page.
//   • Every step gets an entry. Missing or unrecognised picks are dropped rather than guessed at,
//     and the UI falls back to the user's own last choice.
//   • It is advisory. Nothing is applied server-side; the user still clicks.
// ────────────────────────────────────────────────────────────────

// ────────────────────────────────────────────────────────────────
// Templates
//
// A guide that asks about everything is just the old form with extra clicks. So a view starts from a
// *template*: one pick that sets most controls at once, derived from what the studio already knows —
// the Dashboard brief, the channels' styles and regions, the declared goal, today's topic, the social
// feed, and the user's own history. Only what the template cannot reasonably decide is then asked.
//
// The frontend sends its control catalogue (id, type, allowed values, which section each belongs to)
// and the AI fills values for the controls it wants to set. Everything is validated against that
// catalogue before it reaches the UI: an unknown control, a value outside an enum, a number out of
// range — dropped. So the worst a bad model answer can do is under-fill a template.
// ────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize, Clone)]
pub struct ControlDescriptor {
    pub id: String,
    #[serde(default)]
    pub label: String,
    /// "enum" | "number" | "bool" | "text" | "list"
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub options: Vec<Value>,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    /// Which page section this control lives in — how a template knows what it has covered.
    #[serde(default)]
    pub section: String,
    #[serde(default)]
    pub hint: String,
}

#[derive(serde::Deserialize)]
pub struct GuideTemplateRequest {
    /// View id, e.g. "composer" — also the key the workflow state is stored under.
    pub view: String,
    #[serde(default)]
    pub goal: String,
    pub controls: Vec<ControlDescriptor>,
    /// Steps the flow could ask, so the template can declare which it makes unnecessary.
    #[serde(default)]
    pub steps: Vec<GuideStep>,
    #[serde(default)]
    pub context: Value,
}

/// Coerce one proposed value onto what the control actually accepts. `None` drops it.
fn coerce_value(ctrl: &ControlDescriptor, v: &Value) -> Option<Value> {
    match ctrl.kind.as_str() {
        "enum" => {
            // Match on the option's own value, or on a string that equals it case-insensitively.
            let matches = ctrl.options.iter().find(|o| {
                o == &v || match (o.as_str(), v.as_str()) {
                    (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                    _ => false,
                }
            });
            matches.cloned()
        }
        "number" => {
            let n = v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))?;
            let lo = ctrl.min.unwrap_or(f64::MIN);
            let hi = ctrl.max.unwrap_or(f64::MAX);
            Some(serde_json::json!(n.clamp(lo, hi)))
        }
        "bool" => Some(Value::Bool(match v {
            Value::Bool(b) => *b,
            Value::String(s) => matches!(s.trim().to_ascii_lowercase().as_str(), "true" | "yes" | "on" | "1"),
            Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
            _ => return None,
        })),
        "list" => {
            let arr = v.as_array().cloned().or_else(|| {
                // A model that answers with "a, b, c" still meant a list.
                v.as_str().map(|s| s.split(',').map(|p| Value::String(p.trim().to_string()))
                    .filter(|p| p.as_str().map(|s| !s.is_empty()).unwrap_or(false)).collect())
            })?;
            if arr.is_empty() { None } else { Some(Value::Array(arr)) }
        }
        // "text" and anything unrecognised: accept a non-empty string.
        _ => v.as_str().map(|s| s.trim()).filter(|s| !s.is_empty()).map(|s| Value::String(s.to_string())),
    }
}

/// Validate a proposed template patch against the control catalogue.
/// Returns the accepted patch and the ids that were rejected (surfaced for debugging, not to the user).
fn sanitise_patch(controls: &[ControlDescriptor], patch: &Value) -> (serde_json::Map<String, Value>, Vec<String>) {
    let mut clean = serde_json::Map::new();
    let mut rejected = Vec::new();
    let Some(obj) = patch.as_object() else { return (clean, rejected) };
    for (k, v) in obj {
        match controls.iter().find(|c| &c.id == k) {
            Some(ctrl) => match coerce_value(ctrl, v) {
                Some(ok) => { clean.insert(k.clone(), ok); }
                None => rejected.push(k.clone()),
            },
            None => rejected.push(k.clone()),
        }
    }
    (clean, rejected)
}

/// Propose a handful of "starter templates" for a view: aggregate preset packs, each a single choice
/// that fills most of the page in a direction the user can recognise as theirs.
#[tauri::command]
pub async fn guide_templates(state: State<'_, AppState>, payload: GuideTemplateRequest) -> Res<Value> {
    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let project_id = payload.context["project_id"].as_str().unwrap_or("").to_string();

    let brief = crate::commands::ai::project_brief_block(&state.db, &project_id).await;
    let learnings = crate::commands::learnings::learnings_prompt_block(&state, &project_id).await;
    let past = past_choices_block(&state, &project_id, &payload.view).await;
    let research = crate::commands::ai::channel_research_block(&state.db).await;
    let social = social_signal_block(&state.db, &project_id).await;
    // What was actually watched, where there is enough of it to mean anything. See
    // `performance_prompt_block` for why this is separate from — and outranks — the habit blocks.
    let performance = crate::commands::insights::performance_prompt_block(&state, &project_id).await;

    let catalogue: Vec<Value> = payload.controls.iter().map(|c| serde_json::json!({
        "id": c.id, "label": c.label, "kind": c.kind, "section": c.section,
        "options": c.options, "min": c.min, "max": c.max, "hint": c.hint,
    })).collect();
    let steps_json: Vec<Value> = payload.steps.iter().map(|s| serde_json::json!({
        "id": s.id, "question": s.question,
        "options": s.options.iter().map(|o| serde_json::json!({ "id": o.id, "label": o.label })).collect::<Vec<_>>(),
    })).collect();

    let system = format!(
        "You prepare a creator's workspace before they touch it.\n\n\
         For the '{view}' view you receive: a CONTROL CATALOGUE (every setting the page has, with its \
         allowed values and the section it belongs to), the STEPS the guide could otherwise ask, and \
         everything the studio knows about this creator — their brief, their channels, their goal, \
         today's topic, their social signal, and their own history.\n\n\
         Propose 3 or 4 TEMPLATES. A template is one recognisable creative direction — not a random \
         mix of settings — that a user reads and thinks 'yes, that is what I am doing today'. For each:\n\
         - `label`: ≤5 words, concrete and evocative, no marketing words.\n\
         - `pitch`: one sentence on what the result will feel like.\n\
         - `why`: ≤15 words tying it to something real about THIS creator (their brief, a channel's \
           region, today's topic, what they keep choosing).\n\
         - `patch`: control id → value, for every control this direction can reasonably decide. Use \
           ONLY ids from the catalogue and values its `options`/`min`/`max` allow. Set as many as the \
           direction genuinely implies — that is the point of a template — but leave a control out when \
           the choice is really the user's (their target channels, whether to publish).\n\
         - `covers`: the section ids your patch settles.\n\
         - `open_questions`: step ids that remain genuinely open after your patch, most important first, \
           at most 4. These are what the user will still be asked, so choose the ones that change the \
           result the most.\n\
         - `confidence`: 0.0–1.0, how well this fits what you know.\n\n\
         Make the templates meaningfully different from each other — the same page filled three ways, \
         not one direction with three labels. Never invent a control id.\n\n\
         Return ONLY: {{\"templates\":[{{\"label\":…,\"pitch\":…,\"why\":…,\"patch\":{{…}},\"covers\":[…],\"open_questions\":[…],\"confidence\":0.0}}]}}",
        view = payload.view,
    );

    let user = format!(
        "{brief}{learnings}{past}{performance}{research}{social}\nDECLARED GOAL: {}\n\nENGINES:\n{}\n\nCONTROL CATALOGUE:\n{}\n\nPOSSIBLE STEPS:\n{}\n\nOTHER CONTEXT:\n{}",
        if payload.goal.trim().is_empty() { "(not stated — infer from the brief)" } else { payload.goal.trim() },
        serde_json::to_string_pretty(&payload.context["engines"]).unwrap_or_else(|_| "{}".into()),
        serde_json::to_string_pretty(&catalogue).unwrap_or_else(|_| "[]".into()),
        serde_json::to_string_pretty(&steps_json).unwrap_or_else(|_| "[]".into()),
        serde_json::to_string_pretty(&payload.context).unwrap_or_else(|_| "{}".into()),
    );

    let (content, model) = crate::commands::ai::provider_chat(&settings, &system, &user, 0.7, true).await?;
    let parsed = crate::commands::ai::extract_json_value(&content).unwrap_or(Value::Null);

    let mut out = Vec::new();
    let step_ids: Vec<&str> = payload.steps.iter().map(|s| s.id.as_str()).collect();
    let section_ids: Vec<&str> = payload.controls.iter().map(|c| c.section.as_str()).collect();
    if let Some(arr) = parsed["templates"].as_array() {
        for (i, t) in arr.iter().take(4).enumerate() {
            let (patch, rejected) = sanitise_patch(&payload.controls, &t["patch"]);
            if patch.is_empty() { continue; }   // a template that sets nothing is not a template
            // Keep only real step ids and real section ids.
            let open: Vec<Value> = t["open_questions"].as_array().map(|a| a.iter()
                .filter(|q| q.as_str().map(|s| step_ids.contains(&s)).unwrap_or(false))
                .take(4).cloned().collect()).unwrap_or_default();
            let covers: Vec<Value> = t["covers"].as_array().map(|a| a.iter()
                .filter(|c| c.as_str().map(|s| section_ids.contains(&s)).unwrap_or(false))
                .cloned().collect()).unwrap_or_else(|| {
                    // Fall back to the sections the patch actually touches.
                    let mut seen: Vec<Value> = Vec::new();
                    for k in patch.keys() {
                        if let Some(c) = payload.controls.iter().find(|c| &c.id == k) {
                            let v = Value::String(c.section.clone());
                            if !c.section.is_empty() && !seen.contains(&v) { seen.push(v); }
                        }
                    }
                    seen
                });
            out.push(serde_json::json!({
                "id": format!("ai-{i}"),
                "label": t["label"].as_str().unwrap_or("Suggested direction").chars().take(60).collect::<String>(),
                "pitch": t["pitch"].as_str().unwrap_or("").chars().take(200).collect::<String>(),
                "why": t["why"].as_str().unwrap_or("").chars().take(140).collect::<String>(),
                "confidence": t["confidence"].as_f64().unwrap_or(0.5).clamp(0.0, 1.0),
                "patch": Value::Object(patch),
                "covers": covers,
                "open_questions": open,
                "rejected": rejected,
                "source": "ai",
            }));
        }
    }

    Ok(serde_json::json!({ "templates": out, "model": model }))
}

// ────────────────────────────────────────────────────────────────
// Setup recommendation
//
// First-run setup used to be a fixed list of everything the app can connect to, which is the wrong
// question: somebody trying the app out and somebody publishing to fifty channels need different
// halves of it. So the guide asks for the goal, and this works out what that goal actually requires —
// what is needed now, what can wait, and what to preset so the user never sees the choice at all.
//
// Step ids and setting keys are whitelisted: the model chooses *among* the app's real setup steps,
// it cannot invent one.
// ────────────────────────────────────────────────────────────────

/// The setup steps the first-run guide knows how to show. The model may only speak about these.
const SETUP_STEPS: &[(&str, &str)] = &[
    ("language", "Interface language"),
    ("goal", "What the user is trying to do"),
    ("voice", "Assistant voice and spoken answers"),
    ("permissions", "Microphone and folder access"),
    ("ai", "AI provider key (lyrics, styles, planning)"),
    ("kaggle", "Free GPU servers for music and images"),
    ("youtube", "Google account for publishing"),
    ("files", "Where generated files are stored"),
    ("sync", "Backing the project up to a git remote"),
    ("remote_render", "Rendering and uploading off this machine"),
];

/// Settings a recommendation may preset, so the user is not asked about them at all.
fn setup_presets() -> Vec<ControlDescriptor> {
    let enum_ctrl = |id: &str, section: &str, opts: Vec<&str>| ControlDescriptor {
        id: id.into(), label: id.into(), kind: "enum".into(),
        options: opts.into_iter().map(|o| Value::String(o.into())).collect(),
        min: None, max: None, section: section.into(), hint: String::new(),
    };
    vec![
        enum_ctrl("music_engine", "ai", vec!["heartmula", "acestep", "suno"]),
        enum_ctrl("image_engine", "ai", vec!["comfy", "flux", "midjourney", "gemini"]),
        enum_ctrl("ai_provider", "ai", vec!["openrouter", "gemini"]),
        enum_ctrl("remote_render_provider", "remote_render", vec!["local", "kaggle", "actions", "modal", "http"]),
        ControlDescriptor {
            id: "is_christian".into(), label: "Christian content mode".into(), kind: "bool".into(),
            options: vec![], min: None, max: None, section: "goal".into(), hint: String::new(),
        },
    ]
}

#[derive(serde::Deserialize)]
pub struct SetupRecommendationRequest {
    /// What the user said they want, in their own words or as a picked phrase.
    pub goal: String,
    /// Which steps are already satisfied, so the answer is about what is left.
    #[serde(default)]
    pub done: Vec<String>,
    #[serde(default)]
    pub context: Value,
}

/// Work out what this user's goal actually requires, and what can be decided for them.
#[tauri::command]
pub async fn setup_recommendation(state: State<'_, AppState>, payload: SetupRecommendationRequest) -> Res<Value> {
    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();

    let presets = setup_presets();
    let steps_json: Vec<Value> = SETUP_STEPS.iter()
        .map(|(id, what)| serde_json::json!({ "id": id, "what": what, "already_done": payload.done.iter().any(|d| d == id) }))
        .collect();

    let system = "You set up a Bible music-video studio for one creator, from their goal.\n\n\
         You get the app's real setup steps and the settings you are allowed to preset. Decide, for \
         each step that is not already done, whether it is `required` for this goal, `optional` (worth \
         doing later), or `skip` (this goal does not need it at all) — and say why in at most 12 words, \
         addressed to the user.\n\n\
         Facts to reason with, not to repeat: the free path is Kaggle GPUs for music/images, an \
         OpenRouter or Gemini key for text (OpenRouter's free tier is 50 requests a day, Gemini's is \
         per-minute and often busy), a Google OAuth client per YouTube account, and remote rendering on \
         Kaggle CPU sessions when many videos a week are planned. Somebody just trying the app out \
         needs the AI key and nothing else. Somebody publishing daily to many channels needs \
         publishing, storage and remote rendering early.\n\n\
         Preset only what the goal clearly implies, using the given ids and allowed values.\n\n\
         Return ONLY: {\"summary\":\"one sentence, ≤25 words, what you will set up for them\",\
         \"steps\":[{\"id\":…,\"need\":\"required|optional|skip\",\"why\":…}],\"presets\":{…}}";

    let user = format!(
        "GOAL (their words): {}\n\nSETUP STEPS:\n{}\n\nPRESETTABLE SETTINGS:\n{}\n\nCONTEXT:\n{}",
        payload.goal.trim(),
        serde_json::to_string_pretty(&steps_json).unwrap_or_else(|_| "[]".into()),
        serde_json::to_string_pretty(&presets.iter().map(|c| serde_json::json!({
            "id": c.id, "kind": c.kind, "options": c.options,
        })).collect::<Vec<_>>()).unwrap_or_else(|_| "[]".into()),
        serde_json::to_string_pretty(&payload.context).unwrap_or_else(|_| "{}".into()),
    );

    let (content, model) = crate::commands::ai::provider_chat(&settings, system, &user, 0.4, true).await?;
    let parsed = crate::commands::ai::extract_json_value(&content).unwrap_or(Value::Null);

    // Keep only real step ids and real need levels; anything else is dropped rather than shown.
    let mut steps = Vec::new();
    if let Some(arr) = parsed["steps"].as_array() {
        for s in arr {
            let id = s["id"].as_str().unwrap_or("");
            if !SETUP_STEPS.iter().any(|(k, _)| *k == id) { continue; }
            let need = match s["need"].as_str().unwrap_or("optional") {
                "required" => "required", "skip" => "skip", _ => "optional",
            };
            steps.push(serde_json::json!({
                "id": id, "need": need,
                "why": s["why"].as_str().unwrap_or("").chars().take(120).collect::<String>(),
            }));
        }
    }
    let (clean_presets, _rejected) = sanitise_patch(&presets, &parsed["presets"]);

    Ok(serde_json::json!({
        "summary": parsed["summary"].as_str().unwrap_or("").chars().take(200).collect::<String>(),
        "steps": steps,
        "presets": Value::Object(clean_presets),
        "model": model,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctrl(id: &str, kind: &str) -> ControlDescriptor {
        ControlDescriptor {
            id: id.into(), label: id.into(), kind: kind.into(),
            options: vec![], min: None, max: None, section: "sec".into(), hint: String::new(),
        }
    }

    #[test]
    fn unknown_controls_are_dropped() {
        // The catalogue is the boundary: a model that invents a setting must not reach the UI.
        let controls = vec![ctrl("theme_global", "text")];
        let patch = serde_json::json!({ "theme_global": "sacred", "delete_everything": true });
        let (clean, rejected) = sanitise_patch(&controls, &patch);
        assert_eq!(clean.len(), 1);
        assert_eq!(clean["theme_global"], serde_json::json!("sacred"));
        assert_eq!(rejected, vec!["delete_everything".to_string()]);
    }

    #[test]
    fn enum_values_must_be_offered() {
        let mut c = ctrl("aspect_ratio", "enum");
        c.options = vec![serde_json::json!("16:9"), serde_json::json!("9:16")];
        let controls = vec![c];

        let (clean, _) = sanitise_patch(&controls, &serde_json::json!({ "aspect_ratio": "16:9" }));
        assert_eq!(clean["aspect_ratio"], serde_json::json!("16:9"));

        // Case-insensitive matching is a courtesy; an unlisted value is still refused.
        let (clean, rejected) = sanitise_patch(&controls, &serde_json::json!({ "aspect_ratio": "21:9" }));
        assert!(clean.is_empty());
        assert_eq!(rejected, vec!["aspect_ratio".to_string()]);
    }

    #[test]
    fn numbers_are_clamped_not_rejected() {
        let mut c = ctrl("stylize", "number");
        c.min = Some(0.0);
        c.max = Some(1000.0);
        let controls = vec![c];
        let (clean, _) = sanitise_patch(&controls, &serde_json::json!({ "stylize": 5000 }));
        assert_eq!(clean["stylize"], serde_json::json!(1000.0), "out of range clamps to the ceiling");
        // A number sent as a string is still a number.
        let (clean, _) = sanitise_patch(&controls, &serde_json::json!({ "stylize": "250" }));
        assert_eq!(clean["stylize"], serde_json::json!(250.0));
    }

    #[test]
    fn bools_accept_the_shapes_models_actually_send() {
        let controls = vec![ctrl("generate_lyrics", "bool")];
        for (input, expected) in [
            (serde_json::json!(true), true),
            (serde_json::json!("yes"), true),
            (serde_json::json!("false"), false),
            (serde_json::json!(1), true),
            (serde_json::json!(0), false),
        ] {
            let (clean, _) = sanitise_patch(&controls, &serde_json::json!({ "generate_lyrics": input }));
            assert_eq!(clean["generate_lyrics"], serde_json::json!(expected));
        }
    }

    #[test]
    fn lists_accept_a_comma_string() {
        let controls = vec![ctrl("style_keywords", "list")];
        let (clean, _) = sanitise_patch(&controls, &serde_json::json!({ "style_keywords": "gold leaf, byzantine, ancient mosaic" }));
        assert_eq!(clean["style_keywords"].as_array().unwrap().len(), 3);
        // An empty list carries no intent, so it is dropped rather than clearing the user's keywords.
        let (clean, rejected) = sanitise_patch(&controls, &serde_json::json!({ "style_keywords": [] }));
        assert!(clean.is_empty());
        assert_eq!(rejected, vec!["style_keywords".to_string()]);
    }

    #[test]
    fn empty_text_is_not_a_value() {
        let controls = vec![ctrl("theme_global", "text")];
        let (clean, rejected) = sanitise_patch(&controls, &serde_json::json!({ "theme_global": "   " }));
        assert!(clean.is_empty(), "blank text must not overwrite a real theme");
        assert_eq!(rejected, vec!["theme_global".to_string()]);
    }
}

/// Recent social signal for this project, as a short prompt block. Empty when there is none.
///
/// Included because "what my audience is reacting to" is exactly the kind of thing a template should
/// weigh, and the app already collects it.
async fn social_signal_block(db: &crate::store::Db, project_id: &str) -> String {
    use futures_util::StreamExt;
    let filter = if project_id.is_empty() { doc! {} } else { doc! { "project_id": project_id } };
    let Ok(mut cursor) = db.collection::<Document>("social_posts")
        .find(filter).sort(doc! { "created_at": -1 }).limit(8).await else { return String::new() };
    let mut lines = Vec::new();
    while let Some(Ok(d)) = cursor.next().await {
        let p = bson_to_value(d);
        let text = p["text"].as_str().or_else(|| p["caption"].as_str()).unwrap_or("");
        if text.trim().is_empty() { continue; }
        let platform = p["platform"].as_str().unwrap_or("social");
        lines.push(format!("- {platform}: {}", text.chars().take(120).collect::<String>()));
    }
    if lines.is_empty() { return String::new(); }
    format!("RECENT SOCIAL POSTS (what this creator has been putting out):\n{}\n", lines.join("\n"))
}

// ────────────────────────────────────────────────────────────────
// Workflow state
//
// A guided session is not a modal — it is a place the user can leave and come back to. The state of
// each view (which template, which answers, how far each section got, where the guide paused) is
// therefore persisted per project, so returning to a page shows what was decided, in what state, and
// offers to pick up where it stopped.
// ────────────────────────────────────────────────────────────────

fn state_id(project_id: &str, view: &str) -> String { format!("{project_id}:{view}") }

#[tauri::command]
pub async fn get_workflow_state(state: State<'_, AppState>, project_id: String, view: String) -> Res<Value> {
    let doc = state.db.collection::<Document>("workflow_states")
        .find_one(doc! { "_id": state_id(&project_id, &view) }).await.map_err(e)?;
    Ok(doc.map(bson_to_value).unwrap_or_else(|| serde_json::json!({
        "project_id": project_id, "view": view,
        "template": Value::Null, "answers": {}, "sections": {}, "cursor": 0, "completed": false,
    })))
}

/// Merge a patch into the stored state. Shallow per top-level key, which is what the caller wants:
/// `answers` and `sections` arrive as complete maps, `cursor`/`template` as scalars.
#[tauri::command]
pub async fn save_workflow_state(state: State<'_, AppState>, project_id: String, view: String, patch: Value) -> Res<Value> {
    let id = state_id(&project_id, &view);
    let mut set = doc! {
        "project_id": &project_id,
        "view": &view,
        "updated_at": crate::models::now_iso(),
    };
    if let Some(obj) = patch.as_object() {
        for (k, v) in obj {
            if k == "_id" { continue; }
            if let Ok(b) = bson::to_bson(v) { set.insert(k.clone(), b); }
        }
    }
    state.db.collection::<Document>("workflow_states")
        .update_one(doc! { "_id": &id }, doc! { "$set": set }).upsert(true)
        .await.map_err(e)?;
    get_workflow_state(state, project_id, view).await
}

/// Forget a view's guided progress (the "start over" button).
#[tauri::command]
pub async fn reset_workflow_state(state: State<'_, AppState>, project_id: String, view: String) -> Res<Value> {
    state.db.collection::<Document>("workflow_states")
        .delete_one(doc! { "_id": state_id(&project_id, &view) }).await.map_err(e)?;
    Ok(serde_json::json!({ "ok": true }))
}

#[derive(serde::Deserialize)]
pub struct GuideStepOption {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub hint: String,
}

#[derive(serde::Deserialize)]
pub struct GuideStep {
    pub id: String,
    #[serde(default)]
    pub question: String,
    #[serde(default)]
    pub options: Vec<GuideStepOption>,
}

#[derive(serde::Deserialize)]
pub struct GuideProposalRequest {
    /// Flow id, e.g. "composer" — also the key under which choices are learned.
    pub flow: String,
    #[serde(default)]
    pub title: String,
    pub steps: Vec<GuideStep>,
    /// Page-supplied context: selected engines and their capabilities, channels, current config.
    #[serde(default)]
    pub context: Value,
}

/// What this user has chosen before in this flow, newest first, as a compact prompt block.
///
/// Read straight from the learning signals the guide itself records, so the proposals visibly bend
/// toward the user's habits after a few sessions instead of restarting from generic defaults.
async fn past_choices_block(state: &AppState, project_id: &str, flow: &str) -> String {
    let signals = crate::commands::learnings::recent_signals(state, project_id, "guided_choice", 80).await;
    let prefix = format!("{flow}:");
    // Newest first per step, deduplicated: the model needs the habit, not a hundred repetitions.
    let mut per_step: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for sig in &signals {
        let key = sig["key"].as_str().unwrap_or("");
        if !key.starts_with(&prefix) { continue; }
        let step = key[prefix.len()..].to_string();
        let picked = sig["detail"].as_str().unwrap_or("").to_string();
        if picked.is_empty() { continue; }
        let entry = per_step.entry(step).or_default();
        if entry.len() < 4 && !entry.contains(&picked) { entry.push(picked); }
    }
    if per_step.is_empty() { return String::new(); }
    let mut lines: Vec<String> = per_step.into_iter()
        .map(|(step, picks)| format!("- {step}: {} (most recent first)", picks.join(", ")))
        .collect();
    lines.sort();
    format!("THIS USER'S PAST CHOICES IN THIS FLOW (weigh them heavily — they are habits, not accidents):\n{}\n", lines.join("\n"))
}

#[tauri::command]
pub async fn guide_proposal(state: State<'_, AppState>, payload: GuideProposalRequest) -> Res<Value> {
    if payload.steps.is_empty() {
        return Ok(serde_json::json!({ "picks": {} }));
    }

    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();

    let project_id = payload.context["project_id"].as_str().unwrap_or("").to_string();
    let brief = crate::commands::ai::project_brief_block(&state.db, &project_id).await;
    let learnings = crate::commands::learnings::learnings_prompt_block(&state, &project_id).await;
    let past = past_choices_block(&state, &project_id, &payload.flow).await;
    let performance = crate::commands::insights::performance_prompt_block(&state, &project_id).await;

    // The options are handed over as a closed list, and the model is told to answer with ids only.
    let steps_json: Vec<Value> = payload.steps.iter().map(|s| serde_json::json!({
        "id": s.id,
        "question": s.question,
        "options": s.options.iter().map(|o| serde_json::json!({
            "id": o.id, "label": o.label, "hint": o.hint,
        })).collect::<Vec<_>>(),
    })).collect();

    let system = format!(
        "You are the studio's guide: you walk one creator through a production workflow, proposing the \
         choice that fits THEIR project rather than a generic best practice.\n\n\
         You receive the steps of the '{}' flow with a closed list of options per step. For every step, \
         pick exactly one option id FROM THAT LIST and give a reason of at most 12 words, addressed to \
         the user, referring to something concrete about their project, their engine or their habits \
         (\"your brief asks for warmth\", \"HeartMuLa ignores performance tags\", \"you picked this the \
         last three times\", \"this combination doubles your median views\").\n\n\
         Rules: never invent an option id; never suggest a control the selected engine does not support; \
         prefer what their published videos actually did over what they habitually pick, and say so when \
         the two disagree (\"your Spanish uploads out-perform the rest\"); otherwise prefer their past \
         choices unless the brief or today's topic argues against them; never cite a measurement that was \
         not given to you, and never treat a row marked thin as settled; if a step has no meaningful \
         preference, still answer with the safest option and say why it is safe.\n\n\
         Return ONLY this JSON: {{\"greeting\":\"one sentence, ≤20 words, what today's run is about\", \
         \"picks\":[{{\"step\":\"<step id>\",\"option\":\"<option id>\",\"why\":\"…\"}}]}}",
        payload.title,
    );

    let user = format!(
        "{brief}{learnings}{past}{performance}\nENGINES AND CAPABILITIES (only offer what these support):\n{}\n\nCURRENT PAGE CONTEXT:\n{}\n\nSTEPS:\n{}",
        serde_json::to_string_pretty(&payload.context["engines"]).unwrap_or_else(|_| "{}".into()),
        serde_json::to_string_pretty(&payload.context).unwrap_or_else(|_| "{}".into()),
        serde_json::to_string_pretty(&steps_json).unwrap_or_else(|_| "[]".into()),
    );

    let (content, model) = crate::commands::ai::provider_chat(&settings, &system, &user, 0.4, true).await?;
    let parsed = crate::commands::ai::extract_json_value(&content).unwrap_or(Value::Null);

    // Keep only picks that name a real step and a real option for it.
    let mut picks = serde_json::Map::new();
    if let Some(arr) = parsed["picks"].as_array() {
        for p in arr {
            let step_id = p["step"].as_str().unwrap_or("");
            let option_id = p["option"].as_str().unwrap_or("");
            let Some(step) = payload.steps.iter().find(|s| s.id == step_id) else { continue };
            if !step.options.iter().any(|o| o.id == option_id) { continue; }
            picks.insert(step_id.to_string(), serde_json::json!({
                "option": option_id,
                "why": p["why"].as_str().unwrap_or("").chars().take(120).collect::<String>(),
            }));
        }
    }

    Ok(serde_json::json!({
        "picks": Value::Object(picks),
        "greeting": parsed["greeting"].as_str().unwrap_or("").chars().take(200).collect::<String>(),
        "model": model,
    }))
}
