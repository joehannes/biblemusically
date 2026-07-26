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
// Shorts
//
// A vertical cut is not the first sixty seconds of the video rotated. The first sixty seconds are
// usually an intro — the part a scrolling viewer leaves during. So this picks the *hook*: the chorus
// if the analysis found one, otherwise the section whose mood reads as the peak, otherwise a window
// a third of the way in. Then it burns in a link card in the closing seconds, because a short that
// does not say where the full video is sends nobody there.
//
// Everything is derived from what the studio already has — sections, their moods and timings, the
// rendered video, the channel handle and the published YouTube id.
// ────────────────────────────────────────────────────────────────

/// Frame size, hard limit and *default* length per platform.
///
/// Kept here rather than guessed per call, because a wrong aspect ratio is silently re-cropped and a
/// too-long clip is rejected — and because on TikTok the default length decides whether the clip earns
/// anything at all. Checked July 2026:
///
///   • TikTok's Creator Rewards programme pays only for videos over one minute; anything shorter earns
///     nothing however well it performs. So the default is 75s, deliberately past the line — the one
///     place where the obvious "make it 60 seconds" is the expensive choice.
///   • YouTube Shorts is capped strictly under 60s for the Shorts feed, and pays from a pooled share
///     of feed ad revenue rather than per view.
///   • Facebook Reels pays per view but needs 10k followers and 600k plays in 60 days first.
///   • Snapchat Spotlight revenue-shares with a $100 payout floor.
///   • Pinterest pays nothing per view at all — its Creator Rewards programme ended and was not
///     replaced. A pin is worth cutting for traffic, never for revenue.
///
/// Returns `(width, height, max_seconds, default_seconds)`.
fn platform_spec(platform: &str) -> (i64, i64, f64, f64) {
    match platform {
        // Over a minute, on purpose: under it, Creator Rewards pays zero.
        "tiktok" => (1080, 1920, 600.0, 75.0),
        "youtube" | "shorts" => (1080, 1920, 59.0, 59.0),
        "instagram" | "reels" => (1080, 1920, 89.0, 75.0),
        "facebook" => (1080, 1920, 89.0, 75.0),
        "snapchat" | "spotlight" => (1080, 1920, 59.0, 55.0),
        "pinterest" => (1080, 1920, 59.0, 45.0),
        _ => (1080, 1920, 59.0, 59.0),
    }
}

/// What a platform actually pays, so the choice is made with the number in view.
fn payout_note(platform: &str) -> &'static str {
    match platform {
        "tiktok" => "Creator Rewards pays roughly $0.50–1.00 per 1,000 qualified views — but only for \
                     videos over one minute.",
        "youtube" | "shorts" => "Pooled share of Shorts feed ad revenue, roughly $0.01–0.10 per 1,000 \
                                 views. Needs 1,000 subscribers and 10M Shorts views in 90 days.",
        "facebook" => "Roughly $0.005–0.01 per 1,000 views, after 10,000 followers and 600,000 plays \
                       in 60 days.",
        "instagram" | "reels" => "No per-view payout; gifts and subscriptions start at 10,000 followers.",
        "snapchat" | "spotlight" => "Revenue share, $100 minimum payout, available in 45 countries.",
        "pinterest" => "Pays nothing per view — cut this one for traffic, not revenue.",
        _ => "",
    }
}

/// Escape a value for an ffmpeg filter argument.
///
/// `:` separates options and `,` separates filters, so an unescaped title with a colon in it — which
/// scripture references have constantly ("John 3:16") — silently truncates the text or breaks the
/// whole graph. Verified against ffmpeg before this shipped.
fn ff_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace(':', "\\:").replace('\'', "\\'").replace(',', "\\,")
}

/// Where the hook is, in seconds from the start of the song.
///
/// Sections carry `start`, `duration`, `name` and `mood` after analysis. A chorus is the hook by
/// definition; failing that, the brightest-mood section; failing that, one third in — never zero,
/// which is the intro nobody stays for.
fn hook_start(sections: &[Value], total: f64, window: f64) -> f64 {
    let named = |needle: &str| sections.iter().find(|s| {
        s["name"].as_str().unwrap_or("").to_ascii_lowercase().contains(needle)
    });
    let bright = || sections.iter().find(|s| {
        matches!(s["mood"].as_str().unwrap_or(""), "radiant" | "epic" | "celestial" | "warm")
    });

    let candidate = named("chorus").or_else(|| named("hook")).or_else(|| named("refrain"))
        .or_else(bright)
        .and_then(|s| s["start"].as_f64())
        .unwrap_or(total / 3.0);

    // Never run past the end: leave the whole window inside the track.
    candidate.clamp(0.0, (total - window).max(0.0))
}

