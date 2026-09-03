# Features

Full feature inventory as observed in the working tree on **2026-07-08**. "Status" reflects what the code actually does, not what any prior doc claimed. See [ARCHITECTURE.md](ARCHITECTURE.md) for how these fit together and [TODOS.md](TODOS.md) for gaps found while compiling this list.

Legend: ✅ Real/working · 🟡 Real but rough edge (documented) · 🔴 Broken/disconnected · ⚪ Not integrated

## Dashboard (`/`)

| Feature | Status | Notes |
|---|---|---|
| Project CRUD | ✅ | Name, topic, schedule, multi-language/multi-style toggles |
| Project templates | ✅ | `TemplatesManager` component; local presets + import-by-URL; can seed default settings/channels on create |
| Copy channels into a new project | ✅ | Duplicates selected existing channels |
| Export / import project (JSON + media folder) | ✅ | Native folder/file pickers via `tauri-plugin-dialog` |
| **Git-based version history per project** | ✅ (new) | Save to local `.tar`, view branches/tags grouped by date, one-click checkout of any tag/branch, create branch from detached HEAD |
| **Google Drive backup** | ✅ (new, bug fixed 2026-07-08) | Per-project OAuth, upload full or JSON-only tar. The JSON-only mode originally still leaked historical media via `.git/objects` since `.git` wasn't excluded from the tar — fixed (verified: 260KB→10KB on a one-media-file repro). See [TODOS.md](TODOS.md) #3. |
| Global controls: reset settings, clear/export/import all channels | ✅ | Destructive actions gated by `confirm()` |
| **Daily Content scheduler panel** | ✅ (new 2026-07-08) | Per-project: pick a Bible book/translation, frequency (daily/weekly) + time, chapter cursor, and languages/styles to auto-generate (up to 4 combos/run). "Save schedule" persists it; "Generate now" runs the exact same generation the background scheduler uses, manually, on demand. See `commands/scheduler.rs`. |

## Scheduler (background, new 2026-07-08)

| Feature | Status | Notes |
|---|---|---|
| Automatic daily/weekly chapter progression per project | ✅ | Background tick every 5 minutes (`commands::run_scheduler_tick`) checks every project's `schedule_config`; when due, fetches the next Bible chapter, generates that chapter's lyrics via AI (one call per configured language×style, capped at 4/run), inserts each as a draft song, and auto-enqueues music generation. Chapter cursor wraps to 1 once a book is exhausted. |
| Deliberately manual stop point | ✅ (by design) | Analysis/images/video/upload are **not** auto-triggered — a human reviews and progresses those stages explicitly, so nothing reaches a real YouTube channel without review. |
| Manual "Generate Now" trigger | ✅ | Same underlying function (`generate_next_chapter_now` command), callable on demand from the Dashboard — one implementation for both the automatic and manual path. |

## Job Queue (new 2026-07-08)

