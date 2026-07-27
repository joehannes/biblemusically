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
// Poetic graphic novels, and the ebooks they become
//
// The lyrics already exist, the art already exists, and the song already exists. What is missing is a
// reading of the text — a poetic, annotated edition in a chosen register — and a container that can
// hold all three at once.
//
// That container is EPUB 3, because it is the only mainstream ebook format that carries audio: an
// `<audio>` element in the page plus Media Overlays for read-along. A PDF of a graphic novel about a
// song cannot play the song, which is most of what makes this worth making.
//
// The registers below are deliberately distinct voices rather than settings on one voice. "Make it
// more poetic" produces the same faintly purple prose every time; "write it as an illuminated
// manuscript margin gloss" produces something with a shape.
// ────────────────────────────────────────────────────────────────

/// The voices an edition can be written in.
struct Register {
    id: &'static str,
    label: &'static str,
    hint: &'static str,
    /// Appended to the system prompt. Written as an instruction to a writer, not as adjectives.
    direction: &'static str,
}

const REGISTERS: &[Register] = &[
    Register {
        id: "illuminated",
        label: "Illuminated manuscript",
        hint: "Margin glosses and a scribe's commentary beside the verse.",
        direction: "Write as a medieval scribe glossing the passage in the margin: short, reverent, \
                    concrete. Each page is one verse or image, followed by two or three lines of \
                    gloss that notice something a reader would otherwise pass over. No modern idiom.",
    },
    Register {
        id: "free_verse",
        label: "Free verse",
        hint: "The passage re-heard as a contemporary poem.",
        direction: "Re-hear the passage as free verse in a contemporary voice. Short lines, no rhyme, \
                    no forced archaism. Keep the images that are already in the text and resist adding \
                    new ones. Let a line break do the work a stage direction would.",
    },
    Register {
        id: "panels",
        label: "Graphic-novel panels",
        hint: "Panel captions and dialogue, one beat per page.",
        direction: "Write as a graphic novel: one beat per page, each with a CAPTION of at most twelve \
                    words and, where the text has speech, one line of DIALOGUE. Describe nothing the \
                    art will show — the caption carries what the art cannot. In each page's `art` \
                    prompt, ask for uncluttered negative space where the words will sit (\"clear sky \
                    in the upper third\", \"open ground to the left\"): the bubble is composited on \
                    afterwards, and a picture composed for it reads as one image rather than as a \
                    caption stuck over a painting. Return `dialogue` as a plain string, and `bubble` \
                    as one of speech, thought, shout or caption.",
    },
    Register {
        id: "annotated",
        label: "Annotated study edition",
        hint: "The passage with the notes a first-time reader needs.",
        direction: "Write a study edition: the passage in plain modern prose, then the two or three \
                    notes a first-time reader actually needs — a place, a custom, a word that does not \
                    mean now what it meant then. Explain, never preach.",
    },
    Register {
        id: "childrens",
        label: "For children",
        hint: "Simple, warm, and never frightening.",
        direction: "Write for a child of about seven being read to at bedtime. Short sentences, warm, \
                    concrete, never frightening. One idea per page. Do not moralise at the end.",
    },
];

/// Page geometry per format. Graphic-novel pages are not video frames, and a 16:9 panel in a portrait
/// book is a letterboxed strip — so the aspect is part of the format rather than a later crop.
struct PageFormat {
    id: &'static str,
    label: &'static str,
    aspect: &'static str,
    /// Pixels, long edge — sized for a retina reader without making a 400MB book.
    width: i64,
    height: i64,
    hint: &'static str,
}

const FORMATS: &[PageFormat] = &[
    PageFormat { id: "page", label: "Full page", aspect: "2:3", width: 1600, height: 2400,
                 hint: "One image per page, the standard trade-paperback shape." },
    PageFormat { id: "panel", label: "Square panels", aspect: "1:1", width: 1600, height: 1600,
                 hint: "Two or three per page; reads well on a phone." },
    PageFormat { id: "splash", label: "Splash spread", aspect: "3:2", width: 2400, height: 1600,
                 hint: "Landscape, for the moments that deserve a whole spread." },
];

