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
// Music-only distribution
//
// The videos go to YouTube; the songs themselves belong on Spotify and Apple Music, and getting them
// there means a distributor. Two facts shape everything in this module, and both were checked in
// July 2026 rather than assumed:
//
//   1. No self-service distributor has a public upload API. DistroKid is hiring to build one and has
//      none today; Amuse, RouteNote and CD Baby publish no developer documentation. The API platforms
//      that do exist (LabelGrid and similar) sell to labels and distributors, not to artists. So this
//      module does the part that can be automated properly — assembling a complete, correctly named,
//      metadata-complete release package — and hands the upload itself to a browser macro or to the
//      person, rather than pretending to a delivery it cannot make.
//
//   2. AI-generated music is accepted by some distributors and refused by others, and the ones that
//      accept it impose volume caps. TuneCore rejects fully AI-generated works outright; CD Baby
//      requires meaningful human authorship; DistroKid accepts AI but not "mass auto-generated
//      content"; Amuse allows ten releases per rolling seven days; LANDR allows twelve AI songs a
//      month and excludes several platforms. A studio that generates a song a day walks straight into
//      those limits, so the limits are data here, checked before a release is queued — not a support
//      email three weeks later.
//
// Spotify's DDEX-based AI credits standard (2026) requires disclosing AI involvement in vocals,
// lyrics, melody and instrumentation. Every exported package carries that disclosure, filled in from
// what the studio actually did, because it is both required and the honest answer.
// ────────────────────────────────────────────────────────────────

/// What a distributor costs, allows and caps.
///
/// `ai`: "accepts" | "limited" | "rejects" — whether fully AI-generated music may be delivered.
/// `cap_releases` / `cap_days`: the publishing rate the distributor allows, 0 for no stated cap.
struct Distributor {
    id: &'static str,
    name: &'static str,
    price: &'static str,
    royalty: &'static str,
    ai: &'static str,
    ai_note: &'static str,
    cap_releases: i64,
    cap_days: i64,
    excludes: &'static str,
    api: &'static str,
    url: &'static str,
}

/// Prices and policies checked July 2026. They move; the app shows them as *what to verify*, never as
/// a promise, and links to each distributor so the current terms are one click away.
const DISTRIBUTORS: &[Distributor] = &[
    Distributor {
        id: "routenote", name: "RouteNote",
        price: "Free tier, or $10–30 per release for the premium tier",
        royalty: "85% on the free tier; 100% on premium",
        ai: "accepts",
        ai_note: "Accepts AI music and asks for links to the tools used, so keep the engine names — \
                  this app records them per song.",
        cap_releases: 0, cap_days: 0,
        excludes: "",
        api: "none published",
        url: "https://routenote.com",
    },
    Distributor {
        id: "amuse", name: "Amuse",
        price: "Free tier, or about $24–60 a year",
        royalty: "100% on the paid tiers",
        ai: "limited",
        ai_note: "Accepts AI music with discretionary detection.",
        cap_releases: 10, cap_days: 7,
        excludes: "Meta and YouTube Content ID are excluded for AI releases",
        api: "none published",
        url: "https://amuse.io",
    },
    Distributor {
        id: "distrokid", name: "DistroKid",
        price: "About $23–25 a year for one artist, more for several",
        royalty: "100%",
        ai: "limited",
        ai_note: "Accepts AI if you hold the rights and impersonate nobody — but excludes \
                  mass auto-generated content, which a fifty-channel daily schedule can look like. \
                  Read their terms before pointing a scheduler at it.",
        cap_releases: 0, cap_days: 0,
        excludes: "",
        api: "none public (they are building one)",
        url: "https://distrokid.com",
    },
    Distributor {
        id: "symphonic", name: "Symphonic",
        price: "Subscription, quote-based",
        royalty: "Revenue share",
        ai: "accepts",
        ai_note: "Accepts both fully AI-generated and AI-assisted music, with disclosure in the \
                  upload flow.",
        cap_releases: 0, cap_days: 0,
        excludes: "",
        api: "partner/label API on request",
        url: "https://symphonic.com",
    },
    Distributor {
        id: "landr", name: "LANDR",
        price: "Subscription",
        royalty: "100%",
        ai: "limited",
        ai_note: "Accepts AI under strict limits.",
        cap_releases: 12, cap_days: 30,
        excludes: "YouTube Content ID, Meta, TikTok, Deezer and Pandora are excluded",
        api: "none published",
        url: "https://landr.com",
    },
    Distributor {
        id: "unitedmasters", name: "UnitedMasters",
        price: "Free tier, or $19.99–59.99 a year",
        royalty: "Free tier takes 10%",
        ai: "accepts",
        ai_note: "Does not explicitly restrict AI distribution.",
        cap_releases: 0, cap_days: 0,
        excludes: "",
        api: "none published",
        url: "https://unitedmasters.com",
    },
    Distributor {
        id: "tunecore", name: "TuneCore",
        price: "Per-release or subscription",
        royalty: "100%",
        ai: "rejects",
        ai_note: "Rejects works that are 100% AI-generated; AI-enhanced human writing is allowed. \
                  Not a fit for songs this studio generates end to end.",
        cap_releases: 0, cap_days: 0,
        excludes: "",
        api: "none published",
        url: "https://tunecore.com",
    },
    Distributor {
        id: "cdbaby", name: "CD Baby",
        price: "Per release",
        royalty: "100%",
        ai: "rejects",
        ai_note: "Rejects fully AI-generated music; accepts AI-assisted work with meaningful human \
                  authorship.",
        cap_releases: 0, cap_days: 0,
        excludes: "",
        api: "none published",
        url: "https://cdbaby.com",
    },
];

