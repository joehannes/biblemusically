//! Two conversations: the one that starts a project, and the one that starts a day.
//!
//! Both exist because the guided layer had a shape problem rather than a coverage problem. Fourteen
//! pages have a guided flow, and every one of them begins and ends inside its own page — so a person
//! who has just made a project is guided *within* whichever of thirty-five doors they happen to open,
//! and guided nowhere at all about which door that should be.
//!
//! ## The interview
//!
//! A project's Brief is eight free-text boxes (mood, attitude, motivation, goals, audience,
//! storyline, humor, personality) and everything downstream reads it — lyrics, per-channel style,
//! imagery, characters. Facing eight empty boxes is the same wall as facing thirty-five nav entries,
//! and the usual answer, a fixed wizard, is barely better: question four is worth asking only for
//! some answers to question two.
//!
//! So the questions **cascade**. Each call hands the model everything answered so far and asks for
//! the single next question worth asking, with two to five concrete options *and* the freedom to
//! answer in your own words. A project about a children's Bible-story channel and one about grief
//! poetry diverge after the first answer, and neither sits through the other's questions.
//!
//! Three properties keep it honest:
//!   * **It writes only into the Brief's own fields.** The model names a field; anything it invents
//!     is dropped. So a bad answer is a wasted question, never a corrupted project.
//!   * **It can always be finished early.** `done` is a state the caller can force, not only one the
//!     model can reach — nobody is held in a conversation they want out of.
//!   * **Without an AI it still works**, from a fixed opening set. A free tier that has run out must
//!     not mean a project cannot be started.
//!
//! ## The day
//!
//! `guide_today` answers "what should I do now?" from what the project actually contains rather than
//! from a script: the songs at each pipeline stage, what is stalled, what was done today already.
//! The steps it returns each name a route, so the answer is clickable rather than advisory, and the
//! reason is about this project ("four songs have audio and no images") rather than about the app.

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

/// The Brief's fields, mirroring `BRIEF_FIELDS` in components/ProjectBrief.jsx.
///
/// Listed here so the model's answer can be validated against them: a question that writes into a
/// field the Brief does not have is a question whose answer disappears.
pub const BRIEF_FIELDS: &[(&str, &str)] = &[
    ("mood", "how it should feel"),
    ("attitude", "the stance it takes toward the listener"),
    ("motivation", "why this project exists"),
    ("goals", "what success looks like"),
    ("audience", "who it is for"),
    ("storyline", "the arc across songs"),
    ("humor", "how much, and what kind"),
    ("personality", "the channel's voice as if it were a person"),
];

pub fn is_brief_field(name: &str) -> bool {
    BRIEF_FIELDS.iter().any(|(k, _)| *k == name)
}

/// How many questions before the interview stops asking, whatever the model wants.
///
/// Eight fields, and an interview that asked about each in turn would be the form it replaces. The
/// cap is above the number of fields on purpose — a good cascade sometimes returns to a field to
/// sharpen it — but far enough below "endless" that nobody is trapped.
pub const MAX_QUESTIONS: usize = 12;

/// The opening question, used when there is no AI to ask for one.
///
/// Deliberately the widest one: every later question in every branch depends on the answer, and it
/// is the only question whose usefulness does not depend on knowing anything yet.
fn fallback_question(answered: &[String]) -> Option<Value> {
    // Each entry: field, question, and options that are directions rather than values, since the
    // point is to get somebody talking, not to make them pick from a menu of adjectives.
    const SEED: &[(&str, &str, &[&str])] = &[
        ("motivation", "What is this project for?", &[
            "Sharing scripture in a way people will actually listen to",
            "A daily practice, for me as much as anyone",
            "Building an audience around one voice",
            "Something for my own family",
        ]),
        ("audience", "Who do you picture hearing it?", &[
            "People who already know the text",
            "People who have never opened it",
            "Children, or a family listening together",
            "Anyone the algorithm brings",
        ]),
        ("mood", "How should it feel when it lands?", &[
            "Comforting and quiet",
            "Bright and celebratory",
            "Serious, with weight",
            "Playful",
        ]),
        ("personality", "If the channel were a person, who?", &[
            "A friend who has been through it",
            "A teacher who never talks down",
            "A poet more than a preacher",
            "A storyteller for children",
        ]),
    ];
    SEED.iter()
        .find(|(field, _, _)| !answered.iter().any(|a| a == field))
        .map(|(field, question, options)| json!({
            "field": field,
            "question": question,
            "why": "",
            "options": options.iter().map(|o| json!({ "label": o })).collect::<Vec<_>>(),
            "source": "fallback",
        }))
}

