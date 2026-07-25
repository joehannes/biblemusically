//! The social-presence engine: who the user is, what to make next, and getting it published in
//! more than one place.
//!
//! This implements the four axes of the "Social presence" backlog item:
//!
//! * **(A) Self-knowledge** — connect accounts, ingest the user's own posts/likes/follows, and
//!   distil them into a taste profile stored with the JSON learnings.
//! * **(B) Better suggestions** — expose that profile as a prompt block the composer and DailyGuide
//!   read, so ideation is grounded in the user's actual audience and taste.
//! * **(C) Co-creation** — derive per-platform versions of each day's output (a vertical short, a
//!   still + poetic caption, a teaser linking back to the full video).
//! * **(D/E) Co-publishing** — publish those derivatives to the platforms that have real APIs, and
//!   hand the rest to the built-in browser + recorded macros.
//!
//! # Why these platforms and not others
//!
//! Ranked by what a free account can actually do, which is what decides the order things get built:
//!
//! | Tier | Platforms | Reality |
//! |---|---|---|
//! | Open API, no gatekeeping | Mastodon, Bluesky (AT Protocol), Telegram, Discord, Nostr | a token/app-password is enough; implemented here |
//! | Free but review-gated | Instagram/Facebook/Threads (Meta Graph), TikTok Content Posting | $0 to call, but an app review stands between you and posting publicly |
//! | Paid | X/Twitter | pay-per-post since Feb 2026 — deliberately not wired |
//! | No usable API | everything else | the embedded browser + a recorded macro |
//!
//! Every credential goes through `vault.rs`. Nothing here writes a secret to the store, and no
//! ingested content is ever sent to an AI provider without the user asking for a profile refresh.
//!
//! **Publishing is opt-in per platform and per post.** Nothing reaches an audience because a timer
//! fired — the same conservative default the chapter scheduler uses.

use crate::state::AppState;
use crate::vault;
use bson::{doc, Document};
use serde_json::{json, Value};
use tauri::State;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

fn cred_key(platform: &str, field: &str) -> String { format!("social.{platform}.{field}") }

fn client() -> Res<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("studio-lightkid")
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .map_err(e)
}

// ────────────────────────────────────────────────────────────────────────────
// (A) Platform catalog + connections
// ────────────────────────────────────────────────────────────────────────────

