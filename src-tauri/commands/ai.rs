use crate::state::AppState;
use bson::{doc, Document};
use crate::store::UpdateOptions;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;
use regex::Regex;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

// ────────────────────────────────────────────────────────────────
// Automatic fallback when the chosen provider is overloaded
//
// Free tiers fail by being busy, not by being broken: Gemini's free quota answers "the model is
// overloaded" (503 / RESOURCE_EXHAUSTED) under load, which surfaced in the app as "the AI is busy"
// and lost the whole request. OpenRouter's free Nemotron 3 Ultra is far less contended, so whenever
// the primary fails with an overload-class error and an OpenRouter key exists, the request is
// retried once on that model instead of failing. Every such switch leaves a notice the UI drains
// (`take_ai_notices`) and toasts, so an automatic provider change is never silent.
// ────────────────────────────────────────────────────────────────

/// The always-free model used when the configured provider is overloaded.
pub const FALLBACK_MODEL: &str = "nvidia/nemotron-3-ultra-550b-a55b:free";

/// Pending one-shot notices for the UI. In-memory only — a notice describes the request that just
/// happened, so there is nothing worth persisting across restarts.
static AI_NOTICES: std::sync::Mutex<Vec<Value>> = std::sync::Mutex::new(Vec::new());

/// Is this failure the kind another provider might succeed at right now?
///
/// Deliberately narrow: a bad key, an unknown model or a malformed request (HTTP 400/401/403/404)
/// would fail identically on the fallback, and retrying those would only hide a misconfiguration.
fn is_overload_error(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    let http_status = |code: &str| m.contains(&format!("http {code}"));
    ["429", "500", "502", "503", "504", "529"].iter().any(|c| http_status(c))
        || m.contains("overload")
        || m.contains("resource_exhausted")
        || m.contains("rate limit") || m.contains("rate-limit") || m.contains("ratelimit")
        || m.contains("quota")
        || m.contains("too many requests")
        || m.contains("unavailable")
        || m.contains("is busy") || m.contains("try again")
        || m.contains("timed out") || m.contains("timeout")
        || m.contains("connection closed") || m.contains("connection reset")
}

/// Record that a request was served by the fallback. Repeats within a minute (a batch translation
/// makes many calls in a row) collapse into one notice with a count, so the user gets one toast.
fn note_fallback(from: &str, to: &str, reason: &str) {
    let Ok(mut notices) = AI_NOTICES.lock() else { return };
    let now = chrono::Utc::now();
    let short: String = reason.chars().take(160).collect();
    if let Some(last) = notices.last_mut() {
        let same = last["from"].as_str() == Some(from) && last["to"].as_str() == Some(to);
        let recent = last["at"].as_str()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|t| (now - t.with_timezone(&chrono::Utc)).num_seconds() < 60)
            .unwrap_or(false);
        if same && recent {
            let n = last["count"].as_i64().unwrap_or(1) + 1;
            last["count"] = serde_json::json!(n);
            last["at"] = serde_json::json!(now.to_rfc3339());
            return;
        }
    }
    notices.push(serde_json::json!({
        "kind": "ai_fallback",
        "from": from,
        "to": to,
        "reason": short,
        "count": 1,
        "at": now.to_rfc3339(),
    }));
    // Keep the buffer bounded if the UI never drains it (e.g. a headless/scheduler-only run).
    let len = notices.len();
    if len > 8 { notices.drain(..len - 8); }
}

// ────────────────────────────────────────────────────────────────
// Provider catalogue + live model discovery
//
// Model names move faster than any list baked into an app, so the UI asks the provider itself what
// the user's key can reach. The catalogue below is only what a *person* needs to choose a provider —
// what it costs, whether it is free, and where its key comes from.
// ────────────────────────────────────────────────────────────────

/// The text providers this app can use, with the facts a user picks between.
#[tauri::command]
pub async fn list_ai_providers() -> Res<Value> {
    Ok(serde_json::json!({ "providers": [
        {
            "id": "openrouter", "label": "OpenRouter", "tier": "free",
            "detail": "Free access to capable open models. The free tier allows 50 requests a day for the whole account; adding $10 of credit raises it to 1,000.",
            "key_url": "https://openrouter.ai/keys",
            "key_field": "openrouter_api_key", "model_field": "openrouter_model",
            "login": "internal",
        },
        {
            "id": "gemini", "label": "Google Gemini", "tier": "free",
            "detail": "Free tier with per-minute limits; often busy at peak. A billed key removes both. Also powers the assistant's voice.",
            "key_url": "https://aistudio.google.com/apikey",
            "key_field": "gemini_api_key", "model_field": "gemini_model",
            // Google blocks sign-in inside embedded webviews, so this one has to leave the app.
            "login": "external",
            "login_note": "Google refuses to sign in inside an embedded browser, so this page opens in your normal browser.",
        },
        {
            "id": "anthropic", "label": "Claude (Anthropic)", "tier": "paid",
            "detail": "Strongest writing and instruction-following of the three paid options. Pay per token, no subscription.",
            "key_url": "https://console.anthropic.com/settings/keys",
            "key_field": "anthropic_api_key", "model_field": "anthropic_model",
            "login": "internal",
        },
        {
            "id": "openai", "label": "ChatGPT (OpenAI)", "tier": "paid",
            "detail": "Pay per token via the API — separate from a ChatGPT Plus subscription, which does not include API access.",
            "key_url": "https://platform.openai.com/api-keys",
            "key_field": "openai_api_key", "model_field": "openai_model",
            "login": "internal",
        },
    ]}))
}

/// What the free tiers have left today, and what a run would cost.
///
/// Exists so the answer arrives *before* the spending. "This run needs 14 requests and you have 9"
/// is something a user can act on; the same fact discovered at request 9 is half a song and a
/// wasted evening. The counts are real — every call through `provider_chat` is recorded — rather
/// than the job-count proxy `quota_report` used to offer, which was unrelated to AI usage in both
/// directions.
#[tauri::command]
pub async fn ai_budget_report(state: State<'_, AppState>) -> Res<Value> {
    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" })
        .with_options(proj0())
        .await.map_err(e)?
        .map(bson_to_value)
        .unwrap_or_default();
    let usage = crate::ai_budget::usage_today();

    let providers: Vec<Value> = ["openrouter", "gemini", "anthropic", "openai"].iter().map(|p| {
        let used = usage.get(*p).and_then(|v| v.as_u64()).unwrap_or(0);
        serde_json::json!({
            "id": p,
            "configured": crate::ai_budget::configured(p, &settings),
            "used_today": used,
            "daily_cap": crate::ai_budget::daily_cap(p, &settings),
            "remaining": crate::ai_budget::remaining(p, &settings),
        })
    }).collect();

    let order = crate::ai_budget::rotation(&settings);
    let free_left: u64 = ["openrouter", "gemini"].iter()
        .filter(|p| crate::ai_budget::configured(p, &settings))
        .filter_map(|p| crate::ai_budget::remaining(p, &settings))
        .sum();

    Ok(serde_json::json!({
        "providers": providers,
        "rotation": order,
        "free_remaining": free_left,
        "cost": crate::ai_budget::workflow_cost(),
        // Enough for a whole song, or not. The number the guide actually needs.
        "songs_left_today": free_left / 4,
    }))
}

/// Ask a provider which models this key can actually use.
///
/// Live, because a hardcoded model list goes stale the moment a provider ships something new — and a
/// stale id is a 404 the user cannot diagnose. Returns `[]` (never an error) when the key is missing,
/// so the picker can just fall back to a text field.
#[tauri::command]
pub async fn list_ai_models(state: State<'_, AppState>, provider: String) -> Res<Value> {
    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).with_options(proj0()).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build().map_err(e)?;
    let key_of = |field: &str| settings[field].as_str().unwrap_or("").trim().to_string();

    let (url, req) = match provider.as_str() {
        "anthropic" => {
            let key = key_of("anthropic_api_key");
            if key.is_empty() { return Ok(serde_json::json!({ "models": [] })); }
            ("https://api.anthropic.com/v1/models?limit=100",
             http.get("https://api.anthropic.com/v1/models?limit=100")
                 .header("x-api-key", key).header("anthropic-version", "2023-06-01"))
        }
        "openai" => {
            let key = key_of("openai_api_key");
            if key.is_empty() { return Ok(serde_json::json!({ "models": [] })); }
            ("https://api.openai.com/v1/models",
             http.get("https://api.openai.com/v1/models").header("Authorization", format!("Bearer {key}")))
        }
        "gemini" => {
            let key = key_of("gemini_api_key");
            if key.is_empty() { return Ok(serde_json::json!({ "models": [] })); }
            ("https://generativelanguage.googleapis.com/v1beta/models?pageSize=200",
             http.get("https://generativelanguage.googleapis.com/v1beta/models?pageSize=200")
                 .header("x-goog-api-key", key))
        }
        "openrouter" => ("https://openrouter.ai/api/v1/models",
                         http.get("https://openrouter.ai/api/v1/models")),
        other => return Err(format!("Unknown AI provider '{other}'.")),
    };

    let r = req.send().await.map_err(|err| format!("Could not reach {provider} ({url}): {err}"))?;
    if !r.status().is_success() {
        let status = r.status();
        let txt = r.text().await.unwrap_or_default();
        return Err(format!("{provider} rejected the model list (HTTP {status}): {}",
            txt.chars().take(200).collect::<String>()));
    }
    let res: Value = r.json().await.map_err(e)?;

    let models = normalise_models(&provider, &res);
    Ok(serde_json::json!({ "models": models, "provider": provider }))
}


