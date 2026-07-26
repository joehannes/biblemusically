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
// Recording a posting macro once, replaying it per song
//
// The Macro Manager could already record and replay a browser flow. What made it awkward for publishing
// was that a recorded flow contains the *values* from the day it was recorded: yesterday's caption,
// yesterday's link. Replaying it posts yesterday's song again.
//
// The fix is not a smarter recorder — it is preparing the values first. Before recording starts, this
// puts today's caption, hashtags and link into the clipboard **queue**, in the order the form asks for
// them. The user then records themselves pasting: field, paste, field, paste. Each of those becomes a
// `paste-queue` step, which takes whatever is queued *at replay time*.
//
// So the same recorded macro posts a different song tomorrow, with no editing, because the macro
// contains "paste the next prepared value" rather than the value itself.
//
// A recipe below is not a script — it cannot be, since these sites change their forms without notice.
// It is the URL, the values worth preparing, and the checklist of what to do while recording. The
// recording is still the user's; the app just makes sure they never have to alt-tab for a caption.
// ────────────────────────────────────────────────────────────────

/// What to prepare and what to expect, per platform.
struct Recipe {
    platform: &'static str,
    label: &'static str,
    /// Where the posting form lives.
    url: &'static str,
    /// Which prepared values this platform's form wants, in form order.
    fields: &'static [&'static str],
    /// What the person does while recording. Shown as a checklist beside the browser.
    checklist: &'static [&'static str],
    /// Why a macro rather than an API.
    why: &'static str,
}

const RECIPES: &[Recipe] = &[
    Recipe {
        platform: "instagram",
        label: "Instagram",
        url: "https://www.instagram.com/",
        fields: &["caption", "hashtags"],
        checklist: &[
            "Click the + (new post) and choose the video file the app wrote.",
            "In the caption box: paste (the caption is already on the clipboard).",
            "Paste again for the hashtags — the queue advances by itself.",
            "Click Share, and stop recording once the post appears.",
        ],
        why: "The API can publish to your own account without App Review — see Access & Permissions. A \
              macro is the route while that is not set up, or for accounts that are not yours.",
    },
    Recipe {
        platform: "tiktok",
        label: "TikTok",
        url: "https://www.tiktok.com/tiktokstudio/upload",
        fields: &["caption", "hashtags"],
        checklist: &[
            "Drop in the short the app cut (Publicity → the short's file path).",
            "Paste the caption, then paste again for the hashtags.",
            "Set the visibility you want, then Post.",
        ],
        why: "`video.upload` needs no audit and puts the video in your drafts — that is the better route. \
              A macro covers the case where you would rather not connect an app at all.",
    },
    Recipe {
        platform: "reddit",
        label: "Reddit",
        url: "https://www.reddit.com/submit",
        fields: &["title", "body", "link"],
        checklist: &[
            "Choose the subreddit. Record this — the macro will post to the same one.",
            "Paste the title, then the body, then the link.",
            "Post, and stop recording.",
        ],
        why: "Reddit's API works and is already wired. Record this only if you post to a subreddit whose \
              rules need a flair or a form the API path does not cover.",
    },
    Recipe {
        platform: "pinterest",
        label: "Pinterest",
        url: "https://www.pinterest.com/pin-builder/",
        fields: &["title", "body", "link"],
        checklist: &[
            "Upload the cover image.",
            "Paste the title, the description, then the destination link.",
            "Publish.",
        ],
        why: "Pins are worth making for traffic — Pinterest pays nothing per view.",
    },
    Recipe {
        platform: "x",
        label: "X",
        url: "https://x.com/compose/post",
        fields: &["body", "link"],
        checklist: &[
            "Paste the post text, then the link.",
            "Attach the short if you want video.",
            "Post.",
        ],
        why: "The API tier that posts costs money. A macro does not.",
    },
    Recipe {
        platform: "distrokid",
        label: "A music distributor",
        url: "",
        fields: &["title", "artist", "genre"],
        checklist: &[
            "Start a new release and upload the audio files from the exported package, in order.",
            "Paste the title, then the artist, then the genre.",
            "Upload cover.jpg from the same folder.",
            "Declare the AI contributions from AI-CREDITS.txt — do not skip this.",
            "Submit.",
        ],
        why: "No self-service distributor publishes an upload API (checked July 2026), so this is the \
              only way to automate the delivery at all. Export the release package first.",
    },
];

fn recipe(platform: &str) -> Option<&'static Recipe> {
    RECIPES.iter().find(|r| r.platform == platform)
}

