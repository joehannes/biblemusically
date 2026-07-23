# Status Log

A dated log of observed project state, **newest observation first** (reverse-chronological — read top-down for "what's true now", scroll to the bottom for where this app started). Each entry records what was true *as of that date*, based on git history and direct code inspection. For the current feature-by-feature state see [FEATURES.md](FEATURES.md); for open issues see [TODOS.md](TODOS.md); for what could come next see [BACKLOG.md](BACKLOG.md).

---

## 2026-07-08 — Implementation pass 3: Job Queue, Scheduler, Character builder, daily-lyrics usability

Four features requested directly, all implemented and verified (`cargo check` + `npm run build` clean throughout):

- **Job Queue.** `AppState.job_queue: Arc<Mutex<Vec<String>>>` — declared, never drained, called out as vestigial in `TODOS.md` #15 — was replaced with `AppState.job_semaphore: Arc<Semaphore>`, sized from a new `max_concurrent_jobs` setting (default 2, editable in Settings → Job Queue). `jobs::enqueue` and `retry_job` now acquire a permit before running a job; anything beyond the cap sits visibly `"queued"` in the Jobs Monitor instead of running unbounded (a real resource concern given each Midjourney job launches a full browser and Suno/YouTube calls share account-level rate limits).
- **Scheduler.** New module `src-tauri/commands/scheduler.rs`. Rather than trying to parse the existing free-text `schedule` field ("weekly Sunday 9am"), each project gets an optional structured `schedule_config` (book, translation, frequency, time, languages, styles, chapter cursor) stored as a loose JSON blob on the project document — no Rust model changes needed, since `get_project`/`update_project` already round-trip arbitrary fields through Mongo. A background tick (spawned once at app startup, every 5 minutes) checks every project and, when due: fetches the next Bible chapter, generates that chapter's lyrics via AI (up to 4 language×style combinations per run), inserts each as a draft song, and auto-enqueues music generation. The chapter cursor wraps back to 1 once a book is exhausted. **Deliberately stops there** — analysis, images, video, and upload all stay manual, so nothing reaches a real YouTube channel without a human reviewing it first. A "Generate Now" button in a new Dashboard "Daily Content" panel calls the exact same function manually — one implementation of "make today's song," whether triggered automatically or by hand. This is the interpretation chosen for the ambiguous "AI assistant for basic lyrics generation on a daily iteration" ask; flagged to the user as a judgment call rather than silently assumed.
- **Character builder enhancements.** Three concrete additions: (1) project-level scoping now actually works — `Character.project_id` existed in the model from the start, but `list_characters` never accepted or filtered by it, so every song-less character leaked into every song's list regardless of project; the create dialog now has an explicit song-only vs. project-wide toggle, and `list_characters` filters correctly while still showing pre-existing legacy data everywhere (so old characters don't silently disappear). (2) `appearance_tags` — short, stable visual descriptors prepended to every Midjourney prompt for a character (both fresh generation and "Vary"), so regenerated portraits stay visually consistent instead of drifting each time the free-text description alone is re-sent. (3) A client-side search/filter bar over name/description/tags.

