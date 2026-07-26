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
// Getting permission from other people's platforms
//
// Every token, key and API permission this app can use has to be created by the user, in someone
// else's dashboard, through a flow that changes without notice. No app can do that part. What it can do
// is stop the user from having to hold twelve tabs and a half-remembered blog post in their head.
//
// So a guide is: an ordered list of steps, each with the exact URL to open **in the app's own browser
// beside the steps**, the value to paste (already on the clipboard, in order), and a place to keep the
// screenshot the user takes of their own dashboard.
//
// Two things this deliberately does NOT do:
//
//   • It ships no screenshots of its own. A stock screenshot of somebody else's Meta dashboard from a
//     year ago is worse than no screenshot: it looks authoritative and is wrong. Instead the user
//     captures their own as they go, and the guide becomes accurate for their account.
//   • It does not claim a step is done. Progress is what the user marked, because the app cannot see
//     inside Meta's review queue.
//
// The facts in these steps were checked in July 2026 and are written up with their sources in
// docs/THIRD_PARTY_ACCESS.md. Two of them change what is worth doing at all: Meta App Review is
// avoidable when publishing only to your own account (an app in Development mode can post to accounts
// that hold a role on it), and TikTok's `video.upload` scope — video into the creator's drafts — needs
// no audit at all, while `video.publish` forces every post private until the audit passes.
// ────────────────────────────────────────────────────────────────

/// One step of a guide.
///
/// `url` is opened in the app's browser next to the text. `copy` is put on the clipboard when the step
/// opens, because every one of these flows involves pasting something into a form.
struct Step {
    id: &'static str,
    title: &'static str,
    /// Markdown. Rendered next to the live page.
    body: &'static str,
    url: &'static str,
    copy: &'static str,
    /// True when the step is waiting on somebody else (a review queue), so the UI can say "waiting"
    /// rather than "not done".
    waiting: bool,
}

struct Guide {
    id: &'static str,
    platform: &'static str,
    label: &'static str,
    summary: &'static str,
    /// What you can do the moment the guide is finished, as opposed to eventually.
    outcome: &'static str,
    steps: &'static [Step],
}

const INSTAGRAM: &[Step] = &[
    Step {
        id: "professional",
        title: "Make the Instagram account Professional",
        body: "A **personal** Instagram account can never be published to by any third-party app. It \
               has to be Business or Creator first.\n\n\
               In the Instagram app: **Settings → Account type and tools → Switch to professional \
               account**. Creator is fine; Business is fine. This is reversible.",
        url: "https://help.instagram.com/502981923235522",
        copy: "",
        waiting: false,
    },
    Step {
        id: "page",
        title: "Link it to a Facebook Page",
        body: "The publishing API reaches Instagram *through* a Facebook Page. Without the link, the \
               API cannot see the account at all.\n\n\
               On your Page: **Settings → Linked accounts → Instagram → Connect**.",
        url: "https://www.facebook.com/pages/",
        copy: "",
        waiting: false,
    },
    Step {
        id: "app",
        title: "Create a Meta developer app",
        body: "Choose type **Business**. Name it after this studio so you recognise it later.\n\n\
               Then add the **Instagram** product to it.",
        url: "https://developers.facebook.com/apps/",
        copy: "",
        waiting: false,
    },
    Step {
        id: "role",
        title: "Give your own account a role on the app",
        body: "**This is the step that saves weeks.**\n\n\
               App Review is required to publish on behalf of accounts you do *not* own. An app in \
               **Development mode** can already publish to accounts that hold a role on it — yours.\n\n\
               **App roles → Add people →** add yourself. Then publishing to your own account works \
               now, and App Review becomes optional rather than a gate.\n\n\
               Check Meta's current wording for your case before relying on it; the distinction is \
               between \"an account with a role on the app\" and \"any account\".",
        url: "https://developers.facebook.com/apps/",
        copy: "",
        waiting: false,
    },
    Step {
        id: "token",
        title: "Generate a token with the two permissions",
        body: "You need exactly these:\n\n\
               - `instagram_business_basic`\n\
               - `instagram_business_content_publish`\n\n\
               **The names changed.** Older guides — and anything written from memory — say \
               `instagram_basic` and `instagram_content_publish`. Those are retired.\n\n\
               Both are on the clipboard now; paste them into the permission picker one at a time.",
        url: "https://developers.facebook.com/tools/explorer/",
        copy: "instagram_business_basic\ninstagram_business_content_publish",
        waiting: false,
    },
    Step {
        id: "paste",
        title: "Paste the token into the app",
        body: "**Social Presence → Instagram → paste the token.** The app verifies it immediately and \
               tells you which account it can see.\n\n\
               Publishing is two calls, which is why a failure can leave a container that never went \
               live: the app creates the media container, then publishes it. It reports which of the \
               two failed.\n\n\
               Rate limit: **200 calls per user per hour**. A daily upload is nowhere near it; a \
               backfill of fifty chapters is, so the app paces those.",
        url: "",
        copy: "",
        waiting: false,
    },
    Step {
        id: "review",
        title: "Only if you need other people's accounts: App Review",
        body: "Skip this unless you are publishing to accounts that are not yours.\n\n\
               Each permission is a **separate submission**, and each needs a **screencast of your app \
               using that specific permission in context** — the real flow, including the login, not a \
               mockup. That is the part submissions fail on.\n\n\
               Meta's documented timeline is **2–4 weeks per submission**.",
        url: "https://developers.facebook.com/docs/app-review",
        copy: "",
        waiting: true,
    },
];

