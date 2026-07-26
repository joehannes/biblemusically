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
// Suno without a browser
//
// Suno has no official API — checked July 2026, and every wrapper that exists is unofficial and
// cookie-based. The important discovery is that they are all **pure HTTP at generation time**: the
// session cookie is exchanged for a Clerk JWT, that JWT is refreshed before each call, and the session
// stays usable indefinitely while the cookie is valid.
//
// That is what makes Suno possible on a phone. Android gives a Tauri app exactly one webview — its own
// UI — so the desktop trick of driving a hidden second webview cannot work there. But if no browser is
// needed *at generation time*, the phone only needs the cookie, and capturing a cookie is a one-off.
//
// Two deliberate choices:
//
//   • **The hosts are settings, not constants.** The base moved from `studio-api.suno.ai` to
//     `studio-api.prod.suno.com`, and it will move again. Hardcoding it means every move is a release;
//     a setting means it is a paste. The defaults are what works today.
//   • **A wrapper URL is a first-class fallback.** The community projects (gcui-art/suno-api and the
//     FastAPI one) get patched when Suno changes their front end, often before we would notice. If the
//     native path starts failing and a wrapper is configured, calls go there instead — so the app
//     degrades to somebody else's maintenance rather than to nothing.
//
// The honest risks, stated in the UI too: unofficial and "for research"; the cookie expires and has to
// be re-captured from a real session; accounts can be rate-limited or suspended.
// ────────────────────────────────────────────────────────────────

const DEFAULT_API: &str = "https://studio-api.prod.suno.com";
const DEFAULT_CLERK: &str = "https://clerk.suno.com";
/// Clerk requires a version parameter and rejects requests without one.
const CLERK_JS_VERSION: &str = "5.65.0";

/// Trim a configured base URL into something safe to concatenate.
///
/// A trailing slash plus a leading slash on the path gives `//api/...`, which some gateways route to a
/// different place entirely; and a base with a path already on it must not be silently doubled.
pub fn normalise_base(raw: &str, fallback: &str) -> String {
    let t = raw.trim().trim_end_matches('/');
    if t.is_empty() { return fallback.to_string(); }
    if t.starts_with("http://") || t.starts_with("https://") { t.to_string() }
    else { format!("https://{t}") }
}

/// The body Suno's generate endpoint expects.
///
/// `mv` is the model version and travels as given: it changes on Suno's schedule, so it is a setting
/// rather than something to keep up with in code. `make_instrumental` has to be a real boolean — a
/// string "false" is truthy on the far side.
pub fn generate_payload(
    prompt: &str, tags: &str, title: &str, model: &str, instrumental: bool, custom: bool,
) -> Value {
    if custom {
        // Custom mode: the lyrics are supplied verbatim as `prompt`, and `tags` carries the style.
        json!({
            "prompt": prompt,
            "tags": tags,
            "title": title,
            "mv": model,
            "make_instrumental": instrumental,
            "continue_clip_id": Value::Null,
            "continue_at": Value::Null,
        })
    } else {
        // Description mode: Suno writes the lyrics from a description, so `gpt_description_prompt`
        // carries it and `prompt` must be empty rather than absent.
        json!({
            "gpt_description_prompt": prompt,
            "prompt": "",
            "tags": tags,
            "mv": model,
            "make_instrumental": instrumental,
        })
    }
}

/// Pull the clips out of whatever shape the response has.
///
/// The generate call answers with `{clips: [...]}`, the feed answers with a bare array, and a wrapper
/// may answer with either. All three become the same list rather than three call sites each guessing.
pub fn clips_from(response: &Value) -> Vec<Value> {
    if let Some(a) = response.as_array() { return a.clone(); }
    for key in ["clips", "data", "songs"] {
        if let Some(a) = response[key].as_array() { return a.clone(); }
    }
    Vec::new()
}