#[derive(serde::Deserialize)]
pub struct InterviewRequest {
    pub project_id: String,
    /// `field` → what the user said. Both the picked option's label and free text land here.
    #[serde(default)]
    pub answers: Value,
    /// Set when the user says they are finished; the caller does not need the model's permission.
    #[serde(default)]
    pub finish: bool,
}

/// The next question worth asking, or `done`.
///
/// Cascading rather than scripted: the model sees every answer so far and decides what is still worth
/// knowing, which is why a children's-story project and a grief-poetry project stop sharing a path
/// after the first answer.
#[tauri::command]
pub async fn project_interview_next(state: State<'_, AppState>, payload: InterviewRequest) -> Res<Value> {
    let answered: Vec<String> = payload.answers.as_object()
        .map(|o| o.keys().filter(|k| is_brief_field(k)).cloned().collect())
        .unwrap_or_default();

    if payload.finish || answered.len() >= MAX_QUESTIONS {
        return Ok(json!({ "done": true, "answered": answered.len() }));
    }

    let project = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &payload.project_id }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();

    let remaining: Vec<&str> = BRIEF_FIELDS.iter()
        .map(|(k, _)| *k)
        .filter(|k| !answered.iter().any(|a| a == k))
        .collect();
    if remaining.is_empty() {
        return Ok(json!({ "done": true, "answered": answered.len() }));
    }

    let system = "You are interviewing somebody who has just started a project in a studio that turns \
         text into music videos. Your job is to ask ONE question — the single most useful thing still \
         unknown about what they are making.\n\n\
         Cascade: read what they have already said and go where it leads. Do not walk a checklist. If \
         an answer implies another field, ask about that; if an answer already told you something, do \
         not ask it again in other words.\n\n\
         The question must be answerable in a breath, in plain language, about THEIR project — never \
         about the app, never about a setting, never a term of art. Offer 2 to 4 options that are \
         real directions somebody would recognise as theirs, not a menu of adjectives. They can \
         always answer in their own words instead, so the options are a way in rather than a cage.\n\n\
         Return ONLY: {\"field\":\"<one of the allowed fields>\",\"question\":\"…\",\
         \"why\":\"at most 12 words, why this is worth asking now\",\
         \"options\":[{\"label\":\"…\"},…]}";

    let user = format!(
        "PROJECT: {}\nTOPIC: {}\n\nALREADY ANSWERED (do not ask these again):\n{}\n\n\
         FIELDS STILL UNANSWERED (pick exactly one):\n{}\n\n\
         This is question {} of at most {}.",
        project["name"].as_str().unwrap_or("(unnamed)"),
        project["topic"].as_str().unwrap_or(""),
        if answered.is_empty() { "(nothing yet — this is the first question)".to_string() }
        else { serde_json::to_string_pretty(&payload.answers).unwrap_or_default() },
        remaining.iter()
            .map(|k| format!("- {k}: {}", BRIEF_FIELDS.iter().find(|(f, _)| f == k).map(|(_, d)| *d).unwrap_or("")))
            .collect::<Vec<_>>().join("\n"),
        answered.len() + 1, MAX_QUESTIONS,
    );

    // No AI, or a refusal: the fixed opening set. Somebody must be able to start a project on a
    // spent free tier.
    let Ok((content, model)) = crate::commands::ai::provider_chat(&settings, system, &user, 0.7, true).await
    else {
        return Ok(match fallback_question(&answered) {
            Some(q) => json!({ "done": false, "question": q, "answered": answered.len() }),
            None => json!({ "done": true, "answered": answered.len() }),
        });
    };
    let parsed = crate::commands::ai::extract_json_value(&content).unwrap_or(Value::Null);

    // A field the Brief does not have is an answer with nowhere to go, so it is not asked.
    let field = parsed["field"].as_str().unwrap_or("");
    let question = parsed["question"].as_str().unwrap_or("").trim();
    if !is_brief_field(field) || question.is_empty() || answered.iter().any(|a| a == field) {
        return Ok(match fallback_question(&answered) {
            Some(q) => json!({ "done": false, "question": q, "answered": answered.len() }),
            None => json!({ "done": true, "answered": answered.len() }),
        });
    }

    let options: Vec<Value> = parsed["options"].as_array().map(|a| a.iter()
        .filter_map(|o| o["label"].as_str().map(|l| json!({ "label": l.trim() })))
        .filter(|o| !o["label"].as_str().unwrap_or("").is_empty())
        .take(4).collect()).unwrap_or_default();

    Ok(json!({
        "done": false,
        "answered": answered.len(),
        "remaining": remaining.len(),
        "question": {
            "field": field,
            "question": question,
            "why": parsed["why"].as_str().unwrap_or("").chars().take(80).collect::<String>(),
            "options": options,
            "source": model,
        },
    }))
}

