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
// Accounts, trial, subscription
//
// The server signs an entitlement; this verifies it and decides what the app will do. Three things make
// that more than a checkbox:
//
//   1. **It is verified, not trusted.** The entitlement is Ed25519-signed by the server and checked here
//      against a public key compiled into the binary. Editing the cached JSON does nothing: the
//      signature stops matching and the entitlement is discarded.
//
//   2. **The gate is in the backend, not the interface.** A blurred panel is a CSS property anybody can
//      delete. So every gated command asks `require()` before doing any work — the frontend's blur is
//      only there to explain what is happening, and removing it reveals a page whose buttons still
//      refuse. That is the difference between an inconvenience and a lock.
//
//   3. **The project cache is encrypted with a key derived from the entitlement.** This is the part that
//      closes the loop the user actually worried about: you cannot escape the subscription by making a
//      new trial account and re-importing your projects, because a new account has a different `vault`
//      salt and cannot decrypt the old cache. And a cracked binary that skips the check still cannot
//      read the work, because the key was never in the binary.
//
// What this is not: unbreakable. The app runs on the user's machine with their own API keys, so there is
// nothing the server can hold hostage. Somebody determined will get past it. The goal is that it is hard
// enough that no casual tool does it, and that the *data* stays sealed even then.
//
// A grace window exists on purpose. A signed entitlement lasts about a day, and the app accepts a stale
// one for a further week while it cannot reach the server. Somebody on a plane, or with a flaky
// connection, must not lose access to work they paid for — and a week is far too short to be a way of
// avoiding payment.
// ────────────────────────────────────────────────────────────────

/// Where the entitlement server lives. Overridable in settings so a self-hoster or a staging deployment
/// does not need a rebuild.
const DEFAULT_BASE: &str = "https://studio-lightkid.johannes-neugschwentner.workers.dev";

/// The server's Ed25519 public key, base64url. Compiled in: a public key in a config file is a public
/// key an attacker can replace with their own.
const SUBS_PUBLIC_KEY: &str = "9-9bAxvvDtG98OKRR8xn3OeOHk0S0aruy4UA8FUmQwY";

/// How long a stale entitlement keeps working when the server cannot be reached.
const GRACE_DAYS: i64 = 7;

/// Every capability the app gates. Named rather than derived from a plan, so adding one later does not
/// need the server and the client to agree about what a plan means.
pub const FEATURES: &[&str] = &["read", "generate", "publish", "export", "save_copies", "remote_sync"];

fn b64u_decode(s: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s.trim()).ok()
}

/// Check the signature and the expiry, and return the payload.
///
/// `None` for anything that does not verify — a tampered, truncated or foreign-signed token is treated
/// exactly like no token at all, because the alternative is deciding how much of a forgery to believe.
pub fn verify_entitlement(token: &str, public_key_b64: &str, now_unix: i64, grace_days: i64) -> Option<Value> {
    use ed25519_dalek::{Signature, VerifyingKey, Verifier};

    let (body, sig_b64) = token.trim().split_once('.')?;
    let key_bytes: [u8; 32] = b64u_decode(public_key_b64)?.try_into().ok()?;
    let key = VerifyingKey::from_bytes(&key_bytes).ok()?;
    let sig_bytes: [u8; 64] = b64u_decode(sig_b64)?.try_into().ok()?;
    let sig = Signature::from_bytes(&sig_bytes);
    key.verify(body.as_bytes(), &sig).ok()?;

    let payload: Value = serde_json::from_slice(&b64u_decode(body)?).ok()?;
    let exp = payload["exp"].as_i64().unwrap_or(0);
    // Past its expiry but inside the grace window: usable, and flagged as stale so the UI can say so.
    if now_unix > exp + grace_days * 86400 { return None; }
    let mut out = payload;
    out["stale"] = json!(now_unix > exp);
    Some(out)
}

/// Is this capability allowed by a verified entitlement?
///
/// Absent means no. A missing feature flag is the safe answer, not an old client's excuse to allow
/// everything.
pub fn allows(payload: &Value, feature: &str) -> bool {
    payload["features"][feature].as_bool().unwrap_or(false)
}