/// Is a clip finished, and where is its audio?
///
/// Suno reports `status` as submitted → queued → streaming → complete. **`streaming` already has a
/// playable URL**, which is why waiting only for `complete` wastes a minute or two per song; but the
/// file at that point is still growing, so it is worth reporting separately rather than treating as done.
pub fn clip_state(clip: &Value) -> (&'static str, String) {
    let status = clip["status"].as_str().unwrap_or("");
    let audio = clip["audio_url"].as_str()
        .filter(|u| !u.is_empty())
        .map(|u| u.to_string())
        .unwrap_or_default();
    let state = match status {
        "complete" if !audio.is_empty() => "complete",
        "streaming" if !audio.is_empty() => "streaming",
        "error" => "error",
        "" if !audio.is_empty() => "complete",     // a wrapper that omits status but has the file
        _ => "pending",
    };
    (state, audio)
}

/// How long an unanswered "your Suno session expired" prompt is allowed to stall generation.
///
/// After this, the app stops waiting for a person and uses the wrapper instead. Overnight is exactly
/// when a cookie expires and exactly when nobody is there to paste a new one, so waiting indefinitely
/// means losing the night's output to a dialog nobody saw.
const DEFAULT_FALLBACK_MINUTES: i64 = 15;

/// Has the outstanding cookie prompt gone unanswered long enough to stop waiting?
///
/// `None` for `prompted_at` means nobody has been asked yet, which is not a timeout — the first failure
/// should still try the native path and ask.
pub fn prompt_timed_out(prompted_at: Option<&str>, now: &str, minutes: i64) -> bool {
    let Some(at) = prompted_at.filter(|s| !s.is_empty()) else { return false };
    let (Ok(then), Ok(now)) = (
        chrono::DateTime::parse_from_rfc3339(at),
        chrono::DateTime::parse_from_rfc3339(now),
    ) else { return false };
    (now - then).num_minutes() >= minutes.max(1)
}

/// Does this failure mean the cookie is finished, as opposed to something transient?
///
/// The distinction decides whether to ask the user for a new cookie or just retry. Getting it wrong
/// either nags them for no reason or silently fails all night.
pub fn is_auth_failure(status: u16, body: &str) -> bool {
    if matches!(status, 401 | 403) { return true; }
    let lower = body.to_lowercase();
    lower.contains("unauthenticated")
        || lower.contains("unauthorized")
        || lower.contains("invalid token")
        || lower.contains("session not found")
        || lower.contains("no active session")
}

fn client() -> Res<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        // Suno's front end is a browser; a request that does not look like one gets a different answer.
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36")
        .build().map_err(e)
}

/// Exchange the stored cookie for a fresh JWT.
///
/// Two calls: find the active session, then mint a token on it. Done before every generation, which is
/// what keeps a session alive indefinitely while the cookie lasts.
async fn refresh_jwt(cookie: &str, clerk_base: &str) -> Res<String> {
    let http = client()?;
    let who = http.get(format!("{clerk_base}/v1/client?_clerk_js_version={CLERK_JS_VERSION}"))
        .header("Cookie", cookie)
        .send().await.map_err(e)?;
    let status = who.status().as_u16();
    let body = who.text().await.unwrap_or_default();
    if is_auth_failure(status, &body) {
        return Err("SUNO_COOKIE_EXPIRED".into());
    }
    let parsed: Value = serde_json::from_str(&body)
        .map_err(|_| format!("Suno's sign-in service answered with something unreadable ({status})"))?;
    let session = parsed["response"]["last_active_session_id"].as_str()
        .or_else(|| parsed["last_active_session_id"].as_str())
        .ok_or("SUNO_COOKIE_EXPIRED")?
        .to_string();

    let token = http.post(format!(
        "{clerk_base}/v1/client/sessions/{session}/tokens?_clerk_js_version={CLERK_JS_VERSION}"))
        .header("Cookie", cookie)
        .header("Content-Length", "0")
        .send().await.map_err(e)?;
    let status = token.status().as_u16();
    let body = token.text().await.unwrap_or_default();
    if is_auth_failure(status, &body) { return Err("SUNO_COOKIE_EXPIRED".into()); }
    let parsed: Value = serde_json::from_str(&body)
        .map_err(|_| format!("Suno's token service answered with something unreadable ({status})"))?;
    parsed["jwt"].as_str().map(|s| s.to_string())
        .ok_or_else(|| "Suno returned no token for this session.".to_string())
}