#[tauri::command]
pub async fn list_novel_styles() -> Res<Value> {
    Ok(json!({
        "registers": REGISTERS.iter().map(|r| json!({
            "id": r.id, "label": r.label, "hint": r.hint,
        })).collect::<Vec<_>>(),
        "formats": FORMATS.iter().map(|f| json!({
            "id": f.id, "label": f.label, "aspect": f.aspect,
            "width": f.width, "height": f.height, "hint": f.hint,
        })).collect::<Vec<_>>(),
    }))
}

fn register(id: &str) -> &'static Register {
    REGISTERS.iter().find(|r| r.id == id).unwrap_or(&REGISTERS[1])
}

fn page_format(id: &str) -> &'static PageFormat {
    FORMATS.iter().find(|f| f.id == id).unwrap_or(&FORMATS[0])
}

/// Split an AI edition response into pages, tolerating the shapes a model actually returns.
///
/// The prompt asks for `{ "pages": [{ heading, lines[], art }] }`. Models sometimes return a bare
/// array, sometimes `lines` as one string with newlines. All three become the same pages here rather
/// than an error the user has to re-roll their quota on.
pub fn pages_from_response(parsed: &Value) -> Vec<Value> {
    let raw = if parsed.is_array() {
        parsed.as_array().cloned().unwrap_or_default()
    } else {
        parsed["pages"].as_array().cloned().unwrap_or_default()
    };
    raw.iter().enumerate().filter_map(|(i, p)| {
        let heading = p["heading"].as_str().or_else(|| p["title"].as_str()).unwrap_or("").trim().to_string();
        let lines: Vec<String> = match &p["lines"] {
            Value::Array(a) => a.iter().filter_map(|l| l.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty()).collect(),
            Value::String(s) => s.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect(),
            _ => match &p["text"] {
                Value::String(s) => s.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect(),
                _ => Vec::new(),
            },
        };
        if heading.is_empty() && lines.is_empty() { return None; }
        Some(json!({
            "index": i,
            "heading": heading,
            "lines": lines,
            "art_prompt": p["art"].as_str().or_else(|| p["art_prompt"].as_str()).unwrap_or("").trim(),
            "caption": p["caption"].as_str().unwrap_or("").trim(),
            // The register asks for a line of dialogue and the model returns one; it was being
            // dropped here, which is why the bubbles never appeared. It is laid over the art as
            // real text at build time (see typography.rs), never baked into the picture.
            "dialogue": p["dialogue"].as_str().or_else(|| p["speech"].as_str()).unwrap_or("").trim(),
            "bubble_kind": p["bubble"].as_str().unwrap_or("speech").trim(),
            "image_url": Value::Null,
        }))
    }).collect()
}

#[derive(serde::Deserialize)]
pub struct AuthorRequest {
    pub song_id: String,
    /// A `REGISTERS` id.
    #[serde(default)]
    pub register: Option<String>,
    /// A `FORMATS` id — decides the aspect ratio of every panel prompt.
    #[serde(default)]
    pub format: Option<String>,
    /// Roughly how many pages. The model is told this is a target, not a quota.
    #[serde(default)]
    pub pages: Option<i64>,
}

