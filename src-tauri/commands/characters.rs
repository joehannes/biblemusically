use crate::{
    models::{Character, CharacterCreate},
    state::AppState,
};
use bson::{doc, Document};
use serde_json::Value;
use tauri::State;
use uuid::Uuid;

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

#[tauri::command]
pub async fn list_characters(
    state: State<'_, AppState>,
    song_id: Option<String>,
    project_id: Option<String>,
) -> Res<Vec<Value>> {
    use futures_util::StreamExt;
    // Characters are visible if they belong to the given song, OR if they're project-level
    // (no song_id) and belong to the given project. Previously `project_id` wasn't accepted at
    // all here, so a character created at the project level (Character.project_id has always
    // existed in the model) had no way to actually be scoped/filtered by project — every
    // song-less character showed up everywhere. Passing no filters at all preserves the old
    // "list everything" behavior for any caller that wants it.
    let song_clause = song_id.as_ref().map(|sid| doc! { "song_id": sid });
    // Also match characters with no project_id at all (legacy data from before project-level
    // scoping existed) so pre-existing song-less characters don't silently disappear.
    let project_clause = project_id.as_ref().map(|pid| doc! {
        "$and": [
            { "$or": [ { "song_id": { "$exists": false } }, { "song_id": null } ] },
            { "$or": [ { "project_id": pid }, { "project_id": { "$exists": false } }, { "project_id": null } ] },
        ]
    });
    let filter = match (song_clause, project_clause) {
        (Some(s), Some(p)) => doc! { "$or": [s, p] },
        (Some(s), None) => s,
        (None, Some(p)) => p,
        (None, None) => doc! {},
    };
    let mut cursor = state.db.collection::<Document>("characters")
        .find(filter)
        .sort(doc! { "created_at": -1 })
        .await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }
    Ok(out)
}

#[tauri::command]
pub async fn create_character(
    state: State<'_, AppState>,
    body: CharacterCreate,
) -> Res<Value> {
    let ch = Character {
        id: Uuid::new_v4().to_string(),
        name: body.name,
        song_id: body.song_id,
        project_id: body.project_id,
        description: body.description.unwrap_or_default(),
        image_prompt: body.image_prompt.unwrap_or_default(),
        image_url: None,
        image_variants: vec![],
        selected_variant: 0,
        appearance_tags: body.appearance_tags.unwrap_or_default(),
        created_at: crate::models::now_iso(),
    };
    let bson = bson::to_document(&ch).map_err(e)?;
    state.db.collection::<Document>("characters").insert_one(bson).await.map_err(e)?;
    Ok(serde_json::to_value(&ch).map_err(e)?)
}

#[tauri::command]
pub async fn update_character(
    state: State<'_, AppState>,
    char_id: String,
    body: Value,
) -> Res<Value> {
    let mut update = doc! {};
    if let Some(name) = body["name"].as_str() {
        update.insert("name", name);
    }
    if let Some(desc) = body["description"].as_str() {
        update.insert("description", desc);
    }
    if let Some(prompt) = body["image_prompt"].as_str() {
        update.insert("image_prompt", prompt);
    }
    if let Some(song_id) = body["song_id"].as_str() {
        update.insert("song_id", song_id);
    }
    if let Some(project_id) = body["project_id"].as_str() {
        update.insert("project_id", project_id);
    }
    if let Some(tags) = body["appearance_tags"].as_array() {
        let tags: Vec<String> = tags.iter().filter_map(|v| v.as_str().map(String::from)).collect();
        update.insert("appearance_tags", tags);
    }
    if update.is_empty() {
        return Err("No fields to update".to_string());
    }
    state.db.collection::<Document>("characters")
        .update_one(doc! { "id": &char_id }, doc! { "$set": update })
        .await.map_err(e)?;
    let doc = state.db.collection::<Document>("characters")
        .find_one(doc! { "id": &char_id }).await.map_err(e)?
        .ok_or_else(|| "Character not found".to_string())?;
    Ok(bson_to_value(doc))
}

