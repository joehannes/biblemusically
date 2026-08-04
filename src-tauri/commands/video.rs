//! Video generation: the catalogue, the advice, and the render itself.
//!
//! The graphs live in `comfy_workflows/video_*.json` and the numbers in `video_models.rs`; this
//! module is the join between them. Kept separate from `jobs.rs`'s image path on purpose — the two
//! share a server *protocol* and nothing else. Image generation picks a graph from an intention plus
//! a checkpoint profile (see `comfy_registry.rs`); video picks one from the model family, because a
//! Wan graph and an LTX graph are not variants of each other, they are different node sets.

use crate::state::AppState;
use crate::video_advisor::{self as advisor, Setup};
use crate::video_models::{self as vm, Drive, Tier};
use bson::{doc, Document};
use serde_json::{json, Value};
use tauri::State;
use uuid::Uuid;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

/// The graph for a model family, and the placeholders it expects filled.
fn graph_for(model_id: &str) -> Option<&'static str> {
    Some(match model_id {
        "ltx2b" | "ltx13b" => include_str!("../comfy_workflows/video_ltx_t2v.json"),
        "wan21_t2v_1_3b" | "wan21_t2v_14b" => include_str!("../comfy_workflows/video_wan_t2v.json"),
        "wan22_ti2v_5b" | "wan22_i2v_14b" => include_str!("../comfy_workflows/video_wan_i2v.json"),
        "animatediff_sd15" => include_str!("../comfy_workflows/video_animatediff_loop.json"),
        _ => return None,
    })
}

/// Substitute a string into a `"__TOKEN__"` slot, JSON-escaping it.
///
/// The quotes are part of the token so that escaping cannot break the document: a prompt containing
/// a quote or a newline is serialised by serde rather than pasted in raw.
fn fill_str(wf: &str, token: &str, value: &str) -> String {
    let quoted = serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into());
    wf.replace(&format!("\"{token}\""), &quoted)
}

/// The weight filenames a graph needs, as ComfyUI will see them on disk.
///
/// ComfyUI addresses models by basename within its `models/<dir>` tree, so the repo path a weight
/// was downloaded from is irrelevant here — only the last path segment.
fn basename(file: &str) -> &str { file.rsplit('/').next().unwrap_or(file) }