const TIKTOK: &[Step] = &[
    Step {
        id: "register",
        title: "Register as a TikTok developer",
        body: "Create an account, then an app. Name it after this studio.",
        url: "https://developers.tiktok.com/",
        copy: "",
        waiting: false,
    },
    Step {
        id: "product",
        title: "Add the Content Posting API",
        body: "In your app: add the **Content Posting API** product.",
        url: "https://developers.tiktok.com/doc/content-posting-api-get-started",
        copy: "",
        waiting: false,
    },
    Step {
        id: "scope",
        title: "Request `video.upload` — not `video.publish`",
        body: "This is the important choice, and the docs make it easy to get wrong.\n\n\
               | Scope | What it does | Audit? |\n\
               |---|---|---|\n\
               | `video.upload` | Video lands in **your TikTok drafts**. You tap once to post. | **No** |\n\
               | `video.publish` | Posts straight to live, no human step. | **Yes**, 2–4 weeks |\n\n\
               Until an app passes audit, **every post it makes is forced to private visibility**. So \
               \"direct posting without an audit\" does not exist — the real choice is a draft you \
               confirm, or a private post nobody sees.\n\n\
               For a video a day, `video.upload` is the honest answer: the app does the work, your \
               phone does one tap.",
        url: "https://developers.tiktok.com/doc/content-posting-api-reference-direct-post",
        copy: "video.upload",
        waiting: false,
    },
    Step {
        id: "privacy",
        title: "Provide a privacy policy URL",
        body: "TikTok requires one **even for the unaudited path**, and it has to be reachable — they \
               check. Any hosted page describing what you do with the data is enough; it must not be a \
               placeholder.",
        url: "",
        copy: "",
        waiting: false,
    },
    Step {
        id: "connect",
        title: "Connect the account here",
        body: "**Social Presence → TikTok → connect.** From then on the app uploads to your drafts.",
        url: "",
        copy: "",
        waiting: false,
    },
    Step {
        id: "audit",
        title: "Only if you want direct posting: the audit",
        body: "Needs the privacy policy URL, a **demo video of the complete OAuth and upload flow**, a \
               description of how you handle user data, and a compliance confirmation.\n\n\
               A clean first submission clears in roughly **1–2 weeks**; expect rounds of feedback.",
        url: "https://developers.tiktok.com/doc/content-sharing-guidelines",
        copy: "",
        waiting: true,
    },
];

