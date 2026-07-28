# Architecture

_Lightkid AI Studio ("AI Music Video Studio" / repo name `biblemusically`) — a Tauri 2 desktop app that automates AI-generated, multilingual Bible-verse music videos and publishes them to a pool of YouTube channels._

This document describes the system as it exists in the working tree, including uncommitted changes, as of **2026-07-08**. See [STATUS.md](STATUS.md) for a dated log of how it got here and [FEATURES.md](FEATURES.md) for a per-feature real/mocked breakdown.

## 1. High-level shape

The project used to be a three-tier web app (React frontend + FastAPI/Motor backend + MongoDB), per the legacy `memory/PRD.md`. It has since been **rewritten as a native Tauri 2 desktop application**:

```
┌─────────────────────────────────────────────────────────────────┐
│  Tauri 2 desktop shell (single process)                         │
│                                                                   │
│  ┌───────────────────────────┐      ┌──────────────────────────┐ │
│  │ WebView (React 19 + Vite) │◄────►│ Rust core (src-tauri)     │ │
│  │ src/src/**                │ IPC  │ commands/*.rs + jobs.rs   │ │
│  └───────────────────────────┘      └──────────┬────────────────┘ │
│                                                  │                │
│                                     ┌────────────▼─────────────┐  │
│                                     │ mongod sidecar (bundled)  │  │
│                                     │ 127.0.0.1:27018, per-user │  │
│                                     │ app-data dir              │  │
│                                     └───────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
          │                    │                    │
          ▼                    ▼                    ▼
   Suno (cookie HTTP)   Midjourney (Playwright   Google/YouTube Data
                        visible-browser driver)   API v3 + OAuth2
```