/// A human sentence for a refusal.
///
/// Written here rather than at each call site so every gated command explains itself the same way, and
/// so the reason is about the person's situation rather than about a boolean.
pub fn refusal(payload: Option<&Value>, feature: &str) -> String {
    let status = payload.and_then(|p| p["status"].as_str()).unwrap_or("none");
    match (status, feature) {
        ("none", _) => "Sign in to use this — there is a free week, and no card is asked for.".into(),
        ("blocked", _) => "This account is blocked. Reply to any of your reports and I will look at it.".into(),
        ("trial", "export") => "Exporting is part of a subscription. Your work is kept safe in the app \
             for the whole trial, so nothing is lost by waiting.".into(),
        ("trial", "save_copies") => "Saving a copy out of the app is part of a subscription. Everything \
             you make during the trial stays here and stays yours.".into(),
        ("trial", "remote_sync") => "Syncing to your own git remote is part of a subscription.".into(),
        ("expired", _) => "The free week is over. Your projects are still here and still intact — a \
             subscription unlocks them again.".into(),
        _ => format!("A subscription is needed for this ({feature})."),
    }
}

// ── Local state ─────────────────────────────────────────────────────────────

async fn settings_of(state: &AppState) -> Value {
    state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.ok().flatten()
        .map(bson_to_value).unwrap_or_default()
}

fn base_of(settings: &Value) -> String {
    let raw = settings["subs_base_url"].as_str().unwrap_or("").trim();
    if raw.is_empty() { DEFAULT_BASE.to_string() } else { raw.trim_end_matches('/').to_string() }
}

/// The verified entitlement, or `None`.
pub async fn current(state: &AppState) -> Option<Value> {
    let settings = settings_of(state).await;
    let token = settings["subs_entitlement"].as_str().unwrap_or("");
    if token.is_empty() { return None; }
    verify_entitlement(token, SUBS_PUBLIC_KEY, chrono::Utc::now().timestamp(), GRACE_DAYS)
}

/// The gate every restricted command calls first.
///
/// Backend-side on purpose: the interface's blur is an explanation, not a lock. Deleting the CSS reveals
/// a page whose buttons still refuse.
pub async fn require(state: &AppState, feature: &str) -> Res<Value> {
    let ent = current(state).await;
    match &ent {
        Some(p) if allows(p, feature) => Ok(p.clone()),
        _ => Err(refusal(ent.as_ref(), feature)),
    }
}

/// The key material the project cache is encrypted with.
///
/// Derived from the account's server-issued salt, so:
///   • a different account cannot open this cache — which is what stops "new trial, re-import";
///   • a patched binary that skips the entitlement check still has no key, because the key was never in
///     the binary in the first place.
pub async fn cache_key_material(state: &AppState) -> Option<String> {
    let ent = current(state).await?;
    let vault = ent["vault"].as_str()?;
    let sub = ent["sub"].as_str()?;
    // Both halves: the account id ties it to the person, the salt to the server's record of them.
    Some(format!("{sub}:{vault}"))
}

fn http() -> Res<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        // Cloudflare's bot protection answers a request with no browser User-Agent with an error page
        // rather than the Worker's response. Found the hard way; without this every call fails with
        // something that looks nothing like the real cause.
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) \
                     Chrome/126.0 Safari/537.36 StudioLightkid")
        .build().map_err(e)
}

/// A stable-per-install identifier, so the admin can see "one account, nine machines".
///
/// Not a fingerprint of the hardware: a random id kept in the store. It identifies an installation, which
/// is all the licensing question needs, and it cannot be used to recognise the person anywhere else.
async fn device_id(state: &AppState) -> String {
    let settings = settings_of(state).await;
    if let Some(id) = settings["device_id"].as_str().filter(|s| !s.is_empty()) {
        return id.to_string();
    }
    let id = uuid::Uuid::new_v4().to_string();
    let _ = state.db.collection::<Document>("settings")
        .update_one(doc! { "_id": "singleton" }, doc! { "$set": { "device_id": &id } }).await;
    id
}

async fn store_entitlement(state: &AppState, token: &str) {
    let _ = state.db.collection::<Document>("settings")
        .update_one(doc! { "_id": "singleton" },
                    doc! { "$set": { "subs_entitlement": token,
                                     "subs_checked_at": crate::models::now_iso() } }).await;
}

// ── Commands ────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct SignInRequest {
    /// A Google ID token, from the sign-in the app already does for YouTube.
    pub id_token: String,
    #[serde(default)]
    pub username: Option<String>,
    /// Whoever referred them, if they arrived through a share link.
    #[serde(default)]
    pub referral: Option<String>,
}

