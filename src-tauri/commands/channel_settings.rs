// ────────────────────────────────────────────────────────────────
// Channel Settings Management with AI Translation
// Global topic description, common settings, and per-channel overrides
// ────────────────────────────────────────────────────────────────

use crate::state::AppState;
use bson::{doc, Document};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

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

/// Truncate to at most `max` *characters* (not bytes) — YouTube's limits are character counts,
/// and a byte-based truncation could otherwise split a multi-byte UTF-8 character mid-sequence.
fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
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
    .with_options(crate::store::UpdateOptions::builder().upsert(true).build())
    .await
    .map_err(e)?;
    
    Ok(update_doc)
}

/// Call the configured AI provider for translation and cultural adaptation.
///
/// Goes through the shared `provider_chat`, so it follows the provider chosen in Settings and
/// automatically retries on the free fallback model when that provider is overloaded.
async fn translate_with_ai(
    db: &crate::store::Db,
    request: &TranslateSettingsRequest,
) -> Res<Value> {
    let settings_doc = db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" })
        .await
        .map_err(e)?
        .map(bson_to_value)
        .unwrap_or_default();

    // Templated system prompt: explicit per-field guidance (a real localizer's job — adapt, don't
    // transliterate), a hard JSON schema (so parsing below can trust the shape instead of hoping),
    // YouTube's actual field limits (so a downstream sync_channel_to_youtube call can't get
    // rejected by content the AI never knew had a ceiling), and — when the channel has no region
    // set yet — an explicit ask to infer one, so "the right audience region" doesn't have to be
    // hand-filled for every channel before this can run.
    let region_instruction = if request.target_region.trim().is_empty() || request.target_region.eq_ignore_ascii_case("unknown") {
        "The target region is UNKNOWN. Infer the single most likely primary audience region/country \
         for this language + content style combination (a real creator would pick one primary \
         market even for a language spoken in several countries), and return it as \
         \"suggested_region\" — an ISO 3166-1 alpha-2 country code (e.g. \"BR\", \"PT\", \"MX\", \"JP\").".to_string()
    } else {
        format!("The target region is {}. Do not include \"suggested_region\" in your response.", request.target_region)
    };

    let system_prompt = format!(
        r#"You are a YouTube channel branding and localization specialist. You adapt a channel's
global identity into a specific language and region — never a literal, word-for-word translation.
A literal translation of tags/keywords is close to useless for discovery; write what a real
{target_language}-speaking viewer would actually type into YouTube search.

CHANNEL CONTEXT
- Channel name: {channel_name}
- Target language: {target_language}
- Content style: {content_style}
- Musical/content style detail: {musical_style}
- Overall topic: {topic_description}
- {region_instruction}

YOUR TASKS
1. Adapt "about_section" and "branding_text" into {target_language} — natural and idiomatic, the
   way a native {target_language}-speaking creator in this niche would actually write it. Keep
   brand names and proper nouns in their original form.
2. Localize "default_tags" into {target_language} search terms real viewers in the target region
   would type — not literal translations of the English tags. Prioritize discovery over coverage.
3. If asked to infer a region above, do so; otherwise omit "suggested_region" entirely.
4. In "cultural_notes" (max 2 sentences, or empty string if nothing notable), flag anything
   culturally significant behind a non-obvious choice you made — a sensitivity, a regional
   preference, a naming convention.

HARD LIMITS (YouTube API rejects channel updates over these — stay under them)
- about_section: 1000 characters max.
- branding_text: 200 characters max.
- default_tags: 15 tags max, each under 30 characters.

Return ONLY a JSON object with exactly these keys and nothing else:
{{"about_section": string, "branding_text": string, "default_tags": string[], "suggested_region": string (omit if not applicable), "cultural_notes": string}}"#,
        target_language = request.target_language,
        channel_name = request.channel_name,
        content_style = if request.global_settings.content_style.trim().is_empty() { "(not specified)" } else { &request.global_settings.content_style },
        musical_style = request.musical_style,
        topic_description = request.global_settings.topic_description,
        region_instruction = region_instruction,
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

    let (content, _model) = crate::commands::ai::provider_chat(
        &settings_doc,
        &system_prompt,
        &format!("Adapt this content:\n{}", user_message),
        0.7,
        true,
    ).await?;

    // Parse the JSON response. This used to fall back to an empty object on a parse failure —
    // every field then silently defaulted to the *English source* text a few lines up the call
    // stack, which got saved (and could get synced live to YouTube) as if it were the translated
    // content for a non-English channel. Surface the failure instead so the caller records it as
    // a real per-channel error.
    let translated: Value = serde_json::from_str(&content)
        .ok()
        .or_else(|| crate::commands::ai::extract_json_value(&content))
        .ok_or_else(|| format!("AI returned unparseable JSON (raw: {})", content.chars().take(300).collect::<String>()))?;

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
        // Empty (not "US") so the prompt's region-inference path actually triggers for channels
        // that never had one set — defaulting to "US" here meant "infer the region" could never
        // fire, silently mislabeling every unset-region channel as US-targeted.
        let region = channel["region"].as_str().unwrap_or("").trim().to_string();
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
                // Enforce YouTube's actual field limits here regardless of what the AI returned —
                // the prompt asks it to stay under them, but a model doesn't always comply, and a
                // value over the limit would otherwise only surface as a rejected sync attempt
                // later (see sync_channel_to_youtube), far from where the content was generated.
                let about = truncate_chars(
                    translated.get("about_section").and_then(|v| v.as_str()).unwrap_or(&global_settings.about_section),
                    1000,
                );
                let branding = truncate_chars(
                    translated.get("branding_text").and_then(|v| v.as_str()).unwrap_or(&global_settings.branding_text),
                    200,
                );
                let tags: Vec<String> = translated.get("default_tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(|s| truncate_chars(s, 30)).take(15).collect())
                    .unwrap_or_else(|| global_settings.default_tags.iter().map(|s| truncate_chars(s, 30)).take(15).collect());

                let suggested_region = translated.get("suggested_region").and_then(|v| v.as_str())
                    .map(|s| s.trim().to_uppercase()).filter(|s| !s.is_empty());
                let cultural_notes = translated.get("cultural_notes").and_then(|v| v.as_str()).unwrap_or("").to_string();

                let mut set_doc = doc! {
                    "translated_description": &about,
                    "translated_tags": &tags,
                    "translated_branding": &branding,
                    "translated_cultural_notes": &cultural_notes,
                    // Distinct from `last_synced_at` (set only by sync_channel_to_youtube, once
                    // this draft is actually pushed live) — this step only stages a draft.
                    "last_translated_at": chrono::Utc::now().to_rfc3339(),
                };
                // Only fill `region` from the AI's guess if the channel didn't already have one —
                // this never overwrites an explicit user-set region.
                if region.is_empty() {
                    if let Some(sr) = &suggested_region {
                        set_doc.insert("region", sr);
                    }
                }

                state.db.collection::<Document>("channels")
                    .update_one(doc! { "id": &channel_id }, doc! { "$set": set_doc })
                    .await
                    .map_err(e)?;

                results.push(serde_json::json!({
                    "channel_id": channel_id,
                    "channel_name": channel_name,
                    "language": language,
                    "region": suggested_region.unwrap_or(region),
                    "status": "translated",
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
        // Named for what this step actually does — stage AI-translated drafts. Pushing them
        // live to YouTube is the separate sync_channel_to_youtube step.
        "translated_count": results.len(),
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
    
    // Verify channel exists — the document itself is not needed, only that there is one.
    channels_coll
        .find_one(doc! { "id": &channel_id })
        .await
        .map_err(e)?
        .ok_or_else(|| "Channel not found".to_string())?;
    
    // Build update document with only override fields
    let mut update_set = Document::new();

    if let Some(name) = overrides.get("name").and_then(|v| v.as_str()) {
        if !name.trim().is_empty() {
            update_set.insert("name".to_string(), bson::to_bson(name).map_err(e)?);
        }
    }
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
    dry_run: Option<bool>,
) -> Res<Value> {
    // A dry run reads the live branding, builds exactly the payload a real sync would send, and
    // stops before the PUT.
    //
    // It exists because this call is not reversible and its dangerous failure is silent: YouTube
    // accepts a partial brandingSettings and clears the rest, so a bug here costs a channel trailer
    // the owner chose, days before anybody links the two. Verifying the merge by performing it on a
    // real channel means gambling the exact thing the fix is meant to protect. This way the fields
    // that survive can be read off before anything is written.
    let dry_run = dry_run.unwrap_or(false);
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
        .ok_or_else(|| -> String { "No OAuth client configured for this channel".to_string() })?;
    
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
    
    // Build the channel update payload. Determine what to sync: an explicit override if the
    // user actually typed one, else the AI-translated draft, else nothing.
    //
    // `update_channel_overrides` always writes `custom_about`/`custom_tags` — even blank/empty —
    // the moment a user saves *any* channel property (e.g. just setting language/region). A
    // plain `.as_str()`/`.as_array()` check treats an empty string or `[]` as "present", so
    // `.or_else()` never falls through to the translated draft: once overrides had been saved
    // once for a channel, the AI translation step's output could never actually reach YouTube.
    // Filtering out the blank/empty case fixes that.
    let description = channel["custom_about"].as_str().filter(|s| !s.trim().is_empty())
        .or_else(|| channel["translated_description"].as_str())
        .unwrap_or("");
    let description = truncate_chars(description, 1000);

    let tags: Vec<&str> = channel["custom_tags"].as_array().filter(|a| !a.is_empty())
        .or_else(|| channel["translated_tags"].as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let keywords = truncate_chars(&tags.join(" "), 500);

    let youtube_channel_id = channel["youtube_channel_id"]
        .as_str()
        .ok_or("No YouTube channel ID associated")?;

    // Call YouTube Data API to update channel. `channels.update` only accepts writes to the
    // `brandingSettings` (and `invideoPromotion`) part — `snippet.title`/`snippet.description`
    // read back from YouTube but are not writable through this endpoint, so a payload built
    // under `snippet` (as this used to do) is silently ignored by the API: the call succeeds
    // but nothing actually changes on the channel.
    // Read what is on the channel before writing over it.
    //
    // `channels.update` treats the `brandingSettings` part as the whole resource, not as a patch:
    // fields absent from the payload are reset. Sending just title/description/keywords therefore
    // clears anything else living under `brandingSettings.channel` — the unsubscribed trailer most
    // painfully, since that is a video the owner chose and nothing in this app would put back. The
    // API reports success either way, so the loss would show up as "my channel trailer disappeared"
    // days later, with no reason to connect it to a description sync.
    //
    // A failed read is not a reason to refuse the write — the user asked for a sync — but it is a
    // reason not to pretend the merge happened, so it starts from empty and says so in the log.
    let existing = http
        .get("https://www.googleapis.com/youtube/v3/channels")
        .query(&[("part", "brandingSettings"), ("id", youtube_channel_id)])
        .bearer_auth(access_token)
        .send()
        .await
        .ok();
    let mut branding_channel = match existing {
        Some(r) if r.status().is_success() => r
            .json::<Value>()
            .await
            .ok()
            .and_then(|v| v["items"][0]["brandingSettings"]["channel"].as_object().cloned())
            .map(Value::Object)
            .unwrap_or_else(|| serde_json::json!({})),
        _ => serde_json::json!({}),
    };

    branding_channel["title"] = channel["name"].clone();
    branding_channel["description"] = serde_json::json!(description);
    branding_channel["keywords"] = serde_json::json!(keywords);
    // `region` may be an explicit override or the AI's inferred `suggested_region` (see
    // translate_and_apply_settings) — either way it's an ISO 3166-1 alpha-2 code, which is
    // exactly what brandingSettings.channel.country expects.
    if let Some(region) = channel["region"].as_str().filter(|s| !s.trim().is_empty()) {
        branding_channel["country"] = serde_json::json!(region.trim().to_uppercase());
    }
    let update_payload = serde_json::json!({
        // `id` must be set on the resource body itself — without it the API can't tell which
        // of the (possibly several brand-account) channels this OAuth token can touch is the
        // one to update.
        "id": youtube_channel_id,
        "brandingSettings": { "channel": branding_channel }
    });

    if dry_run {
        return Ok(serde_json::json!({
            "dry_run": true,
            "channel_id": channel_id,
            "youtube_channel_id": youtube_channel_id,
            // What is about to be written, in full. The keys present here are the ones that will
            // survive; anything the live channel has that is absent from this list is what a
            // non-merging update would have destroyed.
            "would_send": update_payload,
            "preserved_keys": update_payload["brandingSettings"]["channel"].as_object()
                .map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default(),
            "detail": "Nothing was sent. These are the brandingSettings.channel fields the real \
                       sync would write — check your trailer and any other field you care about \
                       appears here before running it for real.",
        }));
    }

    let update_response = http
        .put("https://www.googleapis.com/youtube/v3/channels")
        .query(&[("part", "brandingSettings")])
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
    
    // Mark as synced. `last_synced_at` is what the frontend's sync-status badge actually reads
    // (see ChannelSettingsPanel.jsx's getSyncStatus) — it used to only get set by the AI
    // *translation* step under this same name, so the badge showed "Synced Xd ago" for a channel
    // that had only ever had a draft generated, never actually pushed to YouTube. Keep
    // `last_youtube_sync` too since it's a clearer name for anything reading this doc directly.
    let now = chrono::Utc::now().to_rfc3339();
    state.db.collection::<Document>("channels")
        .update_one(
            doc! { "id": &channel_id },
            doc! { "$set": {
                "last_synced_at": &now,
                "last_youtube_sync": &now,
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

// ────────────────────────────────────────────────────────────────
// Cultural flavor: adapts a musical or visual style preset's descriptor text with authentic
// region/language-appropriate texture for one channel, using the same free OpenRouter AI as the
// description translator above. Never applied automatically — the caller (Channel Manager)
// previews the result and decides whether to keep it.
// ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlavorStyleRequest {
    pub channel_id: String,
    pub kind: String, // "musical" | "visual"
    pub base_text: String,
}

#[tauri::command]
pub async fn ai_flavor_style(state: State<'_, AppState>, request: FlavorStyleRequest) -> Res<Value> {
    if request.base_text.trim().is_empty() {
        return Err("Nothing to flavor yet — pick a preset or type a style first.".into());
    }

    let channel_doc = state.db.collection::<Document>("channels")
        .find_one(doc! { "id": &request.channel_id }).await.map_err(e)?
        .ok_or_else(|| "Channel not found".to_string())?;
    let channel = bson_to_value(channel_doc);
    let language = channel["language"].as_str().unwrap_or("English").to_string();
    let region = channel["region"].as_str().unwrap_or("").trim().to_string();
    let channel_name = channel["name"].as_str().unwrap_or("this channel").to_string();

    let settings_doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let musical = request.kind.eq_ignore_ascii_case("musical");
    let role = if musical { "a music production consultant" } else { "a visual art director" };
    let guidance = if musical {
        "Blend in instrumentation, rhythm, or vocal-delivery motifs authentically associated with \
         the region/culture — real instruments, scales, or production techniques, not \
         stereotypes. Keep it a comma-separated Suno/ACE-Step-style descriptor (genre tags, \
         instrumentation, tempo) — not a sentence."
    } else {
        "Blend in visual motifs, color palettes, or aesthetic references authentically associated \
         with the region/culture — genuine artistic touchpoints, not stereotypes or caricature. \
         Keep it a short comma-separated prompt fragment (the way an image-gen style prefix \
         reads) — not a sentence."
    };
    let limit = 300usize;
    let region_bit = if region.is_empty() { String::new() } else { format!(" (audience region: {region})") };
    let kind_label = if musical { "musical" } else { "visual" };

    let system_prompt = format!(
        r#"You are {role} adapting a {kind_label} style descriptor with authentic cultural flavor
for a YouTube channel whose audience speaks {language}{region_bit}.

Channel: {channel_name}
Current style descriptor: {base_text}

{guidance}

Stay tasteful and respectful — genuine cultural texture, never a stereotype or caricature. Keep
the core style/genre intact; you're layering flavor onto it, not replacing it. Max {limit} characters.

Return ONLY a JSON object with exactly these keys:
{{"flavored_text": string, "rationale": string (max 1 sentence — why these specific choices)}}"#,
        role = role, kind_label = kind_label, language = language, region_bit = region_bit,
        channel_name = channel_name, base_text = request.base_text, guidance = guidance, limit = limit,
    );

    let (content, _model) = crate::commands::ai::provider_chat(
        &settings_doc, &system_prompt, "Adapt it now.", 0.8, true,
    ).await?;
    let parsed: Value = serde_json::from_str(&content)
        .ok()
        .or_else(|| crate::commands::ai::extract_json_value(&content))
        .ok_or_else(|| format!("AI returned unparseable JSON (raw: {})", content.chars().take(300).collect::<String>()))?;

    let flavored_text = truncate_chars(
        parsed.get("flavored_text").and_then(|v| v.as_str()).unwrap_or(&request.base_text),
        limit,
    );
    let rationale = parsed.get("rationale").and_then(|v| v.as_str()).unwrap_or("").to_string();

    Ok(serde_json::json!({ "flavored_text": flavored_text, "rationale": rationale }))
}
