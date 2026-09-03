//! Avatar universes: one story, told for a person who is not the person it was written for.
//!
//! An edition written from a song is written *for somebody*, whether or not anybody said so. The
//! default somebody is whoever the model imagines when nothing is specified, which in practice is a
//! reader who shares the writer's language, region and assumptions. Making a second edition for a
//! different reader means writing it again from nothing.
//!
//! A **universe** is that reader made explicit: an avatar (who they are) plus the givens their world
//! supplies (language, region, cultural background, era, circumstances, upbringing, faith background,
//! means, family shape). Once it is written down, three things become cheap that were expensive:
//!
//!   * **Deriving siblings.** From one universe, vary the axes that matter and get a set of
//!     neighbouring ones — the same person's story, in another language, another region, another
//!     upbringing. The variation is on named axes rather than "make it different", so what changed
//!     between two universes is a fact you can read rather than a diff you have to infer.
//!   * **Retelling.** An edition is rewritten *through* a universe: translated into its language,
//!     re-grounded in its region and circumstances, its images re-prompted. Not a translation pass
//!     over finished prose — the retelling is written, so a metaphor that only works in one place is
//!     replaced rather than rendered literally into nonsense.
//!   * **Depth on demand.** A universe can be three answers or twelve. The interview asks as many
//!     questions as the chosen depth wants and no more, so a sketch costs a minute and a deep one is
//!     available to anybody who wants it without being imposed on everybody who does not.
//!
//! ## The thing this must not become
//!
//! An axis vocabulary plus a generative model is a caricature machine if it is pointed carelessly.
//! "Write this for a Nigerian reader" invites the model to write the idea of a Nigerian reader. Two
//! defences are built in rather than hoped for:
//!
//!   * The prompt names the avatar as **one specific person** and instructs the writer to write for
//!     them, not for a demographic — and says so explicitly, because the failure mode is the model
//!     helpfully generalising.
//!   * Every derived universe is a **draft the user edits**. Nothing here writes a universe into a
//!     project without a person looking at it, and the offline fallback marks itself as a starting
//!     point in as many words.
//!
//! ## Working without an AI
//!
//! Every command here degrades: the interview falls back to a fixed question set, derivation falls
//! back to a deterministic rotation over neutral, factual axis values, and only the retelling —
//! which is writing, and cannot be faked — requires a provider.

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
// The vocabulary
// ────────────────────────────────────────────────────────────────

/// One coordinate of a universe.
pub struct Axis {
    pub id: &'static str,
    pub label: &'static str,
    /// What the picker says under the label, for somebody filling this in.
    pub hint: &'static str,
    /// The question the interview asks when it has no AI to phrase one.
    pub question: &'static str,
    /// Concrete openings, offered as a way in. Never a closed list — every answer is free text.
    pub examples: &'static [&'static str],
    /// How this given bears on the writing. Handed to the model verbatim, so it has to say
    /// something a writer could act on rather than restate the axis name.
    pub direction: &'static str,
}

/// Ordered by consequence: the first three change a telling more than the last three, and depth
/// takes them from the front. Language leads because it is the only axis that changes every
/// sentence, and region follows because it decides what the concrete nouns can be.
pub const AXES: &[Axis] = &[
    Axis {
        id: "language",
        label: "Language",
        hint: "The language it is written in — not translated into afterwards.",
        question: "What language does this telling live in?",
        examples: &["English", "Spanish", "Yoruba", "Tagalog", "Arabic", "Portuguese"],
        direction: "Write in this language natively. Do not write in English and translate: choose \
                    the idiom, the rhythm and the register a writer working in this language would \
                    choose. Where the source text has a fixed traditional rendering in this \
                    language, honour it rather than inventing a fresh one.",
    },
    Axis {
        id: "region",
        label: "Region",
        hint: "Where they are. Decides what the concrete details can be.",
        question: "Where do they live?",
        examples: &["A coastal city in West Africa", "A farming valley in the Andes",
                    "A northern European port town", "A market town in south India",
                    "A suburb of a large North American city"],
        direction: "Ground every concrete detail here — the weather, the food, the plants, the sound \
                    outside a window, what a road looks like. Replace any image from the source that \
                    would not be recognised here with one that carries the same weight and is \
                    ordinary in this place. Never explain the place to an outsider; write as though \
                    the reader lives in it.",
    },
    Axis {
        id: "culture",
        label: "Cultural background",
        hint: "What goes without saying for them.",
        question: "What is taken for granted in the world they grew up in?",
        examples: &["A large extended family, everyone in earshot",
                    "A minority community keeping a language alive",
                    "A mixed household with two traditions in it",
                    "A secular upbringing, faith met later"],
        direction: "Treat this as the set of things that go without saying, and therefore are not \
                    said. Let it show in what the writing assumes rather than in what it describes. \
                    Do not exoticise and do not use it as decoration: it is the water, not the fish.",
    },
    Axis {
        id: "circumstance",
        label: "Circumstances",
        hint: "What their life is actually like right now.",
        question: "What is going on in their life at the moment?",
        examples: &["Working two jobs, tired most days", "New in a country, still finding footing",
                    "Caring for a parent who is ill", "Settled, comfortable, restless",
                    "A student, everything ahead"],
        direction: "This is what the reader brings to the page today. Let it decide which parts of \
                    the source land hardest and give those the most room. Do not narrate their \
                    circumstances back at them and do not offer consolation they did not ask for.",
    },
    Axis {
        id: "upbringing",
        label: "Upbringing",
        hint: "How they were raised, and by whom.",
        question: "How were they brought up?",
        examples: &["Raised by grandparents", "One parent, moved often",
                    "A strict religious household", "A house full of books and argument",
                    "Between two countries"],
        direction: "Decides the voice they trust. Someone raised on being told stories reads a story; \
                    someone raised on being taught reads an argument. Match the mode of address to \
                    how they learnt that a serious thing gets said.",
    },
    Axis {
        id: "era",
        label: "When",
        hint: "Now, or another time.",
        question: "When is this set?",
        examples: &["Now", "The 1970s", "A generation ago", "The time of the text itself",
                    "Deliberately timeless"],
        direction: "Fixes what may appear. Nothing from after this time exists in the writing — no \
                    object, no phrase, no idea. If the era is the source text's own, use its \
                    material world and not a modern one dressed up.",
    },
    Axis {
        id: "faith",
        label: "Faith background",
        hint: "What they already know of the text, if anything.",
        question: "How much of this text do they already know?",
        examples: &["Knows it by heart", "Grew up around it, drifted",
                    "Never opened it", "Another tradition entirely", "Actively sceptical"],
        direction: "Decides what may be assumed and what must be earned. A reader who knows the text \
                    is bored by explanation; a reader who does not is lost without it. Pitch the \
                    amount of scaffolding to exactly this, and never signal that you are doing so.",
    },
    Axis {
        id: "means",
        label: "Means",
        hint: "The economic weather they live in.",
        question: "What are their circumstances, materially?",
        examples: &["Not much, and it is tight", "Enough, carefully", "Comfortable",
                    "Went from one to the other"],
        direction: "Governs what the concrete images can be without being tone-deaf. Do not make \
                    poverty picturesque and do not make comfort invisible. Where the source speaks of \
                    plenty or want, let it mean here what it would mean to them.",
    },
    Axis {
        id: "family",
        label: "Family shape",
        hint: "Who is around them.",
        question: "Who is around them day to day?",
        examples: &["A full house", "Alone, by choice", "Alone, not by choice",
                    "A couple", "Small children", "Grown children far away"],
        direction: "Decides who the writing's second person could be, and which relationships in the \
                    source text will be felt rather than read. Use it to choose the analogy, not to \
                    add characters.",
    },
];

