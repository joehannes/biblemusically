use crate::{
    models::{Channel, ChannelCreate},
    state::AppState,
};
use bson::{doc, Document};
use regex::Regex;
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
pub async fn list_channels(state: State<'_, AppState>) -> Res<Vec<Value>> {
    use futures_util::StreamExt;
    let mut cursor = state.db.collection::<Document>("channels")
        .find(doc! {}).await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }
    Ok(out)
}

#[tauri::command]
pub async fn create_channel(state: State<'_, AppState>, body: ChannelCreate) -> Res<Value> {
    let ch = Channel {
        id: Uuid::new_v4().to_string(),
        name: body.name,
        youtube_channel_id: body.youtube_channel_id,
        language: body.language,
        styles: body.styles,
        region: body.region,
        refresh_token: None,
        connected: false,
        avatar: None,
        subscriber_count: 0,
        oauth_client_id: None,
    };
    let bson = bson::to_document(&ch).map_err(e)?;
    state.db.collection::<Document>("channels").insert_one(bson).await.map_err(e)?;
    Ok(serde_json::to_value(&ch).map_err(e)?)
}

#[tauri::command]
pub async fn delete_channel(state: State<'_, AppState>, cid: String) -> Res<Value> {
    state.db.collection::<Document>("channels")
        .delete_one(doc! { "id": &cid }).await.map_err(e)?;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn oauth_complete_channel(
    state: State<'_, AppState>,
    cid: String,
    body: Value,
) -> Res<Value> {
    let token = body["refresh_token"].as_str()
        .ok_or("refresh_token required")?;
    let yt_id = body["youtube_channel_id"].as_str().unwrap_or("").to_string();
    let subs  = body["subscriber_count"].as_i64().unwrap_or(0);
    state.db.collection::<Document>("channels")
        .update_one(
            doc! { "id": &cid },
            doc! { "$set": {
                "refresh_token": token,
                "youtube_channel_id": &yt_id,
                "subscriber_count": subs,
                "connected": true,
            }},
        ).await.map_err(e)?;
    let doc = state.db.collection::<Document>("channels")
        .find_one(doc! { "id": &cid }).await.map_err(e)?
        .ok_or_else(|| "not found".to_string())?;
    Ok(bson_to_value(doc))
}

#[tauri::command]
pub async fn channels_connect_all_urls(state: State<'_, AppState>) -> Res<Value> {
    use futures_util::StreamExt;
    let mut cursor = state.db.collection::<Document>("channels")
        .find(doc! {}).await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await {
        let ch = bson_to_value(d);
        if ch["connected"].as_bool().unwrap_or(false)
            && ch["refresh_token"].is_string() { continue; }
        if let Some(client) = crate::jobs::pick_oauth_client(
            &state.db, &ch, ch["oauth_client_id"].as_str()
        ).await {
            let scope = "https://www.googleapis.com/auth/youtube.upload https://www.googleapis.com/auth/youtube.readonly";
            let url = format!(
                "https://accounts.google.com/o/oauth2/v2/auth?response_type=code&client_id={}&redirect_uri={}&scope={}&access_type=offline&prompt=consent&state={}",
                client["client_id"].as_str().unwrap_or(""),
                client["redirect_uri"].as_str().unwrap_or(""),
                scope.replace(' ', "%20"),
                ch["id"].as_str().unwrap_or(""),
            );
            state.db.collection::<Document>("channels")
                .update_one(
                    doc! { "id": ch["id"].as_str().unwrap_or("") },
                    doc! { "$set": { "oauth_client_id": client["id"].as_str().unwrap_or("") } },
                ).await.map_err(e)?;
            out.push(serde_json::json!({
                "channel_id": ch["id"],
                "name": ch["name"],
                "language": ch["language"],
                "url": url,
                "label": client["label"],
            }));
        }
    }
    Ok(serde_json::json!({ "items": out }))
}

// ─────────────────────────────────────────────────────────────────────
// Pure Rust YouTube channel discovery  –  no Node.js, no Puppeteer
// Uses reqwest to fetch YouTube pages and parse the embedded
// ytInitialData JSON payload to extract channel info.
// ─────────────────────────────────────────────────────────────────────

fn build_candidate_urls(user: &str) -> Vec<String> {
    let normalized = user.trim().to_string();
    let mut candidates = Vec::new();
    if normalized.is_empty() { return candidates; }
    if normalized.starts_with("http://") || normalized.starts_with("https://") {
        candidates.push(normalized.clone());
    } else if normalized.starts_with("@") {
        candidates.push(format!("https://www.youtube.com/{}", normalized));
        candidates.push(format!("https://www.youtube.com/{}/channels", normalized));
    } else if normalized.starts_with("UC") && normalized.len() >= 24 {
        candidates.push(format!("https://www.youtube.com/channel/{}", normalized));
        candidates.push(format!("https://www.youtube.com/channel/{}/channels", normalized));
    } else {
        candidates.push(format!("https://www.youtube.com/@{}", normalized));
        candidates.push(format!("https://www.youtube.com/c/{}", normalized));
        candidates.push(format!("https://www.youtube.com/user/{}", normalized));
    }
    candidates
}

fn parse_channel_id(text: &str) -> Option<String> {
    let re = Regex::new(r"youtu(?:\.be/|be\.com/(?:channel/|user/|c/|@))([A-Za-z0-9_-]+)").ok()?;
    re.captures(text).and_then(|c| c.get(1)).map(|m| m.as_str().to_string())
}

/// Find the ytInitialData JSON blob in a YouTube HTML page and parse it.
fn find_yt_initial_data(body: &str) -> Option<Value> {
    // Look for window["ytInitialData"] = { ... }; or window['ytInitialData'] = { ... };
    // or just ytInitialData = { ... };
    let markers = [
        "window[",
        "ytInitialData",
    ];

    for &first_mark in &markers {
        let Some(start_of_marker) = body.find(first_mark) else { continue };
        let after_mark = &body[start_of_marker..];

        // Find the '=' sign
        let Some(eq_pos) = after_mark.find('=') else { continue };
        let after_eq = &after_mark[eq_pos + 1..];

        // Now find the opening '{'
        let Some(open_brace) = after_eq.find('{') else { continue };
        let json_start = open_brace;
        let json_body = &after_eq[json_start..];

        // Count braces to find the matching closing brace, handling strings
        let mut depth: i32 = 0;
        let mut end_pos = 0;
        let mut in_string = false;
        let mut escaped = false;
        for (i, ch) in json_body.char_indices() {
            if escaped { escaped = false; continue; }
            if ch == '\\' { escaped = true; continue; }
            if ch == '"' { in_string = !in_string; continue; }
            if in_string { continue; }
            if ch == '{' { depth += 1; }
            else if ch == '}' { depth -= 1; }
            if depth == 0 { end_pos = i + 1; break; }
        }
        if end_pos == 0 { continue; }

        let json_str = &json_body[..end_pos];
        if json_str.len() < 3 { continue; }
        if let Ok(data) = serde_json::from_str::<Value>(json_str) {
            return Some(data);
        }
    }
    None
}

/// Extract channel items from the parsed ytInitialData structure.
fn extract_channels_from_data(data: &Value) -> Vec<Value> {
    let mut results = Vec::new();

    // Navigate: contents.twoColumnBrowseResultsRenderer.tabs[]
    let tabs = data
        .pointer("/contents/twoColumnBrowseResultsRenderer/tabs")
        .or_else(|| data.pointer("/contents"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    for tab in &tabs {
        let contents_paths = [
            "/tabRenderer/content/sectionListRenderer/contents",
            "/tabRenderer/content/richGridRenderer/contents",
        ];
        for path in &contents_paths {
            let Some(sections) = tab.pointer(path).and_then(|v| v.as_array()) else { continue };
            for section in sections {
                let items = section
                    .pointer("/itemSectionRenderer/contents")
                    .or_else(|| Some(section))
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();

                for item in &items {
                    // Try gridChannelRenderer, then channelRenderer
                    let cr = item
                        .pointer("/gridChannelRenderer")
                        .or_else(|| item.pointer("/channelRenderer"))
                        .or_else(|| item.pointer("/richItemRenderer/content/channelRenderer"));
                    let Some(cr) = cr else { continue };

                    let channel_id = cr["channelId"].as_str().unwrap_or("").to_string();
                    let title = cr["title"]["simpleText"]
                        .as_str()
                        .or_else(|| cr["title"]["runs"].as_array()
                            .and_then(|a| a.first())
                            .and_then(|r| r["text"].as_str()))
                        .unwrap_or("")
                        .to_string();
                    if channel_id.is_empty() || title.is_empty() { continue; }

                    let subscriber_text = cr["subscriberCountText"]["simpleText"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    let avatar = cr["thumbnail"]["thumbnails"]
                        .as_array()
                        .and_then(|t| t.first())
                        .and_then(|t| t["url"].as_str())
                        .unwrap_or("")
                        .to_string();

                    results.push(serde_json::json!({
                        "channel_id": channel_id,
                        "channel_url": format!("https://www.youtube.com/channel/{}", channel_id),
                        "title": title,
                        "subscriber_count": subscriber_text,
                        "avatar": if avatar.starts_with("http") { avatar } else { String::new() },
                    }));
                }
            }
        }
    }
    results
}

/// Scrape a YouTube page for channel info using only HTTP + HTML parsing.
async fn scrape_youtube_page(
    client: &reqwest::Client,
    url: &str,
    timeout: std::time::Duration,
) -> Result<Vec<Value>, String> {
    let resp = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/120.0.0.0 Safari/537.36")
        .header("Accept-Language", "en-US,en;q=0.9")
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {} for {}", resp.status(), url));
    }

    let body = resp.text().await.map_err(|e| format!("Read body failed: {}", e))?;

    // Attempt 1: extract channels from ytInitialData
    if let Some(data) = find_yt_initial_data(&body) {
        let channels = extract_channels_from_data(&data);
        if !channels.is_empty() {
            return Ok(channels);
        }
    }

    // Attempt 2: fallback – extract single channel info from meta tags
    let page_title = {
        Regex::new(r"<title>(.+?)</title>")
            .ok()
            .and_then(|re| re.captures(&body))
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
    };
    let channel_id = parse_channel_id(url).or_else(|| parse_channel_id(&body));
    let canonical_url = {
        Regex::new(r#"<link\s+rel="canonical"\s+href="([^"]+)""#)
            .ok()
            .and_then(|re| re.captures(&body))
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
    };

    if let Some(cid) = channel_id {
        let clean_title = page_title
            .map(|t| t.replace(" - YouTube", "").trim().to_string())
            .unwrap_or_default();
        return Ok(vec![serde_json::json!({
            "channel_id": cid,
            "channel_url": canonical_url.unwrap_or_else(|| format!("https://www.youtube.com/channel/{}", cid)),
            "title": clean_title,
            "subscriber_count": "",
            "avatar": "",
        })]);
    }

    Ok(Vec::new())
}

#[tauri::command]
pub async fn import_from_google_account(
    state: State<'_, AppState>,
    oauth_client_id: String,
) -> Res<Value> {
    use crate::commands::oauth::perform_oauth_loopback;
    let scope = "https://www.googleapis.com/auth/youtube.readonly https://www.googleapis.com/auth/youtube.force-ssl";
    // Validate OAuth client credentials before starting the loopback
    // (inline the validation logic to avoid cross-command coupling)
    let oauth_client_label: String;
    {
        let client_doc = state.db.collection::<Document>("oauth_clients")
            .find_one(doc! { "id": &oauth_client_id }).await.map_err(e)?
            .ok_or_else(|| format!("OAuth client '{}' not found", oauth_client_id))?;
        let client = crate::commands::oauth::bson_to_value(client_doc);
        let mut missing = Vec::new();
        let cid = client["client_id"].as_str().unwrap_or("");
        if cid.trim().is_empty() { missing.push("client_id"); }
        let csec = client["client_secret"].as_str().unwrap_or("");
        if csec.trim().is_empty() || csec.starts_with('•') { missing.push("client_secret"); }
        let redirect = client["redirect_uri"].as_str().unwrap_or("");
        if redirect.trim().is_empty() { missing.push("redirect_uri"); }
        if !missing.is_empty() {
            let label = client["label"].as_str().unwrap_or("Unknown");
            return Err(format!(
                "OAuth client '{}' has missing or invalid credentials: {}. Open the 'YouTube OAuth client pool' section above and edit this client to fill in all required fields.",
                label,
                missing.join(", ")
            ));
        }
        oauth_client_label = client["label"].as_str().unwrap_or("Unknown").to_string();
    }
    let tokens = perform_oauth_loopback(&state.db, &oauth_client_id, Some(scope.to_string())).await?;
    let refresh = tokens["refresh_token"].as_str().unwrap_or("").to_string();
    let access = tokens["access_token"].as_str().unwrap_or("").to_string();
    if access.is_empty() {
        return Err("OAuth flow did not produce an access token".into());
    }

    // Call YouTube Data API v3 to list all channels managed by the authenticated user
    let http = reqwest::Client::new();
    let mut all_channels: Vec<Value> = Vec::new();
    let mut next_page_token = String::new();

    loop {
        let mut params = vec![
            ("part".to_string(), "snippet,statistics,contentDetails".to_string()),
            ("mine".to_string(), "true".to_string()),
            ("maxResults".to_string(), "50".to_string()),
        ];
        if !next_page_token.is_empty() {
            params.push(("pageToken".to_string(), next_page_token.clone()));
        }
        let resp = http.get("https://www.googleapis.com/youtube/v3/channels")
            .query(&params)
            .bearer_auth(&access)
            .send().await.map_err(e)?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("YouTube API error {}: {}", status, body));
        }

        let data: Value = resp.json().await.map_err(e)?;
        if let Some(items) = data["items"].as_array() {
            for item in items {
                let channel_id = item["id"].as_str().unwrap_or("").to_string();
                let title = item["snippet"]["title"].as_str().unwrap_or("").to_string();
                let description = item["snippet"]["description"].as_str().unwrap_or("").to_string();
                let thumbnail = item["snippet"]["thumbnails"]
                    .as_object()
                    .and_then(|t| t.get("default"))
                    .and_then(|d| d.as_object())
                    .and_then(|d| d.get("url"))
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .to_string();
                let subscriber_count = item["statistics"]["subscriberCount"]
                    .as_str()
                    .unwrap_or("0")
                    .parse::<i64>()
                    .unwrap_or(0);
                let video_count = item["statistics"]["videoCount"]
                    .as_str()
                    .unwrap_or("0")
                    .parse::<i64>()
                    .unwrap_or(0);

                all_channels.push(serde_json::json!({
                    "channel_id": channel_id,
                    "title": title,
                    "description": description,
                    "thumbnail": thumbnail,
                    "subscriber_count": subscriber_count,
                    "video_count": video_count,
                }));
            }
        }

        next_page_token = data["nextPageToken"].as_str().unwrap_or("").to_string();
        if next_page_token.is_empty() {
            break;
        }
    }

    // ── Fix 2: Persist discovered channels into the DB with the refresh token ──
    // This means channels are immediately connected and ready for upload.
    let channels_coll = state.db.collection::<Document>("channels");
    let mut created_count = 0;
    let mut existing_count = 0;
    let mut created_channels = Vec::new();

    for ch_info in &all_channels {
        let channel_id = ch_info["channel_id"].as_str().unwrap_or("").trim().to_string();
        if channel_id.is_empty() {
            continue;
        }
        // Check if channel already exists
        let existing = channels_coll
            .find_one(doc! { "youtube_channel_id": &channel_id })
            .await
            .map_err(e)?;
        if existing.is_some() {
            // Update existing channel with refresh token + connected status if not already
            if !refresh.is_empty() {
                let _ = channels_coll
                    .update_one(
                        doc! { "youtube_channel_id": &channel_id },
                        doc! { "$set": {
                            "connected": true,
                            "refresh_token": &refresh,
                            "oauth_client_id": &oauth_client_id,
                        }},
                    )
                    .await;
            }
            existing_count += 1;
            continue;
        }
        // Create new channel with all fields filled
        let name = ch_info["title"].as_str().unwrap_or("Imported Channel").to_string();
        let sub_count = ch_info["subscriber_count"].as_i64().unwrap_or(0);
        let avatar = ch_info["thumbnail"].as_str().unwrap_or("").to_string();
        let new_ch = Channel {
            id: Uuid::new_v4().to_string(),
            name,
            youtube_channel_id: channel_id.clone(),
            language: "English".to_string(),
            styles: String::new(),
            region: String::new(),
            refresh_token: if refresh.is_empty() { None } else { Some(refresh.clone()) },
            connected: !refresh.is_empty(),
            avatar: if avatar.is_empty() { None } else { Some(avatar) },
            subscriber_count: sub_count,
            oauth_client_id: Some(oauth_client_id.clone()),
        };
        let bson = bson::to_document(&new_ch).map_err(e)?;
        channels_coll.insert_one(bson).await.map_err(e)?;
        created_count += 1;
        created_channels.push(serde_json::to_value(&new_ch).map_err(e)?);
    }

    Ok(serde_json::json!({
        "ok": true,
        "channels": all_channels,
        "count": all_channels.len(),
        "oauth_client_label": oauth_client_label,
        "created_count": created_count,
        "existing_count": existing_count,
        "tokens_available": !refresh.is_empty(),
    }))
}

/// Locate a resource file (same as locate_resource_file in settings.rs)
fn locate_switcher_script(name: &str) -> Option<std::path::PathBuf> {
    // Current working directory (dev mode from project root)
    if let Ok(cwd) = std::env::current_dir() {
        let p = cwd.join("src-tauri").join("packaging").join(name);
        if p.exists() { return Some(p); }
    }
    // TAURI_RESOURCE_DIR env var
    if let Ok(rd) = std::env::var("TAURI_RESOURCE_DIR") {
        let p = std::path::PathBuf::from(&rd).join(name);
        if p.exists() { return Some(p); }
        let p2 = std::path::PathBuf::from(&rd).join("packaging").join(name);
        if p2.exists() { return Some(p2); }
    }
    // Walk up from executable
    if let Ok(exe) = std::env::current_exe() {
        let mut path = exe.parent();
        while let Some(dir) = path {
            let checks = [
                dir.join("resources").join(name),
                dir.join(name),
                dir.join("packaging").join(name),
                dir.join("_up_").join("src-tauri").join("packaging").join(name),
                dir.join("_up_").join("packaging").join(name),
            ];
            for p in &checks {
                if p.exists() { return Some(p.clone()); }
            }
            if let Some(parent) = dir.parent() {
                let p2 = parent.join("src-tauri").join("packaging").join(name);
                if p2.exists() { return Some(p2); }
            }
            path = dir.parent();
        }
    }
    None
}

/// Connect ALL not-connected channels with a single OAuth flow.
/// Does one OAuth loopback, gets all channels from the YouTube API,
/// then applies the same refresh token to every existing unconnected channel.
#[tauri::command]
pub async fn connect_all_channels_one_shot(
    state: State<'_, AppState>,
    oauth_client_id: Option<String>,
) -> Res<Value> {
    use crate::commands::oauth::perform_oauth_loopback;
    use futures_util::StreamExt;

    // Find all unconnected channels
    let mut cursor = state.db.collection::<Document>("channels")
        .find(doc! { "$or": [
            { "connected": { "$ne": true } },
            { "refresh_token": { "$exists": false } },
            { "refresh_token": "" },
        ]})
        .await.map_err(e)?;
    let mut targets = Vec::new();
    while let Some(Ok(d)) = cursor.next().await {
        targets.push(bson_to_value(d));
    }
    if targets.is_empty() {
        return Ok(serde_json::json!({ "ok": true, "connected_count": 0, "message": "All channels already connected" }));
    }

    // Pick an OAuth client - use provided, or auto-pick from the first target channel
    let client = if let Some(ref forced_id) = oauth_client_id {
        let doc = state.db.collection::<Document>("oauth_clients")
            .find_one(doc! { "id": forced_id }).await.map_err(e)?
            .ok_or_else(|| format!("OAuth client '{}' not found", forced_id))?;
        Some(bson_to_value(doc))
    } else {
        // Try to auto-pick from the first unconnected channel
        if let Some(ch) = targets.first() {
            crate::jobs::pick_oauth_client(&state.db, ch, None).await
        } else {
            None
        }
    };

    let client = client.ok_or_else(|| {
        "No OAuth client configured. Add one in the 'YouTube OAuth client pool' section above.".to_string()
    })?;

    let client_label = client["label"].as_str().unwrap_or("OAuth client").to_string();

    // Do a single OAuth loopback
    let client_id_db = client["id"].as_str().unwrap_or("").to_string();
    let tokens = perform_oauth_loopback(&state.db, &client_id_db, None).await?;
    let refresh = tokens["refresh_token"].as_str().unwrap_or("").to_string();
    let access = tokens["access_token"].as_str().unwrap_or("").to_string();

    if access.is_empty() {
        return Err("OAuth flow did not produce an access token".into());
    }

    // Call YouTube API to get info for ALL channels managed by this account
    // so we can match them to our existing channels
    let http = reqwest::Client::new();
    let mut yt_channels: Vec<Value> = Vec::new();
    let mut next_page_token = String::new();

    loop {
        let mut params = vec![
            ("part".to_string(), "snippet".to_string()),
            ("mine".to_string(), "true".to_string()),
            ("maxResults".to_string(), "50".to_string()),
        ];
        if !next_page_token.is_empty() {
            params.push(("pageToken".to_string(), next_page_token.clone()));
        }
        let resp = http.get("https://www.googleapis.com/youtube/v3/channels")
            .query(&params)
            .bearer_auth(&access)
            .send().await.map_err(e)?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("YouTube API error {}: {}", status, body));
        }

        let data: Value = resp.json().await.map_err(e)?;
        if let Some(items) = data["items"].as_array() {
            for item in items {
                let channel_id = item["id"].as_str().unwrap_or("").to_string();
                let title = item["snippet"]["title"].as_str().unwrap_or("").to_string();
                let thumbnail = item["snippet"]["thumbnails"]["default"]["url"]
                    .as_str().unwrap_or("").to_string();
                yt_channels.push(serde_json::json!({
                    "channel_id": channel_id,
                    "title": title,
                    "thumbnail": thumbnail,
                }));
            }
        }
        next_page_token = data["nextPageToken"].as_str().unwrap_or("").to_string();
        if next_page_token.is_empty() { break; }
    }

    // Build a lookup map by youtube_channel_id
    let yt_lookup: std::collections::HashMap<String, Value> = yt_channels.into_iter()
        .map(|ch| (ch["channel_id"].as_str().unwrap_or("").to_string(), ch))
        .collect();

    // Update all target channels with the refresh token
    let channels_coll = state.db.collection::<Document>("channels");
    let mut connected_count = 0;
    let mut already_count = 0;

    for ch in &targets {
        let yt_id = ch["youtube_channel_id"].as_str().unwrap_or("").to_string();
        if refresh.is_empty() { break; }

        // Check if this channel is already connected
        if ch["connected"].as_bool().unwrap_or(false) && !ch["refresh_token"].as_str().unwrap_or("").is_empty() {
            already_count += 1;
            continue;
        }

        // Update with the refresh token
        let mut update = doc! {
            "connected": true,
            "refresh_token": &refresh,
            "oauth_client_id": &client_id_db,
        };

        // Add metadata from YouTube API response if available
        if !yt_id.is_empty() {
            if let Some(yt_ch) = yt_lookup.get(&yt_id) {
                if let Some(title) = yt_ch["title"].as_str() {
                    update.insert("name", title);
                }
                let thumb = yt_ch["thumbnail"].as_str().unwrap_or("");
                if !thumb.is_empty() {
                    update.insert("avatar", thumb);
                }
            }
        }

        channels_coll
            .update_one(
                doc! { "id": ch["id"].as_str().unwrap_or("") },
                doc! { "$set": update },
            )
            .await.map_err(e)?;
        connected_count += 1;
    }

    Ok(serde_json::json!({
        "ok": true,
        "connected_count": connected_count,
        "already_connected": already_count,
        "total_targets": targets.len(),
        "oauth_client_label": client_label,
        "tokens_available": !refresh.is_empty(),
    }))
}

