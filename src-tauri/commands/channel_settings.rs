// ────────────────────────────────────────────────────────────────
// Channel Settings Management with AI Translation
// Global topic description, common settings, and per-channel overrides
// ────────────────────────────────────────────────────────────────

use crate::state::AppState;
use bson::{doc, Document};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;
use uuid::Uuid;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

/// Global project settings structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalChannelSettings {
    pub topic_description: String,
    pub default_tags: Vec<String>,
    pub branding_text: String,
    pub about_section: String,
    pub layout_preferences: Value,
    pub content_style: String,
    pub upload_schedule: String,
}

/// Per-channel override structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelOverrides {
    pub language: String,
    pub region: String,
    pub musical_style: String,
    pub custom_tags: Option<Vec<String>>,
    pub custom_about: Option<String>,
}

/// Request structure for AI translation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslateSettingsRequest {
    pub global_settings: GlobalChannelSettings,
    pub target_language: String,
    pub target_region: String,
    pub musical_style: String,
    pub channel_name: String,
}

/// Helper to convert BSON document to JSON value
fn bson_to_value(doc: Document) -> Value {
    let mut m = serde_json::Map::new();
    for (k, v) in doc {
        if k == "_id" { continue; }
        if let Ok(jv) = bson::from_bson::<Value>(v) { m.insert(k, jv); }
    }
    Value::Object(m)
}

/// Get global channel settings for the current project
#[tauri::command]
pub async fn get_global_channel_settings(
    state: State<'_, AppState>,
    project_id: Option<String>,
) -> Res<Value> {
    let coll = state.db.collection::<Document>("global_channel_settings");
    
    let filter = if let Some(pid) = project_id {
        doc! { "project_id": &pid }
    } else {
        // Use a singleton for single-project setups
        doc! { "_id": "singleton" }
    };
    
    let result = coll
        .find_one(filter)
        .await
        .map_err(e)?
        .map(bson_to_value)
        .unwrap_or_else(|| serde_json::json!({}));
    
    Ok(result)
}

/// Save global channel settings
#[tauri::command]
pub async fn save_global_channel_settings(
    state: State<'_, AppState>,
    project_id: Option<String>,
    settings: Value,
) -> Res<Value> {
    let coll = state.db.collection::<Document>("global_channel_settings");
    
    let mut update_doc = settings.clone();
    if let Some(obj) = update_doc.as_object_mut() {
        obj.remove("_id");
        if let Some(pid) = &project_id {
            obj.insert("project_id".to_string(), Value::String(pid.clone()));
        }
    }
    
    let filter = if let Some(pid) = project_id {
        doc! { "project_id": &pid }
    } else {
        doc! { "_id": "singleton" }
    };
    
    let bson = bson::to_document(&update_doc).map_err(e)?;
    coll.update_one(
        filter,
        doc! { "$set": &bson },
    )
    .with_options(mongodb::options::UpdateOptions::builder().upsert(true).build())
    .await
    .map_err(e)?;
    
    Ok(update_doc)
}