/// Normalise a provider's model list into `{ id, label, free }`, newest-friendly order.
///
/// Each provider answers in its own shape, and each mixes in models that cannot chat at all —
/// embeddings, text-to-speech, image and moderation endpoints. Filtering them here is what keeps the
/// picker from offering a model that 400s on the first prompt. Free models sort first.
fn normalise_models(provider: &str, res: &Value) -> Vec<Value> {
    let mut models: Vec<Value> = match provider {
        "anthropic" => res["data"].as_array().map(|a| a.iter().map(|m| serde_json::json!({
            "id": m["id"],
            "label": m["display_name"].as_str().unwrap_or(m["id"].as_str().unwrap_or("")),
            "free": false,
        })).collect()).unwrap_or_default(),
        "openai" => res["data"].as_array().map(|a| a.iter()
            .filter(|m| {
                let id = m["id"].as_str().unwrap_or("");
                (id.starts_with("gpt") || id.starts_with('o'))
                    && !["embedding", "tts", "whisper", "audio", "image", "dall-e", "moderation", "realtime", "transcribe"]
                        .iter().any(|bad| id.contains(bad))
            })
            .map(|m| serde_json::json!({ "id": m["id"], "label": m["id"], "free": false }))
            .collect()).unwrap_or_default(),
        "gemini" => res["models"].as_array().map(|a| a.iter()
            .filter(|m| m["supportedGenerationMethods"].as_array()
                .map(|ms| ms.iter().any(|x| x == "generateContent")).unwrap_or(false))
            .filter(|m| {
                let n = m["name"].as_str().unwrap_or("");
                !n.contains("tts") && !n.contains("image") && !n.contains("embedding")
            })
            .map(|m| {
                let id = m["name"].as_str().unwrap_or("").rsplit('/').next().unwrap_or("").to_string();
                let label = m["displayName"].as_str().unwrap_or(&id).to_string();
                serde_json::json!({ "id": id, "label": label, "free": true })
            }).collect()).unwrap_or_default(),
        // OpenRouter and anything else that answers in its shape.
        _ => res["data"].as_array().map(|a| a.iter().map(|m| {
            let id = m["id"].as_str().unwrap_or("").to_string();
            let label = m["name"].as_str().unwrap_or(&id).to_string();
            serde_json::json!({ "id": id.clone(), "label": label, "free": id.ends_with(":free") })
        }).collect()).unwrap_or_default(),
    };
    models.sort_by(|a, b| {
        b["free"].as_bool().unwrap_or(false).cmp(&a["free"].as_bool().unwrap_or(false))
            .then_with(|| a["id"].as_str().unwrap_or("").cmp(b["id"].as_str().unwrap_or("")))
    });
    models
}

/// Drain pending AI notices. Polled by the UI alongside the job list; each notice is delivered once.
#[tauri::command]
pub async fn take_ai_notices() -> Res<Value> {
    let drained = AI_NOTICES.lock().map(|mut n| std::mem::take(&mut *n)).unwrap_or_default();
    Ok(serde_json::json!({ "notices": drained }))
}

fn proj0() -> crate::store::FindOneOptions {
    crate::store::FindOneOptions::builder().projection(doc! { "_id": 0 }).build()
}

fn bson_to_value(doc: Document) -> Value {
    let mut m = serde_json::Map::new();
    for (k, v) in doc {
        if k == "_id" { continue; }
        if let Ok(jv) = bson::from_bson::<Value>(v) { m.insert(k, jv); }
    }
    Value::Object(m)
}

/// Attach channelId/channelName from the request's own targets onto each generated item, so
/// lyrics stay linked to the channel they were generated for — without the AI ever having to
/// understand the concept of a "channel" or the response schema having to encode one. Targets
/// are sent to the LLM "one JSON element per target" in order, so matching is positional first;
/// that positional guess is only trusted when the item's language actually lines up (or either
/// side left it blank), and falls back to a language lookup otherwise — a cheap guard against a
/// reordered or dropped item silently inheriting the wrong channel.
fn attach_channel_info(items: &mut [Value], targets: &[Value]) {
    for (i, item) in items.iter_mut().enumerate() {
        let positional = targets.get(i).filter(|t| {
            let tl = t["language"].as_str().unwrap_or("");
            let il = item["language"].as_str().unwrap_or("");
            tl.is_empty() || il.is_empty() || tl.eq_ignore_ascii_case(il)
        });
        let target = positional.or_else(|| {
            let il = item["language"].as_str().unwrap_or("");
            if il.is_empty() { return None; }
            targets.iter().find(|t| t["language"].as_str().unwrap_or("").eq_ignore_ascii_case(il))
        });
        if let Some(target) = target {
            if let Some(cid) = target["channelId"].as_str().filter(|s| !s.is_empty()) {
                item["channel_id"] = Value::String(cid.to_string());
            }
            if let Some(cname) = target["channelName"].as_str().filter(|s| !s.is_empty()) {
                item["channel_name"] = Value::String(cname.to_string());
            }
        }
    }
}

#[tauri::command]
pub async fn get_compose_config(state: State<'_, AppState>) -> Res<Value> {
    let doc = state.db.collection::<Document>("compose_configs")
        .find_one(doc! { "_id": "singleton" })
        .with_options(proj0())
        .await.map_err(e)?;
    Ok(doc.map(bson_to_value).unwrap_or_else(|| serde_json::json!({})))
}

#[tauri::command]
pub async fn save_compose_config(state: State<'_, AppState>, payload: Value) -> Res<Value> {
    let mut body = payload.clone();
    if let Some(obj) = body.as_object_mut() {
        obj.remove("_id");
    }
    let bson = bson::to_document(&body).map_err(e)?;
    state.db.collection::<Document>("compose_configs")
        .update_one(doc! { "_id": "singleton" }, doc! { "$set": &bson })
        .with_options(UpdateOptions::builder().upsert(true).build())
        .await.map_err(e)?;
    Ok(body)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeRequest {
    pub chapter_text: String,
    /// When set, the project's Brief + daily topic + cached per-channel research are pulled into
    /// the prompt so lyrics/translations speak with the project's voice, adapted per channel.
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub sections: Vec<Value>,
    #[serde(default)]
    pub targets: Vec<Value>,
    #[serde(default)]
    pub themes: Value,
    #[serde(default)]
    pub mj_params: String,
    #[serde(default)]
    pub style_keywords: Vec<String>,
    #[serde(default)]
    pub generate: Value,
    #[serde(default)]
    pub title_pattern: String,
    #[serde(default)]
    pub artist: String,
}

/// Request for the freeform (non-Bible) topic composer. The user roughly describes a topic
/// (optionally via mic → text), a daily mood/vibe driver, and a set of targets (each a
/// language + genre, ultimately its own channel). The AI produces per-target songs that are
/// culturally + genre adapted so each speaks to its regional/genre audience.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreeformComposeRequest {
    /// Rough free-text description of what the song(s) should be about.
    pub topic: String,
    /// Today's driver of humor/wellbeing/mood — how the theme should "vibe" right now.
    #[serde(default)]
    pub mood_driver: String,
    /// Optional per-section ideas the user wants reflected.
    #[serde(default)]
    pub sections: Vec<Value>,
    /// Targets to produce, each e.g. { "language": "Spanish", "genre": "reggaeton", "channel": "..." }.
    #[serde(default)]
    pub targets: Vec<Value>,
    /// When true, the AI adapts each translation + genre to its regional/genre culture.
    #[serde(default = "yes_bool")]
    pub cultural_influence: bool,
    #[serde(default)]
    pub title_pattern: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub image_style_suffix: String,
}