/// Sign in (creating the account and starting the trial on first sight).
#[tauri::command]
pub async fn subs_sign_in(state: State<'_, AppState>, payload: SignInRequest) -> Res<Value> {
    let settings = settings_of(&state).await;
    let base = base_of(&settings);
    let body = json!({
        "id_token": payload.id_token,
        "username": payload.username.unwrap_or_default(),
        "referral": payload.referral.unwrap_or_default(),
        "device_id": device_id(&state).await,
    });
    let r = http()?.post(format!("{base}/v1/auth/google")).json(&body).send().await.map_err(e)?;
    let status = r.status();
    let text = r.text().await.unwrap_or_default();
    let parsed: Value = serde_json::from_str(&text)
        .map_err(|_| format!("The account server answered with something unreadable ({status})."))?;
    if parsed["ok"].as_bool() != Some(true) {
        return Err(parsed["error"].as_str().unwrap_or("sign-in failed").to_string());
    }
    let token = parsed["entitlement"].as_str().unwrap_or("").to_string();
    // Verify what we were just handed. A server that has been replaced cannot grant anything.
    let verified = verify_entitlement(&token, SUBS_PUBLIC_KEY, chrono::Utc::now().timestamp(), GRACE_DAYS)
        .ok_or("The account server returned an entitlement this app cannot verify.")?;
    store_entitlement(&state, &token).await;
    Ok(json!({ "ok": true, "state": verified }))
}

/// Re-check the entitlement. Costs the server no write, so this can run on every launch.
#[tauri::command]
pub async fn subs_refresh(state: State<'_, AppState>) -> Res<Value> {
    let settings = settings_of(&state).await;
    let base = base_of(&settings);
    let email = current(&state).await
        .and_then(|p| p["email"].as_str().map(|s| s.to_string()))
        .ok_or("Not signed in.")?;
    let r = http()?.post(format!("{base}/v1/entitlement"))
        .json(&json!({ "email": email })).send().await.map_err(e)?;
    let parsed: Value = r.json().await.map_err(|_| "unreadable answer".to_string())?;
    if parsed["ok"].as_bool() != Some(true) {
        return Err(parsed["error"].as_str().unwrap_or("could not refresh").to_string());
    }
    let token = parsed["entitlement"].as_str().unwrap_or("").to_string();
    let verified = verify_entitlement(&token, SUBS_PUBLIC_KEY, chrono::Utc::now().timestamp(), GRACE_DAYS)
        .ok_or("The refreshed entitlement does not verify.")?;
    store_entitlement(&state, &token).await;
    Ok(json!({ "ok": true, "state": verified }))
}

/// What the app is allowed to do, and what to tell the user.
///
/// Answers from the cached entitlement, so it works offline and costs nothing to call often.
#[tauri::command]
pub async fn subs_status(state: State<'_, AppState>) -> Res<Value> {
    let ent = current(&state).await;
    let settings = settings_of(&state).await;
    let days_left = ent.as_ref().and_then(|p| {
        let end = p["trial_ends"].as_str().filter(|s| !s.is_empty())
            .or_else(|| p["period_ends"].as_str().filter(|s| !s.is_empty()))?;
        let then = chrono::DateTime::parse_from_rfc3339(end).ok()?;
        Some(((then.with_timezone(&chrono::Utc) - chrono::Utc::now()).num_hours() as f64 / 24.0).ceil() as i64)
    });
    Ok(json!({
        "signed_in": ent.is_some(),
        "status": ent.as_ref().and_then(|p| p["status"].as_str()).unwrap_or("none"),
        "plan": ent.as_ref().and_then(|p| p["plan"].as_str()).unwrap_or(""),
        "email": ent.as_ref().and_then(|p| p["email"].as_str()).unwrap_or(""),
        "username": ent.as_ref().and_then(|p| p["username"].as_str()).unwrap_or(""),
        "features": ent.as_ref().map(|p| p["features"].clone()).unwrap_or(json!({})),
        "days_left": days_left,
        // True when the server has not been reachable but the entitlement is still inside its grace
        // window. Worth showing: it explains why things work now and might not next week.
        "stale": ent.as_ref().and_then(|p| p["stale"].as_bool()).unwrap_or(false),
        "base": base_of(&settings),
        "checked_at": settings["subs_checked_at"],
        "analytics_opt_in": settings["analytics_opt_in"].as_bool().unwrap_or(false),
    }))
}