#[tauri::command]
pub async fn delete_character(
    state: State<'_, AppState>,
    char_id: String,
) -> Res<Value> {
    state.db.collection::<Document>("characters")
        .delete_one(doc! { "id": &char_id }).await.map_err(e)?;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn generate_character_image(
    state: State<'_, AppState>,
    state_arc: State<'_, std::sync::Arc<AppState>>,
    char_id: String,
) -> Res<Value> {
    // Fetch character
    let char_doc = state.db.collection::<Document>("characters")
        .find_one(doc! { "id": &char_id }).await.map_err(e)?
        .ok_or_else(|| "Character not found".to_string())?;
    let character = bson_to_value(char_doc);

    let prompt = character["image_prompt"].as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| character["description"].as_str())
        .unwrap_or("")
        .to_string();
    let has_tags = character["appearance_tags"].as_array().map(|a| !a.is_empty()).unwrap_or(false);

    if prompt.is_empty() && !has_tags {
        return Err("Character has no image prompt, description, or appearance tags. Set one first.".to_string());
    }

    // Enqueue an image job for this character
    let job = crate::jobs::enqueue("character_image", &char_id, &state_arc).await.map_err(e)?;
    Ok(serde_json::to_value(job).map_err(e)?)
}

#[tauri::command]
pub async fn vary_character_image(
    state: State<'_, AppState>,
    state_arc: State<'_, std::sync::Arc<AppState>>,
    char_id: String,
) -> Res<Value> {
    // Same as generate but always creates a new variant
    let char_doc = state.db.collection::<Document>("characters")
        .find_one(doc! { "id": &char_id }).await.map_err(e)?
        .ok_or_else(|| "Character not found".to_string())?;
    let character = bson_to_value(char_doc);

    let prompt = character["image_prompt"].as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| character["description"].as_str())
        .unwrap_or("")
        .to_string();
    let has_tags = character["appearance_tags"].as_array().map(|a| !a.is_empty()).unwrap_or(false);

    if prompt.is_empty() && !has_tags {
        return Err("Character has no image prompt, description, or appearance tags. Set one first.".to_string());
    }

    let job = crate::jobs::enqueue("character_image", &char_id, &state_arc).await.map_err(e)?;
    Ok(serde_json::to_value(job).map_err(e)?)
}

/// Generate a channel-adapted take of a character: the SAME character (appearance tags + its base
/// image as the IP-Adapter reference keep it recognizable) rendered in a given channel's visual
/// culture — so a figure stays coherent across channels while leaning into each channel's taste.
///
/// Sets a transient `variant_channel_id` the character_image job reads to blend the channel's
/// style/research into the prompt and file the result under `channel_variants.<channel_id>`.
#[tauri::command]
pub async fn generate_character_channel_variant(
    state: State<'_, AppState>,
    state_arc: State<'_, std::sync::Arc<AppState>>,
    char_id: String,
    channel_id: String,
) -> Res<Value> {
    let coll = state.db.collection::<Document>("characters");
    let ch = coll.find_one(doc! { "id": &char_id }).await.map_err(e)?
        .ok_or_else(|| "Character not found".to_string())?;
    let character = bson_to_value(ch);
    // Need a base image (or at least appearance) for consistency to mean anything.
    let has_ref = ["reference_image", "image_url"].iter().any(|k| character[*k].as_str().map(|s| !s.is_empty()).unwrap_or(false))
        || character["image_variants"].as_array().map(|a| !a.is_empty()).unwrap_or(false);
    let has_tags = character["appearance_tags"].as_array().map(|a| !a.is_empty()).unwrap_or(false)
        || character["image_prompt"].as_str().map(|s| !s.is_empty()).unwrap_or(false)
        || character["description"].as_str().map(|s| !s.is_empty()).unwrap_or(false);
    if !has_tags {
        return Err("Give the character appearance tags or a prompt first — that's what keeps it consistent.".into());
    }
    if !has_ref {
        return Err("Generate a base image for this character first; per-channel variants build on it for consistency.".into());
    }
    coll.update_one(doc! { "id": &char_id }, doc! { "$set": { "variant_channel_id": &channel_id } }).await.map_err(e)?;
    let job = crate::jobs::enqueue("character_image", &char_id, &state_arc).await.map_err(e)?;
    Ok(serde_json::to_value(job).map_err(e)?)
}