| Feature | Status | Notes |
|---|---|---|
| Bounded job concurrency | ✅ | `AppState.job_semaphore` (a `tokio::sync::Semaphore`) replaces the old unused `job_queue: Vec<String>` field. `enqueue`/`retry_job` acquire a permit before running; jobs beyond the cap show as `"queued"` in the Jobs Monitor until one frees up. |
| Configurable limit | ✅ | Settings → Job Queue → "Max concurrent jobs" (default 2). Takes effect on next app restart (a `Semaphore`'s permit count is fixed at construction). |

## Bible Sources (`/bible`)

| Feature | Status | Notes |
|---|---|---|
| Translation list (20+ languages) | ✅ | Hardcoded catalog in `commands/bible.rs`, all public-domain |
| Chapter fetch | ✅ | `bible-api.com` for 7 English variants, `bible.helloao.org` for everything else |
| Paste-your-own chapter text (cache) | ✅ | Stored in `pasted_chapters` collection |

## AI Composer (`/composer`)

| Feature | Status | Notes |
|---|---|---|
| Qwen/OpenRouter-driven multilingual lyrics composition | ✅ | `compose_lyrics` — builds one JSON item per (language × style) target, with truncated-JSON auto-repair |
| Free-form "assist" calls (preset/task driven) | ✅ | `compose_assist` |
| Per-field generate toggles, theme layering (global/per-language/per-channel), MJ param sliders, style packs | ✅ | All client-side composition of the prompt sent to `compose_lyrics` |
| Config persistence | ✅ | Server-side (`compose_configs` singleton) with localStorage fallback when not running under Tauri |
| Import composed output → Lyrics screen | ✅ | |

## Lyrics Import (`/lyrics`)

| Feature | Status | Notes |
|---|---|---|
| Import JSON lyrics array into songs | ✅ | `import_lyrics`; auto-collects distinct languages/styles onto the project |
| Delete song | ✅ | Cascades to sections via `delete_song` |

## Music Studio (`/music`)

| Feature | Status | Notes |
|---|---|---|
| Generate music via Suno | ✅ | Real HTTP to `studio-api.suno.com`; cookie pre-check, polls up to 200s for 1-2 clip variants |
| Pick between primary/alt variant | ✅ | `select_song_variant` |
| Download + convert audio (mp3/wav/flac) | ✅ | Per-song and bulk-folder download, via FFmpeg subprocess |

## Audio Analysis (`/analysis`)

| Feature | Status | Notes |
|---|---|---|
| Parse `[bracket]` annotation lines into timed sections | ✅ | `parse_annotations` + even time-slicing across song duration |
| Mood derivation per section (keyword table) | ✅ | `helpers::derive_mood`, ~11 mood keywords + light/dark/heaven fallbacks |
| Effect preset suggestion per mood | ✅ | 16 hardcoded FFmpeg filter presets in `helpers::EFFECT_PRESETS` |

## Characters (`/characters`)

| Feature | Status | Notes |
|---|---|---|
| Backend: CRUD, AI-proposed characters from lyrics, Midjourney portrait generation, variant select/discard | ✅ | Fully implemented in `commands/characters.rs`, registered in `lib.rs` |
| **Frontend wiring** | ✅ (fixed 2026-07-08) | `src/src/lib/api.js` was missing all 10 wrapper functions this screen calls — added and build-verified. See [TODOS.md](TODOS.md) #1. |
| **Project-level scoping** | ✅ (new 2026-07-08) | `Character.project_id` existed in the model since the start but `list_characters` never accepted/filtered by it, so a project-wide character had no real way to be scoped — every song-less character leaked into every song's character list. Create dialog now has an explicit "This song only" vs "Reusable across this project" toggle; `list_characters` filters accordingly (with a fallback that keeps pre-existing song-less/project-less characters visible everywhere, so old data doesn't disappear). |
| **Appearance/consistency tags** | ✅ (new 2026-07-08) | New `appearance_tags` field (e.g. "silver beard, blue traveling cloak") is prepended to every Midjourney prompt for that character — both initial generation and every "Vary" — so regenerated portraits stay visually anchored instead of drifting each time the free-text `image_prompt`/`description` alone gets re-sent. |
| **Search/filter bar** | ✅ (new 2026-07-08) | Client-side filter over name, description, and appearance tags. |

## Section Editor (`/sections`)

| Feature | Status | Notes |
|---|---|---|
| Edit per-section text/mood/timing | ✅ | `update_section` |

## Image Generation (`/images`)

| Feature | Status | Notes |
|---|---|---|
| Per-section Midjourney generation | ✅ | Enqueues an `image` job |
| Batch-generate all sections of a song | ✅ | `batch_generate_images` |
| "Generate All Images" for the active project | ✅ (renamed + scoped 2026-07-08) | `bulk_generate_all_images` (was `bulk_generate_all_songs`) — now scoped to the active project instead of silently reaching across every project in the database. See [TODOS.md](TODOS.md) #6. |
| Midjourney autostart / status | 🟡 | `ensure_mj_autostart` is now a no-op stub (legacy proxy autostart deprecated in favor of Playwright capture flow) |

## Video Composer (`/video`)

| Feature | Status | Notes |
|---|---|---|
| Compose video from section images + audio | ✅ | Real FFmpeg subprocess (`zoompan`/`xfade`/etc. per effect preset), downloads all remote assets first |
| Effect preset picker per section | ✅ | `get_effects_presets` |

## Channel Manager (`/channels`)