fn yes_bool() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestTransitionsRequest {
    pub song_id: String,
    /// "manual" (uniform default), "assist" (rule-based per scene), or "full" (AI per scene).
    #[serde(default = "default_automation")]
    pub automation: String,
    /// Allowed transition vocabulary (from the chosen preset pack).
    #[serde(default)]
    pub allowed: Vec<String>,
    #[serde(default)]
    pub default_transition: String,
}
fn default_automation() -> String { "assist".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixGenresRequest {
    /// Genre descriptor strings to fuse (e.g. Suno-style keyword lists).
    #[serde(default)]
    pub genres: Vec<String>,
    /// Optional mood/vibe to bias the fusion.
    #[serde(default)]
    pub mood: String,
    /// Use the LLM to intelligently fuse (true) or the fast deterministic combiner (false).
    #[serde(default = "yes_bool")]
    pub ai: bool,
    /// Optional "superflavor" laid OVER the fused base as a top layer (christian projects):
    /// "jewish" | "ancient-jewish" | "messianic". Empty/None = plain fusion, no top layer.
    #[serde(default)]
    pub flavor_layer: Option<String>,
}

/// Instrumentation/character brief for each superflavor top layer. These describe a LAYER over an
/// existing fused base — the base genres keep driving rhythm and form; the flavor colors them.
fn flavor_layer_brief(layer: &str) -> Option<&'static str> {
    match layer {
        "jewish" => Some(
            "Overlay a JEWISH musical top layer: klezmer clarinet and violin lines, freygish/Ahava \
             Raba scale colorings, hora/freylekhs rhythmic accents, cimbalom or accordion touches — \
             woven OVER the base mix as ornament and melody, not replacing its groove."),
        "ancient-jewish" => Some(
            "Overlay an ANCIENT-JEWISH top layer: shofar calls, kinnor/lyre and harp figures, frame \
             drums (tof), reed flutes, antiphonal temple-chant feel, modal (non-tempered) inflections — \
             an ancient ceremonial atmosphere floating over the modern base."),
        "messianic" => Some(
            "Overlay a MESSIANIC-JEWISH worship top layer: Davidic-dance rhythmic lift, Hebrew worship \
             hooks and call-and-response, violin+guitar interplay, joyful minor-major turns, modern \
             worship production blended with klezmer/Israeli folk color over the base."),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposerAssistRequest {
    #[serde(default)]
    pub preset: String,
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub system_prompt: String,
    #[serde(default)]
    pub user_prompt: String,
    #[serde(default)]
    pub context: Value,
    #[serde(default)]
    pub json_mode: bool,
    #[serde(default = "default_assist_temperature")]
    pub temperature: f32,
}

fn default_assist_temperature() -> f32 { 0.55 }

/// Public helper: call OpenRouter with a system prompt and user message,
/// accepting a DB reference (not Tauri State) so it can be used from
/// other modules like characters.rs.
/// Provider-agnostic single-shot chat call. Reads `ai_provider` from the settings document and
/// dispatches to OpenRouter (default) or Google Gemini (free tier). Returns `(content, model_label)`.
///
/// Both the lyrics composer and the small assist helpers route through this, so switching the
/// provider in Settings transparently changes which backend every AI feature uses.
/// Section-annotation convention per music engine. The lyric text reaches the engine verbatim, and
/// each one parses structure differently — Suno reads rich bracketed performance tags, ACE-Step
/// wants plain lowercase structure tags, and HeartMuLa writes the text straight into a lyrics file
/// where anything but a section header is sung. Emitting the wrong dialect degrades the result.
fn engine_lyric_annotation_guide(engine: &str) -> &'static str {
    match engine {
        "suno" => "Target engine: Suno. Put capitalised bracketed structure tags on their own lines: \
                   [Intro], [Verse], [Pre-Chorus], [Chorus], [Bridge], [Outro]. Suno also honours short \
                   performance hints in brackets (e.g. [Instrumental], [Soft female vocal]) — use them \
                   sparingly, only where they genuinely shape the arrangement.",
        "acestep" => "Target engine: ACE-Step. Put plain lowercase bracketed structure tags on their own \
                      lines: [intro], [verse], [chorus], [bridge], [outro]. Do NOT add performance \
                      directions inside the lyric text — ACE-Step does not interpret them.",
        _ => "Target engine: HeartMuLa. The lyric text is written verbatim into a lyrics file, so use only \
              plain capitalised section headers on their own lines: [Verse], [Chorus], [Bridge], [Outro]. \
              No performance directions and no inline notes — anything that is not a section header is sung.",
    }
}

/// Grounded (live-web-search) chat. When the provider is Gemini, this enables the built-in
/// `google_search` tool so the model researches the live web and cites sources; it returns
/// `(content, model_label, sources)`. Grounding is incompatible with JSON response mode, so this is
/// always a plain-text call — callers that need structure parse the text themselves.
///
/// When the provider isn't Gemini (e.g. OpenRouter/Qwen, which has no native search) or no Gemini
/// key is set, it transparently falls back to `provider_chat` (ungrounded, model knowledge only),
/// returning an empty `sources` list so callers can tell the difference and label the result.
pub async fn provider_chat_grounded(
    settings_doc: &Value,
    system: &str,
    user: &str,
    temperature: f32,
) -> Result<(String, String, Vec<Value>), String> {
    let provider = settings_doc["ai_provider"].as_str().unwrap_or("openrouter").trim();
    let gkey = settings_doc["gemini_api_key"].as_str().unwrap_or("").trim();
    if provider != "gemini" || gkey.is_empty() {
        let (content, model) = provider_chat(settings_doc, system, user, temperature, false).await?;
        return Ok((content, model, Vec::new()));
    }

    let mut model = settings_doc["gemini_model"].as_str()
        .filter(|s| !s.trim().is_empty()).unwrap_or("gemini-3.5-flash").to_string();
    if matches!(model.as_str(), "gemini-2.0-flash" | "gemini-2.0-flash-lite" | "gemini-1.5-flash" | "gemini-1.5-pro" | "gemini-2.5-flash" | "gemini-2.5-flash-lite") {
        model = "gemini-3.5-flash".to_string();
    }
    let timeout_s = settings_doc["ai_timeout_s"].as_f64()
        .or_else(|| settings_doc["ai_timeout_s"].as_str().and_then(|s| s.trim().parse().ok()))
        .filter(|v| *v >= 5.0).unwrap_or(120.0) as u64;
    let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(timeout_s)).build().map_err(e)?;

    // `google_search` is the current grounding tool; no responseMimeType (incompatible with tools).
    let body = serde_json::json!({
        "system_instruction": { "parts": [ { "text": system } ] },
        "contents": [ { "role": "user", "parts": [ { "text": user } ] } ],
        "generationConfig": { "temperature": temperature.clamp(0.0, 2.0) },
        "tools": [ { "google_search": {} } ],
    });
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent");
    let r = http.post(&url).header("x-goog-api-key", gkey).json(&body).send().await.map_err(e)?;
    if !r.status().is_success() {
        let status = r.status();
        let txt = r.text().await.unwrap_or_default();
        // Some models/versions reject the tool (400); an overloaded free tier fails with 429/503.
        // Either way an ungrounded answer beats no answer, and `provider_chat` adds its own
        // fallback model on top — so the request survives, just without live citations.
        let safe: String = txt.chars().take(400).collect();
        if status.as_u16() == 400 || is_overload_error(&format!("http {status}: {safe}")) {
            let (content, model2) = provider_chat(settings_doc, system, user, temperature, false).await?;
            return Ok((content, model2, Vec::new()));
        }
        return Err(format!("Gemini (grounded) HTTP {status}: {safe}"));
    }
    let res: Value = r.json().await.map_err(e)?;
    let content = res["candidates"][0]["content"]["parts"].as_array()
        .map(|parts| parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join(""))
        .unwrap_or_default().trim().to_string();
    // groundingMetadata.groundingChunks[].web.{uri,title} are the cited sources.
    let sources: Vec<Value> = res["candidates"][0]["groundingMetadata"]["groundingChunks"].as_array()
        .map(|arr| arr.iter().filter_map(|c| {
            let w = &c["web"];
            w["uri"].as_str().map(|uri| serde_json::json!({ "uri": uri, "title": w["title"].as_str().unwrap_or("") }))
        }).collect())
        .unwrap_or_default();
    if content.is_empty() {
        let (content2, model2) = provider_chat(settings_doc, system, user, temperature, false).await?;
        return Ok((content2, model2, Vec::new()));
    }
    Ok((content, format!("gemini:{model}+search"), sources))
}

/// Provider-agnostic chat with automatic overload fallback (see `FALLBACK_MODEL`).
///
/// Every AI feature funnels through here, so one retry policy covers lyrics, assists, style
/// adaptation, insights and GUI translation alike. The returned model label reports what actually
/// answered — a fallback is tagged so callers that display the model tell the truth.
pub async fn provider_chat(
    settings_doc: &Value,
    system: &str,
    user: &str,
    temperature: f32,
    json_mode: bool,
) -> Result<(String, String), String> {
    // Who can take this call, in order: the chosen provider first, then any other configured one
    // that still has budget today, free tiers before billed ones. Providers already spent for the
    // day are not in the list at all — asking them buys a 429 and a wait (see ai_budget.rs).
    let order = crate::ai_budget::rotation(settings_doc);
    let mut candidates: Vec<Option<String>> = order.iter().map(|p| Some(p.clone())).collect();
    if candidates.is_empty() {
        // Nothing configured, or everything spent. Fall through to the configured provider anyway so
        // the error the user sees comes from the provider itself and says something true, rather
        // than this module inventing one.
        candidates.push(None);
    }

    let mut first_err: Option<String> = None;
    let mut tried: Vec<String> = Vec::new();

    for (i, provider) in candidates.iter().enumerate() {
        let attempt = provider_chat_as(settings_doc, system, user, temperature, json_mode, provider.as_deref()).await;
        match attempt {
            Ok((content, model)) => {
                if i > 0 {
                    // Say which provider actually answered — a caller that displays the model must
                    // not claim the one the user picked when a different one did the work.
                    note_fallback(&tried.join(" → "), &model, first_err.as_deref().unwrap_or(""));
                    return Ok((content, format!("{model} (rotated)")));
                }
                return Ok((content, model));
            }
            Err(err) => {
                tried.push(provider.clone().unwrap_or_else(|| "configured".into()));
                // Only an overload or an exhausted quota is worth rotating for. A bad prompt or a
                // missing key fails identically everywhere, and trying three providers to learn that
                // spends three requests to receive the same sentence.
                let rotatable = is_overload_error(&err);
                if first_err.is_none() { first_err = Some(err.clone()); }
                if !rotatable { return Err(err); }
            }
        }
    }

    let err = first_err.unwrap_or_else(|| "No AI provider is configured.".to_string());
    // The free-tier case deserves its own sentence: "try again later" is true and useless when the
    // limit resets at midnight UTC and the user could switch providers in ten seconds.
    let spent: Vec<String> = ["openrouter", "gemini"].iter()
        .filter(|p| crate::ai_budget::configured(p, settings_doc)
                 && crate::ai_budget::remaining(p, settings_doc) == Some(0))
        .map(|p| p.to_string())
        .collect();
    if !spent.is_empty() {
        return Err(format!(
            "{err} — today's free allowance is used up on: {}. It resets at midnight UTC; another \
             provider's key in Settings would keep you going now.",
            spent.join(", ")
        ));
    }
    Err(err)
}

/// One attempt, against one named provider (or the configured one when `None`), counted against
/// that provider's ledger whether it succeeds or fails.
///
/// Counting failures too is deliberate: a rejected request still consumed the allowance that
/// rejected it, and a counter that only tracks successes is how an app talks itself past a quota it
/// has already spent.
async fn provider_chat_as(
    settings_doc: &Value,
    system: &str,
    user: &str,
    temperature: f32,
    json_mode: bool,
    provider: Option<&str>,
) -> Result<(String, String), String> {
    let mut doc = settings_doc.clone();
    if let Some(p) = provider {
        doc["ai_provider"] = Value::String(p.to_string());
    }
    let counted = provider.map(|s| s.to_string()).unwrap_or_else(|| {
        settings_doc["ai_provider"].as_str().unwrap_or("openrouter").trim().to_string()
    });
    crate::ai_budget::record(&counted);
    provider_chat_once(&doc, system, user, temperature, json_mode, None).await
}