/// Write a poetic edition of a song's text.
#[tauri::command]
pub async fn author_edition(state: State<'_, AppState>, payload: AuthorRequest) -> Res<Value> {
    let song = state.db.collection::<Document>("songs")
        .find_one(doc! { "id": &payload.song_id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "song not found".to_string())?;

    let reg = register(payload.register.as_deref().unwrap_or("free_verse"));
    let fmt = page_format(payload.format.as_deref().unwrap_or("page"));
    let want = payload.pages.unwrap_or(12).clamp(4, 40);

    let lyrics = song["lyrics"].as_str().unwrap_or("");
    if lyrics.trim().is_empty() {
        return Err("This song has no lyrics yet — there is nothing to make an edition of.".into());
    }
    let brief = crate::commands::ai::project_brief_block(
        &state.db, song["project_id"].as_str().unwrap_or("")).await;

    let system = format!(
        "You write illustrated editions of song texts for publication as ebooks.\n\n\
         VOICE: {direction}\n\n\
         Return ONLY a JSON object: {{\"title\": string, \"pages\": [{{\"heading\": string, \
         \"lines\": [string], \"art\": string, \"caption\": string}}]}}\n\
         - about {want} pages, each a single beat;\n\
         - `lines` are the words on the page, one array entry per line or paragraph;\n\
         - `art` is an image prompt for that page: a scene, not a mood board. Aspect ratio {aspect}. \
           Keep any recurring figure described the same way on every page it appears, or the art will \
           not read as one book;\n\
         - `caption` is a short line under the art, or \"\" for none;\n\
         - no markdown, no fences, no commentary.",
        direction = reg.direction, want = want, aspect = fmt.aspect,
    );
    let user = format!(
        "{brief}Song title: {title}\nLanguage: {lang}\n\nText:\n{lyrics}\n\n\
         Write the edition now.",
        title = song["title"].as_str().unwrap_or(""),
        lang = song["language"].as_str().unwrap_or("en"),
    );

    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let (content, model) = crate::commands::ai::provider_chat(&settings, &system, &user, 0.9, true).await?;
    let parsed = crate::commands::ai::extract_json_value(&content)
        .ok_or("the writer did not return usable JSON — try again")?;
    let pages = pages_from_response(&parsed);
    if pages.is_empty() {
        return Err("the writer returned no pages".into());
    }

    let edition = json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "song_id": payload.song_id,
        "project_id": song["project_id"].as_str().unwrap_or(""),
        "title": parsed["title"].as_str().filter(|t| !t.is_empty())
            .unwrap_or(song["title"].as_str().unwrap_or("Edition")),
        "register": reg.id,
        "register_label": reg.label,
        "format": fmt.id,
        "aspect": fmt.aspect,
        "width": fmt.width,
        "height": fmt.height,
        "language": song["language"].as_str().unwrap_or("en"),
        "pages": pages,
        "model": model,
        "status": "draft",
        "created_at": crate::models::now_iso(),
    });
    let d = bson::to_document(&edition).map_err(e)?;
    state.db.collection::<Document>("editions").insert_one(d).await.map_err(e)?;
    Ok(edition)
}

#[tauri::command]
pub async fn list_editions(state: State<'_, AppState>, project_id: Option<String>, song_id: Option<String>) -> Res<Value> {
    use futures_util::StreamExt;
    let mut filter = Document::new();
    if let Some(p) = project_id.filter(|s| !s.is_empty()) { filter.insert("project_id", p); }
    if let Some(s) = song_id.filter(|s| !s.is_empty()) { filter.insert("song_id", s); }
    let mut out = Vec::new();
    let mut cursor = state.db.collection::<Document>("editions").find(filter).await.map_err(e)?;
    while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }
    Ok(json!({ "editions": out }))
}

#[tauri::command]
pub async fn update_edition(state: State<'_, AppState>, id: String, patch: Value) -> Res<Value> {
    let mut set = Document::new();
    // Only the fields a person edits — a patch is not a licence to rewrite the record.
    for key in ["title", "pages", "status", "language", "cover_url"] {
        if let Some(v) = patch.get(key) {
            if let Ok(b) = bson::to_bson(v) { set.insert(key, b); }
        }
    }
    if set.is_empty() { return Err("nothing to update".into()); }
    state.db.collection::<Document>("editions")
        .update_one(doc! { "id": &id }, doc! { "$set": set }).await.map_err(e)?;
    Ok(json!({ "updated": id }))
}

#[tauri::command]
pub async fn delete_edition(state: State<'_, AppState>, id: String) -> Res<Value> {
    state.db.collection::<Document>("editions")
        .delete_one(doc! { "id": &id }).await.map_err(e)?;
    Ok(json!({ "deleted": id }))
}