#[derive(serde::Deserialize)]
pub struct ShortRequest {
    pub song_id: String,
    /// "tiktok" | "shorts" | "reels" | … — decides frame size and length.
    #[serde(default)]
    pub platform: Option<String>,
    /// Override the automatic hook detection.
    #[serde(default)]
    pub start_at: Option<f64>,
    /// Seconds, capped to the platform's limit.
    #[serde(default)]
    pub seconds: Option<f64>,
    /// Draw the "full video" card in the closing seconds. On by default — it is the entire point.
    #[serde(default)]
    pub link_card: Option<bool>,
}

/// Cut a vertical short from a song's rendered video, with a link card back to the full version.
#[tauri::command]
pub async fn build_short(state: State<'_, AppState>, payload: ShortRequest) -> Res<Value> {
    use futures_util::StreamExt;

    let song = state.db.collection::<Document>("songs")
        .find_one(doc! { "id": &payload.song_id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "song not found".to_string())?;

    let source = song["video_url"].as_str().unwrap_or("").trim_start_matches("file://").to_string();
    if source.is_empty() {
        return Err("This song has no rendered video yet — compose the video first.".into());
    }
    if !std::path::Path::new(&source).exists() {
        return Err(format!("The rendered video is missing from disk: {source}"));
    }

    let platform = payload.platform.clone().unwrap_or_else(|| "shorts".to_string());
    let (w, h, max_secs, default_secs) = platform_spec(&platform);
    let window = payload.seconds.unwrap_or(default_secs).clamp(5.0, max_secs);

    let mut sections: Vec<Value> = Vec::new();
    if let Ok(mut cursor) = state.db.collection::<Document>("sections")
        .find(doc! { "song_id": &payload.song_id }).await {
        while let Some(Ok(d)) = cursor.next().await { sections.push(bson_to_value(d)); }
    }
    sections.sort_by_key(|s| s["index"].as_i64().unwrap_or(0));
    let total = song["duration"].as_f64().filter(|d| *d > 1.0).unwrap_or(180.0);
    let start = payload.start_at.unwrap_or_else(|| hook_start(&sections, total, window));

    // What the card says: where the full video is. A published upload gives a real URL; otherwise
    // the channel handle, which still tells the viewer where to look.
    let upload = state.db.collection::<Document>("uploads")
        .find_one(doc! { "song_id": &payload.song_id, "status": "published" }).await.map_err(e)?
        .map(bson_to_value);
    let video_id = upload.as_ref().and_then(|u| u["youtube_video_id"].as_str()).unwrap_or("");
    let channel = state.db.collection::<Document>("channels")
        .find_one(doc! { "id": song["channel_id"].as_str().unwrap_or("") }).await.map_err(e)?
        .map(bson_to_value);
    let handle = channel.as_ref().and_then(|c| c["handle"].as_str()).unwrap_or("");
    let link_text = if !video_id.is_empty() {
        format!("youtu.be/{video_id}")
    } else if !handle.is_empty() {
        format!("Full video on {handle}")
    } else {
        "Full video on YouTube".to_string()
    };
    let title = song["title"].as_str().unwrap_or("").chars().take(48).collect::<String>();

    let font = find_font().ok_or(
        "No usable font found for the link card — install fonts-dejavu, or turn the card off."
    )?;

    let mut vf = format!(
        "scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h}"
    );
    if payload.link_card.unwrap_or(true) {
        // The card appears for the closing third: long enough to read, late enough not to cover the
        // hook itself.
        let card_from = (window * 0.66).max(2.0);
        vf.push_str(&format!(
            ",drawtext=fontfile={font}:text={title}:fontcolor=white@0.95:fontsize=46:\
             box=1:boxcolor=black@0.5:boxborderw=20:x=(W-text_w)/2:y=H-380:enable=gte(t\\,{card_from:.2})\
             ,drawtext=fontfile={font}:text={link}:fontcolor=white@0.9:fontsize=34:\
             box=1:boxcolor=black@0.5:boxborderw=16:x=(W-text_w)/2:y=H-300:enable=gte(t\\,{card_from:.2})",
            font = ff_escape(&font),
            title = ff_escape(&title),
            link = ff_escape(&format!("▶ {link_text}")),
        ));
    }

    let out_dir = std::path::Path::new(&source).parent()
        .map(|p| p.join("derivatives")).ok_or("could not determine an output folder")?;
    std::fs::create_dir_all(&out_dir).map_err(e)?;
    let out = out_dir.join(format!("{}-{platform}-short.mp4", payload.song_id));

    // The cut runs wherever command-line steps are configured to run. On this device that is the
    // local ffmpeg; on Modal or your own worker it is the same argv against a URL — which is what
    // lets a phone produce a short without carrying ffmpeg or moving the video twice.
    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let runner = crate::commands::remote_exec::configured_runner(&settings);

    let source_ref = if runner == "local" {
        source.clone()
    } else {
        // A remote host cannot open a path on this machine, so the source has to be reachable — the
        // same precondition remote rendering has, and worth failing loudly on rather than silently
        // falling back to local work the user asked to move off the device.
        remote_source_url(&state, &song, &source).await?
    };

    let out_name = out.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or("short.mp4".into());
    let job = crate::commands::remote_exec::CliJob {
        tool: "ffmpeg".into(),
        args: vec![
            "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(),
            "-ss".into(), format!("{start:.2}"), "-i".into(), "{in:source}".into(),
            "-t".into(), format!("{window:.2}"),
            "-vf".into(), vf.clone(),
            "-c:v".into(), "libx264".into(), "-preset".into(), "veryfast".into(), "-crf".into(), "23".into(),
            "-c:a".into(), "aac".into(), "-b:a".into(), "128k".into(),
            "-movflags".into(), "+faststart".into(), "{out:short}".into(),
        ],
        inputs: json!({ "source": source_ref }),
        outputs: json!({ "short": { "name": out_name } }),
        timeout_s: Some(900),
        runner: None,
    };
    let run = crate::commands::remote_exec::exec_job(&settings, job).await?;
    if run["status"].as_str() != Some("ok") {
        return Err(format!("the short could not be cut ({}): {}",
            run["runner"].as_str().unwrap_or("?"),
            run["stderr"].as_str().or_else(|| run["error"].as_str()).unwrap_or("no detail")));
    }
    // A local run wrote into its own temp dir; move it to where derivatives live.
    if let Some(produced) = run["files"]["short"]["local_path"].as_str() {
        if produced != out.to_string_lossy() {
            let _ = std::fs::rename(produced, &out).or_else(|_| std::fs::copy(produced, &out).map(|_| ()));
        }
    }

    // Recorded as a derivative so it shows up wherever the other per-platform versions do.
    let derivative = json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "project_id": song["project_id"].as_str().unwrap_or(""),
        "song_id": payload.song_id,
        "platform": platform,
        "kind": "short",
        "caption": "",
        "media_path": out.to_string_lossy(),
        "width": w, "height": h,
        "hook_start": start,
        "seconds": window,
        "payout": payout_note(&platform),
        "links_to": if video_id.is_empty() { Value::Null } else { json!(format!("https://www.youtube.com/watch?v={video_id}")) },
        "status": "draft",
        "created_at": crate::models::now_iso(),
    });
    if let Ok(d) = bson::to_document(&derivative) {
        state.db.collection::<Document>("derivatives").insert_one(d).await.map_err(e)?;
    }
    Ok(derivative)
}