fn distributor(id: &str) -> Option<&'static Distributor> {
    DISTRIBUTORS.iter().find(|d| d.id == id)
}

fn distributor_json(d: &Distributor) -> Value {
    json!({
        "id": d.id, "name": d.name, "price": d.price, "royalty": d.royalty,
        "ai": d.ai, "ai_note": d.ai_note,
        "cap_releases": d.cap_releases, "cap_days": d.cap_days,
        "excludes": d.excludes, "api": d.api, "url": d.url,
        "recommended": d.ai == "accepts" && d.cap_releases == 0,
    })
}

/// Every distributor, with the AI-generated ones first — because a studio whose songs are generated
/// has no use for a list that puts a distributor which refuses them at the top.
#[tauri::command]
pub async fn list_distributors() -> Res<Value> {
    let mut list: Vec<&Distributor> = DISTRIBUTORS.iter().collect();
    let rank = |d: &Distributor| match d.ai { "accepts" => 0, "limited" => 1, _ => 2 };
    list.sort_by_key(|d| (rank(d), d.cap_releases != 0));
    Ok(json!({
        "distributors": list.into_iter().map(distributor_json).collect::<Vec<_>>(),
        "no_api_anywhere": "No self-service distributor publishes an upload API. Export the release \
                            package here, then either upload it yourself or let the Macro Manager \
                            drive their web uploader.",
        "checked": "July 2026",
    }))
}

// ── Release pacing ──────────────────────────────────────────────────────────

/// How many releases a distributor's window still has room for.
///
/// `recent` is the release dates already sent to this distributor, ISO-8601. A cap of 0 means the
/// distributor states none — which is not the same as unlimited, so the caller still shows `ai_note`.
pub fn pacing(cap_releases: i64, cap_days: i64, recent: &[String], now: &str) -> Value {
    if cap_releases <= 0 || cap_days <= 0 {
        return json!({ "capped": false, "used": 0, "cap": 0, "days": 0, "room": true });
    }
    // Dates are ISO-8601, so a lexical comparison against the cutoff is a date comparison.
    let cutoff = iso_days_ago(now, cap_days);
    let used = recent.iter().filter(|d| d.as_str() >= cutoff.as_str()).count() as i64;
    json!({
        "capped": true,
        "used": used,
        "cap": cap_releases,
        "days": cap_days,
        "room": used < cap_releases,
        "cutoff": cutoff,
    })
}

/// `now` minus `days`, as an ISO-8601 date-time string.
fn iso_days_ago(now: &str, days: i64) -> String {
    let parsed = chrono::DateTime::parse_from_rfc3339(now)
        .map(|d| d.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now());
    (parsed - chrono::Duration::days(days)).to_rfc3339()
}

// ── Metadata ────────────────────────────────────────────────────────────────

/// One CSV field, quoted the way every spreadsheet and delivery tool expects.
fn csv_field(s: &str) -> String {
    let needs = s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r');
    let body = s.replace('"', "\"\"");
    if needs { format!("\"{body}\"") } else { body }
}

