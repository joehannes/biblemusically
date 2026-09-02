//! Volumes: many editions bound as one book.
//!
//! Until now a book was one song. `author_edition` reads one song's text, `build_epub` wraps that
//! edition's pages in an archive, and the result is a twelve-page file with one poem in it. That is
//! a real thing to make and it is not what anybody means by a book — a project with forty songs had
//! forty separate EPUBs and no way to say that they belong together.
//!
//! A **volume** is the manuscript: metadata, an ordered table of contents, and front and back
//! matter. Its entries are of three kinds and that is the whole vocabulary:
//!
//!   * **an edition** — one already-authored edition, which becomes a chapter;
//!   * **a part** — a divider with a title, under which the chapters that follow are nested;
//!   * **matter** — a page of prose that is not a song: a dedication, a foreword, an afterword, an
//!     about-the-author, a colophon.
//!
//! ## Automated first, controllable after
//!
//! `volume_autofill` takes a project and returns a whole manuscript: every song that has an edition
//! becomes a chapter in the project's own song order, optionally grouped into parts by language,
//! with the front and back matter a book is expected to have already in place and empty. That is the
//! path for somebody who wants a book out of what they have already written.
//!
//! Everything it produced is then an ordinary editable list. Reorder it, drop a chapter, add a part,
//! write your own foreword, change any of it — the automated path writes a starting manuscript, it
//! does not own the result. `volume_write_matter` will draft any one matter page from the project's
//! brief and the volume's own contents, and what it returns is text in a field, not a decision.
//!
//! ## What is checked before it is bound
//!
//! `volume_preflight` is the part that makes this publishing rather than exporting. It reports what
//! stands between the manuscript and a store — missing metadata a retailer sorts on, chapters whose
//! art was never generated, matter pages still empty, an empty volume — separated into what will
//! stop a submission and what will merely make the book worse. It is pure, so it is the same answer
//! in the interface and in the build, and it never blocks: a person is allowed to bind a draft.

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

fn s_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("").trim()
}

// ────────────────────────────────────────────────────────────────
// The matter vocabulary
// ────────────────────────────────────────────────────────────────

pub struct Matter {
    pub id: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    /// Front matter comes before the first chapter; back matter after the last.
    pub front: bool,
    /// Whether the automated path puts an empty one in. A book without a colophon is fine; a book
    /// whose author never considered a dedication is a book that was exported.
    pub standard: bool,
    /// What a writer is told when asked to draft it. Specific, because a page drafted from
    /// "write a foreword" is the same foreword every time.
    pub direction: &'static str,
}

/// Ordered as they appear in a book, front matter first.
pub const MATTER: &[Matter] = &[
    Matter { id: "dedication", label: "Dedication", front: true, standard: true,
        hint: "One line. Whom it is for.",
        direction: "Write a dedication: one line, at most fifteen words, addressed to somebody real. \
                    No explanation of why, no quotation, no flourish. If you cannot make it \
                    specific, make it shorter." },
    Matter { id: "epigraph", label: "Epigraph", front: true, standard: false,
        hint: "A line from the text itself, standing before the book.",
        direction: "Choose one short passage from the book's own source text to stand before it — \
                    two lines at most, quoted exactly, with its reference beneath. Pick the line the \
                    whole book turns out to have been about, not the most famous one." },
    Matter { id: "foreword", label: "Foreword", front: true, standard: false,
        hint: "Somebody else's introduction to the book.",
        direction: "Write a foreword of about 200 words in the voice of a reader who came to this \
                    material first and is handing it on. Say what it is, who made it and what it is \
                    for. Never summarise the contents and never praise the author." },
    Matter { id: "preface", label: "Preface", front: true, standard: true,
        hint: "The author's own note on how this came about.",
        direction: "Write a preface of about 150 words in the author's own voice: how this came \
                    about and what they were trying for. Plain, first person, no thesis. It may \
                    admit a difficulty. It must not sell the book." },
    Matter { id: "afterword", label: "Afterword", front: false, standard: false,
        hint: "What is left to say once it is read.",
        direction: "Write an afterword of about 150 words for somebody who has just finished. It \
                    may say what was left out and why. It must not restate what they have read." },
    Matter { id: "about", label: "About the author", front: false, standard: true,
        hint: "Three sentences. Stores show this one.",
        direction: "Write an author biography of three sentences in the third person: what they do, \
                    what this work comes out of, and where to find them. No adjectives about the \
                    work's quality." },
    Matter { id: "also_by", label: "Also by", front: false, standard: false,
        hint: "The other books, listed.",
        direction: "List the author's other titles, one per line, nothing else. If there are none, \
                    return an empty list rather than inventing any." },
    Matter { id: "colophon", label: "Colophon", front: false, standard: true,
        hint: "How the book was made — and, here, by what.",
        direction: "Write a colophon: how this book was made. It must state plainly that the words \
                    and pictures were generated with AI assistance, name the kind of tools rather \
                    than boasting of them, and say who directed the work. Six lines at most. This is \
                    a disclosure, not a credit sequence." },
];

pub fn matter(id: &str) -> Option<&'static Matter> {
    MATTER.iter().find(|m| m.id == id)
}

#[tauri::command]
pub async fn book_matter_kinds() -> Res<Value> {
    Ok(json!({
        "matter": MATTER.iter().map(|m| json!({
            "id": m.id, "label": m.label, "hint": m.hint,
            "front": m.front, "standard": m.standard,
        })).collect::<Vec<_>>(),
    }))
}

// ────────────────────────────────────────────────────────────────
// Planning the pages
// ────────────────────────────────────────────────────────────────

/// One page of a planned volume, before any file is read.
///
/// Separated from the build because this is the part with the decisions in it — order, roles, what
/// appears in the contents and at what depth — and the build is I/O. Pure, so the plan can be tested
/// and shown to somebody before anything is written.
#[derive(Debug, Clone, PartialEq)]
pub struct Planned {
    pub id: String,
    pub heading: String,
    pub lines: Vec<String>,
    pub role: crate::epub::Role,
    pub nav_depth: u8,
    /// Which edition this page came from, and which page of it. `None` for matter and parts.
    pub source: Option<(String, usize)>,
}

