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

/// A public key the server may have signed with, and how long this build keeps accepting it.
pub struct SubsKey {
    /// Ed25519 verifying key, base64url, 32 raw bytes.
    pub b64: &'static str,
    /// `None` for the key in service. `Some(unix)` for a retired one: still accepted until then, so a
    /// rotation does not lock out every user at the instant the server switches over.
    pub accept_until: Option<i64>,
}

/// The keys an entitlement may be signed with, in service first.
///
/// Compiled in rather than configured, because a public key in a config file is a public key an
/// attacker can replace with their own.
///
/// A *list* rather than a constant because rotating a single key is not an operation anyone can
/// safely perform: entitlements last about a day and are cached on disk for a week's grace, so the
/// moment the server starts signing with a new key, every token already issued stops verifying and
/// every user who is offline, or simply has not refreshed yet, is locked out of work they paid for.
/// Overlapping the old key for a fortnight makes the rotation invisible instead.
///
/// **To rotate** (see `docs/SECURITY-KEY-ROTATION.md` for the whole procedure):
///   1. `python3 server/deploy.py --rotate-key` — mints a new pair, uploads the private half as the
///      Worker secret, prints the public half. The private key is never printed and never written
///      anywhere but the gitignored deploy state.
///   2. Put the printed key at the TOP of this list with `accept_until: None`, and give the entry
///      that was on top an `accept_until` a fortnight out.
///   3. Ship a release. After that date the old key stops being accepted and its entry can go.
pub const SUBS_PUBLIC_KEYS: &[SubsKey] = &[
    // ⚠️ COMPROMISED, and still the key in service. Its private half was committed to this
    // repository in v0.88.0 (`server/.deploy-state.json`, removed again in `feee638`) and is still
    // reachable in the history, together with the admin token. Anyone who has ever cloned this repo
    // can mint themselves a lifetime entitlement that this app verifies.
    //
    // Replacing it is two halves that must happen on the same machine, which is why it cannot be
    // done for somebody: `mint_signing_key` (Settings → Subscription, or `deploy.py --mint-only`)
    // makes the pair locally and keeps the private half in the vault, and the public half goes here
    // above this line with an `accept_until` set on this one. A public key whose private half lives
    // on a machine that no longer exists is worse than a compromised one — nothing can sign for it,
    // so every entitlement stops verifying. See `docs/SECURITY-KEY-ROTATION.md`.
    SubsKey { b64: "9-9bAxvvDtG98OKRR8xn3OeOHk0S0aruy4UA8FUmQwY", accept_until: None },
];

/// How long a stale entitlement keeps working when the server cannot be reached.
const GRACE_DAYS: i64 = 7;

/// Every capability the app gates. Named rather than derived from a plan, so adding one later does not
/// need the server and the client to agree about what a plan means.
pub const FEATURES: &[&str] = &["read", "generate", "publish", "export", "save_copies", "remote_sync"];

fn b64u_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Where the newly minted private half is kept: the app's own encrypted vault, never the repo.
pub const SIGNING_KEY_VAULT_SLOT: &str = "subscription_signing_key";

/// The DER header of an Ed25519 PKCS#8 private key, which is the same sixteen bytes every time.
///
/// `SEQUENCE { INTEGER 0, SEQUENCE { OID 1.3.101.112 }, OCTET STRING { OCTET STRING (32) } }` — the
/// only variable part of the structure is the seed, so the whole encoding is this prefix followed by
/// it. Checked against `openssl genpkey -algorithm ed25519 -outform DER`, byte for byte, in
/// `the_pkcs8_wrapper_is_the_one_openssl_writes`.
const PKCS8_ED25519_PREFIX: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06,
    0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

/// A raw 32-byte seed wrapped as PKCS#8, which is what WebCrypto's `importKey` wants.
///
/// Both encodings are needed and neither side accepts the other's: the app verifies with the raw
/// public bytes, and the Worker imports the private half as PKCS#8. Minting that returned only the
/// seed would hand somebody a key their own server cannot load — which they would discover at the
/// moment entitlements stopped being issued.
pub fn pkcs8_of_seed(seed: &[u8]) -> Option<Vec<u8>> {
    if seed.len() != 32 { return None; }
    let mut out = PKCS8_ED25519_PREFIX.to_vec();
    out.extend_from_slice(seed);
    Some(out)
}