#[tauri::command]
pub async fn subs_sign_out(state: State<'_, AppState>) -> Res<Value> {
    // The entitlement goes; the encrypted cache stays. Signing out must not destroy work, and signing
    // back in with the same account opens it again.
    state.db.collection::<Document>("settings")
        .update_one(doc! { "_id": "singleton" },
                    doc! { "$set": { "subs_entitlement": "" } }).await.map_err(e)?;
    Ok(json!({ "signed_in": false }))
}

/// Prices and plans, for the subscribe screen.
#[tauri::command]
pub async fn subs_pricing(state: State<'_, AppState>) -> Res<Value> {
    let base = base_of(&settings_of(&state).await);
    let r = http()?.get(format!("{base}/v1/pricing")).send().await.map_err(e)?;
    r.json::<Value>().await.map_err(|_| "The pricing could not be read.".to_string())
}

#[tauri::command]
pub async fn subs_terms(state: State<'_, AppState>) -> Res<Value> {
    let base = base_of(&settings_of(&state).await);
    let r = http()?.get(format!("{base}/v1/terms")).send().await.map_err(e)?;
    r.json::<Value>().await.map_err(|_| "The terms could not be read.".to_string())
}

#[tauri::command]
pub async fn subs_redeem(state: State<'_, AppState>, code: String) -> Res<Value> {
    let base = base_of(&settings_of(&state).await);
    let email = current(&state).await
        .and_then(|p| p["email"].as_str().map(|s| s.to_string()))
        .ok_or("Sign in first, then redeem.")?;
    let r = http()?.post(format!("{base}/v1/redeem"))
        .json(&json!({ "code": code, "email": email })).send().await.map_err(e)?;
    let parsed: Value = r.json().await.map_err(|_| "unreadable answer".to_string())?;
    if parsed["ok"].as_bool() != Some(true) {
        return Err(parsed["error"].as_str().unwrap_or("that code did not work").to_string());
    }
    if let Some(token) = parsed["entitlement"].as_str() {
        if verify_entitlement(token, SUBS_PUBLIC_KEY, chrono::Utc::now().timestamp(), GRACE_DAYS).is_some() {
            store_entitlement(&state, token).await;
        }
    }
    Ok(parsed)
}

/// The user's own share code, for "tell a friend".
#[tauri::command]
pub async fn subs_referral(state: State<'_, AppState>) -> Res<Value> {
    let base = base_of(&settings_of(&state).await);
    let email = current(&state).await
        .and_then(|p| p["email"].as_str().map(|s| s.to_string()))
        .ok_or("Sign in first.")?;
    let r = http()?.post(format!("{base}/v1/referral"))
        .json(&json!({ "email": email })).send().await.map_err(e)?;
    r.json::<Value>().await.map_err(|_| "unreadable answer".to_string())
}

// ── Reports and events ─────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct ReportRequest {
    /// "error" | "bug" | "idea" | "praise" | "confusion" | "money"
    pub kind: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub stack: Option<String>,
    #[serde(default)]
    pub fingerprint: Option<String>,
}

/// Send a bug report or a piece of feedback.
///
/// Errors are sent without asking, because a crash nobody reports is a crash nobody fixes — and the
/// terms say so plainly rather than burying it. Anything with a person's own words in it is only ever
/// sent because they pressed send.
#[tauri::command]
pub async fn send_report(state: State<'_, AppState>, payload: ReportRequest) -> Res<Value> {
    let settings = settings_of(&state).await;
    let base = base_of(&settings);
    let ent = current(&state).await;
    let body = json!({
        "kind": payload.kind,
        "title": payload.title.unwrap_or_default(),
        "message": payload.message.unwrap_or_default(),
        "comment": payload.comment.unwrap_or_default(),
        "stack": payload.stack.unwrap_or_default(),
        "fingerprint": payload.fingerprint.unwrap_or_default(),
        "version": env!("CARGO_PKG_VERSION"),
        "platform": std::env::consts::OS,
        "email": ent.as_ref().and_then(|p| p["email"].as_str()).unwrap_or(""),
        "device_id": device_id(&state).await,
    });
    let r = http()?.post(format!("{base}/v1/reports")).json(&body).send().await.map_err(e)?;
    r.json::<Value>().await.map_err(|_| "the report could not be sent".to_string())
}

#[derive(serde::Deserialize)]
pub struct EventBatch {
    /// `[{ name, n? }]`, already batched by the client.
    pub events: Vec<Value>,
    #[serde(default)]
    pub session: Option<String>,
}