/// Whether the volume's chapters sit under parts, which decides whether a chapter nests.
fn has_parts(contents: &[Value]) -> bool {
    contents.iter().any(|c| s_of(c, "kind") == "part")
}

/// Turn a volume and its editions into the ordered pages of a book.
///
/// The rules that are not obvious:
///   * Front matter is never in the contents. A table of contents that begins "Dedication ·
///     Preface" is the mark of a converter; a reader that wants the dedication has already turned
///     to it.
///   * A chapter is one contents entry, not one per page. Its interior pages are in the spine and
///     not in the nav, because a volume of twelve twenty-page editions would otherwise have a
///     contents page with two hundred and forty entries.
///   * Chapters nest under parts only if the volume has any. A book with no parts gets a flat
///     contents rather than one indented under nothing.
///   * An entry naming an edition that is gone is skipped rather than becoming an empty chapter.
pub fn plan_pages(volume: &Value, editions: &[Value]) -> Vec<Planned> {
    let contents = volume["contents"].as_array().cloned().unwrap_or_default();
    let nested = has_parts(&contents);
    let mut out: Vec<Planned> = Vec::new();

    // ── front matter ─────────────────────────────────────────────────────
    let title = s_of(volume, "title");
    let mut title_lines = Vec::new();
    for key in ["subtitle", "author", "publisher"] {
        let v = s_of(volume, key);
        if !v.is_empty() { title_lines.push(v.to_string()); }
    }
    out.push(Planned {
        id: "front-title".into(), heading: title.to_string(), lines: title_lines,
        role: crate::epub::Role::TitlePage, nav_depth: 0, source: None,
    });
    let mut copyright: Vec<String> = Vec::new();
    for key in ["rights", "publisher"] {
        let v = s_of(volume, key);
        if !v.is_empty() { copyright.push(v.to_string()); }
    }
    let isbn = s_of(volume, "isbn");
    if !isbn.is_empty() { copyright.push(format!("ISBN {isbn}")); }
    if !copyright.is_empty() {
        out.push(Planned {
            id: "front-copyright".into(), heading: "Copyright".into(), lines: copyright,
            role: crate::epub::Role::Copyright, nav_depth: 0, source: None,
        });
    }

    let mut part_n = 0usize;
    let mut chapter_n = 0usize;
    let mut matter_n = 0usize;

    for entry in &contents {
        match s_of(entry, "kind") {
            "part" => {
                part_n += 1;
                let heading = {
                    let t = s_of(entry, "title");
                    if t.is_empty() { format!("Part {part_n}") } else { t.to_string() }
                };
                out.push(Planned {
                    id: format!("part-{part_n:02}"), heading,
                    lines: s_of(entry, "note").lines().map(|l| l.to_string())
                        .filter(|l| !l.trim().is_empty()).collect(),
                    role: crate::epub::Role::Part, nav_depth: 1, source: None,
                });
            }
            "matter" => {
                let body: Vec<String> = s_of(entry, "body").lines()
                    .map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
                if body.is_empty() { continue; }   // an empty matter page is a blank leaf
                matter_n += 1;
                let role_id = s_of(entry, "role");
                let front = matter(role_id).map(|m| m.front).unwrap_or(true);
                let heading = {
                    let h = s_of(entry, "heading");
                    if !h.is_empty() { h.to_string() }
                    else { matter(role_id).map(|m| m.label.to_string()).unwrap_or_default() }
                };
                out.push(Planned {
                    id: format!("matter-{matter_n:02}"), heading,
                    lines: body,
                    role: if front { crate::epub::Role::FrontMatter } else { crate::epub::Role::BackMatter },
                    // Back matter is worth finding; front matter is turned to, not looked up.
                    nav_depth: if front { 0 } else { 1 },
                    source: None,
                });
            }
            "edition" => {
                let id = s_of(entry, "edition_id");
                let Some(ed) = editions.iter().find(|x| s_of(x, "id") == id) else { continue };
                let pages = ed["pages"].as_array().cloned().unwrap_or_default();
                if pages.is_empty() { continue; }
                chapter_n += 1;
                let chapter_title = {
                    let t = s_of(entry, "title");
                    if !t.is_empty() { t.to_string() } else { s_of(ed, "title").to_string() }
                };
                for (i, p) in pages.iter().enumerate() {
                    out.push(Planned {
                        id: format!("ch{chapter_n:02}-p{:02}", i + 1),
                        // The chapter's own name on its first page; each page's heading after that,
                        // so a long edition still has its internal signposts.
                        heading: if i == 0 { chapter_title.clone() }
                                 else { s_of(p, "heading").to_string() },
                        lines: p["lines"].as_array().map(|a| a.iter()
                            .filter_map(|l| l.as_str().map(|s| s.to_string())).collect())
                            .unwrap_or_default(),
                        role: crate::epub::Role::Body,
                        nav_depth: if i > 0 { 0 } else if nested { 2 } else { 1 },
                        source: Some((id.to_string(), i)),
                    });
                }
            }
            _ => {}
        }
    }
    out
}

// ────────────────────────────────────────────────────────────────
// Preflight
// ────────────────────────────────────────────────────────────────