/// The 32-byte seed inside a PKCS#8 Ed25519 key, or the input if it is already a bare seed.
///
/// Accepting both is not sloppiness: a person pasting "the private key" has one or the other
/// depending on where it came from, and guessing wrong silently produces a key that verifies nothing.
pub fn seed_of(private_b64: &str) -> Option<[u8; 32]> {
    let bytes = b64u_decode(private_b64).or_else(|| {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.decode(private_b64.trim()).ok()
    })?;
    match bytes.len() {
        32 => bytes.try_into().ok(),
        48 if bytes.starts_with(&PKCS8_ED25519_PREFIX) => bytes[16..].try_into().ok(),
        _ => None,
    }
}

/// Mint an Ed25519 signing pair on this machine.
///
/// Pure and local — no network, nothing to install, no Cloudflare credential. That was the thing
/// standing in the way of rotating the compromised key, and it turned out not to be true: minting
/// was welded to `deploy.py`'s upload step and so inherited its requirements. It does not need them.
///
/// The private half goes into the vault (XChaCha20-Poly1305, `vault.rs`) and is returned once, here,
/// so it can be pasted into whatever signs entitlements. It is never written to the repository, and
/// the caller is expected to show it once rather than store it again.
///
/// The pair is **self-tested before it is returned**: a token is signed with the private half and
/// verified with the public half through the app's own `verify_entitlement`. A rotation that shipped
/// a mismatched pair would lock out every user, and it would do so silently — the app would simply
/// stop believing tokens — so the check is here rather than left to whoever runs it.
pub fn mint_pair() -> Result<(String, String), String> {
    use ed25519_dalek::{Signer, SigningKey};

    // The OS entropy source the rest of the app already uses for the vault's own keys.
    let mut seed = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut seed);
    let signing = SigningKey::from_bytes(&seed);
    let public_b64 = b64u_encode(signing.verifying_key().as_bytes());
    let private_b64 = b64u_encode(&seed);

    // Sign something that looks like a real entitlement and check it comes back verified. The
    // expiry is far enough out that the grace window plays no part in the answer.
    let now = chrono::Utc::now().timestamp();
    let payload = serde_json::json!({ "exp": now + 86_400, "features": ["read"], "plan": "selftest" });
    let body = b64u_encode(serde_json::to_string(&payload).map_err(|e| e.to_string())?.as_bytes());
    let token = format!("{body}.{}", b64u_encode(&signing.sign(body.as_bytes()).to_bytes()));
    if verify_entitlement(&token, &public_b64, now, 0).is_none() {
        return Err("the minted pair did not verify against itself — nothing was saved".into());
    }
    Ok((public_b64, private_b64))
}

/// Mint a key, keep the private half, and hand back what has to be published.
#[tauri::command]
pub async fn mint_signing_key() -> Result<Value, String> {
    let (public_b64, private_b64) = mint_pair()?;
    crate::vault::put(SIGNING_KEY_VAULT_SLOT, &private_b64)?;

    // A fortnight, which is the overlap the retiring key needs: a token issued a minute before the
    // switch is valid for its full term, and refusing it would lock out exactly the people who were
    // using the app when the rotation happened.
    let accept_until = chrono::Utc::now().timestamp() + 14 * 86_400;
    let pkcs8 = pkcs8_of_seed(&b64u_decode(&private_b64).unwrap_or_default())
        .map(|d| b64u_encode(&d))
        .unwrap_or_default();

    Ok(serde_json::json!({
        "public_key": public_b64,
        // Returned once. The caller shows it and does not keep it — it is already in the vault.
        "private_key": private_b64,
        // The same key, wrapped for WebCrypto's importKey — which is what the Worker that signs
        // entitlements actually takes. Returning only the seed would hand somebody a key their own
        // server cannot load, discovered at the moment entitlements stopped being issued.
        "private_key_pkcs8": pkcs8,
        "vault_slot": SIGNING_KEY_VAULT_SLOT,
        "accept_until": accept_until,
        // The exact edit, rather than a description of it: this is the step where a typo silently
        // ships a key nothing can sign for.
        "code": format!(
            "SubsKey {{ b64: \"{public_b64}\", accept_until: None }},\n\
             SubsKey {{ b64: \"{}\", accept_until: Some({accept_until}) }},",
            SUBS_PUBLIC_KEYS.first().map(|k| k.b64).unwrap_or(""),
        ),
        "retiring": SUBS_PUBLIC_KEYS.first().map(|k| k.b64).unwrap_or(""),
    }))
}