/// Send a batch of funnel events.
///
/// Batched because the free tier allows a thousand KV writes a day and one write per click would spend
/// that on a handful of users. First-party: this goes to the app's own server and nowhere else.
#[tauri::command]
pub async fn track_events(state: State<'_, AppState>, payload: EventBatch) -> Res<Value> {
    let settings = settings_of(&state).await;
    let ent = current(&state).await;
    let status = ent.as_ref().and_then(|p| p["status"].as_str()).unwrap_or("none").to_string();

    // Subscribers are only measured if they said yes. During the trial it is part of the deal, and the
    // terms say so — but once somebody is paying, being studied is their choice.
    if status == "active" || status == "lifetime" {
        if settings["analytics_opt_in"].as_bool() != Some(true) {
            return Ok(json!({ "ok": true, "skipped": "not opted in" }));
        }
    }
    let base = base_of(&settings);
    let body = json!({
        "events": payload.events,
        "session": payload.session.unwrap_or_default(),
        "status": status,
        "plan": ent.as_ref().and_then(|p| p["plan"].as_str()).unwrap_or(""),
        "version": env!("CARGO_PKG_VERSION"),
        "platform": std::env::consts::OS,
    });
    let r = http()?.post(format!("{base}/v1/events")).json(&body).send().await.map_err(e)?;
    r.json::<Value>().await.map_err(|_| "events could not be sent".to_string())
}

/// The devices this account is signed in on.
///
/// Approximate city rather than IP: enough to recognise your own laptop, and an IP address stored on a
/// server is a liability rather than a feature.
#[tauri::command]
pub async fn list_sessions(state: State<'_, AppState>) -> Res<Value> {
    let settings = settings_of(&state).await;
    let base = base_of(&settings);
    let email = current(&state).await
        .and_then(|p| p["email"].as_str().map(|s| s.to_string()))
        .ok_or("Not signed in.")?;
    let r = http()?.post(format!("{base}/v1/sessions"))
        .json(&json!({ "email": email, "device_id": device_id(&state).await }))
        .send().await.map_err(e)?;
    r.json::<Value>().await.map_err(|_| "The device list could not be read.".to_string())
}

/// Sign a specific device out, by the short id shown in the list.
#[tauri::command]
pub async fn end_session(state: State<'_, AppState>, device: String) -> Res<Value> {
    let settings = settings_of(&state).await;
    let base = base_of(&settings);
    let email = current(&state).await
        .and_then(|p| p["email"].as_str().map(|s| s.to_string()))
        .ok_or("Not signed in.")?;
    let r = http()?.post(format!("{base}/v1/sessions"))
        .json(&json!({ "email": email, "device_id": device_id(&state).await, "end": device }))
        .send().await.map_err(e)?;
    r.json::<Value>().await.map_err(|_| "That device could not be signed out.".to_string())
}

/// Sign every other device out, keeping this one.
#[tauri::command]
pub async fn end_other_sessions(state: State<'_, AppState>) -> Res<Value> {
    let settings = settings_of(&state).await;
    let base = base_of(&settings);
    let email = current(&state).await
        .and_then(|p| p["email"].as_str().map(|s| s.to_string()))
        .ok_or("Not signed in.")?;
    let r = http()?.post(format!("{base}/v1/sessions"))
        .json(&json!({ "email": email, "device_id": device_id(&state).await, "end": "others" }))
        .send().await.map_err(e)?;
    r.json::<Value>().await.map_err(|_| "The other devices could not be signed out.".to_string())
}