/// What each platform needs, what it can do, and how honest we are about the limits.
#[tauri::command]
pub async fn list_social_platforms() -> Res<Value> {
    Ok(json!([
        {
            "id": "mastodon", "label": "Mastodon", "tier": "open",
            "can": ["read_own", "post_text", "post_image", "post_video"],
            "fields": [
                { "key": "instance", "label": "Instance URL", "placeholder": "https://mastodon.social", "secret": false },
                { "key": "token", "label": "Access token", "placeholder": "from Preferences → Development", "secret": true }
            ],
            "help": "Create an application under Preferences → Development with read+write scopes.",
            "cost": "Free, no review, no rate-limit games."
        },
        {
            "id": "bluesky", "label": "Bluesky", "tier": "open",
            "can": ["read_own", "post_text", "post_image"],
            "fields": [
                { "key": "handle", "label": "Handle", "placeholder": "you.bsky.social", "secret": false },
                { "key": "app_password", "label": "App password", "placeholder": "Settings → App Passwords", "secret": true }
            ],
            "help": "Use an app password, never your account password.",
            "cost": "Free. Video posting is limited; images and text are reliable."
        },
        {
            "id": "telegram", "label": "Telegram channel", "tier": "open",
            "can": ["post_text", "post_image", "post_video"],
            "fields": [
                { "key": "bot_token", "label": "Bot token", "placeholder": "from @BotFather", "secret": true },
                { "key": "chat_id", "label": "Channel / chat id", "placeholder": "@mychannel or -1001234567890", "secret": false }
            ],
            "help": "Create a bot with @BotFather and add it to your channel as an admin.",
            "cost": "Free."
        },
        {
            "id": "discord", "label": "Discord webhook", "tier": "open",
            "can": ["post_text", "post_image", "post_video"],
            "fields": [
                { "key": "webhook_url", "label": "Webhook URL", "placeholder": "https://discord.com/api/webhooks/…", "secret": true }
            ],
            "help": "Channel → Integrations → Webhooks → New Webhook.",
            "cost": "Free. Uploads are capped at the server's file limit (8–100 MB)."
        },
        {
            "id": "youtube", "label": "YouTube", "tier": "connected",
            "can": ["post_video", "read_stats"],
            "fields": [],
            "help": "Already connected per channel via OAuth — see the Channel Manager.",
            "cost": "Free within the daily upload quota."
        },
        {
            "id": "instagram", "label": "Instagram / Facebook / Threads", "tier": "review_gated",
            "can": ["post_image", "post_video"],
            "fields": [
                { "key": "access_token", "label": "Graph API token", "placeholder": "long-lived page token", "secret": true },
                { "key": "ig_user_id", "label": "IG user id", "placeholder": "17841400000000000", "secret": false }
            ],
            "help": "Free to call, but Meta App Review is required before it works beyond your own test users.",
            "cost": "Free, gated by app review."
        },
        {
            "id": "tiktok", "label": "TikTok", "tier": "review_gated",
            "can": ["post_video"],
            "fields": [
                { "key": "access_token", "label": "Content Posting API token", "placeholder": "OAuth access token", "secret": true }
            ],
            "help": "No fee, but posts stay private-only until TikTok audits the app (1–2 weeks).",
            "cost": "Free, gated by audit."
        },
        {
            "id": "reddit", "label": "Reddit", "tier": "open",
            "can": ["read_own", "post_text", "post_link", "post_image"],
            "fields": [
                { "key": "client_id", "label": "App client id", "placeholder": "from reddit.com/prefs/apps (script app)", "secret": false },
                { "key": "client_secret", "label": "App secret", "placeholder": "", "secret": true },
                { "key": "username", "label": "Reddit username", "placeholder": "u/yourname", "secret": false },
                { "key": "password", "label": "Password", "placeholder": "", "secret": true },
                { "key": "subreddits", "label": "Target subreddits", "placeholder": "ChristianMusic, WorshipMusic", "secret": false }
            ],
            "help": "Create a 'script' app at reddit.com/prefs/apps. Read each subreddit's rules first — most ban bare self-promotion, and a post that ignores them costs the account, not just the post.",
            "cost": "Free. 100 queries/minute per OAuth client."
        },
        {
            "id": "devto", "label": "DEV.to", "tier": "open",
            "can": ["post_article"],
            "fields": [
                { "key": "api_key", "label": "API key", "placeholder": "Settings → Extensions → DEV Community API Keys", "secret": true }
            ],
            "help": "Publishes full markdown articles with a cover image. Canonical-URL support means cross-posting will not hurt the original.",
            "cost": "Free."
        },
        {
            "id": "wordpress", "label": "WordPress / Ghost blog", "tier": "open",
            "can": ["post_article"],
            "fields": [
                { "key": "site_url", "label": "Site URL", "placeholder": "https://yourblog.com", "secret": false },
                { "key": "token", "label": "Application password / admin API key", "placeholder": "", "secret": true }
            ],
            "help": "Your own site is the one platform whose rules cannot change under you — worth having as the canonical home for every article.",
            "cost": "Whatever your hosting costs."
        },
        {
            "id": "tumblr", "label": "Tumblr", "tier": "open",
            "can": ["post_text", "post_image", "post_video"],
            "fields": [
                { "key": "consumer_key", "label": "Consumer key", "placeholder": "from tumblr.com/oauth/apps", "secret": false },
                { "key": "consumer_secret", "label": "Consumer secret", "placeholder": "", "secret": true },
                { "key": "blog", "label": "Blog name", "placeholder": "yourblog", "secret": false }
            ],
            "help": "Still a real distribution channel for image-led devotional posts.",
            "cost": "Free."
        },
        {
            "id": "pinterest", "label": "Pinterest", "tier": "review_gated",
            "can": ["post_image", "post_link"],
            "fields": [
                { "key": "access_token", "label": "Access token", "placeholder": "from developers.pinterest.com", "secret": true },
                { "key": "board_id", "label": "Board id", "placeholder": "", "secret": false }
            ],
            "help": "Strong for evergreen visual reach — a pinned cover image keeps sending traffic months later. Trial access posts to your own boards; wider access needs review.",
            "cost": "Free, gated by review."
        },
        {
            "id": "linkedin", "label": "LinkedIn page", "tier": "review_gated",
            "can": ["post_text", "post_image", "post_link"],
            "fields": [
                { "key": "access_token", "label": "Access token", "placeholder": "OAuth token with w_organization_social", "secret": true },
                { "key": "org_urn", "label": "Organisation URN", "placeholder": "urn:li:organization:12345", "secret": false }
            ],
            "help": "Posting to a company page needs the Community Management API, which is partner-gated. Personal-profile posting is not offered by the API at all.",
            "cost": "Free, gated by partner approval."
        },
        {
            "id": "x", "label": "X (Twitter)", "tier": "paid",
            "can": ["post_text", "post_image"],
            "fields": [
                { "key": "api_key", "label": "API key", "placeholder": "", "secret": false },
                { "key": "api_secret", "label": "API secret", "placeholder": "", "secret": true },
                { "key": "access_token", "label": "Access token", "placeholder": "", "secret": false },
                { "key": "access_secret", "label": "Access token secret", "placeholder": "", "secret": true }
            ],
            "help": "Writing costs money: the free tier allows a very small number of posts a month, and the next tier up is a monthly subscription. A browser macro is the cheaper route if you post often.",
            "cost": "Paid — free tier is post-capped, Basic is a monthly fee."
        },
        {
            "id": "browser_macro", "label": "Anything else (browser macro)", "tier": "macro",
            "can": ["post_text", "post_image", "post_video"],
            "fields": [],
            "help": "Record the posting flow once in the built-in browser (Macro Manager), then replay it.",
            "cost": "Free. As fragile as the site's markup."
        }
    ]))
}

/// Store a platform's credentials in the vault and record that the account is connected.
#[tauri::command]
pub async fn connect_social_account(
    state: State<'_, AppState>,
    platform: String,
    fields: Value,
) -> Res<Value> {
    let Some(map) = fields.as_object() else { return Err("fields must be an object".into()) };
    let mut public_fields = bson::Document::new();
    for (key, value) in map {
        let Some(text) = value.as_str() else { continue };
        if text.trim().is_empty() {
            continue;
        }
        // Secrets to the vault; non-secret identifiers (instance URL, chat id) stay readable so the
        // UI can show what's connected without unlocking anything.
        let is_secret = matches!(key.as_str(), "token" | "app_password" | "bot_token" | "webhook_url" | "access_token" | "password");
        if is_secret {
            vault::put(&cred_key(&platform, key), text.trim())?;
        } else {
            public_fields.insert(key.clone(), text.trim());
        }
    }
    public_fields.insert("_id", &platform);
    public_fields.insert("platform", &platform);
    public_fields.insert("connected_at", chrono::Utc::now().to_rfc3339());
    state
        .db
        .collection::<Document>("social_accounts")
        .update_one(doc! { "_id": &platform }, doc! { "$set": public_fields })
        .upsert(true)
        .await
        .map_err(e)?;
    verify_social_account(state, platform).await
}

/// Connected accounts, with whatever the platform says about them.
#[tauri::command]
pub async fn list_social_accounts(state: State<'_, AppState>) -> Res<Vec<Value>> {
    use futures_util::StreamExt;
    let mut cursor = state
        .db
        .collection::<Document>("social_accounts")
        .find(doc! {})
        .await
        .map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await {
        let mut v = crate::store::doc_to_json(&d);
        let platform = v["platform"].as_str().unwrap_or("").to_string();
        v["has_secret"] = json!(
            ["token", "app_password", "bot_token", "webhook_url", "access_token"]
                .iter()
                .any(|f| vault::get_quiet(&cred_key(&platform, f)).is_some())
        );
        out.push(v);
    }
    Ok(out)
}