async fn settings_of(state: &AppState) -> Value {
    state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.ok().flatten()
        .map(bson_to_value).unwrap_or_default()
}

/// Remember that the cookie needs re-capturing, so the guide can raise it instead of jobs failing all
/// night with nobody told.
async fn flag_cookie_expired(state: &AppState) {
    // `prompted_at` is only set the first time, so the timeout measures how long the *user* has been
    // asked — not how long since the last retry, which would reset the clock forever.
    let existing = settings_of(state).await;
    let already = existing["suno_cookie_prompted_at"].as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let mut set = doc! {
        "suno_cookie_state": "expired",
        "suno_cookie_checked": crate::models::now_iso(),
    };
    if already.is_none() {
        set.insert("suno_cookie_prompted_at", crate::models::now_iso());
    }
    let _ = state.db.collection::<Document>("settings")
        .update_one(doc! { "_id": "singleton" }, doc! { "$set": set }).await;
}

async fn flag_cookie_ok(state: &AppState) {
    let _ = state.db.collection::<Document>("settings")
        .update_one(doc! { "_id": "singleton" },
                    doc! { "$set": { "suno_cookie_state": "ok",
                                     "suno_cookie_checked": crate::models::now_iso(),
                                     // The prompt is answered, so the timeout clock resets.
                                     "suno_cookie_prompted_at": "" } }).await;
}

#[derive(serde::Deserialize)]
pub struct SunoRequest {
    /// Lyrics in custom mode, or a description otherwise.
    pub prompt: String,
    #[serde(default)]
    pub tags: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    /// True when `prompt` is the lyrics rather than a description.
    #[serde(default)]
    pub custom: Option<bool>,
    #[serde(default)]
    pub instrumental: Option<bool>,
}

/// Ask Suno for a song. Returns the clip ids to poll.
///
/// Native first; the configured wrapper second. A wrapper exists precisely because Suno's front end
/// changes faster than any one person follows it.
#[tauri::command]
pub async fn suno_generate(state: State<'_, AppState>, payload: SunoRequest) -> Res<Value> {
    let settings = settings_of(&state).await;
    let wrapper_url = settings["suno_wrapper_url"].as_str().unwrap_or("").trim().to_string();
    let minutes = settings["suno_fallback_after_minutes"].as_i64().unwrap_or(DEFAULT_FALLBACK_MINUTES);

    // The cookie is known dead and the person was asked long enough ago. Stop trying a path that cannot
    // work and stop waiting for somebody who is not there — go straight to the wrapper.
    if settings["suno_cookie_state"].as_str() == Some("expired")
        && prompt_timed_out(settings["suno_cookie_prompted_at"].as_str(), &crate::models::now_iso(), minutes)
        && !wrapper_url.is_empty()
    {
        let clips = suno_generate_wrapper(&wrapper_url, &payload).await.map_err(|w| format!(
            "The Suno cookie expired over {minutes} minutes ago and nobody re-captured it, so the \
             wrapper was tried instead — and it failed too: {w}"))?;
        let _ = state.db.collection::<Document>("settings")
            .update_one(doc! { "_id": "singleton" },
                        doc! { "$set": { "suno_route": "wrapper-auto" } }).await;
        return Ok(json!({
            "clips": clips,
            "ids": clips.iter().filter_map(|c| c["id"].as_str()).collect::<Vec<_>>(),
            "via": "wrapper-auto",
            "why": format!("The Suno session expired and the prompt went unanswered for {minutes} \
                            minutes, so this used the wrapper. Re-capture the cookie to go back to the \
                            direct route."),
        }));
    }

    let native = suno_generate_native(&state, &settings, &payload).await;
    match native {
        Ok(v) => Ok(v),
        Err(err) => {
            let wrapper = settings["suno_wrapper_url"].as_str().unwrap_or("").trim().to_string();
            if wrapper.is_empty() {
                return Err(if err == "SUNO_COOKIE_EXPIRED" {
                    "Your Suno session has expired. Capture a fresh cookie (Settings → Music engines → \
                     Suno → Capture) and try again.".to_string()
                } else { err });
            }
            // Say which path answered: debugging a music engine is hard enough without that being a
            // mystery.
            let via = suno_generate_wrapper(&wrapper, &payload).await
                .map_err(|w| format!("Native Suno failed ({err}); the wrapper also failed ({w})"))?;
            Ok(json!({ "clips": via, "via": "wrapper", "native_error": err }))
        }
    }
}