/// One attempt at one provider. `force_openrouter_model` pins the call to OpenRouter with that
/// model regardless of the configured provider (used by the fallback path).
async fn provider_chat_once(
    settings_doc: &Value,
    system: &str,
    user: &str,
    temperature: f32,
    json_mode: bool,
    force_openrouter_model: Option<&str>,
) -> Result<(String, String), String> {
    let provider = if force_openrouter_model.is_some() { "openrouter" }
        else { settings_doc["ai_provider"].as_str().unwrap_or("openrouter").trim() };

    // ── Per-engine tunables ──────────────────────────────────────────
    // Settings-driven knobs that apply to whichever provider is selected. Each is opt-in: blank/0
    // keeps the previous built-in behaviour, so an untouched install behaves exactly as before.
    // The UI form stores numbers as text, hence the string fallbacks.
    let num = |k: &str| -> Option<f64> {
        settings_doc[k].as_f64()
            .or_else(|| settings_doc[k].as_str().and_then(|s| s.trim().parse::<f64>().ok()))
    };
    let timeout_s = num("ai_timeout_s").filter(|v| *v >= 5.0).unwrap_or(120.0) as u64;
    let max_tokens = num("ai_max_tokens").filter(|v| *v > 0.0).map(|v| v as u64);
    // An explicit temperature overrides the per-call value (call sites pick e.g. 0.9 for style
    // variation); leave blank to keep those per-task defaults.
    let temperature = num("ai_temperature").map(|v| v as f32).unwrap_or(temperature);

    if provider == "gemini" {
        let key = settings_doc["gemini_api_key"].as_str().unwrap_or("").trim().to_string();
        let mut model = settings_doc["gemini_model"].as_str()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("gemini-3.5-flash")
            .to_string();
        // Self-heal models Google has shut down (a stale saved value otherwise breaks
        // every generation with an "outdated model" error until the user re-picks).
        if matches!(model.as_str(), "gemini-2.0-flash" | "gemini-2.0-flash-lite" | "gemini-1.5-flash" | "gemini-1.5-pro" | "gemini-2.5-flash" | "gemini-2.5-flash-lite") {
            model = "gemini-3.5-flash".to_string();
        }
        if key.is_empty() {
            return Err("Configure gemini_api_key in Settings (get one free at aistudio.google.com/apikey).".to_string());
        }

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_s))
            .build()
            .map_err(e)?;

        let mut gen_config = serde_json::json!({ "temperature": temperature.clamp(0.0, 2.0) });
        if json_mode {
            gen_config["responseMimeType"] = serde_json::json!("application/json");
        }
        if let Some(mt) = max_tokens {
            gen_config["maxOutputTokens"] = serde_json::json!(mt);
        }
        let body = serde_json::json!({
            "system_instruction": { "parts": [ { "text": system } ] },
            "contents": [ { "role": "user", "parts": [ { "text": user } ] } ],
            "generationConfig": gen_config,
        });

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"
        );
        let r = http.post(&url)
            .header("x-goog-api-key", &key)
            .json(&body)
            .send().await.map_err(e)?;
        if !r.status().is_success() {
            let status = r.status();
            let txt = r.text().await.unwrap_or_default();
            let safe_txt = if txt.len() > 400 { &txt[..400] } else { &txt };
            return Err(format!("Gemini HTTP {status}: {safe_txt}"));
        }
        let res_json: Value = r.json().await.map_err(e)?;
        // Concatenate all text parts of the first candidate.
        let content = res_json["candidates"][0]["content"]["parts"]
            .as_array()
            .map(|parts| parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join(""))
            .unwrap_or_default()
            .trim()
            .to_string();
        if content.is_empty() {
            let reason = res_json["candidates"][0]["finishReason"].as_str().unwrap_or("unknown");
            return Err(format!("Gemini returned no text (finishReason: {reason})"));
        }
        return Ok((content, format!("gemini:{model}")));
    }

    // ── Anthropic (paid) ─────────────────────────────────────────────
    //
    // The Messages API differs from the OpenAI shape in three ways that matter here: `max_tokens` is
    // required, the system prompt is its own top-level field rather than a message, and sampling
    // parameters (`temperature`, `top_p`, `top_k`) are REJECTED with a 400 on the current models —
    // so temperature is deliberately not sent. JSON mode is asked for in the prompt; the caller
    // already extracts JSON defensively.
    if provider == "anthropic" {
        let key = settings_doc["anthropic_api_key"].as_str().unwrap_or("").trim().to_string();
        if key.is_empty() {
            return Err("Configure anthropic_api_key in Settings (console.anthropic.com/settings/keys).".to_string());
        }
        let model = settings_doc["anthropic_model"].as_str()
            .filter(|s| !s.trim().is_empty()).unwrap_or("claude-opus-5").to_string();

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_s)).build().map_err(e)?;
        let system_text = if json_mode {
            format!("{system}\n\nReturn ONLY valid JSON — no prose, no markdown fences.")
        } else { system.to_string() };
        let body = serde_json::json!({
            "model": model,
            "max_tokens": max_tokens.unwrap_or(8192),
            "system": system_text,
            "messages": [ { "role": "user", "content": user } ],
        });
        let r = http.post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body).send().await.map_err(e)?;
        if !r.status().is_success() {
            let status = r.status();
            let txt = r.text().await.unwrap_or_default();
            return Err(format!("Anthropic HTTP {status}: {}", txt.chars().take(400).collect::<String>()));
        }
        let res: Value = r.json().await.map_err(e)?;
        // A safety classifier can decline: HTTP 200, stop_reason "refusal", empty content.
        if res["stop_reason"].as_str() == Some("refusal") {
            let why = res["stop_details"]["category"].as_str().unwrap_or("policy");
            return Err(format!("Anthropic declined this request ({why}). Try another provider for it."));
        }
        let content = res["content"].as_array()
            .map(|blocks| blocks.iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>().join(""))
            .unwrap_or_default().trim().to_string();
        if content.is_empty() {
            return Err(format!("Anthropic returned no text (stop_reason: {})",
                res["stop_reason"].as_str().unwrap_or("unknown")));
        }
        return Ok((content, format!("anthropic:{model}")));
    }

    // ── OpenAI (paid) ────────────────────────────────────────────────
    if provider == "openai" {
        let key = settings_doc["openai_api_key"].as_str().unwrap_or("").trim().to_string();
        if key.is_empty() {
            return Err("Configure openai_api_key in Settings (platform.openai.com/api-keys).".to_string());
        }
        let model = settings_doc["openai_model"].as_str()
            .filter(|s| !s.trim().is_empty()).unwrap_or("gpt-5.1").to_string();

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_s)).build().map_err(e)?;
        let mut body = serde_json::json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user },
            ],
        });
        // Reasoning models reject a non-default temperature, so it is only sent to models that take it.
        let takes_temperature = !(model.starts_with('o') || model.starts_with("gpt-5"));
        if takes_temperature { body["temperature"] = serde_json::json!(temperature.clamp(0.0, 2.0)); }
        if json_mode { body["response_format"] = serde_json::json!({ "type": "json_object" }); }
        if let Some(mt) = max_tokens { body["max_completion_tokens"] = serde_json::json!(mt); }

        let r = http.post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {key}"))
            .json(&body).send().await.map_err(e)?;
        if !r.status().is_success() {
            let status = r.status();
            let txt = r.text().await.unwrap_or_default();
            return Err(format!("OpenAI HTTP {status}: {}", txt.chars().take(400).collect::<String>()));
        }
        let res: Value = r.json().await.map_err(e)?;
        let content = res["choices"][0]["message"]["content"].as_str().unwrap_or("").trim().to_string();
        if content.is_empty() {
            let reason = res["choices"][0]["finish_reason"].as_str().unwrap_or("unknown");
            return Err(format!("OpenAI returned no text (finish_reason: {reason})"));
        }
        return Ok((content, format!("openai:{model}")));
    }

    // ── OpenRouter (default) ─────────────────────────────────────────
    let key = settings_doc["openrouter_api_key"].as_str().unwrap_or("").trim().to_string();
    let mut model = force_openrouter_model.map(|m| m.to_string()).unwrap_or_else(|| {
        settings_doc["openrouter_model"].as_str()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("qwen/qwen3-next-80b-a3b-instruct:free")
            .to_string()
    });
    // Self-heal the old default that OpenRouter has delisted (stale DBs still carry it).
    if model == "qwen/qwen-2.5-72b-instruct:free" {
        model = "qwen/qwen3-next-80b-a3b-instruct:free".to_string();
    }
    if key.is_empty() {
        return Err("Configure openrouter_api_key in Settings (get one free at openrouter.ai/keys).".to_string());
    }

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_s))
        .user_agent("Lightkid AI Studio")
        .build()
        .map_err(e)?;

    let user_email = settings_doc["openrouter_email"].as_str().unwrap_or("").trim().to_string();

    let mut body = serde_json::json!({
        "model": model,
        "temperature": temperature.clamp(0.0, 1.5),
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user }
        ]
    });
    if json_mode {
        body["response_format"] = serde_json::json!({ "type": "json_object" });
    }
    if let Some(mt) = max_tokens {
        body["max_tokens"] = serde_json::json!(mt);
    }

    let mut req_builder = http.post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {key}"))
        .header("HTTP-Referer", "https://lightkid.studio")
        .header("X-Title", "Lightkid AI Studio");
    if !user_email.is_empty() {
        req_builder = req_builder.header("X-User-Email", &user_email);
    }

    let r = req_builder.json(&body).send().await.map_err(e)?;
    if !r.status().is_success() {
        let status = r.status();
        let txt = r.text().await.unwrap_or_default();
        let safe_txt = if txt.len() > 400 { &txt[..400] } else { &txt };
        return Err(format!("OpenRouter HTTP {status}: {safe_txt}"));
    }
    let res_json: Value = r.json().await.map_err(e)?;
    let content = res_json["choices"][0]["message"]["content"].as_str().unwrap_or("").trim().to_string();
    Ok((content, model))
}