#[tauri::command]
pub async fn discover_from_channel_switcher(
    _state: State<'_, AppState>,
    profile_dir: Option<String>,
    timeout_sec: Option<i32>,
) -> Res<Value> {
    use crate::helpers::resolve_node_executable;
    use tokio::process::Command;

    let script = locate_switcher_script("youtube-channel-switcher.js")
        .ok_or_else(|| "youtube-channel-switcher.js not found in resources/packaging".to_string())?;
    let node = resolve_node_executable()
        .ok_or_else(|| "Node.js is required for channel switcher discovery. Install Node.js and npm.".to_string())?;

    let timeout = timeout_sec.unwrap_or(120).max(30);
    let profile = profile_dir.unwrap_or_default();

    let mut cmd = Command::new(&node);
    cmd.arg(script.to_string_lossy().to_string());
    if !profile.is_empty() {
        cmd.arg(&profile);
    }
    cmd.arg(timeout.to_string());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let child = cmd.spawn().map_err(|e| format!("Failed to launch channel switcher script: {}", e))?;
    let output = child.wait_with_output().await.map_err(|e| format!("Failed to wait for script: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Try to parse stderr as JSON error
        if let Ok(err_json) = serde_json::from_str::<Value>(&stderr) {
            return Err(err_json["detail"].as_str().unwrap_or(&stderr).to_string());
        }
        return Err(format!("Channel switcher script failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: Value = serde_json::from_str(&stdout)
        .map_err(|e| format!("Failed to parse script output: {} - raw: {}", e, stdout.chars().take(200).collect::<String>()))?;

    if result["ok"].as_bool().unwrap_or(false) {
        Ok(result)
    } else {
        Err(result["detail"].as_str().unwrap_or(result["error"].as_str().unwrap_or("Unknown error")).to_string())
    }
}

#[tauri::command]
pub async fn discover_youtube_channels(
    users: Vec<String>,
    timeout_sec: Option<i32>,
) -> Res<Value> {
    let timeout = std::time::Duration::from_secs(timeout_sec.unwrap_or(180).max(30) as u64);
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let mut discovered = Vec::new();
    let mut errors = Vec::new();

    for user in &users {
        let targets = build_candidate_urls(user);
        let mut matched = false;

        for target in &targets {
            if matched { break; }

            match scrape_youtube_page(&client, target, timeout).await {
                Ok(channels) if !channels.is_empty() => {
                    discovered.push(serde_json::json!({
                        "query": user,
                        "source": target,
                        "channels": channels,
                    }));
                    matched = true;
                }
                Ok(_) => {
                    // Try the /channels sub-page
                    let channels_url = format!("{}/channels", target.trim_end_matches('/'));
                    match scrape_youtube_page(&client, &channels_url, timeout).await {
                        Ok(sub_channels) if !sub_channels.is_empty() => {
                            discovered.push(serde_json::json!({
                                "query": user,
                                "source": channels_url,
                                "channels": sub_channels,
                            }));
                            matched = true;
                        }
                        Ok(_) => {}
                        Err(e) => {
                            errors.push(serde_json::json!({
                                "query": user,
                                "target": target,
                                "error": e,
                            }));
                        }
                    }
                }
                Err(e) => {
                    errors.push(serde_json::json!({
                        "query": user,
                        "target": target,
                        "error": e,
                    }));
                }
            }
        }

        if !matched {
            errors.push(serde_json::json!({
                "query": user,
                "error": "Could not discover channels for this user/URL",
            }));
        }
    }

    Ok(serde_json::json!({
        "ok": true,
        "requested_users": users,
        "discovered": discovered,
        "errors": errors,
    }))
}

#[tauri::command]
pub async fn import_discovered_channels(
    state: State<'_, AppState>,
    channels: Vec<Value>,
) -> Res<Value> {
    let coll = state.db.collection::<Document>("channels");
    let mut inserted = 0;
    let mut skipped = 0;
    let mut duplicates = Vec::new();

    for channel in channels {
        let channel_id = channel["channel_id"].as_str().unwrap_or("").trim().to_string();
        let name = channel["title"]
            .as_str()
            .or_else(|| channel["name"].as_str())
            .unwrap_or("Imported Channel")
            .to_string();
        if channel_id.is_empty() {
            skipped += 1;
            continue;
        }
        if let Ok(Some(_)) = coll.find_one(doc! { "youtube_channel_id": &channel_id }).await {
            duplicates.push(channel_id.clone());
            skipped += 1;
            continue;
        }
        let ch = Channel {
            id: Uuid::new_v4().to_string(),
            name,
            youtube_channel_id: channel_id.clone(),
            language: channel["language"].as_str().unwrap_or("English").to_string(),
            styles: channel["styles"].as_str().unwrap_or("").to_string(),
            region: channel["region"].as_str().unwrap_or("").to_string(),
            refresh_token: None,
            connected: false,
            avatar: channel["avatar"].as_str().map(|s| s.to_string()),
            subscriber_count: channel["subscriber_count"].as_i64().unwrap_or(0),
            oauth_client_id: None,
        };
        let bson = bson::to_document(&ch).map_err(e)?;
        coll.insert_one(bson).await.map_err(e)?;
        inserted += 1;
    }

    Ok(serde_json::json!({
        "ok": true,
        "inserted": inserted,
        "skipped": skipped,
        "duplicates": duplicates,
    }))
}

#[tauri::command]
pub async fn refresh_all_channel_metadata(state: State<'_, AppState>) -> Res<Value> {
    use futures_util::StreamExt;
    let mut cursor = state.db
        .collection::<Document>("channels")
        .find(doc! {})
        .await
        .map_err(e)?;
    let mut updated = 0;
    let mut failed = Vec::new();

    while let Some(Ok(doc)) = cursor.next().await {
        let ch = bson_to_value(doc.clone());
        let connected = ch["connected"].as_bool().unwrap_or(false);
        let yt_id = ch["youtube_channel_id"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let refresh_token = ch["refresh_token"].as_str().unwrap_or("").to_string();
        if !connected || refresh_token.is_empty() || yt_id.is_empty() {
            continue;
        }
        let client = crate::jobs::pick_oauth_client(
            &state.db,
            &ch,
            ch["oauth_client_id"].as_str(),
        )
        .await;
        let Some(client) = client else {
            failed.push(serde_json::json!({ "id": ch["id"], "reason": "missing_oauth_client" }));
            continue;
        };
        let client_id = client["client_id"].as_str().unwrap_or("");
        let client_secret = client["client_secret"].as_str().unwrap_or("");
        if client_id.is_empty() || client_secret.is_empty() {
            failed.push(serde_json::json!({ "id": ch["id"], "reason": "missing_client_secret" }));
            continue;
        }
        let http = reqwest::Client::new();
        let token_res = http
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("client_id", client_id),
                ("client_secret", client_secret),
                ("refresh_token", &refresh_token),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await;
        let access_token = match token_res {
            Ok(resp) if resp.status().is_success() => {
                let body: Value = resp.json().await.map_err(e)?;
                body["access_token"].as_str().unwrap_or("").to_string()
            }
            Ok(resp) => {
                failed.push(serde_json::json!({
                    "id": ch["id"],
                    "reason": format!("token_refresh_failed:{}", resp.status()),
                }));
                continue;
            }
            Err(err) => {
                failed.push(serde_json::json!({ "id": ch["id"], "reason": err.to_string() }));
                continue;
            }
        };
        if access_token.is_empty() {
            failed.push(serde_json::json!({ "id": ch["id"], "reason": "empty_access_token" }));
            continue;
        }
        let detail_res = http
            .get("https://www.googleapis.com/youtube/v3/channels")
            .query(&[("part", "snippet,statistics"), ("id", &yt_id)])
            .bearer_auth(&access_token)
            .send()
            .await;
        let detail = match detail_res {
            Ok(resp) if resp.status().is_success() => resp.json::<Value>().await.map_err(e)?,
            Ok(resp) => {
                failed.push(serde_json::json!({
                    "id": ch["id"],
                    "reason": format!("youtube_api:{}", resp.status()),
                }));
                continue;
            }
            Err(err) => {
                failed.push(serde_json::json!({ "id": ch["id"], "reason": err.to_string() }));
                continue;
            }
        };
        let item = detail["items"]
            .as_array()
            .and_then(|arr| arr.first())
            .cloned();
        if let Some(item) = item {
            let title = item["snippet"]["title"]
                .as_str()
                .unwrap_or("")
                .to_string();
            let subs = item["statistics"]["subscriberCount"]
                .as_str()
                .unwrap_or("0")
                .parse::<i64>()
                .unwrap_or(0);
            let avatar = item["snippet"]["thumbnails"]["default"]["url"]
                .as_str()
                .map(|s| s.to_string());
            state
                .db
                .collection::<Document>("channels")
                .update_one(
                    doc! { "id": ch["id"].as_str().unwrap_or("") },
                    doc! { "$set": {
                        "name": title,
                        "subscriber_count": subs,
                        "avatar": avatar,
                    }},
                )
                .await
                .map_err(e)?;
            updated += 1;
        }
    }

    Ok(serde_json::json!({ "ok": true, "updated": updated, "failed": failed }))
}