/// Queue the page art through the same image pipeline everything else uses.
///
/// One synthetic section per page, tagged with the edition and page index so the result can be found
/// again — the pattern the publicity covers already use, so there is one image path to maintain.
#[tauri::command]
pub async fn generate_edition_art(
    state: State<'_, AppState>,
    state_arc: State<'_, std::sync::Arc<AppState>>,
    id: String,
) -> Res<Value> {
    let edition = state.db.collection::<Document>("editions")
        .find_one(doc! { "id": &id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "edition not found".to_string())?;

    let song_id = edition["song_id"].as_str().unwrap_or("").to_string();
    let aspect = edition["aspect"].as_str().unwrap_or("2:3").to_string();
    let pages = edition["pages"].as_array().cloned().unwrap_or_default();

    let mut queued = 0;
    for (i, page) in pages.iter().enumerate() {
        let prompt = page["art_prompt"].as_str().unwrap_or("");
        if prompt.trim().is_empty() { continue; }
        if page["image_url"].as_str().filter(|s| !s.is_empty()).is_some() { continue; }  // already made

        let section = json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "song_id": song_id,
            // 700+ keeps edition art clear of the song's own sections and of publicity covers (900+).
            "index": 700 + i as i64,
            "name": format!("edition page {}", i + 1),
            "line": page["heading"].as_str().unwrap_or(""),
            "image_prompt": prompt,
            "mood": "edition",
            "edition_id": id,
            "edition_page": i as i64,
            "aspect": aspect,
            "created_at": crate::models::now_iso(),
        });
        let section_id = section["id"].as_str().unwrap_or("").to_string();
        if let Ok(d) = bson::to_document(&section) {
            if state.db.collection::<Document>("sections").insert_one(d).await.is_ok()
                && crate::jobs::enqueue("image", &section_id, &state_arc).await.is_ok() {
                queued += 1;
            }
        }
    }
    if queued == 0 {
        return Err("every page either has art already or has no art prompt".into());
    }
    Ok(json!({ "queued": queued, "aspect": aspect }))
}

/// Pull finished art back onto the edition's pages.
///
/// Separate from queueing because image jobs finish minutes later — this is what the page calls when
/// the user comes back, and it is idempotent.
#[tauri::command]
pub async fn collect_edition_art(state: State<'_, AppState>, id: String) -> Res<Value> {
    use futures_util::StreamExt;
    let edition = state.db.collection::<Document>("editions")
        .find_one(doc! { "id": &id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "edition not found".to_string())?;

    let mut art: std::collections::HashMap<i64, String> = Default::default();
    let mut cursor = state.db.collection::<Document>("sections")
        .find(doc! { "edition_id": &id }).await.map_err(e)?;
    while let Some(Ok(d)) = cursor.next().await {
        let s = bson_to_value(d);
        if let (Some(page), Some(url)) = (s["edition_page"].as_i64(), s["image_url"].as_str()) {
            if !url.is_empty() { art.insert(page, url.to_string()); }
        }
    }

    let mut pages = edition["pages"].as_array().cloned().unwrap_or_default();
    let mut filled = 0;
    for (i, page) in pages.iter_mut().enumerate() {
        if let Some(url) = art.get(&(i as i64)) {
            page["image_url"] = json!(url);
            filled += 1;
        }
    }
    let b = bson::to_bson(&Value::Array(pages.clone())).map_err(e)?;
    state.db.collection::<Document>("editions")
        .update_one(doc! { "id": &id }, doc! { "$set": { "pages": b } }).await.map_err(e)?;
    Ok(json!({ "pages": pages.len(), "with_art": filled }))
}

// ── EPUB assembly ───────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct EpubRequest {
    pub edition_id: String,
    pub author: String,
    /// Embed the song itself, so the book plays. The whole reason this is EPUB.
    #[serde(default)]
    pub include_audio: Option<bool>,
}