pub async fn call_openrouter(
    db: &crate::store::Db,
    system: &str,
    user: &str,
    temperature: f32,
    json_mode: bool,
) -> Result<Value, String> {
    let settings_doc = db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" })
        .with_options(proj0())
        .await.map_err(e)?
        .map(bson_to_value)
        .unwrap_or_default();

    let (content, model) = provider_chat(&settings_doc, system, user, temperature, json_mode).await?;
    Ok(serde_json::json!({
        "text": content,
        "model": model,
    }))
}

fn raw_preview(content: &str) -> String {
    if content.len() > 1200 { content[..1200].to_string() } else { content.to_string() }
}

pub fn extract_json_value(content: &str) -> Option<Value> {
    let re = Regex::new(r"(?s)(\{.*\}|\[.*\])").ok()?;
    let mat = re.find(content)?;
    serde_json::from_str::<Value>(mat.as_str()).ok()
}

/// State-based convenience wrapper around `provider_chat` used by the interactive assist helpers.
async fn provider_text(
    state: &State<'_, AppState>,
    system: &str,
    user: &str,
    temperature: f32,
    json_mode: bool,
) -> Res<(String, String)> {
    let settings_doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" })
        .with_options(proj0())
        .await.map_err(e)?
        .map(bson_to_value)
        .unwrap_or_default();
    provider_chat(&settings_doc, system, user, temperature, json_mode).await
}

#[tauri::command]
pub async fn compose_assist(state: State<'_, AppState>, payload: ComposerAssistRequest) -> Res<Value> {
    let context = serde_json::to_string_pretty(&payload.context).unwrap_or_else(|_| "{}".to_string());
    let system = if payload.system_prompt.trim().is_empty() {
        "You are a practical creative assistant for a Bible music-video production studio. \
         Help with the current small step only. Prefer concise, directly usable output. \
         When asked for JSON, return valid JSON with no markdown fences."
    } else {
        payload.system_prompt.trim()
    };
    let mut user = String::new();
    // The creator profile leads, so every assist reply lands in their voice rather than a
    // generic one (see commands/social.rs — derived from their own posts, empty until they
    // connect an account and refresh it).
    let profile_block = crate::commands::social::taste_profile_block().await;
    if !profile_block.is_empty() {
        user.push_str(&profile_block);
        user.push('\n');
    }
    if !payload.preset.trim().is_empty() {
        user.push_str(&format!("Preset: {}\n", payload.preset.trim()));
    }
    if !payload.task.trim().is_empty() {
        user.push_str(&format!("Task:\n{}\n\n", payload.task.trim()));
    }
    user.push_str(&format!("Context JSON:\n{}\n\n", context));
    if !payload.user_prompt.trim().is_empty() {
        user.push_str(&format!("User instructions:\n{}\n", payload.user_prompt.trim()));
    }

    match provider_text(&state, system, &user, payload.temperature, payload.json_mode).await {
        Ok((text, model)) => {
            Ok(serde_json::json!({
                "text": text,
                "json": extract_json_value(&text),
                "model": model,
                "preset": payload.preset,
            }))
        }
        Err(err) => Ok(serde_json::json!({
            "error": err,
            "text": "",
            "json": null,
            "preset": payload.preset,
        })),
    }
}

#[tauri::command]
pub async fn compose_lyrics(state: State<'_, AppState>, payload: ComposeRequest) -> Res<Value> {
    // 1. Fetch settings to get openrouter_api_key and openrouter_model
    let settings_doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" })
        .with_options(proj0())
        .await.map_err(e)?
        .map(bson_to_value)
        .unwrap_or_default();
    
    // 2. Build the system and user prompts exactly like the Python server did
    // `lyrics` MUST be a plain string, not an array of {section, lines} objects: Song.lyrics is
    // a plain string field in the DB, import_lyrics reads it with `.as_str()`, and every
    // downstream consumer (Suno automation, the Music Studio lyrics editor, annotation parsing)
    // expects flat text. An earlier version of this prompt asked for an array here, which meant
    // import_lyrics's `.as_str()` silently failed and every imported song ended up with empty
    // lyrics — invisible in the AI Composer's own results panel (which specifically handles
    // both shapes) but missing everywhere else, and the array shape also crashed the Lyrics
    // editor's preview (React can't render an array of plain objects as children).
    // The lyric text is consumed VERBATIM by whichever music engine is selected, and each engine
    // reads section structure differently — so the composer is told the right convention up front
    // instead of emitting one generic format that only some engines understand.
    let engine = {
        let sdoc = state.db.collection::<Document>("settings")
            .find_one(doc! { "_id": "singleton" })
            .with_options(proj0())
            .await.ok().flatten()
            .map(bson_to_value).unwrap_or_default();
        sdoc["music_engine"].as_str().unwrap_or("heartmula").to_string()
    };
    let annotation_guide = engine_lyric_annotation_guide(&engine);

    let sys = format!(
        "You compose multilingual song lyrics JSON for AI music video production.\n\
         Return ONLY a valid JSON array — no prose, no markdown fences.\n\
         Each element MUST have: title (string), language (string), styles (string), \
         lyrics (string): the full lyrics as plain text with section headers on their own lines, \
         annotations (string), image_styles (string).\n\
         SECTION STRUCTURE — {}\n\
         annotations format: alternating lines — first a bracketed midjourney-style image prompt in square brackets, then the matching lyric line.\n\
         IMAGERY: give EACH section its own distinct visual idea and carry it through that section's \
         lines — consecutive sections must not repeat the same scene. Let the imagery progress with the \
         song (establishing shot → development → climax → resolution) so the finished video has visual variety.\n\
         Translate lyrics into the target language faithfully if it isn't English. Keep section ideas + themes embedded in the bracket prompts.",
        annotation_guide
    );
    let sys = sys.as_str();

    let targets_str = serde_json::to_string(&payload.targets).unwrap_or_default();
    let sections_str = serde_json::to_string(&payload.sections).unwrap_or_default();
    let style_kw = payload.style_keywords.join(", ");
    let image_styles_suffix = format!("{} {}", payload.mj_params, style_kw).trim().to_string();
    
    // Extract generate fields and custom lists
    let gen = payload.generate.clone();
    let mut gen_fields = Vec::new();
    let mut keep_list = Vec::new();
    if let Some(obj) = gen.as_object() {
        for (k, v) in obj {
            if v.as_bool().unwrap_or(false) {
                gen_fields.push(k.clone());
            } else {
                keep_list.push(k.clone());
            }
        }
    }
    
    let global_theme = payload.themes["global"].as_str().unwrap_or("");
    let per_lang_themes = serde_json::to_string(&payload.themes["per_language"]).unwrap_or_else(|_| "{}".to_string());
    let per_chan_themes = serde_json::to_string(&payload.themes["per_channel"]).unwrap_or_else(|_| "{}".to_string());

    // The project's creative DNA + cached per-channel research, when a project is in play. These
    // are what make each channel's version feel native (regional, cultural, linguistic) and
    // on-brand (mood/audience/personality + today's angle) rather than a bare translation.
    let brief_block = project_brief_block(&state.db, &payload.project_id).await;
    let research_block = if payload.project_id.trim().is_empty() {
        String::new()
    } else {
        channel_research_block(&state.db).await
    };
    // The user's accumulated taste (from JSON learnings files), so output drifts toward what they
    // keep choosing over time.
    let learnings_block = crate::commands::learnings::learnings_prompt_block(state.inner(), &payload.project_id).await;
    // Who the creator actually is, derived from their own social posts (see commands/social.rs).
    // The project brief says what this project is about; this says whose voice it should be in.
    let profile_block = crate::commands::social::taste_profile_block().await;

    let user_prompt = format!(
        "{profile_block}{brief_block}{research_block}{learnings_block}\
         Source chapter text:\n{}\n\n\
         User-authored section ideas (apply to bracket prompts when matching lines):\n{}\n\n\
         Global theme: {}\n\
         Per-language themes: {}\n\
         Per-channel themes: {}\n\n\
         Targets to produce (one JSON element per target):\n{}\n\n\
         Title pattern: {} (artist={}). {{channel}} = that target's channelName from the targets list \
         (omit that segment if the target has none). Do NOT put the full styles/genre list in the title \
         — titles are typed into a song-title field with a hard length limit, so keep the rendered title \
         under 50 characters total, no matter what the pattern implies.\n\
         image_styles must end with: {}\n\
         Fields the user wants AI to GENERATE: {:?}; KEEP empty/template for: {:?}.\n\
         Output the JSON array now.",
        payload.chapter_text,
        sections_str,
        global_theme,
        per_lang_themes,
        per_chan_themes,
        targets_str,
        payload.title_pattern,
        payload.artist,
        image_styles_suffix,
        gen_fields,
        keep_list
    );

    // 3. Call the configured LLM provider (OpenRouter or Gemini).
    let (content, model) = match provider_chat(&settings_doc, sys, &user_prompt, 0.85, false).await {
        Ok(pair) => pair,
        Err(err) => {
            return Ok(serde_json::json!({ "error": err, "items": [] }));
        }
    };

    // 4. Try to extract and parse the JSON array, handling truncated responses
    let re = Regex::new(r"(?s)\[.*\]").map_err(e)?;
    let m = re.find(&content);

    // Helper: attempt to parse, and if it fails, try to fix truncated JSON
    fn parse_with_repair(json_str: &str) -> Result<Value, String> {
        // Try direct parse first
        if let Ok(v) = serde_json::from_str::<Value>(json_str) {
            return Ok(v);
        }
        // If parse fails, try to repair truncated JSON by balancing brackets
        let trimmed = json_str.trim_end();
        let mut repaired = trimmed.to_string();
        // Track bracket depth
        let mut depth: i64 = 0;
        for ch in repaired.chars() {
            match ch {
                '{' | '[' => depth += 1,
                '}' | ']' => depth -= 1,
                _ => {}
            }
        }
        // Close any unclosed brackets/braces
        for _ in 0..depth {
            repaired.push('}');
        }
        // Also close array if needed
        let open_arrays = repaired.matches('[').count() as i64;
        let close_arrays = repaired.matches(']').count() as i64;
        for _ in 0..(open_arrays - close_arrays) {
            repaired.push(']');
        }
        // Remove trailing commas before closing brackets
        let re_trailing = Regex::new(r",(\s*[}\]])").map_err(|e| e.to_string())?;
        let cleaned = re_trailing.replace_all(&repaired, |caps: &regex::Captures| format!("{}", &caps[1]));
        if let Some(last_comma) = cleaned.rfind(',') {
            let after_comma = &cleaned[last_comma + 1..].trim();
            if *after_comma == "]" || *after_comma == "}" || after_comma.starts_with(']') || after_comma.starts_with('}') {
                let fixed = format!("{}{}", &cleaned[..last_comma], &cleaned[last_comma + 1..]);
                if let Ok(v) = serde_json::from_str::<Value>(&fixed) {
                    return Ok(v);
                }
            }
        }
        // Try the repaired version
        serde_json::from_str::<Value>(&repaired).map_err(|e| e.to_string())
    }

    match m {
        Some(mat) => {
            let json_str = mat.as_str();
            match parse_with_repair(json_str) {
                Ok(Value::Array(mut items)) => {
                    attach_channel_info(&mut items, &payload.targets);
                    return Ok(serde_json::json!({
                        "items": items,
                        "model": model,
                        "count": items.len()
                    }));
                }
                Ok(_) => {
                    return Ok(serde_json::json!({
                        "error": "AI returned non-array JSON",
                        "raw": raw_preview(json_str),
                        "items": []
                    }));
                }
                Err(err) => {
                    return Ok(serde_json::json!({
                        "error": format!("JSON parse failed: {err}"),
                        "raw": raw_preview(json_str),
                        "items": []
                    }));
                }
            }
        }
        None => {
            return Ok(serde_json::json!({
                "error": "No JSON array found in AI response",
                "raw": raw_preview(&content),
                "items": []
            }));
        }
    }
}

