# Getting access to other people's platforms

Checked **July 2026**. Every step here is something *you* have to do in someone else's dashboard — no
app can do it for you. What this app can do is put the exact page in front of you, in order, with the
values you need already on the clipboard. That is what the in-app guides do; this file is the content
behind them.

Two findings up front, because they change what is worth doing at all.

## Finding 1 — you may not need Meta App Review

App Review is required to publish **on behalf of accounts you do not own**. Publishing to *your own*
Instagram account, from an app where your account holds a role, is a different situation: an app in
Development mode can call the API for accounts that have a role on it.

So the order that saves weeks is:

1. Build the app in Development mode, add your own Instagram account, publish to it.
2. Only submit for App Review if you later need to publish for accounts that are not yours.

Verify this against Meta's current terms for your case before relying on it — the distinction is
between "your own account with a role on the app" and "any account", and Meta words it carefully.

## Finding 2 — TikTok has a usable path with no audit at all

TikTok has two scopes, and only one of them needs the audit:

| Scope | What it does | Audit needed? |
|---|---|---|
| `video.upload` | Puts the video in the creator's TikTok **inbox/drafts**. They tap once to post. | **No** |
| `video.publish` | Posts directly, live, no human step. | **Yes** — 2–4 weeks, multiple rounds |

Until an app passes audit, everything it posts is **forced to private visibility**. So "direct posting
without an audit" does not exist; the choice is between a draft the creator confirms, and a private
post nobody sees.

For a studio generating a video a day, `video.upload` is the honest answer: the app does the work, the
phone does one tap. That is worth building *before* the audit, not after.

---

## Instagram — publishing to a Professional account

**What you need, all of it, before any code works:**

- a Facebook **Business account**
- a Facebook **Page**, linked to the Instagram account
- an Instagram **Professional** account (Business or Creator) — a personal account cannot be published
  to by any third-party app, ever; it has to be converted first
- a **Meta developer app** (developers.facebook.com)
- permissions: `instagram_business_basic` and `instagram_business_content_publish`

**The permission names changed.** Older guides say `instagram_basic` and `instagram_content_publish`.
Use the `instagram_business_*` names.

**How publishing works** — two calls, not one:

```
POST /{ig-user-id}/media           → returns a container id
POST /{ig-user-id}/media_publish   → publishes that container
```

**Rate limit:** 200 API calls per user per hour (Business Use Case limits). A daily upload is nowhere
near that; a backfill of fifty chapters is, so pace it.

**If you do need App Review:** each permission is a separate submission, each needs a **screencast
showing your app using that specific permission in context**, and Meta's documented timeline is 2–4
weeks per submission. The screencast is the part people fail — it must show the real flow, not a
mockup, including the login.

### Steps, in order

1. **facebook.com/business** — create or confirm your Business account.
2. **Your Facebook Page** → link the Instagram account under Page settings → Linked accounts.
3. **Instagram app** → Settings → Account type → switch to Professional (Business or Creator).
4. **developers.facebook.com/apps** → Create app → type **Business**.
5. In the app: add the **Instagram** product.
6. **App roles** → add yourself/your Instagram account, so Development mode can publish to it.
7. **Generate a token** with `instagram_business_basic` + `instagram_business_content_publish`.
8. Paste the token into the app (Social Presence → Instagram).
9. Publish to your own account and confirm it lands.
10. *Only if you need other people's accounts:* App Review, one submission per permission, with the
    screencast.

---

## TikTok — content posting

1. **developers.tiktok.com** → register, create an app.
2. Add the **Content Posting API** product.
3. Request the **`video.upload`** scope. This is the one that works immediately.
4. Provide a **privacy policy URL** — TikTok requires one even for the unaudited path, and it must be
   reachable.
5. Connect the account in the app and post to drafts.
6. *Only if you want direct posting:* request `video.publish` and submit for audit. That needs a
   privacy policy URL, a **demo video of the complete OAuth and upload flow**, a description of how
   user data is handled, and a compliance confirmation. A clean first submission clears in roughly
   1–2 weeks; expect rounds of feedback.

---

## YouTube — already working, and the one extra scope worth having

YouTube upload works today through the OAuth flow the app already implements. One scope is worth
adding later, opt-in:

- `yt-analytics.readonly` — lets the app read **when your viewers are actually on YouTube**, which is
  the only authoritative answer to "what time should this channel publish". Without it, the app uses
  an AI-researched regional estimate, which is a guess with good reasoning behind it.

Adding the scope means re-consenting, so it is offered per channel rather than forced on everyone.

---

## The platforms with no API worth waiting for

Reddit, Pinterest, LinkedIn, X, Tumblr, DEV, WordPress: all have APIs, all are already wired.
Everything else — and Instagram/TikTok until the above is done — goes through a **recorded browser
macro**: the app opens the site in its own webview, you record the posting flow once, and it replays.

That is not a fallback to apologise for. A macro that pastes prepared values into a real form, in
order, is how a person does it anyway; the app just does not get bored. The pieces it needs are all
built: the clipboard queue holds the prepared values, `paste-queue` steps take them one at a time, and
the macro author can draft the steps from a page digest.

---

## What the in-app guide adds over this file

- The exact page open **beside** the steps, in the app's own webview, so there is no alt-tabbing.
- The values you need to paste already on the clipboard, in order, via the paste queue.
- A **capture** button that saves a screenshot from *your* session into the guide, so the guide becomes
  accurate for your account instead of showing someone else's dashboard from a year ago.
- Progress kept per platform, so a two-week wait for a review does not mean starting over.