There is no separate backend server process the user talks to over HTTP — the "backend" is Rust code linked directly into the desktop binary, exposed to the WebView via Tauri's `invoke()` IPC bridge. MongoDB is not a remote/managed database; it's a `mongod` binary bundled as a Tauri **sidecar** and started automatically on a fixed local port (`27018`) against a per-user app-data directory, acting as an embedded document store (there is no multi-user/server deployment story here — it's single-machine local state).

## 2. Frontend

- **Stack**: React 19, React Router 7, Tailwind + shadcn/ui (Radix primitives), Vite 7.
- **Entry / build**: root [vite.config.ts](vite.config.ts) sets `root: "src"` and aliases `@` → `src/src`; this is what `npm run dev` / `npm run build` and Tauri's `beforeDevCommand`/`beforeBuildCommand` actually invoke (see [src-tauri/tauri.conf.json](src-tauri/tauri.conf.json)).
- **State**: a single React context (`StudioProvider` in [src/src/lib/store.jsx](src/src/lib/store.jsx)) holds the active theme, active project, active song, project list, song list, and a polled job list (refreshed every 2.5s). No Redux/Zustand — it's a plain `useState`/`useEffect` context.
- **API layer**: [src/src/lib/api.js](src/src/lib/api.js) is the single source of truth — a flat `api.*` object wrapping `@tauri-apps/api/core`'s `invoke()`. Every page imports `{ api }` from here. A second, unused file `src/src/lib/tauri-api.js` duplicates this pattern (see [TODOS.md](TODOS.md) — dead code).
- **Routing / shell**: [src/src/App.jsx](src/src/App.jsx) defines 13 routes; [src/src/components/Shell.jsx](src/src/components/Shell.jsx) renders the sidebar/nav, theme switcher (7 themes, see below), keyboard navigation (Ctrl+arrows), and the project **Save** dropdown (local disk / Google Drive, full or JSON-only — see §5).
- **Themes**: 7 CSS themes (`obsidian`, `aurora`, `vellum`, `dawn`, `daybreak`, `twilight`, `midnight`) defined as `html[data-theme="..."]` blocks in [src/src/index.css](src/src/index.css) and switched live via a `data-theme` attribute + localStorage. (The old PRD mentions only 3 — docs had drifted from code.)
- **Build duplication**: `src/package.json` still carries Create-React-App/craco scripts (`craco start/build/test`) and `src/craco.config.js` + `src/plugins/health-check/` exist, but the actual pipeline is the root Vite config. This looks like a leftover from before the Tauri+Vite migration (see [TODOS.md](TODOS.md)).

## 3. Rust core (`src-tauri/`)

```
src-tauri/
├── src/lib.rs        # tauri::Builder setup, store bootstrap, invoke_handler registration
├── src/main.rs       # actual binary entrypoint (calls app_lib::run())
├── store.rs          # THE persistence layer: JSON files with a Mongo-shaped API (see §3.3)
├── mongo_import.rs   # one-time carry-over from the retired mongod sidecar (own wire protocol)
├── project_sync.rs   # each project folder is a git repo; debounced autosave commits
├── vault.rs          # XChaCha20-Poly1305 credential vault (Argon2id passphrase or machine key)
├── tests_logic.rs    # unit tests for the pure logic (annotations, moods, JSON recovery, slugs)
├── state.rs          # AppState { db: store::Db, job_semaphore, cancelled_jobs }
├── models.rs         # serde structs: Project, Song, Section, Channel, OAuthClient, Job, Upload, Character, ComposeRequest
├── helpers.rs        # resource/sidecar path resolution, mood/effect-preset tables, annotation parsing
├── jobs.rs           # background job runner: real Suno/Midjourney/FFmpeg/YouTube-upload integrations
└── commands/
    ├── settings.rs         # global + per-project settings, Suno/MJ/FFmpeg connection tests, session capture
    ├── projects.rs         # project CRUD, export/import, **git-based version control + Google Drive backup** (new)
    ├── songs.rs            # song CRUD, enqueue music/analysis/video jobs, audio download/convert
    ├── sections.rs         # section CRUD, enqueue image jobs, effect presets
    ├── channels.rs         # channel CRUD, YouTube discovery (pure-Rust scraper), bulk OAuth connect
    ├── channel_creation.rs # browser-watcher flow for creating brand-new YouTube channels
    ├── channel_settings.rs # global channel metadata + AI (OpenRouter) translation per channel
    ├── characters.rs       # character CRUD + AI proposal + Midjourney portrait generation
    ├── oauth.rs            # OAuth client pool CRUD, loopback-server OAuth flows (channels + Google Drive)
    ├── uploads.rs          # upload queue CRUD, preflight OAuth check, AI-generated title/description/tags
    ├── bible.rs            # translation/book lists, bible-api.com + bible.helloao.org chapter fetch, pasted-chapter cache
    ├── ai.rs               # OpenRouter/Qwen client: compose_lyrics, compose_assist, get/save compose config
    ├── jobs_cmd.rs         # job list/get/retry/cancel (thin wrapper over jobs.rs)
    ├── data.rs             # where the JSON lives, per-collection counts, legacy import, manual commit
    ├── remote_sync.rs      # push a project to Hugging Face/GitLab/GitHub/Codeberg; asset offloading
    ├── health.rs           # aggregated engine health, scoped to the engines actually in use
    ├── social.rs           # social accounts, taste profile, ideation, derivatives, co-publishing
    ├── insights.rs         # pipeline board, YouTube analytics feedback loop, quota report
    ├── learnings.rs        # JSON learnings store (taste signals, per-project preferences)
    └── webview.rs          # embedded browser child-webviews (GTK placement is Linux-gated)
```

All command modules are re-exported through [src-tauri/commands/mod.rs](src-tauri/commands/mod.rs) via `pub use x::*`, then registered individually in the `tauri::generate_handler![...]` macro call in [src-tauri/src/lib.rs](src-tauri/src/lib.rs). **Registration is manual and easy to desync from the command definitions** — one instance of this has already happened (`probe_node` is defined but not registered; see [TODOS.md](TODOS.md)).

### 3.1 Startup sequence (`src-tauri/src/lib.rs::run()`)

1. Create a Tokio runtime; register the shell/dialog/opener plugins.
2. Resolve the app-data dir. **There is no database server to start** — the `mongod` sidecar was removed along with the `mongodb` dependency.
3. Construct `AppState`, which opens the JSON store at `<config>/studio-lightkid/data` (or `STUDIO_DATA_DIR`). On failure a native dialog is shown on desktop, the message is logged, and the process exits.
4. Run the legacy MongoDB import **once**, blocking, if `<app_data>/db` holds WiredTiger files without the `.migrated-to-json` marker — so an upgrading user never sees a half-empty app. Microseconds on every later launch, skipped entirely on a fresh install. The report is written to `migration-report.json` for the Data panel.
5. Persist any `MJ_PROXY_URL` / `SUNO_COOKIE` / `MJ_DISCORD_TOKEN` env vars into the `settings` collection (dev/CI override path).
6. Fire-and-forget spawn `ensure_mj_autostart_internal` (now a no-op stub — Midjourney proxy autostart was deprecated in favor of the Playwright flow; see §4).
7. `app.manage(app_state)` and `app.manage(Arc::new(app_state.clone()))` — **both** a bare `AppState` and an `Arc<AppState>` are managed, because the job queue (`jobs::enqueue`) needs an owned `Arc` to move into `tokio::spawn`, while most commands just borrow `State<'_, AppState>`.
8. Spawn a tiny `warp` HTTP server on `127.0.0.1:3337` (`POST /auth/suno`) so an external Suno-cookie capture tool could push a cookie in; also spawn a background loop that re-validates the Suno cookie every 15 min and Google refresh tokens every hour.
9. Spawn the background timers: the per-project git autosave sweep (45s), the opt-in remote auto-sync (15min), and the chapter scheduler (5min).
10. If `ffmpeg` isn't found on `PATH`, show a native warning dialog (video composition will be unavailable but the rest of the app still runs).

### 3.2 Job queue (`jobs.rs`)

- `enqueue(kind, target_id, &Arc<AppState>)` inserts a `Job` document (`queued`), then spawns a task that first **acquires a permit from `AppState.job_semaphore`** (a `tokio::sync::Semaphore`, sized from the `max_concurrent_jobs` setting — default 2, editable in Settings → Job Queue, takes effect on next restart) before calling `run_job`. A job beyond the concurrency cap sits visibly `"queued"` until a permit frees up. This replaced an earlier `job_queue: Arc<Mutex<Vec<String>>>` field that was declared but never drained (jobs ran unbounded, immediately, regardless of how many were already in flight) — real, since each Midjourney job launches a full browser and Suno/YouTube calls share account-level rate limits.
- `run_job` dispatches on `job.kind`: `music`, `analysis`, `character_image`, `image`, `video`, `upload`. Each branch calls a real external integration (`real_suno`, `real_mj`, `real_ffmpeg`, `real_youtube_upload`) and writes progress/log lines back into the `jobs` collection as it goes (`db_log`, `set_progress`), which the frontend polls via `list_jobs` every 2.5s.
- **No mock fallback remains** in the job runner — every job kind fails with a descriptive `anyhow` error (with actionable next-steps) if its integration can't run. This differs from the legacy PRD which described mock fallbacks; that behavior was removed at some point (see [STATUS.md](STATUS.md)).
- **Cancellation is real.** `AppState.cancelled_jobs` (`Arc<Mutex<HashSet<String>>>`) is checked by `run_job` at start and again before writing its final status (a cancellation always wins), and polled mid-flight by the two slow integrations: `real_suno`'s ~200s polling loop (every 5s tick) and `real_youtube_upload`'s chunked-upload loop (before every chunk). `real_mj`'s up-to-6-minute Playwright wait uses a `tokio::select!` against a 2s cancellation check that kills the child process by PID.
- `retry_job` re-queues by resetting status, clearing any stale cancellation flag, and spawning a task that acquires a `job_semaphore` permit the same way `enqueue` does before calling `run_job` again.

### 3.3 Data model / persistence — JSON files, no database

`store.rs` is the whole persistence layer. It exposes the slice of the MongoDB API the codebase
already used, so the ~380 call sites still read `state.db.collection::<Document>("songs")
.find_one(doc! { "id": &id })`; only the storage underneath changed. Supported: `find_one`, `find`,
`insert_one`, `insert_many`, `update_one`, `update_many`, `delete_one`, `delete_many`,
`count_documents`, the `.sort()`/`.limit()`/`.skip()`/`.projection()`/`.upsert()`/`.with_options()`
builders, cursors as `futures_util::Stream`, and the operators `$set $setOnInsert $unset $inc $push
$addToSet $pull $pop $rename` / `$eq $ne $in $nin $exists $gt $gte $lt $lte $regex $type $size $all
$elemMatch $not $or $and $nor`.

`bson` remains a dependency, but only as a **serialization** crate for those `doc! {…}` literals — no
client, no sockets, no server, and it cross-compiles to Android, which the driver plus a native
`mongod` binary never could. That was the hard prerequisite for the mobile build.

**Two roots, one logical view:**

```text
GLOBAL      <config>/studio-lightkid/data/<collection>.json
PER PROJECT <project_folder>/data/<collection>.json     ← inside the project's own git repo
```

Project *content* — `songs`, `sections`, `characters`, `uploads`, `assets`, `derivatives` — lives in
the project's folder, so its history is `git log`, restoring an old state is `git checkout`, and
copying the folder copies the project. Everything cross-project — `settings`, `projects`, `channels`,
`oauth_clients`, `jobs`, `sync_configs`, `social_accounts`, the preset collections,
`pasted_chapters`, `compose_configs` — stays global. `jobs` is deliberately global: it is transient
machine state, and committing it would churn git history on every progress tick.

Reads of a project-scoped collection union the global shard with every known project shard, so an
unfiltered `find(doc! {})` still sees everything. Writes route to the shard that owns the document;
`sections` and `uploads` resolve their project *through their song*, which the store looks up itself.
Each file is `{ version, collection, updated_at, documents: [...] }`, pretty-printed with sorted keys
and written via `write`+`rename`, so files are hand-readable, diff minimally, and a crash can't leave
a half-written file. Shards are cached in memory and reloaded when their mtime changes — which is
what makes a `git checkout` of an older project version simply work.

One nuance worth knowing: JSON has a single number type, BSON has three. A value written as
`doc! { "index": 1 }` can come back as `Int32`, `Int64` or `Double`, so `store::get_num()` /
`get_float()` exist for callers that want the number without caring which width it arrived as.

Settings resolution has an interesting fallback chain: job execution (`jobs::get_job_settings`) resolves the owning project from the job's target, then looks up `settings` by `_id: <project_id>`, falling back to the `singleton` doc if no per-project override exists — so most Settings fields are effectively global unless a project explicitly overrides them.

## 4. External integrations

| Integration | Mechanism | Auth | Status |
|---|---|---|---|
| **Suno** (music) | Direct HTTP to `studio-api.suno.com` (unofficial) | Session cookie, pasted manually or captured via a Playwright script (`suno-session-capture.js`) | Real, no mock fallback |
| **Midjourney** (images) | Spawns a Node/Playwright script (`midjourney-generator.js`) that drives a **visible** browser using a persistent profile dir | Playwright browser profile captured via `midjourney-session-capture.js` (Discord auto-login flow also exists) | Real, no mock fallback |
| **FFmpeg** (video compose) | `std::process::Command` subprocess, images zoompan'd/crossfaded per section and muxed with the song audio | N/A (local binary) | Real, no mock fallback |
| **YouTube** (upload/discovery/channel metadata) | Google OAuth2 (loopback server) + YouTube Data API v3 resumable upload protocol | Per-channel refresh token, resolved via an `OAuthClient` pool | Real, no mock fallback |
| **Google Drive** (project backup) | OAuth2 (loopback server, separate scope: `drive.file`) + multipart upload | Per-project refresh token stored in the project's `settings` doc | Real (new/uncommitted, see §5) |
| **OpenRouter / Qwen** (AI composer, translation, tag/description enrichment, character proposal) | HTTPS to `openrouter.ai`, free-tier models | User-supplied API key | Real |
| **Bible text** | `bible-api.com` (English public-domain translations) + `bible.helloao.org` (everything else — 20+ languages) | None (public APIs) | Real |
| **ACE-Step / HeartMuLa** (free music engines) | REST servers, typically a free Kaggle GPU notebook reached over a Cloudflare tunnel | Optional API key | Real. Selectable as the primary engine, or as `music_engine_fallback` so an expired Suno cookie costs a retry instead of the night's queue |
| **Mastodon / Bluesky / Telegram / Discord** (co-publishing) | Open HTTP APIs | Token or app password, in the credential vault | Real (see §11) |
| **Hugging Face / GitLab / GitHub / Codeberg / Internet Archive** (project sync + assets) | Provider REST APIs + `git push` over HTTPS | Token in the credential vault | Real (see §9) |

Automation scripts live in `src-tauri/packaging/*.js` and are located at runtime by `helpers::locate_resource_file`, which walks several candidate paths (dev tree, `TAURI_RESOURCE_DIR`, `/usr/lib/<app>/...` for installed `.deb` packages, snap/flatpak) and executed with a `node` binary resolved by `helpers::resolve_node_executable` (bundled binary → `TAURI_RESOURCE_DIR` → dev tree → system `PATH` → common OS paths).

## 5. Project version control + Google Drive backup

This is the single largest piece of in-flight work in the working tree (`git diff --stat` shows +767 lines in `projects.rs` alone). It gives each **project** its own local git repository:

- **`save_project_version`** (`src-tauri/commands/projects.rs`): on first save, asks the user (via the frontend) for a local folder, `git init`s it if needed, serializes the project + all songs + sections to `project.json` (copying any locally-referenced media into a `media/` subfolder and rewriting URLs to relative paths), commits, and tags it `YYYY-MM-DD.N` (auto-incrementing per day). Depending on `save_type` it either writes a `.tar` archive next to the folder (`local`), or uploads a tar to Google Drive — either the full tree (`gdrive_include`) or JSON+project-files only, excluding `media/` (`gdrive_exclude`). The tar builder (`add_dir_to_tar`) always excludes `.git/` — added 2026-07-08 after verification showed that including it defeated `gdrive_exclude`'s "no media" promise (git's content-addressed object store keeps every historical media blob reachable regardless of what's in the current `media/` dir).
- **`get_project_git_info`**: reports current branch/detached-HEAD state, dirty/clean status, all local branches, and all tags (newest first) by shelling out to `git`. Verified against a real git repo: the exact command sequence (`symbolic-ref`, `diff --quiet`, `branch --format`, `for-each-ref` with the `|`-delimited format string) behaves as the parsing code expects.
- **`checkout_project_git_tag` / `checkout_project_git_branch`**: runs `git checkout`, re-syncs the checked-out `project.json` (`sync_git_to_db`), and then invalidates the store's shard cache so the JSON files git just replaced are re-read (see §3.3).
- **`create_project_git_branch`**: plain `git checkout -b`.
- **`authorize_project_gdrive`**: a loopback-server OAuth2 flow (scope `drive.file` + `userinfo.email`) that stores a per-project Google refresh token.
- Frontend surface: [Shell.jsx](src/src/components/Shell.jsx)'s header **Save** split-button (Local / Google Drive JSON / Google Drive full + a branch-creation modal for detached HEAD), and a **Version History & Branches** panel per project card in [Dashboard.jsx](src/src/pages/Dashboard.jsx) (grouped-by-date tag list, branch switcher, new-branch input).

This is real, working, end-to-end code — not a stub. It compiles cleanly (`cargo check`) and its git plumbing has been verified against a real repository; it is still **uncommitted** in the working tree and untested by any automated suite. One bug was found and fixed during verification (the `.git` exclusion above); see [TODOS.md](TODOS.md) for the one remaining rough edge (the Google OAuth token is stored under a misleadingly-named settings key).

## 6. Scheduler (`commands/scheduler.rs`, new 2026-07-08)

Turns a project into a self-progressing daily/weekly content pipeline through a Bible book, without any dedicated Rust model changes — `schedule_config` is a loose JSON object stored directly on the project document (`get_project`/`update_project` already round-trip arbitrary fields through Mongo via `bson_to_value`, so this needed no schema migration):

```
{ enabled, frequency: "daily"|"weekly", time: "HH:MM", day_of_week: 0-6,
  book, translation, languages: [...], styles: [...], next_chapter, last_run_at }
```

- **Background tick**: spawned once at app startup (`src-tauri/src/lib.rs`, alongside the existing token-maintenance loop), fires every 5 minutes, checks every project's `schedule_config` via `is_due()` (enabled + time-of-day passed + weekday match for weekly + not already run today, compared in local time), and calls `run_scheduled_generation` for any that qualify.
- **`run_scheduled_generation`** (the one core function backing both the automatic tick and the manual trigger below): resolves the next chapter (wrapping to 1 once the book is exhausted, via `commands::bible::list_bible_books`'s chapter counts), fetches its text (`commands::bible::fetch_chapter`), generates up to 4 language×style combinations of lyrics via OpenRouter (`commands::ai::call_openrouter` + the newly-`pub` `extract_json_value` JSON-extraction helper), inserts each as a draft `Song`, and calls `jobs::enqueue("music", ...)` for each — **deliberately stopping there**. Analysis/images/video/upload all stay manual, so nothing reaches a real YouTube channel without a human reviewing it. Advances `next_chapter` and records `last_run_at` regardless of partial per-language failures, and leaves a `scheduler_run`-kind entry in the `jobs` collection purely for visibility in the Jobs Monitor (inserted directly, not via `enqueue`, since there's no async work left to poll once the function returns).
- **Manual trigger**: the `generate_next_chapter_now` Tauri command calls the exact same function. Frontend surface: a "Daily Content" panel per project card in [Dashboard.jsx](src/src/pages/Dashboard.jsx) (book/translation/frequency/time/languages/styles editor, "Save schedule", and "Generate now").
- This is the resolution chosen for the ambiguous "make the AI assistant for basic lyrics generation more usable on a daily iteration" request — reusing one function for both automatic and manual "make today's song," rather than building two separate code paths.

## 7. Character builder (`commands/characters.rs`, enhanced 2026-07-08)

- **Project-level scoping now works.** `Character.project_id` existed in the model from the very first version of this feature, but `list_characters` only ever accepted a `song_id` filter — a character created without a `song_id` showed up on *every* song's character list regardless of project. `list_characters` now accepts both `song_id` and `project_id`; when both are given it matches `(song_id == sid) OR (song_id is empty AND project_id ∈ {given pid, empty})` — the "or empty" half of the project clause exists specifically so pre-existing song-less/project-less characters don't silently disappear once this filtering became real.
- **`appearance_tags`** (new field on `Character`/`CharacterCreate`): short, stable visual descriptors prepended to the prompt in `jobs.rs`'s `character_image` job branch, for both fresh generation and "Vary" — the mechanism that actually delivers on the app's stated goal of "consistent Midjourney portraits across a song's sections" (previously only the free-text `image_prompt`/`description` was re-sent verbatim each time, with no anchor keeping regenerations visually aligned).
- **Search/filter bar** in [Characters.jsx](src/src/pages/Characters.jsx): client-side filter over name/description/tags.
- Not done: linking a character to specific sections and one-click-inserting its prompt into those sections' `image_prompt` — see `BACKLOG.md`.

## 9. Remote sync + asset hosting (`commands/remote_sync.rs`, `project_sync.rs`)

Because the store already keeps each project in its own git repo, "backup" reduces to "push". The
only real decision is where the *large* files go, since generated audio/images/video are what consume
storage. Two strategies, selectable per project:

- **`lfs`** — media stays in the repo, tracked by git-LFS, and the host stores it.
- **`offload`** — media is uploaded to an asset host; the repo keeps only `data/assets.json`
  (path → URL + SHA-256 + size) and the media is git-ignored. `restore_project_assets` pulls it back
  into a fresh clone, skipping anything already present.

Free tiers, checked against each provider's own documentation (July 2026):

| Provider | Free storage | LFS | Use it for |
|---|---|---|---|
| **Hugging Face** (default) | 100 GB private; public is best-effort and generous | native; 500 GB/file hard cap | everything, media included |
| GitLab.com | 10 GiB per project (repo + LFS) | yes, inside that 10 GiB | a private mirror with a predictable quota |
| GitHub | repos free; LFS only 1 GB + 1 GB/mo bandwidth | yes, but small | JSON with media offloaded |
| Codeberg | 750 MiB git + 1.5 GiB LFS | yes | JSON on a non-profit host |
| Internet Archive | free, effectively unlimited — **public** | n/a (asset host) | finished, publishable media |

The repository is created through each provider's API on first sync (a 409 "already exists" is the
expected result on every subsequent run). Credentials never touch the store or `.git/config`: they
live in `vault.rs` and are handed to git through an inline credential helper that reads two
environment variables, so the token stays out of the repo, out of `ps` output, and is scrubbed from
any error text before it reaches the UI.

