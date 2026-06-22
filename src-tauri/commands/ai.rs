use crate::state::AppState;
use bson::{doc, Document};
use mongodb::options::UpdateOptions;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;
use regex::Regex;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

fn proj0() -> mongodb::options::FindOneOptions {
    mongodb::options::FindOneOptions::builder().projection(doc! { "_id": 0 }).build()
}

fn bson_to_value(doc: Document) -> Value {
    let mut m = serde_json::Map::new();
    for (k, v) in doc {
        if k == "_id" { continue; }
        if let Ok(jv) = bson::from_bson::<Value>(v) { m.insert(k, jv); }
    }
    Value::Object(m)
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
pub async fn call_openrouter(
    db: &mongodb::Database,
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

    let key = settings_doc["openrouter_api_key"].as_str().unwrap_or("").trim().to_string();
    let model = settings_doc["openrouter_model"].as_str()
        .unwrap_or("qwen/qwen3-next-80b-a3b-instruct:free")
        .to_string();

    if key.is_empty() {
        return Err("Configure openrouter_api_key in Settings (get one free at openrouter.ai/keys).".to_string());
    }

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
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
    Ok(serde_json::json!({
        "text": content,
        "model": model,
    }))
}

fn raw_preview(content: &str) -> String {
    if content.len() > 1200 { content[..1200].to_string() } else { content.to_string() }
}

fn extract_json_value(content: &str) -> Option<Value> {
    let re = Regex::new(r"(?s)(\{.*\}|\[.*\])").ok()?;
    let mat = re.find(content)?;
    serde_json::from_str::<Value>(mat.as_str()).ok()
}

async fn openrouter_text(
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

    let key = settings_doc["openrouter_api_key"].as_str().unwrap_or("").trim().to_string();
    let model = settings_doc["openrouter_model"].as_str()
        .unwrap_or("qwen/qwen3-next-80b-a3b-instruct:free")
        .to_string();

    if key.is_empty() {
        return Err("Configure openrouter_api_key in Settings (get one free at openrouter.ai/keys).".to_string());
    }

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
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

    match openrouter_text(&state, system, &user, payload.temperature, payload.json_mode).await {
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
    
    let key = settings_doc["openrouter_api_key"].as_str().unwrap_or("").trim().to_string();
    let model = settings_doc["openrouter_model"].as_str()
        .unwrap_or("qwen/qwen3-next-80b-a3b-instruct:free")
        .to_string();
        
    if key.is_empty() {
        return Ok(serde_json::json!({
            "error": "Configure openrouter_api_key in Settings (get one free at openrouter.ai/keys).",
            "items": []
        }));
    }

    // 2. Build the system and user prompts exactly like the Python server did
    let sys = "You compose multilingual song lyrics JSON for AI music video production.\n\
               Return ONLY a valid JSON array — no prose, no markdown fences.\n\
               Each element MUST have: title (string), language (string), styles (string), lyrics (array of {section(string), lines(string)}), annotations (string), image_styles (string).\n\
               Example lyrics: [{section: verse1, lines: The Lord is my shepherd}].\n\
               annotations format: alternating lines — first a bracketed midjourney-style image prompt in square brackets, then the matching lyric line.\n\
               Translate lyrics into the target language faithfully if it isn't English. Keep section ideas + themes embedded in the bracket prompts.";

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

    let user_prompt = format!(
        "Source chapter text:\n{}\n\n\
         User-authored section ideas (apply to bracket prompts when matching lines):\n{}\n\n\
         Global theme: {}\n\
         Per-language themes: {}\n\
         Per-channel themes: {}\n\n\
         Targets to produce (one JSON element per target):\n{}\n\n\
         Title pattern: {} (artist={})\n\
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

    // 3. Make HTTP request to OpenRouter
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
        .map_err(e)?;

    // Get user email from settings for X-User-Email header (helps OpenRouter attribute usage)
    let user_email = settings_doc["openrouter_email"].as_str().unwrap_or("").trim().to_string();

    let mut req_builder = http.post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {key}"))
        .header("HTTP-Referer", "https://lightkid.studio")
        .header("X-Title", "Lightkid AI Studio");

    if !user_email.is_empty() {
        req_builder = req_builder.header("X-User-Email", &user_email);
    }

    let r = req_builder.json(&serde_json::json!({
            "model": model,
            "temperature": 0.85,
            "messages": [
                { "role": "system", "content": sys },
                { "role": "user", "content": user_prompt }
            ]
        }))
        .send()
        .await
        .map_err(e)?;

    if !r.status().is_success() {
        let status = r.status();
        let txt = r.text().await.unwrap_or_default();
        let safe_txt = if txt.len() > 400 { &txt[..400] } else { &txt };
        return Ok(serde_json::json!({
            "error": format!("OpenRouter HTTP {status}: {safe_txt}"),
            "items": []
        }));
    }

    let res_json: Value = r.json().await.map_err(e)?;
    let content = res_json["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string();

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
                Ok(Value::Array(items)) => {
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
