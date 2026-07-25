# TODOs

Concrete, file-anchored issues found while documenting the codebase, first on **2026-07-08**, with
follow-up implementation passes since. Each still-open item below also has an inline `// TODO` or
`// deprecated???` comment at the referenced location. Fixed items keep their original write-up for
context, with a `Fixed —` note.

## Cleared (2026-07-25) — the Mongo→JSON pass

Everything that was still open in this file is now either done or has moved to a precise, named
blocker. See `STATUS.md` for the full write-up and `ARCHITECTURE.md` §3.3/§9–§12 for the design.

| # | Item | Outcome |
|---|---|---|
| 8–12 | Dead code (`main.rs`, `test_warp.rs`, `commands/project_git.rs`, `lib/tauri-api.js`, `youtube-channel-discovery.js`, craco config + CRA scripts, `plugins/health-check`) | **Deleted**, after verifying repo-wide that nothing referenced them. `src/package.json` itself stays — `Shell.jsx` imports it for a first-paint version fallback — but its craco scripts and deps are gone. |
| 13 | Riffusion / a free music engine not integrated | **Superseded and closed.** ACE-Step and HeartMuLa are wired as first-class engines, and `music_engine_fallback` now retries a failed generation on another engine, which was the actual point of the item (a Suno outage shouldn't block the pipeline). |
| 14 | No automated tests | **Done.** 30 tests: the JSON store (CRUD, persistence across reopen, upsert seeding, operators, `$push`/`$inc`/`$unset`, sort/limit/cursor streaming, projection, ext-JSON flattening, project-shard routing), the credential vault (seal/open, wrong-key rejection, masking, salt-dependent KDF), project git conventions, asset walking/hashing, derivative specs, HTML stripping, and the pure pipeline logic in `tests_logic.rs`. |
| 15 | `memory/PRD.md` is stale | **Marked historical** at the top of the file rather than deleted, since it records the original intent. |
| 16 | No Suno/Midjourney health surfacing in the UI | **Done.** `commands/health.rs` + a persistent `HealthBanner`, scoped to the engines the current settings actually use so it can't become wallpaper, with dismissal keyed to the problem set. |
| 17 | Two schedule fields that can drift | **Done.** `schedule` is now *derived* from `schedule_config` on every save (including the scheduler's own chapter-cursor advance) by `projects::describe_schedule`, and the free-text input is gone. |
| 18 | Scheduler's AI calls not gated by the job queue | **Done.** The OpenRouter call takes a `job_semaphore` permit, released before the music job is enqueued so it can't deadlock against the job it is about to create. |
| 19 | Social presence: self-knowledge, co-creation, co-publishing | **Done** — `commands/social.rs`, `/social` page. See `ARCHITECTURE.md` §11. |
| 20 | Mobile build (Android first) | **Rust-side unblocked; one named toolchain blocker left** — see below. |

### What remains for the Android build

The Mongo→JSON migration (the hard prerequisite) is done, and the code-level blockers are addressed:
the unconditional `webkit2gtk` import in `commands/webview.rs` was the single thing keeping the crate
from compiling for anything but Linux — both its call sites were already `cfg(target_os = "linux")`,
and the `linux_layout` module always was. `rfd` is now a desktop-only dependency with its three call
sites guarded, and `mongo_import` / `project_sync` / the FFmpeg derivative cutter already degrade
cleanly off-desktop.

Verified by running `cargo check --lib --target aarch64-linux-android` (with rustup's toolchain —
note the `rustc` on `PATH` here is Homebrew's, which can't see rustup's Android std). It now fails
in exactly one place, and it isn't our code:

```
error: failed to run custom build command for `ring v0.17.14`
  error occurred in cc-rs: failed to find tool "aarch64-linux-android-clang"
```

`ring` (the crypto library under rustls, which `reqwest` uses) compiles C, so it needs the NDK's
clang. That is a toolchain install, not a code change:

1. Install an NDK via Android Studio's SDK Manager and export `NDK_HOME`.
2. Install JDK 17 (the JDK here is 26; the Android Gradle Plugin wants 17) and point Gradle at it.
3. `cargo tauri android dev` / `build` for a debug `.apk`.
4. For release: an Android signing keystore → `.aab`.

Two things are still worth deciding before shipping a mobile build, and neither is a bug: the
embedded browser (GTK child-webviews) has no mobile implementation and its commands will return
errors there, and Playwright-driven Midjourney automation cannot run on Android at all — so a mobile
build is the "mobile-lite" shape the backlog described, not feature parity.

## Fixed (2026-07-08, implementation pass 1)

1. ~~**Characters screen is completely disconnected from the backend.**~~ **Fixed** — added all 10 missing wrappers to [src/src/lib/api.js](src/src/lib/api.js) (`listCharacters`, `createCharacter`, `updateCharacter`, `deleteCharacter`, `generateCharacterImage`, `varyCharacterImage`, `selectCharacterVariant`, `discardCharacterVariant`, `discardAllCharacterVariants`, `proposeCharacters`), matching the parameter names the already-registered Rust commands in `src-tauri/commands/characters.rs` expect. Verified with a full frontend build (`npm run build`) — no compile errors.

2. ~~**`probe_node` Tauri command is never registered.**~~ **Fixed** — added `commands::probe_node,` to the `tauri::generate_handler![...]` list in [src-tauri/src/lib.rs](src-tauri/src/lib.rs). Verified with `cargo check` — compiles cleanly.

3. **Google Drive "JSON only, no media" backup didn't actually exclude media — found and fixed during verification.**
   While exercising the git-versioning/Google Drive feature (`save_project_version` in `src-tauri/commands/projects.rs`), `add_dir_to_tar` excluded the `media/` directory from the tar *listing* when `save_type == "gdrive_exclude"`, but never excluded `.git/`. Since `git add -A` always commits `media/` regardless of `save_type`, and git's object store is content-addressed, every media file ever committed stays reachable from `.git/objects` — so the tar still smuggled in historical media blobs. Reproduced directly: a repo with one 200KB committed media file produced a 260KB "no-media" tar containing 45 git-internal paths, including the media blob itself. **Fix applied**: `add_dir_to_tar` now always skips `.git` (verified: same repro now produces a 10KB tar containing only `project.json`, 0 git-internal paths). This also means the "Save to Local Disk" and "Save to Google Drive (Full)" tars no longer balloon with the entire accumulated git history on every save — they're now snapshots of current state, since the local repo itself (browsable via the Dashboard's Version History panel) remains the actual history store.

4. ~~**Version drift across manifests.**~~ **Fixed** — `src-tauri/Cargo.toml` bumped from `0.24.1` to `0.29.0` to match `package.json`/`src/package.json`/`tauri.conf.json`; `Cargo.lock`'s own package entry refreshed via `cargo check`. [scripts/bump-version.sh](scripts/bump-version.sh) now also updates `Cargo.toml` (via `sed`, since it's TOML not JSON) and attempts to refresh `Cargo.lock` via `cargo check` when `cargo` is on `PATH`, so this won't drift again on the next bump.

## Fixed (2026-07-08, implementation pass 2)

5. ~~**`cancel_job` doesn't actually cancel a running job.**~~ **Fixed.** Previously deleted the job's MongoDB document while the `tokio::spawn`ed `run_job` task kept executing invisibly in the background (still polling Suno, still uploading to YouTube), eventually writing results to a document that no longer existed. Now:
   - `AppState` gained a `cancelled_jobs: Arc<Mutex<HashSet<String>>>` field ([src-tauri/state.rs](src-tauri/state.rs)).
   - `cancel_job` ([src-tauri/commands/jobs_cmd.rs](src-tauri/commands/jobs_cmd.rs)) marks the job `status: "cancelled"` (instead of deleting it) and records the ID in that set.
   - `run_job` ([src-tauri/jobs.rs](src-tauri/jobs.rs)) checks the set before starting and again right before writing its final status — a cancellation always wins and forces `status: "cancelled"`, regardless of what the underlying work returned.
   - The two genuinely long-running operations now poll the cancellation flag mid-flight instead of only finding out afterward: `real_suno`'s ~200s result-polling loop checks every 5s tick; `real_youtube_upload`'s chunked-upload loop checks before every chunk; `real_mj`'s up-to-6-minute Playwright wait was restructured from a single blocking `timeout().await` into a `tokio::select!` against a 2s cancellation-check interval, killing the child process by PID the same way the existing timeout path already did.
   - `retry_job` clears any stale cancellation flag first, so cancelling then immediately retrying the same job doesn't cause the new run to be treated as pre-cancelled.
   - Frontend ([src/src/pages/Jobs.jsx](src/src/pages/Jobs.jsx)): added a badge style for the new `cancelled` status, the Cancel button now only shows for `queued`/`running` jobs (cancelling a finished job made no sense), and Retry now also shows for `cancelled` jobs (previously only `failed`).
   - Verified with `cargo check` (compiles cleanly, including the `tokio::select!`/`pin!` restructuring) and `npm run build`.

6. ~~**`bulk_generate_all_songs` doesn't scope by project.**~~ **Fixed and renamed to `bulk_generate_all_images`** (the old name was doubly misleading — it generates *images*, not music, and operated on every song in every project). It now takes an optional `project_id`; when set, it resolves that project's song IDs first and filters sections by `song_id: { $in: [...] }` instead of an unfiltered `find(doc! {})`. The Settings-page "Generate All Images" button ([src/src/pages/Settings.jsx](src/src/pages/Settings.jsx)) now passes the active project ID. Updated everywhere the old name appeared: `src-tauri/src/lib.rs` registration, `src/src/lib/api.js`'s `bulkGenerateAll` wrapper.

7. ~~**Google OAuth loopback stores tokens under `suno_google_refresh_token`/`suno_google_access_token`.**~~ **Fixed** — renamed to `google_loopback_refresh_token`/`google_loopback_access_token` in [src-tauri/commands/oauth.rs](src-tauri/commands/oauth.rs). Pure rename, zero behavior change (confirmed nothing reads these two keys back before renaming them).

## Built (2026-07-08, implementation pass 3 — features, not bugs)

Four items requested directly: a real Job Queue, a Scheduler, Character-builder enhancements, and a more usable daily-lyrics AI assistant. Full write-up in `STATUS.md` and `ARCHITECTURE.md §7`; summary here for cross-reference:

- ~~**Job queue field is vestigial.**~~ **Resolved by replacing it.** `AppState.job_queue: Arc<Mutex<Vec<String>>>` (which nothing ever drained) was replaced with `AppState.job_semaphore: Arc<Semaphore>`, sized from a new `max_concurrent_jobs` setting (default 2). `jobs::enqueue` and `retry_job` now acquire a permit before running a job, so jobs beyond the cap wait — visibly, as `"queued"` — instead of running unbounded.
- ~~**No scheduler acts on a project's schedule.**~~ **Resolved with a new, narrower mechanism**, not by parsing the old free-text `schedule` field. Projects get an optional `schedule_config` (book, translation, frequency, time, languages, styles, chapter cursor) driving a background tick (`commands/scheduler.rs::run_scheduler_tick`, every 5 minutes) that fetches the next chapter, generates lyrics via AI, saves a draft song, and auto-enqueues music generation — analysis/images/video/upload stay manual. A "Generate Now" button in the Dashboard's new Daily Content panel runs the exact same function manually.
- **Character builder enhancements**: project-level scoping actually works now (`Character.project_id` existed in the model but `list_characters` never accepted/filtered by it — every song-less character leaked into every song's list); added `appearance_tags` (stable visual descriptors prepended to every Midjourney prompt so regenerated portraits stay consistent); added a client-side search/filter bar.

## Dead code (all deleted 2026-07-25 — kept here for the record)

8. **`src-tauri/main.rs`** (root-level, 5 lines) — duplicate of `src-tauri/src/main.rs`; not part of the Cargo build (no `[[bin]]` override points at it). Annotated `deprecated???`.
9. **`src-tauri/test_warp.rs`** — standalone scratch file with its own `fn main()`, not declared as a module anywhere, not compiled. Annotated `deprecated???`.
10. **`src/src/lib/tauri-api.js`** (197 lines) — an earlier draft of the same Tauri-invoke wrapper pattern as `lib/api.js`. Nothing imports it (verified repo-wide). Annotated `deprecated???`.
11. **`src-tauri/packaging/youtube-channel-discovery.js`** (265 lines, Playwright-based) — not invoked from any Rust command; channel discovery is now done by the pure-Rust HTTP scraper (`commands/channels.rs::discover_youtube_channels`) or `youtube-channel-switcher.js` (via `discover_from_channel_switcher`). Annotated `deprecated???`.
12. **`src/package.json` (craco scripts) + `src/craco.config.js` + `src/plugins/health-check/`** — leftover Create-React-App tooling. The actual build is the root `vite.config.ts` (`root: "src"`), which Tauri and `npm run dev`/`build` invoke directly; nothing calls `craco`. Annotated `deprecated???` in `craco.config.js` (can't add a JS-style comment inside `package.json`, which is strict JSON).

All of the above were deleted on 2026-07-25 after a repo-wide reference check. They are recorded here so that a future reader who finds a stale external reference knows what was removed and why.

## Missing / incomplete pieces (not bugs — features that don't exist yet)

13. **Riffusion (Kaggle) is not integrated.** `scripts/kaggle_riffusion/` is a complete, documented, standalone project for free long-form song generation on a Kaggle GPU, but there is no Rust command or Settings field to point the app at a running Kaggle instance. If it's meant as a free Suno alternative, it needs: a settings field for the tunnel URL, a `real_riffusion` job-runner branch in `jobs.rs` analogous to `real_suno`, and a Settings-page connection card.
14. **No automated tests anywhere.** No `#[test]` functions in the Rust crate; no JS test runner actually wired up (the `craco test` script is part of the dead CRA config). `test_result.md` at the repo root still contains only the unfilled testing-protocol template from an earlier scaffold, with no actual entries.
15. **`memory/PRD.md` is stale.** It describes 3 themes (actual: 7), describes mock fallbacks for Suno/Midjourney/FFmpeg/YouTube (actual: none remain — every job kind fails loudly instead), and predates the git-versioning/Google Drive feature and the scheduler entirely. Superseded by this doc set; consider deleting or clearly marking it historical.
16. **No Suno/Midjourney health surfacing in the UI.** The backend already tracks cookie/profile validity and re-checks periodically, but the frontend only shows it when you manually click "Test" in Settings — see `BACKLOG.md`.
17. **The original free-text `schedule` field on Project is now redundant with `schedule_config`.** It's still shown/edited on the Dashboard's project-creation form and displayed on each card, but the new automation reads `schedule_config` (a separate, structured field) instead. They can drift out of sync (e.g. card says "weekly Sunday 9am" in free text while `schedule_config` says daily at 6am). Worth eventually merging into one field/UI rather than carrying both.
18. **Scheduler has no per-project concurrency awareness of the job queue's cap.** If a scheduler tick fires for several due projects at once, it can enqueue several "music" jobs in the same tick; they'll correctly queue behind the `max_concurrent_jobs` semaphore rather than all running at once, but there's no stagger/backoff on the AI (OpenRouter) calls themselves, which aren't gated by the job semaphore at all (they run synchronously inside the scheduler tick, not as jobs).

19. **Social presence: self-knowledge, co-creation, co-publishing** (requested 2026-07-24). Encrypted credential vault + built-in-browser login for social networks; OSS scraper (Instaloader / Playwright / Bluesky+Mastodon APIs) to build a per-user taste profile the AI reads for better suggestions; auto-derived shorts/image/poetry versions of each daily generation; co-publish across many channels via an OSS cross-poster (Postiz/Mixpost). Full write-up, platform-by-platform API feasibility, and security caveats in `BACKLOG.md` → "Social presence".

20. **Mobile build (Android first).** Android is *scaffolded* (`src-tauri/gen/android/`, all Rust android targets installed) but nothing builds yet, and the desktop-only subsystems (bundled `mongod`, Playwright, `Command::new` sidecars incl. FFmpeg, GTK child-webview, OAuth loopback) can't run on Android and aren't `cfg`-guarded. The Mongo→JSON migration is a hard prerequisite. Toolchain gaps: `NDK_HOME` unset, JDK is 26 (AGP wants 17). Full readiness assessment + recommended path in `BACKLOG.md` → "Mobile build".