const YOUTUBE_ANALYTICS: &[Step] = &[
    Step {
        id: "why",
        title: "What this adds",
        body: "The app can already upload. This adds one read-only scope so it can see **when your \
               viewers are actually on YouTube** — the only authoritative answer to what time a \
               channel should publish. Without it, the app uses an AI-researched regional estimate: \
               reasoned, but a guess.",
        url: "",
        copy: "",
        waiting: false,
    },
    Step {
        id: "scope",
        title: "Add the analytics scope to your OAuth client",
        body: "Add this scope to the consent screen of the client this app already uses:\n\n\
               `https://www.googleapis.com/auth/yt-analytics.readonly`\n\n\
               It is on the clipboard. It is read-only — it cannot change anything on the channel.",
        url: "https://console.cloud.google.com/apis/credentials/consent",
        copy: "https://www.googleapis.com/auth/yt-analytics.readonly",
        waiting: false,
    },
    Step {
        id: "reconsent",
        title: "Re-connect the channel",
        body: "Adding a scope means the existing sign-in does not carry it. **Channel Manager → \
               Re-connect** on each channel you want measured times for. Channels you skip keep using \
               the estimate.",
        url: "",
        copy: "",
        waiting: false,
    },
];

const PRINTIFY: &[Step] = &[
    Step {
        id: "store",
        title: "Add a storefront — the free one first",
        body: "Printify's own **Pop-Up Store** is free, needs no external account, and charges nothing \
               per listing. It is the only storefront where publishing something every day costs \
               nothing.\n\n\
               Etsy charges **$0.20 per listing**, which a product a day turns into a monthly bill \
               before a single sale. Worth having, worth knowing.",
        url: "https://printify.com/app/store-management",
        copy: "",
        waiting: false,
    },
    Step {
        id: "token",
        title: "Create a Personal Access Token",
        body: "**My Profile → Connections → Generate token.** Printify's API is real and documented, \
               unlike every distributor and ebook store in this app — this token is all it needs.",
        url: "https://printify.com/app/account/connections",
        copy: "",
        waiting: false,
    },
    Step {
        id: "paste",
        title: "Paste it into Settings",
        body: "**Settings → Print on demand.** Then **Print on Demand → Shop → Connect** lists the \
               shops the token can reach.",
        url: "",
        copy: "",
        waiting: false,
    },
];

const GUIDES: &[Guide] = &[
    Guide {
        id: "instagram", platform: "instagram", label: "Instagram publishing",
        summary: "Publish to your own Instagram account without App Review.",
        outcome: "The app posts to Instagram. App Review is only needed for accounts that are not yours.",
        steps: INSTAGRAM,
    },
    Guide {
        id: "tiktok", platform: "tiktok", label: "TikTok publishing",
        summary: "Upload straight into your TikTok drafts — no audit required.",
        outcome: "The app puts finished videos in your TikTok drafts; you tap once to post.",
        steps: TIKTOK,
    },
    Guide {
        id: "yt_analytics", platform: "youtube", label: "YouTube Analytics (publication times)",
        summary: "One read-only scope, so publication times come from your real audience.",
        outcome: "Publication times measured from your own viewers instead of estimated.",
        steps: YOUTUBE_ANALYTICS,
    },
    Guide {
        id: "printify", platform: "printify", label: "Printify (print on demand)",
        summary: "A token and a free storefront.",
        outcome: "Products created and published from the app.",
        steps: PRINTIFY,
    },
];

fn guide(id: &str) -> Option<&'static Guide> {
    GUIDES.iter().find(|g| g.id == id)
}

fn step_json(s: &Step) -> Value {
    json!({
        "id": s.id, "title": s.title, "body": s.body,
        "url": s.url, "copy": s.copy, "waiting": s.waiting,
        // A step with values to paste can prime the clipboard queue; the UI shows that as an action.
        "copy_lines": s.copy.lines().filter(|l| !l.trim().is_empty()).collect::<Vec<_>>(),
    })
}