/// Build the graph for a preset.
///
/// Returns the filled workflow. Any placeholder left unfilled would survive into the submitted JSON
/// and be rejected by `/prompt` with a message about an invalid node input, so every token a graph
/// can contain is filled here unconditionally — see the same contract in `jobs.rs`.
pub fn build_graph(
    preset_id: &str,
    prompt: &str,
    negative: &str,
    seed: u64,
    input_image: Option<&str>,
) -> Res<Value> {
    let p = vm::preset(preset_id).ok_or_else(|| format!("Unknown video preset '{preset_id}'."))?;
    let m = vm::model_of(p).ok_or_else(|| format!("Preset '{preset_id}' names an unknown model."))?;
    let raw = graph_for(m.id).ok_or_else(|| format!("No workflow graph for model '{}'.", m.id))?;

    if m.drive == Drive::ImageToVideo && input_image.unwrap_or("").trim().is_empty() {
        return Err(format!(
            "{} animates an existing picture, so it needs one. Pick an image first, or choose a \
             text-to-video preset.", m.label));
    }

    // Weight slots. Each model family uses a different subset, and a token a graph does not contain
    // is simply not replaced — so filling all of them is safe and forgetting one is not.
    let ckpt = m.weights.first().map(|w| basename(w.file)).unwrap_or("");
    let clip = m.weights.iter().find(|w| w.dir == "text_encoders").map(|w| basename(w.file)).unwrap_or("");
    let vae = m.weights.iter().find(|w| w.dir == "vae").map(|w| basename(w.file)).unwrap_or("");
    let motion = m.weights.iter().find(|w| w.dir == "animatediff_models").map(|w| basename(w.file)).unwrap_or("");
    // AnimateDiff drives SD 1.5, so its checkpoint is the *checkpoints* entry, not the motion module.
    let ckpt = if m.id == "animatediff_sd15" {
        m.weights.iter().find(|w| w.dir == "checkpoints").map(|w| basename(w.file)).unwrap_or(ckpt)
    } else { ckpt };

    let mut wf = raw.to_string();
    wf = fill_str(&wf, "__CKPT__", ckpt);
    wf = fill_str(&wf, "__CLIP__", clip);
    wf = fill_str(&wf, "__VAE__", vae);
    wf = fill_str(&wf, "__MOTION__", motion);
    wf = fill_str(&wf, "__PROMPT__", prompt.trim());
    wf = fill_str(&wf, "__NEGATIVE__", negative.trim());
    wf = fill_str(&wf, "__INPUT_IMAGE__", input_image.unwrap_or(""));
    wf = wf.replace("__WIDTH__", &p.width.to_string());
    wf = wf.replace("__HEIGHT__", &p.height.to_string());
    wf = wf.replace("__FRAMES__", &p.frames.to_string());
    wf = wf.replace("__FPS__", &p.fps.to_string());
    wf = wf.replace("__STEPS__", &p.steps.to_string());
    wf = wf.replace("__CFG__", &format!("{:.2}", p.cfg));
    wf = wf.replace("__SEED__", &seed.to_string());

    let mut parsed: Value = serde_json::from_str(&wf)
        .map_err(|err| format!("The {} graph did not parse after filling: {err}", m.id))?;
    // `_comment` documents the graph for whoever reads the file; ComfyUI would reject it as a node.
    if let Some(obj) = parsed.as_object_mut() { obj.remove("_comment"); }
    Ok(parsed)
}

// ── Commands ────────────────────────────────────────────────────────────────

async fn settings_of(state: &AppState) -> Value {
    state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.ok().flatten()
        .map(|d| crate::store::doc_to_json(&d)).unwrap_or_default()
}

fn tier_of(settings: &Value) -> Tier {
    Tier::parse(settings["video_tier"].as_str().unwrap_or(""))
}

/// Everything the Video Studio needs to render itself, filtered to what this hardware can run.
#[tauri::command]
pub async fn video_catalogue(state: State<'_, AppState>) -> Res<Value> {
    let settings = settings_of(&state).await;
    let tier = tier_of(&settings);
    Ok(json!({
        "tier": tier.id(),
        "tiers": vm::TIERS.iter().map(|t| json!({
            "id": t.id(), "label": t.label(), "vram_gb": t.vram_gb(),
            "native_low_precision": t.native_low_precision(),
        })).collect::<Vec<_>>(),
        // Everything, with a runnable flag — rather than hiding what does not fit. Somebody deciding
        // whether better hardware is worth paying for needs to see what it would buy them.
        "models": vm::MODELS.iter().map(|m| json!({
            "id": m.id, "label": m.label, "drive": m.drive, "vram_gb": m.vram_gb,
            "min_tier": m.min_tier.id(), "minutes_per_clip": [m.minutes_per_clip.0, m.minutes_per_clip.1],
            "character": m.character, "licence": m.licence, "runs": m.runs_on(tier),
        })).collect::<Vec<_>>(),
        "presets": vm::PRESETS.iter().map(|p| json!({
            "id": p.id, "label": p.label, "model": p.model, "purpose": p.purpose,
            "width": p.width, "height": p.height, "frames": p.frames, "fps": p.fps,
            "seconds": p.seconds(), "vertical": p.is_vertical(), "use_for": p.use_for,
            "runs": vm::model_of(p).map(|m| m.runs_on(tier)).unwrap_or(false),
        })).collect::<Vec<_>>(),
        "packs": vm::PACKS.iter().map(|pk| json!({
            "id": pk.id, "label": pk.label, "description": pk.description,
            "presets": pk.presets, "typical_minutes": pk.typical_minutes,
            "download_gb": vm::pack_download_gb(pk.id),
            "runs": vm::packs_for_tier(tier).iter().any(|x| x.id == pk.id),
        })).collect::<Vec<_>>(),
        "selected_preset": settings["video_preset"],
        "selected_pack": settings["video_pack"],
        "server_url": settings["video_api_url"],
    }))
}

