//! fal.ai — generation with no server to run.
//!
//! Every other engine in this app is a server somebody has to bring up: a Kaggle notebook behind a
//! rotating tunnel, or a rented box at a fixed address. That machinery — the log monitor, the quota
//! arithmetic, the account rotation, the idle sweep — exists because a free GPU is a thing with a
//! lifecycle. This has none of it. A POST goes out and a finished file comes back.
//!
//! The trade is money for operations: about five times the cost per video of renting a 4090 by the
//! hour, and none of the failure modes that cost is buying away. Somebody who never wants to see the
//! word "tunnel" should use this and nothing else.
//!
//! # Why it does not go through the ComfyUI path
//!
//! fal hosts *models*, not graphs. There is no checkpoint to name, no sampler to pick, no node set
//! to keep in step with a custom-node release. Routing this through `comfy_workflows/` would mean
//! building a graph in order to throw it away, so the model id and a handful of parameters are the
//! whole request — which is also why this file is a fraction of the size of the video command it
//! parallels.
//!
//! # The queue
//!
//! fal answers a submit with a status URL rather than a result, because a video takes minutes. The
//! polling below is deliberately patient (up to 15 minutes) and deliberately quiet about it: a clip
//! that takes four minutes is normal, not a hang.

use crate::state::AppState;
use bson::{doc, Document};
use serde_json::{json, Value};
use tauri::State;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

const FAL_QUEUE: &str = "https://queue.fal.run";

/// A model fal hosts, with what it costs and what it is for.
///
/// Deliberately a short list rather than fal's whole catalogue: five models somebody can choose
/// between beats six hundred they have to research. Each maps onto a purpose the app already has.
pub struct FalModel {
    pub id: &'static str,
    pub label: &'static str,
    /// fal's own endpoint path.
    pub endpoint: &'static str,
    pub kind: &'static str, // "video" | "image"
    pub price: &'static str,
    pub note: &'static str,
}

pub const FAL_MODELS: &[FalModel] = &[
    FalModel {
        id: "wan_t2v", label: "Wan 2.5 — text to video",
        endpoint: "fal-ai/wan-25/text-to-video", kind: "video",
        price: "$0.05 per second of output",
        note: "The best price-to-quality on fal for video. A 5-second clip is about 25 cents.",
    },
    FalModel {
        id: "wan_i2v", label: "Wan 2.5 — image to video",
        endpoint: "fal-ai/wan-25/image-to-video", kind: "video",
        price: "$0.05 per second of output",
        note: "Animates a still this project already made, keeping its composition.",
    },
    FalModel {
        id: "ltx", label: "LTX-Video — fast",
        endpoint: "fal-ai/ltx-video", kind: "video",
        price: "cheapest video on fal",
        note: "For volume B-roll where the clip sits behind text and need not be perfect.",
    },
    FalModel {
        id: "flux_schnell", label: "FLUX.1 schnell",
        endpoint: "fal-ai/flux/schnell", kind: "image",
        price: "~$0.003 per image",
        note: "Few-step FLUX. Cheap enough to over-generate and cut.",
    },
    FalModel {
        id: "flux_dev", label: "FLUX.1 dev",
        endpoint: "fal-ai/flux/dev", kind: "image",
        price: "~$0.025 per image",
        note: "Slower and markedly better than schnell. For pictures somebody looks at directly.",
    },
];

pub fn fal_model(id: &str) -> Option<&'static FalModel> {
    FAL_MODELS.iter().find(|m| m.id == id.trim())
}

async fn settings_of(state: &AppState) -> Value {
    state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.ok().flatten()
        .map(|d| crate::store::doc_to_json(&d)).unwrap_or_default()
}

fn api_key(settings: &Value) -> Res<String> {
    let k = settings["fal_api_key"].as_str().unwrap_or("").trim().to_string();
    if k.is_empty() {
        return Err("No fal.ai API key yet. Create one at fal.ai → Dashboard → Keys and paste it \
                    into Settings → fal.ai.".into());
    }
    Ok(k)
}

