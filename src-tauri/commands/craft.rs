//! What kind of song this is — the decisions that separate one lyric from another.
//!
//! Until now everything steering `compose_lyrics` came down to three fields: a free-text theme, a
//! genre CSV, and the user's own section ideas. The system prompt asked for section headers in the
//! engine's dialect and for imagery that progresses, and that was the whole of it. So two runs from
//! the same chapter differed by temperature and by nothing a person had decided.
//!
//! Six dials, chosen because each one changes the output and a person can answer it without knowing
//! anything about the app:
//!
//!   * **form** — what shape the song is. Bounded by the engine's tag dialect, since the headers it
//!     implies are read verbatim by whichever engine sings it.
//!   * **faithfulness** — how close to the source text to stay. For scripture this is the most
//!     consequential decision in the whole app and it had no word for it: "quote it", "keep every
//!     claim", and "take it as a starting point" are three different products, and a user who wanted
//!     the first was getting whichever the model felt like.
//!   * **voice** — who is speaking. A psalm in the first person and the same psalm reported about
//!     somebody else are not versions of one lyric.
//!   * **shape** — lines per verse and a syllable range, because the engines sing the text verbatim
//!     and a line that does not scan costs a whole generation to discover.
//!   * **repetition** — how hard the hook works.
//!   * **register** — plain, literary or archaic, and how hard the words are.
//!
//! Everything here is a closed vocabulary rather than free text. A dial the model can be handed as
//! "whatever the user typed" is a dial with no defined effect, and this is a *prompt* block: its
//! whole job is to say something specific enough to change what comes back.

use serde_json::{json, Value};

/// One option on one dial: the id stored, the words shown, and the sentence the model is given.
pub struct Choice {
    pub id: &'static str,
    pub label: &'static str,
    /// What the picker says under the label. Written for somebody choosing, not documenting.
    pub hint: &'static str,
    /// What the composer is told. This is the part that has to be specific, because it is the only
    /// part that changes the output.
    pub instruction: &'static str,
}

pub const FORMS: &[Choice] = &[
    Choice { id: "verse_chorus", label: "Verses and a chorus",
        hint: "The familiar shape. The chorus is the line people leave with.",
        instruction: "Structure: verses alternating with a repeated chorus. The chorus must be the same \
                      words every time and must be the line worth remembering; verses carry the argument \
                      forward, the chorus does not develop." },
    Choice { id: "verse_refrain", label: "Verses with a returning line",
        hint: "Quieter than a chorus — one line comes back at the end of each verse.",
        instruction: "Structure: verses each ending with the same single refrain line. No separate chorus \
                      section. The refrain gains meaning from what precedes it rather than restating it." },
    Choice { id: "through_composed", label: "It never repeats",
        hint: "A story or an argument that only moves forward.",
        instruction: "Structure: through-composed. Nothing repeats — no chorus, no refrain. Each section \
                      moves the thought somewhere it has not been. Earn the ending." },
    Choice { id: "call_response", label: "Call and answer",
        hint: "Two voices — one asks or declares, the other answers.",
        instruction: "Structure: call and response. Alternate a leading line with an answering line \
                      throughout. The answer must be shorter than the call and must be singable by a group." },
    Choice { id: "litany", label: "A list that builds",
        hint: "The same frame, filled differently each time, until it lands.",
        instruction: "Structure: litany. Repeat one grammatical frame and change what fills it, so the \
                      accumulation is the effect. Break the pattern exactly once, at the end." },
];

pub const FAITHFULNESS: &[Choice] = &[
    Choice { id: "quote", label: "Use its own words",
        hint: "Arranged and set to music, but not rewritten.",
        instruction: "Faithfulness: use the source's own wording. You may select, order and repeat lines, \
                      and add nothing but the minimum connective words singing requires. Invent no images, \
                      no claims and no characters that are not in the source. If a passage will not sing, \
                      choose a different part of it rather than paraphrasing." },
    Choice { id: "close", label: "Say the same thing, singably",
        hint: "Every image and every claim kept; the words are yours.",
        instruction: "Faithfulness: paraphrase closely. Every image, claim and turn in the source must \
                      survive, in the same order, and nothing may be added that changes what it asserts. \
                      The wording is free; the content is not." },
    Choice { id: "inspired", label: "Take it somewhere",
        hint: "The source is the starting point, not the boundary.",
        instruction: "Faithfulness: the source is a point of departure. Stay true to its spirit and never \
                      contradict it, but you may bring in your own images, a present-day setting, and \
                      material it does not contain." },
];

pub const VOICES: &[Choice] = &[
    Choice { id: "first_person", label: "I", hint: "Someone singing their own experience.",
        instruction: "Point of view: first person singular throughout. This is one person's own \
                      experience, not a report of somebody else's." },
    Choice { id: "we", label: "We", hint: "A congregation, singing together.",
        instruction: "Point of view: first person plural. Written to be sung by a room of people at once, \
                      so nothing in it can be true of only one of them." },
    Choice { id: "witness", label: "Someone who saw it",
        hint: "Told about, not from inside.",
        instruction: "Point of view: a witness recounting what happened, in the third person. Close enough \
                      to have seen it; never inside the subject's head." },
    Choice { id: "addressed", label: "Spoken to you",
        hint: "The song addresses the listener directly.",
        instruction: "Point of view: second person. The song speaks to the listener throughout. Do not \
                      slip into narration." },
    Choice { id: "child", label: "A child's voice",
        hint: "Small words, real questions, nothing arch.",
        instruction: "Point of view: a child speaking, honestly and without irony. Short sentences, \
                      concrete nouns, real questions. Never a knowing adult imitating a child." },
];

