use crate::{
    helpers::mask_secret,
    models::{now_iso, OAuthClient, OAuthClientCreate},
    state::AppState,
};
use bson::{doc, Document};
use serde_json::Value;
use tauri::State;
use uuid::Uuid;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use url::Url;
use open;
use warp::Filter;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

/// What uploading to YouTube needs.
pub const YOUTUBE_SCOPE: &str =
    "https://www.googleapis.com/auth/youtube.upload https://www.googleapis.com/auth/youtube.readonly";

/// OpenID Connect, added on top of whatever a flow is really after.
///
/// Every Google flow asks for it, so every token exchange also returns an `id_token` — and that token
/// is exactly what the account server verifies to sign somebody in. Which means connecting YouTube
/// signs you into the app on the same trip to the browser, instead of sending you through a second
/// consent screen to prove the same thing to the same provider.
pub fn signin_scope(base: &str) -> String {
    let base = base.trim();
    if base.is_empty() { return "openid email profile".to_string(); }
    format!("openid email profile {base}")
}

/// The claims out of a Google ID token, without verifying it.
///
/// Deliberately unverified *here*: this is only used to show "signed in as …" while the flow
/// finishes. The signature that actually decides anything is checked by the account server, which is
/// the only party whose opinion grants access — a client-side check would prove nothing to it.
fn id_token_claims(id_token: &str) -> Value {
    use base64::Engine;
    let Some(body) = id_token.split('.').nth(1) else { return Value::Null };
    let padded = format!("{body}{}", "=".repeat((4 - body.len() % 4) % 4));
    base64::engine::general_purpose::URL_SAFE.decode(padded.as_bytes()).ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .unwrap_or(Value::Null)
}

pub fn bson_to_value(doc: Document) -> Value {
    let mut m = serde_json::Map::new();
    for (k, v) in doc {
        if k == "_id" { continue; }
        if let Ok(jv) = bson::from_bson::<Value>(v) { m.insert(k, jv); }
    }
    Value::Object(m)
}

// ────────────────────────────────────────────────────────────────
// Loopback redirect handling
//
// Google matches `redirect_uri` as an exact string, so the URI in the authorization request has to
// be byte-identical to what is registered in the Cloud Console. Two things used to break that:
//
//   • the stored URI was round-tripped through `Url::to_string()`, which normalises
//     `http://127.0.0.1:3335` into `http://127.0.0.1:3335/` — a different string to Google;
//   • the callback port was re-read from the actually-bound socket, so if the configured port was
//     taken the request went out with a port nobody had registered.
//
// Now the stored string is sent verbatim whenever it names a port. A URI with no explicit port
// keeps the old ephemeral-port behaviour, which is what "Desktop app" credentials want — for those
// Google ignores the loopback port entirely.
// ────────────────────────────────────────────────────────────────

/// Validate a stored redirect URI for the loopback flow.
/// Returns `(uri_as_stored, explicit_port)` — `None` port means "pick an ephemeral one".
fn loopback_target(stored: &str) -> Res<(String, Option<u16>)> {
    let stored = stored.trim().to_string();
    let parsed = Url::parse(&stored).map_err(|err| format!("redirect_uri is not a valid URL: {err}"))?;
    let host = parsed.host_str().unwrap_or("");
    if host != "127.0.0.1" && host != "localhost" {
        return Err("OAuth client redirect_uri must be a loopback address (127.0.0.1 or localhost)".into());
    }
    // `Url::port()` hides the scheme default, which is exactly what we want: only an explicitly
    // written port pins the socket.
    Ok((stored, parsed.port()))
}

/// The redirect URI to put in the authorization request and the token exchange (the same value has
/// to appear in both). Verbatim when the client named a port; otherwise the stored URI with the
/// ephemeral port that actually got bound.
fn redirect_for_request(stored: &str, explicit_port: Option<u16>, bound_port: u16) -> Res<String> {
    if explicit_port.is_some() { return Ok(stored.trim().to_string()); }
    let mut url = Url::parse(stored.trim()).map_err(e)?;
    url.set_port(Some(bound_port)).map_err(|_| "cannot set port on redirect_uri".to_string())?;
    Ok(url.to_string())
}

/// Build the authorization URL. One place for it, so the pre-flight probe and the request the browser
/// actually opens cannot drift apart — if they did, the probe would be answering about a different
/// request than the one the user makes, which is worse than no probe at all.
fn authorize_url(client_id: &str, redirect: &str, scope: &str, state_param: &str) -> String {
    format!(
        "https://accounts.google.com/o/oauth2/v2/auth?response_type=code&client_id={}&redirect_uri={}&scope={}&access_type=offline&prompt=consent&state={}",
        client_id.trim(),
        urlencoding::encode(redirect.trim()),
        urlencoding::encode(scope),
        urlencoding::encode(state_param),
    )
}

/// The other ways the same loopback address can be written.
///
/// Google compares `redirect_uri` byte for byte, so `http://127.0.0.1:3335` and
/// `http://127.0.0.1:3335/` are two different addresses to it and only one of them is registered.
/// That single character is the most common cause of `redirect_uri_mismatch`, and it is invisible in
/// the Cloud Console UI — which is why the app probes for it rather than asking the user to spot it.
fn redirect_variants(sent: &str) -> Vec<String> {
    let sent = sent.trim();
    let Ok(url) = Url::parse(sent) else { return Vec::new() };
    let Some(port) = url.port() else { return Vec::new() };

    let mut out = Vec::new();
    // 127.0.0.1 first: that is the address the callback server is actually bound to, so a spelling
    // found here can be adopted without moving the socket.
    for host in ["127.0.0.1", "localhost"] {
        for tail in ["", "/"] {
            let candidate = format!("http://{host}:{port}{tail}");
            if candidate != sent { out.push(candidate); }
        }
    }
    out
}

