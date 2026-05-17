use crate::{
    helpers::mask_secret,
    models::{now_iso, OAuthClient, OAuthClientCreate},
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

// ────────────────────────────────────────────────────────────────
// OAuth Clients CRUD
// ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_oauth_clients(state: State<'_, AppState>) -> Res<Vec<Value>> {
    use futures_util::StreamExt;
    let mut cursor = state.db.collection::<Document>("oauth_clients")
        .find(doc! {}).sort(doc! { "created_at": -1 })
        .await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { out.push(mask_secret(bson_to_value(d))); }
    Ok(out)
}

#[tauri::command]
pub async fn create_oauth_client(state: State<'_, AppState>, body: OAuthClientCreate) -> Res<Value> {
    let c = OAuthClient {
        id: Uuid::new_v4().to_string(),
        label: body.label,
        client_id: body.client_id,
        client_secret: body.client_secret,
        redirect_uri: body.redirect_uri,
        languages: body.languages,
        notes: body.notes,
        created_at: now_iso(),
    };
    let bson = bson::to_document(&c).map_err(e)?;
    state.db.collection::<Document>("oauth_clients").insert_one(bson).await.map_err(e)?;
    Ok(mask_secret(serde_json::to_value(&c).map_err(e)?))
}

#[tauri::command]
pub async fn update_oauth_client(
    state: State<'_, AppState>,
    oid: String,
    mut body: Value,
) -> Res<Value> {
    if let Some(obj) = body.as_object_mut() {
        obj.remove("id");
        obj.remove("_id");
        obj.remove("created_at");
        // ignore masked secret
        if let Some(s) = obj.get("client_secret").and_then(|v| v.as_str()) {
            if s.starts_with('•') { obj.remove("client_secret"); }
        }
    }
    let bson = bson::to_bson(&body).map_err(e)?;
    state.db.collection::<Document>("oauth_clients")
        .update_one(doc! { "id": &oid }, doc! { "$set": bson })
        .await.map_err(e)?;
    let doc = state.db.collection::<Document>("oauth_clients")
        .find_one(doc! { "id": &oid }).await.map_err(e)?
        .ok_or_else(|| "missing".to_string())?;
    Ok(mask_secret(bson_to_value(doc)))
}

#[tauri::command]
pub async fn delete_oauth_client(state: State<'_, AppState>, oid: String) -> Res<Value> {
    state.db.collection::<Document>("oauth_clients")
        .delete_one(doc! { "id": &oid }).await.map_err(e)?;
    state.db.collection::<Document>("channels")
        .update_many(doc! { "oauth_client_id": &oid }, doc! { "$unset": { "oauth_client_id": "" } })
        .await.map_err(e)?;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn channel_picked_client(state: State<'_, AppState>, cid: String) -> Res<Value> {
    let ch_doc = state.db.collection::<Document>("channels")
        .find_one(doc! { "id": &cid }).await.map_err(e)?
        .ok_or_else(|| "channel missing".to_string())?;
    let ch = bson_to_value(ch_doc);
    let client = crate::jobs::pick_oauth_client(
        &state.db, &ch, ch["oauth_client_id"].as_str()
    ).await.map(mask_secret);
    Ok(serde_json::json!({ "client": client }))
}

// ────────────────────────────────────────────────────────────────
// OAuth start — build consent URL
// ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn oauth_start(
    state: State<'_, AppState>,
    cid: String,
    body: Option<Value>,
) -> Res<Value> {
    let ch_doc = state.db.collection::<Document>("channels")
        .find_one(doc! { "id": &cid }).await.map_err(e)?
        .ok_or_else(|| "channel not found".to_string())?;
    let ch = bson_to_value(ch_doc);
    let body = body.unwrap_or_default();
    let forced = body["oauth_client_id"].as_str();
    let client = crate::jobs::pick_oauth_client(&state.db, &ch, forced).await;
    let Some(client) = client else {
        return Ok(serde_json::json!({
            "url": "",
            "error": "No OAuth client configured. Add one in Channel Manager → OAuth Clients, or fill Settings → Google OAuth fields."
        }));
    };
    let cid_g    = client["client_id"].as_str().unwrap_or("");
    let redirect = client["redirect_uri"].as_str().unwrap_or("");
    let scope = "https://www.googleapis.com/auth/youtube.upload https://www.googleapis.com/auth/youtube.readonly";
    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?response_type=code&client_id={cid_g}&redirect_uri={redirect}&scope={}&access_type=offline&prompt=consent&state={cid}",
        scope.replace(' ', "%20"),
    );
    state.db.collection::<Document>("channels")
        .update_one(doc! { "id": &cid },
            doc! { "$set": { "oauth_client_id": client["id"].as_str().unwrap_or("") } })
        .await.map_err(e)?;
    Ok(serde_json::json!({
        "url": url,
        "oauth_client_id": client["id"],
        "label": client["label"],
    }))
}

