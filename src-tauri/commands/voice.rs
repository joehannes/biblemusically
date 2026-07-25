use crate::state::AppState;
use bson::{doc, Document};
use serde_json::{json, Value};
use tauri::State;

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
// The assistant's voice
//
// The guided layer talks the user through a production run; hearing it is what makes it feel like
// being guided rather than filling a form. Speech both ways:
//
//   out  Gemini's TTS models return raw 24 kHz mono PCM, which browsers will not play, so a WAV
//        header is prepended here and the result handed over as a data URL. Every phrase is cached on
//        disk by (text, voice) — a guide repeats itself constantly, and the free tier counts requests.
//   in   Gemini transcribes a recorded clip. The webview's SpeechRecognition is tried first by the
//        frontend; this is the fallback for WebKitGTK, where it does not exist.
//
// The whole feature degrades quietly: no key, no network, or voice turned off simply means the guide
// stays silent and typed answers keep working.
// ────────────────────────────────────────────────────────────────

/// A deliberately small, friendly set out of Gemini's ~30 prebuilt voices, described the way a user
/// would choose ("warm", "bright"), not the way a spec sheet would.
const VOICES: &[(&str, &str, &str)] = &[
    ("Kore",    "Warm and steady",     "female"),
    ("Aoede",   "Light and breezy",    "female"),
    ("Leda",    "Young and friendly",  "female"),
    ("Zephyr",  "Bright and clear",    "female"),
    ("Callirrhoe", "Calm and gentle",  "female"),
    ("Puck",    "Upbeat and playful",  "male"),
    ("Charon",  "Calm and explanatory","male"),
    ("Orus",    "Firm and grounded",   "male"),
    ("Fenrir",  "Energetic",           "male"),
    ("Enceladus", "Soft and breathy",  "male"),
];

const TTS_MODELS: &[&str] = &["gemini-2.5-flash-preview-tts", "gemini-3.1-flash-tts-preview"];

#[tauri::command]
pub async fn list_assistant_voices() -> Res<Value> {
    Ok(json!({
        "voices": VOICES.iter().map(|(name, desc, gender)| json!({
            "id": name, "label": name, "description": desc, "gender": gender,
        })).collect::<Vec<_>>(),
        "engines": [
            { "id": "gemini", "label": "Gemini voices", "detail": "Natural voices via your Gemini key. Cached, so repeated lines cost nothing." },
            { "id": "browser", "label": "System voice", "detail": "Whatever your OS provides. No network, but quality varies." },
            { "id": "off", "label": "Silent", "detail": "The guide stays text-only." },
        ],
    }))
}

/// Wrap raw PCM in a WAV container so a browser `Audio` element will play it.
fn pcm_to_wav(pcm: &[u8], sample_rate: u32, channels: u16) -> Vec<u8> {
    let bits = 16u16;
    let byte_rate = sample_rate * channels as u32 * (bits / 8) as u32;
    let block_align = channels * (bits / 8);
    let data_len = pcm.len() as u32;
    let mut out = Vec::with_capacity(44 + pcm.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());      // PCM fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes());       // format = PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(pcm);
    out
}

/// Pull the sample rate out of a mime like `audio/L16;codec=pcm;rate=24000`, defaulting to 24 kHz.
fn rate_from_mime(mime: &str) -> u32 {
    mime.split(';')
        .find_map(|p| p.trim().strip_prefix("rate=").and_then(|r| r.trim().parse().ok()))
        .unwrap_or(24_000)
}