/// The private half of the key minted here, for whatever signs entitlements. `None` if none was.
#[tauri::command]
pub async fn signing_key_status() -> Result<Value, String> {
    let held = crate::vault::get(SIGNING_KEY_VAULT_SLOT)?;
    Ok(serde_json::json!({
        "minted": held.is_some(),
        // Whether the key this build trusts is one this machine can sign for. A "yes" here is what
        // makes a rotation finished rather than half-done.
        "in_service": match held.as_deref() {
            Some(private) => public_of(private).is_some_and(|pubk|
                SUBS_PUBLIC_KEYS.iter().any(|k| k.b64 == pubk)),
            None => false,
        },
        "compromised": SUBS_PUBLIC_KEYS.iter().any(|k| k.b64 == COMPROMISED_KEY),
    }))
}

/// The key committed in v0.88.0. Named so the app can say out loud whether it is still trusted.
pub const COMPROMISED_KEY: &str = "9-9bAxvvDtG98OKRR8xn3OeOHk0S0aruy4UA8FUmQwY";

/// The public half of a stored private key, so a build can be checked against what it can sign for.
pub fn public_of(private_b64: &str) -> Option<String> {
    use ed25519_dalek::SigningKey;
    let seed = seed_of(private_b64)?;
    Some(b64u_encode(SigningKey::from_bytes(&seed).verifying_key().as_bytes()))
}

fn b64u_decode(s: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s.trim()).ok()
}

/// Check a token against every key this build accepts, honouring each key's retirement.
///
/// Tries them in order, so the key in service is the one that normally answers and a retired key
/// costs an extra signature check only for a token old enough to need it. A key past its
/// `accept_until` is not tried at all: that date is the whole point of listing it.
pub fn verify_with_keys(token: &str, keys: &[SubsKey], now_unix: i64, grace_days: i64) -> Option<Value> {
    keys.iter()
        .filter(|k| k.accept_until.is_none_or(|until| now_unix <= until))
        .find_map(|k| verify_entitlement(token, k.b64, now_unix, grace_days))
}

/// Check the signature and the expiry against ONE key, and return the payload.
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
    verify_with_keys(token, SUBS_PUBLIC_KEYS, chrono::Utc::now().timestamp(), GRACE_DAYS)
}