pub const REGISTERS: &[Choice] = &[
    Choice { id: "plain", label: "Plain and modern",
        hint: "Ordinary words. Nothing anyone has to decode.",
        instruction: "Register: contemporary and plain. Everyday vocabulary, no archaisms, no inversions. \
                      A twelve-year-old should understand every line on first hearing." },
    Choice { id: "literary", label: "Literary",
        hint: "Denser images, a wider vocabulary.",
        instruction: "Register: literary. Richer imagery and a wider vocabulary are welcome, but every \
                      line must still be sayable aloud without stumbling." },
    Choice { id: "archaic", label: "Old and formal",
        hint: "Thee, thine, and the cadence that goes with them.",
        instruction: "Register: deliberately archaic. Older pronouns and word order, consistently — not \
                      sprinkled. If you use it once, use it throughout." },
];

pub const REPETITION: &[Choice] = &[
    Choice { id: "sparing", label: "Say it once",
        hint: "Almost nothing repeats. Every line earns its place.",
        instruction: "Repetition: sparing. Repeat only what carries real weight. Prefer a new line over a \
                      returning one." },
    Choice { id: "balanced", label: "Balanced", hint: "A hook that returns, and verses that move.",
        instruction: "Repetition: balanced. The hook returns as written; the verses do not repeat each \
                      other." },
    Choice { id: "hook_heavy", label: "Make it stick",
        hint: "The hook returns often, and early.",
        instruction: "Repetition: hook-forward. The hook appears early and returns often, including at \
                      least once before the first full verse ends. It must survive being sung twenty times." },
];

/// Sensible defaults. Chosen so a user who never opens this section still gets a coherent song, and
/// so the defaults describe what the app produced before this existed rather than a new opinion.
pub const DEFAULTS: &[(&str, &str)] = &[
    ("form", "verse_chorus"),
    ("faithfulness", "close"),
    ("voice", "we"),
    ("register", "plain"),
    ("repetition", "balanced"),
];