/// What is wrong with the current setup, before any GPU time is spent on it.
#[tauri::command]
pub async fn video_advice(
    state: State<'_, AppState>,
    preset: Option<String>,
    pack: Option<String>,
    planned_clips: Option<u32>,
) -> Res<Value> {
    let settings = settings_of(&state).await;
    let accounts = settings["kaggle_accounts"].as_array().map(|a| a.len() as u32).unwrap_or(0).max(1);
    let url = settings["video_api_url"].as_str().unwrap_or("").to_string();

    // A stored URL is not a live server — Cloudflare quick tunnels rotate every run, so the honest
    // answer needs an actual request.
    let server_live = if url.trim().is_empty() { false } else {
        matches!(reqwest::Client::new().get(format!("{}/system_stats", url.trim_end_matches('/')))
            .timeout(std::time::Duration::from_secs(6)).send().await,
            Ok(r) if r.status().as_u16() < 500)
    };

    // Best effort: no token, no network, or an unexpected shape all mean "unknown", which the
    // advisor already treats as "do not claim a quota problem" rather than as zero.
    let quota_left = crate::kaggle_api::quota().await.ok().map(|q| q.left_minutes.max(0) as u32);

    let setup = Setup {
        tier: tier_of(&settings),
        preset: preset.or_else(|| settings["video_preset"].as_str().map(String::from)),
        pack: pack.or_else(|| settings["video_pack"].as_str().map(String::from)),
        planned_clips: planned_clips.unwrap_or(1),
        // Asked of Kaggle directly rather than read from settings. The stored key was never
        // populated by anything — the advisor was written expecting a number the app had no way to
        // obtain, so its quota rules could not fire. `kernels/quota` is that number.
        quota_minutes_left: quota_left.or_else(||
            settings["kaggle_quota_minutes_left"].as_u64().map(|v| v as u32)),
        connected_accounts: accounts,
        server_live,
        disk_gb_free: settings["video_disk_gb_free"].as_f64().map(|v| v as f32),
    };
    let hints = advisor::advise(&setup);
    Ok(json!({
        "server_live": server_live,
        "blocked": advisor::is_blocked(&hints),
        "hints": hints,
        // Surfaced so the page can show the real figure rather than only warn when it runs low.
        "quota_minutes_left": quota_left,
    }))
}