pub fn axis(id: &str) -> Option<&'static Axis> {
    AXES.iter().find(|a| a.id == id)
}

/// The avatar itself — who this is, as opposed to the world they are in.
///
/// Kept separate from the axes because these do not vary when siblings are derived: a sibling
/// universe is the *same story for a different person*, and the person gets a new name and a new
/// description precisely because the axes moved.
pub const AVATAR_FIELDS: &[(&str, &str)] = &[
    ("name", "what they are called"),
    ("who", "who they are — age, work, where they sit in the world"),
    ("appearance", "what a reader would see, if they were drawn"),
];

pub fn is_avatar_field(name: &str) -> bool {
    AVATAR_FIELDS.iter().any(|(k, _)| *k == name)
}

/// A field the interview may write into: the avatar, or one of the axes.
pub fn is_universe_field(name: &str) -> bool {
    is_avatar_field(name) || axis(name).is_some()
}

// ────────────────────────────────────────────────────────────────
// Depth
// ────────────────────────────────────────────────────────────────

pub struct Depth {
    pub id: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    /// How many axes, taken from the front of `AXES`.
    pub axes: usize,
    /// How many questions the interview may ask in total, avatar included.
    pub questions: usize,
    /// What the writer is told about how much of this world to hold in view.
    pub direction: &'static str,
}

/// Three depths, because two is a false choice and five is a menu nobody reads.
///
/// The counts are not arbitrary: a sketch that asked for six axes would not be a sketch, and a deep
/// universe that stopped at six would leave out exactly the axes — means, family — that make a
/// telling feel like it is about a person rather than about a place.
pub const DEPTHS: &[Depth] = &[
    Depth {
        id: "sketch", label: "A sketch", axes: 3, questions: 5,
        hint: "Language, place, background. A minute, and enough to change what gets written.",
        direction: "You have only the broadest strokes of this reader. Use them, and do not invent \
                    detail you were not given — an unfounded specific reads as a mistake to somebody \
                    who lives there.",
    },
    Depth {
        id: "grounded", label: "Grounded", axes: 6, questions: 9,
        hint: "Enough that the writing has somewhere particular to stand.",
        direction: "You have enough to write from a particular standpoint. Let the givens decide the \
                    concrete choices — which image, which example, how much is explained — without \
                    ever naming them on the page.",
    },
    Depth {
        id: "deep", label: "Deep", axes: 9, questions: 14,
        hint: "Every axis. For a telling that has to feel like it was written for one person.",
        direction: "You know this reader well enough to write for them and nobody else. Every choice \
                    should be one you would not have made for a different reader. Resist summarising \
                    them back to themselves.",
    },
];

pub fn depth(id: &str) -> &'static Depth {
    DEPTHS.iter().find(|d| d.id == id).unwrap_or(&DEPTHS[1])
}

/// The axes a given depth asks about, in order of consequence.
pub fn axes_for_depth(depth_id: &str) -> Vec<&'static Axis> {
    AXES.iter().take(depth(depth_id).axes).collect()
}

#[tauri::command]
pub async fn universe_axes() -> Res<Value> {
    Ok(json!({
        "axes": AXES.iter().map(|a| json!({
            "id": a.id, "label": a.label, "hint": a.hint,
            "question": a.question,
            "examples": a.examples,
        })).collect::<Vec<_>>(),
        "depths": DEPTHS.iter().map(|d| json!({
            "id": d.id, "label": d.label, "hint": d.hint,
            "axes": d.axes, "questions": d.questions,
        })).collect::<Vec<_>>(),
        "avatar_fields": AVATAR_FIELDS.iter().map(|(k, d)| json!({ "id": k, "hint": d }))
            .collect::<Vec<_>>(),
    }))
}

// ────────────────────────────────────────────────────────────────
// The prompt block
// ────────────────────────────────────────────────────────────────

fn field_str<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("").trim()
}