/// Every posting recipe, with whether a macro has already been recorded for it.
#[tauri::command]
pub async fn list_post_recipes(state: State<'_, AppState>) -> Res<Value> {
    use futures_util::StreamExt;
    let mut linked: std::collections::HashMap<String, Value> = Default::default();
    if let Ok(mut cursor) = state.db.collection::<Document>("post_macros").find(doc! {}).await {
        while let Some(Ok(d)) = cursor.next().await {
            let m = bson_to_value(d);
            if let Some(p) = m["platform"].as_str() { linked.insert(p.to_string(), m); }
        }
    }
    Ok(json!({
        "recipes": RECIPES.iter().map(|r| json!({
            "platform": r.platform, "label": r.label, "url": r.url,
            "fields": r.fields, "checklist": r.checklist, "why": r.why,
            "macro": linked.get(r.platform).cloned().unwrap_or(Value::Null),
        })).collect::<Vec<_>>(),
    }))
}

/// Build the values a post needs, from what the studio already knows about the song.
///
/// Order matters: it is the order the form asks for them, so a recorded sequence of pastes lines up
/// with the queue on every later replay.
pub fn post_values(fields: &[&str], song: &Value, piece: Option<&Value>, link: &str) -> Vec<String> {
    let title = song["title"].as_str().unwrap_or("").to_string();
    let body = piece
        .and_then(|p| p["body"].as_str())
        .or_else(|| song["annotations"].as_str())
        .unwrap_or("")
        .chars().take(2000).collect::<String>();
    let caption = piece
        .and_then(|p| p["summary"].as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| title.clone());
    let tags = piece
        .and_then(|p| p["hashtags"].as_array())
        .map(|a| a.iter().filter_map(|t| t.as_str()).collect::<Vec<_>>().join(" "))
        .unwrap_or_default();

    fields.iter().map(|f| match *f {
        "title" => title.clone(),
        "caption" => caption.clone(),
        "body" => body.clone(),
        "hashtags" => tags.clone(),
        "link" => link.to_string(),
        "artist" => song["artist"].as_str().unwrap_or("").to_string(),
        "genre" => "Gospel".to_string(),
        _ => String::new(),
    })
    // A blank value would silently shift every later paste by one, which is the failure that is
    // hardest to see: the caption ends up in the hashtag field and the post still goes out.
    .map(|v| if v.trim().is_empty() { "—".to_string() } else { v })
    .collect()
}

#[derive(serde::Deserialize)]
pub struct PrepareRequest {
    pub platform: String,
    pub song_id: String,
    /// A publicity piece to take the wording from. Without it, the song's own text is used.
    #[serde(default)]
    pub piece_id: Option<String>,
}

