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
// Book compilations
//
// A chapter at a time is how the songs get made; a whole book in one sitting is how people actually
// listen. So this collects a project's chapter videos for one book, puts them in scripture order, and
// concatenates them into a single long video with a timestamped tracklist for the description —
// which is what makes a two-hour upload navigable instead of a wall.
//
// Two things are deliberate:
//
//   • Order comes from the chapter number parsed out of the title, not from creation time. Songs are
//     generated language by language and style by style, so creation order interleaves; and a plain
//     text sort puts chapter 10 before chapter 2.
//   • The concat uses the filter, not the demuxer. The demuxer needs every input to share codec and
//     frame size and silently produces a broken file when they don't — and inputs here were rendered
//     across months, engines and settings.
// ────────────────────────────────────────────────────────────────

/// Recognise the scripture reference in a song title.
///
/// Titles follow the project's own pattern, which puts the reference in the middle:
/// `"Lightkid - Genesis 3 (ambient gospel)"`. Book names can carry a leading ordinal ("1 Samuel"),
/// and a chapter number is always the number that directly follows the name — never the "2" in
/// "Volume 2" or a year in the style tag.
///
/// Returns `(book_id, chapter)`.
pub fn parse_reference(title: &str) -> Option<(String, i64)> {
    let hay = title.to_lowercase();
    let books = crate::commands::bible::BIBLE_BOOKS.as_array()?;

    // Longest name first: "1 samuel" must win over "samuel", and "song of solomon" over "song".
    let mut candidates: Vec<(&str, &str)> = books.iter()
        .filter_map(|b| Some((b["id"].as_str()?, b["name"].as_str()?)))
        .collect();
    candidates.sort_by_key(|(_, name)| std::cmp::Reverse(name.len()));

    for (id, name) in candidates {
        let needle = name.to_lowercase();
        let Some(at) = hay.find(&needle) else { continue };
        // The number that follows the name, skipping only separators — so "Genesis 3" matches and
        // "Genesis, sung in 2026" does not take 2026 as a chapter.
        let rest = &hay[at + needle.len()..];
        let digits: String = rest.chars()
            .skip_while(|c| matches!(c, ' ' | ':' | '.' | '-' | '_' | '#'))
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if digits.is_empty() { continue; }
        if let Ok(n) = digits.parse::<i64>() {
            if n >= 1 && n <= 150 { return Some((id.to_string(), n)); }
        }
    }
    None
}

/// Cumulative start time of each track, in seconds.
pub fn track_starts(durations: &[f64]) -> Vec<f64> {
    let mut out = Vec::with_capacity(durations.len());
    let mut acc = 0.0;
    for d in durations {
        out.push(acc);
        acc += d.max(0.0);
    }
    out
}

