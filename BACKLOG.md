# Backlog / Future Outlook

Forward-looking ideas, organized by horizon. These are **not commitments** — a menu of what would make the app more valuable, informed by what's already built (see [FEATURES.md](FEATURES.md)) and what's currently broken or missing (see [TODOS.md](TODOS.md)).

## Done (was: "Now — finish what's already 90% done")

As of the 2026-07-08 implementation pass, all four items formerly in this section are shipped and verified — see `STATUS.md`'s "Implementation pass" entry and `TODOS.md`'s "Fixed" section:

- ~~Wire up the Characters screen~~ — done, build-verified.
- ~~Fix "Check Node"~~ — done, `cargo check`-verified.
- ~~Land the git-versioning + Google Drive backup work~~ — verified end-to-end via direct git-command simulation; found and fixed a real bug in the process (the "JSON only" Drive backup wasn't actually excluding media — see `TODOS.md` #3). Still **uncommitted** in the working tree — committing it is a separate decision for whoever's driving the repo.
- ~~Reconcile version numbers~~ — done; `scripts/bump-version.sh` now also updates `Cargo.toml` so this can't drift again.

Retiring the dead CRA/craco config (the other half of the old version-numbers bullet) is **not yet done** — it's lower-risk to leave annotated (`deprecated???`) than to delete without the repo owner confirming nothing depends on it.

Also done in the same pass: ~~job cancellation that actually cancels~~ (`TODOS.md` #5) and ~~`bulk_generate_all_songs`/project-scoping~~ (`TODOS.md` #6) — both were originally called out below as near-term de-risking work but are now shipped.

**Implementation pass 3 (same day) also shipped**, on direct request:
- ~~Job Queue / rate limiting~~ — `AppState.job_semaphore`, configurable in Settings, gates `enqueue`/`retry_job`.
- ~~A scheduler~~ — not by parsing the old free-text `schedule` field (see `TODOS.md` #17 on the resulting duplication), but via a new structured `schedule_config`: daily/weekly automatic Bible-chapter progression → AI lyrics → draft song → auto-enqueued music generation, deliberately stopping short of auto-publish. Manual "Generate Now" trigger doubles as the "make basic daily lyrics generation more usable" ask.
- ~~Character consistency across a whole project~~ — project-level scoping now works, plus new `appearance_tags` baked into every Midjourney prompt for a character. **Not** done: linking a character to specific sections and one-click-inserting its prompt into those sections' `image_prompt` — see "Near-term" below.

## Near-term — de-risk the integrations that are "unofficial"

The app's core value (Suno music + Midjourney images) rides on session cookies and browser automation against services with no public API and no ToS blessing for this usage pattern. That's a real fragility:

- **Health/expiry surfacing**: the backend already tracks `suno_cookie_valid`/`suno_cookie_status` and re-checks every 15 minutes — surface this proactively in the UI (a persistent banner, not just the Settings test button) so a stale cookie doesn't silently fail a batch of queued jobs overnight.
- **Graceful multi-provider fallback for music generation**: wire the already-built `scripts/kaggle_riffusion/` project in as a second, free, ToS-friendlier music backend (`TODOS.md` #13) — even just as a manual "Riffusion mode" toggle in Settings — so a Suno cookie outage doesn't fully block the pipeline.
- **Character → section linking**: let a character be attached to specific sections of a song, with a one-click action that inserts its `image_prompt`/`appearance_tags` into those sections' `image_prompt`, so "consistent characters" actually flows through to "consistent generated images" without manual copy-paste.
- **Gate the scheduler's AI calls through the job queue too** — they currently run synchronously inside the 5-minute scheduler tick rather than as semaphore-gated jobs, so several due projects firing in the same tick will make their OpenRouter calls concurrently rather than queued.

## Mid-term — make multi-channel operation actually scale

The project's stated purpose is "topic → multi-language → multi-style → multi-channel," which implies dozens of channels and hundreds of upload combinations. A few things would make that tractable:

- **Reconcile the two schedule fields on Project** — the original free-text `schedule` (still shown on the Dashboard's creation form/cards) and the new structured `schedule_config` (drives actual automation) can say different things. Worth merging into one field/UI.
- **Bulk pipeline view**: a single screen showing, per project, how many songs are at each pipeline stage (draft → music → analyzed → images → video → uploaded) across all languages/styles, so a user managing 10 channels doesn't have to click through each song individually.
- **Upload analytics / performance feedback loop** — pull view counts / retention from the YouTube Data API (already OAuth-connected) back into the app, so channel/style/language combinations that underperform can be deprioritized. This was hinted at in the old PRD as a "P2: Qwen prompt → effect recommendation feedback loop."
- **Let the scheduler optionally progress further than music generation** (analysis/images) for users who want more automation than the current conservative default, behind an explicit opt-in — today it deliberately stops after enqueuing music so nothing reaches images/video/upload without a human in the loop.

## Longer-term / bigger bets

- **Cross-platform packaging** — `tauri.conf.json` currently only targets `deb` (Linux). macOS/Windows builds would meaningfully widen who can use this without a Linux box, especially since the core audience (Bible-content creators) skews non-technical.
- **Automated test coverage** (`TODOS.md` #14) — at minimum, Rust unit tests for the pure-logic pieces (`parse_annotations`, `derive_mood`, `suggest_effects`, the JSON-repair logic in `compose_lyrics`) and one end-to-end smoke test that exercises the full mock-free pipeline against fixtures, so a refactor of `jobs.rs` doesn't require manually re-testing every integration by hand.
- **Multi-user / team support** — everything today is single-machine, single-user (local Mongo, local OAuth client pool, local git repos). If this is ever meant to be used by a small team producing content together, that's a fundamentally different architecture (shared DB, shared credential vault, conflict resolution on the git-versioned projects).
- **In-app cost/quota visibility** — OpenRouter free-tier models, Suno's unofficial rate limits, and YouTube's daily upload quota are all real constraints the user currently discovers by trial and error; a lightweight "budget/quota" dashboard would reduce surprise failures.

## Requested 2026-07-24 — Social presence: self-knowledge, co-creation, co-publishing

Directly requested. The goal is to widen the app from "YouTube video factory" into a **personal media-presence engine**: it should learn who *you* are, use that to make better and more relevant content, automatically spin up smaller/alternate versions of each day's output, and co-publish them across many channels for marketing/publicity. Grounded in what the app already has — a bundled Playwright browser, the embedded child-webview + `bmChannel` injection bridge (see `memory/embedded-webview-csp-bridge`), OAuth loopback, free in-app AI (OpenRouter Qwen + Gemini), and the JSON learnings store — so most of this is reachable without new paid services. Organized under the five enhancement axes the request named.

### A. Get to know the user (self-profile that stays current)
- **Credential vault for social accounts.** Let the user store logins for their networks, used to sign in *through the built-in webview browser* (not a headless API), reusing the existing persistent-profile/session pattern already used for Suno/Midjourney. **Security is a hard requirement:** these are the user's real social passwords — encrypt at rest (OS keyring / a passphrase-derived key), never plaintext JSON, never logged, never sent to any AI provider. This is a prerequisite for everything below that needs an authenticated session.
- **Personal-feed ingestion via an OSS scraper.** Integrate a ready-made open-source scraper rather than building one. Candidates by surface:
  - *Instagram* — [Instaloader](https://github.com/instaloader/instaloader) (most-maintained OSS, ~12k★, but login-gated and rate-limited — account-ban risk, use the user's own session sparingly).
  - *Generic / ToS-fragile surfaces* — drive the **already-bundled Playwright** browser with the user's logged-in session (the flexible, no-extra-dep route; same fragility class as the current Suno/MJ automation).
  - *Open protocols (cheapest, no scraping)* — Mastodon and Bluesky/AT-Protocol expose the user's own timeline via clean public APIs; prefer these where the user has accounts.
  - AI-orchestrated: the local free AI plans *what* to pull and *prompt-enhances* extraction (e.g. "summarize the aesthetic of the last 50 posts this account liked"), so scraping is targeted, not a firehose.
- **Taste/profile model.** Feed ingested signals — own posts, who they follow, what they like, friends/audience — into a per-user JSON profile in the learnings store (mobile-friendly, no Mongo). This becomes creative-DNA context the AI reads alongside the Project Brief.

### B. AI assistance for more relevant, interesting content
- Wire the self-profile into the DailyGuide (`Tours.jsx`) and `compose_assist` context so angle suggestions, follow-up themes/topics, and ideation are grounded in the user's actual tastes and audience — not just the project brief.
- **Ideation partner**: a back-and-forth "what should I make next?" mode that reasons over the profile + recent performance ("your audience engages most with reflective Psalms shorts on Sunday evenings").
- **Macros**: recordable/replayable browser-automation macros (a captured Playwright/webview sequence) for repetitive posting/scraping flows the AI can then trigger — the semi-automated middle ground between a real API and full manual clicking.

### C. Co-creation of smaller / alternate versions (the "partner" auto-derivative feature)
- For each daily generation, automatically derive **shorts/micro-versions**: a vertical <60s cut of the video (TikTok/Reels/Shorts), a still-image + poetic-summary text post, a carousel, a "preview" teaser linking back to the full YouTube video.
- Each derivative is **style-adapted per destination** (poetry summary in the channel's voice, aspect ratio + length per platform spec) — reuse the per-channel adaptation machinery already used for lyrics/imagery.
- FFmpeg (already bundled) handles the re-cuts; the free AI writes the adapted captions/poetry and the link-back copy.

### D & E. Publicity / co-publishing + co-upload to many channels
- **Integrate an OSS cross-poster instead of hand-rolling each network.** [Postiz](https://github.com/gitroomhq/postiz-app) (AGPL-3.0, ~32k★, self-hostable, 30+ networks incl. TikTok/Instagram/Facebook/LinkedIn/Bluesky/Mastodon/Telegram/Discord/Reddit/Pinterest, with AI-assist built in) is the strongest fit; [Mixpost](https://mixpost.app/) is the lighter alternative. Either can run as a local sidecar the app posts to, mirroring how Kaggle engines are already treated as external services.
- **Platform automation feasibility (2026), tiered by cost/effort:**
  - *Free open APIs, no gatekeeping* — **Bluesky (AT Protocol), Mastodon, Nostr, Telegram, Discord** and the already-integrated **YouTube**. Start here.
  - *Free but review-gated* — **Meta Graph API** (Instagram/Facebook/Threads: $0 to call but needs App Review beyond 25 test users) and **TikTok Content Posting API** (no fee, but forces private-visibility until a ~1–2 week app audit passes).
  - *Now paid* — **X/Twitter** is pay-per-use as of Feb 2026 (~$0.01/post) — deprioritize or reach via webview-macro instead.
  - *No API / webview-macro only* — anything without a usable API falls back to the built-in-browser + recorded-macro route from (A)/(B).
  - *Escape hatch* — a unified API (Ayrshare/Blotato) trades per-platform review for usage-based cost; keep as an option, not the default, to preserve the free/self-hosted ethos.
- **Auto co-publish**: opt-in, so nothing reaches a real audience without a human toggle — same conservative default as the scheduler. Each daily run fans its derivatives out to the enabled channels with links back to the canonical YouTube video.

**Cross-cutting flags:** (1) credential encryption is non-negotiable; (2) scraping/automation against no-API networks carries the same ToS/ban fragility already noted for Suno/Midjourney — surface health/expiry, don't fail silently; (3) build all new persistence as **JSON, not Mongo**, so it survives the mobile migration; (4) sequence it: open-protocol APIs first (cheap, robust), review-gated APIs next, webview-macros last.

## Requested 2026-07-24 — Mobile build (Android first, iOS later)

Directly requested: investigate what it takes to ship a mobile build. **Findings from inspecting the tree (2026-07-24):**

**What already exists (good news):**
- Tauri Android was **initialized** on 2026-07-19 — `src-tauri/gen/android/` has the full Gradle project (`app`, `buildSrc`, `gradlew`, `build.gradle.kts`). No app has been built yet (no `.apk`/`.aab`).
- All four **Rust Android targets are installed** (`aarch64/armv7/i686/x86_64-linux-android`).
- `ANDROID_HOME` is set (`/home/johannes/Android`); GPU generation is already off-device (Kaggle over HTTP), which is inherently mobile-friendly.

**Toolchain gaps to close first:**
- **`NDK_HOME` is unset** — required for the Rust→Android cross-compile. Install an NDK via Android Studio's SDK Manager and export it.
- **JDK is 26; the Android Gradle Plugin wants JDK 17** — installing a JDK 17 and pointing Gradle at it will almost certainly be necessary (26 is too new for current AGP).

**The real blocker — desktop-only subsystems that cannot run on Android (none are cfg-guarded today; 0 `cfg(mobile/desktop)` guards in the crate):**
- **Bundled `mongod` sidecar** (referenced across ~17 files) — a native x86_64 Linux server process. Android can't run it. **The Mongo→JSON migration is a hard prerequisite for mobile**, not just a nicety.
- **Playwright browser automation** (~6 files — Midjourney, YouTube channel switching) — spawns Node + Chromium. No equivalent on Android; these features must be cfg-gated off or re-implemented via the system WebView.
- **Sidecar / subprocess spawning** (`Command::new`/`.spawn()` across ~11 files, incl. FFmpeg) — Android has no general desktop subprocess model. FFmpeg needs an ARM/`ffmpeg-kit` build or a mobile media pipeline; other spawns need mobile substitutes.
- **GTK child-webview** (the embedded in-app browser) — Linux/GTK-specific; Android has a single system WebView with a different embedding model.
- **OAuth loopback servers** — binding `localhost` for the OAuth redirect works on desktop; mobile needs custom URI schemes / App Links instead.

**Recommended path:**
1. Finish the **Mongo→JSON migration** (already underway) — the gating dependency.
2. Add `cfg(desktop)` / `cfg(mobile)` guards so the crate *compiles* for Android with desktop-only features (Playwright, sidecars, GTK webview, mongod) excluded — target a **"mobile-lite" build first**: dashboard, brief, compose, and Kaggle-backed generation over HTTP, with the browser-automation and local-sidecar features hidden on mobile.
3. Install NDK + JDK 17, then `cargo tauri android dev` / `build` to produce a debug `.apk`.
4. For release: an Android signing keystore → `.aab` for Play Store (or sideload `.apk`).
5. **iOS later** — needs macOS + Xcode hardware the current Linux box doesn't have; revisit once Android is proven.

### Alternative mobile strategy — web-first (PWA/TWA WebView) instead of a native port

A second, distinct path to "use it on a phone" that trades the native Android build for a hosted/served web UI. Captured alongside the native mobile-lite plan above so the two are weighed side by side.

**The shell is easy:** the frontend is already a plain Vite web build, so the UI is hostable as a website today. Wrap it for mobile via a **PWA + Trusted Web Activity** on Android (Google's blessed path — install, push, offline shell, no URL bar). iOS is stricter: App Store guideline 4.2 rejects thin WebView wrappers, so iOS needs genuine native integration or ships as an add-to-home-screen PWA.

**The work is relocating the backend.** Today the "backend" is Rust on the user's own machine spawning local `mongod`/`ffmpeg`/Playwright. Two sub-options:

- **(B1) Multi-tenant PaaS** — host the Rust backend on a Linux server. Upside: the desktop-only subsystems (`mongod`, `ffmpeg`, headless-Chromium Playwright) *all run fine on a Linux VM*, so this sidesteps the entire Android-porting problem and can carry more features than a `cfg`-gated native build. Downside: it flips single-user-local into SaaS — real hosting cost (no longer free), a multi-tenancy/auth/shared-DB rewrite (the "multi-user/team" item above), **centralized ToS/ban risk** (automation from datacenter IPs on many users' cookies gets blocked far faster than each user from their own IP), and you become a **credential custodian** for strangers' social/Google/Suno secrets. Scope this as a product pivot, not a build target.
- **(B2) Self-hosted-per-user remote control (recommended)** — keep the backend on the *user's own* machine, expose its existing web UI over a secure tunnel, and point a thin mobile PWA/WebView at it. The phone becomes a remote for the user's own desktop instance. Low-effort here because the app **already** runs a local server and **already** stands up `cloudflared` tunnels (that's how it reaches Kaggle). Preserves free + private + own-IP + own-cookies, needs **zero** porting of the native subsystems: phone does the light work (brief, compose, review, trigger), desktop does the heavy/fragile work.

**Key enabler for any web-first route:** the Tauri→web transport swap is centralized — every backend call goes through the single `invoke` wrapper in `src/src/lib/api.js`, so converting Tauri IPC to HTTP/WebSocket is one chokepoint, not a scatter across 25 pages.

**Decision guide:** phone-against-your-own-PC → **B2** (cheapest, on-ethos). Strangers with only a phone → **B1** (full PaaS pivot). Neither replaces the native mobile-lite plan; they're parallel bets.

## Explicitly out of scope for now (per current architecture)

- Building a real Suno/Midjourney/YouTube *server-side* SaaS — the whole design (bundled `mongod`, local file paths, local git repos, local OAuth loopback servers) is single-desktop by construction. Any of the above should keep that constraint unless there's an explicit decision to re-architect toward a hosted service.