/// After a `redirect_uri_mismatch`, find a spelling of the same loopback address that this client
/// *does* accept.
///
/// Returns `(uri, adoptable)`. `adoptable` is true only for a 127.0.0.1 spelling — the app can switch
/// to one of those silently because the callback server is already listening there. A `localhost`
/// spelling is reported but never adopted: `localhost` can resolve to `::1`, and a redirect to a
/// socket nothing is listening on would trade a clear error for a silent 120-second hang.
async fn working_redirect(client_id: &str, scope: &str, sent: &str) -> Option<(String, bool)> {
    for candidate in redirect_variants(sent) {
        let probe = authorize_url(client_id, &candidate, scope, "probe");
        if authorize_rejection(&probe).await.is_none() {
            let adoptable = candidate.starts_with("http://127.0.0.1:");
            return Some((candidate, adoptable));
        }
    }
    None
}

/// Pre-flight a request and, on a redirect mismatch, try to rescue it.
///
/// `Ok(uri)` is the redirect URI to actually use — the one passed in, or a spelling the probe found
/// Google accepts. `Err(message)` is a refusal that no spelling fixes, already written for a human.
async fn settle_redirect(client_id: &str, scope: &str, sent: &str) -> Result<String, String> {
    let auth_url = authorize_url(client_id, sent, scope, "preflight");
    let Some((code, _)) = authorize_rejection(&auth_url).await else {
        return Ok(sent.to_string());
    };
    // Anything that is not about the redirect URI cannot be fixed by rewriting the redirect URI, so it
    // goes back as the explanation it is.
    if code != "redirect_uri_mismatch" {
        return Err(preflight_authorize(&auth_url, sent).await
            .unwrap_or_else(|| format!("Google rejected the sign-in request ({code}).")));
    }

    match working_redirect(client_id, scope, sent).await {
        Some((uri, true)) => Ok(uri),
        Some((uri, false)) => Err(format!(
            "Google rejected {sent}: redirect_uri_mismatch.\n\n\
             This client does accept {uri} — the same address written differently. Either register \
             {sent} in Google Cloud Console → APIs & Services → Credentials → your OAuth client, or \
             change this app's redirect URI to {uri} exactly.",
        )),
        // No spelling of this address works, so the client genuinely does not have it registered.
        None => Err(preflight_authorize(&auth_url, sent).await
            .unwrap_or_else(|| format!("Google rejected {sent}: redirect_uri_mismatch."))),
    }
}

/// Ask Google whether it will accept this authorization request *before* sending the user to the
/// browser.
///
/// A rejected `redirect_uri` (or an unknown client) is decided server-side, at the authorize
/// endpoint, and Google answers with a redirect to its own error page carrying a base64 `authError`
/// blob. Without this check the app opened the browser, the user hit "Access blocked", and the app
/// sat waiting on a callback that was never coming until the 120s timeout — with nothing but
/// "timed out" to show for it. Now the actual reason is reported immediately.
///
/// Returns `Some(message)` only on a *positive* rejection. Anything ambiguous — network failure,
/// an unexpected page, a real sign-in page — returns `None` so the flow proceeds as before.
///
/// `sent_redirect` is the URI this app actually put in the request. It is passed in rather than read
/// back out of Google's error blob: the blob also carries Google's own documentation link, and
/// scavenging "the first URL in there" used to pick that up, producing the advice to register
/// `https://developers.google.com/identity/protocols/oauth2/…` as a redirect URI. Nonsense, and it
/// sent people to the Cloud Console to paste a docs URL. The app knows what it sent; it should say so.
async fn preflight_authorize(auth_url: &str, sent_redirect: &str) -> Option<String> {
    let (code, _detail) = authorize_rejection(auth_url).await?;

    Some(match code.as_str() {
        "redirect_uri_mismatch" => format!(
            "Google rejected the sign-in before it started: redirect_uri_mismatch.\n\n\
             This app asked to be sent back to {sent_redirect}, and that exact address is not \
             registered on the OAuth client.\n\nFix it in Google Cloud Console → APIs & Services → \
             Credentials → your OAuth client, either:\n\
             • add {sent_redirect} under \"Authorised redirect URIs\" (copy it exactly — the trailing \
             slash counts), or\n\
             • create a \"Desktop app\" credential instead, which accepts any 127.0.0.1 port without \
             registering one.",
        ),
        "invalid_client" | "unauthorized_client" | "deleted_client" | "disabled_client" => format!(
            "Google rejected the OAuth client ({code}). Check that the Client ID belongs to a live \
             project with the YouTube Data API v3 enabled, and that it was not deleted or disabled."
        ),
        "access_denied" | "admin_policy_enforced" => format!(
            "Google blocked this client ({code}). If the consent screen is still in Testing, add \
             your Google account under \"Test users\" first."
        ),
        other => format!("Google rejected the sign-in request ({other}). {_detail}").trim().to_string(),
    })
}

/// Ask Google's authorize endpoint whether it will accept a request, and return `(code, detail)` when
/// it will not.
///
/// Split out of `preflight_authorize` because the redirect-variant probe below needs the raw verdict
/// rather than a paragraph of advice — and because "did Google refuse this, and why" is one question
/// worth having one implementation of.
async fn authorize_rejection(auth_url: &str) -> Option<(String, String)> {
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        // A plain UA and English keep the response (and the error text we surface) predictable.
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36")
        .build().ok()?;
    let r = http.get(auth_url).header("Accept-Language", "en-US,en;q=0.9").send().await.ok()?;
    let final_url = r.url().clone();
    if !final_url.path().contains("/signin/oauth/error") { return None; }

    let blob = final_url.query_pairs()
        .find(|(k, _)| k == "authError")
        .map(|(_, v)| v.to_string())
        .unwrap_or_default();
    let decoded = decode_auth_error(&blob);
    let code = decoded.first().cloned().unwrap_or_else(|| "invalid_request".to_string());
    // The prose, if any — skipping Google's own docs link, which is in every one of these blobs and
    // says nothing about this particular client.
    let detail = decoded.iter()
        .find(|s| s.contains(' ') && !s.contains("developers.google.com"))
        .cloned().unwrap_or_default();
    Some((code, detail))
}