/// What stands between this manuscript and a store.
///
/// Two severities and no third: `blocking` is what a retailer rejects or what makes the book
/// unreadable, `warning` is what makes it worse. Nothing here refuses to build — a draft is a
/// legitimate thing to want, and a checker that will not let you see your own work is a checker
/// people route around.
pub fn preflight(volume: &Value, editions: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();
    let mut say = |severity: &str, what: &str, fix: &str| {
        out.push(json!({ "severity": severity, "what": what, "fix": fix }));
    };

    if s_of(volume, "title").is_empty() {
        say("blocking", "The volume has no title.", "Give it one — it is the dc:title every store sorts on.");
    }
    if s_of(volume, "author").is_empty() {
        say("blocking", "No author.", "A book with no dc:creator is rejected at upload.");
    }
    if s_of(volume, "language").is_empty() {
        say("blocking", "No language.", "Set the language; readers use it for hyphenation and stores for the storefront.");
    }

    let plan = plan_pages(volume, editions);
    let chapters = plan.iter().filter(|p| p.role == crate::epub::Role::Body
        && p.source.as_ref().is_some_and(|(_, i)| *i == 0)).count();
    if chapters == 0 {
        say("blocking", "No chapters. The volume has no editions in it.",
            "Add editions to the contents, or run the automatic fill.");
    }

    // Entries pointing at editions that no longer exist. Silent in the plan by design — a chapter
    // that vanished from a book without a word is exactly the failure this exists to catch.
    let contents = volume["contents"].as_array().cloned().unwrap_or_default();
    let missing = contents.iter()
        .filter(|c| s_of(c, "kind") == "edition")
        .filter(|c| !editions.iter().any(|e| s_of(e, "id") == s_of(c, "edition_id")))
        .count();
    if missing > 0 {
        say("blocking", &format!("{missing} chapter(s) point at an edition that has been deleted."),
            "Remove them from the contents, or write those editions again.");
    }

    // Art. Counted across the editions actually in the volume rather than across the project.
    let (mut with_art, mut wanted) = (0usize, 0usize);
    for c in contents.iter().filter(|c| s_of(c, "kind") == "edition") {
        let Some(ed) = editions.iter().find(|e| s_of(e, "id") == s_of(c, "edition_id")) else { continue };
        for p in ed["pages"].as_array().cloned().unwrap_or_default() {
            if s_of(&p, "art").is_empty() && s_of(&p, "art_prompt").is_empty() { continue; }
            wanted += 1;
            if !s_of(&p, "image_url").is_empty() { with_art += 1; }
        }
    }
    if wanted > 0 && with_art < wanted {
        say("warning", &format!("{} of {wanted} illustrated pages have no art yet.", wanted - with_art),
            "Generate the page art and collect it, or the book ships with the prompts unrendered.");
    }

    if s_of(volume, "cover_url").is_empty() {
        say("blocking", "No cover.",
            "Every store requires one, and it is the only thing most readers will ever see of the book.");
    }
    if s_of(volume, "description").is_empty() {
        say("warning", "No description.", "It is the blurb on the store page. Without it the listing is a title and a price.");
    }
    if volume["subjects"].as_array().is_none_or(|a| a.is_empty()) {
        say("warning", "No subjects.", "These are the categories the book is browsable under.");
    }
    if s_of(volume, "rights").is_empty() {
        say("warning", "No rights statement.", "A copyright page with nothing on it invites the assumption there is no claim.");
    }
    if s_of(volume, "isbn").is_empty() {
        say("warning", "No ISBN.",
            "Some stores issue one; the ones that do not will not list the book without it.");
    }

    let empty_matter: Vec<String> = contents.iter()
        .filter(|c| s_of(c, "kind") == "matter" && s_of(c, "body").is_empty())
        .map(|c| matter(s_of(c, "role")).map(|m| m.label.to_string())
            .unwrap_or_else(|| s_of(c, "role").to_string()))
        .collect();
    if !empty_matter.is_empty() {
        say("warning", &format!("Still empty: {}.", empty_matter.join(", ")),
            "Write them, or take them out — an empty matter page is dropped from the book, not printed blank.");
    }

    let has_colophon = contents.iter().any(|c|
        s_of(c, "kind") == "matter" && s_of(c, "role") == "colophon" && !s_of(c, "body").is_empty());
    if !has_colophon {
        // Not a nicety: the pages were written and drawn by a model, and several stores now require
        // that to be stated on the page rather than in a submission form.
        say("warning", "No colophon, so nothing in the book says how it was made.",
            "Write the colophon — it is where the AI assistance is disclosed.");
    }

    out
}

// ────────────────────────────────────────────────────────────────
// Commands
// ────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct VolumeSave {
    #[serde(default)] pub id: Option<String>,
    pub project_id: String,
    #[serde(default)] pub patch: Value,
}

/// The fields a person edits. A patch is not a licence to rewrite the record.
const EDITABLE: &[&str] = &[
    "title", "subtitle", "author", "illustrator", "translator", "publisher", "description",
    "rights", "subjects", "isbn", "series", "series_index", "pubdate", "language", "cover_url",
    "contents", "status",
];

#[tauri::command]
pub async fn volume_save(state: State<'_, AppState>, payload: VolumeSave) -> Res<Value> {
    let col = state.db.collection::<Document>("volumes");
    if let Some(id) = payload.id.clone().filter(|s| !s.is_empty()) {
        let mut set = Document::new();
        for key in EDITABLE {
            if let Some(v) = payload.patch.get(*key) {
                if let Ok(b) = bson::to_bson(v) { set.insert(*key, b); }
            }
        }
        if set.is_empty() { return Err("nothing to update".into()); }
        set.insert("updated_at", crate::models::now_iso());
        col.update_one(doc! { "id": &id }, doc! { "$set": set }).await.map_err(e)?;
        return Ok(col.find_one(doc! { "id": &id }).await.map_err(e)?
            .map(bson_to_value).unwrap_or_else(|| json!({ "id": id })));
    }

    let mut volume = json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "project_id": payload.project_id,
        "title": "", "subtitle": "", "author": "", "illustrator": "", "translator": "",
        "publisher": "", "description": "", "rights": "", "isbn": "", "series": "",
        "series_index": Value::Null, "pubdate": "", "language": "en", "cover_url": "",
        "subjects": [], "contents": [],
        "status": "draft",
        "created_at": crate::models::now_iso(),
    });
    if let (Some(obj), Some(patch)) = (volume.as_object_mut(), payload.patch.as_object()) {
        for key in EDITABLE {
            if let Some(v) = patch.get(*key) { obj.insert((*key).to_string(), v.clone()); }
        }
    }
    let d = bson::to_document(&volume).map_err(e)?;
    col.insert_one(d).await.map_err(e)?;
    Ok(volume)
}

