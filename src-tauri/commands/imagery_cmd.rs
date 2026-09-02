//! The devotional-imagery catalogue, as commands.
//!
//! The data and every rule live in `imagery.rs` so they can be tested without a window. These are
//! the four things the interface needs: what exists, what a choice will produce, whether words are
//! allowed here, and whether this will print.

use crate::imagery::{self, Controls};
use crate::state::AppState;
use serde_json::{json, Value};
use tauri::State;

type Res<T> = Result<T, String>;

/// Models, destinations, packaged intents and the control vocabulary.
#[tauri::command]
pub async fn imagery_catalogue() -> Res<Value> {
    Ok(imagery::catalogue())
}

#[derive(serde::Deserialize)]
pub struct ImageryComposeRequest {
    pub subject: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub destination: Option<String>,
    /// A packaged intent fills in both the destination and a sensible voice.
    #[serde(default)]
    pub intent: Option<String>,
    #[serde(default)]
    pub controls: Option<Controls>,
}

/// What this choice will actually generate — shown before anything is spent.
///
/// The `notes` are the point: they are where the app admits that a negative prompt would be ignored
/// on this model, that restraint was switched off, or that this shape is not what the model is best
/// at. A preview that only showed the prompt would hide exactly the decisions worth seeing.
#[tauri::command]
pub async fn compose_image_prompt(payload: ImageryComposeRequest) -> Res<Value> {
    let (destination, controls, model) = resolve(&payload)?;
    let out = imagery::compose(&model, &destination, &controls, &payload.subject)?;
    Ok(serde_json::to_value(out).unwrap_or_else(|_| json!({})))
}

/// Resolve an intent (or explicit choices) into the three things `compose` needs.
fn resolve(payload: &ImageryComposeRequest) -> Res<(String, Controls, String)> {
    if let Some(id) = payload.intent.as_deref().filter(|s| !s.is_empty()) {
        let i = imagery::intent(id).ok_or_else(|| format!("There is no preset called {id:?}."))?;
        let controls = match &payload.controls {
            // An explicit control set still wins: a preset is a starting point, not a cage.
            Some(c) => c.clone(),
            None => imagery::controls_for(i),
        };
        let model = payload.model.clone()
            .filter(|s| !s.is_empty())
            .or_else(|| i.model.map(String::from))
            .unwrap_or_else(|| "flux_dev".into());
        return Ok((i.destination.to_string(), controls, model));
    }
    Ok((
        payload.destination.clone().unwrap_or_else(|| "video_frame".into()),
        payload.controls.clone().unwrap_or_default(),
        payload.model.clone().unwrap_or_else(|| "flux_dev".into()),
    ))
}

/// May words appear inside this picture?
#[tauri::command]
pub async fn imagery_text_allowed(destination: String, model: String) -> Res<Value> {
    match imagery::text_allowed(&destination, &model) {
        Ok(()) => Ok(json!({ "allowed": true })),
        Err(reason) => Ok(json!({ "allowed": false, "reason": reason })),
    }
}

/// Will this print, at this size, in that print area?
///
/// Asked before the art is made rather than after, because afterwards the GPU time and the listing
/// are already spent.
#[tauri::command]
pub async fn imagery_print_check(
    destination: String, art_w: u32, art_h: u32, area_w: u32, area_h: u32,
) -> Res<Value> {
    match imagery::print_check(&destination, art_w, art_h, area_w, area_h) {
        Ok(report) => Ok(report),
        Err(reason) => Ok(json!({ "ok": false, "refused": true, "reason": reason })),
    }
}

/// Remember the last imagery choice per project, so the next image starts where the last one left
/// off rather than at the defaults.
#[tauri::command]
pub async fn save_imagery_choice(
    state: State<'_, AppState>, project_id: String, choice: Value,
) -> Res<Value> {
    use bson::{doc, Document};
    let bson = bson::to_document(&choice).map_err(|e| e.to_string())?;
    state.db.collection::<Document>("settings")
        .update_one(doc! { "_id": &project_id }, doc! { "$set": { "imagery": bson } })
        .upsert(true).await.map_err(|e| e.to_string())?;
    Ok(json!({ "ok": true }))
}
