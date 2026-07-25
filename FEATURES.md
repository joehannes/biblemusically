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

## Interface language (2026-07-25)

- **Built-in languages** — German, Spanish, Portuguese and Russian ship as translation catalogs
  inside the app. Switching is instant and offline; no AI request is made. The picker marks them
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
