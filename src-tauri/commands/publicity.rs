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
// Publicity
//
// A finished video is not a published one. By the time a song has lyrics, a style, analysed sections
// and rendered images, the studio knows enough to write *about* it — and that writing is what
// actually reaches people on the platforms where a bare video link is ignored or removed.
//
// So this authors a piece per platform from everything already on hand: the song, its sections and
// their moods, the images that were made for it, the project's brief, today's topic, the channel's
// language and region, and the creator's own social voice. Each piece carries its own cover-image
// prompts, written for that platform's crop, and links back to the YouTube video and any short.
//
// Two things it deliberately does NOT do:
//   • publish without being told to — `status` starts at "draft" and a human moves it;
//   • pretend every platform has an API. Where posting is gated or paid, the piece carries a
//     `manual` route with the exact text to paste, and the browser macro path takes it from there.
// ────────────────────────────────────────────────────────────────

/// What one platform expects a piece to look like. Written from each platform's own norms, because a
/// Reddit post that reads like a press release is removed, and a blog post that reads like a tweet
/// is ignored.
fn platform_brief(platform: &str) -> &'static str {
    match platform {
        "reddit" => "Reddit. Value first, promotion last. Open with the idea or the story, not the song. \
                     No hype words, no emoji walls, no 'check out my'. One link at the end, in context. \
                     Assume the reader is sceptical of self-promotion and knows exactly what it looks like. \
                     600–900 words, plain paragraphs, no headings.",
        "devto" | "wordpress" | "medium" => "A blog article. Markdown with 2–4 `##` headings, 700–1200 words. \
                     Open with something concrete — a verse, an image, a question — never 'In today's world'. \
                     The song is the illustration of the idea, not the subject of the article.",
        "x" => "X. One post of at most 240 characters, then 2–4 more as a thread. No hashtag spam \
                (at most two, at the end of the last post). The first post must stand alone.",
        "linkedin" => "LinkedIn. Reflective and professional without being corporate. 150–250 words, \
                       short paragraphs, one clear takeaway. No hashtags beyond two.",
        "mastodon" | "bluesky" => "A single federated post, at most 480 characters, warm and direct. \
                                   One link, no hashtag spam. Alt text for the image matters here.",
        "pinterest" => "A pin description: 2–3 sentences of what the viewer is looking at and why it matters, \
                        written for someone who will read it months from now with no context.",
        "tumblr" => "A Tumblr post: image-led, a short evocative caption, tags as full phrases rather than \
                     single words.",
        "telegram" | "discord" => "A channel post for people who already follow this project. Familiar tone, \
                                   3–5 sentences, the link inline.",
        "instagram" | "tiktok" => "A caption: 2–3 short lines, the hook in the first line because the rest is \
                                   collapsed. Hashtags on their own final line.",
        _ => "A short post: a clear hook, two or three sentences of substance, one link.",
    }
}