| Feature | Status | Notes |
|---|---|---|
| Channel CRUD | ✅ | |
| OAuth connect (per-channel loopback flow) | ✅ | `oauth_start_for_channel` |
| Bulk "connect all channels in one shot" | ✅ | Single OAuth flow, matches by `youtube_channel_id` |
| Import all channels from a connected Google account | ✅ | `import_from_google_account`, paginated `channels.list?mine=true` |
| Pure-Rust YouTube discovery by handle/URL | ✅ | Scrapes `ytInitialData` from YouTube HTML, no headless browser |
| Discovery via Playwright "channel switcher" script | ✅ | `discover_from_channel_switcher` — actively being extended (largest uncommitted JS diff, +224 lines) |
| Legacy Playwright discovery script (`youtube-channel-discovery.js`) | ⚪ | Not called from any Rust command — superseded, see [TODOS.md](TODOS.md) |
| Refresh channel metadata (subs/avatar) in bulk | ✅ | `refresh_all_channel_metadata` |
| Channel-creation browser watcher (detect new handle after creating a channel) | ✅ | Local callback server + system browser |
| Per-channel settings + AI translation of global metadata | ✅ | `channel_settings.rs`, OpenRouter-driven |
| Sync channel branding to YouTube | ✅ | `sync_channel_to_youtube` |

## Upload (`/upload`)

| Feature | Status | Notes |
|---|---|---|
| Upload queue CRUD | ✅ | |
| Real YouTube resumable upload | ✅ | Full chunked resumable protocol, 256KB chunks, token refresh first |
| Bulk-create uploads from all "video_ready" songs × channels × formats | ✅ | `bulk_uploads_from_videos` |
| Preflight OAuth check (which channels need auth before publishing) | ✅ | `uploads_preflight` |
| AI-enrich titles/descriptions/tags per upload | ✅ | OpenRouter-driven, falls back to template text if AI call fails |

## Jobs Monitor (`/jobs`)

| Feature | Status | Notes |
|---|---|---|
| List/poll all jobs (2.5s interval) | ✅ | |
| Retry / cancel | ✅ (real cancellation fixed 2026-07-08) | Cancel now actually stops in-flight work (mid-poll for Suno, mid-chunk for YouTube upload, kills the Playwright process for Midjourney) instead of just deleting the DB row while the task kept running invisibly. See [TODOS.md](TODOS.md) #5. |

## Settings (`/settings`)

| Feature | Status | Notes |
|---|---|---|
| Suno cookie: manual paste, "open login" browser launch, Playwright session capture, test | ✅ | |
| Midjourney: "open login", Playwright profile capture, Discord auto-login (2FA), on-demand single generation, "Generate All Images" | ✅ | |
| FFmpeg path + probe | ✅ | |
| Node.js probe ("Check Node" button) | ✅ (fixed 2026-07-08) | `probe_node` was defined in Rust but never registered in `invoke_handler!` — registered now. See [TODOS.md](TODOS.md) #2. |
| Google Drive per-project connect | ✅ (new) | |
| Google OAuth client pool (multi-client, per-language binding) | ✅ | Legacy single-client fields auto-sync to the first pool entry |
| OpenRouter API key/email/model picker (6 free models + custom) | ✅ | |
| Per-project settings override | ✅ | Falls back to a global singleton doc |
| 7 live-switchable themes | ✅ | Sidebar theme buttons, `data-theme` attribute driven |

## Not part of the app

| Feature | Status | Notes |
|---|---|---|
| Riffusion long-form song generation via free Kaggle GPU | ⚪ | Complete standalone Python project in `scripts/kaggle_riffusion/` (FastAPI server, Cloudflare tunnel, checkpointing) but **zero references from Rust or the frontend** — a manual side-tool today, not an in-app alternative to Suno |

## Data & Sync (2026-07-25)

- **Your data is files.** Everything the app knows is plain JSON you can open, diff and copy. The
  Data & Sync page shows the global folder and every project folder with one-click reveal, plus a
  document count per collection.
- **Each project is a git repository.** Songs, sections, characters, uploads and asset manifests live
  in `<project folder>/data/`, committed automatically (debounced to 45s) — so a project's history is
  `git log` and restoring an older state is a checkout.