async fn suno_generate_native(state: &AppState, settings: &Value, payload: &SunoRequest) -> Res<Value> {
    let cookie = crate::vault::get("suno_cookie").map_err(e)?.unwrap_or_default();
    if cookie.trim().is_empty() {
        return Err("No Suno cookie saved yet.".into());
    }
    let clerk = normalise_base(settings["suno_clerk_base"].as_str().unwrap_or(""), DEFAULT_CLERK);
    let api = normalise_base(settings["suno_api_base"].as_str().unwrap_or(""), DEFAULT_API);
    let model = settings["suno_model"].as_str().unwrap_or("chirp-v4").to_string();

    let jwt = match refresh_jwt(&cookie, &clerk).await {
        Ok(j) => j,
        Err(err) => {
            if err == "SUNO_COOKIE_EXPIRED" { flag_cookie_expired(state).await; }
            return Err(err);
        }
    };

    let body = generate_payload(
        &payload.prompt,
        payload.tags.as_deref().unwrap_or(""),
        payload.title.as_deref().unwrap_or(""),
        &model,
        payload.instrumental.unwrap_or(false),
        payload.custom.unwrap_or(true),
    );
    let http = client()?;
    let r = http.post(format!("{api}/api/generate/v2/"))
        .bearer_auth(&jwt)
        .json(&body)
        .send().await.map_err(e)?;
    let status = r.status().as_u16();
    let text = r.text().await.unwrap_or_default();
    if is_auth_failure(status, &text) {
        flag_cookie_expired(state).await;
        return Err("SUNO_COOKIE_EXPIRED".into());
    }
    if !(200..300).contains(&status) {
        return Err(format!("Suno refused the request ({status}): {}",
            text.chars().take(300).collect::<String>()));
    }
    let parsed: Value = serde_json::from_str(&text)
        .map_err(|_| "Suno answered with something unreadable.".to_string())?;
    let clips = clips_from(&parsed);
    if clips.is_empty() {
        return Err("Suno accepted the request but returned no clips.".into());
    }
    flag_cookie_ok(state).await;
    Ok(json!({
        "clips": clips,
        "ids": clips.iter().filter_map(|c| c["id"].as_str()).collect::<Vec<_>>(),
        "via": "native",
    }))
}

async fn suno_generate_wrapper(wrapper: &str, payload: &SunoRequest) -> Res<Vec<Value>> {
    let base = normalise_base(wrapper, "");
    let http = client()?;
    // The community wrappers agree on these two shapes.
    let path = if payload.custom.unwrap_or(true) { "/api/custom_generate" } else { "/api/generate" };
    let body = json!({
        "prompt": payload.prompt,
        "tags": payload.tags.clone().unwrap_or_default(),
        "title": payload.title.clone().unwrap_or_default(),
        "make_instrumental": payload.instrumental.unwrap_or(false),
        "wait_audio": false,
    });
    let r = http.post(format!("{base}{path}")).json(&body).send().await.map_err(e)?;
    let status = r.status().as_u16();
    let text = r.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(format!("wrapper answered {status}: {}", text.chars().take(200).collect::<String>()));
    }
    let parsed: Value = serde_json::from_str(&text).map_err(|_| "wrapper answered unreadable JSON".to_string())?;
    let clips = clips_from(&parsed);
    if clips.is_empty() { return Err("wrapper returned no clips".into()); }
    Ok(clips)
}