fn is_punchy_transition(t: &str) -> bool {
    matches!(t,
        "slideleft" | "slideright" | "slideup" | "slidedown" |
        "wipeleft" | "wiperight" | "wipeup" | "wipedown" |
        "pixelize" | "zoomin" | "circleopen" | "circleclose" | "circlecrop" | "rectcrop" | "radial" |
        "diagtl" | "diagtr" | "diagbl" | "diagbr" |
        "hlslice" | "hrslice" | "vuslice" | "vdslice" | "squeezev" | "squeezeh" | "distance")
}

/// Rule-based transition pick for one scene boundary, drawn from the pack's allowed vocabulary:
/// a mood change gets a more pronounced transition; a steady mood gets a gentle one. `idx` rotates
/// choices so consecutive boundaries vary.
fn deterministic_transition(allowed: &[String], default_t: &str, mood: &str, mood_prev: &str, idx: usize) -> String {
    let changed = !mood.is_empty() && !mood_prev.is_empty() && mood != mood_prev;
    let pool: Vec<&String> = allowed.iter()
        .filter(|t| if changed { is_punchy_transition(t) } else { !is_punchy_transition(t) })
        .collect();
    if pool.is_empty() {
        if allowed.is_empty() { default_t.to_string() } else { allowed[idx % allowed.len()].clone() }
    } else {
        pool[idx % pool.len()].clone()
    }
}

/// Suggest a transition for every scene boundary of a song and persist it to the sections.
/// `automation` chooses how much the AI drives it: manual → the pack default everywhere; assist →
/// mood/pacing rules; full → the LLM picks per scene (with the rules as fallback). Considers the
/// per-section mood transitions and durations produced by the audio-analysis step.
#[tauri::command]
pub async fn suggest_transitions(state: State<'_, AppState>, payload: SuggestTransitionsRequest) -> Res<Value> {
    use futures_util::StreamExt;
    let default_t = if payload.default_transition.trim().is_empty() { "fade".to_string() } else { payload.default_transition.trim().to_string() };
    let allowed: Vec<String> = if payload.allowed.is_empty() {
        vec!["fade".into(), "dissolve".into(), "slideleft".into(), "wipeup".into(), "circleopen".into()]
    } else { payload.allowed.clone() };

    // Load sections in order.
    let mut cursor = state.db.collection::<Document>("sections")
        .find(doc! { "song_id": &payload.song_id }).await.map_err(e)?;
    let mut secs: Vec<Value> = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { secs.push(bson_to_value(d)); }
    secs.sort_by_key(|s| s["index"].as_i64().unwrap_or(0));
    if secs.is_empty() {
        return Ok(serde_json::json!({ "error": "No sections found — analyze the song first.", "transitions": [] }));
    }

    // Build the base (deterministic / manual) suggestions.
    let mut chosen: Vec<String> = secs.iter().enumerate().map(|(i, s)| {
        if payload.automation == "manual" {
            default_t.clone()
        } else {
            deterministic_transition(&allowed, &default_t,
                s["mood"].as_str().unwrap_or(""), s["mood_prev"].as_str().unwrap_or(""), i)
        }
    }).collect();
    let mut model = if payload.automation == "manual" { "manual".to_string() } else { "rules".to_string() };

    // "full" automation: ask the LLM to pick per scene from the allowed vocabulary.
    if payload.automation == "full" {
        let settings_doc = state.db.collection::<Document>("settings")
            .find_one(doc! { "_id": "singleton" }).with_options(proj0()).await.map_err(e)?
            .map(bson_to_value).unwrap_or_default();
        let scene_lines: Vec<String> = secs.iter().enumerate().map(|(i, s)| {
            let dur = (s["end"].as_f64().unwrap_or(0.0) - s["start"].as_f64().unwrap_or(0.0)).max(0.0);
            format!("{}. mood '{}' (prev '{}'), {:.1}s: {}", i, s["mood"].as_str().unwrap_or("?"),
                s["mood_prev"].as_str().unwrap_or(""), dur,
                s["image_prompt"].as_str().or_else(|| s["line"].as_str()).unwrap_or(""))
        }).collect();
        let sys = "You are a music-video editor choosing scene-to-scene transitions. For each scene, \
                   pick ONE transition from the allowed list that fits its mood change and pacing \
                   (punchy transitions for big mood shifts / energetic moments, gentle ones for calm \
                   or continuous scenes). Return ONLY a JSON array of transition names, one per scene, \
                   same length and order as the scenes, each value from the allowed list.";
        let user = format!("Allowed transitions: {}\n\nScenes (index. mood, duration: description):\n{}\n\nReturn the JSON array now.",
            allowed.join(", "), scene_lines.join("\n"));
        if let Ok((text, m)) = provider_chat(&settings_doc, sys, &user, 0.5, false).await {
            if let Some(Value::Array(arr)) = extract_json_value(&text) {
                for (i, v) in arr.iter().enumerate() {
                    if i >= chosen.len() { break; }
                    if let Some(t) = v.as_str() {
                        if allowed.iter().any(|a| a == t) { chosen[i] = t.to_string(); }
                    }
                }
                model = m;
            }
        }
    }

    // Persist to the sections.
    let coll = state.db.collection::<Document>("sections");
    let mut out: Vec<Value> = Vec::new();
    for (i, s) in secs.iter().enumerate() {
        if let Some(id) = s["id"].as_str() {
            let tr = &chosen[i];
            let _ = coll.update_one(doc! { "id": id }, doc! { "$set": { "transition": tr } }).await;
            out.push(serde_json::json!({
                "id": id, "index": s["index"], "mood": s["mood"], "transition": tr,
                "line": s["image_prompt"].as_str().or_else(|| s["line"].as_str()).unwrap_or("")
            }));
        }
    }
    Ok(serde_json::json!({ "transitions": out, "model": model, "count": out.len() }))
}

/// Deterministically fuse several genre descriptors into one coherent style string: merge all
/// comma-separated keywords (dedup, case-insensitive, order-preserving) and reconcile BPMs into a
/// single averaged tempo. Fast, offline, and used as the fallback when the LLM isn't available.
fn deterministic_genre_mix(genres: &[String], mood: &str) -> String {
    let bpm_re = Regex::new(r"(?i)(\d{2,3})\s*bpm").ok();
    let mut bpms: Vec<f64> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut tokens: Vec<String> = Vec::new();

    if !mood.trim().is_empty() {
        for part in mood.split(',') {
            let p = part.trim();
            if !p.is_empty() && seen.insert(p.to_lowercase()) { tokens.push(p.to_string()); }
        }
    }
    for g in genres {
        for part in g.split(',') {
            let p = part.trim();
            if p.is_empty() { continue; }
            if let Some(re) = &bpm_re {
                if let Some(c) = re.captures(p) {
                    if let Some(n) = c.get(1).and_then(|m| m.as_str().parse::<f64>().ok()) { bpms.push(n); }
                    continue; // drop individual bpm tokens; we add one averaged bpm at the end
                }
            }
            if seen.insert(p.to_lowercase()) { tokens.push(p.to_string()); }
        }
    }
    // Keep the descriptor a reasonable length for the music models.
    tokens.truncate(40);
    if !bpms.is_empty() {
        let avg = (bpms.iter().sum::<f64>() / bpms.len() as f64).round() as i64;
        tokens.push(format!("{} bpm", avg));
    }
    tokens.join(", ")
}