## 10. Credential vault (`vault.rs`)

XChaCha20-Poly1305 with a random nonce per entry, in one of two modes:

- **Passphrase** — Argon2id-derived key, held only in memory after an explicit unlock. Someone who
  copies the disk gets ciphertext.
- **Machine key** (default, so the app works without ceremony) — a random key in a `0600` file
  beside the vault. This protects against *casual* exposure (a cloud-synced folder, a git commit, a
  screenshot); it does **not** protect against anyone who can read the user's files. The UI says
  exactly that instead of implying more.

Switching to a passphrase re-encrypts every entry in one write and deletes the machine key. Secrets
are masked (`••••••••cdef`) on the way out; the plaintext never crosses the IPC boundary.

## 11. Social presence (`commands/social.rs`)

Four stages, each usable on its own:

1. **Accounts** — Mastodon, Bluesky, Telegram and Discord have open APIs and are fully implemented.
   Meta (Instagram/Facebook/Threads) and TikTok are free but gated behind app review/audit; X is
   pay-per-post since Feb 2026 and deliberately not wired. Anything else falls back to the embedded
   browser plus a recorded macro. Each platform's tier is shown in the UI so expectations match
   reality.
2. **Ingestion → taste profile** — reads the user's own posts and favourites from the open-protocol
   platforms only (scraping Instagram with the user's session risks their account for a worse
   signal), then distils voice / themes / audience / what-performs / avoid with the free in-app AI.
   Stored in the JSON learnings, so it is hand-editable and travels with the user's other
   preferences. Only post text and engagement counts are sent to the model.