Not touched: the free-text `schedule` field on Project still exists alongside the new `schedule_config` (can drift out of sync — see `TODOS.md` #17), and the AI calls inside a scheduler tick aren't gated by the new job semaphore (they run synchronously in the tick itself, not as jobs) — both noted as follow-ups rather than silently fixed in this pass.

## 2026-07-08 — Implementation pass 2: fixed all remaining known bugs

Went through every open item in `TODOS.md`'s bug/naming lists (everything except the dead-code deletions, which stay annotated rather than removed, and the genuinely-missing features, which are out of scope for a bug-fix pass) and fixed each one, verified with `cargo check` + `npm run build` after every change:

- **`cancel_job` now actually cancels.** Added `AppState.cancelled_jobs` (a shared `HashSet<String>` behind a `Mutex`). `cancel_job` marks the job `"cancelled"` and records its ID instead of deleting the document; `run_job` checks the flag before starting and again before writing its final status, always winning over whatever the underlying work returned. The two slow integrations now notice a cancel mid-flight instead of only after the fact: Suno's ~200s result-polling loop checks every 5s tick, the YouTube chunked-upload loop checks before every chunk, and Midjourney's up-to-6-minute Playwright wait was restructured from a single blocking `await` into a `tokio::select!` against a 2s cancellation check that kills the child process. `retry_job` clears any stale flag first. The Jobs Monitor UI now shows a `cancelled` state distinctly, only offers Cancel on active jobs, and offers Retry on cancelled ones too.
- **`bulk_generate_all_songs` renamed to `bulk_generate_all_images` and scoped to a project.** It silently reached across every song in every project before; now it takes an optional `project_id` and the Settings-page "Generate All Images" button passes the active one.
- **Misleading `suno_google_*` settings keys renamed** to `google_loopback_*` (pure rename — confirmed nothing read the old keys back before changing them).

Not touched in this pass, on purpose: the five dead-code files/configs (still just annotated `deprecated???`, not deleted — that's a decision for whoever owns the repo, not something to do silently mid-bugfix), and the genuinely-missing features (Riffusion integration, automated tests, a real scheduler, job-queue rate limiting) — those are captured in `TODOS.md`'s "missing / incomplete pieces" section and `BACKLOG.md`, not bugs to fix.

## 2026-07-08 — Implementation pass 1: finished the "90% done" items

Acted on the `BACKLOG.md` "Now" section and shipped the following, each verified (not just written):

- **Characters screen wired up.** Added the 10 missing wrapper functions to `src/src/lib/api.js` that `Characters.jsx` was calling into a void. Verified with a clean `npm run build`.
- **Settings → "Check Node" fixed.** Registered `commands::probe_node` in `src-tauri/src/lib.rs`'s `invoke_handler!`. Verified with a clean `cargo check`.
- **Version drift reconciled and the script fixed at the root cause.** `src-tauri/Cargo.toml` bumped `0.24.1` → `0.29.0` to match the other manifests; `Cargo.lock` refreshed via `cargo check`. `scripts/bump-version.sh` now also updates `Cargo.toml` (it previously only touched the two `package.json`s and `tauri.conf.json`), so this won't drift again.
- **Git-versioning / Google Drive backup feature verified end-to-end** by direct inspection plus real git-command simulation (init → commit → date-prefixed annotated tag → tag listing/parsing format → detached-HEAD detection → branch creation from detached HEAD — all reproduced in a scratch repo and confirmed to match exactly what `projects.rs` expects). This surfaced one real, previously-undetected bug:
  - **The "Google Drive (JSON only, no media)" backup wasn't actually excluding media.** `add_dir_to_tar` skipped the `media/` folder from the tar listing but never excluded `.git/` — and since `git add -A` commits `media/` on every save regardless of destination, every historical media file stayed reachable from `.git/objects` and rode along anyway. Reproduced directly (a repo with one 200KB committed media file produced a 260KB "no-media" tar containing the blob), fixed by always excluding `.git/` from the tar, and re-verified (same repro → 10KB tar, zero git-internal paths). This also stops the "Save to Local Disk"/"Full" Google Drive tars from re-packing the *entire* accumulated git history on every single save.
- Documentation (`FEATURES.md`, `TODOS.md`) updated in place to reflect all of the above as fixed rather than open issues.

Not changed in this pass: `cancel_job`'s lack of real task cancellation and the naming nits (both addressed in "Implementation pass 2" above), plus the Riffusion integration gap and the dead-code files, which remain open — see `TODOS.md`.

## 2026-07-08 — Documentation pass (this observation)

Full-repo analysis performed: Rust backend (`src-tauri/`, ~8,300 lines across `commands/*.rs`, `jobs.rs`, `models.rs`, `helpers.rs`, `state.rs`), React frontend (`src/src/`, ~8,000 lines across 13 pages + components), automation scripts (`src-tauri/packaging/*.js`), and the standalone `scripts/kaggle_riffusion/` project. Wrote `ARCHITECTURE.md`, `FEATURES.md`, `TODOS.md`, `BACKLOG.md`, this file, and refreshed `README.md`.

**Headline findings:**
- The app is architecturally a native Tauri 2 desktop app with a bundled `mongod` sidecar — **not** the FastAPI+Motor web-app described in the legacy `memory/PRD.md`. That doc is stale (wrong theme count, describes mock fallbacks that no longer exist, predates the git-versioning feature).
- Every external integration (Suno, Midjourney, FFmpeg, YouTube) is a **real** implementation with no mock fallback left — job failures now surface descriptive errors instead of fake success data.
- Found one fully-built-but-disconnected feature (**Characters** screen — backend complete, frontend API wrapper missing entirely) and one broken button (**Settings → Check Node**, calls an unregistered Tauri command). Both documented in `TODOS.md` with fixes.
- Found and annotated 4 dead files/configs (`src-tauri/main.rs`, `src-tauri/test_warp.rs`, `src/src/lib/tauri-api.js`, `src-tauri/packaging/youtube-channel-discovery.js`) plus a whole leftover CRA/craco build path in `src/`.
- Confirmed `scripts/kaggle_riffusion/` (a complete, documented, free long-form-song-generation service designed for Kaggle GPUs) has **zero** wiring into the Rust app — it's a manual side-tool today.

## 2026-06-27 (working tree, uncommitted) — Project version control + Google Drive backup

The working tree (as of this analysis) contains a large uncommitted feature: **git-backed project version control**.
- `src-tauri/commands/projects.rs` grew by ~767 lines: `get_project_git_info`, `save_project_version`, `checkout_project_git_tag`, `checkout_project_git_branch`, `create_project_git_branch`, `authorize_project_gdrive`.
- Each project can now be tied to a local folder that's a real git repo; "Save" commits + auto-tags (`YYYY-MM-DD.N`) and optionally uploads a tar snapshot to Google Drive (full or JSON-only).
- Frontend: `Shell.jsx` gained a Save split-button + branch-creation modal (+226 lines); `Dashboard.jsx` gained a per-project "Version History & Branches" panel (+300 lines); `Settings.jsx` gained a Google Drive connection card (+91 lines).
- `channels.rs` (+31 lines) and `jobs.rs` (+58 lines) also changed alongside this, plus `youtube-channel-switcher.js` grew by 224 lines — channel-discovery robustness work happening in parallel.
- `package.json`/`src/package.json`/`tauri.conf.json` bumped to `0.29.0`; `Cargo.toml` was **not** bumped (still `0.24.1`) — version drift, see `TODOS.md` #11.
- This work is real and functional but uncommitted, untested by any automated suite, and not yet reflected in any prior doc.

## 2026-06-23 — Tauri/Playwright/OAuth stabilization (PR #8, `c484378`)

"Fix async handling in Tauri OAuth and scraping, bundle Playwright" — async/blocking fixes around the OAuth loopback servers and the scraping-based channel discovery; Playwright got bundled as part of the packaged app rather than assumed to be globally installed.

## 2026-06-22 — Riffusion Kaggle Song Studio added (PR #7, `e0e3c2f`)

A complete standalone Python project was added under `scripts/kaggle_riffusion/`: FastAPI server + job queue + model manager + long-form generator + audio stitcher + Cloudflare tunnel, meant to run on a free Kaggle T4 GPU as a Suno alternative with no API cost. As of this writing it has never been wired into the Tauri app (no Rust command references it) — see `TODOS.md` #12 and `BACKLOG.md`.

## 2026-06-22 — Channel discovery + sync + AI translation cluster (PRs #1–#6)

A dense cluster of same-day work (`5ec6998`, `9af4102`, `0ffcf45`, `b0a78d0`, and their merge commits):
- Advanced channel management + automated onboarding.
- Channel Settings Panel with AI (OpenRouter) translation of channel metadata per language/region (`commands/channel_settings.rs`).
- Fixes to channel auto-discovery reliability and error handling.
- "Finalising app" (`6904c27`) and an OAuth loopback shutdown/error-handling fix (`48600d8`) closed out the day.

## 2026-06-19 — Housekeeping

`.gitignore` updated to exclude `binaries/` and `src-tauri/binaries/` (bundled sidecar binaries — `mongod`, ffmpeg, etc. — kept out of version control).

## 2026-06-13/14 — Templates, autosave, jobs UX (`866fd02`, `fe78401`)

Project templates + `TemplatesManager`, per-project autosave, jobs-log enhancements, channel import/copy. Version bumped to `0.10.0`.

## 2026-06-06 — Midjourney moved from cookies to Playwright profiles (`b470c53`, `29cac81`)

The `midjourney-proxy` and `browsh` git submodules (added `2026-06-01`, `9385186`) were removed again just 5 days later in favor of a visible-browser Playwright automation flow with a persistent profile directory — the architecture described in `ARCHITECTURE.md` §4 today. This is visible in `src-tauri/src/lib.rs`'s comment: *"Browsh sidecar and midjourney-proxy autostart have been deprecated in favor of a visible Playwright-driven browser workflow."*

## 2026-05-17 — "rust backend" (`7edc75d`)

The point at which the Rust/Tauri backend appears to have been introduced, superseding whatever came before (the legacy `memory/PRD.md`'s FastAPI+Motor description dates from around here or earlier and was never updated afterward).

## 2026-05-16 — Repository origin

Earliest commits are a run of `auto-commit for <uuid>` messages — consistent with the project having started life inside an AI-assisted scaffolding tool (the `.emergent/` directory and `memory/PRD.md` at the repo root are artifacts of that origin) before being manually developed further into the current Tauri desktop app.
