//! The paid image engines: fal.ai, Leonardo, Ideogram and Recraft.
//!
//! Four providers, one shape: post a prompt with an API key, get image URLs back. They differ in the
//! three places that always differ — how the key is presented, how the picture's shape is asked for,
//! and where the URLs sit in the answer — so those three are the only things described per provider.
//! Everything else is shared.
//!
//! # Why these four
//!
//! | Engine | What it is for | Roughly |
//! |---|---|---|
//! | **Leonardo** | A meta-platform: Flux, Ideogram, Recraft, Seedream and its own Phoenix behind one key. One subscription instead of six accounts. | ~$0.007/image |
//! | **fal.ai** | Pay-per-use, no subscription, the widest model list. | ~$0.03–0.06/image |
//! | **Ideogram** | Text *inside* images, by a distance. Verse cards and study cards. | ~$0.03–0.09 |
//! | **Recraft** | Logos and real SVG output — the only one here that returns vector. | $0.04 raster / $0.08 vector |
//!
//! # Endpoints are settings, not constants
//!
//! Every base URL and model id below is a **default**, overridable in Settings. Provider endpoints
//! move — this app has already been through `studio-api.suno.ai` → `studio-api.prod.suno.com` — and a
//! URL baked into a binary turns a five-second edit into a release. The defaults are what these
//! providers documented when this was written; if one moves, the app keeps working.
//!
//! # What is deliberately not here
//!
//! No cost estimation that pretends to be exact. `price_hint` is a per-image figure for the picker so
//! nobody discovers the cost from an invoice, and it says "about". Providers change prices; a number
//! presented as precise would be a lie with a decimal point on it.

use serde_json::{json, Value};

/// Everything the app needs to talk to one paid image API.
#[derive(Debug, Clone, PartialEq)]
pub struct Provider {
    pub id: &'static str,
    pub label: &'static str,
    /// Default base URL. Overridden by `<id>_base_url` in settings.
    pub base_url: &'static str,
    /// Default model, where the provider has one. Overridden by `<id>_model`.
    pub model: &'static str,
    /// How the key is presented. These genuinely differ and getting it wrong is a 401 that looks
    /// like a wrong key.
    pub auth: Auth,
    /// Roughly what one image costs, in US dollars, for the picker's badge.
    pub price_hint: f64,
    /// Does it need a second request to collect the result?
    pub polls: bool,
    pub vector_output: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Auth {
    /// `Authorization: Bearer <key>`
    Bearer,
    /// `Authorization: Key <key>` — fal.ai's own scheme.
    FalKey,
    /// `Api-Key: <key>` — Ideogram.
    ApiKeyHeader,
}

pub const PROVIDERS: &[Provider] = &[
    Provider {
        id: "leonardo",
        label: "Leonardo",
        base_url: "https://cloud.leonardo.ai/api/rest/v1",
        // Leonardo names models by uuid rather than by string, so this is a settings field people
        // will genuinely change — their model list is the point of the platform.
        model: "b24e16ff-06e3-43eb-8d33-4416c2d75876",
        auth: Auth::Bearer,
        price_hint: 0.007,
        // Leonardo starts a job and hands back an id; the images arrive on a second request.
        polls: true,
        vector_output: false,
    },
    Provider {
        id: "fal",
        label: "fal.ai",
        base_url: "https://fal.run",
        model: "fal-ai/flux-pro/v1.1",
        auth: Auth::FalKey,
        price_hint: 0.05,
        polls: false,
        vector_output: false,
    },
    Provider {
        id: "ideogram",
        label: "Ideogram",
        base_url: "https://api.ideogram.ai",
        model: "V_3",
        auth: Auth::ApiKeyHeader,
        price_hint: 0.06,
        polls: false,
        vector_output: false,
    },
    Provider {
        id: "recraft",
        label: "Recraft",
        base_url: "https://external.api.recraft.ai/v1",
        model: "recraftv3",
        auth: Auth::Bearer,
        price_hint: 0.04,
        polls: false,
        vector_output: true,
    },
];

pub fn provider(id: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|p| p.id == id)
}

/// Is this engine id one of the paid APIs?
pub fn is_paid_api(id: &str) -> bool { provider(id).is_some() }

fn setting<'a>(settings: &'a Value, key: &str) -> &'a str {
    settings.get(key).and_then(|v| v.as_str()).unwrap_or("").trim()
}

/// The configured base URL, or the documented default.
pub fn base_url(p: &Provider, settings: &Value) -> String {
    let configured = setting(settings, &format!("{}_base_url", p.id));
    let raw = if configured.is_empty() { p.base_url } else { configured };
    raw.trim_end_matches('/').to_string()
}