fn cache_dir() -> Option<std::path::PathBuf> {
    let dir = dirs::config_dir()?.join("studio-lightkid").join("tts-cache");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Cache key for one spoken phrase. Short hash — collisions here would only mean a wrong cached line.
fn cache_key(text: &str, voice: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    voice.hash(&mut h);
    format!("{:016x}", h.finish())
}

#[derive(serde::Deserialize)]
pub struct SpeakRequest {
    pub text: String,
    #[serde(default)]
    pub voice: Option<String>,
    /// Optional style direction, e.g. "warmly, unhurried" — Gemini TTS follows plain instructions.
    #[serde(default)]
    pub style: Option<String>,
}

/// Speak one line. Returns a base64 WAV the frontend plays, or an error the caller can ignore.
#[tauri::command]
pub async fn tts_speak(state: State<'_, AppState>, payload: SpeakRequest) -> Res<Value> {
    use base64::Engine as _;
    let text = payload.text.trim();
    if text.is_empty() { return Err("nothing to say".into()); }
    // A guide line is a sentence or two; a whole lyric sheet is not something to read aloud.
    let text: String = text.chars().take(600).collect();

    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let voice = payload.voice
        .filter(|v| !v.trim().is_empty())
        .or_else(|| settings["assistant_voice"].as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "Kore".to_string());

    // Cache first: the same question is spoken every time a user walks a flow.
    let key = cache_key(&text, &voice);
    let cached = cache_dir().map(|d| d.join(format!("{key}.wav")));
    if let Some(path) = cached.as_ref().filter(|p| p.exists()) {
        if let Ok(bytes) = std::fs::read(path) {
            return Ok(json!({
                "audio": base64::engine::general_purpose::STANDARD.encode(&bytes),
                "mime": "audio/wav", "voice": voice, "cached": true,
            }));
        }
    }

    let gkey = settings["gemini_api_key"].as_str().unwrap_or("").trim().to_string();
    if gkey.is_empty() {
        return Err("Speech needs a Gemini API key (Settings → AI). The guide stays text-only without one.".into());
    }

    let prompt = match payload.style.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(style) => format!("Say {style}: {text}"),
        None => format!("Say in a warm, friendly, unhurried voice: {text}"),
    };
    let body = json!({
        "contents": [ { "parts": [ { "text": prompt } ] } ],
        "generationConfig": {
            "responseModalities": ["AUDIO"],
            "speechConfig": { "voiceConfig": { "prebuiltVoiceConfig": { "voiceName": voice } } },
        },
    });

    let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(90)).build().map_err(e)?;
    let mut last_err = String::new();
    for model in TTS_MODELS {
        let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent");
        let r = match http.post(&url).header("x-goog-api-key", &gkey).json(&body).send().await {
            Ok(r) => r,
            Err(err) => { last_err = err.to_string(); continue; }
        };
        if !r.status().is_success() {
            let status = r.status();
            let txt = r.text().await.unwrap_or_default();
            last_err = format!("{model} HTTP {status}: {}", txt.chars().take(200).collect::<String>());
            continue;   // an overloaded or retired TTS model: try the next one
        }
        let res: Value = match r.json().await { Ok(v) => v, Err(err) => { last_err = err.to_string(); continue; } };
        let part = &res["candidates"][0]["content"]["parts"][0];
        let inline = if part["inlineData"].is_object() { &part["inlineData"] } else { &part["inline_data"] };
        let Some(data) = inline["data"].as_str() else {
            last_err = format!("{model} returned no audio");
            continue;
        };
        let mime = inline["mimeType"].as_str().or_else(|| inline["mime_type"].as_str()).unwrap_or("audio/L16;rate=24000");
        let pcm = base64::engine::general_purpose::STANDARD.decode(data).map_err(e)?;
        let wav = pcm_to_wav(&pcm, rate_from_mime(mime), 1);
        if let Some(path) = cached.as_ref() { let _ = std::fs::write(path, &wav); }
        return Ok(json!({
            "audio": base64::engine::general_purpose::STANDARD.encode(&wav),
            "mime": "audio/wav", "voice": voice, "model": model, "cached": false,
        }));
    }
    Err(format!("Speech is unavailable right now: {last_err}"))
}

#[derive(serde::Deserialize)]
pub struct TranscribeRequest {
    /// Base64 audio recorded in the webview (webm/opus, wav, ogg — whatever MediaRecorder produced).
    pub audio: String,
    #[serde(default)]
    pub mime: Option<String>,
    /// Language hint, e.g. "German" — improves accuracy for non-English answers.
    #[serde(default)]
    pub language: Option<String>,
}