/// Render one clip and return the URLs ComfyUI wrote.
#[tauri::command]
pub async fn generate_video(
    state: State<'_, AppState>,
    preset: String,
    prompt: String,
    negative: Option<String>,
    seed: Option<u64>,
    input_image: Option<String>,
) -> Res<Value> {
    let settings = settings_of(&state).await;
    let base = settings["video_api_url"].as_str().unwrap_or("").trim().trim_end_matches('/').to_string();
    if base.is_empty() {
        return Err("The video server has no URL yet — press Start & connect in Settings → Video."
                   .to_string());
    }

    // Refuse a preset this hardware cannot load, with the advisor's own wording, rather than letting
    // the run fail on the GPU after the session has spent time on it.
    let tier = tier_of(&settings);
    if let Some(m) = vm::preset(&preset).and_then(vm::model_of) {
        if !m.runs_on(tier) {
            let alt = vm::preset(&preset).and_then(|p| vm::best_for(p.purpose, tier));
            return Err(format!(
                "{} needs {} and this is set to {}. {}",
                m.label, m.min_tier.label(), tier.label(),
                alt.map(|a| format!("Use “{}” instead.", a.label))
                   .unwrap_or_else(|| "Raise the tier in Settings → Video.".into())));
        }
    }

    let seed = seed.unwrap_or_else(|| rand::random::<u32>() as u64);
    let negative = negative.unwrap_or_else(|| DEFAULT_NEGATIVE.to_string());
    let graph = build_graph(&preset, &prompt, &negative, seed, input_image.as_deref())?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(90)).build().map_err(e)?;
    let client_id = Uuid::new_v4().to_string();
    let submit = client.post(format!("{base}/prompt"))
        .json(&json!({ "prompt": graph, "client_id": client_id }))
        .send().await.map_err(|err| format!("Could not reach the video server: {err}"))?;
    if !submit.status().is_success() {
        let body = submit.text().await.unwrap_or_default();
        // A rejected graph names the offending node, which is the one thing worth surfacing verbatim
        // — especially for the image-to-video graph, which is not transcribed from an official
        // example and is the likeliest to name a node this server does not have.
        return Err(format!("The video server rejected the workflow: {}",
                           body.chars().take(400).collect::<String>()));
    }
    let prompt_id = submit.json::<Value>().await.map_err(e)?["prompt_id"]
        .as_str().ok_or("The server accepted the job but returned no prompt_id.")?.to_string();

    // Video is minutes, not seconds: poll for up to 40 min, which covers the slowest preset in the
    // catalogue (Wan 2.2 5B at 35 min) with headroom.
    for _ in 0..600 {
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
        let Ok(hr) = client.get(format!("{base}/history/{prompt_id}")).send().await else { continue };
        if !hr.status().is_success() { continue }
        let Ok(hist) = hr.json::<Value>().await else { continue };
        let entry = &hist[&prompt_id];
        if entry.is_null() { continue }

        let mut urls = Vec::new();
        if let Some(outputs) = entry["outputs"].as_object() {
            for (_node, out) in outputs {
                // SaveWEBM lands under "videos"; VHS nodes use "gifs"; a still fallback under
                // "images". Collect all three so a graph swap cannot silently produce nothing.
                for key in ["videos", "gifs", "images"] {
                    for item in out[key].as_array().unwrap_or(&Vec::new()) {
                        let f = item["filename"].as_str().unwrap_or("");
                        if f.is_empty() { continue }
                        urls.push(format!("{base}/view?filename={}&subfolder={}&type={}",
                            urlencoding::encode(f),
                            urlencoding::encode(item["subfolder"].as_str().unwrap_or("")),
                            urlencoding::encode(item["type"].as_str().unwrap_or("output"))));
                    }
                }
            }
        }
        if !urls.is_empty() {
            crate::idle_guard::touch("video");
            return Ok(json!({ "ok": true, "urls": urls, "seed": seed, "preset": preset }));
        }
        if entry["status"]["status_str"].as_str() == Some("error") {
            return Err("The video server reported a workflow error — open the notebook to see which \
                        node failed.".to_string());
        }
    }
    Err("Timed out waiting for the clip (40 min). The session may have been cut short.".to_string())
}