/// The release's track metadata as CSV.
///
/// Column names follow the fields every distributor's uploader asks for, in the order they ask for
/// them, so the file can be read straight down a web form — or pasted into the bulk-upload sheet the
/// paid tiers offer. Scripture titles contain colons and commas constantly ("John 3:16, sung"), which
/// is exactly what breaks a naive CSV.
pub fn metadata_csv(release: &Value) -> String {
    let cols = ["track", "title", "artist", "album", "isrc", "language", "genre",
                "explicit", "writer", "producer", "ai_generated", "duration_seconds", "file"];
    let mut out = cols.join(",");
    out.push('\n');
    let album = release["title"].as_str().unwrap_or("");
    let artist = release["artist"].as_str().unwrap_or("");
    for (i, t) in release["tracks"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        let row = [
            (i + 1).to_string(),
            t["title"].as_str().unwrap_or("").to_string(),
            t["artist"].as_str().unwrap_or(artist).to_string(),
            album.to_string(),
            normalise_isrc(t["isrc"].as_str().unwrap_or("")),
            t["language"].as_str().unwrap_or("en").to_string(),
            release["genre"].as_str().unwrap_or("Gospel").to_string(),
            if t["explicit"].as_bool() == Some(true) { "yes" } else { "no" }.to_string(),
            t["writer"].as_str().unwrap_or(artist).to_string(),
            t["producer"].as_str().unwrap_or(artist).to_string(),
            "yes".to_string(),                        // this studio generates the music; see AI-CREDITS
            format!("{:.0}", t["duration"].as_f64().unwrap_or(0.0)),
            t["file"].as_str().unwrap_or("").to_string(),
        ];
        out.push_str(&row.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}

/// An ISRC with the punctuation people type stripped out.
///
/// The canonical form is twelve characters, no dashes. Returned empty when it cannot be one, because
/// a malformed ISRC is rejected at delivery — better blank, which the distributor then assigns.
pub fn normalise_isrc(raw: &str) -> String {
    let cleaned: String = raw.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_uppercase();
    if cleaned.len() == 12 { cleaned } else { String::new() }
}

/// The AI disclosure Spotify's 2026 DDEX credits standard asks for.
///
/// Filled in from what the studio actually did rather than from a checkbox: the music engine, the
/// lyrics model, and whether a human edited either. Ticking "no AI" on a generated release is the one
/// thing here that could get a whole catalogue removed.
pub fn ai_credits_block(engines: &[String], lyrics_model: &str, human_edited: bool) -> String {
    let mut out = String::from("AI CREDITS (DDEX AI-credits standard, required by Spotify since 2026)\n\n");
    out.push_str(&format!("Vocals:        AI-generated ({})\n",
        if engines.is_empty() { "engine not recorded" } else { &engines[0] }));
    out.push_str(&format!("Instrumentation: AI-generated ({})\n",
        if engines.is_empty() { "engine not recorded" } else { &engines[0] }));
    out.push_str(&format!("Melody:        AI-generated ({})\n",
        if engines.is_empty() { "engine not recorded" } else { &engines[0] }));
    out.push_str(&format!("Lyrics:        {}\n", if lyrics_model.is_empty() {
        "AI-generated".to_string()
    } else {
        format!("AI-generated ({lyrics_model})")
    }));
    out.push_str(&format!("Human editing: {}\n", if human_edited {
        "yes — lyrics and/or arrangement were edited by a person"
    } else {
        "none recorded"
    }));
    if engines.len() > 1 {
        out.push_str(&format!("\nEngines used across this release: {}\n", engines.join(", ")));
    }
    out.push_str("\nSource text is scripture in the public domain. Declare this at upload; some \
                  distributors ask for links to the tools, which are the engine names above.\n");
    out
}

// ── Artist profiles (a channel, as an artist) ───────────────────────────────

#[derive(serde::Deserialize)]
pub struct ArtistProfile {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub bio: Option<String>,
    #[serde(default)]
    pub artwork_path: Option<String>,
    #[serde(default)]
    pub spotify_url: Option<String>,
    #[serde(default)]
    pub apple_url: Option<String>,
    #[serde(default)]
    pub distributor: Option<String>,
}

/// Save the artist a channel releases under. One channel is one artist — that is the whole point of
/// treating channels as artists, and it keeps the store links attached to the thing that earns them.
#[tauri::command]
pub async fn save_artist_profile(state: State<'_, AppState>, payload: ArtistProfile) -> Res<Value> {
    let id = payload.id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let doc_out = json!({
        "id": id,
        "name": payload.name,
        "channel_id": payload.channel_id.unwrap_or_default(),
        "project_id": payload.project_id.unwrap_or_default(),
        "bio": payload.bio.unwrap_or_default(),
        "artwork_path": payload.artwork_path.unwrap_or_default(),
        "spotify_url": payload.spotify_url.unwrap_or_default(),
        "apple_url": payload.apple_url.unwrap_or_default(),
        "distributor": payload.distributor.unwrap_or_default(),
        "updated_at": crate::models::now_iso(),
    });
    let d = bson::to_document(&doc_out).map_err(e)?;
    state.db.collection::<Document>("artists")
        .update_one(doc! { "id": doc_out["id"].as_str().unwrap_or("") }, doc! { "$set": d })
        .upsert(true)
        .await.map_err(e)?;
    Ok(doc_out)
}

#[tauri::command]
pub async fn list_artist_profiles(state: State<'_, AppState>) -> Res<Value> {
    use futures_util::StreamExt;
    let mut out = Vec::new();
    let mut cursor = state.db.collection::<Document>("artists").find(doc! {}).await.map_err(e)?;
    while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }
    Ok(json!({ "artists": out }))
}

// ── Releases ────────────────────────────────────────────────────────────────

/// Propose the releases a project's finished songs make up.
///
/// A chapter with audio is a single; a book whose chapters are all present is an album. Proposals
/// only — nothing is created until the user says so, because a release date is a commitment and a
/// duplicate delivery is a mess to unwind.
#[tauri::command]
pub async fn plan_releases(state: State<'_, AppState>, project_id: String) -> Res<Value> {
    use futures_util::StreamExt;

    let mut songs: Vec<Value> = Vec::new();
    let mut cursor = state.db.collection::<Document>("songs")
        .find(doc! { "project_id": &project_id }).await.map_err(e)?;
    while let Some(Ok(d)) = cursor.next().await { songs.push(bson_to_value(d)); }

    let mut existing: Vec<Value> = Vec::new();
    let mut rc = state.db.collection::<Document>("releases")
        .find(doc! { "project_id": &project_id }).await.map_err(e)?;
    while let Some(Ok(d)) = rc.next().await { existing.push(bson_to_value(d)); }
    let already = |song_id: &str| existing.iter().any(|r| {
        r["tracks"].as_array().map_or(false, |ts| ts.iter().any(|t| t["song_id"].as_str() == Some(song_id)))
    });

    let mut singles = Vec::new();
    let mut by_book: std::collections::BTreeMap<String, Vec<(i64, Value)>> = Default::default();
    for s in &songs {
        let audio = audio_path(s);
        if audio.is_empty() { continue; }
        if let Some((book, chapter)) = crate::commands::compilation::parse_reference(s["title"].as_str().unwrap_or("")) {
            by_book.entry(book).or_default().push((chapter, s.clone()));
        }
        if !already(s["id"].as_str().unwrap_or("")) {
            singles.push(json!({
                "kind": "single",
                "song_id": s["id"],
                "title": s["title"],
                "duration": s["duration"],
                "language": s["language"],
                "audio": audio,
            }));
        }
    }

    let books_meta = crate::commands::bible::CANON;
    let mut albums = Vec::new();
    for (book, mut entries) in by_book {
        entries.sort_by_key(|(c, _)| *c);
        entries.dedup_by_key(|(c, _)| *c);
        let meta = books_meta.iter().find(|(id, _, _)| *id == book);
        let (name, total) = meta.map(|(_, n, c)| (*n, *c)).unwrap_or((book.as_str(), 0));
        let complete = total > 0 && entries.len() as i64 == total;
        albums.push(json!({
            "kind": "album",
            "book": book,
            "title": format!("{name} — the complete book"),
            "chapters": entries.len(),
            "chapters_total": total,
            "complete": complete,
            "seconds": entries.iter().map(|(_, s)| s["duration"].as_f64().unwrap_or(0.0)).sum::<f64>(),
            "tracks": entries.iter().map(|(c, s)| json!({
                "chapter": c, "song_id": s["id"], "title": s["title"],
                "duration": s["duration"], "language": s["language"], "audio": audio_path(s),
            })).collect::<Vec<_>>(),
        }));
    }

    Ok(json!({ "singles": singles, "albums": albums, "existing": existing.len() }))
}

fn audio_path(song: &Value) -> String {
    for key in ["local_audio_path", "local_audio_path_alt", "audio_url"] {
        let v = song[key].as_str().unwrap_or("").trim_start_matches("file://");
        if !v.is_empty() { return v.to_string(); }
    }
    String::new()
}

#[derive(serde::Deserialize)]
pub struct SaveRelease {
    #[serde(default)]
    pub id: Option<String>,
    pub project_id: String,
    /// "single" | "album"
    pub kind: String,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub book: Option<String>,
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub distributor: Option<String>,
    #[serde(default)]
    pub cover_path: Option<String>,
    #[serde(default)]
    pub upc: Option<String>,
    /// `[{ song_id, title, duration, language, isrc?, audio }]`
    #[serde(default)]
    pub tracks: Vec<Value>,
    #[serde(default)]
    pub human_edited: Option<bool>,
}

#[tauri::command]
pub async fn save_release(state: State<'_, AppState>, payload: SaveRelease) -> Res<Value> {
    if payload.tracks.is_empty() { return Err("a release needs at least one track".into()); }
    let id = payload.id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let doc_out = json!({
        "id": id,
        "project_id": payload.project_id,
        "kind": payload.kind,
        "title": payload.title,
        "artist": payload.artist,
        "book": payload.book.unwrap_or_default(),
        "genre": payload.genre.unwrap_or_else(|| "Gospel".into()),
        "release_date": payload.release_date.unwrap_or_default(),
        "distributor": payload.distributor.unwrap_or_default(),
        "cover_path": payload.cover_path.unwrap_or_default(),
        "upc": payload.upc.unwrap_or_default(),
        "tracks": payload.tracks,
        "human_edited": payload.human_edited.unwrap_or(false),
        "status": "draft",
        "updated_at": crate::models::now_iso(),
    });
    let d = bson::to_document(&doc_out).map_err(e)?;
    state.db.collection::<Document>("releases")
        .update_one(doc! { "id": doc_out["id"].as_str().unwrap_or("") }, doc! { "$set": d })
        .upsert(true)
        .await.map_err(e)?;
    Ok(doc_out)
}

#[tauri::command]
pub async fn list_releases(state: State<'_, AppState>, project_id: String) -> Res<Value> {
    use futures_util::StreamExt;
    let mut out = Vec::new();
    let mut cursor = state.db.collection::<Document>("releases")
        .find(doc! { "project_id": &project_id }).await.map_err(e)?;
    while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }

    // Pacing is per distributor, and it is the number that decides whether today's release can go
    // out at all — so it comes back with the list rather than needing a second call.
    let now = crate::models::now_iso();
    let mut caps = serde_json::Map::new();
    for d in DISTRIBUTORS {
        let dates: Vec<String> = out.iter()
            .filter(|r| r["distributor"].as_str() == Some(d.id))
            .filter_map(|r| r["released_at"].as_str().or_else(|| r["release_date"].as_str()))
            .map(|s| s.to_string())
            .collect();
        caps.insert(d.id.to_string(), pacing(d.cap_releases, d.cap_days, &dates, &now));
    }
    Ok(json!({ "releases": out, "pacing": Value::Object(caps) }))
}