/// Assemble the edition into an EPUB 3 file.
#[tauri::command]
pub async fn build_epub(state: State<'_, AppState>, payload: EpubRequest) -> Res<Value> {
    // A file written to disk leaves the app, so this is the "export" gate — checked here rather
    // than in the interface, because a disabled button is a CSS property anybody can delete.
    super::subscription::require(&state, "export").await?;
    let edition = state.db.collection::<Document>("editions")
        .find_one(doc! { "id": &payload.edition_id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "edition not found".to_string())?;

    let pages_raw = edition["pages"].as_array().cloned().unwrap_or_default();
    if pages_raw.is_empty() { return Err("this edition has no pages".into()); }

    let mut images: Vec<(String, Vec<u8>)> = Vec::new();
    let mut pages: Vec<crate::epub::Page> = Vec::new();
    let mut missing_art = 0;

    for (i, p) in pages_raw.iter().enumerate() {
        let mut image_name = None;
        let url = p["image_url"].as_str().unwrap_or("").trim_start_matches("file://");
        if !url.is_empty() && std::path::Path::new(url).exists() {
            match std::fs::read(url) {
                Ok(bytes) => {
                    let ext = std::path::Path::new(url).extension()
                        .map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "jpg".into());
                    let name = format!("panel-{:02}.{ext}", i + 1);
                    images.push((name.clone(), bytes));
                    image_name = Some(name);
                }
                Err(_) => missing_art += 1,
            }
        } else if !p["art_prompt"].as_str().unwrap_or("").is_empty() {
            missing_art += 1;
        }
        pages.push(crate::epub::Page {
            id: format!("page-{:02}", i + 1),
            heading: p["heading"].as_str().unwrap_or("").to_string(),
            lines: p["lines"].as_array().map(|a| a.iter()
                .filter_map(|l| l.as_str().map(|s| s.to_string())).collect()).unwrap_or_default(),
            image: image_name,
            caption: p["caption"].as_str().filter(|c| !c.is_empty()).map(|c| c.to_string()),
            // The dialogue the `panels` register already produces, finally rendered. It goes on as
            // HTML over the picture rather than into it: the words stay selectable and translatable,
            // so one artwork serves every language instead of one set of panels per language.
            dialogue: p["dialogue"].as_str().filter(|d| !d.is_empty()).map(|d| d.to_string()),
            bubble_kind: p["bubble_kind"].as_str().unwrap_or("speech").to_string(),
            speaker_at: (
                p["speaker_x"].as_f64().unwrap_or(0.5),
                // Assume the speaker is in the lower half unless told otherwise, which puts the
                // bubble at the top — where a panel's negative space usually is.
                p["speaker_y"].as_f64().unwrap_or(0.7),
            ),
            // Filled in below, once it is known whether there is audio to read along with.
            has_overlay: false,
            span: (0.0, 0.0),
        });
    }

    // The audio: what makes this an ebook that plays rather than one that describes.
    let mut audio: Vec<(String, Vec<u8>)> = Vec::new();
    if payload.include_audio.unwrap_or(true) {
        if let Ok(Some(d)) = state.db.collection::<Document>("songs")
            .find_one(doc! { "id": edition["song_id"].as_str().unwrap_or("") }).await {
            let song = bson_to_value(d);
            for key in ["local_audio_path", "local_audio_path_alt", "audio_url"] {
                let path = song[key].as_str().unwrap_or("").trim_start_matches("file://");
                if path.is_empty() || !std::path::Path::new(path).exists() { continue; }
                if let Ok(bytes) = std::fs::read(path) {
                    let ext = std::path::Path::new(path).extension()
                        .map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "mp3".into());
                    audio.push((format!("song.{ext}"), bytes));
                    break;
                }
            }
        }
    }
    // Read-along: one overlay per page, timed from the song's own analysed sections where there are
    // any. Paragraph-level rather than word-level, which would need forced alignment — and a reader
    // without overlay support ignores them and still plays the audio, so this cannot make a book worse.
    let mut synced = false;
    if !audio.is_empty() && !pages.is_empty() {
        let mut starts: Vec<f64> = Vec::new();
        if let Ok(mut cursor) = state.db.collection::<Document>("sections")
            .find(doc! { "song_id": edition["song_id"].as_str().unwrap_or("") }).await {
            use futures_util::StreamExt;
            let mut secs: Vec<Value> = Vec::new();
            while let Some(Ok(d)) = cursor.next().await { secs.push(bson_to_value(d)); }
            secs.sort_by_key(|s| s["index"].as_i64().unwrap_or(0));
            starts = secs.iter().filter_map(|s| s["start"].as_f64()).collect();
        }
        let total = state.db.collection::<Document>("songs")
            .find_one(doc! { "id": edition["song_id"].as_str().unwrap_or("") }).await.ok().flatten()
            .map(bson_to_value)
            .and_then(|s| s["duration"].as_f64())
            .filter(|d| *d > 1.0)
            .unwrap_or(180.0);
        let (spans, real) = crate::epub::page_spans(total, &starts, pages.len());
        synced = real;
        for (p, span) in pages.iter_mut().zip(spans) {
            p.has_overlay = true;
            p.span = span;
        }
    }

    // A page that plays the song, referenced from the first page's spine position.
    if let Some((name, _)) = audio.first() {
        pages.insert(0, crate::epub::Page {
            id: "page-00".into(),
            heading: edition["title"].as_str().unwrap_or("").to_string(),
            lines: vec![format!("This edition plays. If your reader supports audio, the song is here: audio/{name}")],
            image: None,
            caption: None,
            dialogue: None,
            bubble_kind: String::new(),
            speaker_at: (0.5, 0.7),
            has_overlay: false,
            span: (0.0, 0.0),
        });
    }

    let cover = images.first().map(|(n, _)| n.clone());
    let bytes = crate::epub::build(
        edition["title"].as_str().unwrap_or("Edition"),
        &payload.author,
        edition["language"].as_str().unwrap_or("en"),
        &format!("urn:uuid:{}", payload.edition_id),
        &pages,
        &images,
        &audio,
        cover.as_deref(),
        &crate::models::now_iso(),
    );

    let dir = match crate::project_sync::project_folder(
        &state.db, edition["project_id"].as_str().unwrap_or("")).await {
        Some(folder) => folder.join("ebooks"),
        None => state.db.global_root().join("ebooks"),
    };
    std::fs::create_dir_all(&dir).map_err(e)?;
    let file = dir.join(format!("{}.epub", slugify(edition["title"].as_str().unwrap_or("edition"))));
    std::fs::write(&file, &bytes).map_err(e)?;

    state.db.collection::<Document>("editions")
        .update_one(doc! { "id": &payload.edition_id },
                    doc! { "$set": { "epub_path": file.to_string_lossy().to_string(),
                                     "status": "built" } })
        .await.map_err(e)?;

    Ok(json!({
        "edition_id": payload.edition_id,
        "path": file.to_string_lossy(),
        "bytes": bytes.len(),
        "pages": pages.len(),
        "images": images.len(),
        "audio": audio.len(),
        "missing_art": missing_art,
        "read_along": !audio.is_empty(),
        // Said apart from "read_along" because a guess that reads along roughly should not be
        // advertised as synced.
        "read_along_timed_from_analysis": synced,
        "note": if audio.is_empty() {
            "No audio embedded — the song has no local file yet. EPUB is the format that can carry it, \
             so it is worth generating the music first."
        } else { "" },
    }))
}