/// `h:mm:ss` past an hour, `m:ss` below it — the format YouTube parses into chapter links.
pub fn fmt_timestamp(secs: f64) -> String {
    let t = secs.max(0.0).round() as i64;
    let (h, m, s) = (t / 3600, (t % 3600) / 60, t % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// The `filter_complex` graph that joins `n` inputs into one stream.
///
/// Every input is normalised first — scaled into the frame, padded rather than stretched, and forced
/// to one frame rate, sample rate and channel count. Without that the concat filter refuses inputs
/// whose parameters differ, and inputs rendered months apart do differ.
pub fn concat_filter(n: usize, w: i64, h: i64, fps: i64) -> String {
    let mut parts = String::new();
    for i in 0..n {
        parts.push_str(&format!(
            "[{i}:v]scale={w}:{h}:force_original_aspect_ratio=decrease,\
             pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,fps={fps}[v{i}];\
             [{i}:a]aresample=44100,aformat=sample_fmts=fltp:channel_layouts=stereo[a{i}];"
        ));
    }
    for i in 0..n { parts.push_str(&format!("[v{i}][a{i}]")); }
    parts.push_str(&format!("concat=n={n}:v=1:a=1[outv][outa]"));
    parts
}

/// The description body: what the compilation is, then a timestamp per chapter.
pub fn description(book_name: &str, tracks: &[(i64, String, f64)]) -> String {
    let mut out = String::new();
    out.push_str(&format!("{book_name} — the complete book, chapter by chapter.\n\n"));
    // YouTube turns a list starting at 0:00 into chapter markers; anything else stays plain text.
    for (i, (chapter, title, start)) in tracks.iter().enumerate() {
        let stamp = if i == 0 { "0:00".to_string() } else { fmt_timestamp(*start) };
        out.push_str(&format!("{stamp} — {book_name} {chapter}: {title}\n"));
    }
    out
}

/// How many inputs one ffmpeg invocation is asked to join.
///
/// Psalms is 150 chapters; a 150-way concat filter with 300 normalisation stages is a graph that
/// exhausts memory before it produces anything. Long books get released in volumes, which is how
/// compilations are published anyway — so the cap is a real limit, not a placeholder.
const MAX_TRACKS: usize = 40;

// ── Planning ────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct PlanRequest {
    pub project_id: String,
    /// A `BIBLE_BOOKS` id. Omitted: report every book the project has songs for.
    #[serde(default)]
    pub book: Option<String>,
    /// Chapter range, inclusive. Used to cut a long book into volumes.
    #[serde(default)]
    pub from: Option<i64>,
    #[serde(default)]
    pub to: Option<i64>,
}

/// What a compilation of this book would contain, and what is missing from it.
///
/// Reports rather than builds: a book whose chapters are half-rendered should say so before an hour
/// of encoding, not after.
#[tauri::command]
pub async fn compilation_plan(state: State<'_, AppState>, payload: PlanRequest) -> Res<Value> {
    plan(&state, &payload).await
}

async fn plan(state: &State<'_, AppState>, payload: &PlanRequest) -> Res<Value> {
    use futures_util::StreamExt;

    let mut songs: Vec<Value> = Vec::new();
    let mut cursor = state.db.collection::<Document>("songs")
        .find(doc! { "project_id": &payload.project_id }).await.map_err(e)?;
    while let Some(Ok(d)) = cursor.next().await { songs.push(bson_to_value(d)); }

    // book id → chapter → song
    let mut by_book: std::collections::BTreeMap<String, Vec<(i64, Value)>> = Default::default();
    let mut unmatched = 0;
    for s in songs {
        match parse_reference(s["title"].as_str().unwrap_or("")) {
            Some((book, chapter)) => by_book.entry(book).or_default().push((chapter, s)),
            None => unmatched += 1,
        }
    }

    let books_meta = crate::commands::bible::BIBLE_BOOKS.as_array().cloned().unwrap_or_default();
    let name_of = |id: &str| books_meta.iter()
        .find(|b| b["id"].as_str() == Some(id))
        .and_then(|b| b["name"].as_str()).unwrap_or(id).to_string();
    let total_chapters = |id: &str| books_meta.iter()
        .find(|b| b["id"].as_str() == Some(id))
        .and_then(|b| b["chapters"].as_i64()).unwrap_or(0);

    let mut books = Vec::new();
    for (id, mut entries) in by_book {
        if let Some(want) = payload.book.as_deref() { if want != id { continue; } }
        entries.sort_by_key(|(c, _)| *c);
        entries.dedup_by_key(|(c, _)| *c);          // one song per chapter; the first wins
        let in_range: Vec<&(i64, Value)> = entries.iter()
            .filter(|(c, _)| payload.from.map_or(true, |f| *c >= f) && payload.to.map_or(true, |t| *c <= t))
            .collect();

        let mut ready = Vec::new();
        let mut missing_video = Vec::new();
        for (chapter, song) in &in_range {
            let path = song["video_url"].as_str().unwrap_or("").trim_start_matches("file://");
            if path.is_empty() {
                missing_video.push(*chapter);
            } else {
                ready.push(json!({
                    "chapter": chapter,
                    "song_id": song["id"],
                    "title": song["title"],
                    "duration": song["duration"].as_f64().unwrap_or(0.0),
                    "video": path,
                    "on_disk": std::path::Path::new(path).exists(),
                }));
            }
        }
        let have: Vec<i64> = entries.iter().map(|(c, _)| *c).collect();
        let expected = total_chapters(&id);
        let gaps: Vec<i64> = (1..=expected).filter(|c| !have.contains(c)).collect();
        let secs: f64 = ready.iter().filter_map(|t| t["duration"].as_f64()).sum();

        books.push(json!({
            "book": id,
            "book_name": name_of(&id),
            "chapters_total": expected,
            "chapters_with_songs": have.len(),
            "tracks": ready,
            "missing_video": missing_video,
            "missing_songs": gaps,
            "complete": gaps.is_empty(),
            "estimated_seconds": secs,
            "estimated_runtime": fmt_timestamp(secs),
            "over_track_cap": ready.len() > MAX_TRACKS,
            "track_cap": MAX_TRACKS,
        }));
    }

    Ok(json!({ "books": books, "unmatched_titles": unmatched }))
}

// ── Building ────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct BuildRequest {
    pub project_id: String,
    pub book: String,
    #[serde(default)]
    pub from: Option<i64>,
    #[serde(default)]
    pub to: Option<i64>,
    /// Appended to the title, for volumes: "Vol. 2".
    #[serde(default)]
    pub label: Option<String>,
}