#[tauri::command]
pub async fn volume_list(state: State<'_, AppState>, project_id: Option<String>) -> Res<Value> {
    use futures_util::StreamExt;
    let mut filter = Document::new();
    if let Some(p) = project_id.filter(|s| !s.is_empty()) { filter.insert("project_id", p); }
    let mut out = Vec::new();
    let mut cursor = state.db.collection::<Document>("volumes").find(filter).await.map_err(e)?;
    while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }
    Ok(json!({ "volumes": out }))
}

#[tauri::command]
pub async fn volume_delete(state: State<'_, AppState>, id: String) -> Res<Value> {
    state.db.collection::<Document>("volumes")
        .delete_one(doc! { "id": &id }).await.map_err(e)?;
    Ok(json!({ "deleted": id }))
}

async fn editions_of(state: &AppState, project_id: &str) -> Res<Vec<Value>> {
    use futures_util::StreamExt;
    let mut out = Vec::new();
    let mut cursor = state.db.collection::<Document>("editions")
        .find(doc! { "project_id": project_id }).await.map_err(e)?;
    while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }
    Ok(out)
}

/// The contents an automatic fill produces: every song that has an edition, in the project's own
/// song order, plus the matter a book is expected to have.
///
/// Pure, so what the automated path decides is testable and the same every time. Songs are the
/// ordering authority rather than editions, because editions are created in whatever order somebody
/// happened to write them and a book in that order is a book in no order at all.
pub fn autofill_contents(songs: &[Value], editions: &[Value], group_by: &str) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut last_group = String::new();

    for song in songs {
        let song_id = s_of(song, "id");
        // The newest edition that is not itself a retelling: a retold edition belongs in its own
        // volume for its own reader, not interleaved with the originals.
        let Some(ed) = editions.iter()
            .filter(|e| s_of(e, "song_id") == song_id && s_of(e, "retold_from").is_empty())
            .max_by_key(|e| s_of(e, "created_at").to_string())
        else { continue };

        if group_by == "language" {
            let lang = s_of(song, "language");
            if !lang.is_empty() && lang != last_group {
                out.push(json!({ "kind": "part", "title": lang.to_uppercase(), "note": "" }));
                last_group = lang.to_string();
            }
        }
        out.push(json!({
            "kind": "edition",
            "edition_id": s_of(ed, "id"),
            "title": s_of(song, "title"),
        }));
    }

    // Front matter goes at the front, so it is built separately and prepended — otherwise a
    // dedication would sit after the first part divider.
    let front: Vec<Value> = MATTER.iter().filter(|m| m.front && m.standard)
        .map(|m| json!({ "kind": "matter", "role": m.id, "heading": m.label, "body": "" }))
        .collect();
    let back: Vec<Value> = MATTER.iter().filter(|m| !m.front && m.standard)
        .map(|m| json!({ "kind": "matter", "role": m.id, "heading": m.label, "body": "" }))
        .collect();

    let mut all = front;
    all.extend(out);
    all.extend(back);
    all
}

#[derive(serde::Deserialize)]
pub struct AutofillRequest {
    pub project_id: String,
    /// Update this volume instead of making a new one.
    #[serde(default)] pub id: Option<String>,
    /// `none` or `language`.
    #[serde(default)] pub group_by: Option<String>,
}

/// A whole manuscript from what the project already contains.
#[tauri::command]
pub async fn volume_autofill(state: State<'_, AppState>, payload: AutofillRequest) -> Res<Value> {
    use futures_util::StreamExt;
    let mut songs = Vec::new();
    let mut cursor = state.db.collection::<Document>("songs")
        .find(doc! { "project_id": &payload.project_id }).await.map_err(e)?;
    while let Some(Ok(d)) = cursor.next().await { songs.push(bson_to_value(d)); }

    let editions = editions_of(&state, &payload.project_id).await?;
    let contents = autofill_contents(&songs, &editions,
        payload.group_by.as_deref().unwrap_or("none"));

    let chapters = contents.iter().filter(|c| s_of(c, "kind") == "edition").count();
    if chapters == 0 {
        return Err("No song in this project has an edition yet — write one first, and this will \
                    assemble the rest.".into());
    }
    let without: Vec<String> = songs.iter()
        .filter(|s| !editions.iter().any(|e| s_of(e, "song_id") == s_of(s, "id")))
        .map(|s| s_of(s, "title").to_string())
        .filter(|t| !t.is_empty())
        .collect();

    let project = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &payload.project_id }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let language = songs.first().map(|s| s_of(s, "language").to_string())
        .filter(|l| !l.is_empty()).unwrap_or_else(|| "en".into());

    let patch = json!({
        "title": s_of(&project, "name"),
        "language": language,
        "contents": contents,
    });
    let volume = volume_save(state.clone(), VolumeSave {
        id: payload.id.clone(), project_id: payload.project_id.clone(), patch,
    }).await?;

    Ok(json!({
        "volume": volume,
        "chapters": chapters,
        // Named rather than counted: "four songs are not in this book" is a number, and "these four
        // songs are not in this book" is something a person can act on.
        "songs_without_an_edition": without,
    }))
}

#[derive(serde::Deserialize)]
pub struct MatterRequest {
    pub volume_id: String,
    /// A `MATTER` id.
    pub role: String,
}