/// The account this install belongs to, taken from the stored token WITHOUT verifying it.
///
/// Only ever used to ask the server "what is this account entitled to now?" — never to grant
/// anything. That distinction is what makes reading an expired token safe here, and reading it is
/// the whole point: `current` returns `None` once a token is past its grace window, and
/// `subs_refresh` used that as its source of the email. So an install that sat closed for longer
/// than the grace period could no longer refresh, because refreshing required a valid entitlement,
/// which is exactly the thing it no longer had. It reported "Not signed in", the frontend swallowed
/// it, and the app stayed locked with a paid licence sitting on the server.
///
/// Observed on 2026-08-14: a token that expired on the 6th, one day past its seven-day grace, on an
/// account the server was returning `status: lifetime` for the entire time.
///
/// The answer it produces is checked properly — `subs_refresh` verifies the entitlement that comes
/// back before storing it — so the worst a tampered email achieves is a correctly-signed entitlement
/// for somebody else's account, which is no more than signing in as them would give.
async fn account_email(state: &AppState) -> Option<String> {
    if let Some(p) = current(state).await {
        if let Some(e) = p["email"].as_str().filter(|e| !e.is_empty()) {
            return Some(e.to_string());
        }
    }
    let settings = settings_of(state).await;
    let token = settings["subs_entitlement"].as_str().unwrap_or("");
    let body = token.split('.').next().filter(|b| !b.is_empty())?;
    let decoded = b64u_decode(body)?;
    let parsed: Value = serde_json::from_slice(&decoded).ok()?;
    parsed["email"].as_str().filter(|e| !e.is_empty()).map(|e| e.to_string())
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

/// Whether project data is sealed on disk. On by default; a user can turn it off, and the files are
/// rewritten in plain text when they do, so it is a reversible choice rather than a trap.
async fn sealing_wanted(state: &AppState) -> bool {
    settings_of(state).await["cache_sealing"].as_bool().unwrap_or(true)
}

/// Hand the store the key derived from the current entitlement — or take it away.
///
/// Called at startup and after every change to the entitlement, so the store's idea of who is
/// signed in never lags the app's. Signing out locks the cache without destroying it: the files
/// stay, and signing back in with the same account opens them.
pub async fn apply_cache_key(state: &AppState) {
    let material = cache_key_material(state).await;
    let enable = sealing_wanted(state).await;
    crate::store::set_cache_key(material.as_deref(), enable);
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
pub struct SubsSignInRequest {
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
pub async fn subs_sign_in(state: State<'_, AppState>, payload: SubsSignInRequest) -> Res<Value> {
    redeem_id_token(&state, &payload.id_token,
                    payload.username.as_deref(), payload.referral.as_deref()).await
}

/// Sign in with Google in one call: open the consent screen, take the ID token it returns, and hand
/// that to the account server.
///
/// The whole point is that it is one call. Split across "get a token" and "send the token" it was
/// possible — and was in fact the case — for the second half to exist while the first did not, which
/// presents to the user as a sign-in button that does nothing.
#[tauri::command]
pub async fn subs_sign_in_google(
    state: State<'_, AppState>,
    oauth_client_id: Option<String>,
    username: Option<String>,
) -> Res<Value> {
    let who = crate::commands::oauth::google_id_token(&state.db, oauth_client_id.as_deref()).await?;
    let id_token = who["id_token"].as_str().unwrap_or("");
    let mut out = redeem_id_token(&state, id_token, username.as_deref(), None).await?;
    out["email"] = who["email"].clone();
    out["name"] = who["name"].clone();
    Ok(out)
}

/// Give the account server a Google ID token and store the entitlement it returns.
///
/// Also called after a YouTube connect, which produces the very same kind of token — so connecting a
/// channel signs you in rather than leaving you connected to YouTube but locked out of the app.
pub async fn redeem_id_token(
    state: &AppState,
    id_token: &str,
    username: Option<&str>,
    referral: Option<&str>,
) -> Res<Value> {
    if id_token.trim().is_empty() {
        return Err("The Google sign-in returned no ID token.".into());
    }
    let settings = settings_of(state).await;
    let base = base_of(&settings);
    let body = json!({
        "id_token": id_token,
        "username": username.unwrap_or_default(),
        "referral": referral.unwrap_or_default(),
        "device_id": device_id(state).await,
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
    let verified = verify_with_keys(&token, SUBS_PUBLIC_KEYS, chrono::Utc::now().timestamp(), GRACE_DAYS)
        .ok_or("The account server returned an entitlement this app cannot verify.")?;
    store_entitlement(state, &token).await;
    apply_cache_key(state).await;
    Ok(json!({ "ok": true, "state": verified }))
}

/// Sign in from a token another flow already obtained, but only if nobody is signed in.
///
/// Best-effort on purpose: this rides along with connecting YouTube, and a failure to reach the
/// account server must not turn a successful channel connection into an error. It returns whether it
/// did anything so the caller can say so.
pub async fn sign_in_alongside(state: &AppState, id_token: &str) -> bool {
    if id_token.trim().is_empty() || current(state).await.is_some() { return false; }
    redeem_id_token(state, id_token, None, None).await.is_ok()
}

/// Re-check the entitlement. Costs the server no write, so this can run on every launch.
#[tauri::command]
pub async fn subs_refresh(state: State<'_, AppState>) -> Res<Value> {
    let settings = settings_of(&state).await;
    let base = base_of(&settings);
    // Deliberately not `current()`: a token past its grace window is exactly when a refresh is most
    // needed, and reading the email from it is what breaks that deadlock. See `account_email`.
    let email = account_email(&state).await.ok_or("Not signed in.")?;
    let r = http()?.post(format!("{base}/v1/entitlement"))
        .json(&json!({ "email": email })).send().await.map_err(e)?;
    let parsed: Value = r.json().await.map_err(|_| "unreadable answer".to_string())?;
    if parsed["ok"].as_bool() != Some(true) {
        return Err(parsed["error"].as_str().unwrap_or("could not refresh").to_string());
    }
    let token = parsed["entitlement"].as_str().unwrap_or("").to_string();
    let verified = verify_with_keys(&token, SUBS_PUBLIC_KEYS, chrono::Utc::now().timestamp(), GRACE_DAYS)
        .ok_or("The refreshed entitlement does not verify.")?;
    store_entitlement(&state, &token).await;
    apply_cache_key(&state).await;
    Ok(json!({ "ok": true, "state": verified }))
}

/// What the app is allowed to do, and what to tell the user.
///
/// Answers from the cached entitlement, so it works offline and costs nothing to call often.
#[tauri::command]
pub async fn subs_status(state: State<'_, AppState>) -> Res<Value> {
    let ent = current(&state).await;
    let settings = settings_of(&state).await;
    // Which clock is running depends on the status, and for a lifetime licence none is.
    //
    // This used to prefer `trial_ends` over `period_ends` unconditionally. An upgraded account keeps
    // the `trial_ends` it had before the upgrade, so a lifetime licence counted down a trial that
    // had already finished — the Account page showed "2 days left on this period" directly beneath
    // a "yours for this version" badge. A already-elapsed date also produced a negative count.
    let days_left = ent.as_ref().and_then(|p| {
        let end = match p["status"].as_str().unwrap_or("") {
            "lifetime" => return None,          // nothing to count down
            "trial" => p["trial_ends"].as_str().filter(|s| !s.is_empty())?,
            _ => p["period_ends"].as_str().filter(|s| !s.is_empty())?,
        };
        let then = chrono::DateTime::parse_from_rfc3339(end).ok()?;
        let days = ((then.with_timezone(&chrono::Utc) - chrono::Utc::now()).num_hours() as f64 / 24.0).ceil() as i64;
        // A period that has already run out is reported by `status`, not as a negative countdown.
        if days < 0 { None } else { Some(days) }
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
        "cache_sealed": crate::store::cache_sealing(),
        "cache_unlocked": crate::store::cache_unlocked(),
    }))
}

#[tauri::command]
pub async fn subs_sign_out(state: State<'_, AppState>) -> Res<Value> {
    // The entitlement goes; the encrypted cache stays. Signing out must not destroy work, and signing
    // back in with the same account opens it again.
    state.db.collection::<Document>("settings")
        .update_one(doc! { "_id": "singleton" },
                    doc! { "$set": { "subs_entitlement": "" } }).await.map_err(e)?;
    apply_cache_key(&state).await;
    Ok(json!({ "signed_in": false }))
}

/// Turn sealing on or off, and convert what is already on disk to match.
///
/// Both directions matter. Switching on and leaving old projects in plain text would make the
/// promise false; switching off and leaving them sealed would make it a one-way door.
#[tauri::command]
pub async fn subs_seal_projects(state: State<'_, AppState>, enable: bool) -> Res<Value> {
    // Both directions need the key, and for different reasons. Sealing needs it to write; unsealing
    // needs it to *read* what it is about to write out in plain text. Refusing here with a sentence
    // beats a report full of per-file failures that all say the same thing.
    if cache_key_material(&state).await.is_none() {
        return Err(if enable {
            "Sign in first — the key that seals your projects comes from your account.".into()
        } else {
            "Sign in first. Unsealing has to read your projects before it can write them back              readable, and only your account can open them.".to_string()
        });
    }
    state.db.collection::<Document>("settings")
        .update_one(doc! { "_id": "singleton" }, doc! { "$set": { "cache_sealing": enable } })
        .upsert(true).await.map_err(e)?;
    // The key must be loaded before a conversion in either direction: unsealing needs it to *read*
    // what it is about to write out plainly.
    let material = cache_key_material(&state).await;
    crate::store::set_cache_key(material.as_deref(), enable);
    let report = state.db.reseal_projects(enable).await.map_err(e)?;
    Ok(report)
}

/// What state the sealed cache is in, for the Account and Data panels.
#[tauri::command]
pub async fn subs_cache_state(state: State<'_, AppState>) -> Res<Value> {
    let settings = settings_of(&state).await;
    Ok(json!({
        "sealing": settings["cache_sealing"].as_bool().unwrap_or(true),
        "unlocked": crate::store::cache_unlocked(),
        "active": crate::store::cache_sealing(),
        "explanation": if crate::store::cache_sealing() {
            "Your projects are encrypted on disk with a key derived from your account. Another \
             account — including a new free trial — cannot open them."
        } else if crate::store::cache_unlocked() {
            "Your projects are readable on disk. Sealing is switched off."
        } else {
            "Sign in to unlock your projects. Nothing has been deleted."
        },
    }))
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
        if verify_with_keys(token, SUBS_PUBLIC_KEYS, chrono::Utc::now().timestamp(), GRACE_DAYS).is_some() {
            store_entitlement(&state, token).await;
            apply_cache_key(&state).await;
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

    #[test]
    fn a_minted_pair_verifies_against_itself() {
        // The whole point of minting locally: no network, no credential, and a pair that provably
        // works before anybody ships its public half.
        let (public, private) = mint_pair().expect("minting is a local operation");
        assert_eq!(public_of(&private).as_deref(), Some(public.as_str()));
        assert_eq!(b64u_decode(&public).unwrap().len(), 32);
        assert_eq!(b64u_decode(&private).unwrap().len(), 32);
    }

    #[test]
    fn two_mints_are_two_different_keys() {
        // A generator that returned the same key twice would be the same failure as not rotating.
        let (a, _) = mint_pair().unwrap();
        let (b, _) = mint_pair().unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn a_token_signed_with_the_new_key_verifies_and_one_from_another_key_does_not() {
        use ed25519_dalek::{Signer, SigningKey};
        let (public, private) = mint_pair().unwrap();
        let seed: [u8; 32] = b64u_decode(&private).unwrap().try_into().unwrap();
        let signing = SigningKey::from_bytes(&seed);

        let now = 1_800_000_000i64;
        let payload = serde_json::json!({ "exp": now + 3600, "features": ["export"] });
        let body = b64u_encode(serde_json::to_string(&payload).unwrap().as_bytes());
        let token = format!("{body}.{}", b64u_encode(&signing.sign(body.as_bytes()).to_bytes()));

        let ok = verify_entitlement(&token, &public, now, 0).expect("its own key verifies it");
        assert_eq!(ok["features"][0], "export");
        // And the key it replaces must not verify it, or the rotation would have changed nothing.
        let (other, _) = mint_pair().unwrap();
        assert!(verify_entitlement(&token, &other, now, 0).is_none());
    }

    #[test]
    fn the_public_half_of_a_private_key_is_derived_and_not_guessed() {
        // This is what tells somebody whether the build they are running can be signed for at all.
        let (public, private) = mint_pair().unwrap();
        assert_eq!(public_of(&private).unwrap(), public);
        // Rubbish in, nothing out — never a panic and never a plausible-looking wrong answer.
        assert!(public_of("not base64 at all !!").is_none());
        assert!(public_of("").is_none());
        assert!(public_of(&b64u_encode(&[1u8; 16])).is_none(), "a 16-byte seed is not a key");
    }

    #[test]
    fn the_pkcs8_wrapper_is_the_one_openssl_writes() {
        // Checked against a real `openssl genpkey -algorithm ed25519 -outform DER`: 48 bytes, of
        // which the first sixteen are fixed and the rest is the seed. If this drifted, the Worker's
        // importKey would reject the key and entitlements would stop being issued.
        let der = concat!("302e020100300506032b657004220420",
                          "824c9ce5c2ac34f395b7b89bbdfc69b84657bc1f408525dfa17278ddfa820847");
        let bytes: Vec<u8> = (0..der.len()).step_by(2)
            .map(|i| u8::from_str_radix(&der[i..i + 2], 16).unwrap()).collect();
        assert_eq!(bytes.len(), 48);
        assert_eq!(&bytes[..16], &PKCS8_ED25519_PREFIX);
        assert_eq!(pkcs8_of_seed(&bytes[16..]).unwrap(), bytes);
        // …and it comes back out again.
        assert_eq!(seed_of(&b64u_encode(&bytes)).unwrap().to_vec(), bytes[16..].to_vec());
    }

    #[test]
    fn a_private_key_is_read_in_whichever_encoding_it_arrives_in() {
        // Somebody pasting "the private key" has a bare seed or a PKCS#8 blob depending on where it
        // came from, and guessing wrong silently produces a key that verifies nothing.
        let (public, seed_b64) = mint_pair().unwrap();
        let der = pkcs8_of_seed(&b64u_decode(&seed_b64).unwrap()).unwrap();
        assert_eq!(public_of(&seed_b64).unwrap(), public, "a bare seed");
        assert_eq!(public_of(&b64u_encode(&der)).unwrap(), public, "the same key as PKCS#8");
        // Standard base64 too, since that is what openssl and most tools print.
        use base64::Engine;
        let std_b64 = base64::engine::general_purpose::STANDARD.encode(&der);
        assert_eq!(public_of(&std_b64).unwrap(), public, "PKCS#8 in standard base64");
        // Anything else is refused rather than half-read.
        assert!(seed_of(&b64u_encode(&[0u8; 40])).is_none());
        assert!(seed_of("nonsense").is_none());
    }

    #[test]
    fn the_compromised_key_constant_is_the_key_actually_listed() {
        // The interface says "this build still trusts the leaked key" by comparing these. If the
        // constant drifted from the list, the warning would quietly stop being true.
        assert!(SUBS_PUBLIC_KEYS.iter().any(|k| k.b64 == COMPROMISED_KEY),
                "the constant no longer matches the shipped list — has the rotation happened?");
    }
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

    // ── rotating the signing key ────────────────────────────────────────────
    //
    // The property that makes a rotation performable at all: for a fortnight, tokens signed by either
    // key verify. Without that, switching the server's key locks out every user holding a token
    // issued a minute earlier — which is why the compromised key had not been rotated.

    fn keyed(seed: u8) -> (SigningKey, String) {
        use base64::Engine;
        let key = SigningKey::from_bytes(&[seed; 32]);
        let pub_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(key.verifying_key().to_bytes());
        (key, pub_b64)
    }

    #[test]
    fn during_the_overlap_both_the_new_and_the_retired_key_verify() {
        let (old_key, old_pub) = keyed(1);
        let (new_key, new_pub) = keyed(2);
        let now = 1_000_000_000;
        let keys = [
            SubsKey { b64: Box::leak(new_pub.into_boxed_str()), accept_until: None },
            SubsKey { b64: Box::leak(old_pub.into_boxed_str()), accept_until: Some(now + 14 * 86400) },
        ];

        // A token minted before the switch still works…
        let old_token = issue(sample(2_000_000_000), &old_key);
        assert!(verify_with_keys(&old_token, &keys, now, 7).is_some(), "locked out an existing user");
        // …and so does one minted after it.
        let new_token = issue(sample(2_000_000_000), &new_key);
        assert!(verify_with_keys(&new_token, &keys, now, 7).is_some());
    }

    #[test]
    fn past_its_retirement_the_old_key_stops_being_accepted() {
        let (old_key, old_pub) = keyed(1);
        let (_, new_pub) = keyed(2);
        let retires = 1_000_000_000;
        let keys = [
            SubsKey { b64: Box::leak(new_pub.into_boxed_str()), accept_until: None },
            SubsKey { b64: Box::leak(old_pub.into_boxed_str()), accept_until: Some(retires) },
        ];
        let token = issue(sample(i64::MAX / 2), &old_key);

        assert!(verify_with_keys(&token, &keys, retires, 7).is_some(), "retired a second too early");
        assert!(verify_with_keys(&token, &keys, retires + 1, 7).is_none(),
                "a compromised key must stop verifying once its window closes, whatever the token says");
    }

    #[test]
    fn a_key_that_is_on_no_list_never_verifies_however_long_the_window() {
        let (attacker, _) = keyed(3);
        let (_, mine) = keyed(2);
        let keys = [SubsKey { b64: Box::leak(mine.into_boxed_str()), accept_until: None }];
        let forged = issue(sample(2_000_000_000), &attacker);
        assert!(verify_with_keys(&forged, &keys, 1_000_000_000, 7).is_none());
    }

    #[test]
    fn an_empty_key_list_grants_nothing() {
        // The state a build would be in if somebody removed the last entry: refuse, never allow.
        let (key, _) = keyed(1);
        let token = issue(sample(2_000_000_000), &key);
        assert!(verify_with_keys(&token, &[], 1_000_000_000, 7).is_none());
    }

    #[test]
    fn the_shipped_list_has_exactly_one_key_in_service() {
        // More than one un-retired key means two live signers, which is not a rotation but a second
        // way in. Zero means nobody can use the app at all.
        let in_service = SUBS_PUBLIC_KEYS.iter().filter(|k| k.accept_until.is_none()).count();
        assert_eq!(in_service, 1, "exactly one key may be in service");
        for k in SUBS_PUBLIC_KEYS {
            use base64::Engine;
            let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(k.b64)
                .expect("every listed key is base64url");
            assert_eq!(raw.len(), 32, "an Ed25519 verifying key is 32 bytes");
        }
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
