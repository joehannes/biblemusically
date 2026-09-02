//! Rewriting one section of a lyric, instead of the whole song.
//!
//! Every path into a lyric until now replaced all of it. `compose_lyrics` writes a whole song;
//! `compose_assist` proposes whole fields. So the ordinary experience of writing — this verse is
//! right, that chorus is not — had no move in the app at all. The only way to fix the chorus was to
//! roll the song again and lose the verse, and the way people actually coped was to stop using the
//! composer and edit the text by hand, which throws away every dial they set.
//!
//! Three pieces, and the first two are the ones that had to exist:
//!
//!   * **Splitting a lyric into its sections.** The engines read `[Verse]` / `[Chorus]` headers
//!     verbatim, so they are already the structure — this just reads what is there. It has to
//!     tolerate a lyric with no headers at all, and a preamble before the first one, because both
//!     arrive from real songs.
//!   * **Splicing one back.** Replacing a section must leave every other byte alone. Not "regenerate
//!     the surrounding text" — alone, so a person who spent an hour on verse two still has it.
//!   * **Asking for the rewrite.** The model sees the whole song and is told to change one section,
//!     which is a different and much easier request than writing a song: it can match the metre of
//!     the verses it is not touching, keep the rhyme scheme, and not restate what the chorus already
//!     says. That context is the reason a section rewrite beats writing a new song and pasting.
//!
//! The craft dials and, when there is one, the reader's universe come along, so a rewrite is written
//! under the same constraints as the original rather than reverting to the model's defaults.

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

/// A header line: `[Chorus]`, `[Soft female vocal]`. Whatever is in the brackets, because the
/// engines take arbitrary direction there and it is not this module's business what.
pub fn is_header(line: &str) -> bool {
    let t = line.trim();
    t.len() > 2 && t.starts_with('[') && t.ends_with(']') && !t[1..t.len() - 1].contains('[')
}

/// One section of a lyric: its header, the lines under it, and where it sits in the original.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    /// The header text without brackets, or empty for a preamble.
    pub header: String,
    /// Line indices into the original text, `start..end`, header line included.
    pub start: usize,
    pub end: usize,
    /// The section verbatim, header line and all.
    pub text: String,
}

/// Read a lyric's own structure.
///
/// Lines before the first header are a section with an empty header rather than being dropped or
/// folded into the first one: a song that opens with two lines and then `[Verse]` is a real thing,
/// and either of the alternatives silently changes it.
///
/// A lyric with no headers is one section. That is the honest answer — not an error, and not zero
/// sections, since "rewrite the whole thing" is then exactly what a section rewrite means.
pub fn split_sections(lyrics: &str) -> Vec<Section> {
    let lines: Vec<&str> = lyrics.split('\n').collect();
    if lines.iter().all(|l| l.trim().is_empty()) { return Vec::new(); }

    let starts: Vec<usize> = lines.iter().enumerate()
        .filter(|(_, l)| is_header(l)).map(|(i, _)| i).collect();

    let mut bounds: Vec<(usize, usize)> = Vec::new();
    match starts.first() {
        None => bounds.push((0, lines.len())),
        Some(&first) => {
            if first > 0 { bounds.push((0, first)); }
            for (n, &s) in starts.iter().enumerate() {
                let end = starts.get(n + 1).copied().unwrap_or(lines.len());
                bounds.push((s, end));
            }
        }
    }

    bounds.into_iter().map(|(start, end)| {
        let text = lines[start..end].join("\n");
        let header = if is_header(lines[start]) {
            lines[start].trim().trim_start_matches('[').trim_end_matches(']').trim().to_string()
        } else { String::new() };
        Section { header, start, end, text }
    }).collect()
}