/// Poll clips until they have audio.
#[tauri::command]
pub async fn suno_poll(state: State<'_, AppState>, ids: Vec<String>) -> Res<Value> {
    if ids.is_empty() { return Err("no clip ids to poll".into()); }
    let settings = settings_of(&state).await;
    let joined = ids.join(",");

    let native = async {
        let cookie = crate::vault::get("suno_cookie").map_err(e)?.unwrap_or_default();
        if cookie.trim().is_empty() { return Err("no cookie".to_string()); }
        let clerk = normalise_base(settings["suno_clerk_base"].as_str().unwrap_or(""), DEFAULT_CLERK);
        let api = normalise_base(settings["suno_api_base"].as_str().unwrap_or(""), DEFAULT_API);
        let jwt = refresh_jwt(&cookie, &clerk).await?;
        let http = client()?;
        let r = http.get(format!("{api}/api/feed/?ids={joined}"))
            .bearer_auth(&jwt).send().await.map_err(e)?;
        let status = r.status().as_u16();
        let text = r.text().await.unwrap_or_default();
        if is_auth_failure(status, &text) { return Err("SUNO_COOKIE_EXPIRED".to_string()); }
        serde_json::from_str::<Value>(&text).map_err(|_| "unreadable feed".to_string())
    }.await;

    let response = match native {
        Ok(v) => v,
        Err(err) => {
            let wrapper = settings["suno_wrapper_url"].as_str().unwrap_or("").trim().to_string();
            if wrapper.is_empty() { return Err(err); }
            let base = normalise_base(&wrapper, "");
            let http = client()?;
            let r = http.get(format!("{base}/api/get?ids={joined}")).send().await.map_err(e)?;
            let text = r.text().await.unwrap_or_default();
            serde_json::from_str::<Value>(&text).map_err(|_| format!("native failed ({err}) and the wrapper answered unreadable JSON"))?
        }
    };

    let clips = clips_from(&response);
    let detailed: Vec<Value> = clips.iter().map(|c| {
        let (state, audio) = clip_state(c);
        json!({
            "id": c["id"], "state": state, "audio_url": audio,
            "title": c["title"], "status": c["status"],
        })
    }).collect();
    Ok(json!({
        "clips": detailed,
        // `streaming` already plays, so a caller that only waits for `complete` wastes a minute or two
        // per song — but the file is still growing, so it is reported apart from done.
        "playable": detailed.iter().filter(|c| c["state"] != "pending" && c["state"] != "error").count(),
        "done": detailed.iter().filter(|c| c["state"] == "complete").count(),
    }))
}

#[derive(serde::Deserialize)]
pub struct CookieRequest {
    /// The full `Cookie:` header from a signed-in suno.com session.
    pub cookie: String,
}

/// Save a captured cookie and verify it immediately.
///
/// Verified on the spot rather than at the next generation: a cookie that was pasted wrong should say
/// so while the person is still looking at the screen.
#[tauri::command]
pub async fn save_suno_cookie(state: State<'_, AppState>, payload: CookieRequest) -> Res<Value> {
    let cookie = payload.cookie.trim().to_string();
    if cookie.len() < 20 || !cookie.contains('=') {
        return Err("That does not look like a Cookie header. Copy the whole line, not just one value.".into());
    }
    crate::vault::put("suno_cookie", &cookie).map_err(e)?;

    let settings = settings_of(&state).await;
    let clerk = normalise_base(settings["suno_clerk_base"].as_str().unwrap_or(""), DEFAULT_CLERK);
    match refresh_jwt(&cookie, &clerk).await {
        Ok(_) => {
            flag_cookie_ok(&state).await;
            Ok(json!({ "ok": true, "note": "Suno session is live." }))
        }
        Err(err) => {
            flag_cookie_expired(&state).await;
            Err(if err == "SUNO_COOKIE_EXPIRED" {
                "Saved, but Suno did not accept it. Make sure you copied the Cookie header from a \
                 signed-in suno.com tab.".to_string()
            } else { err })
        }
    }
}