- **Free remote backup with room for the media.** Push a project to Hugging Face (100 GB free
  private, native git-LFS), GitLab, GitHub, Codeberg or your own git server. Media either rides along
  in LFS or is offloaded to Internet Archive with a checksummed manifest kept in the repo;
  "Restore media" rebuilds a fresh clone. Optional 15-minute auto-sync.
- **Credential vault.** Every token is encrypted with XChaCha20-Poly1305, optionally behind an
  Argon2id passphrase. The UI states plainly what each mode does and does not protect against.
- **Legacy database import.** Data from the retired MongoDB sidecar is carried over on first launch,
  idempotently, and the old folder is only deleted when you say so.

## Social presence (2026-07-25)

- **Connect your accounts** — Mastodon, Bluesky, Telegram and Discord work with a token or app
  password. Instagram/TikTok are shown with their app-review requirement rather than pretending;
  anything else falls back to the browser-macro route.
- **A profile of you.** The app reads your own posts and likes and distils voice, themes, audience
  and what performs — then feeds that into the composer and the assistant, so generated work sounds
  like you rather than like nobody.
- **"What should I make next?"** Ideation grounded in that profile, your accumulated learnings and
  what this project already produced.
- **One song, many versions.** A vertical short cut with FFmpeg, an image post with an AI-written
  poem, and a teaser linking back to the full video — captioned per platform, published only when
  you press publish.

## Insights (2026-07-25)

- **Pipeline board** — every song by stage across projects, languages and styles, with anything
  unfinished for a week flagged (which usually means a job failed quietly).
- **What performs** — view counts pulled back from YouTube and ranked by median, so one lucky video
  doesn't get mistaken for a strategy. Thin samples are labelled as such.
- **Quotas** — YouTube uploads used today, Kaggle GPU accounts available to rotate, AI requests and
  failures today, and disk headroom.

## Reliability (2026-07-25)

- **Engine health banner** — a persistent warning when the engines you actually use are failing,
  with the age of the check shown. It stays quiet about engines you don't use.
- **Music engine fallback** — if the primary engine fails, retry once on another; the job log records
  which engine produced the audio.
- **Characters in scenes** — attach a character to specific sections and push its appearance tags and
  prompt into them in one action.

## AI-authored browser macros (2026-07-25)

- **For every platform without a usable posting API** — open the page in the app's browser, say what
  the macro should do, and it is written from that page's own elements.
- **The AI never sees raw HTML** — a structural digest of the interactive elements (stable selector,
  visible label, role, whether it takes text) is collected in the page itself, because one modern
  page is hundreds of kilobytes of framework noise that buries the handful of elements that matter.
- **Every step is validated against the player's real vocabulary** before it is saved: an unknown
  step type, a target nothing can resolve, a `fill` with neither a value nor a playback parameter,
  an unbounded wait — all dropped rather than saved. A macro that half-works on a live site is worse
  than one that refuses to save.
- **Fields that change per run become parameters**, so one macro serves every song rather than one.
- Saved into the normal macro library, so it plays, edits, exports and slots into a workflow exactly
  like a recorded one.

## Publicity (`/publicity`, 2026-07-25)

- **The studio writes about the song** — once a song has lyrics, a style, analysed sections and
  rendered images, it can author an article or post per platform from all of it, plus the project's
  brief, today's topic, the channel's language, and your own social voice.
- **Each platform gets its own register** — Reddit value-first with no hype, a blog article with
  headings, X as a thread, LinkedIn short and reflective. A post written in the wrong register is
  removed or ignored, so the register is part of the prompt, not an afterthought.
- **Cover images made for the piece**, not frames lifted from the video — each piece plans its own
  prompts and renders them through the normal image pipeline.
- **Links back** to the YouTube upload and the short, substituted into the text where the writer
  marked them.
- **Honest about posting** — platforms with a usable API are marked as such; the review-gated, paid
  and macro-only ones say so and hand over a copy button plus the in-app posting page.
- **Nothing posts by itself.** Every piece is a draft until a human moves it.

## Social platforms (2026-07-25)