fn slugify(s: &str) -> String {
    s.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-")
}

// ── Stores ──────────────────────────────────────────────────────────────────

/// An ebook store, its cut, and whether anything can be automated against it.
struct Store {
    id: &'static str,
    name: &'static str,
    royalty: &'static str,
    /// The price window where the good royalty applies, in USD. `(0.0, 0.0)` for none.
    band: (f64, f64),
    band_note: &'static str,
    api: &'static str,
    reach: &'static str,
    url: &'static str,
}

/// Checked July 2026. Royalty bands are the part that decides pricing, so they are data rather than
/// prose — the advice below is computed from them, not guessed.
const STORES: &[Store] = &[
    Store {
        id: "draft2digital", name: "Draft2Digital",
        royalty: "Takes 10% of list price",
        band: (0.0, 0.0),
        band_note: "No royalty band of its own — each downstream store applies its own.",
        api: "no public API; web upload",
        reach: "Apple Books, Kobo, Barnes & Noble, Google Play, Smashwords, OverDrive libraries — one \
                upload reaches all of them, which is why it is the sane first move.",
        url: "https://draft2digital.com",
    },
    Store {
        id: "kdp", name: "Amazon KDP",
        royalty: "70% inside the band, 35% outside — and the 70% is reduced by a per-megabyte delivery fee",
        band: (2.99, 9.99),
        band_note: "Price outside $2.99–$9.99 and the royalty halves. The delivery fee is why an \
                    image-heavy graphic novel earns closer to 55% than 70%: file size costs money here.",
        api: "no public API; web upload only",
        reach: "The largest single market.",
        url: "https://kdp.amazon.com",
    },
    Store {
        id: "kobo", name: "Kobo Writing Life",
        royalty: "70% inside the band, 45% outside",
        band: (2.99, 12.99),
        band_note: "A wider band than Amazon's, so a book priced above $9.99 is worth listing here \
                    directly.",
        api: "no public API; web upload",
        reach: "Second-largest dedicated ebook platform; strong in Canada, France, the Netherlands, \
                Australia and Japan.",
        url: "https://writinglife.kobo.com",
    },
    Store {
        id: "google_play", name: "Google Play Books",
        royalty: "About 52%",
        band: (0.0, 0.0),
        band_note: "No band; payment processing takes about 30 days.",
        api: "Partner Center supports bulk metadata upload (ONIX/spreadsheet) — the closest thing to \
              automation on this list",
        reach: "Android's default reader.",
        url: "https://play.google.com/books/publish",
    },
];