/// Call OpenRouter AI for translation and cultural adaptation
async fn translate_with_ai(
    db: &mongodb::Database,
    request: &TranslateSettingsRequest,
) -> Res<Value> {
    let settings_doc = db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" })
        .await
        .map_err(e)?
        .map(bson_to_value)
        .unwrap_or_default();
    
    let api_key = settings_doc["openrouter_api_key"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();
    
    if api_key.is_empty() {
        return Err("Configure openrouter_api_key in Settings first".into());
    }
    
    let model = settings_doc["openrouter_model"]
        .as_str()
        .unwrap_or("qwen/qwen3-next-80b-a3b-instruct:free")
        .to_string();
    
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(e)?;
    
    // Build the system prompt for translation
    let system_prompt = format!(
        r#"You are an expert translator and cultural adapter for YouTube channel metadata.
Your task is to translate and adapt channel settings from English to {} for the {} region.
The channel focuses on: {}
Musical style: {}

Guidelines:
1. Translate naturally, not literally - adapt idioms and cultural references
2. Keep brand names and proper nouns in original form
3. Optimize tags for local search behavior in {}
4. Maintain the tone and style appropriate for {} audience
5. Return ONLY valid JSON with no additional text"#,
        request.target_language,
        request.target_region,
        request.global_settings.topic_description,
        request.musical_style,
        request.target_region,
        request.target_region
    );
    
    // Build the user message with content to translate
    let user_message = serde_json::json!({
        "topic_description": request.global_settings.topic_description,
        "branding_text": request.global_settings.branding_text,
        "about_section": request.global_settings.about_section,
        "default_tags": request.global_settings.default_tags,
        "content_style": request.global_settings.content_style,
        "channel_name": request.channel_name,
    });
    
    let body = serde_json::json!({
        "model": model,
        "temperature": 0.7,
        "response_format": { "type": "json_object" },
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": format!("Translate and adapt this content:\n{}", user_message) }
        ]
    });
    
    let response = http
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("HTTP-Referer", "https://lightkid.studio")
        .header("X-Title", "Lightkid AI Studio")
        .json(&body)
        .send()
        .await
        .map_err(e)?;
    
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("OpenRouter API error {}: {}", status, text));
    }
    
    let result: Value = response.json().await.map_err(e)?;
    
    // Extract the translated content
    let content = result["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("{}");
    
    // Parse the JSON response
    let translated: Value = serde_json::from_str(content)
        .unwrap_or_else(|_| serde_json::json!({}));
    
    Ok(translated)
}

/// Translate global settings for all channels using AI
#[tauri::command]
pub async fn translate_and_apply_settings(
    state: State<'_, AppState>,
    project_id: Option<String>,
    channel_ids: Option<Vec<String>>,
) -> Res<Value> {
    // Fetch global settings
    let global_settings_doc = state.db.collection::<Document>("global_channel_settings")
        .find_one(if let Some(pid) = &project_id {
            doc! { "project_id": &pid }
        } else {
            doc! { "_id": "singleton" }
        })
        .await
        .map_err(e)?
        .ok_or_else(|| "No global settings configured".to_string())?;
    
    let global_settings: GlobalChannelSettings = bson::from_bson(
        bson::Bson::Document(global_settings_doc)
    ).map_err(|e| format!("Failed to parse global settings: {}", e))?;
    
    // Fetch all channels or specific ones
    let channels_filter = if let Some(ids) = channel_ids {
        doc! { "id": { "$in": &ids } }
    } else if let Some(pid) = &project_id {
        doc! { "project_id": &pid }
    } else {
        doc! {}
    };
    
    use futures_util::StreamExt;
    let mut cursor = state.db.collection::<Document>("channels")
        .find(channels_filter)
        .await
        .map_err(e)?;
    
    let mut results = Vec::new();
    let mut errors = Vec::new();
    
    while let Some(Ok(channel_doc)) = cursor.next().await {
        let channel = bson_to_value(channel_doc.clone());
        let channel_id = channel["id"].as_str().unwrap_or("").to_string();
        let channel_name = channel["name"].as_str().unwrap_or("Unknown").to_string();
        let language = channel["language"].as_str().unwrap_or("English").to_string();
        let region = channel["region"].as_str().unwrap_or("US").to_string();
        let musical_style = channel["styles"].as_str().unwrap_or("").to_string();
        
        // Skip if same as source language (assume global settings are in English)
        if language.to_lowercase() == "english" {
            results.push(serde_json::json!({
                "channel_id": channel_id,
                "channel_name": channel_name,
                "status": "skipped_english",
                "message": "Using global settings directly"
            }));
            continue;
        }
        
        // Prepare translation request
        let translate_request = TranslateSettingsRequest {
            global_settings: global_settings.clone(),
            target_language: language.clone(),
            target_region: region.clone(),
            musical_style: musical_style.clone(),
            channel_name: channel_name.clone(),
        };
        
        // Call AI for translation
        match translate_with_ai(&state.db, &translate_request).await {
            Ok(translated) => {
                // Update channel with translated settings
                let update_doc = doc! {
                    "$set": {
                        "translated_description": translated.get("about_section")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&global_settings.about_section),
                        "translated_tags": translated.get("default_tags")
                            .and_then(|v| v.as_array())
                            .map(|arr| arr.iter()
                                .filter_map(|v| v.as_str())
                                .collect::<Vec<_>>()
                            )
                            .unwrap_or(global_settings.default_tags.clone()),
                        "translated_branding": translated.get("branding_text")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&global_settings.branding_text),
                        "last_synced_at": chrono::Utc::now().to_rfc3339(),
                    }
                };
                
                state.db.collection::<Document>("channels")
                    .update_one(doc! { "id": &channel_id }, update_doc)
                    .await
                    .map_err(e)?;
                
                results.push(serde_json::json!({
                    "channel_id": channel_id,
                    "channel_name": channel_name,
                    "language": language,
                    "region": region,
                    "status": "synced",
                    "translated_fields": ["description", "tags", "branding"]
                }));
            }
            Err(err) => {
                errors.push(serde_json::json!({
                    "channel_id": channel_id,
                    "channel_name": channel_name,
                    "error": err
                }));
            }
        }
    }
    
    Ok(serde_json::json!({
        "ok": true,
        "synced_count": results.len(),
        "error_count": errors.len(),
        "results": results,
        "errors": errors
    }))
}