/// Every guide, with how far the user has got in each.
#[tauri::command]
pub async fn list_access_guides(state: State<'_, AppState>) -> Res<Value> {
    use futures_util::StreamExt;
    let mut progress: std::collections::HashMap<String, Value> = Default::default();
    if let Ok(mut cursor) = state.db.collection::<Document>("access_progress").find(doc! {}).await {
        while let Some(Ok(d)) = cursor.next().await {
            let p = bson_to_value(d);
            if let Some(id) = p["guide"].as_str() { progress.insert(id.to_string(), p); }
        }
    }
    let guides: Vec<Value> = GUIDES.iter().map(|g| {
        let p = progress.get(g.id);
        let done: Vec<String> = p.and_then(|p| p["done"].as_array().cloned()).unwrap_or_default()
            .iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
        json!({
            "id": g.id, "platform": g.platform, "label": g.label,
            "summary": g.summary, "outcome": g.outcome,
            "steps": g.steps.len(),
            "done": done.len(),
            "complete": g.steps.iter().filter(|s| !s.waiting).all(|s| done.contains(&s.id.to_string())),
        })
    }).collect();
    Ok(json!({ "guides": guides }))
}

/// One guide's full content, plus what has been marked done and captured.
#[tauri::command]
pub async fn access_guide(state: State<'_, AppState>, id: String) -> Res<Value> {
    let g = guide(&id).ok_or_else(|| format!("no guide called '{id}'"))?;
    let p = state.db.collection::<Document>("access_progress")
        .find_one(doc! { "guide": &id }).await.map_err(e)?
        .map(bson_to_value).unwrap_or(json!({ "done": [], "captures": {}, "notes": {} }));
    Ok(json!({
        "id": g.id, "platform": g.platform, "label": g.label,
        "summary": g.summary, "outcome": g.outcome,
        "steps": g.steps.iter().map(step_json).collect::<Vec<_>>(),
        "done": p["done"],
        "captures": p["captures"],
        "notes": p["notes"],
    }))
}

#[derive(serde::Deserialize)]
pub struct StepRequest {
    pub guide: String,
    pub step: String,
    pub done: bool,
}

/// Mark a step done, or undo that.
///
/// Progress is only ever what the user marked. The app cannot see inside Meta's review queue, and
/// pretending to know would be worse than asking.
#[tauri::command]
pub async fn set_access_step(state: State<'_, AppState>, payload: StepRequest) -> Res<Value> {
    let existing = state.db.collection::<Document>("access_progress")
        .find_one(doc! { "guide": &payload.guide }).await.map_err(e)?
        .map(bson_to_value).unwrap_or(json!({}));
    let mut done: Vec<String> = existing["done"].as_array().cloned().unwrap_or_default()
        .iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
    done.retain(|s| s != &payload.step);
    if payload.done { done.push(payload.step.clone()); }

    let d = bson::to_bson(&done).map_err(e)?;
    state.db.collection::<Document>("access_progress")
        .update_one(doc! { "guide": &payload.guide },
                    doc! { "$set": { "guide": &payload.guide, "done": d,
                                     "updated_at": crate::models::now_iso() } })
        .upsert(true).await.map_err(e)?;
    Ok(json!({ "guide": payload.guide, "done": done }))
}

