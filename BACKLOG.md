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

## Explicitly out of scope for now (per current architecture)

- Building a real Suno/Midjourney/YouTube *server-side* SaaS — the whole design (bundled `mongod`, local file paths, local git repos, local OAuth loopback servers) is single-desktop by construction. Any of the above should keep that constraint unless there's an explicit decision to re-architect toward a hosted service.