/// Everything the studio knows about one song, rendered as a prompt block.
async fn song_context_block(db: &crate::store::Db, song_id: &str) -> Res<(Value, String)> {
    use futures_util::StreamExt;
    let song = db.collection::<Document>("songs")
        .find_one(doc! { "id": song_id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "song not found".to_string())?;

    let mut sections: Vec<Value> = Vec::new();
    if let Ok(mut cursor) = db.collection::<Document>("sections").find(doc! { "song_id": song_id }).await {
        while let Some(Ok(d)) = cursor.next().await { sections.push(bson_to_value(d)); }
    }
    sections.sort_by_key(|s| s["index"].as_i64().unwrap_or(0));

    // The images that exist are as much a part of the piece as the words — their prompts describe
    // what the reader will see next to the text.
    let scenes: Vec<String> = sections.iter()
        .filter(|s| s["image_url"].as_str().map(|u| !u.is_empty()).unwrap_or(false))
        .map(|s| format!("- {} ({}): {}",
            s["name"].as_str().unwrap_or("section"),
            s["mood"].as_str().unwrap_or("—"),
            s["image_prompt"].as_str().or_else(|| s["text"].as_str()).unwrap_or("").chars().take(140).collect::<String>()))
        .collect();

    let block = format!(
        "SONG\nTitle: {}\nLanguage: {}\nStyle: {}\nSections: {}\n{}\n\nLYRICS (excerpt)\n{}\n",
        song["title"].as_str().unwrap_or(""),
        song["language"].as_str().unwrap_or(""),
        song["styles"].as_str().unwrap_or(""),
        sections.len(),
        if scenes.is_empty() { "(no images rendered yet)".to_string() } else { format!("SCENES\n{}", scenes.join("\n")) },
        song["lyrics"].as_str().unwrap_or("").chars().take(1500).collect::<String>(),
    );
    Ok((song, block))
}

#[derive(serde::Deserialize)]
pub struct AuthorPieceRequest {
    pub song_id: String,
    /// Platform id from `list_social_platforms` — decides length, tone and structure.
    pub platform: String,
    /// Optional steer ("lead with the Hebrew imagery", "aim at worship leaders").
    #[serde(default)]
    pub angle: Option<String>,
    /// How many cover images to plan. 0–4.
    #[serde(default)]
    pub covers: Option<u8>,
}

/// Write one publicity piece for one platform, with its own cover-image prompts.
#[tauri::command]
pub async fn author_publicity_piece(state: State<'_, AppState>, payload: AuthorPieceRequest) -> Res<Value> {
    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();

    let (song, song_block) = song_context_block(&state.db, &payload.song_id).await?;
    let project_id = song["project_id"].as_str().unwrap_or("").to_string();

    // Where the finished work already lives, so the piece can point at it rather than describe it.
    let upload = state.db.collection::<Document>("uploads")
        .find_one(doc! { "song_id": &payload.song_id, "status": "published" }).await.map_err(e)?
        .map(bson_to_value);
    let youtube_url = upload.as_ref()
        .and_then(|u| u["youtube_video_id"].as_str())
        .map(|id| format!("https://www.youtube.com/watch?v={id}"))
        .or_else(|| song["video_url"].as_str().filter(|u| u.starts_with("http")).map(|s| s.to_string()));
    let short_url = state.db.collection::<Document>("derivatives")
        .find_one(doc! { "song_id": &payload.song_id, "kind": "short" }).await.map_err(e)?
        .map(bson_to_value)
        .and_then(|d| d["published_url"].as_str().map(|s| s.to_string()));

    let brief = crate::commands::ai::project_brief_block(&state.db, &project_id).await;
    let learnings = crate::commands::learnings::learnings_prompt_block(&state, &project_id).await;
    let research = crate::commands::ai::channel_research_block(&state.db).await;
    let taste = crate::commands::social::taste_profile_block().await;

    let covers = payload.covers.unwrap_or(2).min(4);
    let system = format!(
        "You are the publicist for a Bible music-video project — a good one, which means you write \
         something worth reading rather than an announcement.\n\n\
         TARGET: {}\n\n\
         Write ONE piece about this song for that platform. Rules that hold everywhere:\n\
         - Lead with an idea, an image, or a question. Never with the product.\n\
         - Ground it in this specific song: its scripture, its scenes, its language, its sound.\n\
         - Say something true. If the song is a lament, do not sell it as uplifting.\n\
         - No invented facts, no fake statistics, no claims about the artist you were not given.\n\
         - Write in the language of the song's audience when that is not English.\n\
         - Where a link belongs, write the literal token <<LINK>> — it is substituted afterwards.\n\n\
         Also plan {covers} cover image(s) for this piece: not frames from the video, but images made \
         for this article — each prompt a single vivid sentence naming subject, composition and light, \
         in the project's visual style.\n\n\
         Return ONLY: {{\"title\":\"…\",\"body\":\"…\",\"summary\":\"one sentence\",\
         \"tags\":[\"…\"],\"cover_prompts\":[\"…\"],\"posting_notes\":\"anything the human should know \
         before posting this — subreddit rules, tone risk, timing\"}}",
        platform_brief(&payload.platform),
    );

    let user = format!(
        "{brief}{taste}{learnings}{research}\nTODAY'S TOPIC: {}\n\n{song_block}\nLINKS AVAILABLE: {}\n\nEXTRA STEER: {}",
        settings["daily_topic"].as_str().unwrap_or("(none)"),
        [youtube_url.clone(), short_url.clone()].iter().flatten().cloned().collect::<Vec<_>>().join(", "),
        payload.angle.clone().unwrap_or_else(|| "(none)".into()),
    );

    let (content, model) = crate::commands::ai::provider_chat(&settings, &system, &user, 0.8, true).await?;
    let parsed = crate::commands::ai::extract_json_value(&content)
        .ok_or_else(|| format!("The AI returned no usable JSON (raw: {})", content.chars().take(200).collect::<String>()))?;

    let body_raw = parsed["body"].as_str().unwrap_or("").trim().to_string();
    if body_raw.is_empty() { return Err("The AI returned an empty piece.".into()); }
    // Substitute the link token now, so the stored piece is postable as-is.
    let link = youtube_url.clone().or_else(|| short_url.clone()).unwrap_or_default();
    let body = if link.is_empty() {
        // Nothing published yet: drop the token rather than leave a placeholder in the text.
        body_raw.replace("<<LINK>>", "").replace("  ", " ")
    } else { body_raw.replace("<<LINK>>", &link) };

    let piece = json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "song_id": payload.song_id,
        "project_id": project_id,
        "platform": payload.platform,
        "title": parsed["title"].as_str().unwrap_or("").trim(),
        "body": body,
        "summary": parsed["summary"].as_str().unwrap_or("").trim(),
        "tags": parsed["tags"].clone(),
        "cover_prompts": parsed["cover_prompts"].as_array()
            .map(|a| a.iter().take(covers as usize).cloned().collect::<Vec<_>>()).unwrap_or_default(),
        "cover_urls": Value::Array(vec![]),
        "posting_notes": parsed["posting_notes"].as_str().unwrap_or("").trim(),
        "youtube_url": youtube_url,
        "short_url": short_url,
        "status": "draft",
        "model": model,
        "created_at": crate::models::now_iso(),
    });
    if let Ok(d) = bson::to_document(&piece) {
        state.db.collection::<Document>("publicity_pieces").insert_one(d).await.map_err(e)?;
    }
    Ok(piece)
}

