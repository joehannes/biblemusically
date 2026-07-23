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
├── src/lib.rs        # tauri::Builder setup, sidecar/DB bootstrap, invoke_handler registration
├── src/main.rs       # actual binary entrypoint (calls app_lib::run())
├── main.rs           # deprecated??? — orphaned duplicate at crate root, not built (see TODOS.md)
├── test_warp.rs      # deprecated??? — orphaned scratch file, not built (see TODOS.md)
├── state.rs          # AppState { db: mongodb::Database, job_queue: Arc<Mutex<Vec<String>>> }
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
    └── jobs_cmd.rs         # job list/get/retry/cancel (thin wrapper over jobs.rs)
```

All command modules are re-exported through [src-tauri/commands/mod.rs](src-tauri/commands/mod.rs) via `pub use x::*`, then registered individually in the `tauri::generate_handler![...]` macro call in [src-tauri/src/lib.rs](src-tauri/src/lib.rs). **Registration is manual and easy to desync from the command definitions** — one instance of this has already happened (`probe_node` is defined but not registered; see [TODOS.md](TODOS.md)).

### 3.1 Startup sequence (`src-tauri/src/lib.rs::run()`)

1. Create a Tokio runtime; register the shell/dialog/opener plugins.
2. Resolve the app-data dir, `mkdir -p <app_data>/db`, and spawn the bundled `mongod` sidecar bound to `127.0.0.1:27018` with that data directory.
3. Sleep 1.5s (fixed) to let `mongod` bind, then set `MONGO_URL` env var and construct `AppState` (which pings the DB — if it fails, a native error dialog is shown and the process exits).
4. Persist any `MJ_PROXY_URL` / `SUNO_COOKIE` / `MJ_DISCORD_TOKEN` env vars into the `settings` collection (dev/CI override path).
5. Fire-and-forget spawn `ensure_mj_autostart_internal` (now a no-op stub — Midjourney proxy autostart was deprecated in favor of the Playwright flow; see §4).
6. `app.manage(app_state)` and `app.manage(Arc::new(app_state.clone()))` — **both** a bare `AppState` and an `Arc<AppState>` are managed, because the job queue (`jobs::enqueue`) needs an owned `Arc` to move into `tokio::spawn`, while most commands just borrow `State<'_, AppState>`.
7. Spawn a tiny `warp` HTTP server on `127.0.0.1:3337` (`POST /auth/suno`) so an external Suno-cookie capture tool could push a cookie in; also spawn a background loop that re-validates the Suno cookie every 15 min and Google refresh tokens every hour.
8. If `ffmpeg` isn't found on `PATH`, show a native warning dialog (video composition will be unavailable but the rest of the app still runs).

### 3.2 Job queue (`jobs.rs`)

- `enqueue(kind, target_id, &Arc<AppState>)` inserts a `Job` document (`queued`), then spawns a task that first **acquires a permit from `AppState.job_semaphore`** (a `tokio::sync::Semaphore`, sized from the `max_concurrent_jobs` setting — default 2, editable in Settings → Job Queue, takes effect on next restart) before calling `run_job`. A job beyond the concurrency cap sits visibly `"queued"` until a permit frees up. This replaced an earlier `job_queue: Arc<Mutex<Vec<String>>>` field that was declared but never drained (jobs ran unbounded, immediately, regardless of how many were already in flight) — real, since each Midjourney job launches a full browser and Suno/YouTube calls share account-level rate limits.
- `run_job` dispatches on `job.kind`: `music`, `analysis`, `character_image`, `image`, `video`, `upload`. Each branch calls a real external integration (`real_suno`, `real_mj`, `real_ffmpeg`, `real_youtube_upload`) and writes progress/log lines back into the `jobs` collection as it goes (`db_log`, `set_progress`), which the frontend polls via `list_jobs` every 2.5s.
- **No mock fallback remains** in the job runner — every job kind fails with a descriptive `anyhow` error (with actionable next-steps) if its integration can't run. This differs from the legacy PRD which described mock fallbacks; that behavior was removed at some point (see [STATUS.md](STATUS.md)).
- **Cancellation is real.** `AppState.cancelled_jobs` (`Arc<Mutex<HashSet<String>>>`) is checked by `run_job` at start and again before writing its final status (a cancellation always wins), and polled mid-flight by the two slow integrations: `real_suno`'s ~200s polling loop (every 5s tick) and `real_youtube_upload`'s chunked-upload loop (before every chunk). `real_mj`'s up-to-6-minute Playwright wait uses a `tokio::select!` against a 2s cancellation check that kills the child process by PID.
- `retry_job` re-queues by resetting status, clearing any stale cancellation flag, and spawning a task that acquires a `job_semaphore` permit the same way `enqueue` does before calling `run_job` again.

