//! Unit tests for the pure logic the pipeline depends on.
//!
//! TODOS.md #14 asked for exactly this: the functions here have no I/O, no network and no store,
//! but a wrong answer from any of them silently corrupts a whole generation run — a mis-parsed
//! annotation block puts the wrong image on the wrong lyric, a bad JSON extraction throws away an
//! entire AI response, a mangled cookie makes every Suno call fail with a confusing 401. They were
//! previously only ever exercised by running the real pipeline by hand.
//!
//! Kept in one file rather than scattered `#[cfg(test)]` blocks so the pure-logic surface is
//! reviewable in one place; the store/vault/sync modules keep their tests next to their code
//! because those exercise real files.

#![cfg(test)]

use crate::commands::ai::extract_json_value;
use crate::commands::projects::slugify;
use crate::helpers::{derive_mood, mask_secret, parse_annotations, suggest_effects, EFFECT_PRESETS};

// ── Annotation parsing (lyrics → image-prompt pairing) ──────────────────────

#[test]
fn annotations_pair_each_line_with_the_bracket_above_it() {
    let annotations = "\
[a wide desert at dawn]
The morning breaks
Over the sand
[a lone figure walking]
He goes alone";
    let pairs = parse_annotations(annotations, "");
    assert_eq!(pairs.len(), 3);
    assert_eq!(pairs[0].line, "The morning breaks");
    assert_eq!(pairs[0].image_prompt, "a wide desert at dawn");
    // A prompt applies to the first line under it; later lines inherit nothing implicitly.
    assert_eq!(pairs[1].line, "Over the sand");
    assert_eq!(pairs[1].image_prompt, "");
    assert_eq!(pairs[2].image_prompt, "a lone figure walking");
}

#[test]
fn annotations_fall_back_to_bare_lyrics() {
    let pairs = parse_annotations("", "Line one\n\n  Line two  \n");
    assert_eq!(pairs.len(), 2, "blank lines are dropped, whitespace trimmed");
    assert_eq!(pairs[1].line, "Line two");
    assert!(pairs.iter().all(|p| p.image_prompt.is_empty()));
}

#[test]
fn annotations_of_nothing_produce_nothing() {
    assert!(parse_annotations("", "").is_empty());
    assert!(parse_annotations("   \n\n  ", "").is_empty());
}

// ── Mood derivation + effect suggestion ─────────────────────────────────────

#[test]
fn mood_comes_from_keywords_and_always_has_an_answer() {
    assert_eq!(derive_mood("a celestial choir above the clouds"), "serene");
    assert_eq!(derive_mood("VICTORY over the enemy"), "triumphant", "matching is case-insensitive");
    assert_eq!(derive_mood("radiant light breaking through"), "radiant");
    assert_eq!(derive_mood("a dark shadowed valley"), "dramatic");
    // No keyword at all still yields a usable mood rather than an empty string.
    assert_eq!(derive_mood("a chair"), "warm");
}

#[test]
fn suggested_effects_are_real_presets_and_never_empty() {
    for prompt in ["divine glory", "a chair", "wilderness", "grace"] {
        let mood = derive_mood(prompt);
        let effects = suggest_effects(mood);
        assert!(!effects.is_empty(), "{prompt} produced no effects");
        assert!(effects.len() <= 3, "{prompt} produced too many effects");
        for id in &effects {
            assert!(
                EFFECT_PRESETS.iter().any(|p| p.id == id),
                "{id} is not a known ffmpeg preset — it would fail at compose time"
            );
        }
    }
}

#[test]
fn unknown_moods_fall_back_to_safe_effects() {
    let effects = suggest_effects("no-such-mood");
    assert_eq!(effects, vec!["fade-cross".to_string(), "slow-zoom-in".to_string()]);
}

// ── JSON extraction from AI output ──────────────────────────────────────────

#[test]
fn json_is_recovered_from_chatty_model_output() {
    let wrapped = "Sure! Here you go:\n```json\n{\"title\": \"Psalm 23\", \"n\": 2}\n```\nHope that helps.";
    let value = extract_json_value(wrapped).expect("should find the object");
    assert_eq!(value["title"], "Psalm 23");
    assert_eq!(value["n"], 2);
}

#[test]
fn json_arrays_and_nesting_survive_extraction() {
    let content = "prefix [ {\"a\": [1,2]}, {\"b\": {\"c\": \"d\"}} ] suffix";
    let value = extract_json_value(content).expect("should find the array");
    assert!(value.is_array());
    assert_eq!(value[0]["a"][1], 2);
    assert_eq!(value[1]["b"]["c"], "d");
}

#[test]
fn unparseable_output_returns_none_rather_than_a_wrong_guess() {
    assert!(extract_json_value("no json here at all").is_none());
    assert!(extract_json_value("{ not: valid, json }").is_none());
    assert!(extract_json_value("").is_none());
}

// ── Slugs (project folder names) ────────────────────────────────────────────

#[test]
fn slugs_are_filesystem_safe() {
    assert_eq!(slugify("Psalms of Ascent"), "psalms-of-ascent");
    assert_eq!(slugify("Genesis 1: In the Beginning!"), "genesis-1-in-the-beginning");
    assert_eq!(slugify("  spaced  out  "), "spaced-out");
    assert_eq!(slugify("///"), "project", "a name with nothing usable still gets a folder");
    assert_eq!(slugify(""), "project");
    let slug = slugify("A/B\\C:D*E?F");
    assert!(
        !slug.contains(['/', '\\', ':', '*', '?']),
        "path separators would escape the projects directory: {slug}"
    );
}

// ── Secret masking ──────────────────────────────────────────────────────────

#[test]
fn oauth_secrets_are_masked_but_recognizable() {
    let masked = mask_secret(serde_json::json!({ "client_secret": "GOCSPX-abcdefghijkl" }));
    let shown = masked["client_secret"].as_str().unwrap();
    assert!(shown.ends_with("ijkl"));
    assert!(!shown.contains("GOCSPX"));
    // Too short to mask meaningfully → left alone rather than half-revealed.
    let short = mask_secret(serde_json::json!({ "client_secret": "abc" }));
    assert_eq!(short["client_secret"], "abc");
    // Nothing to mask is not an error.
    let none = mask_secret(serde_json::json!({ "label": "x" }));
    assert_eq!(none["label"], "x");
}