/// Put a rewritten section back, leaving every other byte of the lyric alone.
///
/// The trailing newline handling matters more than it looks: a replacement that swallows the blank
/// line between two sections runs them together, and the engines read that as one section.
pub fn replace_section(lyrics: &str, index: usize, replacement: &str) -> String {
    let sections = split_sections(lyrics);
    let Some(target) = sections.get(index) else { return lyrics.to_string() };
    let lines: Vec<&str> = lyrics.split('\n').collect();

    // The blank lines that separated this section from the next belong to the gap, not to either
    // section — a section's slice happens to include them, and the model's replacement will not, so
    // they are counted off the original and put back. Without this the two sections run together
    // and the engine sings them as one.
    let trailing_blanks = lines[target.start..target.end].iter().rev()
        .take_while(|l| l.trim().is_empty()).count();

    let mut out: Vec<String> = lines[..target.start].iter().map(|s| s.to_string()).collect();
    out.extend(replacement.trim_end_matches('\n').split('\n').map(|s| s.to_string()));
    out.extend(std::iter::repeat_n(String::new(), trailing_blanks));
    out.extend(lines[target.end..].iter().map(|s| s.to_string()));
    out.join("\n")
}

/// Make sure a rewritten section still declares itself as the section it replaces.
///
/// A model asked for one chorus often returns the lines without the header, and a lyric that loses
/// a `[Chorus]` line does not lose a label — it loses a section, because the engine reads the
/// headers as the structure and will sing the chorus as part of the verse before it.
pub fn ensure_header(replacement: &str, header: &str) -> String {
    let body = replacement.trim_matches('\n');
    if header.is_empty() { return body.to_string(); }
    let first = body.split('\n').next().unwrap_or("");
    if is_header(first) { return body.to_string(); }
    format!("[{header}]\n{body}")
}

#[tauri::command]
pub async fn lyric_sections(lyrics: String) -> Res<Value> {
    Ok(json!({
        "sections": split_sections(&lyrics).iter().enumerate().map(|(i, s)| json!({
            "index": i,
            "header": s.header,
            "text": s.text,
            "lines": s.text.split('\n').skip(usize::from(!s.header.is_empty()))
                .filter(|l| !l.trim().is_empty()).count(),
        })).collect::<Vec<_>>(),
    }))
}

#[derive(serde::Deserialize)]
pub struct RewriteRequest {
    /// The whole lyric, so the model can see what it is not changing.
    pub lyrics: String,
    /// Which section, as returned by `lyric_sections`.
    pub index: usize,
    /// What is wrong with it, in the user's words. Optional: with nothing said, the instruction is
    /// simply to write it again, which is a real request.
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub project_id: String,
    /// The same craft dials the song was composed under.
    #[serde(default)]
    pub craft: Value,
    /// The reader this song is for, if one has been described.
    #[serde(default)]
    pub universe_id: Option<String>,
    /// The source text, when the song is a setting of one. Without it a rewrite drifts off the
    /// passage the rest of the song is holding to.
    #[serde(default)]
    pub source_text: String,
    /// How many to offer. More than one because the point of a section rewrite is choosing.
    #[serde(default)]
    pub count: Option<usize>,
}