/// Draft one matter page from the project's brief and the volume's own contents.
///
/// What comes back is text in a field, not a decision: it lands in the entry for somebody to edit,
/// and the automated path never writes matter without being asked for that page by name.
#[tauri::command]
pub async fn volume_write_matter(state: State<'_, AppState>, payload: MatterRequest) -> Res<Value> {
    let volume = state.db.collection::<Document>("volumes")
        .find_one(doc! { "id": &payload.volume_id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "that volume is gone".to_string())?;
    let m = matter(&payload.role).ok_or_else(|| format!("no such matter page: {}", payload.role))?;

    let editions = editions_of(&state, s_of(&volume, "project_id")).await?;
    let chapter_titles: Vec<String> = volume["contents"].as_array().cloned().unwrap_or_default()
        .iter()
        .filter(|c| s_of(c, "kind") == "edition")
        .filter_map(|c| {
            let t = s_of(c, "title");
            if !t.is_empty() { return Some(t.to_string()); }
            editions.iter().find(|e| s_of(e, "id") == s_of(c, "edition_id"))
                .map(|e| s_of(e, "title").to_string())
        })
        .collect();

    let brief = crate::commands::ai::project_brief_block(
        &state.db, s_of(&volume, "project_id")).await;

    let system = format!(
        "You write the front and back matter of books — the pages that are not the book. This is a \
         {label}.\n\n{direction}\n\n\
         Write only the page itself. No heading, no label, no preamble, no markdown. Plain \
         paragraphs separated by blank lines. Return ONLY: {{\"body\": \"…\"}}",
        label = m.label, direction = m.direction,
    );
    let user = format!(
        "{brief}BOOK: {title}{subtitle}\nBY: {author}\nLANGUAGE: {lang}\n\nCONTENTS:\n{chapters}\n\n\
         Write the {label} now, in {lang}.",
        title = s_of(&volume, "title"),
        subtitle = {
            let s = s_of(&volume, "subtitle");
            if s.is_empty() { String::new() } else { format!(" — {s}") }
        },
        author = s_of(&volume, "author"),
        lang = s_of(&volume, "language"),
        chapters = if chapter_titles.is_empty() { "(no chapters yet)".to_string() }
                   else { chapter_titles.iter().map(|t| format!("- {t}")).collect::<Vec<_>>().join("\n") },
        label = m.label,
    );

    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let (content, model) =
        crate::commands::ai::provider_chat(&settings, &system, &user, 0.8, true).await?;
    let parsed = crate::commands::ai::extract_json_value(&content).unwrap_or(Value::Null);
    let body = parsed["body"].as_str().unwrap_or("").trim().to_string();
    if body.is_empty() {
        return Err("the writer returned an empty page — try again".into());
    }

    // Written into the entry it belongs to, creating it if the volume did not have that page.
    let mut contents = volume["contents"].as_array().cloned().unwrap_or_default();
    match contents.iter_mut().find(|c| s_of(c, "kind") == "matter" && s_of(c, "role") == payload.role) {
        Some(entry) => { entry["body"] = json!(body); }
        None => {
            let new = json!({ "kind": "matter", "role": m.id, "heading": m.label, "body": body });
            // Front matter belongs before the first chapter, back matter at the end.
            if m.front {
                let at = contents.iter().position(|c| s_of(c, "kind") != "matter").unwrap_or(contents.len());
                contents.insert(at, new);
            } else { contents.push(new); }
        }
    }
    state.db.collection::<Document>("volumes")
        .update_one(doc! { "id": &payload.volume_id },
                    doc! { "$set": { "contents": bson::to_bson(&contents).map_err(e)?,
                                     "updated_at": crate::models::now_iso() } })
        .await.map_err(e)?;

    Ok(json!({ "role": m.id, "body": body, "model": model, "contents": contents }))
}

#[tauri::command]
pub async fn volume_preflight(state: State<'_, AppState>, id: String) -> Res<Value> {
    let volume = state.db.collection::<Document>("volumes")
        .find_one(doc! { "id": &id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "that volume is gone".to_string())?;
    let editions = editions_of(&state, s_of(&volume, "project_id")).await?;
    let findings = preflight(&volume, &editions);
    let plan = plan_pages(&volume, &editions);
    Ok(json!({
        "findings": findings,
        "blocking": findings.iter().filter(|f| f["severity"] == "blocking").count(),
        "warnings": findings.iter().filter(|f| f["severity"] == "warning").count(),
        "pages": plan.len(),
        "chapters": plan.iter().filter(|p| p.source.as_ref().is_some_and(|(_, i)| *i == 0)).count(),
    }))
}

#[derive(serde::Deserialize)]
pub struct BuildVolumeRequest {
    pub id: String,
    /// Embed each chapter's song, so the book plays. The reason this is EPUB rather than PDF.
    #[serde(default = "yes")]
    pub include_audio: bool,
}
fn yes() -> bool { true }

/// Bind the whole volume into one EPUB.
#[tauri::command]
pub async fn build_volume_epub(state: State<'_, AppState>, payload: BuildVolumeRequest) -> Res<Value> {
    // A file written to disk leaves the app, so this is the export gate — checked here rather than
    // in the interface, because a disabled button is a CSS property anybody can delete.
    super::subscription::require(&state, "export").await?;

    let volume = state.db.collection::<Document>("volumes")
        .find_one(doc! { "id": &payload.id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "that volume is gone".to_string())?;
    let project_id = s_of(&volume, "project_id").to_string();
    let editions = editions_of(&state, &project_id).await?;

    let plan = plan_pages(&volume, &editions);
    if !plan.iter().any(|p| p.role == crate::epub::Role::Body) {
        return Err("This volume has no chapters in it yet.".into());
    }

    // ── the art ─────────────────────────────────────────────────────────
    // One file per illustrated page, named by the page it belongs to so two chapters cannot collide
    // on `panel-01.jpg`.
    let mut images: Vec<(String, Vec<u8>)> = Vec::new();
    let mut missing_art = 0usize;
    let mut art_for: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for planned in &plan {
        let Some((edition_id, index)) = &planned.source else { continue };
        let Some(ed) = editions.iter().find(|x| s_of(x, "id") == edition_id) else { continue };
        let Some(page) = ed["pages"].as_array().and_then(|a| a.get(*index)) else { continue };
        let url = s_of(page, "image_url").trim_start_matches("file://").to_string();
        if url.is_empty() || !std::path::Path::new(&url).exists() {
            if !s_of(page, "art").is_empty() { missing_art += 1; }
            continue;
        }
        match std::fs::read(&url) {
            Ok(bytes) => {
                let ext = std::path::Path::new(&url).extension()
                    .map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "jpg".into());
                let name = format!("{}.{ext}", planned.id);
                art_for.insert(planned.id.clone(), name.clone());
                images.push((name, bytes));
            }
            Err(_) => missing_art += 1,
        }
    }

    // ── the songs ───────────────────────────────────────────────────────
    // One audio file per chapter rather than one per book, and each page names its own — a page
    // narrated by the wrong song is worse than a page narrated by none.
    let mut audio: Vec<(String, Vec<u8>)> = Vec::new();
    let mut audio_for: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if payload.include_audio {
        let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for ed in &editions {
            let song_id = s_of(ed, "song_id");
            if song_id.is_empty() || seen.contains_key(song_id) { continue; }
            if !plan.iter().any(|p| p.source.as_ref().is_some_and(|(id, _)| id == s_of(ed, "id"))) {
                continue;
            }
            let Ok(Some(d)) = state.db.collection::<Document>("songs")
                .find_one(doc! { "id": song_id }).await else { continue };
            let song = bson_to_value(d);
            for key in ["local_audio_path", "local_audio_path_alt", "audio_url"] {
                let path = s_of(&song, key).trim_start_matches("file://").to_string();
                if path.is_empty() || !std::path::Path::new(&path).exists() { continue; }
                let Ok(bytes) = std::fs::read(&path) else { continue };
                let ext = std::path::Path::new(&path).extension()
                    .map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "mp3".into());
                let name = format!("song-{}.{ext}", audio.len() + 1);
                audio.push((name.clone(), bytes));
                seen.insert(song_id.to_string(), name);
                break;
            }
        }
        for planned in &plan {
            let Some((edition_id, _)) = &planned.source else { continue };
            let Some(ed) = editions.iter().find(|x| s_of(x, "id") == edition_id) else { continue };
            if let Some(name) = seen.get(s_of(ed, "song_id")) {
                audio_for.insert(planned.id.clone(), name.clone());
            }
        }
    }

    let pages: Vec<crate::epub::Page> = plan.iter().map(|p| crate::epub::Page {
        id: p.id.clone(),
        heading: p.heading.clone(),
        lines: p.lines.clone(),
        image: art_for.get(&p.id).cloned(),
        caption: None,
        dialogue: None,
        bubble_kind: "speech".into(),
        speaker_at: (0.5, 0.7),
        // Read-along is per-chapter and would need each chapter's own section timings; the audio is
        // manifested and playable either way, so this ships the book without pretending to a sync
        // it has not computed.
        has_overlay: false,
        span: (0.0, 0.0),
        audio: audio_for.get(&p.id).cloned(),
        role: p.role,
        nav_depth: p.nav_depth,
    }).collect();

    let isbn = s_of(&volume, "isbn").to_string();
    let mut meta = crate::epub::Metadata::new(
        s_of(&volume, "title"), s_of(&volume, "author"),
        {
            let l = s_of(&volume, "language");
            if l.is_empty() { "en" } else { l }
        },
        &if isbn.is_empty() { format!("urn:uuid:{}", payload.id) } else { format!("urn:isbn:{isbn}") },
    );
    for (field, slot) in [
        ("subtitle", &mut meta.subtitle), ("publisher", &mut meta.publisher),
        ("description", &mut meta.description), ("rights", &mut meta.rights),
        ("series", &mut meta.series), ("pubdate", &mut meta.pubdate),
    ] {
        let v = s_of(&volume, field);
        if !v.is_empty() { *slot = Some(v.to_string()); }
    }
    if !isbn.is_empty() { meta.isbn = Some(isbn); }
    meta.series_index = volume["series_index"].as_i64();
    meta.subjects = volume["subjects"].as_array().map(|a| a.iter()
        .filter_map(|s| s.as_str().map(|s| s.to_string())).collect()).unwrap_or_default();
    for (role, field) in [("ill", "illustrator"), ("trl", "translator")] {
        let name = s_of(&volume, field);
        if !name.is_empty() { meta.contributors.push((role.into(), name.into())); }
    }

    // The cover, if there is one, goes in as an image and is declared as such — otherwise the first
    // page of art becomes the thumbnail, which is a picture chosen by the alphabet.
    let mut cover: Option<String> = None;
    let cover_path = s_of(&volume, "cover_url").trim_start_matches("file://").to_string();
    if !cover_path.is_empty() && std::path::Path::new(&cover_path).exists() {
        if let Ok(bytes) = std::fs::read(&cover_path) {
            let ext = std::path::Path::new(&cover_path).extension()
                .map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "jpg".into());
            let name = format!("cover.{ext}");
            images.insert(0, (name.clone(), bytes));
            cover = Some(name);
        }
    }

    let bytes = crate::epub::build(
        &meta, &pages, &images, &audio, cover.as_deref(), &crate::models::now_iso());

    let dir = match crate::project_sync::project_folder(&state.db, &project_id).await {
        Some(folder) => folder.join("ebooks"),
        None => state.db.global_root().join("ebooks"),
    };
    std::fs::create_dir_all(&dir).map_err(e)?;
    let file = dir.join(format!("{}.epub", super::projects::slugify(
        if s_of(&volume, "title").is_empty() { "volume" } else { s_of(&volume, "title") })));
    std::fs::write(&file, &bytes).map_err(e)?;

    state.db.collection::<Document>("volumes")
        .update_one(doc! { "id": &payload.id },
                    doc! { "$set": { "epub_path": file.to_string_lossy().to_string(),
                                     "status": "built",
                                     "updated_at": crate::models::now_iso() } })
        .await.map_err(e)?;

    let findings = preflight(&volume, &editions);
    Ok(json!({
        "volume_id": payload.id,
        "path": file.to_string_lossy(),
        "bytes": bytes.len(),
        "pages": pages.len(),
        "chapters": plan.iter().filter(|p| p.source.as_ref().is_some_and(|(_, i)| *i == 0)).count(),
        "images": images.len(),
        "songs": audio.len(),
        "missing_art": missing_art,
        // Reported rather than enforced: a draft is a legitimate thing to bind, and a person who
        // wanted to see their book is not helped by being refused it.
        "blocking": findings.iter().filter(|f| f["severity"] == "blocking").count(),
        "findings": findings,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epub::Role;

    fn edition(id: &str, title: &str, pages: usize) -> Value {
        json!({
            "id": id, "title": title, "song_id": format!("song-{id}"),
            "created_at": "2026-01-01T00:00:00Z",
            "pages": (0..pages).map(|i| json!({
                "heading": format!("Page {}", i + 1),
                "lines": [format!("line {i}")],
                "art": "a scene",
                "image_url": if i == 0 { "file:///art.png" } else { "" },
            })).collect::<Vec<_>>(),
        })
    }

    fn volume(contents: Value) -> Value {
        json!({
            "id": "v1", "project_id": "p1",
            "title": "The Pentateuch", "author": "Lightkid", "language": "en",
            "publisher": "Lightkid Press", "rights": "© 2026", "isbn": "978",
            "cover_url": "file:///cover.png", "description": "A book",
            "subjects": ["Religion"],
            "contents": contents,
        })
    }

    #[test]
    fn a_volume_opens_with_a_title_page_that_is_not_in_its_own_contents() {
        let eds = vec![edition("e1", "Genesis", 3)];
        let plan = plan_pages(&volume(json!([{ "kind": "edition", "edition_id": "e1" }])), &eds);
        assert_eq!(plan[0].role, Role::TitlePage);
        assert_eq!(plan[0].heading, "The Pentateuch");
        assert_eq!(plan[0].nav_depth, 0, "a contents page does not list itself");
        assert_eq!(plan[1].role, Role::Copyright);
        assert_eq!(plan[1].nav_depth, 0);
    }

    #[test]
    fn a_chapter_is_one_contents_entry_however_many_pages_it_has() {
        // Twelve twenty-page editions would otherwise give a contents page of 240 entries.
        let eds = vec![edition("e1", "Genesis", 20)];
        let plan = plan_pages(&volume(json!([{ "kind": "edition", "edition_id": "e1" }])), &eds);
        let listed = plan.iter().filter(|p| p.role == Role::Body && p.nav_depth > 0).count();
        assert_eq!(listed, 1);
        assert_eq!(plan.iter().filter(|p| p.role == Role::Body).count(), 20, "all 20 are in the book");
        // The chapter's name is on its first page; the interior pages keep their own headings.
        let first = plan.iter().find(|p| p.role == Role::Body).unwrap();
        assert_eq!(first.heading, "Genesis");
        assert_eq!(plan.iter().filter(|p| p.heading == "Page 2").count(), 1);
    }

    #[test]
    fn chapters_nest_only_when_the_volume_actually_has_parts() {
        let eds = vec![edition("e1", "Genesis", 2), edition("e2", "Exodus", 2)];
        let flat = plan_pages(&volume(json!([
            { "kind": "edition", "edition_id": "e1" },
            { "kind": "edition", "edition_id": "e2" },
        ])), &eds);
        assert!(flat.iter().filter(|p| p.role == Role::Body && p.nav_depth > 0)
            .all(|p| p.nav_depth == 1), "no parts means a flat contents");

        let parted = plan_pages(&volume(json!([
            { "kind": "part", "title": "The Law" },
            { "kind": "edition", "edition_id": "e1" },
            { "kind": "edition", "edition_id": "e2" },
        ])), &eds);
        assert_eq!(parted.iter().filter(|p| p.role == Role::Part).count(), 1);
        assert!(parted.iter().filter(|p| p.role == Role::Body && p.nav_depth > 0)
            .all(|p| p.nav_depth == 2), "with parts, chapters nest under them");
    }

    #[test]
    fn an_untitled_part_is_numbered_rather_than_blank() {
        let plan = plan_pages(&volume(json!([{ "kind": "part" }, { "kind": "part" }])), &[]);
        let parts: Vec<&str> = plan.iter().filter(|p| p.role == Role::Part)
            .map(|p| p.heading.as_str()).collect();
        assert_eq!(parts, vec!["Part 1", "Part 2"]);
    }

    #[test]
    fn an_empty_matter_page_is_dropped_rather_than_bound_blank() {
        let plan = plan_pages(&volume(json!([
            { "kind": "matter", "role": "dedication", "body": "" },
            { "kind": "matter", "role": "preface", "body": "It began with a psalm." },
        ])), &[]);
        assert!(!plan.iter().any(|p| p.heading == "Dedication"));
        assert!(plan.iter().any(|p| p.lines == vec!["It began with a psalm."]));
    }

    #[test]
    fn back_matter_is_findable_and_front_matter_is_turned_to() {
        let plan = plan_pages(&volume(json!([
            { "kind": "matter", "role": "preface", "body": "How this began." },
            { "kind": "matter", "role": "about", "body": "They live by the sea." },
        ])), &[]);
        let preface = plan.iter().find(|p| p.role == Role::FrontMatter).unwrap();
        let about = plan.iter().find(|p| p.role == Role::BackMatter).unwrap();
        assert_eq!(preface.nav_depth, 0);
        assert_eq!(about.nav_depth, 1, "an about-the-author is looked up, not turned to");
    }

    #[test]
    fn a_chapter_whose_edition_was_deleted_is_skipped_and_then_reported() {
        let v = volume(json!([{ "kind": "edition", "edition_id": "gone" }]));
        assert!(!plan_pages(&v, &[]).iter().any(|p| p.role == Role::Body),
                "an empty chapter is worse than none");
        // …but silence would be worse still, so preflight names it.
        let f = preflight(&v, &[]);
        assert!(f.iter().any(|x| x["severity"] == "blocking"
            && x["what"].as_str().unwrap().contains("deleted")), "{f:?}");
    }

    #[test]
    fn preflight_blocks_on_what_a_store_rejects_and_warns_about_the_rest() {
        let mut v = volume(json!([{ "kind": "edition", "edition_id": "e1" }]));
        v["title"] = json!("");
        v["cover_url"] = json!("");
        v["description"] = json!("");
        let f = preflight(&v, &[edition("e1", "Genesis", 2)]);
        let blocking: Vec<&str> = f.iter().filter(|x| x["severity"] == "blocking")
            .map(|x| x["what"].as_str().unwrap()).collect();
        assert!(blocking.iter().any(|w| w.contains("no title")), "{blocking:?}");
        assert!(blocking.iter().any(|w| w.contains("No cover")), "{blocking:?}");
        assert!(f.iter().any(|x| x["severity"] == "warning"
            && x["what"].as_str().unwrap().contains("No description")));
    }

    #[test]
    fn a_book_with_nothing_in_it_says_so_before_anything_else_about_it() {
        let f = preflight(&volume(json!([])), &[]);
        assert!(f.iter().any(|x| x["severity"] == "blocking"
            && x["what"].as_str().unwrap().contains("No chapters")), "{f:?}");
    }

    #[test]
    fn missing_page_art_is_a_warning_counted_over_this_book_and_not_the_project() {
        let eds = vec![edition("e1", "Genesis", 4), edition("e2", "Exodus", 4)];
        // Only the first edition is in the volume, so only its three unillustrated pages count.
        let f = preflight(&volume(json!([{ "kind": "edition", "edition_id": "e1" }])), &eds);
        let art = f.iter().find(|x| x["what"].as_str().unwrap().contains("no art yet")).unwrap();
        assert_eq!(art["severity"], "warning", "a draft may be bound");
        assert!(art["what"].as_str().unwrap().starts_with("3 of 4"), "{art:?}");
    }

    #[test]
    fn every_book_is_asked_for_the_page_that_discloses_how_it_was_made() {
        let f = preflight(&volume(json!([{ "kind": "edition", "edition_id": "e1" }])),
                          &[edition("e1", "Genesis", 2)]);
        assert!(f.iter().any(|x| x["what"].as_str().unwrap().contains("colophon")), "{f:?}");
        assert!(matter("colophon").unwrap().direction.contains("AI"),
                "the colophon is where the disclosure lives, so it must ask for one");
    }

    #[test]
    fn the_automatic_fill_follows_the_songs_order_and_not_the_editions() {
        // Editions are made in whatever order somebody wrote them; a book in that order is a book
        // in no order.
        let songs = vec![
            json!({ "id": "s1", "title": "Genesis", "language": "en" }),
            json!({ "id": "s2", "title": "Exodus", "language": "en" }),
        ];
        let eds = vec![
            json!({ "id": "e2", "song_id": "s2", "title": "Exodus", "created_at": "2026-01-02T00:00:00Z", "pages": [{}] }),
            json!({ "id": "e1", "song_id": "s1", "title": "Genesis", "created_at": "2026-01-05T00:00:00Z", "pages": [{}] }),
        ];
        let c = autofill_contents(&songs, &eds, "none");
        let chapters: Vec<&str> = c.iter().filter(|x| s_of(x, "kind") == "edition")
            .map(|x| s_of(x, "edition_id")).collect();
        assert_eq!(chapters, vec!["e1", "e2"]);
    }

    #[test]
    fn the_automatic_fill_takes_the_newest_edition_and_never_a_retelling() {
        let songs = vec![json!({ "id": "s1", "title": "Genesis", "language": "en" })];
        let eds = vec![
            json!({ "id": "old", "song_id": "s1", "created_at": "2026-01-01T00:00:00Z", "pages": [{}] }),
            json!({ "id": "new", "song_id": "s1", "created_at": "2026-03-01T00:00:00Z", "pages": [{}] }),
            // A retelling belongs in its own volume, for its own reader.
            json!({ "id": "retold", "song_id": "s1", "retold_from": "new",
                    "created_at": "2026-04-01T00:00:00Z", "pages": [{}] }),
        ];
        let c = autofill_contents(&songs, &eds, "none");
        let chapters: Vec<&str> = c.iter().filter(|x| s_of(x, "kind") == "edition")
            .map(|x| s_of(x, "edition_id")).collect();
        assert_eq!(chapters, vec!["new"]);
    }

    #[test]
    fn grouping_by_language_opens_a_part_per_language_and_not_per_song() {
        let songs = vec![
            json!({ "id": "s1", "title": "A", "language": "en" }),
            json!({ "id": "s2", "title": "B", "language": "en" }),
            json!({ "id": "s3", "title": "C", "language": "es" }),
        ];
        let eds: Vec<Value> = ["s1", "s2", "s3"].iter().map(|s| json!({
            "id": format!("e-{s}"), "song_id": s, "created_at": "2026-01-01T00:00:00Z", "pages": [{}],
        })).collect();
        let c = autofill_contents(&songs, &eds, "language");
        let parts: Vec<&str> = c.iter().filter(|x| s_of(x, "kind") == "part")
            .map(|x| s_of(x, "title")).collect();
        assert_eq!(parts, vec!["EN", "ES"]);
    }

    #[test]
    fn the_automatic_fill_puts_front_matter_at_the_front_and_back_matter_at_the_back() {
        let songs = vec![json!({ "id": "s1", "title": "A", "language": "en" })];
        let eds = vec![json!({ "id": "e1", "song_id": "s1", "created_at": "2026-01-01T00:00:00Z", "pages": [{}] })];
        let c = autofill_contents(&songs, &eds, "language");
        let first_chapter = c.iter().position(|x| s_of(x, "kind") == "edition").unwrap();
        for (i, entry) in c.iter().enumerate() {
            if s_of(entry, "kind") != "matter" { continue; }
            let front = matter(s_of(entry, "role")).unwrap().front;
            assert_eq!(front, i < first_chapter, "{} is on the wrong side", s_of(entry, "role"));
        }
        // A part divider must not end up before the dedication.
        let first_part = c.iter().position(|x| s_of(x, "kind") == "part").unwrap();
        assert!(first_part > 0 && first_part < first_chapter);
    }

    #[test]
    fn every_matter_page_carries_a_direction_specific_enough_to_change_what_comes_back() {
        for m in MATTER {
            assert!(m.direction.len() > 100, "{} would produce the same page every time", m.id);
            assert!(!m.hint.is_empty());
        }
        // The standard set is what a book is expected to have, on both sides of it.
        assert!(MATTER.iter().any(|m| m.front && m.standard));
        assert!(MATTER.iter().any(|m| !m.front && m.standard));
    }
}