3. **Grounded generation** — `taste_profile_block()` is prepended to `compose_lyrics` and
   `compose_assist`; `ideate_next` reasons over the profile, the learnings and the project's recent
   output.
4. **Derivatives + publishing** — FFmpeg cuts a centre-cropped vertical short (per-platform aspect
   and duration caps), an image post is built from the song's first section image with an AI-written
   poem, and a teaser links back to the canonical YouTube upload. One AI call writes all the copy so
   the set reads consistently, and a missing AI degrades to video-only rather than failing.
   **Publishing is always an explicit action** — no timer posts anything.

## 12. Insights (`commands/insights.rs`)

- **Pipeline overview** — every song bucketed by stage, split by project/language/style, with
  anything unfinished for ≥7 days flagged (in practice: a job failed quietly and nobody noticed).
- **Upload analytics** — view/like/comment counts pulled back from the YouTube Data API through the
  per-channel OAuth the Channel Manager already holds, batched 50 ids per call. Combinations are
  ranked by **median** views so one runaway video isn't mistaken for a strategy, and any combination
  with fewer than ~5 videos is marked thin rather than trusted.
- **Quota report** — YouTube uploads today against the ~6/day an upload's quota cost allows, Kaggle
  accounts available for rotation, AI jobs/failures today, and disk headroom. Where an exact figure
  isn't knowable without paying for it, the report states what the app observed and names the limit
  rather than inventing precision.