/// Get individual channel settings with inheritance info
#[tauri::command]
pub async fn get_channel_settings(
    state: State<'_, AppState>,
    channel_id: String,
) -> Res<Value> {
    let channel_doc = state.db.collection::<Document>("channels")
        .find_one(doc! { "id": &channel_id })
        .await
        .map_err(e)?
        .ok_or_else(|| "Channel not found".to_string())?;
    
    let channel = bson_to_value(channel_doc);
    
    // Find the main/first channel (typically English) for inheritance
    let main_channel = state.db.collection::<Document>("channels")
        .find_one(doc! { "language": "English" })
        .sort(doc! { "created_at": 1 })
        .await
        .map_err(e)?
        .map(bson_to_value);
    
    Ok(serde_json::json!({
        "channel": channel,
        "main_channel": main_channel,
        "inherits_from_main": channel["language"].as_str() != Some("English")
    }))
}

/// Update individual channel overrides
#[tauri::command]
pub async fn update_channel_overrides(
    state: State<'_, AppState>,
    channel_id: String,
    overrides: Value,
) -> Res<Value> {
    let channels_coll = state.db.collection::<Document>("channels");
    
    // Verify channel exists
    let existing = channels_coll
        .find_one(doc! { "id": &channel_id })
        .await
        .map_err(e)?
        .ok_or_else(|| "Channel not found".to_string())?;
    
    // Build update document with only override fields
    let mut update_set = Document::new();
    
    if let Some(lang) = overrides.get("language").and_then(|v| v.as_str()) {
        update_set.insert("language".to_string(), bson::to_bson(lang).map_err(e)?);
    }
    if let Some(region) = overrides.get("region").and_then(|v| v.as_str()) {
        update_set.insert("region".to_string(), bson::to_bson(region).map_err(e)?);
    }
    if let Some(style) = overrides.get("musical_style").and_then(|v| v.as_str()) {
        update_set.insert("styles".to_string(), bson::to_bson(style).map_err(e)?);
    }
    if let Some(custom_tags) = overrides.get("custom_tags").and_then(|v| v.as_array()) {
        update_set.insert("custom_tags".to_string(), bson::to_bson(custom_tags).map_err(e)?);
    }
    if let Some(custom_about) = overrides.get("custom_about").and_then(|v| v.as_str()) {
        update_set.insert("custom_about".to_string(), bson::to_bson(custom_about).map_err(e)?);
    }
    
    if update_set.is_empty() {
        return Err("No valid override fields provided".into());
    }
    
    channels_coll
        .update_one(
            doc! { "id": &channel_id },
            doc! { "$set": &update_set },
        )
        .await
        .map_err(e)?;
    
    let updated = channels_coll
        .find_one(doc! { "id": &channel_id })
        .await
        .map_err(e)?
        .ok_or_else(|| "Channel not found after update".to_string())?;
    
    Ok(bson_to_value(updated))
}