pub fn model_of(p: &Provider, settings: &Value) -> String {
    let configured = setting(settings, &format!("{}_model", p.id));
    if configured.is_empty() { p.model.to_string() } else { configured.to_string() }
}

pub fn api_key(p: &Provider, settings: &Value) -> String {
    setting(settings, &format!("{}_api_key", p.id)).to_string()
}

/// The header this provider wants the key in.
pub fn auth_header(p: &Provider, key: &str) -> (&'static str, String) {
    match p.auth {
        Auth::Bearer => ("Authorization", format!("Bearer {key}")),
        Auth::FalKey => ("Authorization", format!("Key {key}")),
        Auth::ApiKeyHeader => ("Api-Key", key.to_string()),
    }
}

/// Where the generation request goes.
pub fn generate_url(p: &Provider, settings: &Value) -> String {
    let base = base_url(p, settings);
    match p.id {
        // fal addresses the model as a path, so the "endpoint" is the model.
        "fal" => format!("{base}/{}", model_of(p, settings)),
        "leonardo" => format!("{base}/generations"),
        "ideogram" => format!("{base}/v1/ideogram-v3/generate"),
        _ => format!("{base}/images/generations"),
    }
}

/// A picture's shape, in the vocabulary each provider happens to use.
///
/// One aspect ratio in, four dialects out. The app talks in `"16:9"` everywhere else and nobody
/// should have to remember that fal says `landscape_16_9` while Ideogram says `16x9`.
pub fn shape_for(p: &Provider, aspect: &str, width: u32, height: u32) -> Value {
    match p.id {
        "fal" => json!({ "image_size": match aspect {
            "16:9" => "landscape_16_9",
            "9:16" => "portrait_16_9",
            "4:3" => "landscape_4_3",
            "3:4" | "2:3" => "portrait_4_3",
            _ => "square_hd",
        }}),
        "ideogram" => json!({ "aspect_ratio": match aspect {
            "16:9" => "16x9",
            "9:16" => "9x16",
            "4:3" => "4x3",
            "3:4" => "3x4",
            "2:3" => "2x3",
            _ => "1x1",
        }}),
        // Leonardo and Recraft take real pixels. Recraft wants them as one string.
        "recraft" => json!({ "size": format!("{width}x{height}") }),
        _ => json!({ "width": width, "height": height }),
    }
}

/// The body of the generation request.
pub fn request_body(
    p: &Provider,
    settings: &Value,
    prompt: &str,
    negative: Option<&str>,
    aspect: &str,
    width: u32,
    height: u32,
    count: u32,
) -> Value {
    let mut body = match p.id {
        "fal" => json!({ "prompt": prompt, "num_images": count }),
        "leonardo" => json!({
            "prompt": prompt,
            "modelId": model_of(p, settings),
            "num_images": count,
        }),
        "ideogram" => json!({
            "prompt": prompt,
            "rendering_speed": setting(settings, "ideogram_rendering_speed"),
            "num_images": count,
        }),
        _ => {
            let style = setting(settings, "recraft_style");
            json!({
                "prompt": prompt,
                "model": model_of(p, settings),
                "n": count,
                "style": if style.is_empty() { "realistic_image" } else { style },
            })
        }
    };

    if let Some(map) = body.as_object_mut() {
        if let Some(shape) = shape_for(p, aspect, width, height).as_object() {
            for (k, v) in shape { map.insert(k.clone(), v.clone()); }
        }
        // A negative prompt only travels to providers that have one. Sending it where it is ignored
        // is how somebody ends up believing "no text anywhere" was respected when it was dropped.
        if let Some(neg) = negative.map(str::trim).filter(|s| !s.is_empty()) {
            match p.id {
                "leonardo" => { map.insert("negative_prompt".into(), json!(neg)); }
                "ideogram" => { map.insert("negative_prompt".into(), json!(neg)); }
                _ => {}
            }
        }
        // An empty rendering_speed would be sent as "" and rejected; drop it instead.
        if map.get("rendering_speed").and_then(|v| v.as_str()) == Some("") {
            map.remove("rendering_speed");
        }
    }
    body
}

/// Does this provider act on a negative prompt at all?
///
/// The same question `engineCapabilities.js` answers for the interface, kept here too so the backend
/// never silently drops a constraint the user typed.
pub fn honours_negative(p: &Provider) -> bool {
    matches!(p.id, "leonardo" | "ideogram")
}

