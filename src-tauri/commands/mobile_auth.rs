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
// Signing in to Google from a phone
//
// The desktop flow uses a loopback redirect — Google sends the code to `http://127.0.0.1:8765/`, which
// the app is listening on. Android cannot do that: there is no loopback server to bind, and Google
// rejects the loopback redirect for Android client types anyway.
//
// So there are two routes, and both are supported because they fail in different situations:
//
//   1. **Transfer** — sign in once on the desktop and move the resulting refresh tokens to the phone.
//      Needs no new OAuth client, no new consent screen, and no Play Store review. The phone holds a
//      token it did not obtain, which is fine: a refresh token is exactly the thing designed to be
//      stored and reused.
//   2. **Deep link** — register an Android OAuth client whose redirect is a custom scheme, and let the
//      phone do the whole flow itself. Needs the user to create that client in Google Cloud, and needs
//      the scheme registered in the Android manifest. Better once set up, because the phone becomes
//      independent of the desktop.
//
// Transfer is the default because it works today with nothing new created. The deep link is offered
// beside it rather than instead of it — a phone that has been away from its desktop for a month still
// needs a way to re-consent.
//
// The transfer blob is **sealed with the vault's own key**, not base64'd and hoped for. A refresh token
// in plain text in a chat message is a token somebody else can use for a year.
// ────────────────────────────────────────────────────────────────

/// Which redirect this platform can actually use.
///
/// A pure decision so the same rule can be tested and shown in the UI, rather than being buried inside
/// the request builder where a wrong answer looks like a Google error.
pub fn redirect_mode(is_desktop: bool, android_client_configured: bool) -> &'static str {
    if is_desktop { return "loopback"; }
    if android_client_configured { "deeplink" } else { "transfer" }
}

/// The custom-scheme redirect an Android OAuth client uses.
///
/// Google's convention is the reversed client id followed by a path. It has to match the client exactly
/// — the same exact-string rule that makes the desktop loopback so easy to get wrong.
pub fn deeplink_redirect(android_client_id: &str) -> Option<String> {
    let id = android_client_id.trim();
    if id.is_empty() { return None; }
    // `1234-abc.apps.googleusercontent.com` → `com.googleusercontent.apps.1234-abc:/oauth2redirect`
    let prefix = id.strip_suffix(".apps.googleusercontent.com")?;
    Some(format!("com.googleusercontent.apps.{prefix}:/oauth2redirect"))
}

/// What this device can and cannot do, as one answer the whole UI can read.
///
/// Collected here rather than probed in six places: a page that offers a local ffmpeg render on a phone
/// has already wasted the user's time by the point the job fails.
#[tauri::command]
pub async fn platform_capabilities(state: State<'_, AppState>) -> Res<Value> {
    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let android_client = settings["android_oauth_client_id"].as_str().unwrap_or("").trim().to_string();
    let is_desktop = cfg!(desktop);

    Ok(json!({
        "desktop": is_desktop,
        "mobile": !is_desktop,
        // The two things a phone genuinely cannot do, and what stands in for each.
        "local_cli": is_desktop,
        "local_cli_note": if is_desktop { "" } else {
            "No ffmpeg here — command-line steps run on Modal or your own worker. Free credits cover a \
             daily video comfortably."
        },
        "hidden_webview": is_desktop,
        "hidden_webview_note": if is_desktop { "" } else {
            "A phone gives the app one webview: its own. So Suno cannot be driven by automation here — \
             it goes through your captured session instead, which needs no browser at all."
        },
        // Everything that does work, stated because "mobile-lite" undersells it.
        "voice_out": true,
        "voice_in": true,
        "guides": true,
        "suno_via_cookie": true,
        "oauth_mode": redirect_mode(is_desktop, !android_client.is_empty()),
        "android_client_configured": !android_client.is_empty(),
        "deeplink_redirect": deeplink_redirect(&android_client),
    }))
}

/// Package this install's channel refresh tokens for another device.
///
/// Sealed with the vault's key, so the blob is useless to anyone who does not have the passphrase. A
/// refresh token pasted in plain text into a chat is a token somebody else can use for a year.
#[tauri::command]
pub async fn export_channel_tokens(state: State<'_, AppState>, passphrase: String) -> Res<Value> {
    use futures_util::StreamExt;
    if passphrase.trim().len() < 8 {
        return Err("Use a passphrase of at least eight characters — this blob carries live tokens.".into());
    }
    let mut channels = Vec::new();
    let mut cursor = state.db.collection::<Document>("channels").find(doc! {}).await.map_err(e)?;
    while let Some(Ok(d)) = cursor.next().await {
        let c = bson_to_value(d);
        let token = c["refresh_token"].as_str().unwrap_or("");
        if token.is_empty() { continue; }
        channels.push(json!({
            "id": c["id"], "name": c["name"],
            "youtube_channel_id": c["youtube_channel_id"],
            "refresh_token": token,
            "oauth_client_id": c["oauth_client_id"],
            "language": c["language"], "region": c["region"],
        }));
    }
    if channels.is_empty() {
        return Err("No channel here has a refresh token yet — connect one first.".into());
    }
    let payload = json!({ "v": 1, "kind": "channel-tokens", "channels": channels,
                          "exported_at": crate::models::now_iso() });
    let sealed = crate::vault::seal_with(&passphrase, &payload.to_string()).map_err(e)?;
    Ok(json!({
        "blob": sealed,
        "channels": channels.len(),
        "how": "Paste this into the phone under Team → Bring my sign-ins across, with the same \
                passphrase. It is sealed, so it is safe to send through a normal chat.",
        "caveat": "It carries live refresh tokens. Anyone who has both the blob and the passphrase can \
                   post to these channels.",
    }))
}