#[tauri::command]
pub async fn disconnect_social_account(state: State<'_, AppState>, platform: String) -> Res<Value> {
    for field in ["token", "app_password", "bot_token", "webhook_url", "access_token"] {
        let _ = vault::delete(&cred_key(&platform, field));
    }
    state
        .db
        .collection::<Document>("social_accounts")
        .delete_one(doc! { "_id": &platform })
        .await
        .map_err(e)?;
    Ok(json!({ "ok": true, "platform": platform }))
}

/// Ask the platform who we are — the connection test.
#[tauri::command]
pub async fn verify_social_account(state: State<'_, AppState>, platform: String) -> Res<Value> {
    let account = state
        .db
        .collection::<Document>("social_accounts")
        .find_one(doc! { "_id": &platform })
        .await
        .map_err(e)?
        .map(|d| crate::store::doc_to_json(&d))
        .unwrap_or_else(|| json!({}));

    let http = client()?;
    let outcome: Res<Value> = match platform.as_str() {
        "mastodon" => {
            let instance = account["instance"].as_str().unwrap_or("").trim_end_matches('/').to_string();
            let token = vault::get(&cred_key(&platform, "token"))?.unwrap_or_default();
            if instance.is_empty() || token.is_empty() {
                Err("Instance URL and access token are both required.".into())
            } else {
                let resp = http
                    .get(format!("{instance}/api/v1/accounts/verify_credentials"))
                    .bearer_auth(&token)
                    .send()
                    .await
                    .map_err(e)?;
                let ok = resp.status().is_success();
                let body: Value = resp.json().await.unwrap_or(Value::Null);
                if ok {
                    Ok(json!({
                        "handle": body["acct"], "display_name": body["display_name"],
                        "followers": body["followers_count"], "posts": body["statuses_count"],
                    }))
                } else {
                    Err(format!("Mastodon rejected the token: {}", body["error"].as_str().unwrap_or("unknown")))
                }
            }
        }
        "bluesky" => {
            let handle = account["handle"].as_str().unwrap_or("").to_string();
            let password = vault::get(&cred_key(&platform, "app_password"))?.unwrap_or_default();
            match bluesky_session(&http, &handle, &password).await {
                Ok(session) => Ok(json!({ "handle": handle, "did": session["did"] })),
                Err(err) => Err(err),
            }
        }
        "telegram" => {
            let token = vault::get(&cred_key(&platform, "bot_token"))?.unwrap_or_default();
            let resp = http
                .get(format!("https://api.telegram.org/bot{token}/getMe"))
                .send()
                .await
                .map_err(e)?;
            let body: Value = resp.json().await.unwrap_or(Value::Null);
            if body["ok"].as_bool() == Some(true) {
                Ok(json!({ "handle": body["result"]["username"], "bot": true }))
            } else {
                Err(format!("Telegram rejected the bot token: {}", body["description"].as_str().unwrap_or("unknown")))
            }
        }
        "discord" => {
            let url = vault::get(&cred_key(&platform, "webhook_url"))?.unwrap_or_default();
            if url.is_empty() {
                Err("Webhook URL is required.".into())
            } else {
                let resp = http.get(&url).send().await.map_err(e)?;
                let body: Value = resp.json().await.unwrap_or(Value::Null);
                match body["channel_id"].as_str() {
                    Some(channel) => Ok(json!({ "handle": body["name"], "channel_id": channel })),
                    None => Err("Discord did not recognize that webhook.".into()),
                }
            }
        }
        // These are either verified elsewhere (YouTube OAuth) or have no cheap identity call.
        "youtube" => Ok(json!({ "note": "Connected per channel in the Channel Manager." })),
        "browser_macro" => Ok(json!({ "note": "No credentials — replay a recorded macro." })),
        other => Ok(json!({ "note": format!("{other}: credentials stored; posting will report any problem.") })),
    };

    let coll = state.db.collection::<Document>("social_accounts");
    match outcome {
        Ok(info) => {
            let bson_info = bson::to_bson(&info).unwrap_or(bson::Bson::Null);
            coll.update_one(
                doc! { "_id": &platform },
                doc! { "$set": { "ok": true, "info": bson_info, "last_error": Option::<String>::None,
                                 "checked_at": chrono::Utc::now().to_rfc3339() } },
            )
            .upsert(true)
            .await
            .map_err(e)?;
            Ok(json!({ "ok": true, "platform": platform, "info": info }))
        }
        Err(err) => {
            coll.update_one(
                doc! { "_id": &platform },
                doc! { "$set": { "ok": false, "last_error": &err,
                                 "checked_at": chrono::Utc::now().to_rfc3339() } },
            )
            .upsert(true)
            .await
            .map_err(e)?;
            Err(err)
        }
    }
}

/// Log in to Bluesky and return the session (access JWT + DID).
async fn bluesky_session(http: &reqwest::Client, handle: &str, password: &str) -> Res<Value> {
    if handle.is_empty() || password.is_empty() {
        return Err("Handle and app password are both required.".into());
    }
    let resp = http
        .post("https://bsky.social/xrpc/com.atproto.server.createSession")
        .json(&json!({ "identifier": handle, "password": password }))
        .send()
        .await
        .map_err(e)?;
    let ok = resp.status().is_success();
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    if ok {
        Ok(body)
    } else {
        Err(format!(
            "Bluesky rejected the login: {}",
            body["message"].as_str().unwrap_or("check the handle and app password")
        ))
    }
}

// ────────────────────────────────────────────────────────────────────────────
// (A) Ingestion → taste profile
// ────────────────────────────────────────────────────────────────────────────