/// The universe rendered as instructions to whoever is writing.
///
/// Pure, and the single place the vocabulary turns into words a model reads — so composing a lyric,
/// authoring an edition and retelling one all describe the same universe the same way. An axis the
/// user left blank contributes nothing at all rather than an empty heading, because a heading with
/// nothing under it reads to a model as a thing it should fill in.
pub fn universe_prompt_block(universe: &Value) -> String {
    if !universe.is_object() { return String::new(); }
    let axes_val = universe.get("axes").cloned().unwrap_or(Value::Null);
    let avatar = universe.get("avatar").cloned().unwrap_or(Value::Null);

    let mut lines: Vec<String> = Vec::new();

    let name = field_str(&avatar, "name");
    let who = field_str(&avatar, "who");
    let appearance = field_str(&avatar, "appearance");
    if !name.is_empty() || !who.is_empty() {
        lines.push("THE READER — one specific person, not a category:".into());
        if !name.is_empty() { lines.push(format!("  Name: {name}")); }
        if !who.is_empty() { lines.push(format!("  Who: {who}")); }
        if !appearance.is_empty() { lines.push(format!("  Seen: {appearance}")); }
    }

    let mut given: Vec<String> = Vec::new();
    for a in AXES {
        let v = field_str(&axes_val, a.id);
        if v.is_empty() { continue; }
        given.push(format!("  {} — {}\n    {}", a.label, v, a.direction));
    }
    if !given.is_empty() {
        if !lines.is_empty() { lines.push(String::new()); }
        lines.push("THEIR WORLD — givens this telling holds to:".into());
        lines.extend(given);
    }

    if lines.is_empty() { return String::new(); }

    let d = depth(universe.get("depth").and_then(|v| v.as_str()).unwrap_or("grounded"));
    lines.push(String::new());
    lines.push(format!("HOW MUCH YOU KNOW: {}", d.direction));
    // The guard rail. Stated last because it is the instruction most likely to be needed and models
    // weight the end of a block; stated at all because the failure mode here is not a model refusing
    // to write for this reader but a model cheerfully writing for an idea of them.
    lines.push(
        "WRITE FOR THIS PERSON, NOT ABOUT THEIR GROUP. The givens above are where they happen to \
         stand, not what they are like. Never generalise from them, never explain their own world \
         back to them, never let a given become a subject. If a choice would read as a description \
         of a people rather than as a sentence written for one reader, make the other choice."
            .into());
    lines.push(String::new());
    format!("{}\n", lines.join("\n"))
}

// ────────────────────────────────────────────────────────────────
// The interview
// ────────────────────────────────────────────────────────────────

/// The fixed question set, used when there is no AI to phrase one.
///
/// Name first, because everything after it is easier to answer about somebody who has one, and
/// because an interview that opens with "what cultural background" is an intake form.
pub fn fallback_universe_question(answered: &[String], depth_id: &str) -> Option<Value> {
    if !answered.iter().any(|a| a == "name") {
        return Some(json!({
            "field": "name",
            "question": "Who is this telling for? Give them a name.",
            "why": "Everything after this is easier to answer about a person than about a reader.",
            "options": [],
            "source": "fallback",
        }));
    }
    if !answered.iter().any(|a| a == "who") {
        return Some(json!({
            "field": "who",
            "question": "Who are they? A sentence is plenty.",
            "why": "",
            "options": [],
            "source": "fallback",
        }));
    }
    axes_for_depth(depth_id).into_iter()
        .find(|a| !answered.iter().any(|x| x == a.id))
        .map(|a| json!({
            "field": a.id,
            "question": a.question,
            "why": a.hint,
            "options": a.examples.iter().map(|x| json!({ "label": x })).collect::<Vec<_>>(),
            "source": "fallback",
        }))
}

#[derive(serde::Deserialize)]
pub struct UniverseInterviewRequest {
    #[serde(default)]
    pub project_id: String,
    /// `field` → what the user said. Avatar fields and axis ids both land here.
    #[serde(default)]
    pub answers: Value,
    #[serde(default)]
    pub depth: Option<String>,
    #[serde(default)]
    pub finish: bool,
}

/// The next question worth asking about this avatar, or `done`.
///
/// Cascades the same way the project interview does — the model sees everything said so far — but
/// over a different vocabulary, and bounded by the chosen depth rather than by one fixed cap. A
/// sketch and a deep universe are the same conversation stopped at different points, which is why
/// deepening one later is just answering more questions rather than starting again.
#[tauri::command]
pub async fn universe_interview_next(
    state: State<'_, AppState>,
    payload: UniverseInterviewRequest,
) -> Res<Value> {
    let depth_id = payload.depth.clone().unwrap_or_else(|| "grounded".into());
    let d = depth(&depth_id);
    let answered: Vec<String> = payload.answers.as_object()
        .map(|o| o.keys().filter(|k| is_universe_field(k)).cloned().collect())
        .unwrap_or_default();

    if payload.finish || answered.len() >= d.questions {
        return Ok(json!({ "done": true, "answered": answered.len() }));
    }

    // What is still open at this depth. An axis beyond the depth is not asked about even if the
    // model would like to, because the depth is the user's answer to "how long is this going to be".
    let mut remaining: Vec<(&str, &str)> = AVATAR_FIELDS.iter()
        .map(|(k, h)| (*k, *h))
        .filter(|(k, _)| !answered.iter().any(|a| a == k))
        .collect();
    remaining.extend(axes_for_depth(&depth_id).into_iter()
        .filter(|a| !answered.iter().any(|x| x == a.id))
        .map(|a| (a.id, a.hint)));
    if remaining.is_empty() {
        return Ok(json!({ "done": true, "answered": answered.len() }));
    }

    let fallback = || match fallback_universe_question(&answered, &depth_id) {
        Some(q) => json!({ "done": false, "question": q, "answered": answered.len(),
                           "total": d.questions }),
        None => json!({ "done": true, "answered": answered.len() }),
    };

    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let project = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &payload.project_id }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();

    let system = "You are helping somebody describe one reader — a specific person a story is going \
         to be retold for — and the world that person lives in. Ask ONE question: the single most \
         useful thing still unknown.\n\n\
         Cascade. Read what they have already said and go where it leads; do not walk a checklist. \
         If an answer implies the next question, ask that one.\n\n\
         The question must be about this person, answerable in a breath, in plain words. Offer 2 to \
         4 options that are real, specific and ordinary — never a list of nationalities, never a \
         list of adjectives, never anything that reads as a demographic checkbox. They can always \
         answer in their own words, so the options are a way in rather than a cage.\n\n\
         Return ONLY: {\"field\":\"<one of the allowed fields>\",\"question\":\"…\",\
         \"why\":\"at most 12 words\",\"options\":[{\"label\":\"…\"},…]}";

    let user = format!(
        "PROJECT: {}\nDEPTH: {} ({} questions in total)\n\n\
         ALREADY ANSWERED (do not ask these again):\n{}\n\n\
         FIELDS STILL OPEN (pick exactly one):\n{}\n\n\
         This is question {} of at most {}.",
        project["name"].as_str().unwrap_or("(unnamed)"),
        d.label, d.questions,
        if answered.is_empty() { "(nothing yet — this is the first question)".to_string() }
        else { serde_json::to_string_pretty(&payload.answers).unwrap_or_default() },
        remaining.iter().map(|(k, h)| format!("- {k}: {h}")).collect::<Vec<_>>().join("\n"),
        answered.len() + 1, d.questions,
    );

    let Ok((content, model)) =
        crate::commands::ai::provider_chat(&settings, system, &user, 0.7, true).await
    else { return Ok(fallback()); };
    let parsed = crate::commands::ai::extract_json_value(&content).unwrap_or(Value::Null);

    let field = parsed["field"].as_str().unwrap_or("");
    let question = parsed["question"].as_str().unwrap_or("").trim();
    let in_scope = remaining.iter().any(|(k, _)| *k == field);
    if !in_scope || question.is_empty() {
        return Ok(fallback());
    }

    let options: Vec<Value> = parsed["options"].as_array().map(|a| a.iter()
        .filter_map(|o| o["label"].as_str().map(|l| json!({ "label": l.trim() })))
        .filter(|o| !o["label"].as_str().unwrap_or("").is_empty())
        .take(4).collect()).unwrap_or_default();

    Ok(json!({
        "done": false,
        "answered": answered.len(),
        "total": d.questions,
        "question": {
            "field": field,
            "question": question,
            "why": parsed["why"].as_str().unwrap_or("").trim(),
            // An AI question with no options is fine; an axis question with none is a blank box
            // where the app knows five good answers, so the axis lends its own.
            "options": if options.is_empty() {
                axis(field).map(|a| a.examples.iter().map(|x| json!({ "label": x }))
                    .collect::<Vec<_>>()).unwrap_or_default()
            } else { options },
            "source": model,
        },
    }))
}