### 3.3 Data model / persistence

MongoDB collections (all accessed as loosely-typed `bson::Document` → `serde_json::Value`, not through the `mongodb` driver's typed API, except on insert): `projects`, `songs`, `sections`, `channels`, `oauth_clients`, `jobs`, `uploads`, `characters`, `settings` (singleton doc `_id: "singleton"`, or per-project docs keyed by project id), `global_channel_settings`, `pasted_chapters`, `compose_configs`.

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
| **Riffusion** (Kaggle) | Standalone Python/FastAPI project in `scripts/kaggle_riffusion/`, meant to run on a free Kaggle GPU and expose a REST API via Cloudflare tunnel | Kaggle account | **Not wired into the app at all** — no Rust command references it; it's a separate manual tool (see [TODOS.md](TODOS.md)) |

Automation scripts live in `src-tauri/packaging/*.js` and are located at runtime by `helpers::locate_resource_file`, which walks several candidate paths (dev tree, `TAURI_RESOURCE_DIR`, `/usr/lib/<app>/...` for installed `.deb` packages, snap/flatpak) and executed with a `node` binary resolved by `helpers::resolve_node_executable` (bundled binary → `TAURI_RESOURCE_DIR` → dev tree → system `PATH` → common OS paths).

## 5. Project version control + Google Drive backup (new, uncommitted)

This is the single largest piece of in-flight work in the working tree (`git diff --stat` shows +767 lines in `projects.rs` alone). It gives each **project** its own local git repository:

- **`save_project_version`** (`src-tauri/commands/projects.rs`): on first save, asks the user (via the frontend) for a local folder, `git init`s it if needed, serializes the project + all songs + sections to `project.json` (copying any locally-referenced media into a `media/` subfolder and rewriting URLs to relative paths), commits, and tags it `YYYY-MM-DD.N` (auto-incrementing per day). Depending on `save_type` it either writes a `.tar` archive next to the folder (`local`), or uploads a tar to Google Drive — either the full tree (`gdrive_include`) or JSON+project-files only, excluding `media/` (`gdrive_exclude`). The tar builder (`add_dir_to_tar`) always excludes `.git/` — added 2026-07-08 after verification showed that including it defeated `gdrive_exclude`'s "no media" promise (git's content-addressed object store keeps every historical media blob reachable regardless of what's in the current `media/` dir).
- **`get_project_git_info`**: reports current branch/detached-HEAD state, dirty/clean status, all local branches, and all tags (newest first) by shelling out to `git`. Verified against a real git repo: the exact command sequence (`symbolic-ref`, `diff --quiet`, `branch --format`, `for-each-ref` with the `|`-delimited format string) behaves as the parsing code expects.
- **`checkout_project_git_tag` / `checkout_project_git_branch`**: runs `git checkout`, then re-syncs the checked-out `project.json` back into MongoDB (`sync_git_to_db`), replacing that project's `songs`/`sections` documents wholesale.
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

## 8. Known architectural gaps

- **No automated tests.** No `#[test]` in Rust, no JS test runner wired up (the `craco test` script in `src/package.json` is part of the dead CRA config, not actually run).
- **Manual command registration** in `lib.rs` is the only thing connecting a `#[tauri::command]` fn to the frontend; nothing enforces it stays in sync. (This already caused one real gap — `probe_node` — fixed 2026-07-08.)
- **Two parallel frontend API wrapper files** (`lib/api.js`, used; `lib/tauri-api.js`, dead) and **two parallel frontend build configs** (root Vite, used; `src/craco.config.js` + CRA `src/package.json` scripts, dead).
- **Two overlapping schedule fields on `Project`**: the original free-text `schedule` (still shown on the Dashboard creation form/cards) and the new structured `schedule_config` (drives actual automation) can say different things about the same project.
- **The scheduler's AI calls aren't gated by `job_semaphore`** — they run synchronously inside the 5-minute background tick rather than as jobs, so several projects becoming due in the same tick fire their OpenRouter calls concurrently rather than queued.