#[tauri::command]
pub async fn mix_genres(state: State<'_, AppState>, payload: MixGenresRequest) -> Res<Value> {
    let cleaned: Vec<String> = payload.genres.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    if cleaned.is_empty() {
        return Ok(serde_json::json!({ "error": "Select at least one genre to mix.", "styles": "" }));
    }
    let deterministic = deterministic_genre_mix(&cleaned, &payload.mood);

    if !payload.ai {
        return Ok(serde_json::json!({ "styles": deterministic, "model": "deterministic" }));
    }

    let settings_doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" })
        .with_options(proj0())
        .await.map_err(e)?
        .map(bson_to_value)
        .unwrap_or_default();

    let sys = "You are a music A&R that fuses genres into ONE coherent, producible style descriptor for an \
               AI song generator (Suno / ACE-Step). Blend the given genres into a single cohesive style that \
               actually works together — reconcile tempo into one BPM, resolve conflicting instrumentation \
               tastefully, and keep the best defining traits of each. Output ONLY a single line of \
               comma-separated style keywords (no prose, no lists, no quotes), ending with one '<n> bpm'.";
    let flavor = payload.flavor_layer.as_deref().map(|l| l.trim().to_ascii_lowercase()).unwrap_or_default();
    let flavor_block = flavor_layer_brief(&flavor)
        .map(|b| format!("\n\nTOP LAYER (apply OVER the fused base, coloring not replacing it): {b}"))
        .unwrap_or_default();

    let user = format!(
        "Genres to fuse:\n- {}\n\nMood/vibe to honor: {}{}\n\nReturn the single fused style line now.",
        cleaned.join("\n- "),
        if payload.mood.trim().is_empty() { "(none specified)" } else { payload.mood.trim() },
        flavor_block,
    );

    match provider_chat(&settings_doc, sys, &user, 0.7, false).await {
        Ok((text, model)) => {
            let one_line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or(&text).trim().trim_matches('"').to_string();
            let styles = if one_line.is_empty() { deterministic.clone() } else { one_line };
            Ok(serde_json::json!({ "styles": styles, "model": model, "fallback": deterministic }))
        }
        // If the LLM isn't configured/fails, quietly fall back to the deterministic mix.
        Err(err) => Ok(serde_json::json!({ "styles": deterministic, "model": "deterministic", "note": err })),
    }
}

/// Freeform (non-Bible) topic composer. Turns a rough topic + a daily mood/vibe driver into a set
/// of per-target songs, each culturally + genre adapted so it speaks to its regional/genre YouTube
/// audience. Returns items in the same importable shape as `compose_lyrics` (so `import_lyrics`
/// works unchanged) plus per-item intel fields for display: `cultural_influence`, `mood_context`,
/// `section_intel`, and `image_intel`. The cultural adaptation is also baked into `styles` /
/// `image_styles` / `annotations` so it actually steers the downstream music + image generation.
#[tauri::command]
pub async fn compose_freeform(state: State<'_, AppState>, payload: FreeformComposeRequest) -> Res<Value> {
    let settings_doc = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" })
        .with_options(proj0())
        .await.map_err(e)?
        .map(bson_to_value)
        .unwrap_or_default();

    if payload.topic.trim().is_empty() {
        return Ok(serde_json::json!({ "error": "Describe a topic first (type it or use the mic).", "items": [] }));
    }

    let sys = "You compose multilingual, genre- and culture-adapted song lyrics JSON for AI music \
               video production (NON-Bible, general topics).\n\
               Return ONLY a valid JSON array — no prose, no markdown fences.\n\
               Produce one element per requested target. Each element MUST have these fields:\n\
               - title (string)\n\
               - language (string)\n\
               - styles (string): music-genre + production style keywords; when cultural influence is on, \
                 fold in the regional/genre cultural flavor so the generated MUSIC speaks to that audience.\n\
               - lyrics (string): the full lyrics as plain text with [section] headers (e.g. [verse 1], [chorus]) \
                 on their own lines. Translate faithfully into the target language when it isn't English.\n\
               - annotations (string): alternating lines — first a bracketed Midjourney-style image prompt in \
                 square brackets, then the matching lyric line — so each section has visual image intel.\n\
               - image_styles (string): global image-style keywords for this target, culturally adapted.\n\
               - cultural_influence (string): a SHORT (1–2 sentence) summary of how this target was culturally/\
                 genre adapted, to display in the music-generation view.\n\
               - mood_context (string): 1 sentence on how today's mood/vibe driver shaped the tone.\n\
               - section_intel (array of {section, note, image_intel}): per-section intel — what the section is \
                 about and what its imagery should convey.\n\
               - image_intel (string): a broad, general description of the overall visual direction (project + \
                 daily theme) for this target.\n\
               Keep it directly usable. Output the JSON array now.";

    let targets_str = if payload.targets.is_empty() {
        "[{\"language\":\"English\",\"genre\":\"cinematic pop\"}]".to_string()
    } else {
        serde_json::to_string(&payload.targets).unwrap_or_default()
    };
    let sections_str = serde_json::to_string(&payload.sections).unwrap_or_else(|_| "[]".to_string());

    let user_prompt = format!(
        "Topic (rough, from the user):\n{}\n\n\
         Daily mood / vibe driver (shade of humor / wellbeing / state of mood — set the tone accordingly):\n{}\n\n\
         User-authored section ideas (apply to sections + bracket image prompts when matching):\n{}\n\n\
         Targets to produce (one JSON element per target; each has a language and a music genre, and \
         ultimately becomes its own YouTube channel):\n{}\n\n\
         Cultural influence: {}. When ON, adapt each translation AND genre to its regional + genre culture so \
         it speaks directly to that audience (idioms, references, rhythmic/production conventions), and reflect \
         that adaptation in styles, image_styles, annotations, and the cultural_influence summary.\n\
         Title pattern: {} (artist={}).\n\
         image_styles should end with: {}.\n\
         Output the JSON array now.",
        payload.topic.trim(),
        if payload.mood_driver.trim().is_empty() { "(none specified — pick a fitting contemporary tone)" } else { payload.mood_driver.trim() },
        sections_str,
        targets_str,
        if payload.cultural_influence { "ON" } else { "OFF" },
        payload.title_pattern,
        payload.artist,
        payload.image_style_suffix,
    );

    let (content, model) = match provider_chat(&settings_doc, sys, &user_prompt, 0.9, false).await {
        Ok(pair) => pair,
        Err(err) => return Ok(serde_json::json!({ "error": err, "items": [] })),
    };

    // Extract the JSON array from the response (models sometimes wrap it in prose/fences).
    let re = Regex::new(r"(?s)\[.*\]").map_err(e)?;
    match re.find(&content).map(|m| m.as_str().to_string()).or_else(|| Some(content.clone())) {
        Some(json_str) => match serde_json::from_str::<Value>(&json_str) {
            Ok(Value::Array(mut items)) => {
                attach_channel_info(&mut items, &payload.targets);
                Ok(serde_json::json!({
                    "items": items,
                    "model": model,
                    "count": items.len(),
                }))
            }
            _ => match extract_json_value(&content) {
                Some(Value::Array(mut items)) => {
                    attach_channel_info(&mut items, &payload.targets);
                    Ok(serde_json::json!({ "items": items, "model": model, "count": items.len() }))
                }
                _ => Ok(serde_json::json!({ "error": "AI did not return a JSON array", "raw": raw_preview(&content), "items": [] })),
            },
        },
        None => Ok(serde_json::json!({ "error": "Empty AI response", "items": [] })),
    }
}

// ────────────────────────────────────────────────────────────────
// UI translation
// ────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct TranslateUiRequest {
    /// Distinct UI strings to translate (labels, buttons, hints).
    pub strings: Vec<String>,
    /// Target language name, e.g. "Spanish", "Deutsch".
    pub language: String,
}

/// Translate a batch of interface strings, returning a `{ original: translated }` map.
///
/// Drives the runtime GUI translation: the frontend collects the visible interface text, sends the
/// distinct strings here, and swaps them in place. Batches are cached client-side, so this is
/// called once per language per unseen batch rather than on every render.
///
/// Returns the ORIGINAL string for anything the model omits or mangles, so a partial response
/// degrades to untranslated text instead of blanking the interface.
#[tauri::command]
pub async fn ai_translate_ui(state: State<'_, AppState>, payload: TranslateUiRequest) -> Res<Value> {
    let lang = payload.language.trim();
    if lang.is_empty() { return Err("No target language given.".into()); }
    let strings: Vec<String> = payload.strings.into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if strings.is_empty() {
        return Ok(serde_json::json!({ "translations": {} }));
    }

    let system = format!(
        "You localise software interface text into {lang}.\n\
         You receive a JSON array of UI strings. Return ONLY a JSON object mapping EACH original \
         string EXACTLY as given to its {lang} translation — no extra keys, no commentary, no fences.\n\
         Rules: keep it short enough for buttons and labels; preserve capitalisation style, \
         surrounding punctuation and any leading/trailing spaces; do NOT translate proper nouns, \
         brand or product names (Kaggle, YouTube, Suno, ComfyUI, HeartMuLa, ACE-Step, FLUX, \
         Midjourney, OpenRouter, Gemini, Google, GPU, API, JSON, OAuth, URL), file names, code, \
         numbers or units. If a string should stay as-is, map it to itself."
    );
    let user = serde_json::to_string(&strings).unwrap_or_else(|_| "[]".into());

    let (content, model) = provider_text(&state, &system, &user, 0.1, true).await?;
    let parsed = extract_json_value(&content).unwrap_or(Value::Null);

    // Rebuild the map defensively: every requested string gets an entry, falling back to itself.
    let mut out = serde_json::Map::new();
    for s in &strings {
        let t = parsed.get(s).and_then(|v| v.as_str()).unwrap_or(s.as_str());
        out.insert(s.clone(), Value::String(t.to_string()));
    }
    Ok(serde_json::json!({ "translations": Value::Object(out), "model": model }))
}