/// Rewrite one section, in the context of the whole song.
#[tauri::command]
pub async fn rewrite_section(state: State<'_, AppState>, payload: RewriteRequest) -> Res<Value> {
    let sections = split_sections(&payload.lyrics);
    let target = sections.get(payload.index)
        .ok_or_else(|| "that section is not in this lyric any more".to_string())?
        .clone();
    let count = payload.count.unwrap_or(3).clamp(1, 5);

    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let engine = settings["music_engine"].as_str().unwrap_or("heartmula").to_string();

    let universe = match payload.universe_id.as_deref().filter(|s| !s.is_empty()) {
        Some(id) => state.db.collection::<Document>("universes")
            .find_one(doc! { "id": id }).await.map_err(e)?.map(bson_to_value),
        None => None,
    };
    let universe_block = universe.as_ref()
        .map(crate::commands::universe::universe_prompt_block)
        .unwrap_or_default();
    let craft_block = crate::commands::craft::craft_prompt_block(&payload.craft);
    let brief = crate::commands::ai::project_brief_block(&state.db, &payload.project_id).await;

    let system = format!(
        "You rewrite ONE section of a song that already exists. You are not writing a song.\n\n\
         This is the easier and more useful request, and only because you can see the rest: match \
         the metre the other sections establish, keep the rhyme scheme they use, do not restate what \
         the chorus already says, and land in a place the section after this one can follow from.\n\n\
         SECTION STRUCTURE — {guide}\n\n\
         Change nothing but this section. Do not comment on the rest of the song and do not return \
         any of it.\n\n\
         Return ONLY: {{\"options\":[{{\"text\":\"the section, header line included\",\
         \"what_changed\":\"at most 12 words\"}}, …]}} — {count} genuinely different options, not \
         one idea reworded {count} times.",
        guide = crate::commands::ai::engine_lyric_annotation_guide(&engine),
        count = count,
    );

    let user = format!(
        "{brief}{craft_block}{universe_block}\
         {source}THE WHOLE SONG (for context — return only the marked section):\n\n{marked}\n\n\
         THE SECTION TO REWRITE{header}:\n{current}\n\n{note}\n\
         Write {count} rewrites of that section now.",
        source = if payload.source_text.trim().is_empty() { String::new() }
                 else { format!("THE SOURCE TEXT THIS SONG SETS:\n{}\n\n", payload.source_text.trim()) },
        marked = mark_section(&sections, payload.index),
        header = if target.header.is_empty() { String::new() }
                 else { format!(" ([{}])", target.header) },
        current = target.text,
        note = if payload.note.trim().is_empty() {
            "The writer has not said what is wrong with it — they want to see it done differently."
                .to_string()
        } else {
            format!("WHAT THE WRITER SAYS ABOUT IT:\n{}", payload.note.trim())
        },
        count = count,
    );

    let (content, model) =
        crate::commands::ai::provider_chat(&settings, &system, &user, 0.95, true).await?;
    let parsed = crate::commands::ai::extract_json_value(&content)
        .ok_or("the writer did not return usable JSON — try again")?;

    let options: Vec<Value> = parsed["options"].as_array().cloned()
        .or_else(|| parsed.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|o| {
            let text = o["text"].as_str().or_else(|| o.as_str())?.trim();
            if text.is_empty() { return None; }
            let text = ensure_header(text, &target.header);
            Some(json!({
                "text": text,
                "what_changed": o["what_changed"].as_str().unwrap_or("").trim(),
                // The whole lyric with this option in place, so the caller previews and applies the
                // same string rather than splicing it a second way and getting a third result.
                "lyrics": replace_section(&payload.lyrics, payload.index, &text),
            }))
        })
        .take(count)
        .collect();

    if options.is_empty() {
        return Err("nothing usable came back — try again".into());
    }
    Ok(json!({ "options": options, "index": payload.index,
               "header": target.header, "model": model }))
}