/// Called by every gated screen so the interface and the backend can never disagree about what is on.
#[tauri::command]
pub async fn subs_can(state: State<'_, AppState>, feature: String) -> Res<Value> {
    let ent = current(&state).await;
    let ok = ent.as_ref().map(|p| allows(p, &feature)).unwrap_or(false);
    Ok(json!({
        "feature": feature, "allowed": ok,
        "reason": if ok { String::new() } else { refusal(ent.as_ref(), &feature) },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    /// Mint a token the way the server does, so the verifier is tested against the real shape.
    fn issue(payload: Value, key: &SigningKey) -> String {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let body = b64.encode(payload.to_string().as_bytes());
        let sig = key.sign(body.as_bytes());
        format!("{body}.{}", b64.encode(sig.to_bytes()))
    }

    fn keypair() -> (SigningKey, String) {
        use base64::Engine;
        // A fixed seed: the test is about the verifier, not about randomness.
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let pub_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(key.verifying_key().to_bytes());
        (key, pub_b64)
    }

    fn sample(exp: i64) -> Value {
        json!({
            "sub": "u1", "email": "a@b.c", "status": "trial",
            "features": { "read": true, "generate": true, "export": false },
            "exp": exp, "vault": "abc",
        })
    }

    #[test]
    fn a_valid_entitlement_verifies_and_a_tampered_one_does_not() {
        let (key, pub_b64) = keypair();
        let token = issue(sample(2_000_000_000), &key);
        let ok = verify_entitlement(&token, &pub_b64, 1_000_000_000, 7).expect("verifies");
        assert_eq!(ok["email"], json!("a@b.c"));

        // Editing the payload — the obvious attack, since the token is sitting in the local store as
        // readable text — breaks the signature.
        let (body, sig) = token.split_once('.').unwrap();
        use base64::Engine;
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let mut payload: Value = serde_json::from_slice(&b64.decode(body).unwrap()).unwrap();
        payload["features"]["export"] = json!(true);
        payload["status"] = json!("lifetime");
        let forged = format!("{}.{sig}", b64.encode(payload.to_string().as_bytes()));
        assert!(verify_entitlement(&forged, &pub_b64, 1_000_000_000, 7).is_none(),
                "a rewritten payload must not verify");
    }

    #[test]
    fn a_token_signed_by_somebody_else_is_rejected() {
        // Standing up a fake server is easier than forging a signature, so this is the attack that
        // matters most.
        let (_, real_pub) = keypair();
        let other = SigningKey::from_bytes(&[9u8; 32]);
        let token = issue(sample(2_000_000_000), &other);
        assert!(verify_entitlement(&token, &real_pub, 1_000_000_000, 7).is_none());
    }

    #[test]
    fn an_expired_entitlement_still_works_through_the_grace_window_and_says_it_is_stale() {
        // Somebody on a plane with a paid subscription must not lose their work.
        let (key, pub_b64) = keypair();
        let exp = 1_000_000_000;
        let token = issue(sample(exp), &key);

        let fresh = verify_entitlement(&token, &pub_b64, exp - 10, 7).expect("fresh");
        assert_eq!(fresh["stale"], json!(false));

        let stale = verify_entitlement(&token, &pub_b64, exp + 3 * 86400, 7).expect("still inside grace");
        assert_eq!(stale["stale"], json!(true), "usable, but the UI should say why");

        // Past the grace window it is gone. A week is far too short to be a way of avoiding payment.
        assert!(verify_entitlement(&token, &pub_b64, exp + 8 * 86400, 7).is_none());
    }

    #[test]
    fn garbage_is_rejected_rather_than_half_believed() {
        let (_, pub_b64) = keypair();
        for junk in ["", ".", "abc", "abc.def", "no-dot-at-all"] {
            assert!(verify_entitlement(junk, &pub_b64, 1, 7).is_none(), "accepted {junk:?}");
        }
    }

    #[test]
    fn a_missing_feature_flag_means_no() {
        // An old client meeting a new server must not read "I do not know that word" as "allowed".
        let p = json!({ "features": { "read": true } });
        assert!(allows(&p, "read"));
        assert!(!allows(&p, "export"));
        assert!(!allows(&json!({}), "read"));
    }

    #[test]
    fn a_refusal_talks_about_the_persons_situation() {
        let trial = json!({ "status": "trial" });
        let msg = refusal(Some(&trial), "export");
        assert!(msg.contains("nothing is lost"), "{msg}");
        assert!(!msg.contains("false") && !msg.contains("feature flag"), "{msg}");

        // Expired must reassure that the work still exists — that is the whole reason the cache is kept.
        let expired = json!({ "status": "expired" });
        assert!(refusal(Some(&expired), "generate").contains("still here"));
        // And no entitlement at all should invite, not scold.
        assert!(refusal(None, "generate").contains("free week"));
    }

    #[test]
    fn every_gated_capability_is_in_the_feature_list() {
        // The list is what the UI iterates to explain what a subscription buys; a capability missing
        // from it is one the user is never told about.
        for f in ["read", "generate", "publish", "export", "save_copies", "remote_sync"] {
            assert!(FEATURES.contains(&f), "{f} is gated but not listed");
        }
    }
}