// ────────────────────────────────────────────────────────────────
// Storage
// ────────────────────────────────────────────────────────────────

/// Split a flat answer map into the avatar and its axes.
///
/// Pure, and the reason the interview can hand back one flat object: the shape a conversation
/// produces and the shape a record wants are not the same, and reconciling them at the boundary
/// keeps both honest.
pub fn universe_from_answers(answers: &Value) -> (Value, Value) {
    let mut avatar = serde_json::Map::new();
    let mut axes = serde_json::Map::new();
    if let Some(obj) = answers.as_object() {
        for (k, v) in obj {
            let text = v.as_str().unwrap_or("").trim().to_string();
            if text.is_empty() { continue; }
            if is_avatar_field(k) { avatar.insert(k.clone(), json!(text)); }
            else if axis(k).is_some() { axes.insert(k.clone(), json!(text)); }
        }
    }
    (Value::Object(avatar), Value::Object(axes))
}

/// A name for a universe that has not been given one: the person and where they are.
pub fn universe_title(avatar: &Value, axes: &Value) -> String {
    let name = field_str(avatar, "name");
    let region = field_str(axes, "region");
    let language = field_str(axes, "language");
    match (name.is_empty(), region.is_empty(), language.is_empty()) {
        (false, false, _) => format!("{name} — {region}"),
        (false, true, false) => format!("{name} — {language}"),
        (false, true, true) => name.to_string(),
        (true, false, _) => region.to_string(),
        (true, true, false) => language.to_string(),
        (true, true, true) => "Untitled universe".into(),
    }
}

#[derive(serde::Deserialize)]
pub struct UniverseSaveRequest {
    #[serde(default)]
    pub id: Option<String>,
    pub project_id: String,
    #[serde(default)]
    pub name: Option<String>,
    /// The flat interview map, or `avatar`/`axes` already split — both are accepted.
    #[serde(default)]
    pub answers: Value,
    #[serde(default)]
    pub avatar: Value,
    #[serde(default)]
    pub axes: Value,
    #[serde(default)]
    pub depth: Option<String>,
    #[serde(default)]
    pub derived_from: Option<String>,
    #[serde(default)]
    pub varied: Vec<String>,
}

#[tauri::command]
pub async fn universe_save(state: State<'_, AppState>, payload: UniverseSaveRequest) -> Res<Value> {
    let (from_answers_avatar, from_answers_axes) = universe_from_answers(&payload.answers);
    let avatar = if payload.avatar.is_object() && !payload.avatar.as_object().unwrap().is_empty() {
        payload.avatar.clone()
    } else { from_answers_avatar };
    let axes = if payload.axes.is_object() && !payload.axes.as_object().unwrap().is_empty() {
        payload.axes.clone()
    } else { from_answers_axes };

    let depth_id = payload.depth.clone().unwrap_or_else(|| "grounded".into());
    let name = payload.name.clone()
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| universe_title(&avatar, &axes));

    let col = state.db.collection::<Document>("universes");
    if let Some(id) = payload.id.clone().filter(|s| !s.is_empty()) {
        let set = doc! {
            "name": &name,
            "avatar": bson::to_bson(&avatar).map_err(e)?,
            "axes": bson::to_bson(&axes).map_err(e)?,
            "depth": depth(&depth_id).id,
            "updated_at": crate::models::now_iso(),
        };
        col.update_one(doc! { "id": &id }, doc! { "$set": set }).await.map_err(e)?;
        let saved = col.find_one(doc! { "id": &id }).await.map_err(e)?.map(bson_to_value);
        return Ok(saved.unwrap_or_else(|| json!({ "id": id })));
    }

    let universe = json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "project_id": payload.project_id,
        "name": name,
        "avatar": avatar,
        "axes": axes,
        "depth": depth(&depth_id).id,
        "derived_from": payload.derived_from,
        "varied": payload.varied,
        "created_at": crate::models::now_iso(),
    });
    let d = bson::to_document(&universe).map_err(e)?;
    col.insert_one(d).await.map_err(e)?;
    Ok(universe)
}

