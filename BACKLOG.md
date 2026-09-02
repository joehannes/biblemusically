# Backlog / Future Outlook

Forward-looking ideas, organized by horizon. These are **not commitments** — a menu of what would make the app more valuable, informed by what's already built (see [FEATURES.md](FEATURES.md)) and what's currently broken or missing (see [TODOS.md](TODOS.md)).

## Shipped 2026-07-25 — the persistence + presence pass

A large pass that closed most of this file. Full write-up in `STATUS.md`; design in
`ARCHITECTURE.md` §3.3 and §9–§12.

**Persistence rebuilt on JSON files, in each project's own git repo.** `store.rs` replaces MongoDB
with plain JSON files behind the same API the codebase already used, so the swap touched storage
rather than 380 queries. Project content lives in `<project_folder>/data/*.json` inside the
project's git repo; global data stays in the app config folder. The `mongod` sidecar, the `mongodb`
dependency and the bundled 219 MB binary are gone; `mongo_import.rs` carries old data over by
speaking the wire protocol directly rather than keeping the driver around for one migration.

**Free remote sync with real large-file storage** (the "where do the assets go" question):
Hugging Face by default — 100 GB free private, native git-LFS — plus GitLab, GitHub, Codeberg or any
custom git URL, and Internet Archive as an asset host with a SHA-256 manifest kept in the repo.
Tokens live in a new XChaCha20-Poly1305 vault and never touch `.git/config` or argv.

**Closed from the lists below:**
- ~~Health/expiry surfacing~~ — a persistent banner, scoped to the engines actually in use.
- ~~Graceful multi-provider fallback for music~~ — `music_engine_fallback`, with the job log
  recording which engine produced the audio.
- ~~Character → section linking~~ — apply/detach a character's tags + prompt across chosen sections,
  idempotently, with the link recorded on the section.
- ~~Gate the scheduler's AI calls through the job queue~~ — done, permit released before enqueueing.
- ~~Reconcile the two schedule fields~~ — `schedule` is now derived from `schedule_config`.
- ~~Bulk pipeline view~~ — the Insights pipeline board, per project/language/style, with stalled
  songs flagged.
- ~~Upload analytics / performance feedback loop~~ — YouTube stats pulled back through the existing
  OAuth, ranked by median views.
- ~~In-app cost/quota visibility~~ — the quota report (YouTube uploads, Kaggle accounts, AI
  requests, disk).
- ~~Automated test coverage~~ — 30 tests over the store, vault, sync and pure pipeline logic.
- ~~Cross-platform packaging~~ — the bundler now targets dmg/app/nsis/msi/rpm/appimage as well as
  deb, and gating the `webkit2gtk` import is what makes a non-Linux compile possible at all.
- ~~Social presence (A–E)~~ — accounts, ingestion, taste profile, ideation, derivatives,
  co-publishing. See §11.
- ~~Mobile: the Mongo→JSON prerequisite~~ — done; what remains is an NDK + JDK 17 install, see
  `TODOS.md`.

## Shipped 2026-09-02 — the guided layer, the craft layer, and the book engine

Written as a plan on the morning of 2026-09-02 and largely built the same day. What follows is the
original plan with the shipped parts struck through, so the reasoning that produced each shape is
still readable next to what it became. Full write-up in `STATUS.md` (2026-09-02, later).

**Also shipped that day, from the audit's own findings rather than from this plan:** a rotatable
entitlement signing key with an overlap window (`SUBS_PUBLIC_KEYS`) plus the rotation tooling and
procedure; two static audits (`npm run audit:secrets`, `npm run audit:ipc`) wired into CI; Suno
generation over HTTP with the browser path kept as an offered fallback; Midjourney un-hidden.

**And beyond the plan, the book engine:** avatar universes — a reader described across nine axes at
three depths, neighbouring readers derived by naming the axes that move, and an edition *retold*
through one of them page-for-page — and volumes, which bind a project's editions into one book with
store metadata, an ordered contents, front and back matter, and a preflight that separates what a
retailer rejects from what merely makes the book worse.

**Still open from this plan:** A1 (the cross-page flow), A3 (hands-free conversation), B3 (one lyric
editor in one place), and all of C.

### A. A conversation that spans pages, not one per page

The guided layer is real and broad — fourteen flows over fourteen pages, template-first, persisted,
capability-gated, spoken aloud, answerable out loud (`docs/GUIDED_WORKFLOW.md`). What it is not is
**one journey**. Every flow starts and ends inside its own page, and the thing that carries a user
between pages is the sidebar: thirty-five entries in five groups. So the guided experience is
excellent and the app is still overwhelming, because the overwhelming part is the map, not the pages.

**Partly shipped.** Two of the three pieces that make a journey exist now, at the ends rather than in
the middle: `project_interview_next` cascades a conversation at project start — the model sees every
answer so far and picks the next question worth asking, writing only into the Brief's own fields,
endable at any point, with a fixed fallback so a spent free tier can still start a project — and
`guide_today` answers "what now" from what the project actually contains, counting artefacts rather
than trusting statuses, with every step naming a route. What is still missing is the walk *between*
them: A1 below.