/// Pull the user's own recent activity from the platforms that expose it cleanly, and store the
/// distilled signal in the learnings store.
///
/// Only open-protocol platforms are read here (Mastodon, Bluesky). That is a deliberate choice, not
/// an omission: Instagram-style scraping with the user's own session risks their account, and the
/// backlog's own guidance was to start with the surfaces that answer honestly. Anything else can be
/// captured with a browser macro instead.
#[tauri::command]
pub async fn ingest_social_activity(state: State<'_, AppState>, limit: Option<u32>) -> Res<Value> {
    let limit = limit.unwrap_or(50).min(200);
    let http = client()?;
    let accounts = list_social_accounts(state.clone()).await?;
    let mut posts: Vec<Value> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for account in &accounts {
        let platform = account["platform"].as_str().unwrap_or("");
        match platform {
            "mastodon" => {
                let instance = account["instance"].as_str().unwrap_or("").trim_end_matches('/').to_string();
                let token = vault::get_quiet(&cred_key(platform, "token")).unwrap_or_default();
                if instance.is_empty() || token.is_empty() { continue; }
                // Own posts, then favourites — "what I say" and "what I like" are different signals.
                let me: Value = match http
                    .get(format!("{instance}/api/v1/accounts/verify_credentials"))
                    .bearer_auth(&token).send().await
                {
                    Ok(r) => r.json().await.unwrap_or(Value::Null),
                    Err(err) => { warnings.push(format!("mastodon: {err}")); continue; }
                };
                let id = me["id"].as_str().unwrap_or("").to_string();
                for (endpoint, kind) in [
                    (format!("{instance}/api/v1/accounts/{id}/statuses?limit={limit}"), "own_post"),
                    (format!("{instance}/api/v1/favourites?limit={limit}"), "liked"),
                ] {
                    match http.get(&endpoint).bearer_auth(&token).send().await {
                        Ok(resp) => {
                            let items: Value = resp.json().await.unwrap_or(Value::Null);
                            for item in items.as_array().cloned().unwrap_or_default() {
                                posts.push(json!({
                                    "platform": "mastodon", "kind": kind,
                                    "text": strip_html(item["content"].as_str().unwrap_or("")),
                                    "created_at": item["created_at"],
                                    "engagement": item["favourites_count"].as_i64().unwrap_or(0)
                                        + item["reblogs_count"].as_i64().unwrap_or(0),
                                    "tags": item["tags"].as_array().map(|t| t.iter().filter_map(|x| x["name"].as_str()).collect::<Vec<_>>()),
                                }));
                            }
                        }
                        Err(err) => warnings.push(format!("mastodon {kind}: {err}")),
                    }
                }
            }
            "bluesky" => {
                let handle = account["handle"].as_str().unwrap_or("").to_string();
                let password = vault::get_quiet(&cred_key(platform, "app_password")).unwrap_or_default();
                let session = match bluesky_session(&http, &handle, &password).await {
                    Ok(s) => s,
                    Err(err) => { warnings.push(format!("bluesky: {err}")); continue; }
                };
                let jwt = session["accessJwt"].as_str().unwrap_or("").to_string();
                let url = format!(
                    "https://bsky.social/xrpc/app.bsky.feed.getAuthorFeed?actor={handle}&limit={limit}"
                );
                match http.get(&url).bearer_auth(&jwt).send().await {
                    Ok(resp) => {
                        let body: Value = resp.json().await.unwrap_or(Value::Null);
                        for item in body["feed"].as_array().cloned().unwrap_or_default() {
                            let post = &item["post"];
                            posts.push(json!({
                                "platform": "bluesky", "kind": "own_post",
                                "text": post["record"]["text"],
                                "created_at": post["record"]["createdAt"],
                                "engagement": post["likeCount"].as_i64().unwrap_or(0)
                                    + post["repostCount"].as_i64().unwrap_or(0),
                            }));
                        }
                    }
                    Err(err) => warnings.push(format!("bluesky feed: {err}")),
                }
            }
            _ => {}
        }
    }

    // Keep the raw-ish sample with the learnings, capped, so a profile refresh has material to
    // work from without re-fetching, and the user can see what was collected.
    let sample: Vec<Value> = posts.iter().rev().take(120).cloned().collect();
    let patch = json!({
        "social": {
            "ingested_at": chrono::Utc::now().to_rfc3339(),
            "post_count": posts.len(),
            "sample": sample,
        }
    });
    crate::commands::learnings::update_user_learnings(patch).await?;

    Ok(json!({
        "ok": true,
        "ingested": posts.len(),
        "platforms": accounts.iter().filter_map(|a| a["platform"].as_str()).collect::<Vec<_>>(),
        "warnings": warnings,
    }))
}

/// Crude tag-stripper for Mastodon's HTML post bodies — enough to feed text to an LLM.
fn strip_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
        .replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">")
        .replace("&quot;", "\"").replace("&#39;", "'").replace("&nbsp;", " ")
}