#[tauri::command]
pub async fn universe_list(state: State<'_, AppState>, project_id: Option<String>) -> Res<Value> {
    use futures_util::StreamExt;
    let mut filter = Document::new();
    if let Some(p) = project_id.filter(|s| !s.is_empty()) { filter.insert("project_id", p); }
    let mut out = Vec::new();
    let mut cursor = state.db.collection::<Document>("universes").find(filter).await.map_err(e)?;
    while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }
    Ok(json!({ "universes": out }))
}

#[tauri::command]
pub async fn universe_delete(state: State<'_, AppState>, id: String) -> Res<Value> {
    state.db.collection::<Document>("universes")
        .delete_one(doc! { "id": &id }).await.map_err(e)?;
    Ok(json!({ "deleted": id }))
}

// ────────────────────────────────────────────────────────────────
// Deriving siblings
// ────────────────────────────────────────────────────────────────

/// Neutral, factual alternatives per axis, for deriving without a model.
///
/// Descriptions of circumstance rather than of people: "a farming valley in the Andes" is a place,
/// where "a traditional Andean family" would be a claim about who lives there. The offline path is
/// a rotation over these, and every universe it makes is labelled a starting point — the point is
/// to give somebody something to edit, not to pretend a table knows anybody's life.
const CONTRASTS: &[(&str, &[&str])] = &[
    ("language", &["Spanish", "Portuguese", "French", "Arabic", "Swahili", "Tagalog",
                   "Hindi", "Mandarin", "German", "Korean"]),
    ("region", &["A coastal city in West Africa", "A farming valley in the Andes",
                 "A northern European port town", "A market town in south India",
                 "A river delta in southeast Asia", "A high desert town",
                 "A suburb of a large North American city", "An island in the Caribbean"]),
    ("culture", &["A large extended family, everyone in earshot",
                  "A minority community keeping a language alive",
                  "A mixed household with two traditions in it",
                  "A secular upbringing, faith met later",
                  "A diaspora community two generations in"]),
    ("circumstance", &["Working two jobs, tired most days", "New in a country, still finding footing",
                       "Caring for a parent who is ill", "Settled, comfortable, restless",
                       "A student, everything ahead", "Recently retired, more time than expected"]),
    ("upbringing", &["Raised by grandparents", "One parent, moved often",
                     "A strict religious household", "A house full of books and argument",
                     "Between two countries", "A quiet only child"]),
    ("era", &["Now", "The 1970s", "A generation ago", "The time of the text itself"]),
    ("faith", &["Knows it by heart", "Grew up around it, drifted", "Never opened it",
                "Another tradition entirely", "Actively sceptical"]),
    ("means", &["Not much, and it is tight", "Enough, carefully", "Comfortable",
                "Went from one to the other"]),
    ("family", &["A full house", "Alone, by choice", "A couple", "Small children",
                 "Grown children far away"]),
];

fn contrasts(axis_id: &str) -> &'static [&'static str] {
    CONTRASTS.iter().find(|(k, _)| *k == axis_id).map(|(_, v)| *v).unwrap_or(&[])
}

/// Sibling universes without a model: rotate each varied axis through values it does not already
/// hold, and leave everything else exactly as it was.
///
/// Deterministic on purpose — the same base and the same axes give the same neighbours, so a person
/// who derives four, deletes two and derives again does not get a reshuffled set they have to
/// re-read. The avatar's name is deliberately *not* invented here: naming a person from a table of
/// regions is precisely the caricature this module is built to avoid, so the offline path leaves the
/// name blank and says why.
pub fn derive_offline(base: &Value, vary: &[String], count: usize) -> Vec<Value> {
    let base_axes = base.get("axes").cloned().unwrap_or(json!({}));
    let vary: Vec<&str> = vary.iter().map(|s| s.as_str())
        .filter(|s| axis(s).is_some() && !contrasts(s).is_empty())
        .collect();
    if vary.is_empty() { return Vec::new(); }

    (0..count.min(8)).map(|i| {
        let mut axes = base_axes.as_object().cloned().unwrap_or_default();
        for (n, id) in vary.iter().enumerate() {
            let pool: Vec<&&str> = contrasts(id).iter()
                .filter(|v| !field_str(&base_axes, id).eq_ignore_ascii_case(v))
                .collect();
            if pool.is_empty() { continue; }
            // Offset per axis so two varied axes do not move in lockstep and produce four universes
            // that differ along one diagonal.
            axes.insert((*id).to_string(), json!(*pool[(i + n * 3) % pool.len()]));
        }
        let axes = Value::Object(axes);
        json!({
            "name": universe_title(&json!({}), &axes),
            "avatar": { "name": "", "who": "", "appearance": "" },
            "axes": axes,
            "varied": vary,
            "source": "offline",
            "note": "A starting point, not a person — give them a name and say who they are.",
        })
    }).collect()
}

#[derive(serde::Deserialize)]
pub struct DeriveRequest {
    pub id: String,
    /// Axis ids to move. Everything else is held constant.
    #[serde(default)]
    pub vary: Vec<String>,
    #[serde(default)]
    pub count: Option<usize>,
    /// Write them straight into the project rather than returning them for review.
    #[serde(default)]
    pub save: bool,
}

