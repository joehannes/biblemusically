use crate::{
    models::{Channel, ChannelCreate},
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