/// Whether Suno is usable right now, and by which route.
#[tauri::command]
pub async fn suno_status(state: State<'_, AppState>) -> Res<Value> {
    let settings = settings_of(&state).await;
    let has_cookie = crate::vault::get("suno_cookie").ok().flatten()
        .map(|c| !c.trim().is_empty()).unwrap_or(false);
    let wrapper = settings["suno_wrapper_url"].as_str().unwrap_or("").trim().to_string();
    let cookie_state = settings["suno_cookie_state"].as_str().unwrap_or("unknown").to_string();
    Ok(json!({
        "has_cookie": has_cookie,
        "cookie_state": cookie_state,
        "checked": settings["suno_cookie_checked"],
        "wrapper": wrapper,
        "usable": has_cookie && cookie_state != "expired" || !wrapper.is_empty(),
        "api_base": normalise_base(settings["suno_api_base"].as_str().unwrap_or(""), DEFAULT_API),
        "clerk_base": normalise_base(settings["suno_clerk_base"].as_str().unwrap_or(""), DEFAULT_CLERK),
        // Worth saying plainly wherever this appears.
        "caveat": "Suno has no official API. This uses your own signed-in session, which expires \
                   periodically and can break when Suno changes their site. A wrapper URL is the \
                   fallback for exactly that.",
        "needs_capture": !has_cookie || cookie_state == "expired",
        "prompted_at": settings["suno_cookie_prompted_at"],
        "fallback_after_minutes": settings["suno_fallback_after_minutes"].as_i64().unwrap_or(DEFAULT_FALLBACK_MINUTES),
        // Whether the app has already stopped waiting for a person.
        "auto_fallen_back": settings["suno_route"].as_str() == Some("wrapper-auto"),
        "will_fall_back": prompt_timed_out(
            settings["suno_cookie_prompted_at"].as_str(), &crate::models::now_iso(),
            settings["suno_fallback_after_minutes"].as_i64().unwrap_or(DEFAULT_FALLBACK_MINUTES),
        ) && !wrapper.is_empty(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_configured_base_is_normalised_rather_than_concatenated_blindly() {
        // `base + "/api/..."` with a trailing slash gives `//api/...`, which some gateways route
        // somewhere else entirely.
        assert_eq!(normalise_base("https://x.dev/", DEFAULT_API), "https://x.dev");
        assert_eq!(normalise_base("x.dev", DEFAULT_API), "https://x.dev");
        assert_eq!(normalise_base("  ", DEFAULT_API), DEFAULT_API);
        assert_eq!(normalise_base("http://localhost:3000", DEFAULT_API), "http://localhost:3000");
    }

    #[test]
    fn the_default_base_is_the_one_that_works_today() {
        // It moved from studio-api.suno.ai; a stale default is a silent 404 for everybody.
        assert_eq!(DEFAULT_API, "https://studio-api.prod.suno.com");
        assert!(!DEFAULT_API.contains("suno.ai"));
    }

    #[test]
    fn custom_and_description_modes_send_different_shapes() {
        let custom = generate_payload("[Verse]\nlyrics", "gospel", "Genesis 1", "chirp-v4", false, true);
        assert_eq!(custom["prompt"], json!("[Verse]\nlyrics"), "custom mode sends the lyrics as prompt");
        assert!(custom.get("gpt_description_prompt").is_none());

        let described = generate_payload("a calm hymn about light", "gospel", "", "chirp-v4", false, false);
        assert_eq!(described["gpt_description_prompt"], json!("a calm hymn about light"));
        // Empty rather than absent: the far side wants the key present.
        assert_eq!(described["prompt"], json!(""));
    }

    #[test]
    fn make_instrumental_is_a_real_boolean() {
        // A string "false" is truthy on the far side, which produces a silent instrumental.
        let p = generate_payload("x", "y", "z", "chirp-v4", false, true);
        assert_eq!(p["make_instrumental"], json!(false));
        assert!(p["make_instrumental"].is_boolean());
    }

    #[test]
    fn clips_are_found_in_every_shape_the_apis_return() {
        assert_eq!(clips_from(&json!({ "clips": [{ "id": "a" }] })).len(), 1);
        assert_eq!(clips_from(&json!([{ "id": "a" }, { "id": "b" }])).len(), 2);
        assert_eq!(clips_from(&json!({ "data": [{ "id": "a" }] })).len(), 1);
        assert_eq!(clips_from(&json!({ "nothing": true })).len(), 0);
    }

    #[test]
    fn streaming_is_playable_but_not_finished() {
        // Waiting only for `complete` wastes a minute or two per song; treating `streaming` as done
        // downloads a file that is still growing. Both matter, so they are separate states.
        let streaming = json!({ "status": "streaming", "audio_url": "https://x/a.mp3" });
        assert_eq!(clip_state(&streaming), ("streaming", "https://x/a.mp3".to_string()));

        let complete = json!({ "status": "complete", "audio_url": "https://x/a.mp3" });
        assert_eq!(clip_state(&complete).0, "complete");

        // Status says complete but there is no file: that is not done.
        let liar = json!({ "status": "complete", "audio_url": "" });
        assert_eq!(clip_state(&liar).0, "pending");

        assert_eq!(clip_state(&json!({ "status": "queued" })).0, "pending");
        assert_eq!(clip_state(&json!({ "status": "error" })).0, "error");
        // A wrapper that omits status but has the file is finished.
        assert_eq!(clip_state(&json!({ "audio_url": "https://x/a.mp3" })).0, "complete");
    }

    #[test]
    fn an_unanswered_prompt_stops_waiting_after_the_timeout() {
        // A cookie expires overnight, which is exactly when nobody is there to paste a new one. Waiting
        // for a person indefinitely means losing the night's output to a dialog nobody saw.
        let now = "2026-07-26T12:00:00+00:00";
        assert!(prompt_timed_out(Some("2026-07-26T11:44:00+00:00"), now, 15), "16 minutes ago");
        assert!(!prompt_timed_out(Some("2026-07-26T11:50:00+00:00"), now, 15), "only 10 minutes ago");
        // Nobody asked yet is not a timeout: the first failure should still try natively and ask.
        assert!(!prompt_timed_out(None, now, 15));
        assert!(!prompt_timed_out(Some(""), now, 15));
        // Exactly at the boundary counts as timed out — falling back is the safer side to err on.
        assert!(prompt_timed_out(Some("2026-07-26T11:45:00+00:00"), now, 15));
        // Garbage timestamps must not silently mean "fall back immediately".
        assert!(!prompt_timed_out(Some("not a date"), now, 15));
    }

    #[test]
    fn an_expired_cookie_is_told_apart_from_a_transient_failure() {
        // Getting this wrong either nags for a new cookie every time Suno hiccups, or silently fails
        // all night without asking.
        assert!(is_auth_failure(401, ""));
        assert!(is_auth_failure(403, ""));
        assert!(is_auth_failure(200, r#"{"error":"Unauthenticated"}"#));
        assert!(is_auth_failure(400, "no active session for this client"));
        assert!(!is_auth_failure(500, "internal server error"));
        assert!(!is_auth_failure(429, "too many requests"));
        assert!(!is_auth_failure(200, r#"{"clips":[]}"#));
    }
}