/// Prepare everything a recording (or a replay) needs, and load the clipboard queue.
///
/// Called both before recording and before replaying — that symmetry is the whole design: the recorded
/// steps say "paste the next prepared value", so what changes between runs is the queue, not the macro.
#[tauri::command]
pub async fn prepare_post_macro(state: State<'_, AppState>, payload: PrepareRequest) -> Res<Value> {
    let r = recipe(&payload.platform)
        .ok_or_else(|| format!("no posting recipe for '{}'", payload.platform))?;

    let song = state.db.collection::<Document>("songs")
        .find_one(doc! { "id": &payload.song_id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "song not found".to_string())?;

    let piece = match payload.piece_id.as_deref() {
        Some(id) if !id.is_empty() => state.db.collection::<Document>("publicity_pieces")
            .find_one(doc! { "id": id }).await.map_err(e)?.map(bson_to_value),
        _ => None,
    };

    // The link back: a published upload gives a real URL, otherwise the channel handle.
    let upload = state.db.collection::<Document>("uploads")
        .find_one(doc! { "song_id": &payload.song_id, "status": "published" }).await.map_err(e)?
        .map(bson_to_value);
    let link = upload.as_ref()
        .and_then(|u| u["youtube_video_id"].as_str())
        .map(|id| format!("https://www.youtube.com/watch?v={id}"))
        .unwrap_or_default();

    let values = post_values(r.fields, &song, piece.as_ref(), &link);

    // Media the macro will need to attach. Named rather than uploaded: a file picker is the one thing a
    // macro cannot drive, so the path goes on screen for the user to pick.
    let mut media = Vec::new();
    for key in ["video_url", "local_audio_path"] {
        if let Some(p) = song[key].as_str().filter(|p| !p.is_empty()) {
            media.push(p.trim_start_matches("file://").to_string());
        }
    }
    if let Ok(mut cursor) = state.db.collection::<Document>("derivatives")
        .find(doc! { "song_id": &payload.song_id, "kind": "short" }).await {
        use futures_util::StreamExt;
        while let Some(Ok(d)) = cursor.next().await {
            if let Some(p) = bson_to_value(d)["media_path"].as_str() {
                media.push(p.to_string());
            }
        }
    }

    Ok(json!({
        "platform": r.platform,
        "label": r.label,
        "url": r.url,
        "fields": r.fields,
        "values": values,
        "checklist": r.checklist,
        "why": r.why,
        "media": media,
        "link": link,
        "note": "The values are queued in order. Record yourself pasting them field by field — each \
                 paste becomes a step that takes whatever is queued at replay time, so the same macro \
                 posts a different song tomorrow.",
    }))
}

#[derive(serde::Deserialize)]
pub struct LinkRequest {
    pub platform: String,
    /// The macro id from the Macro Manager.
    pub macro_id: String,
    pub macro_name: String,
    /// How many `paste-queue` steps it contains — must match the prepared value count.
    #[serde(default)]
    pub queue_steps: Option<i64>,
}

/// Remember which recorded macro publishes for which platform.
#[tauri::command]
pub async fn link_post_macro(state: State<'_, AppState>, payload: LinkRequest) -> Res<Value> {
    let r = recipe(&payload.platform)
        .ok_or_else(|| format!("no posting recipe for '{}'", payload.platform))?;
    let expected = r.fields.len() as i64;
    let got = payload.queue_steps.unwrap_or(expected);

    let doc_out = json!({
        "platform": payload.platform,
        "macro_id": payload.macro_id,
        "macro_name": payload.macro_name,
        "queue_steps": got,
        "expects": expected,
        // Said plainly rather than silently accepted: a mismatch means the replay will paste the wrong
        // value into a field, and the post still goes out looking fine.
        "warning": if got != expected {
            format!("This macro has {got} paste-the-next-value steps but {} values are prepared for \
                     {}. Every later paste will land in the wrong field.", expected, r.label)
        } else { String::new() },
        "linked_at": crate::models::now_iso(),
    });
    let d = bson::to_document(&doc_out).map_err(e)?;
    state.db.collection::<Document>("post_macros")
        .update_one(doc! { "platform": &payload.platform }, doc! { "$set": d }).upsert(true)
        .await.map_err(e)?;
    Ok(doc_out)
}

#[tauri::command]
pub async fn unlink_post_macro(state: State<'_, AppState>, platform: String) -> Res<Value> {
    state.db.collection::<Document>("post_macros")
        .delete_one(doc! { "platform": &platform }).await.map_err(e)?;
    Ok(json!({ "platform": platform, "unlinked": true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song() -> Value {
        json!({ "id": "s1", "title": "Genesis 1", "annotations": "In the beginning", "artist": "Lightkid" })
    }

    #[test]
    fn values_come_back_in_the_forms_own_order() {
        // The order is load-bearing: a recorded sequence of pastes lines up with the queue only if the
        // queue is in the same order the form asked for them.
        let v = post_values(&["title", "body", "link"], &song(), None, "https://youtu.be/abc");
        assert_eq!(v[0], "Genesis 1");
        assert_eq!(v[1], "In the beginning");
        assert_eq!(v[2], "https://youtu.be/abc");
    }

    #[test]
    fn a_blank_value_becomes_a_placeholder_rather_than_shifting_every_later_paste() {
        // This is the failure that is hardest to see: drop one value and the caption lands in the
        // hashtag field, while the post still goes out looking fine.
        let v = post_values(&["caption", "hashtags", "link"], &song(), None, "");
        assert_eq!(v.len(), 3, "one value per field, always");
        assert!(v.iter().all(|x| !x.trim().is_empty()), "no empty slot: {v:?}");
        assert_eq!(v[1], "—", "no hashtags without a publicity piece");
        assert_eq!(v[2], "—", "no link until the video is published");
    }

    #[test]
    fn a_publicity_piece_supplies_the_wording_when_there_is_one() {
        let piece = json!({
            "summary": "What the first morning sounded like",
            "body": "A longer article body",
            "hashtags": ["#scripture", "#gospel"],
        });
        let v = post_values(&["caption", "body", "hashtags"], &song(), Some(&piece), "");
        assert_eq!(v[0], "What the first morning sounded like");
        assert_eq!(v[1], "A longer article body");
        assert_eq!(v[2], "#scripture #gospel");
    }

    #[test]
    fn every_recipe_says_why_a_macro_rather_than_an_api() {
        for r in RECIPES {
            assert!(!r.why.is_empty(), "{} does not say why", r.platform);
            assert!(!r.checklist.is_empty(), "{} has no checklist", r.platform);
            assert!(!r.fields.is_empty(), "{} prepares nothing", r.platform);
        }
        // And the two with a genuine API route say so rather than presenting the macro as the answer.
        assert!(recipe("instagram").unwrap().why.contains("without App Review"));
        assert!(recipe("tiktok").unwrap().why.contains("no audit"));
    }

    #[test]
    fn a_long_body_is_truncated_before_it_reaches_a_form() {
        let long = "x".repeat(5000);
        let s = json!({ "title": "t", "annotations": long });
        let v = post_values(&["body"], &s, None, "");
        assert!(v[0].chars().count() <= 2000, "got {}", v[0].chars().count());
    }
}