## 13. Free-tier budgets (`ai_budget.rs`, `idle_guard.rs`, new 2026-07-27)

The two scarcest things this app spends are other people's free tiers, and until now it tracked
neither.

**AI requests (`ai_budget.rs`).** OpenRouter's free tier is 50 requests a day *for the whole
account*. Every call through `provider_chat` is recorded per provider per UTC day in
`<config>/studio-lightkid/ai-usage.json` — a file rather than the document store, because
`provider_chat` has ~35 call sites that do not all hold a `Db`. Failures are counted too: a rejected
request still consumed the allowance that rejected it.

Rotation is across **providers**, not models. The previous fallback swapped one OpenRouter model for
another, which does nothing when the limit is per account — it spent a second request to receive the
same 429. Order is: chosen provider → other free ones → billed ones last, so the app never reaches
for a credit card on its own. A provider with nothing left is not asked at all.

Two consequences elsewhere:
- `ai_enrich_uploads` is **batched** (8 uploads per request, results keyed by id never by position).
  It was two calls per upload — the single largest drain.
- `author_publicity_set` is deliberately **not** batched (long-form pieces, one per platform;
  batching invites a truncation that loses all of them) but it now trims to the remaining budget
  rather than dying halfway with nothing left to retry with.

**GPU hours (`idle_guard.rs`).** Kaggle gives ~30 GPU hours a week and a session holds its slot
whether or not anything is generating. A sweeper ends engines untouched for 25 minutes
(`kaggle_idle_stop_minutes`, 0 = off). The rule is deliberately timid because the failure modes are
not symmetric: stopping early costs a cold start, stopping mid-generation costs the generation. So
nothing stops while *any* job is queued or running, and a busy sweep pushes every clock forward so a
long run cannot age out. Engines become candidates only via `idle_guard::touch()` — currently called
for `acestep` and `comfyui`; an engine without one is silently never stopped.