/// The image URLs in a finished response.
///
/// Every provider nests them somewhere different, and several return a list of objects rather than a
/// list of strings, so this walks the known shapes rather than guessing.
pub fn extract_images(p: &Provider, body: &Value) -> Vec<String> {
    let arrays: Vec<&Value> = match p.id {
        "leonardo" => vec![&body["generations_by_pk"]["generated_images"]],
        // fal returns `images`; Ideogram and Recraft both return `data`.
        "fal" => vec![&body["images"]],
        _ => vec![&body["data"]],
    };
    let mut out = Vec::new();
    for arr in arrays {
        let Some(items) = arr.as_array() else { continue };
        for item in items {
            let url = item
                .as_str()
                .or_else(|| item["url"].as_str())
                .or_else(|| item["image_url"].as_str())
                .unwrap_or("");
            if url.starts_with("http") {
                out.push(url.to_string());
            }
        }
    }
    out
}

/// For a provider that polls: the URL to ask again, given the response that started the job.
pub fn poll_url(p: &Provider, settings: &Value, start_response: &Value) -> Option<String> {
    if !p.polls { return None; }
    let id = start_response["sdGenerationJob"]["generationId"]
        .as_str()
        .or_else(|| start_response["id"].as_str())?;
    Some(format!("{}/generations/{id}", base_url(p, settings)))
}

/// Is a polled job finished? `None` means "still working".
pub fn poll_done(p: &Provider, body: &Value) -> Option<Vec<String>> {
    if p.id == "leonardo" {
        let status = body["generations_by_pk"]["status"].as_str().unwrap_or("");
        if status != "COMPLETE" { return None; }
    }
    let images = extract_images(p, body);
    if images.is_empty() { None } else { Some(images) }
}