/// Check a graph against a running server before anything is generated.
///
/// This is how a graph stops being "unverified". ComfyUI's `/object_info` lists every node class it
/// has registered, with each class's required inputs — so a graph can be checked completely without
/// a GPU, without spending a generation, and without me ever having run the model: build it, ask the
/// server what it knows, compare.
///
/// It catches the two failures that otherwise surface as an opaque `/prompt` rejection minutes into
/// a session: a `class_type` this build of ComfyUI does not have (the likeliest fault in the Wan
/// image-to-video graph, which was not transcribed from an official example), and a required input
/// the graph never supplies.
#[tauri::command]
pub async fn verify_video_graphs(state: State<'_, AppState>, preset: Option<String>) -> Res<Value> {
    let settings = settings_of(&state).await;
    let base = settings["video_api_url"].as_str().unwrap_or("").trim().trim_end_matches('/').to_string();
    if base.is_empty() {
        return Ok(json!({ "ok": false, "reachable": false,
            "detail": "No video server URL yet — start one, or paste a ComfyUI address, then run this again.",
            "next_step": "Any ComfyUI works: Kaggle, a rented box, or http://127.0.0.1:8188 locally." }));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30)).build().map_err(e)?;
    let Ok(res) = client.get(format!("{base}/object_info")).send().await else {
        return Ok(json!({ "ok": false, "reachable": false,
            "detail": format!("Could not reach {base}."),
            "next_step": "The tunnel rotates every run — press Start & connect again." }));
    };
    if !res.status().is_success() {
        return Ok(json!({ "ok": false, "reachable": false,
            "detail": format!("{base} answered {}.", res.status()) }));
    }
    let info: Value = res.json().await.map_err(|_| "The server's node list was unreadable.".to_string())?;
    let Some(known) = info.as_object() else {
        return Ok(json!({ "ok": false, "reachable": true, "detail": "The server returned no node list." }));
    };

    // Every preset, or one. Checking all of them is the useful default: it is one request, and it
    // tells you which parts of the catalogue this particular server can run.
    let presets: Vec<&vm::VideoPreset> = match preset.as_deref() {
        Some(id) => vm::preset(id).into_iter().collect(),
        None => vm::PRESETS.iter().collect(),
    };

    let mut results = Vec::new();
    for p in presets {
        let Some(m) = vm::model_of(p) else { continue };
        let img = if m.drive == Drive::ImageToVideo { Some("probe.png") } else { None };
        let graph = match build_graph(p.id, "probe", "", 1, img) {
            Ok(g) => g,
            Err(err) => {
                results.push(json!({ "preset": p.id, "label": p.label, "ok": false,
                                     "problem": err, "missing_nodes": [] }));
                continue;
            }
        };
        let mut missing: Vec<String> = Vec::new();
        let mut unfilled: Vec<String> = Vec::new();
        for (_id, node) in graph.as_object().into_iter().flatten() {
            let Some(class) = node["class_type"].as_str() else { continue };
            let Some(spec) = known.get(class) else {
                if !missing.iter().any(|c| c == class) { missing.push(class.to_string()); }
                continue;
            };
            // Required inputs the graph does not provide. Links count — a wired input arrives as an
            // array, not a literal — so only genuinely absent keys are reported.
            if let Some(req) = spec["input"]["required"].as_object() {
                for key in req.keys() {
                    if node["inputs"].get(key).is_none() {
                        unfilled.push(format!("{class}.{key}"));
                    }
                }
            }
        }
        let ok = missing.is_empty() && unfilled.is_empty();
        results.push(json!({
            "preset": p.id, "label": p.label, "model": m.id, "ok": ok,
            "missing_nodes": missing, "missing_inputs": unfilled,
            "problem": if ok { Value::Null } else { Value::String(format!(
                "This server does not have everything the {} graph needs.", m.label)) },
        }));
    }

    let good = results.iter().filter(|r| r["ok"] == json!(true)).count();
    Ok(json!({
        "ok": good == results.len(),
        "reachable": true,
        "server": base,
        "node_count": known.len(),
        "verified": good,
        "total": results.len(),
        "results": results,
        "detail": format!("{good} of {} graphs run on this server.", results.len()),
    }))
}

/// What every one of these models is bad at, said once.
///
/// Video models drift toward stillness and toward text artefacts far more than image models do, and
/// the published negatives for Wan are a wall of Chinese that nobody here can edit. This is the
/// English equivalent, kept short because a long negative costs conditioning quality.
pub const DEFAULT_NEGATIVE: &str =
    "static, still, frozen, jpeg artifacts, watermark, subtitles, text, logo, low quality, blurry, \
     deformed hands, extra limbs, flickering";