/// The song with one section marked, so the model can see the context and the target at once.
fn mark_section(sections: &[Section], index: usize) -> String {
    sections.iter().enumerate().map(|(i, s)| {
        if i == index { format!(">>> REWRITE THIS SECTION >>>\n{}\n<<< END <<<", s.text) }
        else { s.text.clone() }
    }).collect::<Vec<_>>().join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SONG: &str = "[Verse]\nthe light has come\nthe night has gone\n\n[Chorus]\nsing it out\nsing it loud\n\n[Verse]\nand morning came";

    #[test]
    fn a_lyric_is_split_on_the_headers_the_engine_already_reads() {
        let s = split_sections(SONG);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].header, "Verse");
        assert_eq!(s[1].header, "Chorus");
        assert!(s[1].text.starts_with("[Chorus]"), "the header belongs to its section");
        assert!(s[1].text.contains("sing it loud"));
    }

    #[test]
    fn lines_before_the_first_header_are_a_section_and_not_a_loss() {
        // A song that opens with two lines and then [Verse] is a real thing; folding them into the
        // first verse or dropping them both silently change it.
        let s = split_sections("a quiet opening\nbefore anything\n\n[Verse]\nthen the light");
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].header, "");
        assert!(s[0].text.contains("a quiet opening"));
        assert_eq!(s[1].header, "Verse");
    }

    #[test]
    fn a_lyric_with_no_headers_is_one_section_rather_than_an_error() {
        let s = split_sections("just some lines\nand some more");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].header, "");
        assert_eq!(s[0].text, "just some lines\nand some more");
    }

    #[test]
    fn an_empty_lyric_has_no_sections() {
        assert!(split_sections("").is_empty());
        assert!(split_sections("\n\n  \n").is_empty());
    }

    #[test]
    fn a_bracketed_word_inside_a_line_is_not_a_header() {
        assert!(is_header("[Chorus]"));
        assert!(is_header("  [Soft female vocal]  "));
        assert!(!is_header("the light [that] came"));
        assert!(!is_header("[]"));
        assert_eq!(split_sections("the light [that] came\n[Verse]\nlines").len(), 2);
    }

    #[test]
    fn replacing_a_section_leaves_every_other_byte_alone() {
        // The point of the whole feature: an hour spent on verse two survives fixing the chorus.
        let out = replace_section(SONG, 1, "[Chorus]\nlift it high\nlift it now");
        assert!(out.contains("the light has come"), "{out}");
        assert!(out.contains("and morning came"));
        assert!(out.contains("lift it high"));
        assert!(!out.contains("sing it out"));
        assert_eq!(split_sections(&out).len(), 3, "still three sections: {out}");
    }

    #[test]
    fn the_blank_line_between_sections_survives_a_replacement() {
        // Without it two sections run together and the engine sings them as one.
        let out = replace_section(SONG, 0, "[Verse]\nsomething else");
        assert!(out.contains("something else\n\n[Chorus]"), "{out}");
    }

    #[test]
    fn replacing_the_last_section_does_not_leave_a_dangling_blank() {
        let out = replace_section(SONG, 2, "[Verse]\nand evening fell");
        assert!(out.ends_with("and evening fell"), "{out:?}");
    }

    #[test]
    fn replacing_a_section_that_is_gone_changes_nothing() {
        assert_eq!(replace_section(SONG, 9, "[Verse]\nnope"), SONG);
    }

    #[test]
    fn a_rewrite_that_forgot_its_header_gets_it_back() {
        // A lyric that loses a [Chorus] line does not lose a label, it loses a section — the engine
        // sings the chorus as part of the verse before it.
        assert_eq!(ensure_header("lift it high", "Chorus"), "[Chorus]\nlift it high");
        // …and one that kept it is left exactly as it is.
        assert_eq!(ensure_header("[Chorus]\nlift it high", "Chorus"), "[Chorus]\nlift it high");
        // A preamble has no header to restore.
        assert_eq!(ensure_header("two quiet lines", ""), "two quiet lines");
    }

    #[test]
    fn a_rewrite_that_renamed_its_own_header_is_left_alone() {
        // [Bridge] where a [Verse] was is a choice, not a mistake, and second-guessing it would
        // make the section rewrite unable to change a song's shape.
        assert_eq!(ensure_header("[Bridge]\nlines", "Verse"), "[Bridge]\nlines");
    }

    #[test]
    fn the_model_sees_the_whole_song_with_one_section_marked() {
        let marked = mark_section(&split_sections(SONG), 1);
        assert!(marked.contains("the light has come"), "context is the reason this works");
        assert!(marked.contains(">>> REWRITE THIS SECTION >>>\n[Chorus]"));
        assert_eq!(marked.matches("REWRITE THIS SECTION").count(), 1);
    }
}