/// Render the cover images a piece planned. Each becomes a normal image job, so it uses whatever
/// image engine is configured and shows up in the Jobs monitor like everything else.
#[tauri::command]
pub async fn generate_publicity_covers(
    state: State<'_, AppState>,
    state_arc: State<'_, std::sync::Arc<AppState>>,
    id: String,
) -> Res<Value> {
    let piece = state.db.collection::<Document>("publicity_pieces")
        .find_one(doc! { "id": &id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "piece not found".to_string())?;
    let prompts: Vec<String> = piece["cover_prompts"].as_array()
        .map(|a| a.iter().filter_map(|p| p.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    if prompts.is_empty() { return Err("This piece has no cover prompts.".into()); }

    // Cover images are section-shaped work, so they ride the existing section/image pipeline: one
    // synthetic section per cover, tagged so the Publicity view can find them again.
    let mut queued = 0;
    for (i, prompt) in prompts.iter().enumerate() {
        let section = json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "song_id": piece["song_id"].as_str().unwrap_or(""),
            "index": 900 + i as i64,            // out of the way of the song's real sections
            "name": format!("publicity cover {}", i + 1),
            "text": piece["summary"].as_str().unwrap_or(""),
            "image_prompt": prompt,
            "mood": "publicity",
            "publicity_piece_id": id,
            "created_at": crate::models::now_iso(),
        });
        if let Ok(d) = bson::to_document(&section) {
            if state.db.collection::<Document>("sections").insert_one(d).await.is_ok() {
                let sec_id = section["id"].as_str().unwrap_or("").to_string();
                if crate::jobs::enqueue("image", &sec_id, &state_arc).await.is_ok() { queued += 1; }
            }
        }
    }
    Ok(json!({ "ok": true, "queued": queued }))
}

#[tauri::command]
pub async fn list_publicity_pieces(state: State<'_, AppState>, song_id: Option<String>, project_id: Option<String>) -> Res<Vec<Value>> {
    use futures_util::StreamExt;
    let filter = match (song_id.as_deref(), project_id.as_deref()) {
        (Some(s), _) if !s.is_empty() => doc! { "song_id": s },
        (_, Some(p)) if !p.is_empty() => doc! { "project_id": p },
        _ => doc! {},
    };
    let mut cursor = state.db.collection::<Document>("publicity_pieces")
        .find(filter).sort(doc! { "created_at": -1 }).await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await {
        let mut piece = bson_to_value(d);
        // Attach any cover images that have finished rendering since the piece was written.
        let id = piece["id"].as_str().unwrap_or("").to_string();
        let mut urls = Vec::new();
        if let Ok(mut secs) = state.db.collection::<Document>("sections")
            .find(doc! { "publicity_piece_id": &id }).await {
            while let Some(Ok(sd)) = secs.next().await {
                let s = bson_to_value(sd);
                if let Some(u) = s["image_url"].as_str().filter(|u| !u.is_empty()) {
                    urls.push(Value::String(u.to_string()));
                }
            }
        }
        piece["cover_urls"] = Value::Array(urls);
        out.push(piece);
    }
    Ok(out)
}

#[tauri::command]
pub async fn update_publicity_piece(state: State<'_, AppState>, id: String, patch: Value) -> Res<Value> {
    let mut set = doc! { "updated_at": crate::models::now_iso() };
    if let Some(obj) = patch.as_object() {
        for (k, v) in obj {
            if k == "_id" || k == "id" { continue; }
            if let Ok(b) = bson::to_bson(v) { set.insert(k.clone(), b); }
        }
    }
    state.db.collection::<Document>("publicity_pieces")
        .update_one(doc! { "id": &id }, doc! { "$set": set }).await.map_err(e)?;
    Ok(json!({ "ok": true }))
}

#[tauri::command]
pub async fn delete_publicity_piece(state: State<'_, AppState>, id: String) -> Res<Value> {
    state.db.collection::<Document>("publicity_pieces").delete_one(doc! { "id": &id }).await.map_err(e)?;
    // The cover sections go with it; leaving them would haunt the song's section list.
    let _ = state.db.collection::<Document>("sections").delete_many(doc! { "publicity_piece_id": &id }).await;
    Ok(json!({ "ok": true }))
}

/// Author a piece for every connected platform at once — the after-the-upload sweep.
///
/// Deliberately stops at drafts: publishing is a separate, explicit act, and a batch of posts that
/// went out unread is not something an apology fixes.
#[tauri::command]
pub async fn author_publicity_set(state: State<'_, AppState>, song_id: String, platforms: Option<Vec<String>>) -> Res<Value> {
    use futures_util::StreamExt;
    let targets: Vec<String> = match platforms {
        Some(p) if !p.is_empty() => p,
        _ => {
            let mut connected = Vec::new();
            if let Ok(mut cursor) = state.db.collection::<Document>("social_accounts").find(doc! {}).await {
                while let Some(Ok(d)) = cursor.next().await {
                    if let Some(p) = bson_to_value(d)["platform"].as_str() { connected.push(p.to_string()); }
                }
            }
            connected
        }
    };
    if targets.is_empty() {
        return Err("No social accounts are connected yet — connect one in Social Presence first.".into());
    }

    let mut written = Vec::new();
    let mut failed = Vec::new();
    for platform in targets {
        // Sequential on purpose: a free tier that gets four concurrent requests answers with 429s.
        match author_publicity_piece(state.clone(), AuthorPieceRequest {
            song_id: song_id.clone(), platform: platform.clone(), angle: None, covers: Some(1),
        }).await {
            Ok(p) => written.push(json!({ "platform": platform, "id": p["id"], "title": p["title"] })),
            Err(err) => failed.push(json!({ "platform": platform, "error": err })),
        }
    }
    Ok(json!({ "written": written, "failed": failed }))
}