Added alongside Mastodon, Bluesky, Telegram, Discord, Instagram/Threads and TikTok: **Reddit**
(free API; the catalogue warns that most subreddits ban bare self-promotion), **DEV.to** and
**WordPress/Ghost** (full articles with canonical URLs), **Tumblr**, **Pinterest** and **LinkedIn**
(both review-gated), and **X** (marked paid, because writing on the free tier is capped to a handful
of posts a month).

## AI providers — free and paid (2026-07-25)

- **Four providers**: OpenRouter and Google Gemini (free tiers), Claude/Anthropic and ChatGPT/OpenAI
  (paid per token, no subscription — an OpenAI API key is separate from ChatGPT Plus).
- **The model list is live** — "List models" asks the provider what your key can actually reach, so a
  model that shipped last week is selectable and a retired one never is. Non-chat models (embeddings,
  speech, image, moderation) are filtered out.
- **Overload still falls back** — whichever provider is selected, a rate limit or overload retries once
  on the free Nemotron model and tells you it did.
- **Setup comes early** — the first-run guide asks for the AI key right after language and voice,
  because the steps after it (the setup plan, the guided flows, translation) all use it.
- **Key pages open inside the app** by default, next to the field you paste into. Sites that refuse
  embedded sign-in (anything behind a Google account) open in your normal browser and say why.

## Guided workflows (2026-07-25)

- **Every production page can be a conversation** — the AI Composer, Music Studio, Image Generation and
  Video Composer each offer a short guided path: a few questions, two to four concrete choices, and the
  suggested one already marked with the reason it was suggested.
- **The suggestion knows your project** — it comes from your Dashboard brief, today's topic, your
  channels' languages and regions, what you chose here before, and what the selected engine can do.
- **It only offers what your engine supports** — the track-length question doesn't exist for Suno,
  Midjourney's stylise flag is never written into a FLUX prompt.
- **It never traps you** — "All controls" returns the page to its full form, every visited step is
  clickable, and any section can be opened by hand. The preference is remembered per page.
- Design and how to add a flow: [docs/GUIDED_WORKFLOW.md](docs/GUIDED_WORKFLOW.md).

## Remote rendering (2026-07-25)

- **Render and upload on somebody else's computer** — Kaggle CPU sessions (free), GitHub Actions (free
  on a public repo), Modal (inside its $30/month credits), or any worker you host. Chosen in Settings
  or in the Video Composer's guided flow.
- **Your connection stays free** — the worker fetches audio and images from the project's sync remote,
  encodes there, and uploads to YouTube itself. Your machine sends a few kilobytes of job JSON.
- **It refuses rather than guesses** — a song whose assets aren't reachable remotely says exactly which
  asset and why, instead of rendering a video with no audio.
- **Retry-safe** — a job that already published a song will not publish it twice.
- Cost/limit research and the job contract: [docs/REMOTE_RENDER.md](docs/REMOTE_RENDER.md).

## Interface language (2026-07-25)

- **Built-in languages** — fifteen languages ship as translation catalogs inside the app (German,
  Spanish, French, Italian, Portuguese, Dutch, Polish, Russian, Arabic, Hebrew, Hindi, Indonesian,
  Japanese, Korean and Chinese), each at 100% coverage and gated in CI. Switching is instant and offline; no AI request is made. The picker marks them
  "built-in".
- **Any other language** — translated once by your AI provider and cached, including panels you have
  not opened yet (tours, dialogs, wizards), so nothing appears in English later. Bounded by a
  per-day request cap so interface translation can never consume the quota your generations need.
- **Your content is never translated** — inputs, code blocks, song text, anything marked
  `data-no-i18n`, and any string that looks like data rather than a label.
- **Keeping the catalogs current** — `npm run i18n:extract` refreshes the string inventory from the
  source, `npm run i18n:build` fills the catalogs (resumable), `npm run i18n:check` fails when the
  inventory is stale.

## AI provider resilience (2026-07-25)

- **Automatic fallback** — when the configured provider is overloaded or rate-limited, the request is
  retried once on the free Nemotron 3 Ultra 550B model via OpenRouter rather than failing, and a
  toast names both models so you know which one answered.
- **Google OAuth preflight** — "Validate" and every sign-in flow ask Google whether the client and
  redirect URI are acceptable *before* opening a browser, and report the real reason (with the exact
  URI to register) instead of a silent timeout.