#[tauri::command]
pub async fn delete_release(state: State<'_, AppState>, id: String) -> Res<Value> {
    state.db.collection::<Document>("releases")
        .delete_one(doc! { "id": &id }).await.map_err(e)?;
    Ok(json!({ "deleted": id }))
}

// ── Export ──────────────────────────────────────────────────────────────────

/// Write the release out as a folder a distributor's uploader will accept.
///
/// Everything a delivery needs, named the way delivery expects: numbered audio files, the cover, the
/// track metadata as CSV, the AI credits, and the specific steps for the chosen distributor including
/// its caps. This is the part that can be got right without an API — and getting it right is most of
/// the work, since a rejected release comes back days later with one line of explanation.
#[tauri::command]
pub async fn export_release_package(state: State<'_, AppState>, id: String) -> Res<Value> {
    super::subscription::require(&state, "export").await?;
    let release = state.db.collection::<Document>("releases")
        .find_one(doc! { "id": &id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "release not found".to_string())?;

    let tracks = release["tracks"].as_array().cloned().unwrap_or_default();
    if tracks.is_empty() { return Err("this release has no tracks".into()); }

    let base = release["tracks"][0]["audio"].as_str().unwrap_or("");
    let root = std::path::Path::new(base).parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("releases"))
        .ok_or("could not work out where to write the package")?;
    let folder = root.join(slugify(release["title"].as_str().unwrap_or("release")));
    std::fs::create_dir_all(&folder).map_err(e)?;

    // Numbered copies, so track order survives a folder listing sorted by name.
    let mut copied = Vec::new();
    let mut listed = Vec::new();
    let mut missing = Vec::new();
    for (i, t) in tracks.iter().enumerate() {
        let src = t["audio"].as_str().unwrap_or("").trim_start_matches("file://");
        if src.is_empty() || !std::path::Path::new(src).exists() {
            missing.push(t["title"].as_str().unwrap_or("?").to_string());
            continue;
        }
        let ext = std::path::Path::new(src).extension()
            .map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "mp3".into());
        let name = format!("{:02} - {}.{ext}", i + 1, safe_filename(t["title"].as_str().unwrap_or("track")));
        let dest = folder.join(&name);
        std::fs::copy(src, &dest).map_err(e)?;
        copied.push(dest.to_string_lossy().to_string());
        let mut row = t.clone();
        row["file"] = json!(name);
        listed.push(row);
    }
    if copied.is_empty() {
        return Err("none of this release's audio files are on disk".into());
    }

    let mut with_files = release.clone();
    with_files["tracks"] = Value::Array(listed);
    std::fs::write(folder.join("metadata.csv"), metadata_csv(&with_files)).map_err(e)?;
    std::fs::write(folder.join("release.json"),
        serde_json::to_string_pretty(&with_files).unwrap_or_default()).map_err(e)?;

    // Engines actually used, read off the songs rather than assumed.
    let mut engines: Vec<String> = Vec::new();
    for t in tracks.iter() {
        if let Some(song_id) = t["song_id"].as_str() {
            if let Ok(Some(d)) = state.db.collection::<Document>("songs")
                .find_one(doc! { "id": song_id }).await {
                let s = bson_to_value(d);
                for key in ["music_engine", "engine"] {
                    if let Some(v) = s[key].as_str().filter(|v| !v.is_empty()) {
                        if !engines.iter().any(|x| x == v) { engines.push(v.to_string()); }
                    }
                }
            }
        }
    }
    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let lyrics_model = settings["openrouter_model"].as_str()
        .or_else(|| settings["gemini_model"].as_str()).unwrap_or("").to_string();
    std::fs::write(folder.join("AI-CREDITS.txt"),
        ai_credits_block(&engines, &lyrics_model, release["human_edited"].as_bool() == Some(true)))
        .map_err(e)?;

    // The cover, if there is one. Stores want a square of at least 3000×3000 — noted rather than
    // silently upscaled, because an upscaled cover is rejected too.
    let mut cover = String::new();
    let cover_src = release["cover_path"].as_str().unwrap_or("").trim_start_matches("file://");
    if !cover_src.is_empty() && std::path::Path::new(cover_src).exists() {
        let ext = std::path::Path::new(cover_src).extension()
            .map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "jpg".into());
        let dest = folder.join(format!("cover.{ext}"));
        std::fs::copy(cover_src, &dest).map_err(e)?;
        cover = dest.to_string_lossy().to_string();
    }

    let dist = release["distributor"].as_str().unwrap_or("");
    std::fs::write(folder.join("UPLOAD-STEPS.txt"), upload_steps(dist, &release)).map_err(e)?;

    Ok(json!({
        "release_id": id,
        "folder": folder.to_string_lossy(),
        "audio_files": copied.len(),
        "cover": cover,
        "missing_audio": missing,
        "files": ["metadata.csv", "release.json", "AI-CREDITS.txt", "UPLOAD-STEPS.txt"],
        "note": if cover.is_empty() {
            "No cover art attached — every store requires one, square and at least 3000×3000."
        } else { "" },
    }))
}