#[derive(serde::Deserialize)]
pub struct CaptureRequest {
    pub guide: String,
    pub step: String,
    /// PNG bytes, base64. Empty removes the capture.
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

/// Keep a screenshot of *your own* dashboard against a step, and/or a note.
///
/// This is why the guide ships no screenshots of its own: a stock image of somebody else's Meta
/// dashboard from a year ago looks authoritative and is wrong. Your own capture is accurate for your
/// account, and still accurate when you come back after a two-week review.
#[tauri::command]
pub async fn save_access_capture(state: State<'_, AppState>, payload: CaptureRequest) -> Res<Value> {
    let dir = state.db.global_root().join("access-captures").join(sanitise(&payload.guide));
    std::fs::create_dir_all(&dir).map_err(e)?;

    let mut set = Document::new();
    set.insert("guide", &payload.guide);
    set.insert("updated_at", crate::models::now_iso());

    if let Some(b64) = payload.image.as_deref().filter(|b| !b.is_empty()) {
        let bytes = base64_decode(b64).ok_or("the capture was not valid base64")?;
        let path = dir.join(format!("{}.png", sanitise(&payload.step)));
        std::fs::write(&path, &bytes).map_err(e)?;
        set.insert(format!("captures.{}", sanitise(&payload.step)), path.to_string_lossy().to_string());
    }
    if let Some(note) = payload.note.as_deref() {
        set.insert(format!("notes.{}", sanitise(&payload.step)), note);
    }
    state.db.collection::<Document>("access_progress")
        .update_one(doc! { "guide": &payload.guide }, doc! { "$set": set })
        .upsert(true).await.map_err(e)?;
    Ok(json!({ "saved": true, "guide": payload.guide, "step": payload.step }))
}

/// A key that cannot escape its folder or collide across guides.
fn sanitise(s: &str) -> String {
    let out: String = s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() { "step".into() } else { trimmed.chars().take(48).collect() }
}

/// Standard base64 with padding, as a browser's `toDataURL` produces it.
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    // Accept a full data: URL as well as bare base64 — the frontend has both to hand.
    let body = s.rsplit(',').next().unwrap_or(s);
    let table = |c: u8| -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity(body.len() / 4 * 3);
    let mut acc: u32 = 0;
    let mut bits = 0;
    for &c in body.as_bytes() {
        if c == b'=' || c.is_ascii_whitespace() { continue; }
        let v = table(c)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_guide_ships_a_screenshot_of_someone_elses_dashboard() {
        // The whole point of the capture feature. A stock image looks authoritative and is wrong.
        for g in GUIDES {
            for s in g.steps {
                assert!(!s.body.contains("!["), "{}/{} embeds an image", g.id, s.id);
            }
        }
    }

    #[test]
    fn the_two_findings_that_save_weeks_are_actually_in_the_guides() {
        let ig = guide("instagram").unwrap();
        let role = ig.steps.iter().find(|s| s.id == "role").unwrap();
        assert!(role.body.contains("Development mode"), "the dev-mode shortcut must be stated");
        // And the review step must be last and marked as waiting, not presented as a prerequisite.
        assert_eq!(ig.steps.last().unwrap().id, "review");
        assert!(ig.steps.last().unwrap().waiting);

        let tt = guide("tiktok").unwrap();
        let scope = tt.steps.iter().find(|s| s.id == "scope").unwrap();
        assert!(scope.body.contains("video.upload"));
        assert!(scope.body.contains("forced to private"), "the audit's real consequence: {}", scope.body);
        assert_eq!(scope.copy, "video.upload", "the scope that needs no audit is the one prefilled");
    }

    #[test]
    fn the_retired_instagram_permission_names_are_not_the_ones_prefilled() {
        let ig = guide("instagram").unwrap();
        let token = ig.steps.iter().find(|s| s.id == "token").unwrap();
        assert!(token.copy.contains("instagram_business_content_publish"));
        // The retired names may be *mentioned* as a warning, but must never be what gets pasted.
        for line in token.copy.lines() {
            assert!(line.starts_with("instagram_business_"), "would paste a retired name: {line}");
        }
    }

    #[test]
    fn a_step_key_cannot_escape_its_folder() {
        assert_eq!(sanitise("../../etc"), "etc");
        assert_eq!(sanitise("token"), "token");
        assert_eq!(sanitise(""), "step");
        assert!(!sanitise("a/b").contains('/'));
    }

    #[test]
    fn base64_round_trips_including_a_data_url_prefix() {
        assert_eq!(base64_decode("Zm9v").unwrap(), b"foo");
        assert_eq!(base64_decode("Zm9vYg==").unwrap(), b"foob");
        // A browser hands over the whole data: URL; taking it as-is must not corrupt the image.
        assert_eq!(base64_decode("data:image/png;base64,Zm9v").unwrap(), b"foo");
        assert_eq!(base64_decode("Zm 9v\n").unwrap(), b"foo", "whitespace is ignored");
        assert!(base64_decode("not base64!").is_none());
    }

    #[test]
    fn every_guide_says_what_you_get_at_the_end() {
        for g in GUIDES {
            assert!(!g.outcome.is_empty(), "{} has no stated outcome", g.id);
            assert!(!g.steps.is_empty());
            // Every step either explains something or points somewhere.
            for s in g.steps {
                assert!(s.body.len() > 40, "{}/{} is too thin to be a step", g.id, s.id);
            }
        }
    }
}