#[tauri::command]
pub async fn select_character_variant(
    state: State<'_, AppState>,
    char_id: String,
    variant_index: i32,
) -> Res<Value> {
    let char_doc = state.db.collection::<Document>("characters")
        .find_one(doc! { "id": &char_id }).await.map_err(e)?
        .ok_or_else(|| "Character not found".to_string())?;
    let character = bson_to_value(char_doc);
    let variants = character["image_variants"].as_array().map(|a| a.len()).unwrap_or(0);
    if variant_index < 0 || variant_index as usize >= variants {
        return Err(format!("Variant index {} out of range (0-{})", variant_index, variants.saturating_sub(1)));
    }
    state.db.collection::<Document>("characters")
        .update_one(
            doc! { "id": &char_id },
            doc! { "$set": { "selected_variant": variant_index } },
        ).await.map_err(e)?;
    Ok(serde_json::json!({ "ok": true, "selected_variant": variant_index }))
}

#[tauri::command]
pub async fn discard_character_variant(
    state: State<'_, AppState>,
    char_id: String,
    variant_index: i32,
) -> Res<Value> {
    let char_doc = state.db.collection::<Document>("characters")
        .find_one(doc! { "id": &char_id }).await.map_err(e)?
        .ok_or_else(|| "Character not found".to_string())?;
    let character = bson_to_value(char_doc);
    let variants: Vec<String> = character["image_variants"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    if variant_index < 0 || variant_index as usize >= variants.len() {
        return Err("Variant index out of range".to_string());
    }
    let new_variants: Vec<String> = variants.into_iter().enumerate()
        .filter(|(i, _)| *i != variant_index as usize)
        .map(|(_, v)| v)
        .collect();
    let selected = character["selected_variant"].as_i64().unwrap_or(0);
    let new_selected = if selected as i32 == variant_index {
        0.min(new_variants.len().saturating_sub(1) as i64)
    } else if selected > variant_index as i64 {
        selected - 1
    } else {
        selected
    };
    let bson_variants: Vec<bson::Bson> = new_variants.iter().map(|s| bson::Bson::String(s.clone())).collect();
    state.db.collection::<Document>("characters")
        .update_one(
            doc! { "id": &char_id },
            doc! { "$set": { "image_variants": bson_variants, "selected_variant": new_selected } },
        ).await.map_err(e)?;
    Ok(serde_json::json!({ "ok": true, "remaining": new_variants.len() }))
}

#[tauri::command]
pub async fn discard_all_character_variants(
    state: State<'_, AppState>,
    char_id: String,
) -> Res<Value> {
    state.db.collection::<Document>("characters")
        .update_one(
            doc! { "id": &char_id },
            doc! { "$set": { "image_variants": [], "selected_variant": 0 as i32, "image_url": bson::Bson::Null } },
        ).await.map_err(e)?;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn propose_characters(
    state: State<'_, AppState>,
    song_id: String,
) -> Res<Value> {
    // Fetch the song to get lyrics
    let song_doc = state.db.collection::<Document>("songs")
        .find_one(doc! { "id": &song_id }).await.map_err(e)?
        .ok_or_else(|| "Song not found".to_string())?;
    let song = bson_to_value(song_doc);
    let lyrics = song["lyrics"].as_str().unwrap_or("");
    let title = song["title"].as_str().unwrap_or("Untitled");
    let styles = song["styles"].as_str().unwrap_or("");

    if lyrics.is_empty() {
        return Err("Song has no lyrics to analyze".to_string());
    }

    // The project's Brief shapes who these characters ARE — their bearing, expression and styling
    // should embody the project's mood/personality and read right to its audience, not be generic
    // portraits detached from the channel's voice.
    let project_id = song["project_id"].as_str().unwrap_or("");
    let brief = crate::commands::ai::project_brief_block(&state.db, project_id).await;

    // Use AI to propose characters from lyrics
    let context = serde_json::json!({
        "title": title,
        "lyrics": lyrics,
        "styles": styles,
        "project_brief": brief,
    });

    let system_prompt = "You are a creative assistant that analyzes song lyrics and identifies potential characters (people, personifications, or narrative voices) that appear in the text. For each character, provide a name and a short visual description suitable for generating a character portrait image. Return JSON exactly as {\"characters\":[{\"name\":\"...\",\"description\":\"...\",\"image_prompt\":\"...\"}]}. The image_prompt should be a detailed Midjourney-style prompt for a portrait of this character. When a project_brief is provided, let it shape each character's bearing, expression, wardrobe and atmosphere so they embody the project's mood, personality and audience.";

    let result = crate::commands::ai::call_openrouter(
        &state.db,
        system_prompt,
        &serde_json::to_string(&context).unwrap_or_default(),
        0.5,
        true,
    ).await.map_err(|err| format!("AI proposal failed: {}", err))?;

    let text = result["text"].as_str().unwrap_or("");
    let parsed: Value = serde_json::from_str(text).unwrap_or(serde_json::json!({}));
    let characters = parsed["characters"].as_array().cloned().unwrap_or_default();

    if characters.is_empty() {
        return Err("AI did not identify any characters in the lyrics".to_string());
    }

    // Auto-create character entries in DB
    let coll = state.db.collection::<Document>("characters");
    let mut created = Vec::new();
    for ch_val in &characters {
        let name = ch_val["name"].as_str().unwrap_or("Unknown").to_string();
        let description = ch_val["description"].as_str().unwrap_or("").to_string();
        let image_prompt = ch_val["image_prompt"].as_str().unwrap_or("").to_string();
        let character = Character {
            id: Uuid::new_v4().to_string(),
            name,
            song_id: Some(song_id.clone()),
            project_id: None,
            description,
            image_prompt,
            image_url: None,
            image_variants: vec![],
            selected_variant: 0,
            appearance_tags: vec![],
            created_at: crate::models::now_iso(),
        };
        let bson = bson::to_document(&character).map_err(e)?;
        coll.insert_one(bson).await.map_err(e)?;
        created.push(serde_json::to_value(&character).map_err(e)?);
    }

    Ok(serde_json::json!({
        "ok": true,
        "characters": created,
        "count": created.len(),
    }))
}
// ────────────────────────────────────────────────────────────────────────────
// Character → section linking
// ────────────────────────────────────────────────────────────────────────────

/// The stable visual identity of a character, as prompt text: appearance tags first (they are the
/// descriptors meant to survive every regeneration), then the free-form prompt/description.
///
/// This is the same order `jobs.rs` uses when it renders a character portrait, so a section
/// generated from this text and the character's own portrait describe the same person — which is
/// the entire point of the feature.
fn character_prompt_fragment(character: &Value) -> String {
    let tags: Vec<String> = character["appearance_tags"]
        .as_array()
        .map(|a| a.iter().filter_map(|t| t.as_str()).map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect())
        .unwrap_or_default();
    let body = character["image_prompt"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| character["description"].as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let name = character["name"].as_str().unwrap_or("").trim().to_string();

    let mut parts: Vec<String> = Vec::new();
    if !name.is_empty() { parts.push(name); }
    if !tags.is_empty() { parts.push(tags.join(", ")); }
    if !body.is_empty() { parts.push(body); }
    parts.join(", ")
}

/// Attach a character to specific sections and push its visual identity into their image prompts.
///
/// Before this, "consistent characters" stopped at the character portrait: to actually get the same
/// face into a song's scenes you had to copy the character's prompt into each section's
/// `image_prompt` by hand, for every section, every time you tweaked the character. Now the link is
/// recorded on the section (`character_ids`) and the prompt text is merged in one action.
///
/// `mode`:
/// * `prepend` (default) — the character leads the prompt, the scene follows. Best for
///   image models that weight the start of a prompt more heavily.
/// * `append` — scene first, character after.
/// * `replace` — the section's prompt becomes the character alone (for portrait-style sections).
///
/// Re-applying is safe: a fragment already present in the prompt is not added twice, so pressing
/// the button again after editing a section doesn't accumulate duplicates.
#[tauri::command]
pub async fn apply_character_to_sections(
    state: State<'_, AppState>,
    char_id: String,
    section_ids: Vec<String>,
    mode: Option<String>,
) -> Res<Value> {
    let character = state.db.collection::<Document>("characters")
        .find_one(doc! { "id": &char_id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "Character not found".to_string())?;

    let fragment = character_prompt_fragment(&character);
    if fragment.is_empty() {
        return Err("This character has no name, appearance tags, prompt or description to apply.".into());
    }
    if section_ids.is_empty() {
        return Err("Select at least one section.".into());
    }

    let mode = mode.unwrap_or_else(|| "prepend".into());
    let sections = state.db.collection::<Document>("sections");
    let mut updated = 0u64;
    let mut skipped = 0u64;

    for id in &section_ids {
        let Some(doc_found) = sections.find_one(doc! { "id": id }).await.map_err(e)? else { continue };
        let section = bson_to_value(doc_found);
        let existing = section["image_prompt"].as_str().unwrap_or("").trim().to_string();

        let next = match mode.as_str() {
            "replace" => fragment.clone(),
            _ if existing.is_empty() => fragment.clone(),
            _ if existing.contains(&fragment) => { skipped += 1; existing.clone() }
            "append" => format!("{existing}, {fragment}"),
            _ => format!("{fragment}, {existing}"),
        };

        // Keep the link itself, so the UI can show which sections a character appears in and a
        // later edit of the character can offer to refresh them.
        let mut links: Vec<String> = section["character_ids"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect())
            .unwrap_or_default();
        if !links.contains(&char_id) { links.push(char_id.clone()); }

        let r = sections.update_one(
            doc! { "id": id },
            doc! { "$set": { "image_prompt": &next, "character_ids": &links } },
        ).await.map_err(e)?;
        updated += r.modified_count;
    }

    Ok(serde_json::json!({
        "ok": true,
        "updated": updated,
        "already_applied": skipped,
        "fragment": fragment,
    }))
}

/// Remove a character from sections: drop the link and strip its fragment back out of the prompt.
#[tauri::command]
pub async fn detach_character_from_sections(
    state: State<'_, AppState>,
    char_id: String,
    section_ids: Vec<String>,
) -> Res<Value> {
    let character = state.db.collection::<Document>("characters")
        .find_one(doc! { "id": &char_id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "Character not found".to_string())?;
    let fragment = character_prompt_fragment(&character);
    let sections = state.db.collection::<Document>("sections");
    let mut updated = 0u64;

    for id in &section_ids {
        let Some(doc_found) = sections.find_one(doc! { "id": id }).await.map_err(e)? else { continue };
        let section = bson_to_value(doc_found);
        let existing = section["image_prompt"].as_str().unwrap_or("").to_string();
        // Strip the fragment and tidy up the separator it left behind.
        let cleaned = existing
            .replace(&format!("{fragment}, "), "")
            .replace(&format!(", {fragment}"), "")
            .replace(&fragment, "")
            .trim()
            .trim_matches(',')
            .trim()
            .to_string();
        let links: Vec<String> = section["character_ids"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).filter(|s| *s != char_id).map(|s| s.to_string()).collect())
            .unwrap_or_default();
        let r = sections.update_one(
            doc! { "id": id },
            doc! { "$set": { "image_prompt": &cleaned, "character_ids": &links } },
        ).await.map_err(e)?;
        updated += r.modified_count;
    }
    Ok(serde_json::json!({ "ok": true, "updated": updated }))
}

/// Which sections of a song each character is currently attached to — so the UI can show
/// "appears in 4 sections" and pre-tick the right boxes.
#[tauri::command]
pub async fn character_section_links(state: State<'_, AppState>, song_id: String) -> Res<Value> {
    use futures_util::StreamExt;
    let mut cursor = state.db.collection::<Document>("sections")
        .find(doc! { "song_id": &song_id }).sort(doc! { "index": 1 })
        .await.map_err(e)?;
    let mut by_character: std::collections::HashMap<String, Vec<Value>> = std::collections::HashMap::new();
    let mut sections_out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await {
        let section = bson_to_value(d);
        let id = section["id"].as_str().unwrap_or("").to_string();
        let index = section["index"].clone();
        for cid in section["character_ids"].as_array().cloned().unwrap_or_default() {
            if let Some(cid) = cid.as_str() {
                by_character.entry(cid.to_string()).or_default()
                    .push(serde_json::json!({ "id": id, "index": index }));
            }
        }
        sections_out.push(section);
    }
    Ok(serde_json::json!({ "ok": true, "by_character": by_character, "sections": sections_out }))
}