/// What fal can do, and what each thing costs. Shown in the picker.
#[tauri::command]
pub async fn fal_catalogue(state: State<'_, AppState>) -> Res<Value> {
    let settings = settings_of(&state).await;
    Ok(json!({
        "configured": !settings["fal_api_key"].as_str().unwrap_or("").trim().is_empty(),
        "signup_url": "https://fal.ai/dashboard/keys",
        "models": FAL_MODELS.iter().map(|m| json!({
            "id": m.id, "label": m.label, "kind": m.kind, "price": m.price, "note": m.note,
        })).collect::<Vec<_>>(),
    }))
}

/// Is the key valid? Asks fal rather than checking the string looks plausible.
#[tauri::command]
pub async fn test_fal(state: State<'_, AppState>) -> Res<Value> {
    let settings = settings_of(&state).await;
    let Ok(key) = api_key(&settings) else {
        return Ok(json!({ "ok": false, "detail": "No fal.ai API key set.",
                          "next_step": "Create one at fal.ai → Dashboard → Keys." }));
    };
    // Asking about a made-up request id distinguishes "bad key" (401/403) from "unreachable"
    // without spending a generation to find out. A 404 is the healthy answer: the key was
    // accepted, the request id was not.
    match reqwest::Client::new()
        .get(format!("{FAL_QUEUE}/fal-ai/flux/schnell/requests/does-not-exist/status"))
        .header("Authorization", format!("Key {key}"))
        .timeout(std::time::Duration::from_secs(12)).send().await
    {
        Ok(r) if matches!(r.status().as_u16(), 401 | 403) =>
            Ok(json!({ "ok": false, "detail": "fal.ai rejected that key.",
                       "next_step": "Create a fresh key at fal.ai → Dashboard → Keys." })),
        Ok(_) => Ok(json!({ "ok": true, "detail": "fal.ai accepted the key." })),
        Err(err) => Ok(json!({ "ok": false, "detail": format!("Could not reach fal.ai: {err}") })),
    }
}

/// Generate through fal and return the finished file URLs.
///
/// One call covers images and video because from here they differ only in which endpoint is hit and
/// how long the wait is — which is the whole argument for this engine existing.
#[tauri::command]
pub async fn fal_generate(
    state: State<'_, AppState>,
    model: String,
    prompt: String,
    seconds: Option<u32>,
    image_url: Option<String>,
    aspect: Option<String>,
) -> Res<Value> {
    let settings = settings_of(&state).await;
    let key = api_key(&settings)?;
    let m = fal_model(&model).ok_or_else(|| format!("Unknown fal model '{model}'."))?;
    if prompt.trim().is_empty() { return Err("Describe what to generate first.".into()); }

    let mut input = json!({ "prompt": prompt.trim() });
    if let Some(a) = aspect.as_deref().filter(|s| !s.trim().is_empty()) {
        input["aspect_ratio"] = json!(a);
    }
    if m.kind == "video" {
        input["duration"] = json!(seconds.unwrap_or(5).clamp(1, 10));
    }
    // An image-to-video endpoint without an image fails server-side with a schema error, which
    // reads like an outage rather than a missing field.
    if m.endpoint.contains("image-to-video") {
        let url = image_url.as_deref().unwrap_or("").trim().to_string();
        if url.is_empty() {
            return Err(format!("{} animates an existing picture, so it needs one.", m.label));
        }
        input["image_url"] = json!(url);
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60)).build().map_err(e)?;
    let submit = client.post(format!("{FAL_QUEUE}/{}", m.endpoint))
        .header("Authorization", format!("Key {key}"))
        .json(&input).send().await
        .map_err(|err| format!("Could not reach fal.ai: {err}"))?;

    let status = submit.status();
    let body: Value = submit.json().await
        .map_err(|_| "fal.ai returned something unreadable.".to_string())?;
    if !status.is_success() {
        // fal puts the useful part in `detail`; showing the raw body instead buries it.
        let detail = body["detail"].as_str().map(|s| s.to_string())
            .unwrap_or_else(|| body.to_string().chars().take(300).collect());
        return Err(format!("fal.ai refused the request ({status}): {detail}"));
    }
    let status_url = body["status_url"].as_str().unwrap_or("").to_string();
    let response_url = body["response_url"].as_str().unwrap_or("").to_string();
    if status_url.is_empty() {
        return Err("fal.ai accepted the job but returned no status URL.".into());
    }

    // Patient on purpose: minutes is normal for video, so a short timeout would turn a working
    // generation into a reported failure.
    for _ in 0..225 {
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
        let Ok(r) = client.get(&status_url)
            .header("Authorization", format!("Key {key}")).send().await else { continue };
        let Ok(s) = r.json::<Value>().await else { continue };
        match s["status"].as_str().unwrap_or("") {
            "COMPLETED" => {
                let Ok(rr) = client.get(&response_url)
                    .header("Authorization", format!("Key {key}")).send().await else { continue };
                let out: Value = rr.json().await.map_err(e)?;
                let urls = collect_urls(&out);
                if urls.is_empty() { return Err("fal.ai finished but returned no file.".into()); }
                return Ok(json!({ "ok": true, "urls": urls, "model": m.id }));
            }
            "FAILED" => return Err(format!("fal.ai reported the job failed: {}",
                                           s["error"].as_str().unwrap_or("no reason given"))),
            _ => continue, // IN_QUEUE / IN_PROGRESS
        }
    }
    Err("Timed out waiting for fal.ai (15 min).".into())
}