/// Transcribe a spoken answer. Used when the webview has no SpeechRecognition of its own.
#[tauri::command]
pub async fn stt_transcribe(state: State<'_, AppState>, payload: TranscribeRequest) -> Res<Value> {
    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let gkey = settings["gemini_api_key"].as_str().unwrap_or("").trim().to_string();
    if gkey.is_empty() {
        return Err("Voice answers need a Gemini API key (Settings → AI).".into());
    }
    if payload.audio.trim().is_empty() { return Err("no audio recorded".into()); }

    let lang = payload.language.unwrap_or_default();
    let instruction = if lang.trim().is_empty() {
        "Transcribe this recording exactly. Return only the words spoken, no commentary.".to_string()
    } else {
        format!("Transcribe this recording exactly; it is spoken in {lang}. Return only the words spoken, no commentary.")
    };
    let body = json!({
        "contents": [ { "parts": [
            { "text": instruction },
            { "inline_data": { "mime_type": payload.mime.unwrap_or_else(|| "audio/webm".into()), "data": payload.audio } },
        ] } ],
        "generationConfig": { "temperature": 0.0 },
    });

    let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(90)).build().map_err(e)?;
    // A normal (non-TTS) model does the transcription; the app's own default is fine for it.
    let model = settings["gemini_model"].as_str().filter(|s| !s.trim().is_empty()).unwrap_or("gemini-3.5-flash");
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent");
    let r = http.post(&url).header("x-goog-api-key", &gkey).json(&body).send().await.map_err(e)?;
    if !r.status().is_success() {
        let status = r.status();
        let txt = r.text().await.unwrap_or_default();
        return Err(format!("Transcription failed (HTTP {status}): {}", txt.chars().take(200).collect::<String>()));
    }
    let res: Value = r.json().await.map_err(e)?;
    let text = res["candidates"][0]["content"]["parts"].as_array()
        .map(|parts| parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join(""))
        .unwrap_or_default().trim().to_string();
    if text.is_empty() { return Err("Nothing was recognised in that recording.".into()); }
    Ok(json!({ "text": text }))
}

#[derive(serde::Deserialize)]
pub struct InterpretRequest {
    /// What the user said.
    pub answer: String,
    /// The choices currently on screen: `[{ id, label, hint }]`.
    pub options: Vec<Value>,
    #[serde(default)]
    pub question: String,
}

/// Map a spoken sentence onto one of the offered choices.
///
/// Only reached when the frontend's own text matching was ambiguous — free tiers count requests, and
/// "the second one" or "let's do the quiet version" matches locally most of the time.
#[tauri::command]
pub async fn guide_interpret(state: State<'_, AppState>, payload: InterpretRequest) -> Res<Value> {
    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();

    let ids: Vec<String> = payload.options.iter()
        .filter_map(|o| o["id"].as_str().map(|s| s.to_string())).collect();
    if ids.is_empty() { return Ok(json!({ "option": Value::Null })); }

    let system = "You map a spoken answer onto one of a fixed list of options. \
                  Return ONLY {\"option\":\"<id>\",\"confidence\":0.0} using an id from the list, or \
                  {\"option\":null} when the answer clearly matches none of them. Never invent an id.";
    let user = format!(
        "QUESTION: {}\n\nOPTIONS:\n{}\n\nTHE USER SAID: {}",
        payload.question,
        serde_json::to_string_pretty(&payload.options).unwrap_or_else(|_| "[]".into()),
        payload.answer.chars().take(400).collect::<String>(),
    );
    let (content, _model) = crate::commands::ai::provider_chat(&settings, system, &user, 0.0, true).await?;
    let parsed = crate::commands::ai::extract_json_value(&content).unwrap_or(Value::Null);
    let picked = parsed["option"].as_str().filter(|id| ids.iter().any(|k| k == id));
    Ok(json!({
        "option": picked,
        "confidence": parsed["confidence"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_describes_the_pcm_it_wraps() {
        let pcm = vec![0u8; 480];        // 10 ms of 24 kHz mono s16
        let wav = pcm_to_wav(&pcm, 24_000, 1);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(wav.len(), 44 + pcm.len(), "44-byte header plus the samples");
        // Sample rate at offset 24, byte rate at 28 — a wrong value here plays at the wrong pitch.
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 24_000);
        assert_eq!(u32::from_le_bytes(wav[28..32].try_into().unwrap()), 48_000);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), pcm.len() as u32);
    }

    #[test]
    fn sample_rate_is_read_from_the_mime_google_sends() {
        assert_eq!(rate_from_mime("audio/L16;codec=pcm;rate=24000"), 24_000);
        assert_eq!(rate_from_mime("audio/l16; rate=16000; channels=1"), 16_000);
        assert_eq!(rate_from_mime("audio/wav"), 24_000, "unknown falls back to the documented rate");
    }

    #[test]
    fn cache_key_depends_on_both_text_and_voice() {
        assert_ne!(cache_key("hello", "Kore"), cache_key("hello", "Puck"));
        assert_ne!(cache_key("hello", "Kore"), cache_key("hello there", "Kore"));
        assert_eq!(cache_key("hello", "Kore"), cache_key("hello", "Kore"));
    }
}