/// Write the interview's answers into the project's Brief.
///
/// Separate from asking, so leaving half-way keeps what was said. Only Brief fields are written;
/// anything else in the payload is ignored rather than merged, because this writes into the document
/// every generation prompt reads.
#[tauri::command]
pub async fn project_interview_save(state: State<'_, AppState>, project_id: String, answers: Value) -> Res<Value> {
    let Some(obj) = answers.as_object() else { return Err("answers must be an object".into()) };
    let mut set = Document::new();
    let mut written = Vec::new();
    for (k, v) in obj {
        if !is_brief_field(k) { continue; }
        let Some(text) = v.as_str().map(str::trim).filter(|s| !s.is_empty()) else { continue };
        set.insert(format!("brief.{k}"), text);
        written.push(k.clone());
    }
    if set.is_empty() { return Ok(json!({ "ok": true, "written": 0 })); }
    set.insert("brief_interviewed_at", crate::models::now_iso());
    state.db.collection::<Document>("projects")
        .update_one(doc! { "id": &project_id }, doc! { "$set": set }).await.map_err(e)?;
    Ok(json!({ "ok": true, "written": written.len(), "fields": written }))
}

// ────────────────────────────────────────────────────────────────
// The day
// ────────────────────────────────────────────────────────────────

/// What this project actually contains, as counts a person would recognise.
///
/// Read from the songs rather than from a status field alone: a song can carry `status: "draft"` and
/// still have audio, because the status is set by whichever step last finished and a manual import
/// sets none at all. Counting the artefacts is what makes "four songs have audio and no images" true.
async fn shape_of(state: &AppState, project_id: &str) -> Res<Value> {
    use futures_util::StreamExt;
    let filter = if project_id.trim().is_empty() { doc! {} } else { doc! { "project_id": project_id } };
    let mut cursor = state.db.collection::<Document>("songs").find(filter).await.map_err(e)?;

    let (mut total, mut no_lyrics, mut no_audio, mut no_video, mut uploaded) =
        (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut song_ids: Vec<String> = Vec::new();
    while let Some(Ok(d)) = cursor.next().await {
        let s = bson_to_value(d);
        total += 1;
        if let Some(id) = s["id"].as_str() { song_ids.push(id.to_string()); }
        let has_lyrics = s["lyrics"].as_str().is_some_and(|l| !l.trim().is_empty());
        let has_audio = s["audio_url"].as_str().is_some_and(|u| !u.is_empty())
            || s["local_audio_path"].as_str().is_some_and(|u| !u.is_empty());
        let has_video = s["video_url"].as_str().is_some_and(|u| !u.is_empty())
            || s["local_video_path"].as_str().is_some_and(|u| !u.is_empty());
        if !has_lyrics { no_lyrics += 1; }
        if has_lyrics && !has_audio { no_audio += 1; }
        if has_audio && !has_video { no_video += 1; }
        if s["status"].as_str() == Some("uploaded") { uploaded += 1; }
    }

    // Sections carry the images, so "has audio but no sections" and "has sections but no images" are
    // different problems with different next steps.
    let mut with_sections = std::collections::HashSet::new();
    let mut with_images = std::collections::HashSet::new();
    let mut sec_cursor = state.db.collection::<Document>("sections").find(doc! {}).await.map_err(e)?;
    while let Some(Ok(d)) = sec_cursor.next().await {
        let sec = bson_to_value(d);
        let Some(sid) = sec["song_id"].as_str() else { continue };
        if !song_ids.iter().any(|x| x == sid) { continue; }
        with_sections.insert(sid.to_string());
        if sec["image_url"].as_str().is_some_and(|u| !u.is_empty()) { with_images.insert(sid.to_string()); }
    }
    let no_sections = song_ids.iter().filter(|id| !with_sections.contains(*id)).count() as u64;
    let no_images = with_sections.iter().filter(|id| !with_images.contains(*id)).count() as u64;

    Ok(json!({
        "songs": total, "no_lyrics": no_lyrics, "no_audio": no_audio,
        "no_sections": no_sections, "no_images": no_images, "no_video": no_video,
        "uploaded": uploaded,
    }))
}

/// The obvious next step, worked out without asking anybody.
///
/// This is the floor the AI improves on rather than replaces: it is always right about *what exists*,
/// costs nothing, and works offline — so a day is never blank because a free tier is spent.
pub fn plain_plan(shape: &Value, has_brief: bool) -> Vec<Value> {
    let n = |k: &str| shape[k].as_u64().unwrap_or(0);
    let mut out = Vec::new();
    if !has_brief {
        out.push(json!({ "label": "Say what this project is", "route": "/",
            "why": "Everything downstream reads the brief; without it every song starts from nothing." }));
    }
    if n("songs") == 0 {
        out.push(json!({ "label": "Write today's song", "route": "/composer",
            "why": "There is nothing in this project yet." }));
        return out;
    }
    if n("no_lyrics") > 0 {
        out.push(json!({ "label": "Finish the words", "route": "/composer",
            "why": format!("{} song(s) have no lyrics, and generation refuses to run without them.", n("no_lyrics")) }));
    }
    if n("no_audio") > 0 {
        out.push(json!({ "label": "Render the music", "route": "/music",
            "why": format!("{} song(s) have words and no audio.", n("no_audio")) }));
    }
    if n("no_sections") > 0 {
        out.push(json!({ "label": "Cut it into sections", "route": "/analysis",
            "why": format!("{} song(s) have no sections, which is what the images are hung on.", n("no_sections")) }));
    }
    if n("no_images") > 0 {
        out.push(json!({ "label": "Make the pictures", "route": "/images",
            "why": format!("{} song(s) have sections with no image yet.", n("no_images")) }));
    }
    if n("no_video") > 0 && n("no_images") == 0 {
        out.push(json!({ "label": "Build the video", "route": "/video",
            "why": format!("{} song(s) have everything they need.", n("no_video")) }));
    }
    if out.is_empty() {
        out.push(json!({ "label": "Publish what is finished", "route": "/upload",
            "why": "Nothing is waiting on you here." }));
        out.push(json!({ "label": "Start the next one", "route": "/composer",
            "why": "The pipeline is clear." }));
    }
    out.truncate(4);
    out
}

/// Today, for this project: a few steps, each with a reason drawn from what is actually here.
#[tauri::command]
pub async fn guide_today(state: State<'_, AppState>, project_id: String) -> Res<Value> {
    let project = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &project_id }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let shape = shape_of(state.inner(), &project_id).await?;
    let brief = project["brief"].as_object().map(|b| b.values()
        .any(|v| v.as_str().is_some_and(|s| !s.trim().is_empty()))).unwrap_or(false);
    let plain = plain_plan(&shape, brief);

    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();

    // The AI's job is the wording and the ordering, not the facts: it is handed the plain plan and
    // may rewrite it, and anything it invents a route for is dropped. So the worst it can do is
    // phrase the right steps badly.
    let allowed: Vec<&str> = plain.iter().filter_map(|s| s["route"].as_str()).collect();
    let system = "You are the studio's guide, telling one creator what today looks like. You receive \
         the project's brief, what the project actually contains, and a correct-but-plain plan.\n\n\
         Rewrite it as at most 4 steps in the voice of somebody who knows this project. Keep the \
         routes exactly as given — you may reorder or drop a step, never invent one. Each `why` is \
         one sentence about THIS project and its real numbers, not about the app.\n\n\
         Also give `greeting`: one sentence, at most 20 words, naming what today is for.\n\n\
         Return ONLY: {\"greeting\":\"…\",\"steps\":[{\"label\":\"…\",\"route\":\"…\",\"why\":\"…\"}]}";
    let user = format!(
        "PROJECT: {}\nTODAY'S TOPIC: {}\nBRIEF: {}\n\nWHAT IS ACTUALLY HERE:\n{}\n\nTHE PLAIN PLAN:\n{}",
        project["name"].as_str().unwrap_or("(unnamed)"),
        project["daily_topic"].as_str().unwrap_or("(none set)"),
        serde_json::to_string(&project["brief"]).unwrap_or_else(|_| "{}".into()),
        serde_json::to_string_pretty(&shape).unwrap_or_default(),
        serde_json::to_string_pretty(&plain).unwrap_or_default(),
    );

    let (greeting, steps, model) =
        match crate::commands::ai::provider_chat(&settings, system, &user, 0.5, true).await {
            Ok((content, model)) => {
                let parsed = crate::commands::ai::extract_json_value(&content).unwrap_or(Value::Null);
                let steps: Vec<Value> = parsed["steps"].as_array().map(|a| a.iter()
                    .filter(|s| s["route"].as_str().is_some_and(|r| allowed.contains(&r)))
                    .filter(|s| s["label"].as_str().is_some_and(|l| !l.trim().is_empty()))
                    .take(4).cloned().collect()).unwrap_or_default();
                let greeting = parsed["greeting"].as_str().unwrap_or("").trim().to_string();
                if steps.is_empty() { (greeting, plain.clone(), model) } else { (greeting, steps, model) }
            }
            // Silence from the model is not silence from the guide.
            Err(_) => (String::new(), plain.clone(), String::new()),
        };

    Ok(json!({
        "greeting": greeting,
        "steps": steps,
        "shape": shape,
        "has_brief": brief,
        "model": model,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(pairs: &[(&str, u64)]) -> Value {
        let mut m = serde_json::Map::new();
        for (k, v) in pairs { m.insert((*k).into(), json!(v)); }
        Value::Object(m)
    }

    #[test]
    fn a_project_with_nothing_in_it_is_told_to_write_something() {
        let p = plain_plan(&shape(&[("songs", 0)]), true);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0]["route"], "/composer");
    }

    #[test]
    fn the_brief_comes_first_when_it_is_empty_because_everything_reads_it() {
        let p = plain_plan(&shape(&[("songs", 0)]), false);
        assert_eq!(p[0]["route"], "/", "the brief is asked for before anything is made");
    }

    #[test]
    fn the_plan_names_the_earliest_unfinished_stage_first() {
        // Words missing and pictures missing at once: the words come first, because nothing
        // downstream of them can run.
        let p = plain_plan(&shape(&[("songs", 5), ("no_lyrics", 2), ("no_images", 3)]), true);
        assert_eq!(p[0]["route"], "/composer");
        assert!(p.iter().any(|s| s["route"] == "/images"));
    }

    #[test]
    fn a_reason_carries_the_real_number_rather_than_an_adjective() {
        let p = plain_plan(&shape(&[("songs", 9), ("no_audio", 4)]), true);
        let why = p.iter().find(|s| s["route"] == "/music").unwrap()["why"].as_str().unwrap().to_string();
        assert!(why.contains('4'), "{why}");
    }

    #[test]
    fn video_is_not_offered_while_pictures_are_still_missing() {
        // Building a video over half-rendered sections is how a run finishes green and empty.
        let p = plain_plan(&shape(&[("songs", 3), ("no_images", 2), ("no_video", 3)]), true);
        assert!(!p.iter().any(|s| s["route"] == "/video"));
    }

    #[test]
    fn a_finished_project_is_told_so_rather_than_given_busywork() {
        let p = plain_plan(&shape(&[("songs", 4), ("uploaded", 4)]), true);
        assert!(p.iter().any(|s| s["route"] == "/upload"));
        assert!(p.len() <= 4);
    }

    #[test]
    fn a_day_is_never_more_than_four_steps() {
        let p = plain_plan(&shape(&[
            ("songs", 20), ("no_lyrics", 3), ("no_audio", 4),
            ("no_sections", 5), ("no_images", 6), ("no_video", 7),
        ]), false);
        assert!(p.len() <= 4, "got {}", p.len());
    }

    // ── the interview ───────────────────────────────────────────────────────

    #[test]
    fn only_the_briefs_own_fields_can_be_written_into() {
        for (k, _) in BRIEF_FIELDS { assert!(is_brief_field(k)); }
        for bad in ["", "status", "brief", "id", "project_id", "../etc"] {
            assert!(!is_brief_field(bad), "{bad} is not a brief field");
        }
    }

    #[test]
    fn the_opening_questions_move_on_as_they_are_answered() {
        let first = fallback_question(&[]).unwrap();
        assert_eq!(first["field"], "motivation");
        let answered = vec!["motivation".to_string()];
        assert_eq!(fallback_question(&answered).unwrap()["field"], "audience");
    }

    #[test]
    fn the_fallback_runs_out_rather_than_repeating_itself() {
        let all: Vec<String> = BRIEF_FIELDS.iter().map(|(k, _)| (*k).to_string()).collect();
        assert!(fallback_question(&all).is_none());
    }

    #[test]
    fn every_fallback_question_offers_a_way_in() {
        let mut answered: Vec<String> = Vec::new();
        while let Some(q) = fallback_question(&answered) {
            let opts = q["options"].as_array().unwrap();
            assert!(opts.len() >= 2, "a question with one option is not a question");
            assert!(q["question"].as_str().unwrap().ends_with('?'));
            answered.push(q["field"].as_str().unwrap().to_string());
        }
    }
}