/// Pull the readable strings out of Google's base64 `authError` payload. It is a protobuf blob, so
/// rather than modelling it, the printable runs are extracted in order: the error code comes first,
/// then the human message(s), then any echoed parameter values.
fn decode_auth_error(blob: &str) -> Vec<String> {
    use base64::Engine;
    if blob.is_empty() { return Vec::new(); }
    let padded = format!("{blob}{}", "=".repeat((4 - blob.len() % 4) % 4));
    let bytes = base64::engine::general_purpose::URL_SAFE.decode(padded.as_bytes())
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(blob.as_bytes()))
        .unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes);
    let mut out: Vec<String> = Vec::new();
    let mut run = String::new();
    for ch in text.chars() {
        // Field separators in the blob are control bytes; readable prose is everything else.
        if ch.is_control() || (ch as u32) < 0x20 || ch == '\u{fffd}' {
            if run.chars().count() >= 6 { out.push(run.trim().to_string()); }
            run.clear();
        } else {
            run.push(ch);
        }
    }
    if run.chars().count() >= 6 { out.push(run.trim().to_string()); }
    // Google prefixes some fields with a length byte that survives as a stray leading letter
    // (e.g. "mhttps://…"); trim that so URLs come out usable.
    for s in out.iter_mut() {
        if let Some(idx) = s.find("http://").or_else(|| s.find("https://")) {
            if idx == 1 { *s = s[idx..].to_string(); }
        }
    }
    out
}

// ────────────────────────────────────────────────────────────────
// OAuth Clients CRUD
// ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_oauth_clients(state: State<'_, AppState>) -> Res<Vec<Value>> {
    use futures_util::StreamExt;
    let mut cursor = state.db.collection::<Document>("oauth_clients")
        .find(doc! {}).sort(doc! { "created_at": -1 })
        .await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { out.push(mask_secret(bson_to_value(d))); }
    Ok(out)
}

#[tauri::command]
pub async fn create_oauth_client(state: State<'_, AppState>, body: OAuthClientCreate) -> Res<Value> {
    // Validate required fields before writing to DB
    if body.client_id.trim().is_empty() {
        return Err("client_id is required and cannot be empty".into());
    }
    if body.client_secret.trim().is_empty() {
        return Err("client_secret is required and cannot be empty".into());
    }
    if body.redirect_uri.trim().is_empty() {
        return Err("redirect_uri is required and cannot be empty".into());
    }
    let c = OAuthClient {
        id: Uuid::new_v4().to_string(),
        label: body.label,
        client_id: body.client_id.trim().to_string(),
        client_secret: body.client_secret.trim().to_string(),
        redirect_uri: body.redirect_uri.trim().to_string(),
        languages: body.languages,
        notes: body.notes,
        created_at: now_iso(),
    };
    let bson = bson::to_document(&c).map_err(e)?;
    state.db.collection::<Document>("oauth_clients").insert_one(bson).await.map_err(e)?;
    Ok(mask_secret(serde_json::to_value(&c).map_err(e)?))
}

#[tauri::command]
pub async fn update_oauth_client(
    state: State<'_, AppState>,
    oid: String,
    mut body: Value,
) -> Res<Value> {
    if let Some(obj) = body.as_object_mut() {
        obj.remove("id");
        obj.remove("_id");
        obj.remove("created_at");
        // ignore masked secret
        if let Some(s) = obj.get("client_secret").and_then(|v| v.as_str()) {
            if s.trim().is_empty() || s.starts_with('•') { obj.remove("client_secret"); }
        }
        // Protect client_id: skip update if empty
        if let Some(s) = obj.get("client_id").and_then(|v| v.as_str()) {
            if s.trim().is_empty() { obj.remove("client_id"); }
        }
        // Protect redirect_uri: skip update if empty
        if let Some(s) = obj.get("redirect_uri").and_then(|v| v.as_str()) {
            if s.trim().is_empty() { obj.remove("redirect_uri"); }
        }
    }
    let bson = bson::to_bson(&body).map_err(e)?;
    state.db.collection::<Document>("oauth_clients")
        .update_one(doc! { "id": &oid }, doc! { "$set": bson })
        .await.map_err(e)?;
    let doc = state.db.collection::<Document>("oauth_clients")
        .find_one(doc! { "id": &oid }).await.map_err(e)?
        .ok_or_else(|| "missing".to_string())?;
    Ok(mask_secret(bson_to_value(doc)))
}

#[tauri::command]
pub async fn delete_oauth_client(state: State<'_, AppState>, oid: String) -> Res<Value> {
    state.db.collection::<Document>("oauth_clients")
        .delete_one(doc! { "id": &oid }).await.map_err(e)?;
    state.db.collection::<Document>("channels")
        .update_many(doc! { "oauth_client_id": &oid }, doc! { "$unset": { "oauth_client_id": "" } })
        .await.map_err(e)?;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn channel_picked_client(state: State<'_, AppState>, cid: String) -> Res<Value> {
    let ch_doc = state.db.collection::<Document>("channels")
        .find_one(doc! { "id": &cid }).await.map_err(e)?
        .ok_or_else(|| "channel missing".to_string())?;
    let ch = bson_to_value(ch_doc);
    let client = crate::jobs::pick_oauth_client(
        &state.db, &ch, ch["oauth_client_id"].as_str()
    ).await.map(mask_secret);
    Ok(serde_json::json!({ "client": client }))
}

// ────────────────────────────────────────────────────────────────
// Validate OAuth client — field completeness plus a live check that
// Google will actually accept this client + redirect URI pair.
// Returns { ok, missing: [...], google_ok, google_error }
// ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn validate_oauth_client(state: State<'_, AppState>, oid: String) -> Res<Value> {
    let doc = state.db.collection::<Document>("oauth_clients")
        .find_one(doc! { "id": &oid }).await.map_err(e)?
        .ok_or_else(|| "OAuth client not found".to_string())?;
    let client = bson_to_value(doc);
    let mut missing = Vec::new();
    let cid = client["client_id"].as_str().unwrap_or("");
    if cid.trim().is_empty() { missing.push("client_id"); }
    let csec = client["client_secret"].as_str().unwrap_or("");
    if csec.trim().is_empty() || csec.starts_with('•') { missing.push("client_secret"); }
    let redirect = client["redirect_uri"].as_str().unwrap_or("");
    if redirect.trim().is_empty() { missing.push("redirect_uri"); }
    let fields_ok = missing.is_empty();

    // With the fields in place, ask Google — a registered-redirect mistake is the failure users
    // actually hit, and it is invisible to a field check. This is the same probe the sign-in flows
    // run, so "Validate" now predicts whether they will work.
    let mut google_ok = Value::Null;
    let mut google_error = Value::Null;
    // A spelling of the redirect URI that works when the stored one does not — surfaced so the panel
    // can offer the fix as a button rather than as a paragraph to follow by hand.
    let mut suggested_redirect = Value::Null;
    if fields_ok {
        let scope = signin_scope(YOUTUBE_SCOPE);
        match settle_redirect(cid, &scope, redirect.trim()).await {
            Ok(uri) => {
                google_ok = Value::Bool(true);
                if uri != redirect.trim() { suggested_redirect = Value::String(uri); }
            }
            Err(msg) => { google_ok = Value::Bool(false); google_error = Value::String(msg); }
        }
    }

    Ok(serde_json::json!({
        "ok": fields_ok && google_ok != Value::Bool(false),
        "fields_ok": fields_ok,
        "missing": missing,
        "google_ok": google_ok,
        "google_error": google_error,
        "suggested_redirect_uri": suggested_redirect,
        "id": oid,
        "label": client["label"],
    }))
}

