# TODOs

Concrete, file-anchored issues found while documenting the codebase, first on **2026-07-08**, with
follow-up implementation passes since. Each still-open item below also has an inline `// TODO` or
`// deprecated???` comment at the referenced location. Fixed items keep their original write-up for
context, with a `Fixed —` note.

## Open milestones (2026-07-26, requested)

Four tracks requested after v0.76.0. Recorded with the shape each needs; **none are started.**

### D. OS integration + a clipboard tool — **done in v0.78.0**

Built: `commands/clipboard.rs` (cross-platform: system mirror with a real pop, append-only vault whose
"pop" appends the simulated post-pop state, store-backed paste queue with an atomic take-and-advance),
`tray.rs` (desktop-only tray icon + menu, rebuilt on every change because Tauri 2 hands the OS a static
menu with no lazy-populate hook), `pages/Clipboard.jsx`, and a `paste-queue` macro step.

Mobile: the clipboard, vault and queue all work there — the plugin covers Android and iOS. What mobile
does not get is the tray (no taskbar) and the 700ms poll (polling from an app the OS suspends is
unreliable and costs battery); there the history records on `clipboard_sync`, which the view and the
macro player call at the moment it is about to be used.

### D (original write-up). OS integration + a clipboard tool

| Piece | Shape |
|---|---|
| Tray icon with a context menu | Tauri 2 ships `tauri::tray::TrayIconBuilder` and `tauri::menu::Menu` — a tray icon with a native menu needs no plugin. The menu carries the app-level actions (open, current project, what the workflow is doing, pause/resume) plus the clipboard section below. |
| Clipboard history | `tauri-plugin-clipboard-manager` reads and writes the system clipboard but does **not** notify on change, so history means polling (~500ms) and de-duplicating against the last value. History lives in the JSON store so it survives a restart. |
| The two-clipboard model (the interesting part) | The user asked for a system clipboard the app *syncs* with, plus a **non-destructive** clipboard that never pops. So: `system` (mirrors the OS, supports pop = "consume the top item") and `vault` (append-only; a "pop" there appends a new entry equal to what the system clipboard *would* look like after the pop). That way every historical item is still visible from the user's point of view, and the visible top item is always the next thing that will paste. Both are one collection with a `kind` field and a monotonic sequence number. |
| Sequenced copy/paste for macros | The macro player needs to paste several prepared items in order, one immediately after the other. So a `clipboard_queue` command: given N items, it sets the system clipboard to item 1, waits for the paste (the macro's own step), then advances. This is what lets a browser macro fill five fields from prepared values. The queue must be a real cursor in the store, not a JS closure, so a page navigation cannot lose it. |
| Taskbar menu as a clipboard tool | The tray menu shows the last ~10 items (truncated), each a click-to-copy; plus "clear history", "pop top", and the app actions. Deliberately simple — anything more belongs in the in-app view. |

### E. Autosave as real git commits — **done in v0.79.0**

Built: `commands/autosave.rs` (`stage_field` writes one value into `data/fields/<feature>.json` in the
project repo and `git add`s just that file; `autosave_commit` commits **the index only** — not `add -A`,
so an unrelated working-tree change is never swept into someone's autosave; `save_and_push` reuses the
sync path so there is one place to be right about credentials), `lib/autosave.js` (opt-in via
`data-autosave="feature:field"`, debounced per field, `focusout` in the capture phase so a component
that stops propagation cannot silently disable saving), and the Shell lifecycle: the view **being left**
is the one committed, a project switch commits first, and `visibilitychange` catches the last edit
before a quit.

An autosave with nothing staged is a no-op rather than an empty commit — otherwise `git log` fills with
noise and stops being usable for the thing it exists for.

Still open from E: only the Project Brief opts in so far. Every other view needs its persisted controls
marked with `data-autosave`; the mechanism is done, the annotation is per-view work.

### E (original write-up). Autosave as real git commits

| Piece | Shape |
|---|---|
| `git add` on focus shift | Every blur of an input, every control change that alters a persistable project field, writes the value into that feature's JSON file under the project repo and runs `git add` on it — staged, not committed. `project_sync.rs` already knows the repo layout, so this is a `stage_path` addition, not new machinery. |
| Commit on tab change | Leaving a view commits whatever is staged, with a generated message naming the view and the fields touched. This is the autosave the user asked for: it happens *because* you navigated away, which is exactly when a person expects their work to be safe. |
| Commit on manual save | Same commit path, with an explicit message. |
| Save-and-push | New entry in the save button's dropdown. Only offered when a remote is configured; uses the existing token-injection path from `remote_sync.rs` (tokens never touch `.git/config`). |
| Risk to respect | A commit per tab change is a lot of commits. The message must carry the view and the changed fields so the log stays readable, and an autosave with nothing staged must be a no-op rather than an empty commit. |

### F. Move actions into the app menubar, viewport-aware — **mechanism done in v0.80.0**

Built: `lib/actionOrder.js` (the pure ordering rule, in its own `.js` so it can be tested — it decides
where buttons sit), `lib/pageActions.jsx` (`useBarAction` for view-wide actions, `useSectionAction` which
registers an action **only while its section intersects the viewport**), and the Shell bar that renders
them with an overflow menu past four.

Ordering is by priority then id, never by registration order: a section action appearing when its section
scrolls into view must not reshuffle the bar, or the button someone is about to click moves out from
under the pointer. Four tests cover exactly that.

Applied on the Graphic Novels view as the worked example (Write and Bind are section-scoped, Collect art
is view-wide). Still open: the same annotation for the other views — the mechanism is finished, adopting
it is per-view work, and `usePageActions` keeps working for pages with bespoke controls.

### F (original write-up). Move actions into the app menubar, viewport-aware

| Piece | Shape |
|---|---|
| Global menubar | Every view's main actions belong in the top bar rather than scattered down the page. `PageActionsContext` already exists in `Shell.jsx` and is the seam: a view registers its actions, the Shell renders them. |
| Viewport-scoped buttons | An action that only makes sense for one section (the big *Generate* on a specific control) should appear in the bar **only while that section is in the viewport**. An `IntersectionObserver` per registered section, registering/unregistering its action as it enters and leaves. |
| Care needed | Actions appearing and disappearing as you scroll is jarring if done naively — the bar needs a stable order and a fixed slot count, so a button never jumps sideways when a neighbour appears. |

### G. Project switching without stopping the running workflow — **done in v0.81.0**

Built: `commands/workflow_run.rs` — a `workflow_runs` document per project (steps, cursor, dispatched job
ids, log) advanced by a 10-second backend tick. Eligibility (which songs still need music, analysis,
overlays, video) moved out of the /workflow page's closures and into tested Rust that reads the **store**,
so a run cannot be confused by a half-edited form, or by a form belonging to a different project.

`lib/runHandoff.js` + `selectProject`: a client-side run in flight registers its intent, and switching
project hands it to the backend runner before the page unmounts — so the previous project's run finishes
instead of being abandoned with its queued jobs still completing (which looked like success).

The Workflow view now has "Run in background" beside "Run full pipeline", shows the backend run's step
and last log lines with pause/resume/cancel, and shows runs belonging to *other* projects — the point of
moving the loop is only reassuring if you can see it from wherever you are.

Still open from G: creating upload rows and enriching their metadata stays in the Upload view (those are
decisions, not work), so the background run's upload step only dispatches rows that are already pending.

### G (original write-up). Project switching without stopping the running workflow

| Piece | Shape |
|---|---|
| The actual requirement | Switching the active project must not stall the workflow the previous project is running. It has to continue in the background to completion. |
| Why it currently would | The workflow orchestrator is client-side (`/workflow` chains the pipeline in the page), so its state dies with the view. Backend jobs survive, but the *sequencing* does not. |
| Shape | Move the run loop behind the backend: a `workflow_runs` document per project holding the step list, the cursor and the current job id, driven by the same tick that runs the scheduler. The GUI then *observes* a run rather than being it — which is also what makes the run survive a switch, a reload and a crash. |
| Read from JSON, not the GUI | The user's own requirement, and the right one: a run must read the project's saved JSON fields, never live component state. That makes E (staged/committed JSON per field) a prerequisite for G, so E comes first. |
| Autosave before switching | Switching projects commits first (E's path), then swaps. Non-negotiable: a switch that loses the last edit is the worst possible bug in a studio app. |

## Open milestones (2026-07-25, requested)

Three feature tracks requested after v0.73.0. **A is built** (v0.75.0) — its research is written up in
[docs/DISTRIBUTION.md](docs/DISTRIBUTION.md). B and C are recorded with the shape each needs and the
research each still owes.

### A. Long compilations + music-only distribution — **done in v0.75.0**

Built: `commands/compilation.rs` (chapter order parsed from titles, concat *filter* so inputs rendered
months apart still join, timestamped description), `commands/distribution.rs` (distributor matrix with
AI policies and rate caps, releases, artists, `export_release_package`), `pages/Distribution.jsx`, and
the 66-book canon (the book list held 17, so most of the Bible could not be selected at all).

Findings that changed the design: **no self-service distributor has an upload API** — so the app builds
the release package and a macro or a person uploads; **TuneCore and CD Baby refuse fully AI-generated
music**, Amuse caps at 10 releases/7 days, LANDR at 12 AI songs/month, and DistroKid excludes "mass
auto-generated content"; **Spotify requires a DDEX AI-credits disclosure**; and **TikTok pays nothing
for a clip under one minute**, so the TikTok cut now defaults to 75s.

Still open from A: nothing blocking. A real per-distributor upload macro has to be recorded against a
live account, which needs the user's own login.

### A (original write-up). Long compilations + music-only distribution

| Piece | Shape |
|---|---|
| Whole-book compilations | One long video per Bible book: every chapter's song concatenated, with per-chapter chapter markers, an intro/outro "special", and a tracklist in the description. The remote render worker already assembles from a list — this is a new spec kind (`compilation`), not a new pipeline. Chapter markers are just timestamps in the YouTube description. |
| Monetisable short-form beyond TikTok | Investigate before building: YouTube Shorts (already the render target, monetised via the main channel), Facebook/Instagram Reels bonuses, Snapchat Spotlight, Pinterest. Check which actually pay per-view in 2026 and which need follower thresholds. |
| Music-only distribution | New **Distribution** tab: releases (single = one chapter song, album = whole book), cover art per release, ISRC/UPC handling, release dates, and per-channel "artist" identity. |
| Distributor integration | **Research first:** DistroKid historically has no public API (only a partner API for labels) — verify. Amuse, RouteNote, CD Baby, Ditto, LANDR: check which expose a REST API and at what tier. If none do at a sane price, this becomes an AI-authored browser macro per distributor (the machinery shipped in v0.72.0) plus a metadata export (CSV/DDEX) the distributor's own uploader accepts. |
| Channels as artists | Each channel maps to an artist profile: name, bio, artwork, Spotify/Apple artist links once claimed. Store alongside the channel. |

### B. Poetic graphic novels → ebooks — **done in v0.76.0**

Built: `epub.rs` (a store-only zip writer with its own CRC-32, plus the OPF/nav/XHTML/CSS documents —
validated externally with Python's `zipfile` and an XML parser: CRCs correct, `mimetype` first and
stored, every document well-formed, manifest matching the archive exactly),
`commands/graphic_novel.rs` (five writing registers as *directions to a writer* rather than adjectives,
three page formats with their own aspect ratios, art through the existing image pipeline, EPUB assembly
with the song embedded, ebook-store matrix and band-based pricing), `pages/GraphicNovels.jsx`.

Findings: **EPUB 3 is the only mainstream format that carries audio**, which is what "musical digital
content" needed; **no ebook store has a publishing API** (Google Play's Partner Center takes bulk
ONIX/spreadsheet uploads, which is the closest thing); Draft2Digital reaches Apple/Kobo/B&N/Google for
10% of list; **KDP pays 70% only between $2.99 and $9.99 and charges delivery per megabyte**, so an
illustrated book with audio must be priced *up*, not down — which is what `pricing_advice` does.

Still open from B: Media Overlays (SMIL read-along) are described in the module but not generated —
they need per-line timings, which means the analysis step, not the writer.

### B (original write-up). Poetic graphic novels → ebooks

| Piece | Shape |
|---|---|
| Poetic re-authoring | New tab: the AI writes a poetic, annotated, flavoured version of a song's text — several style registers (illuminated-manuscript prose, free verse, graphic-novel panels with captions) — from the same context the publicity writer uses. |
| Panel imagery | Graphic-novel aspect ratios (2:3 pages, 1:1 panels, splash spreads) driven through the existing image pipeline with per-panel prompts, so character consistency (`appearance_tags`) carries across pages. |
| Ebook assembly | **EPUB 3** is the target format, because it is the one that carries audio: `<audio>` in the content documents plus Media Overlays (SMIL) for read-along. That is what "musical digital content" needs — a PDF cannot do it. Assembly is a zip with a mimetype, container.xml, an OPF manifest and the XHTML pages; no external library required. |
| Store publishing | New tab connecting the stores that accept EPUB directly: Kobo Writing Life, Draft2Digital (aggregates to Apple/Barnes & Noble/Tolino), Google Play Books, Amazon KDP. **Research:** which have real APIs (Google Play Books has a partner API; KDP does not — it is web-only, so a macro), and per-store pricing rules (KDP's 70% royalty band is $2.99–$9.99; Draft2Digital takes 10%). Pricing advice should come from those rules, not from a guess. |

### C. Printify print-on-demand — **done in v0.77.0**

Built: `commands/printify.rs` with request shapes taken from Printify's **OpenAPI specification**, not
from memory — `POST /v1/uploads/images.json` is account-level (the shop-scoped path 404s with no hint),
products carry `variants[].price` in **integer cents**, and the print area needs every enabled variant
listed or it prints blank. Plus `pages/PrintOnDemand.jsx`, settings, and a guided flow.

Findings: Printify caps **publishing at 200 per 30 minutes** (600 req/min globally, 100/min on
catalogue) — paced here rather than discovered as 429s; print areas are 3000–4000px against generated
art at 1024–2048, so `print_quality()` reports the DPI a buyer would actually see and refuses to call
77 DPI acceptable; and the **Pop-Up Store is free** while Etsy charges $0.20 a listing, which a daily
run turns into a monthly bill.

Still open from C: the scheduler hook (`daily_run` returns what to run but is not yet wired into the
tick), and mockup images are not pulled back from Printify for review in-app.

### C (original write-up). Printify print-on-demand

| Piece | Shape |
|---|---|
| Product selection | Printify has a documented REST API (catalogue, products, orders, publishing). Let the user pre-pick blueprints/print providers per category once — the app then applies art + phrasing to those. Store the selection so a daily run needs no decisions. |
| Art + phrasing | Reuse the image pipeline for the artwork and the publicity writer for the phrasing, with print-safe constraints: 300 DPI, transparent PNG, per-blueprint print-area sizes from the catalogue API. |
| Storefronts | **Research:** Printify's own **Pop-Up Store** is free and needs no external account — the obvious default. Etsy charges $0.20 per listing (so a daily run has a real cost); Shopify is a monthly fee; TikTok Shop and eBay are commission-only. Check current terms before recommending. |
| Daily run | Same shape as the chapter scheduler: a scheduled sweep applies the day's phrasing/art to the pre-picked products and publishes to the connected store, with the same deliberate stop — nothing goes live without the toggle being on. |

## Cleared (2026-07-25 requests) — both shipped

**This section said "not yet built" long after both items shipped.** Corrected 2026-07-26: a stale
"open" entry is worse than no entry, because the next session plans around it.

| # | Item | Outcome |
|---|---|---|
| 1 | Guided, adaptive workflow layer | **Done**, v0.69.0–v0.81.0. Fourteen flows in `lib/guidedFlows.js` — stateful templates, sections that manifest progressively, traffic-light status, resumable sessions, engine-capability gating, and a speaking/listening assistant. |
| 2 | Run all rendering and uploading on remote computers | **Done**, v0.68.0–v0.73.0. `commands/remote_render.rs` + `remote_exec.rs`, Kaggle/Actions/Modal/HTTP adapters, and an allowlisted CLI job contract so a phone can produce a short without carrying ffmpeg. See [docs/REMOTE_RENDER.md](docs/REMOTE_RENDER.md). |

### The original write-up, for the reasoning

| # | Item | Shape |
|---|---|---|
| 1 | **Guided, adaptive workflow layer — starting with AI Composer, then every page.** The Composer shows every section at once and is hard to enter. Wanted: an interactive guide that asks a few multiple-choice questions with sensible learnt defaults, reveals only the section that is currently relevant, adapts the offered controls to what the *selected* engine can actually do (Suno tags vs ACE-Step tags vs HeartMuLa plain headers; per-engine image settings), and lets the user cherry-pick or change their mind at any point. It should know the user: project brief, mood, daily topic, social feeds, work style, and past choices via the learnings store. | New `GuidedFlow` component driving a per-page step graph; step definitions declare `capabilities` they need so unsupported controls disappear rather than fail; defaults come from `learnings` + `brief`; every answer is recorded as a learning signal so the proposals improve. `DailyGuide` (v0.65) and `pageSteps.js` are the existing seams to build on. |
| 2 | **Run all rendering and uploading on remote computers.** ffmpeg assembly and YouTube upload should not happen on the user's machine or phone, and should not push media through the user's connection. Free tier is required; a cheap paid tier is wanted as an extra option. | Researched and designed in [docs/REMOTE_RENDER.md](docs/REMOTE_RENDER.md) — job contract, provider matrix with verified 2026 limits, and a 4-step implementation order (job document + provider setting, Kaggle CPU notebook, shared ffmpeg/upload script, Actions/Modal adapters). |

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

### Android: the crate compiles (2026-07-25)

The toolchain is installed and the code builds for Android. What it took, all of it non-obvious:

* **JDK 17** installed from apt (`openjdk-17-jdk` 17.0.19). The system default here is JDK 26, which
  AGP 8.11.0 / Gradle 8.14.3 reject outright.
* **NDK**: none needed — 29.0.13846066 was already installed under `$ANDROID_HOME/ndk`, with SDK
  platform 36 and build-tools 36.0.0. `NDK_HOME` simply was never exported.
* **`rustc` shadowing**: the `rustc` on `PATH` is Homebrew's and cannot see rustup's Android std, so
  every cross-target check failed with a misleading `can't find crate for 'core'` even though
  `rustup target list --installed` showed the target. `RUSTC` must point at rustup's copy.
* **`ring` needs the NDK's clang** (it compiles C, under rustls → reqwest). The NDK ships only
  API-versioned wrappers (`aarch64-linux-android24-clang`); cc-rs looks for an unversioned
  `aarch64-linux-android-clang` that doesn't exist, so `CC_*`/`AR_*`/`RANLIB_*` and the per-target
  linker have to be set explicitly.

All four are captured in [scripts/android-env.sh](scripts/android-env.sh) — `source` it and the
Android commands work.

With that in place, the compile reached our own code with 11 errors, all desktop-only APIs used
unconditionally, now fixed: child webviews (`Window::add_child`, and `show`/`hide`/`close`/
`set_position`/`set_size` on a `Webview`) don't exist in the mobile runtime, and
`blocking_pick_folder` doesn't either. Both are cfg-split with mobile definitions that return a
plain-English explanation, so the commands stay registered and the frontend gets "not available
here" rather than an unknown-command error. `cargo check --lib --target aarch64-linux-android` now
finishes clean.

**A mobile build is "mobile-lite" by construction**, and that is a product decision rather than a
bug: the embedded browser (Suno/Midjourney sign-in, macro recording) and Playwright-driven
Midjourney automation cannot run on Android at all. What does work there is everything that talks
HTTP — brief, compose, the Kaggle-backed engines, review, upload triggering.

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