fn find(list: &'static [Choice], id: &str) -> Option<&'static Choice> {
    list.iter().find(|c| c.id == id)
}

fn pick(craft: &Value, key: &str, list: &'static [Choice]) -> Option<&'static Choice> {
    let chosen = craft[key].as_str().unwrap_or("").trim();
    let id = if chosen.is_empty() {
        DEFAULTS.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)?
    } else { chosen };
    find(list, id)
}

/// Lines per verse, clamped to what a verse can actually be.
///
/// Below two is not a verse and above eight nobody remembers it; a value outside that says more
/// about a malformed settings document than about anybody's intent.
pub fn lines_per_verse(craft: &Value) -> Option<i64> {
    let n = craft["lines_per_verse"].as_i64()?;
    (2..=8).contains(&n).then_some(n)
}

/// The syllable window a line should sit in, if one was asked for.
///
/// Returned as a pair only when both ends are present and sane, because half a range is not a
/// constraint and telling a model "between 6 and 0" is worse than telling it nothing.
pub fn syllable_range(craft: &Value) -> Option<(i64, i64)> {
    let lo = craft["syllables_min"].as_i64()?;
    let hi = craft["syllables_max"].as_i64()?;
    (lo >= 2 && hi >= lo && hi <= 20).then_some((lo, hi))
}

/// The block handed to the composer. Empty when nothing has been decided, so an untouched project's
/// prompt is exactly what it was before.
pub fn craft_prompt_block(craft: &Value) -> String {
    if !craft.is_object() { return String::new(); }
    let mut lines: Vec<String> = Vec::new();
    for (key, list) in [
        ("faithfulness", FAITHFULNESS),
        ("form", FORMS),
        ("voice", VOICES),
        ("register", REGISTERS),
        ("repetition", REPETITION),
    ] {
        if let Some(c) = pick(craft, key, list) { lines.push(format!("- {}", c.instruction)); }
    }
    if let Some(n) = lines_per_verse(craft) {
        lines.push(format!("- Length: about {n} lines per verse. Keep verses the same length as each other."));
    }
    if let Some((lo, hi)) = syllable_range(craft) {
        lines.push(format!(
            "- Metre: aim for {lo}–{hi} syllables per line. The engine sings this text verbatim, so a \
             line far outside that range will be rushed or padded by the singer rather than fixed."));
    }
    if lines.is_empty() { return String::new(); }
    format!("HOW THIS SONG IS WRITTEN (these are decisions the user made; follow them exactly):\n{}\n",
            lines.join("\n"))
}

/// The vocabulary, for the picker. One source of truth, so the interface cannot offer a value the
/// prompt builder does not know.
#[tauri::command]
pub async fn craft_catalogue() -> Result<Value, String> {
    let dump = |list: &'static [Choice]| -> Vec<Value> {
        list.iter().map(|c| json!({ "id": c.id, "label": c.label, "hint": c.hint })).collect()
    };
    Ok(json!({
        "form": dump(FORMS),
        "faithfulness": dump(FAITHFULNESS),
        "voice": dump(VOICES),
        "register": dump(REGISTERS),
        "repetition": dump(REPETITION),
        "defaults": DEFAULTS.iter().map(|(k, v)| (k.to_string(), json!(v)))
            .collect::<serde_json::Map<_, _>>(),
        "lines_per_verse": { "min": 2, "max": 8 },
        "syllables": { "min": 2, "max": 20 },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_decided_means_nothing_added_to_the_prompt() {
        // An untouched project must produce exactly the prompt it produced before this existed.
        assert_eq!(craft_prompt_block(&Value::Null), "");
        assert_eq!(craft_prompt_block(&json!("verse_chorus")), "");
    }

    #[test]
    fn an_empty_object_still_gets_the_defaults() {
        // Different from "no craft at all": the section was opened and left alone, which is a choice
        // to accept the defaults rather than a choice to say nothing.
        let block = craft_prompt_block(&json!({}));
        assert!(block.contains("chorus"), "{block}");
        assert!(block.contains("paraphrase closely"), "{block}");
    }

    #[test]
    fn faithfulness_is_stated_first_because_it_bounds_everything_else() {
        // A form instruction that conflicts with "use its own words" has to lose, and a model reads
        // an earlier instruction as the frame for a later one.
        let block = craft_prompt_block(&json!({ "faithfulness": "quote", "form": "litany" }));
        let f = block.find("own wording").expect("faithfulness present");
        let s = block.find("litany").expect("form present");
        assert!(f < s, "faithfulness must precede form");
    }

    #[test]
    fn an_unknown_value_is_dropped_rather_than_passed_through() {
        // These reach the prompt, so an id from a settings file edited by hand, or from a project
        // shared by somebody on a newer build, must not become an instruction.
        let block = craft_prompt_block(&json!({ "form": "interpretive_dance", "voice": "we" }));
        assert!(!block.contains("interpretive"), "{block}");
        assert!(block.contains("first person plural"), "the rest still applies");
    }

    #[test]
    fn a_half_open_syllable_range_is_not_a_constraint() {
        assert_eq!(syllable_range(&json!({ "syllables_min": 6 })), None);
        assert_eq!(syllable_range(&json!({ "syllables_max": 10 })), None);
        // Backwards, and beyond what a sung line can be.
        assert_eq!(syllable_range(&json!({ "syllables_min": 12, "syllables_max": 4 })), None);
        assert_eq!(syllable_range(&json!({ "syllables_min": 2, "syllables_max": 99 })), None);
        assert_eq!(syllable_range(&json!({ "syllables_min": 6, "syllables_max": 10 })), Some((6, 10)));
    }

    #[test]
    fn a_verse_length_outside_what_a_verse_is_gets_ignored() {
        assert_eq!(lines_per_verse(&json!({ "lines_per_verse": 1 })), None);
        assert_eq!(lines_per_verse(&json!({ "lines_per_verse": 40 })), None);
        assert_eq!(lines_per_verse(&json!({ "lines_per_verse": 4 })), Some(4));
    }

    #[test]
    fn the_metre_line_says_why_it_matters_rather_than_only_the_numbers() {
        let block = craft_prompt_block(&json!({ "syllables_min": 6, "syllables_max": 9 }));
        assert!(block.contains("6–9"));
        assert!(block.contains("verbatim"), "the reason is the part that makes it obeyed: {block}");
    }

    #[test]
    fn every_option_carries_an_instruction_that_could_change_an_output() {
        // A dial whose instruction is a label restated is a dial with no effect, and this is the
        // only thing standing between "six new settings" and "six new settings that do something".
        for list in [FORMS, FAITHFULNESS, VOICES, REGISTERS, REPETITION] {
            for c in list {
                assert!(c.instruction.len() > 60, "{} is too vague to act on", c.id);
                assert!(c.instruction.contains(':'), "{} must name what it constrains", c.id);
                assert!(!c.hint.is_empty() && !c.label.is_empty(), "{}", c.id);
            }
        }
    }

    #[test]
    fn every_default_names_a_real_option() {
        for (key, val) in DEFAULTS {
            let list = match *key {
                "form" => FORMS, "faithfulness" => FAITHFULNESS, "voice" => VOICES,
                "register" => REGISTERS, "repetition" => REPETITION,
                other => panic!("{other} has no vocabulary"),
            };
            assert!(find(list, val).is_some(), "{key}'s default {val} is not an option");
        }
    }

    #[test]
    fn ids_are_unique_within_a_dial() {
        for list in [FORMS, FAITHFULNESS, VOICES, REGISTERS, REPETITION] {
            let mut seen = std::collections::HashSet::new();
            for c in list { assert!(seen.insert(c.id), "duplicate id {}", c.id); }
        }
    }
}