/// Concatenate a book's chapter videos into one compilation, with a timestamped description.
#[tauri::command]
pub async fn build_compilation(state: State<'_, AppState>, payload: BuildRequest) -> Res<Value> {
    let planned = plan(&state, &PlanRequest {
        project_id: payload.project_id.clone(),
        book: Some(payload.book.clone()),
        from: payload.from,
        to: payload.to,
    }).await?;

    let book = planned["books"].as_array().and_then(|a| a.first())
        .ok_or_else(|| format!("no songs in this project parse as chapters of {}", payload.book))?;
    let tracks = book["tracks"].as_array().cloned().unwrap_or_default();
    if tracks.len() < 2 {
        return Err("a compilation needs at least two rendered chapters".into());
    }
    if tracks.len() > MAX_TRACKS {
        return Err(format!(
            "{} rendered chapters is more than one encode should join ({MAX_TRACKS} max). \
             Release it in volumes — set a chapter range.", tracks.len()
        ));
    }
    for t in &tracks {
        if t["on_disk"].as_bool() != Some(true) {
            return Err(format!("chapter {}'s video is missing from disk: {}",
                t["chapter"], t["video"].as_str().unwrap_or("?")));
        }
    }

    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let runner = crate::commands::remote_exec::configured_runner(&settings);

    // Durations decide the timestamps, so a stored 0 has to be resolved rather than guessed — an
    // off-by-one-track description is worse than a slow build.
    let mut durations = Vec::with_capacity(tracks.len());
    for t in &tracks {
        let stored = t["duration"].as_f64().unwrap_or(0.0);
        if stored > 1.0 { durations.push(stored); continue; }
        let path = t["video"].as_str().unwrap_or("").to_string();
        durations.push(probe_seconds(&settings, &path).await.unwrap_or(0.0));
    }

    let (w, h, fps) = (1920_i64, 1080_i64, 30_i64);
    let mut args: Vec<String> = vec![
        "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
    ];
    let mut inputs = serde_json::Map::new();
    for (i, t) in tracks.iter().enumerate() {
        let name = format!("c{i}");
        args.push("-i".into());
        args.push(format!("{{in:{name}}}"));
        let src = t["video"].as_str().unwrap_or("").to_string();
        let reference = if runner == "local" {
            src
        } else {
            // A remote encoder cannot open this machine's disk. Same precondition as remote
            // rendering, and worth failing on rather than quietly encoding an hour of video here.
            remote_video_url(&state, &payload.project_id, &src).await?
        };
        inputs.insert(name, Value::String(reference));
    }
    let out_name = format!("{}-compilation{}.mp4", payload.book,
        payload.label.as_deref().map(|l| format!("-{}", slugify(l))).unwrap_or_default());
    args.extend([
        "-filter_complex".into(), concat_filter(tracks.len(), w, h, fps),
        "-map".into(), "[outv]".into(), "-map".into(), "[outa]".into(),
        "-c:v".into(), "libx264".into(), "-preset".into(), "veryfast".into(), "-crf".into(), "21".into(),
        "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into(),
        "-movflags".into(), "+faststart".into(), "{out:compilation}".into(),
    ]);

    let job = crate::commands::remote_exec::CliJob {
        tool: "ffmpeg".into(),
        args,
        inputs: Value::Object(inputs),
        outputs: json!({ "compilation": { "name": out_name.clone() } }),
        // Hours, not minutes: this is thirty chapters of video in one pass.
        timeout_s: Some(6 * 3600),
        runner: None,
    };
    let run = crate::commands::remote_exec::exec_job(&settings, job).await?;
    if run["status"].as_str() != Some("ok") {
        return Err(format!("the compilation could not be built ({}): {}",
            run["runner"].as_str().unwrap_or("?"),
            run["stderr"].as_str().or_else(|| run["error"].as_str()).unwrap_or("no detail")));
    }

    // Keep it beside the chapter videos rather than in a temp dir.
    let first = tracks[0]["video"].as_str().unwrap_or("");
    let out_dir = std::path::Path::new(first).parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("compilations"))
        .ok_or("could not determine an output folder")?;
    std::fs::create_dir_all(&out_dir).map_err(e)?;
    let out_path = out_dir.join(&out_name);
    if let Some(produced) = run["files"]["compilation"]["local_path"].as_str() {
        if produced != out_path.to_string_lossy() {
            let _ = std::fs::rename(produced, &out_path)
                .or_else(|_| std::fs::copy(produced, &out_path).map(|_| ()));
        }
    }

    let starts = track_starts(&durations);
    let listed: Vec<(i64, String, f64)> = tracks.iter().enumerate().map(|(i, t)| (
        t["chapter"].as_i64().unwrap_or(0),
        t["title"].as_str().unwrap_or("").to_string(),
        starts.get(i).copied().unwrap_or(0.0),
    )).collect();
    let book_name = book["book_name"].as_str().unwrap_or(&payload.book);
    let title = match payload.label.as_deref() {
        Some(l) => format!("{book_name} — {l}"),
        None => format!("{book_name} — the complete book"),
    };

    let doc_out = json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "project_id": payload.project_id,
        "book": payload.book,
        "book_name": book_name,
        "title": title,
        "label": payload.label,
        "media_path": out_path.to_string_lossy(),
        "seconds": durations.iter().sum::<f64>(),
        "runtime": fmt_timestamp(durations.iter().sum()),
        "tracks": tracks.iter().enumerate().map(|(i, t)| json!({
            "chapter": t["chapter"],
            "song_id": t["song_id"],
            "title": t["title"],
            "start": starts.get(i).copied().unwrap_or(0.0),
            "timestamp": fmt_timestamp(starts.get(i).copied().unwrap_or(0.0)),
        })).collect::<Vec<_>>(),
        "description": description(book_name, &listed),
        "status": "ready",
        "created_at": crate::models::now_iso(),
    });
    if let Ok(d) = bson::to_document(&doc_out) {
        state.db.collection::<Document>("compilations").insert_one(d).await.map_err(e)?;
    }
    Ok(doc_out)
}