## Getting started, and each day (2026-09-02)

- **The project interview** — a new project opens on a conversation rather than eight empty boxes.
  One question at a time, and the questions cascade: the guide sees every answer so far and picks the
  next thing worth knowing, so a children's-story project and a grief-poetry project stop sharing a
  path after the first answer. Every question also takes your own words, spoken or typed, and it can
  be ended at any point with what you have said kept. Without an AI it falls back to a fixed opening
  set, so a project can be started on a spent free tier.
- **Today** — a panel that answers "what should I do now?" from what the project actually contains:
  the songs at each stage, what is stalled, what was already done today. Each step names a page, so
  the answer is clickable rather than advisory, and the reason it gives is about your project ("four
  songs have audio and no images") rather than about the app.
- **Where you are in the whole thing** — a strip above the page showing the nine stops a song passes
  through, which one you are at, and one button to it. Every stop's doneness comes from what the
  project actually contains, never from having opened a page, so it is right after a month away and
  a stop reopens by itself when you add a song. It closes for good on one click; the sidebar still
  has everything.
- **Hands-free** — the guide asks, you answer out loud, and it moves on. Start talking and it stops
  talking. If it mishears twice it hands the question back rather than asking again. Off by default,
  offered only where there is a microphone, and one click ends it.
- **A sidebar that folds** — thirty-five entries fold to the fifteen stops a song passes through,
  and expand again on one click. Which state you start in comes from the audience level you set on
  first run. Nothing is ever withheld: folding hides doors, it does not remove them, and the page you
  are on is always shown.

## How a song is written (2026-09-02)

- **Craft dials** — six decisions that change what comes out, in plain words: the song's **form**
  (verses and a chorus, a returning line, through-composed, call and answer, a litany), how
  **faithful** to the source text to stay (quote it, keep every claim, or take it as a starting
  point), **who is speaking**, its **shape** (lines per verse, a syllable range), how hard the
  **hook** works, and its **register**. Faithfulness matters most for scripture and the app had no
  word for it before: "quote it" and "take it somewhere" are different products.
- **A singability check before the GPU** — lines far outside the metre the rest of the song sets are
  marked as you write, with the count and the line. It is an estimate and says so, and it never
  blocks anything: the engines sing the text verbatim, so a line that does not scan costs a whole
  generation to discover otherwise.
- **One section editor, on the song** — sections, structure tags in the engine's own dialect, and a
  per-section image idea, next to the lyric it edits rather than two pages away on the import screen.
- **Rewriting one section** — this verse is right and that chorus is not, which is what writing
  actually is. Pick a section, say what is wrong with it (or say nothing), and get three options
  back. The whole song goes along as context, so a rewrite matches the metre and the rhymes the other
  sections already set. Nothing outside that section changes.

## Illustrated books (`/novels`, extended 2026-09-02)

- **An edition** — a song's text re-heard as an illustrated book, in one of five voices (illuminated
  manuscript, free verse, graphic-novel panels, annotated study edition, or for children) and three
  page shapes. Page art goes through the same image pipeline as everything else; the finished book is
  an EPUB 3 that plays the song, with a read-along overlay where the audio has been analysed.
- **Readers, and universes** — describe one specific reader: who they are, and the givens their world
  supplies (language, region, cultural background, circumstances, upbringing, era, faith background,
  means, family shape). As deep as you like — three answers or twelve. From one reader, derive
  neighbouring ones by naming which axes move; everything you do not pick is held exactly, so what
  differs between two of them is something you decided.
- **Retelling** — an edition rewritten *through* a reader: written again in their language and from
  where they stand, not translated. Page for page, so art already made still belongs where it is.
  The composer takes a reader too, so a project's song and its book are written for the same person.
- **Volumes** — many editions bound as one book. Assembled in one action from every song in the
  project that has an edition, in the project's own song order, optionally one part per language,
  with the front and back matter a book is expected to have. Everything it produces is then editable:
  reorder it, drop a chapter, add a part, write your own preface — or have any matter page drafted
  from the project's brief and the book's own contents.
