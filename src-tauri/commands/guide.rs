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
         last three times\").\n\n\
         Rules: never invent an option id; never suggest a control the selected engine does not support; \
         prefer the user's past choices unless the brief or today's topic argues against them; if a step \
         has no meaningful preference, still answer with the safest option and say why it is safe.\n\n\
         Return ONLY this JSON: {{\"greeting\":\"one sentence, ≤20 words, what today's run is about\", \
         \"picks\":[{{\"step\":\"<step id>\",\"option\":\"<option id>\",\"why\":\"…\"}}]}}",
        payload.title,
    );

    let user = format!(
        "{brief}{learnings}{past}\nENGINES AND CAPABILITIES (only offer what these support):\n{}\n\nCURRENT PAGE CONTEXT:\n{}\n\nSTEPS:\n{}",
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