/// Sync individual channel settings to YouTube Data API
#[tauri::command]
pub async fn sync_channel_to_youtube(
    state: State<'_, AppState>,
    channel_id: String,
) -> Res<Value> {
    let channel_doc = state.db.collection::<Document>("channels")
        .find_one(doc! { "id": &channel_id })
        .await
        .map_err(e)?
        .ok_or_else(|| "Channel not found".to_string())?;
    
    let channel = bson_to_value(channel_doc);
    
    // Check if channel is connected
    let connected = channel["connected"].as_bool().unwrap_or(false);
    let refresh_token = channel["refresh_token"].as_str().unwrap_or("");
    
    if !connected || refresh_token.is_empty() {
        return Err("Channel is not connected via OAuth".into());
    }
    
    // Get OAuth client for this channel
    let oauth_client_id = channel["oauth_client_id"].as_str();
    let client = crate::jobs::pick_oauth_client(&state.db, &channel, oauth_client_id)
        .await
        .ok_or_else(|| "No OAuth client configured for this channel".into())?;
    
    let client_id = client["client_id"].as_str().unwrap_or("");
    let client_secret = client["client_secret"].as_str().unwrap_or("");
    
    if client_id.is_empty() || client_secret.is_empty() {
        return Err("OAuth client credentials incomplete".into());
    }
    
    // Exchange refresh token for access token
    let http = reqwest::Client::new();
    let token_response = http
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(e)?;
    
    if !token_response.status().is_success() {
        return Err(format!("Token refresh failed: {}", token_response.status()));
    }
    
    let tokens: Value = token_response.json().await.map_err(e)?;
    let access_token = tokens["access_token"]
        .as_str()
        .ok_or("No access token returned")?;
    
    // Build the channel update payload
    // Determine what to sync: use custom values if set, otherwise inherited
    let description = channel["custom_about"]
        .as_str()
        .or_else(|| channel["translated_about"].as_str())
        .or_else(|| channel["translated_description"].as_str())
        .unwrap_or("");
    
    let tags: Vec<&str> = channel["custom_tags"]
        .as_array()
        .or_else(|| channel["translated_tags"].as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    
    let youtube_channel_id = channel["youtube_channel_id"]
        .as_str()
        .ok_or("No YouTube channel ID associated")?;
    
    // Call YouTube Data API to update channel
    let update_payload = serde_json::json!({
        "snippet": {
            "title": channel["name"],
            "description": description,
            "keywords": tags.join(" "),
        }
    });
    
    let update_response = http
        .put("https://www.googleapis.com/youtube/v3/channels")
        .query(&[("part", "snippet")])
        .bearer_auth(access_token)
        .json(&update_payload)
        .send()
        .await
        .map_err(e)?;
    
    if !update_response.status().is_success() {
        let status = update_response.status();
        let body = update_response.text().await.unwrap_or_default();
        return Err(format!("YouTube API error {}: {}", status, body));
    }
    
    let result: Value = update_response.json().await.map_err(e)?;
    
    // Mark as synced
    state.db.collection::<Document>("channels")
        .update_one(
            doc! { "id": &channel_id },
            doc! { "$set": {
                "last_youtube_sync": chrono::Utc::now().to_rfc3339(),
                "sync_status": "success",
            }},
        )
        .await
        .map_err(e)?;
    
    Ok(serde_json::json!({
        "ok": true,
        "channel_id": channel_id,
        "youtube_channel_id": youtube_channel_id,
        "result": result
    }))
}
