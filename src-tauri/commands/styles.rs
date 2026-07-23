// Style presets, preset packs, and per-channel sticky styles.
//
// A **style preset** is a named bundle of ComfyUI image parameters (`params`, using the same
// `comfyui_*` keys as Settings) grouped into a **pack**. A **channel style** pins a set of those
// params to a YouTube channel so every image generated for that channel's language automatically
// inherits the look — persisted cross-session in the `channel_styles` collection.

use crate::state::AppState;
use bson::{doc, Document};
use serde_json::{json, Value};
use tauri::State;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

fn to_value(d: Document) -> Value {
    let mut m = serde_json::Map::new();
    for (k, v) in d {
        if k == "_id" { continue; }
        if let Ok(jv) = bson::from_bson::<Value>(v) { m.insert(k, jv); }
    }
    Value::Object(m)
}

/// The `comfyui_*` keys a preset / channel style may carry. Kept explicit so a malformed payload
/// can't inject arbitrary settings, and so resolution knows exactly what to overlay.
const STYLE_KEYS: &[&str] = &[
    "comfyui_style", "comfyui_ckpt", "comfyui_steps", "comfyui_cfg",
    "comfyui_width", "comfyui_height", "comfyui_negative", "comfyui_ip_weight",
    "comfyui_prompt_prefix",
];

fn sanitize_params(raw: &Value) -> Document {
    let mut d = Document::new();
    if let Some(obj) = raw.as_object() {
        for k in STYLE_KEYS {
            if let Some(v) = obj.get(*k) {
                if let Ok(b) = bson::to_bson(v) { d.insert(*k, b); }
            }
        }
    }
    d
}

fn builtin_presets() -> Vec<Value> {
    // Two starter packs; users add more from the Style Studio.
    vec![
        json!({ "id":"builtin-photoreal-cinematic", "name":"Photoreal Cinematic", "pack":"Cinematic", "builtin":true,
            "params":{ "comfyui_style":"photoreal", "comfyui_steps":32, "comfyui_cfg":6.5, "comfyui_width":1024, "comfyui_height":1024,
                       "comfyui_prompt_prefix":"cinematic composition, dramatic film lighting, subtle film grain", "comfyui_negative":"lowres, oversaturated" } }),
        json!({ "id":"builtin-photoreal-soft", "name":"Photoreal Soft Portrait", "pack":"Cinematic", "builtin":true,
            "params":{ "comfyui_style":"photoreal", "comfyui_steps":30, "comfyui_cfg":5.5, "comfyui_width":896, "comfyui_height":1152,
                       "comfyui_prompt_prefix":"soft natural window light, shallow depth of field, gentle tones", "comfyui_ip_weight":0.8 } }),
        json!({ "id":"builtin-comic-bold", "name":"Bold Comic", "pack":"Illustrated", "builtin":true,
            "params":{ "comfyui_style":"comic", "comfyui_steps":28, "comfyui_cfg":7.0, "comfyui_width":1024, "comfyui_height":1024,
                       "comfyui_prompt_prefix":"bold clean linework, vivid saturated palette, dynamic angle" } }),
        json!({ "id":"builtin-graphic-novel-noir", "name":"Graphic Novel Noir", "pack":"Illustrated", "builtin":true,
            "params":{ "comfyui_style":"graphic_novel", "comfyui_steps":30, "comfyui_cfg":6.5, "comfyui_width":1024, "comfyui_height":1024,
                       "comfyui_prompt_prefix":"high-contrast noir shading, moody rim light, textured ink" } }),
        json!({ "id":"builtin-watercolor-dream", "name":"Watercolor Dream", "pack":"Illustrated", "builtin":true,
            "params":{ "comfyui_style":"watercolor", "comfyui_steps":30, "comfyui_cfg":6.0, "comfyui_width":1024, "comfyui_height":1024,
                       "comfyui_prompt_prefix":"loose expressive washes, luminous color bleed" } }),
    ]
}