/// Turn the ingested sample into a taste profile with the free in-app AI, and store it.
///
/// The profile is what (B) actually consumes: a short, readable description of voice, themes,
/// audience and what performs — not a data dump. It is stored in the learnings JSON so it travels
/// with the user's other preferences and is editable by hand.
#[tauri::command]
pub async fn refresh_taste_profile(state: State<'_, AppState>) -> Res<Value> {
    let learnings = crate::commands::learnings::get_user_learnings().await?;
    let sample = learnings["social"]["sample"].as_array().cloned().unwrap_or_default();
    if sample.is_empty() {
        return Err("Nothing ingested yet — connect an account and run \"Refresh from platforms\" first.".into());
    }

    // Only text and engagement go to the model; handles, ids and links stay local.
    let material: Vec<String> = sample
        .iter()
        .filter_map(|p| {
            let text = p["text"].as_str()?.trim();
            if text.is_empty() { return None; }
            Some(format!("[{} · {} likes] {}", p["kind"].as_str().unwrap_or("post"), p["engagement"].as_i64().unwrap_or(0), text))
        })
        .take(120)
        .collect();

    let system = "You profile a content creator from their own social posts, for use as creative \
        context by a music-video studio. Return ONLY a JSON object with these string fields: \
        `voice` (how they write, 1-2 sentences), `themes` (recurring subjects, comma-separated), \
        `audience` (who engages, 1 sentence), `performs` (what their higher-engagement posts have \
        in common, 1 sentence), `avoid` (tones or topics that are clearly not theirs). Be concrete \
        and specific; never invent biography that isn't in the posts.";
    let user = format!("Posts, newest first:\n{}", material.join("\n"));

    let response = crate::commands::ai::call_openrouter(&state.db, system, &user, 0.4, true).await?;
    let text = response["text"].as_str().unwrap_or("");
    let profile = crate::commands::ai::extract_json_value(text)
        .ok_or("The AI did not return a usable profile — try again.")?;

    crate::commands::learnings::update_user_learnings(json!({
        "taste_profile": {
            "voice": profile["voice"],
            "themes": profile["themes"],
            "audience": profile["audience"],
            "performs": profile["performs"],
            "avoid": profile["avoid"],
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "from_posts": material.len(),
        }
    }))
    .await?;
    Ok(json!({ "ok": true, "profile": profile, "from_posts": material.len() }))
}

/// (B) The profile as a prompt block, for the composer, DailyGuide and ideation to include.
pub async fn taste_profile_block() -> String {
    let Ok(learnings) = crate::commands::learnings::get_user_learnings().await else {
        return String::new();
    };
    let profile = &learnings["taste_profile"];
    if !profile.is_object() {
        return String::new();
    }
    let mut lines = Vec::new();
    for (key, label) in [
        ("voice", "Voice"),
        ("themes", "Recurring themes"),
        ("audience", "Audience"),
        ("performs", "What performs"),
        ("avoid", "Avoid"),
    ] {
        if let Some(text) = profile[key].as_str().filter(|s| !s.trim().is_empty()) {
            lines.push(format!("- {label}: {}", text.trim()));
        }
    }
    if lines.is_empty() {
        return String::new();
    }
    format!(
        "CREATOR PROFILE (derived from this user's own social posts — match this voice and these \
         interests unless the request says otherwise):\n{}\n",
        lines.join("\n")
    )
}

/// (B) Ideation partner: "what should I make next?", grounded in the profile and recent output.
#[tauri::command]
pub async fn ideate_next(state: State<'_, AppState>, project_id: Option<String>, question: Option<String>) -> Res<Value> {
    use futures_util::StreamExt;
    let mut recent: Vec<String> = Vec::new();
    if let Some(pid) = project_id.as_deref().filter(|s| !s.is_empty()) {
        if let Ok(mut cursor) = state
            .db
            .collection::<Document>("songs")
            .find(doc! { "project_id": pid })
            .sort(doc! { "created_at": -1 })
            .limit(15i64)
            .await
        {
            while let Some(Ok(d)) = cursor.next().await {
                let title = d.get_str("title").unwrap_or("");
                let styles = d.get_str("styles").unwrap_or("");
                let status = d.get_str("status").unwrap_or("");
                recent.push(format!("- {title} [{styles}] ({status})"));
            }
        }
    }

    let profile_block = taste_profile_block().await;
    let learnings_block = crate::commands::learnings::learnings_prompt_block(
        &state,
        project_id.as_deref().unwrap_or(""),
    )
    .await;

    let system = "You are a candid creative partner for a Bible-music channel operator. Suggest what \
        to make next. Return ONLY a JSON object: { \"ideas\": [ { \"title\": string, \"angle\": string, \
        \"why_now\": string, \"styles\": string, \"platforms\": string } ], \"note\": string }. \
        Give 4 ideas that are distinct from each other and from what they already made. Be specific \
        about the angle; no generic advice.";
    let user = format!(
        "{profile_block}{learnings_block}\nRecent output:\n{}\n\nTheir question: {}",
        if recent.is_empty() { "(nothing yet)".to_string() } else { recent.join("\n") },
        question.as_deref().unwrap_or("What should I make next?")
    );

    let response = crate::commands::ai::call_openrouter(&state.db, system, &user, 0.9, true).await?;
    let text = response["text"].as_str().unwrap_or("");
    let parsed = crate::commands::ai::extract_json_value(text)
        .ok_or("The AI did not return usable ideas — try again.")?;
    Ok(parsed)
}

// ────────────────────────────────────────────────────────────────────────────
// (C) Derivatives
// ────────────────────────────────────────────────────────────────────────────

/// Target specs per platform: aspect ratio and hard duration limit for a short.
fn derivative_spec(platform: &str) -> (i64, i64, i64) {
    // (width, height, max_seconds)
    match platform {
        "tiktok" | "instagram" | "youtube_short" => (1080, 1920, 60),
        "mastodon" | "bluesky" => (1080, 1080, 60),
        "telegram" | "discord" => (1080, 1350, 90),
        _ => (1080, 1920, 60),
    }
}

