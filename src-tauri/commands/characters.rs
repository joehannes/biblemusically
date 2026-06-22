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
) -> Res<Vec<Value>> {
    use futures_util::StreamExt;
    let filter = if let Some(sid) = song_id {
        doc! { "$or": [ { "song_id": &sid }, { "song_id": { "$exists": false } }, { "song_id": null } ] }
    } else {
        doc! {}
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

    if prompt.is_empty() {
        return Err("Character has no image prompt or description. Set one first.".to_string());
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

    if prompt.is_empty() {
        return Err("Character has no image prompt or description.".to_string());
    }

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

    // Use AI to propose characters from lyrics
    let context = serde_json::json!({
        "title": title,
        "lyrics": lyrics,
        "styles": styles,
    });

    let system_prompt = "You are a creative assistant that analyzes song lyrics and identifies potential characters (people, personifications, or narrative voices) that appear in the text. For each character, provide a name and a short visual description suitable for generating a character portrait image. Return JSON exactly as {\"characters\":[{\"name\":\"...\",\"description\":\"...\",\"image_prompt\":\"...\"}]}. The image_prompt should be a detailed Midjourney-style prompt for a portrait of this character.";

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