/// The stores, and what to price at.
#[tauri::command]
pub async fn list_ebook_stores() -> Res<Value> {
    Ok(json!({
        "stores": STORES.iter().map(|s| json!({
            "id": s.id, "name": s.name, "royalty": s.royalty,
            "band_low": s.band.0, "band_high": s.band.1,
            "band_note": s.band_note, "api": s.api, "reach": s.reach, "url": s.url,
        })).collect::<Vec<_>>(),
        "checked": "July 2026",
        "no_api_note": "None of these has a public publishing API. Draft2Digital reaches most of the \
                        others from one upload, so the practical path is: upload there once, list on \
                        Amazon separately, and record the upload as a macro if you will do it often.",
    }))
}

/// What to charge, from the stores' own royalty bands rather than from a feeling.
///
/// Returns a recommended price plus what it earns per copy at each store. An image-heavy book is
/// pushed up the band, not down: on KDP the delivery fee scales with file size, so a cheap graphic
/// novel can earn almost nothing per copy.
pub fn pricing_advice(pages: i64, has_audio: bool, megabytes: f64) -> Value {
    // The overlap of every band that has one — the only window where nothing is leaving money behind.
    let low = STORES.iter().filter(|s| s.band.0 > 0.0).map(|s| s.band.0).fold(0.0_f64, f64::max);
    let high = STORES.iter().filter(|s| s.band.1 > 0.0).map(|s| s.band.1).fold(f64::INFINITY, f64::min);

    // Start from length, then pay for the weight. KDP's delivery fee is per megabyte per copy, so a
    // 40MB illustrated book with audio has to be priced above the floor to clear it.
    let mut price = match pages {
        p if p <= 8 => low,
        p if p <= 16 => (low + 1.00).min(high),
        p if p <= 30 => (low + 2.00).min(high),
        _ => (low + 4.00).min(high),
    };
    if megabytes > 20.0 { price = (price + 1.00).min(high); }
    if has_audio { price = (price + 1.00).min(high); }
    let price = (price * 100.0).round() / 100.0;

    let per_store: Vec<Value> = STORES.iter().map(|s| {
        let in_band = s.band.0 == 0.0 || (price >= s.band.0 && price <= s.band.1);
        let rate = match s.id {
            "kdp" => if in_band { 0.70 } else { 0.35 },
            "kobo" => if in_band { 0.70 } else { 0.45 },
            "google_play" => 0.52,
            _ => 0.90,                                   // Draft2Digital keeps 10%
        };
        // KDP alone charges for delivery: about $0.15 per megabyte per copy sold.
        let delivery = if s.id == "kdp" && in_band { megabytes * 0.15 } else { 0.0 };
        let net = (price * rate - delivery).max(0.0);
        json!({
            "store": s.id, "name": s.name, "in_band": in_band,
            "rate": rate, "delivery_fee": (delivery * 100.0).round() / 100.0,
            "net_per_copy": (net * 100.0).round() / 100.0,
        })
    }).collect();

    json!({
        "recommended_price": price,
        "band": { "low": low, "high": high },
        "per_store": per_store,
        "why": format!(
            "{pages} pages{audio}, about {megabytes:.0}MB. Priced inside every store's good-royalty \
             band; the file size matters because Amazon charges delivery per megabyte per copy.",
            audio = if has_audio { " with audio" } else { "" },
        ),
    })
}

#[derive(serde::Deserialize)]
pub struct PricingRequest {
    pub edition_id: String,
}