/// Neighbouring universes: the same story's reader, moved along named axes.
#[tauri::command]
pub async fn universe_derive(state: State<'_, AppState>, payload: DeriveRequest) -> Res<Value> {
    let base = state.db.collection::<Document>("universes")
        .find_one(doc! { "id": &payload.id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "that universe is gone".to_string())?;

    let vary: Vec<String> = payload.vary.iter()
        .filter(|v| axis(v).is_some())
        .cloned().collect();
    if vary.is_empty() {
        return Err("Pick at least one thing to vary — otherwise every sibling is the original.".into());
    }
    let count = payload.count.unwrap_or(3).clamp(1, 8);

    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();

    let held: Vec<String> = AXES.iter()
        .filter(|a| !vary.iter().any(|v| v == a.id))
        .filter_map(|a| {
            let v = field_str(base.get("axes").unwrap_or(&Value::Null), a.id);
            (!v.is_empty()).then(|| format!("- {}: {}", a.label, v))
        }).collect();
    let moving: Vec<String> = vary.iter().filter_map(|id| axis(id))
        .map(|a| format!("- {}: currently {}",
            a.label,
            {
                let v = field_str(base.get("axes").unwrap_or(&Value::Null), a.id);
                if v.is_empty() { "(unset)".to_string() } else { v.to_string() }
            }))
        .collect();

    let system = "You invent neighbouring lives: given one specific reader, produce other specific \
         readers who differ along named axes and are otherwise the same.\n\n\
         Rules that matter more than variety:\n\
         - Each one is ONE PERSON, with a name that belongs to their language and place, and a \
           sentence about who they are that could only be about them.\n\
         - Never write a representative of a group. No sentence you produce should be true of \
           everybody who shares an axis value. If a description would fit a million people equally, \
           it is wrong.\n\
         - Move ONLY the axes named as varying. Every other axis keeps the base value verbatim.\n\
         - Ordinary lives, not remarkable ones. The point is a different reader, not a better story.\n\n\
         Return ONLY: {\"universes\":[{\"name\":\"…\",\"avatar\":{\"name\":\"…\",\"who\":\"…\",\
         \"appearance\":\"…\"},\"axes\":{\"<axis id>\":\"…\"}}]}";

    let user = format!(
        "THE BASE READER:\n{base_block}\n\
         HOLD THESE EXACTLY AS THEY ARE:\n{held}\n\n\
         MOVE THESE:\n{moving}\n\n\
         Produce {count} neighbouring readers. Return every axis in `axes`, the held ones copied \
         verbatim and the moved ones changed.",
        base_block = universe_prompt_block(&base),
        held = if held.is_empty() { "(nothing else is set)".to_string() } else { held.join("\n") },
        moving = moving.join("\n"),
        count = count,
    );

    let (derived, source) =
        match crate::commands::ai::provider_chat(&settings, system, &user, 0.9, true).await {
            Ok((content, model)) => {
                let parsed = crate::commands::ai::extract_json_value(&content).unwrap_or(Value::Null);
                let list = parsed["universes"].as_array().cloned()
                    .or_else(|| parsed.as_array().cloned())
                    .unwrap_or_default();
                let cleaned = clean_derived(&base, &list, &vary, count);
                if cleaned.is_empty() { (derive_offline(&base, &vary, count), "offline".to_string()) }
                else { (cleaned, model) }
            }
            // No provider, or a spent budget. The table is a worse answer than a model and a much
            // better one than an error, because what it produces is editable.
            Err(_) => (derive_offline(&base, &vary, count), "offline".to_string()),
        };

    if derived.is_empty() {
        return Err("Nothing came back that differed from the original.".into());
    }

    if !payload.save {
        return Ok(json!({ "universes": derived, "source": source, "saved": false }));
    }

    let mut saved = Vec::new();
    for u in &derived {
        let record = json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "project_id": base["project_id"].as_str().unwrap_or(""),
            "name": u["name"].as_str().unwrap_or("Untitled universe"),
            "avatar": u["avatar"].clone(),
            "axes": u["axes"].clone(),
            "depth": base["depth"].as_str().unwrap_or("grounded"),
            "derived_from": base["id"].as_str().unwrap_or(""),
            "varied": vary.clone(),
            "created_at": crate::models::now_iso(),
        });
        let d = bson::to_document(&record).map_err(e)?;
        state.db.collection::<Document>("universes").insert_one(d).await.map_err(e)?;
        saved.push(record);
    }
    Ok(json!({ "universes": saved, "source": source, "saved": true }))
}

/// Keep the model honest about what it was allowed to change.
///
/// A model asked to move two axes will quietly move a third, and the result is a set of universes
/// whose differences nobody can account for — which defeats the whole point of naming axes. Held
/// axes are restored from the base rather than the universe being rejected, since the writing is
/// usually fine and only the bookkeeping drifted.
pub fn clean_derived(base: &Value, list: &[Value], vary: &[String], count: usize) -> Vec<Value> {
    let base_axes = base.get("axes").cloned().unwrap_or(json!({}));
    let mut out: Vec<Value> = Vec::new();
    for item in list.iter().take(count) {
        let mut axes = serde_json::Map::new();
        for a in AXES {
            let held = !vary.iter().any(|v| v == a.id);
            let from_base = field_str(&base_axes, a.id);
            let proposed = field_str(item.get("axes").unwrap_or(&Value::Null), a.id);
            let value = if held { from_base } else if proposed.is_empty() { from_base } else { proposed };
            if !value.is_empty() { axes.insert(a.id.to_string(), json!(value)); }
        }
        let axes = Value::Object(axes);

        // A sibling identical to the base on every varied axis is not a sibling.
        let moved = vary.iter().any(|id|
            !field_str(&axes, id).eq_ignore_ascii_case(field_str(&base_axes, id)));
        if !moved { continue; }

        let avatar = item.get("avatar").cloned().unwrap_or(json!({}));
        let name = item["name"].as_str().unwrap_or("").trim();
        out.push(json!({
            "name": if name.is_empty() { universe_title(&avatar, &axes) } else { name.to_string() },
            "avatar": {
                "name": field_str(&avatar, "name"),
                "who": field_str(&avatar, "who"),
                "appearance": field_str(&avatar, "appearance"),
            },
            "axes": axes,
            "varied": vary,
        }));
    }
    out
}

// ────────────────────────────────────────────────────────────────
// Retelling
// ────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct RetellRequest {
    /// The edition to retell.
    pub edition_id: String,
    /// The universe to retell it through.
    pub universe_id: String,
    /// Keep the original page count and page breaks. On by default: an edition whose art is already
    /// generated has one image per page, and a retelling that re-paginates orphans all of it.
    #[serde(default = "yes")]
    pub keep_pagination: bool,
}
fn yes() -> bool { true }