// ────────────────────────────────────────────────────────────────
// OAuth start — build consent URL
// ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn oauth_start(
    state: State<'_, AppState>,
    cid: String,
    body: Option<Value>,
) -> Res<Value> {
    let ch_doc = state.db.collection::<Document>("channels")
        .find_one(doc! { "id": &cid }).await.map_err(e)?
        .ok_or_else(|| "channel not found".to_string())?;
    let ch = bson_to_value(ch_doc);
    let body = body.unwrap_or_default();
    let forced = body["oauth_client_id"].as_str();
    let client = crate::jobs::pick_oauth_client(&state.db, &ch, forced).await;
    let Some(client) = client else {
        return Ok(serde_json::json!({
            "url": "",
            "error": "No OAuth client configured. Add one in Channel Manager → OAuth Clients, or fill Settings → Google OAuth fields."
        }));
    };
    let cid_g    = client["client_id"].as_str().unwrap_or("");
    let redirect = client["redirect_uri"].as_str().unwrap_or("");
    let scope = "https://www.googleapis.com/auth/youtube.upload https://www.googleapis.com/auth/youtube.readonly";
    // Encode the redirect: it has to reach Google as one parameter value, unsplit by its own ':' or
    // any '&' a hand-entered URI might carry.
    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?response_type=code&client_id={cid_g}&redirect_uri={}&scope={}&access_type=offline&prompt=consent&state={cid}",
        urlencoding::encode(redirect.trim()),
        scope.replace(' ', "%20"),
    );
    state.db.collection::<Document>("channels")
        .update_one(doc! { "id": &cid },
            doc! { "$set": { "oauth_client_id": client["id"].as_str().unwrap_or("") } })
        .await.map_err(e)?;
    Ok(serde_json::json!({
        "url": url,
        "oauth_client_id": client["id"],
        "label": client["label"],
    }))
}

// ────────────────────────────────────────────────────────────────
// OAuth callback — exchange code for tokens, update channel
// In Tauri the redirect lands back in the app via a custom URI scheme
// or a local server; this command handles the code exchange itself.
// ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn oauth_callback(
    state: State<'_, AppState>,
    code: String,
    channel_state: String, // the `state` param Google sends back
    error: Option<String>,
) -> Res<Value> {
    if let Some(err) = error {
        return Err(format!("OAuth error: {err}"));
    }
    if code.is_empty() || channel_state.is_empty() {
        return Err("missing code or state".into());
    }
    let ch_doc = state.db.collection::<Document>("channels")
        .find_one(doc! { "id": &channel_state }).await.map_err(e)?
        .ok_or_else(|| format!("channel {} not found", channel_state))?;
    let ch = bson_to_value(ch_doc);
    let client = crate::jobs::pick_oauth_client(&state.db, &ch, ch["oauth_client_id"].as_str())
        .await
        .ok_or_else(|| "no OAuth client configured for this channel".to_string())?;

    let cid_g = client["client_id"].as_str().unwrap_or("").to_string();
    let csec  = client["client_secret"].as_str().unwrap_or("").to_string();
    let ruri  = client["redirect_uri"].as_str().unwrap_or("").to_string();

    let http = reqwest::Client::new();
    let tr = http.post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code.as_str()),
            ("client_id", &cid_g),
            ("client_secret", &csec),
            ("redirect_uri", &ruri),
            ("grant_type", "authorization_code"),
        ])
        .send().await.map_err(e)?;
    if !tr.status().is_success() {
        let error_body = tr.text().await.unwrap_or_default();
        let diagnostic = if error_body.contains("invalid_client") {
            "Your OAuth client_id or client_secret is invalid. Make sure you're using a 'Desktop App' type credential in Google Cloud Console.".to_string()
        } else if error_body.contains("redirect_uri_mismatch") {
            format!("Redirect URI mismatch. In Google Cloud Console, under 'Authorized redirect URIs', add: {}", ruri)
        } else if error_body.contains("invalid_grant") {
            "The authorization code expired or was already used. Try re-authorizing.".to_string()
        } else {
            format!("Token exchange failed: {}", error_body)
        };
        return Err(diagnostic);
    }
    let tokens: Value = tr.json().await.map_err(e)?;
    let refresh = tokens["refresh_token"].as_str().unwrap_or("").to_string();
    let access  = tokens["access_token"].as_str().unwrap_or("").to_string();

    let mut yt_channel_id = String::new();
    let mut subs: i64 = 0;
    let mut chan_title  = String::new();

    if !access.is_empty() {
        let yr = http.get("https://www.googleapis.com/youtube/v3/channels")
            .query(&[("part", "snippet,statistics"), ("mine", "true")])
            .bearer_auth(&access)
            .send().await;
        if let Ok(yr) = yr {
            if yr.status().is_success() {
                if let Ok(data) = yr.json::<Value>().await {
                    if let Some(items) = data["items"].as_array() {
                        if let Some(item) = items.first() {
                            yt_channel_id = item["id"].as_str().unwrap_or("").to_string();
                            chan_title     = item["snippet"]["title"].as_str().unwrap_or("").to_string();
                            subs           = item["statistics"]["subscriberCount"]
                                .as_str().unwrap_or("0").parse().unwrap_or(0);
                        }
                    }
                }
            }
        }
    }

    let mut update = doc! {
        "connected": true,
        "youtube_channel_id": &yt_channel_id,
        "subscriber_count": subs,
        "oauth_client_id": client["id"].as_str().unwrap_or(""),
    };
    if !refresh.is_empty() { update.insert("refresh_token", &refresh); }
    state.db.collection::<Document>("channels")
        .update_one(doc! { "id": &channel_state }, doc! { "$set": update })
        .await.map_err(e)?;

    Ok(serde_json::json!({
        "ok": true,
        "channel_title": chan_title,
        "youtube_channel_id": yt_channel_id,
        "label": client["label"],
    }))
}