// ────────────────────────────────────────────────────────────────
// Project brief + per-channel background research
// ────────────────────────────────────────────────────────────────

/// Render a project's Brief (its creative DNA from the Dashboard) into a compact prompt block.
/// Returns an empty string when there's no project or no brief — callers just append it.
pub async fn project_brief_block(db: &crate::store::Db, project_id: &str) -> String {
    if project_id.trim().is_empty() { return String::new(); }
    let Some(doc) = db.collection::<Document>("projects")
        .find_one(doc! { "id": project_id }).await.ok().flatten() else { return String::new(); };
    let proj = bson_to_value(doc);
    let mut lines: Vec<String> = Vec::new();
    if let Some(b) = proj["brief"].as_object() {
        for (label, key) in [
            ("Mood", "mood"), ("Attitude", "attitude"), ("Motivation", "motivation"),
            ("Aims & goals", "goals"), ("Audience", "audience"), ("Storyline", "storyline"),
            ("Humor", "humor"), ("Personality", "personality"),
        ] {
            if let Some(v) = b.get(key).and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()) {
                lines.push(format!("{}: {}", label, v.trim()));
            }
        }
    }
    // The daily topic only steers TODAY's generations — a stale one from a previous day is noise.
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    if proj["daily_topic_date"].as_str() == Some(today.as_str()) {
        if let Some(t) = proj["daily_topic"].as_str().filter(|s| !s.trim().is_empty()) {
            lines.push(format!("Today's topic/angle: {}", t.trim()));
        }
    }
    if lines.is_empty() { return String::new(); }
    format!("PROJECT BRIEF (the project's voice — let it shape tone, imagery, style and word choice):\n{}\n", lines.join("\n"))
}

/// Cached research notes per channel, rendered as one prompt block (empty when none).
pub async fn channel_research_block(db: &crate::store::Db) -> String {
    use futures_util::StreamExt;
    let Ok(mut cursor) = db.collection::<Document>("channels").find(doc! {}).await else { return String::new(); };
    let mut lines: Vec<String> = Vec::new();
    while let Some(Ok(d)) = cursor.next().await {
        let ch = bson_to_value(d);
        if let Some(r) = ch["research"].as_str().filter(|s| !s.trim().is_empty()) {
            let name = ch["name"].as_str().unwrap_or("channel");
            let lang = ch["language"].as_str().unwrap_or("");
            lines.push(format!("- {} ({}): {}", name, lang, r.trim()));
        }
    }
    if lines.is_empty() { return String::new(); }
    format!("CHANNEL RESEARCH (per-channel regional/cultural/linguistic notes — apply to the matching target):\n{}\n", lines.join("\n"))
}

/// Run the background research pass: for each channel, have the AI produce compact notes on the
/// regional, cultural and linguistic reality of that channel's audience — measured against the
/// project's brief — and cache them on the channel doc. Called from the Dashboard's "Research
/// channels now" button and automatically after a materially changed brief is saved.
#[tauri::command]
pub async fn research_project_channels(state: State<'_, AppState>, project_id: String) -> Res<Value> {
    use futures_util::StreamExt;
    let brief = project_brief_block(&state.db, &project_id).await;

    let mut channels: Vec<Value> = Vec::new();
    let mut cursor = state.db.collection::<Document>("channels").find(doc! {}).await.map_err(e)?;
    while let Some(Ok(d)) = cursor.next().await { channels.push(bson_to_value(d)); }
    if channels.is_empty() {
        return Ok(serde_json::json!({ "researched": 0, "detail": "No channels exist yet — create channels first." }));
    }

    let system =
        "You are a cultural research assistant for a multi-channel music-video studio. When web \
         search is available, USE IT to check what music and content are currently resonating in \
         this language/region. Produce a COMPACT set of notes (max 110 words, plain text, no lists) \
         covering: what resonates musically and lyrically with this audience right now, cultural and \
         faith-context sensitivities to respect, the linguistic register to use, and one concrete \
         stylistic lean that would make content feel native rather than translated. Ground it in \
         the project brief when one is given.";

    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).with_options(proj0())
        .await.map_err(e)?.map(bson_to_value).unwrap_or_default();

    let mut researched = 0usize;
    let mut grounded_any = false;
    for ch in channels.iter().take(12) {
        let (Some(id), Some(name)) = (ch["id"].as_str(), ch["name"].as_str()) else { continue; };
        let user = format!(
            "{brief}\nCHANNEL: {name}\nLanguage: {}\nRegion: {}\nDeclared style: {}\n\nWrite the notes now.",
            ch["language"].as_str().unwrap_or("English"),
            ch["region"].as_str().unwrap_or("unspecified"),
            ch["styles"].as_str().unwrap_or("unspecified"),
        );
        // Grounded when the provider is Gemini (live web search); ungrounded fallback otherwise.
        match provider_chat_grounded(&settings, system, &user, 0.4).await {
            Ok((notes, _model, sources)) => {
                let clean: String = notes.trim().chars().take(1000).collect();
                if !clean.is_empty() {
                    if !sources.is_empty() { grounded_any = true; }
                    let src_bson = bson::to_bson(&sources).unwrap_or(bson::Bson::Array(vec![]));
                    let _ = state.db.collection::<Document>("channels").update_one(
                        doc! { "id": id },
                        doc! { "$set": { "research": &clean, "research_at": chrono::Utc::now().to_rfc3339(),
                                         "research_sources": src_bson } },
                    ).await;
                    researched += 1;
                }
            }
            Err(err) => {
                if researched == 0 { return Err(format!("Research needs the AI provider: {err}")); }
            }
        }
    }
    Ok(serde_json::json!({ "researched": researched, "total": channels.len(), "grounded": grounded_any }))
}

#[cfg(test)]
mod provider_tests {
    use super::*;

    #[test]
    fn openrouter_models_are_normalised_with_free_first() {
        // Shape verified against the live endpoint: { data: [{ id, name, … }] }.
        let res = serde_json::json!({ "data": [
            { "id": "anthropic/claude-opus-5", "name": "Claude Opus 5" },
            { "id": "nvidia/nemotron-3-ultra-550b-a55b:free", "name": "Nemotron 3 Ultra 550B (free)" },
        ]});
        let models = normalise_models("openrouter", &res);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0]["id"], "nvidia/nemotron-3-ultra-550b-a55b:free", "free models sort first");
        assert_eq!(models[0]["free"], true);
        assert_eq!(models[1]["free"], false);
    }

    #[test]
    fn anthropic_models_use_their_display_name() {
        let res = serde_json::json!({ "data": [
            { "id": "claude-opus-5", "display_name": "Claude Opus 5" },
            { "id": "claude-haiku-4-5", "display_name": "Claude Haiku 4.5" },
        ]});
        let models = normalise_models("anthropic", &res);
        assert_eq!(models[0]["id"], "claude-haiku-4-5");   // alphabetical, none free
        assert_eq!(models[0]["label"], "Claude Haiku 4.5");
        assert!(models.iter().all(|m| m["free"] == false));
    }

    #[test]
    fn openai_non_chat_models_are_filtered_out() {
        // The list mixes in embeddings, speech and image endpoints — offering one would 400 on the
        // first prompt with an error the user cannot act on.
        let res = serde_json::json!({ "data": [
            { "id": "gpt-5.1" },
            { "id": "text-embedding-3-large" },
            { "id": "gpt-4o-realtime-preview" },
            { "id": "dall-e-3" },
            { "id": "whisper-1" },
            { "id": "o3-mini" },
        ]});
        let ids: Vec<String> = normalise_models("openai", &res).iter()
            .map(|m| m["id"].as_str().unwrap_or("").to_string()).collect();
        assert_eq!(ids, vec!["gpt-5.1".to_string(), "o3-mini".to_string()]);
    }

    #[test]
    fn gemini_keeps_only_models_that_generate_content() {
        let res = serde_json::json!({ "models": [
            { "name": "models/gemini-3.5-flash", "displayName": "Gemini 3.5 Flash",
              "supportedGenerationMethods": ["generateContent"] },
            { "name": "models/gemini-2.5-flash-preview-tts", "displayName": "TTS",
              "supportedGenerationMethods": ["generateContent"] },
            { "name": "models/text-embedding-004", "displayName": "Embeddings",
              "supportedGenerationMethods": ["embedContent"] },
        ]});
        let models = normalise_models("gemini", &res);
        assert_eq!(models.len(), 1, "tts and embedding models are dropped");
        assert_eq!(models[0]["id"], "gemini-3.5-flash", "the models/ prefix is stripped");
    }

    #[test]
    fn a_shape_we_do_not_recognise_yields_an_empty_list_not_a_panic() {
        assert!(normalise_models("openai", &serde_json::json!({ "unexpected": true })).is_empty());
        assert!(normalise_models("gemini", &Value::Null).is_empty());
    }

    #[test]
    fn overload_detection_covers_the_failures_that_actually_happen() {
        // Regression net for the fallback trigger: these are real error strings seen from the
        // providers; the 4xx ones must NOT trigger a retry that would fail identically.
        for msg in ["OpenRouter HTTP 429: rate limit", "Gemini HTTP 503: model overloaded",
                    "Anthropic HTTP 529: overloaded_error", "operation timed out",
                    "OpenAI HTTP 500: server error"] {
            assert!(is_overload_error(msg), "{msg} should fall back");
        }
        for msg in ["Gemini HTTP 400: invalid argument", "OpenAI HTTP 401: invalid api key",
                    "Anthropic HTTP 403: permission denied"] {
            assert!(!is_overload_error(msg), "{msg} must not fall back");
        }
    }
}