/// A URL a remote host can fetch this song's video from.
///
/// Delegates to the render spec builder, which already knows how a project's files map onto its sync
/// remote — one place to be right about that rather than two.
async fn remote_source_url(state: &State<'_, AppState>, song: &Value, source: &str) -> Res<String> {
    crate::commands::remote_render::public_url_for(
        state, song["project_id"].as_str().unwrap_or(""), source,
    ).await.map_err(|err| format!(
        "The remote runner cannot reach this video: {err}"
    ))
}

/// The ffmpeg to run: whatever the rest of the app resolved, else the one on PATH.
fn ffmpeg_binary() -> String {
    which::which("ffmpeg").map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| "ffmpeg".into())
}

/// A font file for the link card. Checked in order of how likely it is to exist on a Linux desktop.
fn find_font() -> Option<String> {
    for p in [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/freefont/FreeSans.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
        "C:/Windows/Fonts/arial.ttf",
    ] {
        if std::path::Path::new(p).exists() { return Some(p.to_string()); }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_values_are_escaped_where_ffmpeg_would_break() {
        // A scripture reference contains a colon, which separates options inside a filter — the most
        // common way a burned-in title silently truncates.
        assert_eq!(ff_escape("John 3:16"), "John 3\\:16");
        assert_eq!(ff_escape("a,b"), "a\\,b");
        assert_eq!(ff_escape("it's"), "it\\'s");
        assert_eq!(ff_escape(r"C:\fonts\a.ttf"), "C\\:\\\\fonts\\\\a.ttf");
    }

    #[test]
    fn the_hook_is_the_chorus_when_the_analysis_found_one() {
        let sections = vec![
            json!({ "index": 0, "name": "Intro", "mood": "soft", "start": 0.0 }),
            json!({ "index": 1, "name": "Verse 1", "mood": "warm", "start": 15.0 }),
            json!({ "index": 2, "name": "Chorus", "mood": "epic", "start": 42.0 }),
        ];
        assert_eq!(hook_start(&sections, 200.0, 59.0), 42.0);
    }

    #[test]
    fn without_a_chorus_it_takes_the_brightest_section_then_a_third_in() {
        let bright = vec![
            json!({ "index": 0, "name": "Part A", "mood": "soft", "start": 0.0 }),
            json!({ "index": 1, "name": "Part B", "mood": "radiant", "start": 30.0 }),
        ];
        assert_eq!(hook_start(&bright, 200.0, 59.0), 30.0);

        let dull = vec![json!({ "index": 0, "name": "Part A", "mood": "soft", "start": 0.0 })];
        assert_eq!(hook_start(&dull, 180.0, 59.0), 60.0, "a third in, never the intro");
    }

    #[test]
    fn the_window_always_fits_inside_the_track() {
        // A chorus near the end must not produce a cut that runs past the file.
        let late = vec![json!({ "index": 0, "name": "Chorus", "mood": "epic", "start": 170.0 })];
        assert_eq!(hook_start(&late, 180.0, 59.0), 121.0);
        // A track shorter than the window starts at zero rather than going negative.
        let short_track = vec![json!({ "index": 0, "name": "Chorus", "mood": "epic", "start": 10.0 })];
        assert_eq!(hook_start(&short_track, 30.0, 59.0), 0.0);
    }

    #[test]
    fn platform_limits_are_the_platforms_own() {
        assert_eq!(platform_spec("shorts").2, 59.0, "Shorts is strictly under a minute");
        assert_eq!(platform_spec("reels").2, 89.0);
        assert_eq!(platform_spec("tiktok").0, 1080);
    }

    #[test]
    fn a_tiktok_cut_defaults_past_the_one_minute_pay_line() {
        // Creator Rewards pays nothing for a clip under a minute, so defaulting to "just under 60"
        // — the obvious number, and the one every other platform wants — earns zero on TikTok.
        let (_, _, max, default) = platform_spec("tiktok");
        assert!(default > 60.0, "a TikTok cut of {default}s earns nothing");
        assert!(default <= max);

        // Shorts is the opposite constraint: the feed rejects a minute or more.
        let (_, _, smax, sdefault) = platform_spec("shorts");
        assert!(sdefault < 60.0 && smax < 60.0, "Shorts must stay under a minute");
    }

    #[test]
    fn every_platform_says_what_it_pays_including_the_one_that_pays_nothing() {
        for p in ["tiktok", "shorts", "facebook", "instagram", "snapchat", "pinterest"] {
            assert!(!payout_note(p).is_empty(), "{p} has no payout note");
        }
        assert!(payout_note("pinterest").contains("nothing"),
                "a platform that pays nothing must say so rather than be quietly listed");
    }
}