// ────────────────────────────────────────────────────────────────
// OAuth for a SPECIFIC channel — loopback server approach
// Spins up a local HTTP server to catch the Google redirect,
// exchanges the code, fetches channel info, and updates the
// channel document. This is the recommended flow because it
// handles the full callback automatically without manual steps.
// ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn oauth_start_for_channel(
    state: State<'_, AppState>,
    cid: String,
    body: Option<Value>,
) -> Res<Value> {
    let ch_doc = state.db.collection::<Document>("channels")
        .find_one(doc! { "id": &cid }).await.map_err(e)?
        .ok_or_else(|| "channel not found".to_string())?;
    let ch = bson_to_value(ch_doc);
    let body = body.unwrap_or_default();
    let forced = body["oauth_client_id"].as_str();
    let client = crate::jobs::pick_oauth_client(&state.db, &ch, forced).await;
    let Some(client) = client else {
        return Err("No OAuth client configured. Add one in Channel Manager → OAuth Clients.".into());
    };
    let cid_g = client["client_id"].as_str().unwrap_or("").to_string();
    let csec = client["client_secret"].as_str().unwrap_or("").to_string();
    let redirect = client["redirect_uri"].as_str().unwrap_or("").to_string();

    if cid_g.is_empty() || csec.is_empty() || redirect.is_empty() {
        return Err("OAuth client missing client_id/client_secret/redirect_uri".into());
    }

    let (redirect, explicit_port) = loopback_target(&redirect)?;

    // Update channel's oauth_client_id
    state.db.collection::<Document>("channels")
        .update_one(doc! { "id": &cid },
            doc! { "$set": { "oauth_client_id": client["id"].as_str().unwrap_or("") } })
        .await.map_err(e)?;

    // Set up channel to receive the callback query params
    let (tx, mut rx) = mpsc::channel::<HashMap<String, String>>(1);
    let tx_filter = warp::any().map(move || tx.clone());

    let callback_route = warp::get()
        .and(warp::path::full())
        .and(warp::query::<HashMap<String, String>>())
        .and(tx_filter.clone())
        .and_then(|_full: warp::path::FullPath, qs: HashMap<String, String>, tx: mpsc::Sender<HashMap<String, String>>| async move {
            if qs.contains_key("code") || qs.contains_key("error") {
                let _ = tx.try_send(qs);
            }
            let body = "<html><body style='font-family:sans-serif;text-align:center;padding:60px'><h2>✓ Authentication complete</h2><p>You can close this window and return to the app.</p></body></html>";
            Ok::<_, std::convert::Infallible>(warp::reply::html(body))
        });

    // Bind the callback port — a TcpListener gives a clean error if it is taken instead of a panic.
    let port = explicit_port.unwrap_or(0);
    let bind_addr: SocketAddr = format!("127.0.0.1:{}", port).parse().map_err(e)?;
    let tcp = TcpListener::bind(bind_addr).await
        .map_err(|err| format!("Cannot bind OAuth callback port {}: {}. Another process may already be using it — restart the app and try again.", port, err))?;
    let bound_port = tcp.local_addr().map_err(e)?.port();
    let std_listener = tcp.into_std().map_err(e)?;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server = warp::serve(callback_route)
        .serve_incoming_with_graceful_shutdown(
            tokio_stream::wrappers::TcpListenerStream::new(
                TcpListener::from_std(std_listener).map_err(e)?
            ),
            async move { shutdown_rx.await.ok(); },
        );
    let server_task = tokio::task::spawn(server);

    let redirect_used_str = redirect_for_request(&redirect, explicit_port, bound_port)?;

    let scope = signin_scope(YOUTUBE_SCOPE);
    let state_param = cid.clone();

    // Catch a configuration Google will refuse anyway, instead of parking the user on an
    // "Access blocked" page and waiting out the callback timeout. A mismatch that is only a
    // different spelling of the same loopback address is corrected here rather than reported.
    let redirect_used_str = match settle_redirect(&cid_g, &scope, &redirect_used_str).await {
        Ok(uri) => uri,
        Err(rejected) => {
            let _ = shutdown_tx.send(());
            let _ = server_task.await;
            return Err(rejected);
        }
    };
    let auth_url = authorize_url(&cid_g, &redirect_used_str, &scope, &state_param);

    // Open system browser for user consent
    let _ = open::that(auth_url.clone());

    // Wait for the callback (120s timeout)
    let result = match tokio::time::timeout(std::time::Duration::from_secs(120), rx.recv()).await {
        Ok(Some(qs)) => {
            if let Some(err) = qs.get("error") {
                // Shut down server before returning error
                let _ = shutdown_tx.send(());
                let _ = server_task.await;
                return Err(format!("OAuth error: {}", err));
            }
            let code = qs.get("code").cloned().unwrap_or_default();
            let returned_state = qs.get("state").cloned().unwrap_or_default();
            if code.is_empty() || returned_state != state_param {
                // Shut down server before returning error
                let _ = shutdown_tx.send(());
                let _ = server_task.await;
                return Err("Missing code or invalid state in callback".into());
            }

            // Exchange code for tokens
            let http = reqwest::Client::new();
            let tr = http.post("https://oauth2.googleapis.com/token")
                .form(&[
                    ("code", code.as_str()),
                    ("client_id", cid_g.as_str()),
                    ("client_secret", csec.as_str()),
                    ("redirect_uri", redirect_used_str.as_str()),
                    ("grant_type", "authorization_code"),
                ])
                .send().await.map_err(e)?;
            if !tr.status().is_success() {
                let error_body = tr.text().await.unwrap_or_default();
                let diagnostic = if error_body.contains("invalid_client") {
                    "Your OAuth client_id or client_secret is invalid. Make sure you're using a 'Desktop App' type credential in Google Cloud Console.".to_string()
                } else if error_body.contains("redirect_uri_mismatch") {
                    format!("Redirect URI mismatch. In Google Cloud Console, under 'Authorized redirect URIs', add: {}", redirect_used_str)
                } else if error_body.contains("invalid_grant") {
                    "The authorization code expired or was already used. Try re-authorizing.".to_string()
                } else {
                    format!("Token exchange failed: {}", error_body)
                };
                // Shut down server before returning error
                let _ = shutdown_tx.send(());
                let _ = server_task.await;
                return Err(diagnostic);
            }
            let tokens: Value = tr.json().await.map_err(e)?;

            let refresh = tokens["refresh_token"].as_str().unwrap_or("").to_string();
            let access = tokens["access_token"].as_str().unwrap_or("").to_string();

            // Fetch channel metadata using the access token
            let mut yt_channel_id = String::new();
            let mut subs: i64 = 0;
            let mut chan_title = String::new();

            if !access.is_empty() {
                let yr = http.get("https://www.googleapis.com/youtube/v3/channels")
                    .query(&[("part", "snippet,statistics"), ("mine", "true")])
                    .bearer_auth(&access)
                    .send().await;
                if let Ok(yr) = yr {
                    if yr.status().is_success() {
                        if let Ok(data) = yr.json::<Value>().await {
                            if let Some(items) = data["items"].as_array() {
                                if let Some(item) = items.first() {
                                    yt_channel_id = item["id"].as_str().unwrap_or("").to_string();
                                    chan_title = item["snippet"]["title"].as_str().unwrap_or("").to_string();
                                    subs = item["statistics"]["subscriberCount"]
                                        .as_str().unwrap_or("0").parse().unwrap_or(0);
                                }
                            }
                        }
                    }
                }
            }

            // Update the channel document
            let mut update = doc! {
                "connected": true,
                "youtube_channel_id": &yt_channel_id,
                "subscriber_count": subs,
                "oauth_client_id": client["id"].as_str().unwrap_or(""),
            };
            if !refresh.is_empty() { update.insert("refresh_token", &refresh); }
            state.db.collection::<Document>("channels")
                .update_one(doc! { "id": &cid }, doc! { "$set": update })
                .await.map_err(e)?;

            let success_result = Ok(serde_json::json!({
                "ok": true,
                "channel_title": chan_title,
                "youtube_channel_id": yt_channel_id,
                "oauth_client_id": client["id"],
                "label": client["label"],
            }));
            
            // Shut down server gracefully and wait for it to finish
            let _ = shutdown_tx.send(());
            let _ = server_task.await;
            success_result
        }
        Ok(None) => {
            // Shut down server before returning error
            let _ = shutdown_tx.send(());
            let _ = server_task.await;
            Err("Callback receiver closed".into())
        }
        Err(_) => {
            // Shut down server before returning error
            let _ = shutdown_tx.send(());
            let _ = server_task.await;
            Err("Timed out waiting for OAuth callback (120s)".into())
        }
    };
    result
}