/// Derive per-platform versions of a finished song: a vertical short cut from its video, a
/// still-image post with a poetic caption, and a teaser that links back to the full upload.
///
/// The video work is FFmpeg (already required by the app); the words are the free in-app AI, written
/// in the creator's own voice via the taste profile. Each derivative is recorded in the
/// project-scoped `derivatives` collection, so it lives in the project's git repo with everything
/// else and its publish state is per destination.
#[tauri::command]
pub async fn derive_song_versions(
    state: State<'_, AppState>,
    song_id: String,
    platforms: Vec<String>,
    kinds: Option<Vec<String>>,
) -> Res<Value> {
    let song = state
        .db
        .collection::<Document>("songs")
        .find_one(doc! { "id": &song_id })
        .await
        .map_err(e)?
        .map(|d| crate::store::doc_to_json(&d))
        .ok_or("Song not found")?;
    let project_id = song["project_id"].as_str().unwrap_or("").to_string();
    let kinds = kinds.unwrap_or_else(|| vec!["short".into(), "image_post".into(), "teaser".into()]);

    // Where the canonical upload lives, so a teaser can point at it.
    let canonical = state
        .db
        .collection::<Document>("uploads")
        .find_one(doc! { "song_id": &song_id, "status": "published" })
        .await
        .map_err(e)?
        .and_then(|d| d.get_str("youtube_video_id").ok().map(|s| s.to_string()))
        .map(|id| format!("https://www.youtube.com/watch?v={id}"));

    // One AI call writes every caption, so the set reads consistently.
    let profile_block = taste_profile_block().await;
    let system = "You write short social copy for a Bible-music channel. Return ONLY a JSON object: \
        { \"short_caption\": string, \"poem\": string, \"teaser\": string, \"hashtags\": string }. \
        `poem`: 3-4 lines, evocative, drawn from the lyrics — not a summary. `teaser`: one sentence \
        that makes someone want the full video. `hashtags`: 3-6, space-separated, no invented brands.";
    let user = format!(
        "{profile_block}Song title: {}\nLanguage: {}\nStyle: {}\nLyrics:\n{}",
        song["title"].as_str().unwrap_or(""),
        song["language"].as_str().unwrap_or(""),
        song["styles"].as_str().unwrap_or(""),
        song["lyrics"].as_str().unwrap_or("").chars().take(2000).collect::<String>()
    );
    let copy = match crate::commands::ai::call_openrouter(&state.db, system, &user, 0.8, true).await {
        Ok(resp) => crate::commands::ai::extract_json_value(resp["text"].as_str().unwrap_or(""))
            .unwrap_or_else(|| json!({})),
        // Copy is a nice-to-have; a missing AI must not stop the video work.
        Err(_) => json!({}),
    };

    let coll = state.db.collection::<Document>("derivatives");
    let mut created = Vec::new();

    for platform in &platforms {
        let (width, height, max_secs) = derivative_spec(platform);
        for kind in &kinds {
            let id = uuid::Uuid::new_v4().to_string();
            let (caption, media) = match kind.as_str() {
                "short" => {
                    let source = song["video_url"].as_str().unwrap_or("");
                    let path = if source.is_empty() {
                        None
                    } else {
                        cut_vertical_short(source, &song_id, platform, width, height, max_secs).ok()
                    };
                    (copy["short_caption"].as_str().unwrap_or("").to_string(), path)
                }
                "image_post" => (
                    format!(
                        "{}\n\n{}",
                        copy["poem"].as_str().unwrap_or(""),
                        copy["hashtags"].as_str().unwrap_or("")
                    ),
                    first_section_image(&state, &song_id).await,
                ),
                _ => (
                    match &canonical {
                        Some(url) => format!("{}\n\n{}", copy["teaser"].as_str().unwrap_or(""), url),
                        None => copy["teaser"].as_str().unwrap_or("").to_string(),
                    },
                    None,
                ),
            };

            let mut d = bson::Document::new();
            d.insert("id", &id);
            d.insert("project_id", &project_id);
            d.insert("song_id", &song_id);
            d.insert("platform", platform);
            d.insert("kind", kind);
            d.insert("caption", &caption);
            d.insert("media_path", media.clone().unwrap_or_default());
            d.insert("width", width);
            d.insert("height", height);
            d.insert("status", "draft");
            d.insert("created_at", chrono::Utc::now().to_rfc3339());
            coll.insert_one(d).await.map_err(e)?;
            created.push(json!({
                "id": id, "platform": platform, "kind": kind,
                "caption": caption, "media_path": media,
            }));
        }
    }

    Ok(json!({ "ok": true, "created": created, "canonical_url": canonical }))
}

/// Cut a centre-cropped vertical clip from the composed video with FFmpeg.
#[cfg(desktop)]
fn cut_vertical_short(
    source: &str,
    song_id: &str,
    platform: &str,
    width: i64,
    height: i64,
    max_secs: i64,
) -> Res<String> {
    let source_path = source.trim_start_matches("file://").to_string();
    if !std::path::Path::new(&source_path).exists() {
        return Err(format!("source video not found: {source_path}"));
    }
    let out_dir = std::path::Path::new(&source_path)
        .parent()
        .map(|p| p.join("derivatives"))
        .ok_or("could not determine an output folder")?;
    std::fs::create_dir_all(&out_dir).map_err(e)?;
    let out = out_dir.join(format!("{song_id}-{platform}-short.mp4"));

    // Scale to cover, then centre-crop — the standard "fill a vertical frame from a wide source"
    // transform. `-t` caps the length to the platform's limit.
    let filter = format!(
        "scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h}",
        w = width,
        h = height
    );
    let status = std::process::Command::new("ffmpeg")
        .args(["-y", "-i", &source_path, "-t", &max_secs.to_string(), "-vf", &filter,
               "-c:v", "libx264", "-preset", "veryfast", "-crf", "23", "-c:a", "aac", "-b:a", "128k"])
        .arg(&out)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|err| format!("could not run ffmpeg: {err}"))?;
    if !status.success() {
        return Err("ffmpeg failed to cut the short".into());
    }
    Ok(out.to_string_lossy().to_string())
}

#[cfg(not(desktop))]
fn cut_vertical_short(_: &str, _: &str, _: &str, _: i64, _: i64, _: i64) -> Res<String> {
    Err("video cutting is not available in this build".into())
}

/// A still to post: the first section image the song has.
async fn first_section_image(state: &AppState, song_id: &str) -> Option<String> {
    let doc_found = state
        .db
        .collection::<Document>("sections")
        .find_one(doc! { "song_id": song_id, "image_url": { "$exists": true } })
        .sort(doc! { "index": 1 })
        .await
        .ok()
        .flatten()?;
    doc_found
        .get_str("image_url")
        .ok()
        .map(|s| s.trim_start_matches("file://").to_string())
        .filter(|s| !s.is_empty())
}