/// Pull every file URL out of a fal response.
///
/// The shape differs per model — `video.url`, `images[].url`, `audio.url` — so rather than modelling
/// each, this walks the tree for anything that looks like a delivered file. A model added later
/// therefore works without a code change.
fn collect_urls(v: &Value) -> Vec<String> {
    let mut out = Vec::new();
    walk(v, &mut out);
    out
}

fn walk(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(map) => {
            if let Some(u) = map.get("url").and_then(|u| u.as_str()) {
                if u.starts_with("http") && !out.iter().any(|x| x == u) { out.push(u.to_string()); }
            }
            for (_, val) in map { walk(val, out); }
        }
        Value::Array(arr) => for item in arr { walk(item, out); },
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_model_is_choosable() {
        for m in FAL_MODELS {
            assert!(m.endpoint.contains('/'), "{} has no endpoint", m.id);
            assert!(matches!(m.kind, "video" | "image"), "{} has an unknown kind", m.id);
            // The whole argument for this engine is that the cost is visible before you press go.
            assert!(!m.price.is_empty(), "{} has no price", m.id);
            assert!(m.note.len() > 25, "{} does not say when to pick it", m.id);
            assert!(fal_model(m.id).is_some());
        }
        assert!(fal_model("nope").is_none());
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<&str> = FAL_MODELS.iter().map(|m| m.id).collect();
        let n = ids.len();
        ids.sort_unstable(); ids.dedup();
        assert_eq!(n, ids.len(), "duplicate fal model id");
    }

    /// The response walker is the part most likely to silently return nothing, since every model
    /// shapes its output differently.
    #[test]
    fn urls_are_found_wherever_the_model_put_them() {
        let video = json!({ "video": { "url": "https://fal.media/a.mp4" } });
        assert_eq!(collect_urls(&video), vec!["https://fal.media/a.mp4"]);

        let images = json!({ "images": [ { "url": "https://fal.media/1.png" },
                                         { "url": "https://fal.media/2.png" } ] });
        assert_eq!(collect_urls(&images).len(), 2);

        let nested = json!({ "outputs": { "clips": [ { "file": { "url": "https://fal.media/deep.mp4" } } ] } });
        assert_eq!(collect_urls(&nested), vec!["https://fal.media/deep.mp4"]);
    }

    #[test]
    fn a_repeated_url_is_only_returned_once() {
        // Several models echo the same file under both `video` and `outputs`.
        let dup = json!({ "video": { "url": "https://fal.media/a.mp4" },
                          "outputs": [ { "url": "https://fal.media/a.mp4" } ] });
        assert_eq!(collect_urls(&dup).len(), 1);
    }

    #[test]
    fn non_urls_are_ignored() {
        let noise = json!({ "url": "not-a-url", "seed": 42, "timings": { "inference": 3.2 } });
        assert!(collect_urls(&noise).is_empty());
    }
}