// ────────────────────────────────────────────────────────────────
// OAuth callback — exchange code for tokens, update channel
// In Tauri the redirect lands back in the app via a custom URI scheme
// or a local server; this command handles the code exchange itself.
// ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn oauth_callback(
    state: State<'_, AppState>,
    code: String,
    channel_state: String, // the `state` param Google sends back
    error: Option<String>,
) -> Res<Value> {
    if let Some(err) = error {
        return Err(format!("OAuth error: {err}"));
    }
    if code.is_empty() || channel_state.is_empty() {
        return Err("missing code or state".into());
    }
    let ch_doc = state.db.collection::<Document>("channels")
        .find_one(doc! { "id": &channel_state }).await.map_err(e)?
        .ok_or_else(|| format!("channel {} not found", channel_state))?;
    let ch = bson_to_value(ch_doc);
    let client = crate::jobs::pick_oauth_client(&state.db, &ch, ch["oauth_client_id"].as_str())
        .await
        .ok_or_else(|| "no OAuth client configured for this channel".to_string())?;

    let cid_g = client["client_id"].as_str().unwrap_or("").to_string();
    let csec  = client["client_secret"].as_str().unwrap_or("").to_string();
    let ruri  = client["redirect_uri"].as_str().unwrap_or("").to_string();

    let http = reqwest::Client::new();
    let tr = http.post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code.as_str()),
            ("client_id", &cid_g),
            ("client_secret", &csec),
            ("redirect_uri", &ruri),
            ("grant_type", "authorization_code"),
        ])
        .send().await.map_err(e)?;
    if !tr.status().is_success() {
        return Err(format!("Token exchange failed: {}", tr.text().await.unwrap_or_default()));
    }
    let tokens: Value = tr.json().await.map_err(e)?;
    let refresh = tokens["refresh_token"].as_str().unwrap_or("").to_string();
    let access  = tokens["access_token"].as_str().unwrap_or("").to_string();

    let mut yt_channel_id = String::new();
    let mut subs: i64 = 0;
    let mut chan_title  = String::new();

    if !access.is_empty() {
        let yr = http.get("https://www.googleapis.com/youtube/v3/channels")
            .query(&[("part", "snippet,statistics"), ("mine", "true")])
            .bearer_auth(&access)
            .send().await;
        if let Ok(yr) = yr {
            if yr.status().is_success() {
                if let Ok(data) = yr.json::<Value>().await {
                    if let Some(items) = data["items"].as_array() {
                        if let Some(item) = items.first() {
                            yt_channel_id = item["id"].as_str().unwrap_or("").to_string();
                            chan_title     = item["snippet"]["title"].as_str().unwrap_or("").to_string();
                            subs           = item["statistics"]["subscriberCount"]
                                .as_str().unwrap_or("0").parse().unwrap_or(0);
                        }
                    }
                }
            }
        }
    }

    let mut update = doc! {
        "connected": true,
        "youtube_channel_id": &yt_channel_id,
        "subscriber_count": subs,
        "oauth_client_id": client["id"].as_str().unwrap_or(""),
    };
    if !refresh.is_empty() { update.insert("refresh_token", &refresh); }
    state.db.collection::<Document>("channels")
        .update_one(doc! { "id": &channel_state }, doc! { "$set": update })
        .await.map_err(e)?;

    Ok(serde_json::json!({
        "ok": true,
        "channel_title": chan_title,
        "youtube_channel_id": yt_channel_id,
        "label": client["label"],
    }))
}