Three specific gaps, each cheap relative to what already exists:

1. **A cross-page flow.** *(Still open.)* `workflowFlow` asks how far a run should go and then hands off to the batch
   runner; nothing walks a person *through* the pages, one at a time, in order, resuming where they
   stopped. Every ingredient exists: `guidedFlows.js` is the step data, `get/save_workflow_state` is
   the per-project persistence, `lib/pageSteps.js` is the pipeline order, and `Tours.jsx`'s
   `DailyGuide` already produces an ordered day plan with per-step "Open" buttons. What is missing is
   a single flow whose steps are *pages*, which mounts each page's own flow as its body, and a
   persistent "you are here, N of M" that survives navigation.

2. ~~**The level the app already asked for is never used.**~~ **Shipped**, as *folding* rather than
   filtering. First run asks whether the user is a beginner (`audience_level`), stored it, and read
   it in exactly one place. The sidebar now folds to the fifteen stops a song actually passes
   through and expands to all thirty-five on a click, defaulting from `audience_level` and
   remembered in `nav_focused`. Folding rather than filtering because the app's stated promise is
   that the audience level never withholds a feature — and the current page is always shown, so
   arriving somewhere tucked never hides where you are.

3. **Voice is push-to-talk, not conversation.** *(Still open.)* `GuidedFlow` speaks the question and then waits for a
   click on "Say it" before it will listen. For a genuinely hands-free mode the pieces are all
   present after the 2026-09-02 fixes — `speak()` resolves when playback ends, `listen()` now ends on
   silence rather than on a stopwatch, `interpretAnswer` maps the answer and escalates only when
   ambiguous — and what is missing is the loop that chains them: speak → listen → interpret → apply →
   speak the next question, with a barge-in that stops the speaking when the microphone hears
   someone. It must stay opt-in and interruptible; an assistant that keeps talking is worse than one
   that never starts.

### B. Lyrics: four pages, three editors, and no craft

The lyric journey today is `/bible` or `/freeform` → `/composer` → `/lyrics` → `/music`. Three of
those four are called "Composer" or "Lyrics", and the one named **Lyrics Import** — sitting seventh
in the sidebar, right where a beginner looks for "where the words live" — opens on a textarea whose
placeholder is `[{"title":"...","language":"..."}]`. It is a developer tool with a workflow step's
name and a workflow step's position.

Underneath that, three different editors disagree about what a lyric is:

| Where | What it can do | What it does not know |
| --- | --- | --- |
| AI Composer results panel | read and tweak a generated item | nothing about structure |
| `SectionAnnotator` (on the *import* page) | structural, engine-aware: sections, headers in the right dialect, per-section image idea | is two pages from where lyrics are made and one from where they are last edited |
| Music Gen card | a plain textarea | the engine's dialect — an edit here can silently break the tagging the composer produced |

`SectionAnnotator`'s own doc comment says it exists to replace hand-writing JSON. It sits next to the
JSON box it replaced.

~~**And there are no craft controls at all.**~~ **Shipped** — see item 1. Everything that decided what kind of song this was came
down to three fields: `themes.global` (free text), `targets[].styles` (a genre CSV) and `sections`
(user section ideas). `compose_lyrics`'s system prompt asks for section headers in the engine's
dialect and for imagery that progresses. Nothing anywhere asks about song *form*, point of view,
how close to stay to the source, rhyme, repetition, or reading level — which is most of what
distinguishes one lyric from another, and all of it is one prompt block away.

**The plan, in the order it should be built:**

1. ~~**A `craft` block on the compose config**~~ — **shipped** as `commands/craft.rs`: a closed
   vocabulary of six dials (form, faithfulness, voice, shape, repetition, register), each option
   carrying the instruction sentence the model is handed, with `craft_prompt_block` inserted ahead
   of the source text because "use its own words" is a claim about what the source *is* for this
   run. Faithfulness and shape are guided steps; the rest live in a "How it's written" section.
   Original plan, for the reasoning: a `craft` block on the compose config, plumbed through `ComposeRequest` into
   `compose_lyrics`'s prompt the way `brief_block` and `learnings_block` already are. Fields worth
   having, each because it changes the output and a person can answer it:
   - **form** — verse/chorus, verse/refrain, through-composed, call-and-response, litany. Bounded by
     the engine's tag dialect, which `engineCapabilities.js` already knows.
   - **faithfulness** — quote the source / paraphrase closely / take it as a starting point. For
     scripture this is the most consequential dial in the app and the one it currently has no word
     for.
   - **voice** — who is speaking: the psalmist, a witness, the congregation, God, a child.
   - **shape** — lines per verse and a syllable range, so the text scans. Engines sing what they are
     given; a line that does not scan costs a whole GPU take to discover.
   - **repetition** — how hard the hook works.
   - **register** — plain and modern / literary / archaic, with a reading level.
   2–4 of these become guided steps; the rest live in a section the guide manifests.