#[tauri::command]
pub async fn list_style_presets(state: State<'_, AppState>) -> Res<Value> {
    let coll = state.db.collection::<Document>("style_presets");
    // Seed built-ins once if the collection is empty.
    if coll.count_documents(doc! {}).await.map_err(e)? == 0 {
        for p in builtin_presets() {
            if let Ok(mut d) = bson::to_document(&p) {
                let id = p["id"].as_str().unwrap_or("").to_string();
                d.insert("_id", &id);
                let _ = coll.insert_one(d).await;
            }
        }
    }
    use futures_util::StreamExt;
    let mut cursor = coll.find(doc! {}).await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { out.push(to_value(d)); }
    Ok(json!({ "presets": out }))
}

#[tauri::command]
pub async fn save_style_preset(state: State<'_, AppState>, payload: Value) -> Res<Value> {
    let coll = state.db.collection::<Document>("style_presets");
    let id = payload["id"].as_str().filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let name = payload["name"].as_str().unwrap_or("Untitled").to_string();
    let pack = payload["pack"].as_str().unwrap_or("Custom").to_string();
    let params = sanitize_params(&payload["params"]);
    let mut d = doc! {
        "id": &id, "name": &name, "pack": &pack, "params": &params, "builtin": false,
    };
    d.insert("_id", &id);
    coll.update_one(doc! { "_id": &id }, doc! { "$set": &d }).upsert(true).await.map_err(e)?;
    Ok(json!({ "ok": true, "id": id, "name": name, "pack": pack, "params": to_value(doc!{"params": params})["params"].clone() }))
}

#[tauri::command]
pub async fn delete_style_preset(state: State<'_, AppState>, id: String) -> Res<Value> {
    // Built-ins are protected (their ids are prefixed "builtin-").
    if id.starts_with("builtin-") {
        return Err("Built-in presets cannot be deleted.".into());
    }
    state.db.collection::<Document>("style_presets")
        .delete_one(doc! { "_id": &id }).await.map_err(e)?;
    Ok(json!({ "ok": true, "id": id }))
}

#[tauri::command]
pub async fn get_channel_style(state: State<'_, AppState>, channel_id: String) -> Res<Value> {
    let d = state.db.collection::<Document>("channel_styles")
        .find_one(doc! { "_id": &channel_id }).await.map_err(e)?;
    Ok(match d { Some(d) => to_value(d), None => json!({ "channel_id": channel_id, "params": {} }) })
}

#[tauri::command]
pub async fn set_channel_style(state: State<'_, AppState>, channel_id: String, payload: Value) -> Res<Value> {
    let params = sanitize_params(&payload["params"]);
    let preset_id = payload["preset_id"].as_str().unwrap_or("").to_string();
    let mut d = doc! { "channel_id": &channel_id, "preset_id": &preset_id, "params": &params };
    d.insert("_id", &channel_id);
    state.db.collection::<Document>("channel_styles")
        .update_one(doc! { "_id": &channel_id }, doc! { "$set": &d }).upsert(true).await.map_err(e)?;
    Ok(json!({ "ok": true, "channel_id": channel_id, "preset_id": preset_id }))
}

/// Resolve the sticky style overrides for a given song language: the "general + per-channel
/// specialization" hierarchy — first try the channel-specific pin (matched by language, set via
/// Style Studio / Channel Manager), and if the channel has none pinned, fall back to the
/// project-wide default pack (`global_channel_settings.default_style_preset_id`, set in Channel
/// Manager's Global Settings). Returns the `comfyui_*` params object to overlay onto the job
/// settings, or `None` if neither resolves to anything. Used by the image job.
pub async fn channel_style_overrides(db: &mongodb::Database, language: &str, project_id: Option<&str>) -> Option<Value> {
    let language = language.trim();
    if !language.is_empty() {
        // Case-insensitive language match against channels.
        if let Some(ch) = db.collection::<Document>("channels")
            .find_one(doc! { "language": { "$regex": format!("^{}$", regex_escape(language)), "$options": "i" } })
            .await.ok().flatten()
        {
            if let Ok(channel_id) = ch.get_str("id") {
                if let Some(style) = db.collection::<Document>("channel_styles")
                    .find_one(doc! { "_id": channel_id }).await.ok().flatten()
                {
                    if let Ok(params) = style.get_document("params") {
                        if !params.is_empty() {
                            return Some(to_value(doc! { "params": params.clone() })["params"].clone());
                        }
                    }
                }
            }
        }
    }

    // No channel-specific pin (or no language to match on) — fall back to the project-wide
    // default pack, if one's configured.
    let filter = match project_id {
        Some(pid) if !pid.is_empty() => doc! { "project_id": pid },
        _ => doc! { "_id": "singleton" },
    };
    let global = db.collection::<Document>("global_channel_settings").find_one(filter).await.ok().flatten()?;
    let preset_id = global.get_str("default_style_preset_id").ok()?;
    if preset_id.is_empty() { return None; }
    let preset = db.collection::<Document>("style_presets").find_one(doc! { "_id": preset_id }).await.ok().flatten()?;
    let params = preset.get_document("params").ok()?;
    if params.is_empty() { return None; }
    Some(to_value(doc! { "params": params.clone() })["params"].clone())
}

fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\^$.|?*+()[]{}".contains(c) { out.push('\\'); }
        out.push(c);
    }
    out
}

// ────────────────────────────────────────────────────────────────
// Genre-mix presets (music). Each preset is a named, packaged fused style string plus the source
// genres that produced it, so a mix can be re-loaded, tweaked, and re-applied to songs.
// ────────────────────────────────────────────────────────────────

fn builtin_genre_presets() -> Vec<Value> {
    vec![
        json!({ "id":"builtin-playful-edm-bounce", "name":"Playful EDM Bounce", "pack":"Playful EDM", "builtin":true,
            "genres":["future bounce","playful melodic house","bright supersaw"],
            "styles":"future bounce, playful melodic house, bright supersaw leads, bouncy plucks, vocal chops, sunny festival energy, cheeky rhythm, 126 bpm" }),
        json!({ "id":"builtin-playful-edm-swing", "name":"Playful Electro Swing", "pack":"Playful EDM", "builtin":true,
            "genres":["electro swing","house","vintage brass"],
            "styles":"electro swing, four-on-the-floor house groove, vintage brass stabs, cheeky clarinet, hand claps, playful bounce, retro-future fun, 122 bpm" }),
        json!({ "id":"builtin-playful-edm-tropical", "name":"Playful Tropical Pop-EDM", "pack":"Playful EDM", "builtin":true,
            "genres":["tropical house","playful pop","steel drums"],
            "styles":"tropical house, playful pop toplines, steel drum plucks, marimba, warm sub bass, breezy carefree mood, whistle hook, 108 bpm" }),
        json!({ "id":"builtin-playful-chiptune-dance", "name":"Playful Chiptune Dance", "pack":"Playful", "builtin":true,
            "genres":["chiptune","dance pop","8-bit"],
            "styles":"chiptune, dance pop, 8-bit arpeggios, punchy square leads, cheerful melody, arcade energy, hand claps, 128 bpm" }),
        json!({ "id":"builtin-playful-funk", "name":"Playful Funk Pop", "pack":"Playful", "builtin":true,
            "genres":["funk","pop","brass"],
            "styles":"funk pop, slap bass, bright brass, wah guitar, upbeat groove, feel-good vibe, playful vocal ad-libs, 112 bpm" }),
        json!({ "id":"builtin-cinematic-uplift", "name":"Cinematic Uplift", "pack":"Cinematic", "builtin":true,
            "genres":["cinematic","orchestral","epic"],
            "styles":"cinematic orchestral, soaring strings, epic brass, big percussion, triumphant build, emotional swell, 90 bpm" }),
    ]
}

#[tauri::command]
pub async fn list_genre_presets(state: State<'_, AppState>) -> Res<Value> {
    let coll = state.db.collection::<Document>("genre_presets");
    if coll.count_documents(doc! {}).await.map_err(e)? == 0 {
        for p in builtin_genre_presets() {
            if let Ok(mut d) = bson::to_document(&p) {
                let id = p["id"].as_str().unwrap_or("").to_string();
                d.insert("_id", &id);
                let _ = coll.insert_one(d).await;
            }
        }
    }
    use futures_util::StreamExt;
    let mut cursor = coll.find(doc! {}).await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { out.push(to_value(d)); }
    Ok(json!({ "presets": out }))
}