/// The steps for this distributor, with its caps and exclusions spelled out.
fn upload_steps(dist_id: &str, release: &Value) -> String {
    let title = release["title"].as_str().unwrap_or("");
    let artist = release["artist"].as_str().unwrap_or("");
    let mut out = format!("UPLOADING: {title}\nARTIST: {artist}\n\n");
    match distributor(dist_id) {
        Some(d) => {
            out.push_str(&format!("DISTRIBUTOR: {} ({})\nPRICE: {}\nROYALTY: {}\n\n", d.name, d.url, d.price, d.royalty));
            out.push_str(&format!("AI POLICY: {}\n{}\n\n", d.ai, d.ai_note));
            if d.ai == "rejects" {
                out.push_str("⚠ THIS DISTRIBUTOR REFUSES FULLY AI-GENERATED MUSIC. Delivering this \
                              release risks the whole account, not just the release. Pick another.\n\n");
            }
            if d.cap_releases > 0 {
                out.push_str(&format!("RATE LIMIT: {} releases per {} days.\n", d.cap_releases, d.cap_days));
            }
            if !d.excludes.is_empty() {
                out.push_str(&format!("EXCLUDED PLATFORMS: {}\n", d.excludes));
            }
            out.push_str("\nAPI: none published, so this is a manual or macro-driven upload.\n");
        }
        None => out.push_str("DISTRIBUTOR: not chosen yet — see the Distribution tab for the comparison.\n\n"),
    }
    out.push_str("\nSTEPS\n\
        1. Sign in and start a new release.\n\
        2. Take the title, artist, genre and date from metadata.csv (or release.json).\n\
        3. Upload the numbered audio files in order.\n\
        4. Upload cover.jpg — square, 3000×3000 or larger.\n\
        5. Declare the AI contributions using AI-CREDITS.txt. Do not skip this: Spotify's DDEX AI \
           credits standard requires it, and a false declaration is a catalogue-level problem.\n\
        6. Leave ISRC blank unless you own one — the distributor assigns it.\n\
        7. Submit, then paste the store links back into the release here.\n\n\
        To do this repeatedly, record it once with the Macro Manager (Web Browser → record) and the \
        macro replays it for the next release.\n");
    out
}