#[tauri::command]
pub async fn ebook_pricing(state: State<'_, AppState>, payload: PricingRequest) -> Res<Value> {
    let edition = state.db.collection::<Document>("editions")
        .find_one(doc! { "id": &payload.edition_id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "edition not found".to_string())?;
    let pages = edition["pages"].as_array().map(|a| a.len() as i64).unwrap_or(0);
    let path = edition["epub_path"].as_str().unwrap_or("");
    let megabytes = std::fs::metadata(path).map(|m| m.len() as f64 / 1_048_576.0).unwrap_or(0.0);
    let has_audio = megabytes > 3.0;      // an EPUB this size is carrying the song
    Ok(pricing_advice(pages, has_audio, megabytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_register_is_a_direction_to_a_writer_not_a_pile_of_adjectives() {
        for r in REGISTERS {
            assert!(r.direction.len() > 80, "{} has no real direction", r.id);
            // A register that just says "be poetic" produces the same faintly purple prose every
            // time. Each one has to say what to DO.
            assert!(r.direction.contains("Write") || r.direction.contains("Re-hear"),
                    "{} does not instruct: {}", r.id, r.direction);
        }
        assert!(REGISTERS.len() >= 4);
    }

    #[test]
    fn pages_survive_the_three_shapes_a_model_returns() {
        let object = json!({ "pages": [{ "heading": "One", "lines": ["a", "b"], "art": "a field" }] });
        let bare_array = json!([{ "heading": "One", "lines": ["a", "b"] }]);
        let joined = json!({ "pages": [{ "heading": "One", "lines": "a\nb" }] });
        for shape in [object, bare_array, joined] {
            let pages = pages_from_response(&shape);
            assert_eq!(pages.len(), 1, "shape not handled: {shape}");
            assert_eq!(pages[0]["lines"], json!(["a", "b"]));
        }
    }

    #[test]
    fn an_empty_page_is_dropped_rather_than_published_blank() {
        let parsed = json!({ "pages": [
            { "heading": "", "lines": [] },
            { "heading": "Real", "lines": ["something"] },
            { "heading": "", "lines": ["", "  "] },
        ]});
        let pages = pages_from_response(&parsed);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0]["heading"], json!("Real"));
    }

    #[test]
    fn each_format_has_its_own_aspect_because_a_panel_is_not_a_frame() {
        let shapes: Vec<&str> = FORMATS.iter().map(|f| f.aspect).collect();
        assert_eq!(shapes.len(), shapes.iter().collect::<std::collections::HashSet<_>>().len(),
                   "two formats with the same aspect are one format");
        let page = page_format("page");
        assert!(page.height > page.width, "a book page is portrait");
        let splash = page_format("splash");
        assert!(splash.width > splash.height, "a splash spread is landscape");
        // An unknown id must not silently become a landscape video frame.
        assert_eq!(page_format("nonsense").id, "page");
    }

    #[test]
    fn pricing_lands_inside_every_band_that_has_one() {
        let advice = pricing_advice(12, false, 4.0);
        let price = advice["recommended_price"].as_f64().unwrap();
        assert!(price >= 2.99 && price <= 9.99, "got {price}");
        for s in advice["per_store"].as_array().unwrap() {
            assert_eq!(s["in_band"], json!(true), "{} is out of band at {price}", s["store"]);
        }
    }

    #[test]
    fn a_heavy_illustrated_book_is_priced_up_not_down() {
        // Amazon charges delivery per megabyte per copy, so the cheap price on a 40MB book is the one
        // that earns nothing.
        let light = pricing_advice(12, false, 2.0)["recommended_price"].as_f64().unwrap();
        let heavy = pricing_advice(12, true, 40.0)["recommended_price"].as_f64().unwrap();
        assert!(heavy > light, "heavy {heavy} should cost more than light {light}");

        let advice = pricing_advice(30, true, 40.0);
        let kdp = advice["per_store"].as_array().unwrap().iter()
            .find(|s| s["store"] == json!("kdp")).unwrap().clone();
        assert!(kdp["delivery_fee"].as_f64().unwrap() > 5.0, "the fee has to be visible: {kdp}");
        assert!(kdp["net_per_copy"].as_f64().unwrap() >= 0.0, "never report a negative payout");
    }

    #[test]
    fn no_store_is_listed_as_having_an_api_it_does_not_have() {
        for s in STORES {
            assert!(!s.api.is_empty());
            // Only Google Play's Partner Center offers bulk upload; the rest are web forms, and saying
            // otherwise would send someone looking for a key that does not exist.
            if s.id != "google_play" {
                assert!(s.api.contains("no public API"), "{} claims {}", s.id, s.api);
            }
        }
    }
}