#[derive(serde::Deserialize)]
pub struct ImportTokens {
    pub blob: String,
    pub passphrase: String,
}

/// Take channel sign-ins from another device.
#[tauri::command]
pub async fn import_channel_tokens(state: State<'_, AppState>, payload: ImportTokens) -> Res<Value> {
    let opened = crate::vault::open_with(&payload.passphrase, payload.blob.trim())
        .map_err(|_| "That passphrase does not open this blob. They have to match exactly.".to_string())?;
    let parsed: Value = serde_json::from_str(&opened)
        .map_err(|_| "The blob opened but did not contain sign-ins.".to_string())?;
    if parsed["kind"].as_str() != Some("channel-tokens") {
        return Err("That is a sealed blob, but not one of channel sign-ins.".into());
    }
    let channels = parsed["channels"].as_array().cloned().unwrap_or_default();
    let mut imported = 0;
    for c in channels {
        let Some(id) = c["id"].as_str() else { continue };
        let mut set = Document::new();
        for key in ["name", "youtube_channel_id", "refresh_token", "oauth_client_id", "language", "region"] {
            if let Some(v) = c.get(key) {
                if let Ok(b) = bson::to_bson(v) { set.insert(key, b); }
            }
        }
        set.insert("id", id);
        set.insert("connected", true);
        if state.db.collection::<Document>("channels")
            .update_one(doc! { "id": id }, doc! { "$set": set }).upsert(true).await.is_ok() {
            imported += 1;
        }
    }
    Ok(json!({
        "imported": imported,
        "note": "These channels can upload from this device now. A refresh token stays valid until it is \
                 revoked, so nothing has to be re-consented.",
    }))
}

/// What the user has to do to make the phone sign in on its own.
///
/// Instructions rather than automation, because every step is in Google's console. Kept next to the code
/// that depends on it so the two cannot drift.
#[tauri::command]
pub async fn deeplink_setup_steps(state: State<'_, AppState>) -> Res<Value> {
    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let client = settings["android_oauth_client_id"].as_str().unwrap_or("").trim().to_string();
    Ok(json!({
        "configured": !client.is_empty(),
        "redirect": deeplink_redirect(&client),
        "package": "com.studio.lightkid",
        "steps": [
            "Google Cloud console → Credentials → Create credentials → OAuth client ID.",
            "Application type: Android. Package name: com.studio.lightkid.",
            "Paste the SHA-1 of the keystore the APK is signed with (debug or release — they differ, \
             and a release build signed with a different key will not match).",
            "Copy the client id into Settings → Mobile sign-in.",
            "The redirect is derived from the client id and shown here. The Android manifest carries \
             an intent filter for it, but the scheme comes from *your* client id — so the APK has to \
             be built with GOOGLE_REDIRECT_SCHEME set to the value shown above, or the browser will \
             finish the sign-in and have nowhere to hand it back to.",
        ],
        "why_bother": "Without it the phone borrows sign-ins from the desktop, which works but means the \
                       desktop has to be reachable whenever consent expires.",
        // Said plainly because the previous version of this claimed the build already handled it,
        // and it did not: there was no intent filter in the manifest at all.
        "state": "The manifest now has the intent filter. What is not yet wired is the app-side \
                  handler that receives the returning URL — until that lands, the browser will come \
                  back to the app but the app will not act on it. Borrowing sign-ins from the \
                  desktop is the route that works today.",
        "alternative": "Team → Bring my sign-ins across. No client to create, works immediately.",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_phone_without_its_own_client_borrows_sign_ins() {
        // The default has to work with nothing created, or a new phone is a dead end until somebody
        // sits down with the Google console.
        assert_eq!(redirect_mode(false, false), "transfer");
        assert_eq!(redirect_mode(false, true), "deeplink");
        // Desktop keeps the loopback whatever else is configured — it is the one that needs no setup.
        assert_eq!(redirect_mode(true, false), "loopback");
        assert_eq!(redirect_mode(true, true), "loopback");
    }

    #[test]
    fn the_deeplink_redirect_is_the_reversed_client_id() {
        // Google matches redirect_uri as an exact string, so this is the kind of thing that must be
        // derived rather than typed twice.
        assert_eq!(
            deeplink_redirect("413856182884-j24sp8.apps.googleusercontent.com").as_deref(),
            Some("com.googleusercontent.apps.413856182884-j24sp8:/oauth2redirect"),
        );
        // A web client id is not an Android one and cannot be turned into a scheme.
        assert_eq!(deeplink_redirect(""), None);
        assert_eq!(deeplink_redirect("not-a-client-id"), None);
        assert_eq!(deeplink_redirect("  "), None);
    }
}