// ────────────────────────────────────────────────────────────────
// OAuth start using a local loopback server — opens system browser
// and performs the code exchange, then persists tokens into settings
// ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn oauth_start_loopback(
    state: State<'_, AppState>,
    oauth_client_db_id: String,
    scope: Option<String>,
) -> Res<Value> {
    // Delegate to shared helper that operates on a Database handle so startup code can reuse it
    crate::commands::oauth::perform_oauth_loopback(&state.db, &oauth_client_db_id, scope).await
}

/// Perform loopback OAuth flow using a raw Database handle. This is a reusable helper
/// so the flow can be invoked from startup tasks as well as from the UI.
pub async fn perform_oauth_loopback(db: &crate::store::Db, oauth_client_db_id: &str, scope: Option<String>) -> Res<Value> {
    // Find OAuth client by DB id
    let client_doc = db.collection::<Document>("oauth_clients")
        .find_one(doc! { "id": oauth_client_db_id }).await.map_err(e)?
        .ok_or_else(|| "OAuth client not found".to_string())?;
    let client = bson_to_value(client_doc);
    let cid = client["client_id"].as_str().unwrap_or("").to_string();
    let csec = client["client_secret"].as_str().unwrap_or("").to_string();
    let redirect = client["redirect_uri"].as_str().unwrap_or("").to_string();

    if cid.is_empty() || csec.is_empty() || redirect.is_empty() {
        return Err("OAuth client missing client_id/client_secret/redirect_uri".into());
    }

    // parse redirect URI — only allow localhost/127.0.0.1 loopback flows
    let (redirect, explicit_port) = loopback_target(&redirect)?;

    // Channel to receive the query params from the callback
    let (tx, mut rx) = mpsc::channel::<HashMap<String, String>>(1);
    let tx_filter = warp::any().map(move || tx.clone());

    // Accept any path on the local server and capture query params
    let callback_route = warp::get()
        .and(warp::path::full())
        .and(warp::query::<HashMap<String, String>>())
        .and(tx_filter.clone())
        .and_then(|_full: warp::path::FullPath, qs: HashMap<String, String>, tx: mpsc::Sender<HashMap<String, String>>| async move {
            if qs.contains_key("code") || qs.contains_key("error") {
                let _ = tx.try_send(qs);
            }
            let body = "<html><body><h2>Authentication complete — you can close this window.</h2></body></html>";
            Ok::<_, std::convert::Infallible>(warp::reply::html(body))
        });

    // Determine port to bind — use TcpListener so we get a clean error if the
    // port is already in use instead of a panic.
    let port = explicit_port.unwrap_or(0);
    let bind_addr: SocketAddr = format!("127.0.0.1:{}", port).parse().map_err(e)?;
    let tcp = TcpListener::bind(bind_addr).await
        .map_err(|err| format!("Cannot bind OAuth callback port {}: {}. Another process may already be using it — restart the app and try again.", port, err))?;
    let bound_port = tcp.local_addr().map_err(e)?.port();
    let std_listener = tcp.into_std().map_err(e)?;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server = warp::serve(callback_route)
        .serve_incoming_with_graceful_shutdown(
            tokio_stream::wrappers::TcpListenerStream::new(
                TcpListener::from_std(std_listener).map_err(e)?
            ),
            async move { shutdown_rx.await.ok(); },
        );
    let server_task = tokio::task::spawn(server);

    // The redirect URI must be identical here, in the browser request and in the token exchange.
    let redirect_used_str = redirect_for_request(&redirect, explicit_port, bound_port)?;

    let scope = signin_scope(scope.as_deref().unwrap_or(""));
    let state_param = Uuid::new_v4().to_string();

    // Everything from here to the token exchange runs inside one block so the local server is shut
    // down on *every* path out. It used to be torn down only on the happy path and the timeout:
    // a rejected consent, a bad state parameter or a failed exchange returned early and left warp
    // holding port 3335, so the next attempt died on "Cannot bind OAuth callback port 3335" — the
    // first failure quietly became permanent until the app was restarted.
    let result: Res<Value> = async {
        // Report a configuration Google will refuse up front, rather than after a 120s wait for a
        // callback that the "Access blocked" page never sends. A mismatch that is only a different
        // spelling of this same loopback address is corrected rather than reported.
        let redirect_used_str = settle_redirect(&cid, &scope, &redirect_used_str).await?;
        let auth_url = authorize_url(&cid, &redirect_used_str, &scope, &state_param);

        // Open system browser
        let _ = open::that(auth_url.clone());

        // Wait for the callback (120s)
        let qs = match tokio::time::timeout(std::time::Duration::from_secs(120), rx.recv()).await {
            Ok(Some(qs)) => qs,
            Ok(None) => return Err("Callback receiver closed".into()),
            Err(_) => return Err("Timed out waiting for OAuth callback".into()),
        };
        if let Some(err) = qs.get("error") {
            return Err(format!("OAuth error: {}", err));
        }
        let code = qs.get("code").cloned().unwrap_or_default();
        let returned_state = qs.get("state").cloned().unwrap_or_default();
        if code.is_empty() || returned_state != state_param {
            return Err("Missing code or invalid state in callback".into());
        }

        // Exchange code for tokens
        let http = reqwest::Client::new();
        let tr = http.post("https://oauth2.googleapis.com/token")
            .form(&[
                ("code", code.as_str()),
                ("client_id", cid.as_str()),
                ("client_secret", csec.as_str()),
                ("redirect_uri", redirect_used_str.as_str()),
                ("grant_type", "authorization_code"),
            ])
            .send().await.map_err(e)?;
        if !tr.status().is_success() {
            let error_body = tr.text().await.unwrap_or_default();
            // Provide helpful diagnostics based on common Google OAuth errors
            let diagnostic = if error_body.contains("invalid_client") {
                "Your OAuth client_id or client_secret is invalid. Make sure you're using a 'Desktop App' type credential in Google Cloud Console — 'Web Application' credentials do not work with local loopback redirects. Also verify the client_secret is correct (not masked).".to_string()
            } else if error_body.contains("redirect_uri_mismatch") {
                format!("Redirect URI mismatch. The registered redirect URI '{}' does not match the callback used. In Google Cloud Console, under 'Authorized redirect URIs', add: {}", redirect, redirect_used_str)
            } else if error_body.contains("invalid_grant") {
                "The authorization code expired or was already used. This can happen if you took too long to authorize, or if your OAuth client configuration uses a 'Web Application' credential with a loopback redirect (which is blocked by Google). Create a 'Desktop App' credential instead.".to_string()
            } else {
                format!("Token exchange failed: {}", error_body)
            };
            return Err(diagnostic);
        }
        let tokens: Value = tr.json().await.map_err(e)?;

        // Persist refresh token (if present) into settings for later use
        let refresh = tokens["refresh_token"].as_str().unwrap_or("").to_string();
        let access = tokens["access_token"].as_str().unwrap_or("").to_string();
        if access.is_empty() {
            return Err("OAuth token exchange succeeded but no access_token was returned. This usually indicates a scope permission issue — check that your Google Cloud Console OAuth consent screen includes the YouTube Data API v3 scopes, and that your test user email is added to the Test Users list.".into());
        }
        if !refresh.is_empty() {
            // Stored for parity/debuggability with the rest of the settings doc; this
            // generic loopback helper (also used by the "connect all channels" flow and
            // Google Drive project backup) has no single caller that reads these back today
            // — each flow persists its own scoped tokens (per-channel refresh_token, or the
            // per-project gdrive_refresh_token in authorize_project_gdrive).
            let coll = db.collection::<Document>("settings");
            let _ = coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "google_loopback_refresh_token": refresh.clone(), "google_loopback_access_token": access.clone() } }).await;
        }

        Ok(tokens)
    }.await;

    // Always shut down the local server, and wait for it to actually let go of the port before
    // returning — a caller that retries immediately would otherwise race the socket close.
    let _ = shutdown_tx.send(());
    let _ = server_task.await;
    result
}