/// Rewrite an edition through a universe: written, not translated.
///
/// The distinction is the whole feature. A translation of an edition keeps every image and every
/// assumption and renders them into another language, where half of them stop meaning anything. A
/// retelling keeps the *beat* of each page — what happens, what turns — and finds the words, images
/// and amount of explanation that beat needs for this reader.
///
/// Page-for-page by default, because the art is addressed by page index: a retelling free to
/// re-paginate produces a beautiful book with the wrong pictures in it.
#[tauri::command]
pub async fn universe_retell(state: State<'_, AppState>, payload: RetellRequest) -> Res<Value> {
    let edition = state.db.collection::<Document>("editions")
        .find_one(doc! { "id": &payload.edition_id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "that edition is gone".to_string())?;
    let universe = state.db.collection::<Document>("universes")
        .find_one(doc! { "id": &payload.universe_id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "that universe is gone".to_string())?;

    let pages = edition["pages"].as_array().cloned().unwrap_or_default();
    if pages.is_empty() {
        return Err("This edition has no pages yet — there is nothing to retell.".into());
    }
    let language = field_str(universe.get("axes").unwrap_or(&Value::Null), "language");

    // The voice the edition was written in, carried over. Without this a retelling silently reverts
    // to the model's default register, which is the one thing a retelling must not change.
    let voice_block = crate::commands::authorial::authorial_prompt_block(&edition["voice"]);
    let system = format!(
        "You retell an illustrated edition for one specific reader. You are not translating: you are \
         writing the same book again, for them.\n\n\
         {voice_block}\
         {block}\n\
         WHAT TO KEEP: the sequence of beats. Page {n} of your retelling covers what page {n} of the \
         source covers — the same turn, the same moment, the same weight — {pagination}\n\n\
         WHAT TO CHANGE: everything else that this reader's world decides. The words, the images, \
         how much is explained, which comparison is used, what the art shows.\n\n\
         Each page also carries an `art` prompt. Rewrite it too: a picture built for one reader's \
         world does not serve another's. Keep any recurring figure described identically across \
         pages or the book will not read as one book. Aspect ratio {aspect}.\n\n\
         Return ONLY: {{\"title\": string, \"pages\": [{{\"heading\": string, \"lines\": [string], \
         \"art\": string, \"caption\": string}}]}}. No markdown, no fences, no commentary.",
        block = universe_prompt_block(&universe),
        voice_block = voice_block,
        n = "N",
        pagination = if payload.keep_pagination {
            format!("and there are exactly {} pages, in the same order. Do not merge, split or \
                     reorder them.", pages.len())
        } else {
            "though you may merge or split pages where this reader needs a different amount of room."
                .to_string()
        },
        aspect = edition["aspect"].as_str().unwrap_or("2:3"),
    );

    let source_pages: Vec<String> = pages.iter().enumerate().map(|(i, p)| {
        let lines = p["lines"].as_array().map(|a| a.iter()
            .filter_map(|l| l.as_str()).collect::<Vec<_>>().join("\n"))
            .unwrap_or_default();
        format!("--- page {} ---\nheading: {}\n{}\nart: {}",
            i + 1,
            p["heading"].as_str().unwrap_or(""),
            lines,
            p["art"].as_str().unwrap_or(""))
    }).collect();

    let user = format!(
        "TITLE: {title}\nSOURCE LANGUAGE: {from}\n\n{pages}\n\nRetell it now{into}.",
        title = edition["title"].as_str().unwrap_or(""),
        from = edition["language"].as_str().unwrap_or("en"),
        pages = source_pages.join("\n\n"),
        into = if language.is_empty() { String::new() } else { format!(", in {language}") },
    );

    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let (content, model) =
        crate::commands::ai::provider_chat(&settings, &system, &user, 0.85, true).await?;
    let parsed = crate::commands::ai::extract_json_value(&content)
        .ok_or("the writer did not return usable JSON — try again")?;
    let new_pages = crate::commands::graphic_novel::pages_from_response(&parsed);
    if new_pages.is_empty() {
        return Err("the writer returned no pages".into());
    }

    let retold = json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "song_id": edition["song_id"].as_str().unwrap_or(""),
        "project_id": edition["project_id"].as_str().unwrap_or(""),
        "title": parsed["title"].as_str().filter(|t| !t.is_empty())
            .unwrap_or(edition["title"].as_str().unwrap_or("Edition")),
        "register": edition["register"].clone(),
        "register_label": edition["register_label"].clone(),
        "format": edition["format"].clone(),
        "aspect": edition["aspect"].clone(),
        "width": edition["width"].clone(),
        "height": edition["height"].clone(),
        "language": if language.is_empty() { edition["language"].as_str().unwrap_or("en").to_string() }
                    else { language.to_string() },
        "pages": new_pages,
        "universe_id": payload.universe_id,
        "universe_name": universe["name"].as_str().unwrap_or(""),
        "retold_from": payload.edition_id,
        "model": model,
        "status": "draft",
        "created_at": crate::models::now_iso(),
    });
    let d = bson::to_document(&retold).map_err(e)?;
    state.db.collection::<Document>("editions").insert_one(d).await.map_err(e)?;
    Ok(retold)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_universe() -> Value {
        json!({
            "id": "u1",
            "name": "Amara — Lagos",
            "depth": "grounded",
            "avatar": { "name": "Amara", "who": "A nurse on nights, 34", "appearance": "" },
            "axes": {
                "language": "Yoruba",
                "region": "A coastal city in West Africa",
                "culture": "A large extended family, everyone in earshot",
            },
        })
    }

    #[test]
    fn depth_takes_axes_from_the_front_so_deepening_only_ever_adds() {
        let sketch = axes_for_depth("sketch");
        let deep = axes_for_depth("deep");
        assert_eq!(sketch.len(), 3);
        assert_eq!(deep.len(), AXES.len());
        // Going deeper must never re-order what was already answered, or a sketch could not be
        // extended without re-asking.
        for (i, a) in sketch.iter().enumerate() {
            assert_eq!(a.id, deep[i].id);
        }
    }

    #[test]
    fn an_unknown_depth_lands_somewhere_usable_rather_than_panicking() {
        assert_eq!(depth("nonsense").id, "grounded");
    }

    #[test]
    fn every_axis_carries_an_instruction_a_writer_could_act_on() {
        for a in AXES {
            assert!(a.direction.len() > 80, "{} has no real direction", a.id);
            assert!(!a.examples.is_empty(), "{} offers no way in", a.id);
            assert!(a.question.ends_with('?'), "{} does not ask anything", a.id);
        }
    }

    #[test]
    fn an_unset_axis_contributes_nothing_rather_than_an_empty_heading() {
        let block = universe_prompt_block(&a_universe());
        assert!(block.contains("Yoruba"));
        assert!(block.contains("Language"));
        // Nothing was said about means, so the block must not mention it at all — a heading with
        // nothing under it reads to a model as a thing to fill in.
        assert!(!block.contains("Means"), "{block}");
    }

    #[test]
    fn the_block_names_the_person_and_forbids_generalising_from_them() {
        let block = universe_prompt_block(&a_universe());
        assert!(block.contains("Amara"));
        assert!(block.to_lowercase().contains("not about their group"));
    }

    #[test]
    fn an_empty_universe_produces_no_block_at_all() {
        assert_eq!(universe_prompt_block(&json!({})), "");
        assert_eq!(universe_prompt_block(&Value::Null), "");
    }

    #[test]
    fn the_interview_falls_back_to_a_person_before_it_asks_about_a_world() {
        let q = fallback_universe_question(&[], "grounded").unwrap();
        assert_eq!(q["field"], "name");
        let q = fallback_universe_question(&["name".into()], "grounded").unwrap();
        assert_eq!(q["field"], "who");
        let q = fallback_universe_question(&["name".into(), "who".into()], "grounded").unwrap();
        assert_eq!(q["field"], "language");
    }

    #[test]
    fn a_sketch_stops_asking_where_a_deep_universe_keeps_going() {
        let answered: Vec<String> = ["name", "who", "language", "region", "culture"]
            .iter().map(|s| s.to_string()).collect();
        assert!(fallback_universe_question(&answered, "sketch").is_none());
        assert_eq!(fallback_universe_question(&answered, "deep").unwrap()["field"], "circumstance");
    }

    #[test]
    fn answers_split_into_a_person_and_a_world_and_drop_what_is_neither() {
        let (avatar, axes) = universe_from_answers(&json!({
            "name": "Amara", "language": "Yoruba", "favourite_colour": "blue", "who": "  ",
        }));
        assert_eq!(avatar["name"], "Amara");
        assert!(avatar.get("who").is_none(), "a blank answer is not an answer");
        assert_eq!(axes["language"], "Yoruba");
        assert!(axes.get("favourite_colour").is_none());
    }

    #[test]
    fn offline_siblings_move_only_what_was_asked_for() {
        let out = derive_offline(&a_universe(), &["language".to_string()], 3);
        assert_eq!(out.len(), 3);
        for u in &out {
            assert_eq!(u["axes"]["region"], "A coastal city in West Africa", "a held axis moved");
            assert_ne!(u["axes"]["language"], "Yoruba", "the varied axis did not move");
        }
    }

    #[test]
    fn offline_siblings_are_the_same_every_time_so_a_re_derive_is_recognisable() {
        let vary = vec!["language".to_string(), "region".to_string()];
        assert_eq!(derive_offline(&a_universe(), &vary, 4), derive_offline(&a_universe(), &vary, 4));
    }

    #[test]
    fn offline_siblings_refuse_to_invent_a_person() {
        // Naming somebody from a table of regions is the caricature this module exists to avoid.
        for u in derive_offline(&a_universe(), &["region".to_string()], 2) {
            assert_eq!(u["avatar"]["name"], "");
            assert!(u["note"].as_str().unwrap().contains("starting point"));
        }
    }

    #[test]
    fn varying_nothing_derivable_yields_nothing_rather_than_copies() {
        assert!(derive_offline(&a_universe(), &[], 3).is_empty());
        assert!(derive_offline(&a_universe(), &["not_an_axis".to_string()], 3).is_empty());
    }

    #[test]
    fn a_model_that_moves_an_axis_it_was_not_given_has_it_put_back() {
        let base = a_universe();
        let list = vec![json!({
            "name": "Elena — Andes",
            "avatar": { "name": "Elena", "who": "A teacher, 40" },
            // region was not in `vary`, so this change is bookkeeping drift and gets reverted.
            "axes": { "language": "Spanish", "region": "A farming valley in the Andes" },
        })];
        let out = clean_derived(&base, &list, &["language".to_string()], 3);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["axes"]["language"], "Spanish");
        assert_eq!(out[0]["axes"]["region"], "A coastal city in West Africa");
        // Held axes the model omitted entirely are restored too.
        assert_eq!(out[0]["axes"]["culture"], "A large extended family, everyone in earshot");
    }

    #[test]
    fn a_sibling_identical_to_its_base_is_not_a_sibling() {
        let base = a_universe();
        let list = vec![
            json!({ "name": "Same", "axes": { "language": "Yoruba" } }),
            json!({ "name": "Different", "axes": { "language": "Tagalog" } }),
        ];
        let out = clean_derived(&base, &list, &["language".to_string()], 5);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["name"], "Different");
    }

    #[test]
    fn a_universe_names_itself_from_whatever_it_has() {
        assert_eq!(universe_title(&json!({ "name": "Amara" }), &json!({ "region": "Lagos" })),
                   "Amara — Lagos");
        assert_eq!(universe_title(&json!({ "name": "Amara" }), &json!({ "language": "Yoruba" })),
                   "Amara — Yoruba");
        assert_eq!(universe_title(&json!({}), &json!({})), "Untitled universe");
    }

    #[test]
    fn only_the_avatar_and_the_axes_can_be_written_into() {
        assert!(is_universe_field("name"));
        assert!(is_universe_field("language"));
        assert!(!is_universe_field("project_id"));
        assert!(!is_universe_field("id"));
    }
}