#[tauri::command]
pub async fn list_compilations(state: State<'_, AppState>, project_id: String) -> Res<Value> {
    use futures_util::StreamExt;
    let mut out = Vec::new();
    let mut cursor = state.db.collection::<Document>("compilations")
        .find(doc! { "project_id": &project_id }).await.map_err(e)?;
    while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }
    Ok(json!({ "compilations": out }))
}

#[tauri::command]
pub async fn delete_compilation(state: State<'_, AppState>, id: String) -> Res<Value> {
    state.db.collection::<Document>("compilations")
        .delete_one(doc! { "id": &id }).await.map_err(e)?;
    Ok(json!({ "deleted": id }))
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn slugify(s: &str) -> String {
    s.to_lowercase().chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-').filter(|p| !p.is_empty())
        .collect::<Vec<_>>().join("-")
}

/// Read a media file's duration, wherever command steps run.
async fn probe_seconds(settings: &Value, path: &str) -> Option<f64> {
    let job = crate::commands::remote_exec::CliJob {
        tool: "ffprobe".into(),
        args: vec![
            "-v".into(), "error".into(),
            "-show_entries".into(), "format=duration".into(),
            "-of".into(), "csv=p=0".into(), "{in:src}".into(),
        ],
        inputs: json!({ "src": path }),
        outputs: json!({}),
        timeout_s: Some(120),
        runner: None,
    };
    let run = crate::commands::remote_exec::exec_job(settings, job).await.ok()?;
    run["stdout"].as_str()?.trim().lines().next()?.trim().parse::<f64>().ok()
}

/// A URL a remote encoder can fetch a chapter video from.
async fn remote_video_url(state: &State<'_, AppState>, project_id: &str, path: &str) -> Res<String> {
    crate::commands::remote_render::public_url_for(state, project_id, path).await
        .map_err(|err| format!("The remote encoder cannot reach a chapter video: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_is_read_from_the_middle_of_a_title() {
        assert_eq!(parse_reference("Lightkid - Genesis 3 (ambient gospel)"),
                   Some(("genesis".into(), 3)));
        assert_eq!(parse_reference("Psalms 23"), Some(("psalms".into(), 23)));
    }

    #[test]
    fn a_number_in_the_book_name_is_not_the_chapter() {
        // "1 Samuel 5" must be Samuel chapter 5, never Samuel chapter 1 — and the longest matching
        // name has to win for that to happen.
        let (book, chapter) = parse_reference("Choir - 1 Samuel 5 (choral)").expect("parsed");
        assert!(book.contains("samuel"), "got {book}");
        assert_eq!(chapter, 5);
    }

    #[test]
    fn a_year_or_volume_in_the_style_tag_is_not_a_chapter() {
        // The digits have to follow the book name directly. Anything else and a style tag decides
        // where a track lands in a two-hour upload.
        assert_eq!(parse_reference("Genesis, sung in 2026"), None);
        assert_eq!(parse_reference("Exodus (Vol. 2)"), None);
        assert_eq!(parse_reference("no reference here at all"), None);
    }

    #[test]
    fn chapter_order_is_numeric_not_alphabetical() {
        let mut refs: Vec<i64> = ["Psalms 10", "Psalms 2", "Psalms 100", "Psalms 9"].iter()
            .filter_map(|t| parse_reference(t).map(|(_, c)| c)).collect();
        refs.sort();
        assert_eq!(refs, vec![2, 9, 10, 100], "a text sort would put 10 before 2");
    }

    #[test]
    fn timestamps_accumulate_and_cross_the_hour_correctly() {
        let starts = track_starts(&[210.0, 185.5, 240.0]);
        assert_eq!(starts, vec![0.0, 210.0, 395.5]);
        assert_eq!(fmt_timestamp(starts[1]), "3:30");
        assert_eq!(fmt_timestamp(0.0), "0:00");
        assert_eq!(fmt_timestamp(3725.0), "1:02:05");
    }

    #[test]
    fn the_description_opens_at_zero_so_youtube_makes_chapters() {
        let tracks = vec![
            (1, "In the beginning".to_string(), 0.0),
            (2, "The garden".to_string(), 212.0),
        ];
        let d = description("Genesis", &tracks);
        assert!(d.lines().any(|l| l.starts_with("0:00 — Genesis 1:")), "{d}");
        assert!(d.contains("3:32 — Genesis 2:"), "{d}");
    }

    #[test]
    fn every_input_is_normalised_before_the_concat() {
        // Inputs rendered months apart differ in frame size and sample rate; the concat filter
        // rejects that outright, so each leg must be scaled, padded and resampled first.
        let f = concat_filter(3, 1920, 1080, 30);
        assert_eq!(f.matches("scale=1920:1080").count(), 3);
        assert_eq!(f.matches("aresample=44100").count(), 3);
        assert!(f.ends_with("concat=n=3:v=1:a=1[outv][outa]"), "{f}");
        assert!(f.contains("[v0][a0][v1][a1][v2][a2]concat"), "streams must pair v,a per input: {f}");
    }
}