#[tauri::command]
pub async fn test_video(state: State<'_, AppState>) -> Res<Value> {
    let settings = settings_of(&state).await;
    let base = settings["video_api_url"].as_str().unwrap_or("").trim().trim_end_matches('/').to_string();
    if base.is_empty() {
        return Ok(json!({ "ok": false, "detail": "No video server URL yet.",
                          "next_step": "Press Start & connect." }));
    }
    match reqwest::Client::new().get(format!("{base}/system_stats"))
        .timeout(std::time::Duration::from_secs(10)).send().await
    {
        Ok(r) if r.status().is_success() => {
            let stats: Value = r.json().await.unwrap_or_default();
            let vram = stats["devices"][0]["vram_total"].as_f64().map(|v| v / 1e9);
            Ok(json!({ "ok": true, "detail": match vram {
                Some(gb) => format!("Video server answering — {:.0} GB GPU.", gb),
                None => "Video server answering.".to_string(),
            }, "vram_gb": vram }))
        }
        Ok(r) => Ok(json!({ "ok": false, "detail": format!("Server answered {}.", r.status()) })),
        Err(err) => Ok(json!({ "ok": false, "detail": format!("No answer: {err}"),
                               "next_step": "The tunnel rotates every run — press Start & connect again." })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every preset must produce a graph that parses and leaves nothing unfilled. An unfilled
    /// `__TOKEN__` survives into the submitted JSON and is rejected by ComfyUI with a message about
    /// node inputs, which reads as a server fault rather than a missing substitution.
    #[test]
    fn every_preset_builds_a_complete_graph() {
        for p in vm::PRESETS {
            let img = if vm::model_of(p).unwrap().drive == Drive::ImageToVideo { Some("still.png") } else { None };
            let g = build_graph(p.id, "a river at dawn", "static", 42, img)
                .unwrap_or_else(|e| panic!("{}: {e}", p.id));
            let text = serde_json::to_string(&g).unwrap();
            assert!(!text.contains("__"), "{} left a placeholder: {}", p.id,
                    text.split("__").nth(1).unwrap_or(""));
            assert!(g.as_object().unwrap().len() > 4, "{} produced a stub graph", p.id);
            assert!(!text.contains("_comment"), "{} kept its comment, which ComfyUI reads as a node", p.id);
        }
    }

    /// An image-to-video preset with no image must refuse, and say which it is.
    #[test]
    fn animating_nothing_is_refused_with_a_reason() {
        for p in vm::PRESETS.iter().filter(|p| vm::model_of(p).unwrap().drive == Drive::ImageToVideo) {
            let err = build_graph(p.id, "prompt", "", 1, None).unwrap_err();
            assert!(err.contains("needs one") || err.contains("existing picture"), "{err}");
        }
    }

    /// A prompt with quotes and newlines must not be able to break the JSON.
    #[test]
    fn a_hostile_prompt_cannot_break_the_graph() {
        let nasty = "a \"quoted\" thing,\nnewline \\ backslash {\"node\": 1}";
        let g = build_graph("broll_wide", nasty, "", 7, None).unwrap();
        let found = serde_json::to_string(&g).unwrap();
        assert!(found.contains("quoted"), "the prompt should survive, escaped");
        // Round-trips, which is the actual property that matters.
        let _: Value = serde_json::from_str(&found).unwrap();
    }

    #[test]
    fn every_model_family_has_a_graph() {
        for m in vm::MODELS {
            assert!(graph_for(m.id).is_some(), "model '{}' has no workflow graph", m.id);
        }
    }

    #[test]
    fn an_unknown_preset_is_an_error_not_a_panic() {
        assert!(build_graph("nope", "x", "", 1, None).is_err());
    }

    /// The graph must name the weights by the basename ComfyUI sees, not the repo path they were
    /// downloaded from — `split_files/vae/wan_2.1_vae.safetensors` is not a filename on disk.
    #[test]
    fn weights_are_named_by_basename() {
        let g = build_graph("hero_wide", "x", "", 1, None).unwrap();
        let text = serde_json::to_string(&g).unwrap();
        assert!(text.contains("wan2.1_t2v_1.3B_fp16.safetensors"));
        assert!(!text.contains("split_files"), "a repo path leaked into the graph: {text}");
    }
}