#[tauri::command]
pub async fn save_genre_preset(state: State<'_, AppState>, payload: Value) -> Res<Value> {
    let coll = state.db.collection::<Document>("genre_presets");
    let id = payload["id"].as_str().filter(|s| !s.is_empty())
        .map(|s| s.to_string()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let name = payload["name"].as_str().unwrap_or("Untitled Mix").to_string();
    let pack = payload["pack"].as_str().unwrap_or("Custom").to_string();
    let styles = payload["styles"].as_str().unwrap_or("").to_string();
    let genres = bson::to_bson(&payload["genres"]).unwrap_or(bson::Bson::Array(vec![]));
    let mut d = doc! { "id": &id, "name": &name, "pack": &pack, "styles": &styles, "genres": genres, "builtin": false };
    d.insert("_id", &id);
    coll.update_one(doc! { "_id": &id }, doc! { "$set": &d }).upsert(true).await.map_err(e)?;
    Ok(json!({ "ok": true, "id": id, "name": name, "pack": pack, "styles": styles }))
}

#[tauri::command]
pub async fn delete_genre_preset(state: State<'_, AppState>, id: String) -> Res<Value> {
    if id.starts_with("builtin-") { return Err("Built-in presets cannot be deleted.".into()); }
    state.db.collection::<Document>("genre_presets").delete_one(doc! { "_id": &id }).await.map_err(e)?;
    Ok(json!({ "ok": true, "id": id }))
}

// ────────────────────────────────────────────────────────────────
// Transition preset packs — each defines a vocabulary of ffmpeg xfade transitions plus a default,
// used by the AI transition suggester and the manual editor.
// ────────────────────────────────────────────────────────────────

fn builtin_transition_presets() -> Vec<Value> {
    vec![
        json!({ "id":"builtin-tr-gentle", "name":"Gentle", "builtin":true, "default":"fade",
            "transitions":["fade","dissolve","fadeblack","smoothleft","smoothright"] }),
        json!({ "id":"builtin-tr-dynamic", "name":"Dynamic", "builtin":true, "default":"slideleft",
            "transitions":["slideleft","slideright","slideup","wipeleft","wipeup","pixelize","zoomin"] }),
        json!({ "id":"builtin-tr-dreamy", "name":"Dreamy", "builtin":true, "default":"dissolve",
            "transitions":["dissolve","circleopen","circleclose","radial","fadewhite","distance"] }),
        json!({ "id":"builtin-tr-cinematic", "name":"Cinematic", "builtin":true, "default":"fadeblack",
            "transitions":["fadeblack","fade","smoothup","wipedown","diagtl"] }),
    ]
}

#[tauri::command]
pub async fn list_transition_presets(state: State<'_, AppState>) -> Res<Value> {
    let coll = state.db.collection::<Document>("transition_presets");
    if coll.count_documents(doc! {}).await.map_err(e)? == 0 {
        for p in builtin_transition_presets() {
            if let Ok(mut d) = bson::to_document(&p) {
                let id = p["id"].as_str().unwrap_or("").to_string();
                d.insert("_id", &id);
                let _ = coll.insert_one(d).await;
            }
        }
    }
    use futures_util::StreamExt;
    let mut cursor = coll.find(doc! {}).await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { out.push(to_value(d)); }
    Ok(json!({ "presets": out }))
}

#[tauri::command]
pub async fn save_transition_preset(state: State<'_, AppState>, payload: Value) -> Res<Value> {
    let coll = state.db.collection::<Document>("transition_presets");
    let id = payload["id"].as_str().filter(|s| !s.is_empty())
        .map(|s| s.to_string()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let name = payload["name"].as_str().unwrap_or("Custom Pack").to_string();
    let default_t = payload["default"].as_str().unwrap_or("fade").to_string();
    let transitions = bson::to_bson(&payload["transitions"]).unwrap_or(bson::Bson::Array(vec![]));
    let mut d = doc! { "id": &id, "name": &name, "default": &default_t, "transitions": transitions, "builtin": false };
    d.insert("_id", &id);
    coll.update_one(doc! { "_id": &id }, doc! { "$set": &d }).upsert(true).await.map_err(e)?;
    Ok(json!({ "ok": true, "id": id, "name": name }))
}

#[tauri::command]
pub async fn delete_transition_preset(state: State<'_, AppState>, id: String) -> Res<Value> {
    if id.starts_with("builtin-") { return Err("Built-in packs cannot be deleted.".into()); }
    state.db.collection::<Document>("transition_presets").delete_one(doc! { "_id": &id }).await.map_err(e)?;
    Ok(json!({ "ok": true, "id": id }))
}