## 8. Known architectural gaps

- **Coverage is uneven, not absent.** 238 Rust unit tests run under `cargo test --lib`; the JS side still has no runner wired up (the `craco test` script in `src/package.json` is part of the dead CRA config). The tested parts are the ones with sharp edges — the ComfyUI registry, version comparison, budget rotation, the store — while most command handlers have none.
- **Adding a ComfyUI graph is a two-file change.** The runtime filler in `jobs.rs` replaces a fixed placeholder list; a new `__TOKEN__` in a graph JSON survives into the submitted prompt and fails at submit as "workflow template invalid after fill", pointing at the template rather than the empty setting. Two registry tests catch it, but only for `Want`s listed in `every_choice()`.
- **Manual command registration** in `lib.rs` is the only thing connecting a `#[tauri::command]` fn to the frontend; nothing enforces it stays in sync. (This already caused one real gap — `probe_node` — fixed 2026-07-08.)
- **Two parallel frontend API wrapper files** (`lib/api.js`, used; `lib/tauri-api.js`, dead) and **two parallel frontend build configs** (root Vite, used; `src/craco.config.js` + CRA `src/package.json` scripts, dead).
- **Two overlapping schedule fields on `Project`**: the original free-text `schedule` (still shown on the Dashboard creation form/cards) and the new structured `schedule_config` (drives actual automation) can say different things about the same project.
- **The scheduler's AI calls aren't gated by `job_semaphore`** — they run synchronously inside the 5-minute background tick rather than as jobs, so several projects becoming due in the same tick fire their OpenRouter calls concurrently rather than queued. The budget ledger now counts them and rotation survives the resulting 429s, but concurrency itself is still what provokes them.
- **`idle_guard` only knows the engines that call `touch()`.** All five do as of 2026-07-27, and a test asserts every touched name is one `stop_kaggle_server` recognises — a mismatch would otherwise leave the session running while the app believed it was managing it. A *new* engine still has to remember the `touch()` call; nothing reports an engine it is not watching.