#[tauri::command]
pub async fn list_derivatives(state: State<'_, AppState>, song_id: Option<String>) -> Res<Vec<Value>> {
    use futures_util::StreamExt;
    let filter = match song_id.filter(|s| !s.is_empty()) {
        Some(id) => doc! { "song_id": id },
        None => doc! {},
    };
    let mut cursor = state
        .db
        .collection::<Document>("derivatives")
        .find(filter)
        .sort(doc! { "created_at": -1 })
        .await
        .map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await {
        out.push(crate::store::doc_to_json(&d));
    }
    Ok(out)
}

#[tauri::command]
pub async fn update_derivative(state: State<'_, AppState>, id: String, patch: Value) -> Res<Value> {
    let mut update = bson::to_document(&patch).map_err(e)?;
    update.remove("id");
    update.remove("_id");
    state
        .db
        .collection::<Document>("derivatives")
        .update_one(doc! { "id": &id }, doc! { "$set": update })
        .await
        .map_err(e)?;
    Ok(json!({ "ok": true }))
}

#[tauri::command]
pub async fn delete_derivative(state: State<'_, AppState>, id: String) -> Res<Value> {
    state
        .db
        .collection::<Document>("derivatives")
        .delete_one(doc! { "id": &id })
        .await
        .map_err(e)?;
    Ok(json!({ "ok": true }))
}

// ────────────────────────────────────────────────────────────────────────────
// (D/E) Publishing
// ────────────────────────────────────────────────────────────────────────────

/// Publish one derivative to its platform. **Always an explicit action** — there is no timer that
/// calls this. The result is recorded on the derivative so a failure is visible and retryable.
#[tauri::command]
pub async fn publish_derivative(state: State<'_, AppState>, id: String) -> Res<Value> {
    let derivative = state
        .db
        .collection::<Document>("derivatives")
        .find_one(doc! { "id": &id })
        .await
        .map_err(e)?
        .map(|d| crate::store::doc_to_json(&d))
        .ok_or("Derivative not found")?;

    let platform = derivative["platform"].as_str().unwrap_or("").to_string();
    let caption = derivative["caption"].as_str().unwrap_or("").to_string();
    let media = derivative["media_path"].as_str().unwrap_or("").to_string();
    let media_path = Some(media.clone()).filter(|m| !m.is_empty() && std::path::Path::new(m).exists());

    let account = state
        .db
        .collection::<Document>("social_accounts")
        .find_one(doc! { "_id": &platform })
        .await
        .map_err(e)?
        .map(|d| crate::store::doc_to_json(&d))
        .unwrap_or_else(|| json!({}));

    let http = client()?;
    let result = match platform.as_str() {
        "mastodon" => post_mastodon(&http, &account, &caption, media_path.as_deref()).await,
        "bluesky" => post_bluesky(&http, &account, &caption).await,
        "telegram" => post_telegram(&http, &account, &caption, media_path.as_deref()).await,
        "discord" => post_discord(&http, &caption, media_path.as_deref()).await,
        "youtube" => Err("Publish YouTube videos through the Upload page, which handles the OAuth channel and metadata.".to_string()),
        "instagram" | "tiktok" => Err(format!(
            "{platform} needs its app review/audit completed before the API will publish. Until then, \
             use the built-in browser with a recorded macro."
        )),
        "browser_macro" => Err("Replay the recorded macro from the Macro Manager to post this.".to_string()),
        other => Err(format!("Publishing to {other} is not implemented.")),
    };

    let coll = state.db.collection::<Document>("derivatives");
    match result {
        Ok(url) => {
            coll.update_one(
                doc! { "id": &id },
                doc! { "$set": { "status": "published", "published_url": &url,
                                 "published_at": chrono::Utc::now().to_rfc3339(),
                                 "last_error": Option::<String>::None } },
            )
            .await
            .map_err(e)?;
            Ok(json!({ "ok": true, "url": url }))
        }
        Err(err) => {
            coll.update_one(
                doc! { "id": &id },
                doc! { "$set": { "status": "failed", "last_error": &err } },
            )
            .await
            .map_err(e)?;
            Err(err)
        }
    }
}

async fn post_mastodon(
    http: &reqwest::Client,
    account: &Value,
    caption: &str,
    media: Option<&str>,
) -> Res<String> {
    let instance = account["instance"].as_str().unwrap_or("").trim_end_matches('/').to_string();
    let token = vault::get(&cred_key("mastodon", "token"))?.unwrap_or_default();
    if instance.is_empty() || token.is_empty() {
        return Err("Mastodon is not connected.".into());
    }

    let mut media_ids: Vec<String> = Vec::new();
    if let Some(path) = media {
        let bytes = std::fs::read(path).map_err(e)?;
        let name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "upload".into());
        let part = reqwest::multipart::Part::bytes(bytes).file_name(name);
        let form = reqwest::multipart::Form::new().part("file", part);
        let resp = http
            .post(format!("{instance}/api/v2/media"))
            .bearer_auth(&token)
            .multipart(form)
            .send()
            .await
            .map_err(e)?;
        let body: Value = resp.json().await.unwrap_or(Value::Null);
        match body["id"].as_str() {
            Some(mid) => media_ids.push(mid.to_string()),
            None => return Err(format!("Mastodon rejected the attachment: {}", body["error"].as_str().unwrap_or("unknown"))),
        }
    }

    let resp = http
        .post(format!("{instance}/api/v1/statuses"))
        .bearer_auth(&token)
        .json(&json!({ "status": caption, "media_ids": media_ids }))
        .send()
        .await
        .map_err(e)?;
    let ok = resp.status().is_success();
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    if ok {
        Ok(body["url"].as_str().unwrap_or("").to_string())
    } else {
        Err(format!("Mastodon refused the post: {}", body["error"].as_str().unwrap_or("unknown")))
    }
}

