use crate::{
    models::{now_iso, Project, ProjectCreate},
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
pub async fn list_projects(state: State<'_, AppState>) -> Res<Vec<Value>> {
    use futures_util::StreamExt;
    let mut cursor = state.db.collection::<Document>("projects")
        .find(doc! {}).sort(doc! { "created_at": -1 })
        .await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }
    Ok(out)
}

#[tauri::command]
pub async fn create_project(state: State<'_, AppState>, body: ProjectCreate) -> Res<Value> {
    let p = Project {
        id: Uuid::new_v4().to_string(),
        name: body.name,
        topic: body.topic,
        schedule: body.schedule,
        multi_language: true,
        multi_style: true,
        languages: vec![],
        styles: vec![],
        created_at: now_iso(),
    };
    let bson = bson::to_document(&p).map_err(e)?;
    state.db.collection::<Document>("projects").insert_one(bson).await.map_err(e)?;
    Ok(serde_json::to_value(&p).map_err(e)?)
}

#[tauri::command]
pub async fn get_project(state: State<'_, AppState>, pid: String) -> Res<Value> {
    let doc = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &pid }).await.map_err(e)?
        .ok_or_else(|| "not found".to_string())?;
    Ok(bson_to_value(doc))
}

#[tauri::command]
pub async fn update_project(state: State<'_, AppState>, pid: String, body: Value) -> Res<Value> {
    let mut body = body;
    if let Some(obj) = body.as_object_mut() {
        obj.remove("id");
        obj.remove("_id");
    }
    let bson = bson::to_bson(&body).map_err(e)?;
    state.db.collection::<Document>("projects")
        .update_one(doc! { "id": &pid }, doc! { "$set": bson })
        .await.map_err(e)?;
    let doc = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &pid }).await.map_err(e)?
        .ok_or_else(|| "not found".to_string())?;
    Ok(bson_to_value(doc))
}

#[tauri::command]
pub async fn delete_project(state: State<'_, AppState>, pid: String) -> Res<Value> {
    state.db.collection::<Document>("projects")
        .delete_one(doc! { "id": &pid }).await.map_err(e)?;
    // Cascade delete songs + sections
    use futures_util::StreamExt;
    let mut cursor = state.db.collection::<Document>("songs")
        .find(doc! { "project_id": &pid }).await.map_err(e)?;
    let mut song_ids = Vec::new();
    while let Some(Ok(d)) = cursor.next().await {
        if let Some(id) = d.get_str("id").ok() { song_ids.push(id.to_string()); }
    }
    for sid in &song_ids {
        state.db.collection::<Document>("sections")
            .delete_many(doc! { "song_id": sid }).await.map_err(e)?;
    }
    state.db.collection::<Document>("songs")
        .delete_many(doc! { "project_id": &pid }).await.map_err(e)?;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn import_lyrics(state: State<'_, AppState>, pid: String, body: Value) -> Res<Value> {
    let project = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &pid }).await.map_err(e)?
        .ok_or_else(|| "project missing".to_string())?;

    let proj_val = bson_to_value(project);
    let mut languages: std::collections::HashSet<String> = proj_val["languages"]
        .as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let mut styles: std::collections::HashSet<String> = proj_val["styles"]
        .as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();

    let items = body["items"].as_array().ok_or("expected items: list")?.clone();
    let mut created = Vec::new();
    for it in &items {
        use crate::models::Song;
        let song = Song {
            id: Uuid::new_v4().to_string(),
            project_id: pid.clone(),
            title: it["title"].as_str().unwrap_or("Untitled").to_string(),
            language: it["language"].as_str().unwrap_or("English").to_string(),
            styles: it["styles"].as_str().unwrap_or("").to_string(),
            lyrics: it["lyrics"].as_str().unwrap_or("").to_string(),
            annotations: it["annotations"].as_str().unwrap_or("").to_string(),
            image_styles: it["image_styles"].as_str().unwrap_or("").to_string(),
            audio_url: None,
            video_url: None,
            duration: 0.0,
            status: "draft".into(),
            created_at: now_iso(),
        };
        languages.insert(song.language.clone());
        styles.insert(song.styles.clone());
        let bson = bson::to_document(&song).map_err(e)?;
        state.db.collection::<Document>("songs").insert_one(bson).await.map_err(e)?;
        created.push(serde_json::to_value(&song).map_err(e)?);
    }

    let mut langs_sorted: Vec<String> = languages.into_iter().collect();
    langs_sorted.sort();
    let mut styles_sorted: Vec<String> = styles.into_iter().collect();
    styles_sorted.sort();
    state.db.collection::<Document>("projects")
        .update_one(doc! { "id": &pid },
            doc! { "$set": { "languages": langs_sorted, "styles": styles_sorted } })
        .await.map_err(e)?;
    Ok(serde_json::json!({ "created": created.len(), "songs": created }))
}