// ────────────────────────────────────────────────────────────────
// Signing in to the app itself
//
// Same provider, same browser trip, different question: YouTube OAuth asks "may this app upload to
// your channel", this asks "who are you". Google answers the second one with an `id_token`, which the
// account server verifies to issue an entitlement.
//
// It reuses whichever OAuth client is already configured rather than shipping its own. That is not
// only about avoiding an embedded secret: an app-owned client would put this app in the middle of an
// identity it does not need to hold, when the user already has a Google Cloud client here for
// YouTube and the account server checks the token with Google directly either way.
// ────────────────────────────────────────────────────────────────

/// The OAuth client to sign in with: the one named, else a complete one, else nothing.
///
/// "Complete" matters — a half-filled client is worse than none here, because it fails after the
/// browser has already opened and looks like the sign-in itself is broken.
pub async fn signin_client(db: &crate::store::Db, forced: Option<&str>) -> Option<Value> {
    use futures_util::StreamExt;
    let usable = |c: &Value| {
        let f = |k: &str| c[k].as_str().unwrap_or("").trim().to_string();
        !f("client_id").is_empty() && !f("redirect_uri").is_empty()
            && !f("client_secret").is_empty() && !f("client_secret").starts_with('•')
    };
    if let Some(id) = forced.filter(|s| !s.trim().is_empty()) {
        let doc = db.collection::<Document>("oauth_clients")
            .find_one(doc! { "id": id }).await.ok().flatten()?;
        let c = bson_to_value(doc);
        return usable(&c).then_some(c);
    }
    let mut cursor = db.collection::<Document>("oauth_clients")
        .find(doc! {}).sort(doc! { "created_at": -1 }).await.ok()?;
    while let Some(Ok(d)) = cursor.next().await {
        let c = bson_to_value(d);
        if usable(&c) { return Some(c); }
    }
    None
}