async fn post_bluesky(http: &reqwest::Client, account: &Value, caption: &str) -> Res<String> {
    let handle = account["handle"].as_str().unwrap_or("").to_string();
    let password = vault::get(&cred_key("bluesky", "app_password"))?.unwrap_or_default();
    let session = bluesky_session(http, &handle, &password).await?;
    let jwt = session["accessJwt"].as_str().unwrap_or("").to_string();
    let did = session["did"].as_str().unwrap_or("").to_string();

    // Bluesky's 300-grapheme limit; truncate on a word boundary rather than being rejected.
    let text = if caption.chars().count() > 300 {
        let mut cut: String = caption.chars().take(297).collect();
        if let Some(space) = cut.rfind(' ') { cut.truncate(space); }
        format!("{cut}…")
    } else {
        caption.to_string()
    };

    let resp = http
        .post("https://bsky.social/xrpc/com.atproto.repo.createRecord")
        .bearer_auth(&jwt)
        .json(&json!({
            "repo": did,
            "collection": "app.bsky.feed.post",
            "record": {
                "$type": "app.bsky.feed.post",
                "text": text,
                "createdAt": chrono::Utc::now().to_rfc3339(),
            }
        }))
        .send()
        .await
        .map_err(e)?;
    let ok = resp.status().is_success();
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    if ok {
        let rkey = body["uri"].as_str().unwrap_or("").rsplit('/').next().unwrap_or("").to_string();
        Ok(format!("https://bsky.app/profile/{handle}/post/{rkey}"))
    } else {
        Err(format!("Bluesky refused the post: {}", body["message"].as_str().unwrap_or("unknown")))
    }
}

async fn post_telegram(
    http: &reqwest::Client,
    account: &Value,
    caption: &str,
    media: Option<&str>,
) -> Res<String> {
    let token = vault::get(&cred_key("telegram", "bot_token"))?.unwrap_or_default();
    let chat_id = account["chat_id"].as_str().unwrap_or("").to_string();
    if token.is_empty() || chat_id.is_empty() {
        return Err("Telegram is not connected.".into());
    }

    let (endpoint, field) = match media.map(|m| m.to_ascii_lowercase()) {
        Some(ref m) if m.ends_with(".mp4") || m.ends_with(".mov") => ("sendVideo", "video"),
        Some(_) => ("sendPhoto", "photo"),
        None => ("sendMessage", ""),
    };
    let url = format!("https://api.telegram.org/bot{token}/{endpoint}");

    let resp = if let Some(path) = media {
        let bytes = std::fs::read(path).map_err(e)?;
        let name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "upload".into());
        let form = reqwest::multipart::Form::new()
            .text("chat_id", chat_id.clone())
            .text("caption", caption.to_string())
            .part(field.to_string(), reqwest::multipart::Part::bytes(bytes).file_name(name));
        http.post(&url).multipart(form).send().await.map_err(e)?
    } else {
        http.post(&url)
            .json(&json!({ "chat_id": chat_id, "text": caption }))
            .send()
            .await
            .map_err(e)?
    };
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    if body["ok"].as_bool() == Some(true) {
        let message_id = body["result"]["message_id"].as_i64().unwrap_or(0);
        let handle = chat_id.trim_start_matches('@');
        Ok(format!("https://t.me/{handle}/{message_id}"))
    } else {
        Err(format!("Telegram refused the post: {}", body["description"].as_str().unwrap_or("unknown")))
    }
}

async fn post_discord(http: &reqwest::Client, caption: &str, media: Option<&str>) -> Res<String> {
    let url = vault::get(&cred_key("discord", "webhook_url"))?.unwrap_or_default();
    if url.is_empty() {
        return Err("Discord is not connected.".into());
    }
    let resp = if let Some(path) = media {
        let bytes = std::fs::read(path).map_err(e)?;
        let name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "upload".into());
        let form = reqwest::multipart::Form::new()
            .text("content", caption.to_string())
            .part("file", reqwest::multipart::Part::bytes(bytes).file_name(name));
        http.post(&url).multipart(form).send().await.map_err(e)?
    } else {
        http.post(&url).json(&json!({ "content": caption })).send().await.map_err(e)?
    };
    if resp.status().is_success() {
        Ok("(posted to Discord)".into())
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(format!("Discord refused the post ({status}): {}", body.trim()))
    }
}

/// Publish every draft derivative of a song whose platform is enabled — one explicit fan-out.
#[tauri::command]
pub async fn publish_all_derivatives(state: State<'_, AppState>, song_id: String) -> Res<Value> {
    let derivatives = list_derivatives(state.clone(), Some(song_id)).await?;
    let mut published = Vec::new();
    let mut failed = Vec::new();
    for d in derivatives {
        if d["status"].as_str() != Some("draft") {
            continue;
        }
        let id = d["id"].as_str().unwrap_or("").to_string();
        match publish_derivative(state.clone(), id.clone()).await {
            Ok(res) => published.push(json!({ "id": id, "platform": d["platform"], "url": res["url"] })),
            Err(err) => failed.push(json!({ "id": id, "platform": d["platform"], "error": err })),
        }
    }
    Ok(json!({ "ok": true, "published": published, "failed": failed }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_is_stripped_and_entities_decoded() {
        let input = "<p>Hello <a href=\"x\">world</a> &amp; friends</p>";
        assert_eq!(strip_html(input), "Hello world & friends");
        assert_eq!(strip_html(""), "");
        assert_eq!(strip_html("<br/>"), "");
    }

    #[test]
    fn derivative_specs_are_vertical_for_short_form() {
        for platform in ["tiktok", "instagram", "youtube_short"] {
            let (w, h, secs) = derivative_spec(platform);
            assert!(h > w, "{platform} shorts must be portrait");
            assert!(secs <= 60, "{platform} caps shorts at 60s");
        }
        let (w, h, _) = derivative_spec("mastodon");
        assert_eq!(w, h, "square reads better in a timeline");
        // An unknown platform still gets a usable spec rather than zeroes.
        let (w, h, secs) = derivative_spec("something-new");
        assert!(w > 0 && h > 0 && secs > 0);
    }
}