/// A refusal a person can act on, rather than "401".
pub fn missing_key_message(p: &Provider) -> String {
    format!(
        "{} needs an API key. Settings → Image engine → {} — it is a paid engine, at about ${:.3} an \
         image, and nothing is charged until you generate.",
        p.label, p.label, p.price_hint
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_provider_presents_its_key_the_way_that_provider_wants() {
        // Getting this wrong produces a 401 that looks exactly like a wrong key, and people then go
        // and regenerate a key that was fine.
        let (h, v) = auth_header(provider("fal").unwrap(), "abc");
        assert_eq!((h, v.as_str()), ("Authorization", "Key abc"));
        let (h, v) = auth_header(provider("ideogram").unwrap(), "abc");
        assert_eq!((h, v.as_str()), ("Api-Key", "abc"));
        let (h, v) = auth_header(provider("leonardo").unwrap(), "abc");
        assert_eq!((h, v.as_str()), ("Authorization", "Bearer abc"));
        let (h, v) = auth_header(provider("recraft").unwrap(), "abc");
        assert_eq!((h, v.as_str()), ("Authorization", "Bearer abc"));
    }

    #[test]
    fn a_moved_endpoint_is_a_settings_edit_rather_than_a_release() {
        let p = provider("ideogram").unwrap();
        assert_eq!(base_url(p, &json!({})), "https://api.ideogram.ai");
        let moved = json!({ "ideogram_base_url": "https://api.ideogram.ai/v2/" });
        assert_eq!(base_url(p, &moved), "https://api.ideogram.ai/v2", "a trailing slash must not double up");
        assert_eq!(model_of(p, &json!({ "ideogram_model": "V_4" })), "V_4");
    }

    #[test]
    fn one_aspect_ratio_becomes_four_dialects() {
        // The app says "16:9" everywhere; nobody should have to remember that fal says
        // landscape_16_9 and Ideogram says 16x9.
        assert_eq!(shape_for(provider("fal").unwrap(), "16:9", 1920, 1080)["image_size"], json!("landscape_16_9"));
        assert_eq!(shape_for(provider("ideogram").unwrap(), "9:16", 1080, 1920)["aspect_ratio"], json!("9x16"));
        assert_eq!(shape_for(provider("recraft").unwrap(), "1:1", 1024, 1024)["size"], json!("1024x1024"));
        let leo = shape_for(provider("leonardo").unwrap(), "16:9", 1920, 1080);
        assert_eq!((leo["width"].clone(), leo["height"].clone()), (json!(1920), json!(1080)));
        // An aspect nobody declared falls back to square rather than to nothing.
        assert_eq!(shape_for(provider("fal").unwrap(), "21:9", 1024, 1024)["image_size"], json!("square_hd"));
    }

    #[test]
    fn a_negative_prompt_only_travels_where_it_is_honoured() {
        // Sending it where it is ignored is how somebody ends up believing "no lettering anywhere"
        // was respected when it was silently dropped.
        let ban = Some("lettering, watermark");
        let leo = request_body(provider("leonardo").unwrap(), &json!({}), "a lamp", ban, "1:1", 1024, 1024, 1);
        assert_eq!(leo["negative_prompt"], json!("lettering, watermark"));
        assert!(honours_negative(provider("ideogram").unwrap()));

        let fal = request_body(provider("fal").unwrap(), &json!({}), "a lamp", ban, "1:1", 1024, 1024, 1);
        assert!(fal.get("negative_prompt").is_none(), "fal has none, so it must not be pretended");
        assert!(!honours_negative(provider("fal").unwrap()));
        assert!(!honours_negative(provider("recraft").unwrap()));
    }

    #[test]
    fn an_empty_optional_setting_is_left_out_rather_than_sent_blank() {
        // "" is not a valid rendering speed and comes back as a 400 about a field the user never set.
        let body = request_body(provider("ideogram").unwrap(), &json!({}), "a lamp", None, "1:1", 1024, 1024, 1);
        assert!(body.get("rendering_speed").is_none());
        let set = json!({ "ideogram_rendering_speed": "QUALITY" });
        let body = request_body(provider("ideogram").unwrap(), &set, "a lamp", None, "1:1", 1024, 1024, 1);
        assert_eq!(body["rendering_speed"], json!("QUALITY"));
    }

    #[test]
    fn image_urls_are_found_wherever_a_provider_hides_them() {
        assert_eq!(
            extract_images(provider("fal").unwrap(), &json!({ "images": [{ "url": "https://a/1.png" }] })),
            vec!["https://a/1.png"]
        );
        assert_eq!(
            extract_images(provider("recraft").unwrap(), &json!({ "data": [{ "url": "https://a/2.svg" }] })),
            vec!["https://a/2.svg"]
        );
        // Ideogram has returned both a bare list and a list of objects across versions.
        assert_eq!(
            extract_images(provider("ideogram").unwrap(), &json!({ "data": ["https://a/3.png"] })),
            vec!["https://a/3.png"]
        );
        assert_eq!(
            extract_images(provider("leonardo").unwrap(),
                &json!({ "generations_by_pk": { "generated_images": [{ "url": "https://a/4.png" }] } })),
            vec!["https://a/4.png"]
        );
        // Anything unrecognised is no images, never a panic and never a half-URL.
        assert!(extract_images(provider("fal").unwrap(), &json!({ "error": "nope" })).is_empty());
        assert!(extract_images(provider("fal").unwrap(), &json!({ "images": [{ "url": "/relative" }] })).is_empty());
    }

    #[test]
    fn a_polled_job_is_only_finished_when_the_provider_says_so() {
        let leo = provider("leonardo").unwrap();
        let started = json!({ "sdGenerationJob": { "generationId": "gen-1" } });
        assert_eq!(poll_url(leo, &json!({}), &started).unwrap(),
                   "https://cloud.leonardo.ai/api/rest/v1/generations/gen-1");

        let pending = json!({ "generations_by_pk": { "status": "PENDING", "generated_images": [] } });
        assert!(poll_done(leo, &pending).is_none());
        // Images present but the job not marked complete is still not done — Leonardo fills that
        // array as it goes, and taking it early gives a partial set.
        let partial = json!({ "generations_by_pk": { "status": "PENDING",
                              "generated_images": [{ "url": "https://a/1.png" }] } });
        assert!(poll_done(leo, &partial).is_none());
        let done = json!({ "generations_by_pk": { "status": "COMPLETE",
                           "generated_images": [{ "url": "https://a/1.png" }] } });
        assert_eq!(poll_done(leo, &done).unwrap(), vec!["https://a/1.png"]);

        // A provider that answers in one request never polls.
        assert!(poll_url(provider("fal").unwrap(), &json!({}), &started).is_none());
    }

    #[test]
    fn every_provider_says_what_it_costs_before_anything_is_charged() {
        for p in PROVIDERS {
            assert!(p.price_hint > 0.0, "{} has no price", p.id);
            let msg = missing_key_message(p);
            assert!(msg.contains(p.label));
            assert!(msg.contains("paid"), "a refusal must say it is paid: {msg}");
            assert!(msg.contains("nothing is charged"), "{msg}");
        }
        // Exactly one of them returns vector, and it is the one advertised for logos.
        let vector: Vec<&str> = PROVIDERS.iter().filter(|p| p.vector_output).map(|p| p.id).collect();
        assert_eq!(vector, vec!["recraft"]);
    }

    #[test]
    fn a_free_engine_is_never_mistaken_for_one_of_these() {
        assert!(is_paid_api("fal") && is_paid_api("leonardo"));
        for free in ["flux", "comfyui", "gemini", "midjourney", ""] {
            assert!(!is_paid_api(free), "{free} must not be treated as a paid API");
        }
    }
}