fn slugify(s: &str) -> String {
    s.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-")
}

/// A filename that survives every filesystem a release folder might be copied onto.
fn safe_filename(s: &str) -> String {
    s.chars().map(|c| match c {
        '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
        c => c,
    }).collect::<String>().trim().chars().take(80).collect()
}

// ── Cover art ───────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct CoverRequest {
    pub release_id: String,
    /// Overrides the prompt derived from the release itself.
    #[serde(default)]
    pub prompt: Option<String>,
}

/// Queue cover art for a release, square, through the image pipeline the rest of the app uses.
///
/// Store covers are 1:1 and want no text — a title burnt into the artwork is a common rejection, and
/// the stores draw the title themselves.
#[tauri::command]
pub async fn generate_release_cover(
    state: State<'_, AppState>,
    state_arc: State<'_, std::sync::Arc<AppState>>,
    payload: CoverRequest,
) -> Res<Value> {
    let release = state.db.collection::<Document>("releases")
        .find_one(doc! { "id": &payload.release_id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "release not found".to_string())?;

    let first_song = release["tracks"][0]["song_id"].as_str().unwrap_or("").to_string();
    if first_song.is_empty() {
        return Err("this release has no track to hang the cover generation on".into());
    }
    let prompt = payload.prompt.clone().unwrap_or_else(|| format!(
        "Album cover art, square 1:1, no text and no lettering anywhere. \
         Subject: {}. Mood: reverent, cinematic, timeless. \
         Composition centred and readable at thumbnail size.",
        release["title"].as_str().unwrap_or("scripture set to music"),
    ));

    let section = json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "song_id": first_song,
        "index": 950,                                  // clear of the song's own sections
        "name": "release cover",
        "text": release["title"],
        "image_prompt": prompt,
        "mood": "cover",
        "release_id": payload.release_id,
        "aspect": "1:1",
        "created_at": crate::models::now_iso(),
    });
    let section_id = section["id"].as_str().unwrap_or("").to_string();
    let d = bson::to_document(&section).map_err(e)?;
    state.db.collection::<Document>("sections").insert_one(d).await.map_err(e)?;
    crate::jobs::enqueue("image", &section_id, &state_arc).await.map_err(e)?;

    Ok(json!({ "queued": 1, "section_id": section_id, "prompt": prompt }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_distributor_that_refuses_ai_music_is_never_recommended() {
        for d in DISTRIBUTORS {
            let j = distributor_json(d);
            if d.ai == "rejects" {
                assert_eq!(j["recommended"], json!(false), "{} must not be recommended", d.id);
            }
        }
        // And the ones that refuse must still be listed, with the reason — silently hiding them is
        // how someone finds out by having a release rejected.
        assert!(DISTRIBUTORS.iter().any(|d| d.ai == "rejects" && !d.ai_note.is_empty()));
    }

    #[test]
    fn pacing_counts_only_releases_inside_the_window() {
        // Amuse allows ten per rolling seven days. Two of these are older than that.
        let recent = vec![
            "2026-07-24T10:00:00+00:00".to_string(),
            "2026-07-20T10:00:00+00:00".to_string(),
            "2026-07-01T10:00:00+00:00".to_string(),
            "2026-06-02T10:00:00+00:00".to_string(),
        ];
        let p = pacing(10, 7, &recent, "2026-07-25T10:00:00+00:00");
        assert_eq!(p["used"], json!(2), "only the two inside seven days count");
        assert_eq!(p["room"], json!(true));

        let full: Vec<String> = (0..10).map(|_| "2026-07-24T10:00:00+00:00".to_string()).collect();
        assert_eq!(pacing(10, 7, &full, "2026-07-25T10:00:00+00:00")["room"], json!(false));
    }

    #[test]
    fn an_uncapped_distributor_reports_no_cap_rather_than_infinity() {
        let p = pacing(0, 0, &["2026-07-24T10:00:00+00:00".to_string()], "2026-07-25T10:00:00+00:00");
        assert_eq!(p["capped"], json!(false));
        assert_eq!(p["room"], json!(true));
    }

    #[test]
    fn csv_survives_the_punctuation_scripture_titles_actually_contain() {
        let release = json!({
            "title": "John — the complete book",
            "artist": "Lightkid",
            "genre": "Gospel",
            "tracks": [
                { "title": "John 3:16, and the world", "duration": 214.4, "language": "en" },
                { "title": "He said \"come\"", "duration": 190.0, "language": "de", "isrc": "us-abc-26-00001" },
            ],
        });
        let csv = metadata_csv(&release);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 3, "header plus two tracks");
        assert!(lines[1].contains("\"John 3:16, and the world\""), "comma in a title must be quoted: {}", lines[1]);
        assert!(lines[2].contains("\"He said \"\"come\"\"\""), "quotes must be doubled: {}", lines[2]);
        // Every row keeps the header's field count, which is what a delivery tool checks first.
        let cols = lines[0].matches(',').count();
        for l in &lines[1..] {
            let mut in_q = false;
            let n = l.chars().filter(|c| { if *c == '"' { in_q = !in_q; } *c == ',' && !in_q }).count();
            assert_eq!(n, cols, "row has the wrong number of fields: {l}");
        }
    }

    #[test]
    fn an_isrc_is_normalised_or_dropped_rather_than_passed_through_broken() {
        assert_eq!(normalise_isrc("us-abc-26-00001"), "USABC2600001".to_string());
        assert_eq!(normalise_isrc("USABC2600001"), "USABC2600001".to_string());
        assert_eq!(normalise_isrc("nope"), "", "a malformed ISRC is rejected at delivery");
        assert_eq!(normalise_isrc(""), "");
    }

    #[test]
    fn the_ai_disclosure_never_claims_a_human_wrote_it() {
        let block = ai_credits_block(&["suno".to_string(), "ace-step".to_string()], "gemini-3.6-flash", false);
        assert!(block.contains("Vocals:        AI-generated (suno)"), "{block}");
        assert!(block.contains("gemini-3.6-flash"), "{block}");
        assert!(block.contains("none recorded"), "unedited must say so: {block}");
        assert!(block.contains("suno, ace-step"), "every engine used is listed: {block}");
        assert!(block.to_lowercase().contains("ddex"));
    }

    #[test]
    fn upload_steps_warn_loudly_when_the_distributor_refuses_ai() {
        let release = json!({ "title": "Psalms 1", "artist": "Lightkid" });
        let steps = upload_steps("tunecore", &release);
        assert!(steps.contains("REFUSES FULLY AI-GENERATED"), "{steps}");
        let ok = upload_steps("routenote", &release);
        assert!(!ok.contains("REFUSES"), "{ok}");
        assert!(ok.contains("AI-CREDITS.txt"), "the disclosure step is never dropped");
    }

    #[test]
    fn a_filename_cannot_escape_the_release_folder() {
        assert_eq!(safe_filename("Psalms 23: the shepherd"), "Psalms 23- the shepherd");
        assert_eq!(safe_filename("../../etc/passwd"), "..-..-etc-passwd",
                   "the separators are what make a path; the dots alone cannot traverse");
        assert!(!safe_filename("a/b\\c:d*e?f\"g<h>i|j").contains('/'));
    }
}