- **Before it goes to a store** — a preflight that names what a retailer will reject (no title, no
  cover, no author, a chapter pointing at a deleted edition) separately from what will merely make
  the book worse (no description, no subjects, pages whose art was never generated). It never refuses
  to build: a draft you can hold is worth more than a checklist you cannot get past. It asks every
  book for a colophon, because that is where the AI assistance is disclosed on the page.
- **Metadata a store actually reads** — publisher, description, rights, subjects, series and number,
  publication date, ISBN as the identifier when there is one, illustrator and translator with proper
  role codes. Emitted only where there is something to say, since an empty publisher field reads to a
  store as a publisher named "".

## Whose voice it is written in (2026-09-03)

- **Traditions, not impersonations** — 56 bodies of writing and oratory technique, each with a place
  and a history: the King James cadence, American plain, the ballad, the preached line; Weimar
  classicism, the flat uncanny, Brecht's interrupted scene, Rilke's thing-poem; the wandering
  Cervantine narrator, the marvellous reported plainly, Andalusian deep song, Latin American
  testimonio; French clarity, the symbolists, the chanson; Dante's vernacular sublime, masks and
  asides, neorealism; saudade, cordel, devouring modernism; Dutch plainness, picture-and-motto,
  Flemish nature lyric, the life-song; the rambling gawęda, the examined ordinary, the suffering
  nation; skaz, the argument through people, the chastushka; saj' rhymed prose, the qasida, modern
  Arabic free verse; biblical parallelism, piyyut, the sacred in the kitchen; katha, bhakti, the
  ghazal; pantun, the puppeteer's voice, hikayat; ma, season-and-cut, rakugo; pansori, sijo, held
  sorrow; parallel prose, regulated verse, the storyteller's serial.
- **Every one of the sixteen languages has at least three of its own.** Choosing a language is no
  longer only choosing a vocabulary — Andalusian deep song and Latin American testimonio are both
  Spanish and are not each other. A tradition written in the language you are working in is offered
  before the ones that work anywhere.
- **Named where it can be heard, never as somebody to be** — each tradition lists the writers and
  forms it comes from, because that is how you recognise what you are choosing and because a name
  carries information a description cannot. The instruction sent alongside it is technique a writer
  could act on, which is what keeps the name from collapsing into a caricature. Nobody's voice is
  imitated and no name reaches the page.
- **Four surface dials** — how the sentences go, how much is said in images, things or ideas, how
  raised the voice is. They work with a tradition or without one, and anything left unset is silence
  rather than a default.
- **The same voice everywhere** — a lyric, an illustrated edition and a retelling all describe it the
  same way, and a retelling inherits the edition's voice rather than reverting to the model's own.

## The shop (2026-09-03)

- **What kind of shop this is** — devotional gifts, art prints, things people wear, for children,
  memorial and keepsake. One choice sets the register the listings are written in, which products are
  worth carrying, whether art fills a print area or fits inside it, the markup, and how many words
  may go on the object. Every default it sets can still be changed.
- **The shop's own words** — its name, who buys there, the line under every listing, and house rules
  the copy must obey. Used in every listing and in the prompt that writes the printed phrase; without
  them two people selling completely different things got identical copy.
- **The right picture for the right product** — a 2:3 poster and a square mug get different images
  rather than different crops of one. Chosen by whether it can print at that size first, then by how
  near its shape is to the print area, then by size. Uploaded once each however many products use it.
- **The art goes on straight** — a design bigger than the print area now covers it instead of being
  shrunk into the middle with white around it. Whether it fills the area or fits inside it follows
  the product: a poster is the art, a mug wraps. What filling costs is shown, because a square design
  in a wide banner loses three quarters of itself.
- **Prices that look deliberate** — rounded up to a charm ending, never down, with a floor so a
  markup on a cheap item cannot price under the platform's own cut. The margin is shown as a share of
  the price and stated to be the whole of what reaches you.
- **The catalogue opens on what you sell** — over a thousand blueprints narrowed to the categories
  this shop carries, said out loud so a short list reads as a filter. Type anything and you search
  the lot.
- **Who prints it** — Printify works today. Printful, Gelato and Gooten are listed with what each is
  actually for and what wiring it up would take, rather than omitted: Gelato prints near the buyer,
  which is the argument for it when an audience is spread over several countries.