2. ~~**A rewrite loop.**~~ **Shipped** as `commands/revise.rs`: `lyric_sections` reads a lyric's own
   `[Header]` structure, `rewrite_section` sends the whole song with one section marked and returns
   three options each carrying the spliced whole lyric, and splitting/splicing are pure — replacing
   a section leaves every other byte alone and restores the blank lines that separate it from the
   next. Reached from the per-song lyric editor in Music Gen; each option is scanned against the
   song it is going into.

3. **One editor, in one place.** *(Still open.)* Move `SectionAnnotator` to where lyrics actually are — the composer
   and the Music Gen card — and let it be the *only* lyric editor, so an edit cannot break the
   engine dialect the composer chose. `/lyrics` keeps the JSON box and becomes what it is: an import
   tool, named and placed accordingly.

4. ~~**Singability feedback, before the GPU.**~~ **Shipped** as `lib/singability.js`: a syllable
   heuristic with the three corrections that matter for sung English, checked against the song's own
   median rather than an absolute range, deliberately generous — a checker that cries wolf is turned
   off and then catches nothing. It never blocks; it marks lines far outside the metre the rest of
   the song establishes, and says that it is an estimate.

### C. Combinations the app measures but never crosses

The app is combinatorial by construction — targets are channel × language × style, images are
section × style pack, video is section × transition × overlay, distribution is video × platform —
and `performance_report` ranks channel/language/style by median views with a thin-data guard. As of
2026-09-02 the guide reads that ranking, so what worked can now argue with what is habitual.

What is still uncrossed, in descending value (**none of C is built** — this section is unchanged):

1. **Publish time × everything.** `publish_time` resolves a channel's local hour and reaches YouTube
   as `status.publishAt`, and `performance_report` never groups by it — so the app schedules by hour
   and cannot say whether the hour matters.
2. **Image style pack × performance.** Style packs are the most visible creative choice in the app
   and are not a dimension of the report at all. The data exists: a song's sections carry their
   prompts, and its upload carries its views.
3. **A deliberate A/B.** Every combination today is chosen; none is *varied on purpose*. One flag —
   "vary this axis across today's targets" — would turn a daily run into an experiment that the
   ranking then reads, which is the difference between measuring history and learning.
4. **Thumbnail and title shape.** Neither is a dimension, and both are what a click is decided on.
5. **Section count and song length.** `pick_duration` already places a length in a range from the
   lyric's line count; nothing checks whether the ranges that get watched are the ones being asked
   for.

## Still open

~~**Let the scheduler optionally progress further than music generation**~~ — **shipped, and this
entry outlived it.** A project carries `schedule_autonomy`: `lyrics` (the default, and the behaviour
this described), `video`, or `publish`. `run_scheduler_tick` reads it per tick — so turning it *down*
takes effect on the next run rather than after a restart, which is the direction somebody changes it
in a hurry — and hands the rest to `workflow_run`, the same runner the Workflow page uses, rather
than to a second sequencer. The Dashboard picks it per project, with a warning on `publish`.

**Multi-user / team support.** Everything is still single-machine, single-user. The JSON-in-git
store actually makes a shared-project workflow more plausible than the Mongo design did (two people
could push/pull the same project repo), but conflict resolution on concurrent edits is unsolved and
would need real thought before promising it.

**Instagram/TikTok publishing — RESOLVED (v0.84.0), and the premise was wrong.** This said both were
blocked on paperwork. Neither is:

- **Instagram** publishes to your *own* account with **no App Review** when that account holds a role on
  your Meta app in Development mode. Review is only needed for accounts that are not yours. Implemented
  as the two-call Graph flow (`/media` then `/media_publish`), with the video container polled to
  FINISHED before publishing, because publishing early fails with a generic error.
- **TikTok** `video.upload` needs **no audit** and puts the video in the creator's drafts — one tap to
  post. The audited `video.publish` scope posts directly, and until an app passes that audit *every*
  post it makes is forced to private visibility. So a draft is the better outcome, not a consolation.

Both fetch the media themselves from a URL rather than accepting an upload, so they need the project's
remote sync configured. That precondition is stated in the error rather than discovered.

Still true: App Review/audit is required for publishing on behalf of *other people's* accounts, and for
TikTok's direct posting. Both are guided in Access & Permissions.

**A macro-driven fallback for the no-API platforms — DONE (v0.84.0).** Per-platform posting recipes:
the app prepares the day's values into the clipboard queue *in the form's own order*, you record
yourself pasting them once, and each paste becomes a `paste-queue` step. The recorded macro therefore
contains "paste the next prepared value" rather than yesterday's caption, so one recording posts every
later song with no editing. A recorded macro is linked to its platform, and a mismatch between the
number of paste steps and the number of prepared values is reported rather than silently pasting the
caption into the hashtag field.

**Mobile feature parity.** A mobile build will be "mobile-lite" — no embedded GTK browser, no
Playwright automation. Deciding what the phone *should* do (the backlog's B2 "remote control for
your own desktop instance" idea remains the cheapest good answer) is still an open product
question.

---

# Reference: the original 2026-07-24 write-ups

Kept for the platform-by-platform detail and the mobile findings, which are still accurate.

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