/// Sign in with Google and return the ID token that proves who you are.
///
/// The one piece the paywall has been calling since it was written — `api.startGoogleIdSignIn?.()`,
/// with the optional-call `?.` quietly turning "this command does not exist" into "sign-in silently
/// did nothing", which is why there was no way into the app at all.
#[tauri::command]
pub async fn start_google_id_sign_in(
    state: State<'_, AppState>,
    oauth_client_id: Option<String>,
) -> Res<Value> {
    google_id_token(&state.db, oauth_client_id.as_deref()).await
}

/// The sign-in flow itself, on a raw store handle so the account sign-in can reuse it without going
/// back out through the command layer.
pub async fn google_id_token(db: &crate::store::Db, forced: Option<&str>) -> Res<Value> {
    let client = signin_client(db, forced).await.ok_or_else(|| {
        "No Google OAuth client is set up yet, so there is nothing to sign in with. Add one in \
         Settings → Google OAuth (or the Welcome Guide's YouTube step) — it takes one visit to \
         Google Cloud Console and the same client covers both sign-in and YouTube uploads.".to_string()
    })?;
    let db_id = client["id"].as_str().unwrap_or("").to_string();
    let tokens = perform_oauth_loopback(db, &db_id, None).await?;

    let id_token = tokens["id_token"].as_str().unwrap_or("").to_string();
    if id_token.is_empty() {
        return Err("Google completed the sign-in but returned no ID token. Check that the OAuth \
                    consent screen has the 'openid', 'email' and 'profile' scopes enabled."
                   .to_string());
    }
    let claims = id_token_claims(&id_token);
    Ok(serde_json::json!({
        "id_token": id_token,
        "email": claims["email"],
        "name": claims["name"],
        "picture": claims["picture"],
        "oauth_client_id": db_id,
        "label": client["label"],
    }))
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_cover_the_trailing_slash_and_the_host_spelling() {
        let v = redirect_variants("http://127.0.0.1:3335");
        // The one already sent is not a variant of itself — probing it again would just re-ask a
        // question already answered "no".
        assert!(!v.contains(&"http://127.0.0.1:3335".to_string()));
        assert!(v.contains(&"http://127.0.0.1:3335/".to_string()));
        assert!(v.contains(&"http://localhost:3335".to_string()));
        // 127.0.0.1 spellings come first, because those are the only ones adoptable without moving
        // the socket the callback server is already bound to.
        assert!(v[0].starts_with("http://127.0.0.1:"));
    }

    #[test]
    fn variants_of_a_portless_uri_are_empty() {
        // Nothing to probe: without a port there is no single address to try spellings of, and the
        // desktop-credential flow this shape belongs to accepts any port anyway.
        assert!(redirect_variants("http://127.0.0.1").is_empty());
        assert!(redirect_variants("not a url").is_empty());
    }

    #[test]
    fn google_doc_links_are_not_mistaken_for_the_redirect_uri() {
        // The regression that sent people to Cloud Console to register a documentation URL: Google's
        // authError blob carries its own help link, and the old code took the first URL it found.
        let blob = decode_auth_error("");
        assert!(blob.is_empty());
        let msg = format!(
            "This app asked to be sent back to {}, ", "http://127.0.0.1:3335");
        assert!(!msg.contains("developers.google.com"));
    }

    #[test]
    fn signin_scope_always_asks_for_an_id_token() {
        // Every flow, so every token exchange can also answer "who is this".
        assert_eq!(signin_scope(""), "openid email profile");
        assert!(signin_scope(YOUTUBE_SCOPE).starts_with("openid email profile "));
        assert!(signin_scope(YOUTUBE_SCOPE).contains("youtube.upload"));
    }

    #[test]
    fn id_token_claims_reads_the_payload_without_verifying_it() {
        use base64::Engine;
        let body = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(br#"{"email":"someone@example.com","name":"Someone"}"#);
        let token = format!("header.{body}.signature");
        let claims = id_token_claims(&token);
        assert_eq!(claims["email"], "someone@example.com");
        // Junk must not panic — it is attacker-shaped input from a network response.
        assert!(id_token_claims("garbage").is_null());
        assert!(id_token_claims("").is_null());
    }

    #[test]
    fn authorize_url_encodes_the_redirect_as_one_parameter() {
        let url = authorize_url("cid.apps.googleusercontent.com", "http://127.0.0.1:3335",
                                "openid email", "state-1");
        // The ':' and '/' must survive as encoded bytes, or Google reads the redirect as several
        // parameters and rejects a URI the app never sent.
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A3335"));
        assert!(url.contains("scope=openid%20email"));
        assert!(url.contains("state=state-1"));
    }
}
