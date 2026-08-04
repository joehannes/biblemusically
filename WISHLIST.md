# Wishlist

Desirable-but-unbuilt work, written on **2026-08-04** after an integration audit of the whole app at
v0.111.0 — every route, every page, every registered command, and how they reach each other.

How this file differs from its neighbours:

| File | Holds |
|---|---|
| [TODOS.md](TODOS.md) | Known bugs and gaps, file-anchored, mostly with an inline comment at the site |
| [BACKLOG.md](BACKLOG.md) | Forward-looking product ideas, organised by horizon |
| **WISHLIST.md** (this) | **Things that are already 80% built and don't connect**, plus additions the audit made look worth wanting |

The bias here is deliberate: an app this size gets more value from finishing what exists than from
starting anything. Part 1 is almost entirely "the backend is done, nothing calls it."

---

## How these findings were produced

Every claim below is mechanical and re-runnable. The three checks that found most of it:

```bash
# 1. Commands defined vs. registered vs. actually called from the GUI
grep -rn --include="*.rs" -A6 -E '^\s*#\[(tauri::)?command' src-tauri --exclude-dir=target \
  | grep -oP '(pub\s+)?(async\s+)?fn\s+\K\w+' | sort -u          # 391 defined
grep -rhoP 'invokeCommand\(\s*"\K[a-z0-9_]+' src/src/lib/api.js | sort -u   # 382 wrapped

# 2. api.* methods that no page or component ever calls
grep -oP '^\s{2}\K[a-zA-Z0-9_]+(?=:\s*\()' src/src/lib/api.js | sort -u > /tmp/def
grep -rhoP '\bapi\.\K[a-zA-Z0-9_]+' src/src --include="*.jsx" --include="*.js" | sort -u > /tmp/use
comm -23 /tmp/def /tmp/use

# 3. Routes vs. nav entries
grep -oP 'path="\K[^"]+' src/src/App.jsx | sort -u
grep -oP '^\s+to: "\K[^"]+' src/src/components/Shell.jsx | sort -u
```

**What came back clean, and is worth stating** — this is a well-wired app and the audit mostly
confirmed it:

- **385 commands registered, 385 defined.** No desync. The `probe_node` drift that
  [ARCHITECTURE.md](ARCHITECTURE.md) §3 warns about has been fixed and has not recurred.
- **382 of 385 commands are wrapped in `api.js`.** The 3 that aren't are `greet` (Tauri template
  leftover), `get_job` (superseded by `list_jobs`) and `oauth_start_loopback`. Being wrapped is not
  the same as being reached, which is what Part 1 is about.
- **35 routes, 35 nav entries, zero orphans in either direction.** No page is unreachable and no
  nav item leads to a blank screen.
- **`cargo check` passes** (21 warnings, 0 errors) and `tsc && vite build` passes.
- **108/108 frontend unit tests pass.**
- **No inline `TODO`/`FIXME` left in the Rust or React source** — the only hit is a comment in
  `HealthBanner.jsx` citing a TODOS.md item by number. Loose ends live in that file instead of
  rotting in comments, which is why it is worth its length.
- **Project Brief is genuinely wired through.** `project_brief_block` reaches lyrics
  ([jobs.rs:181](src-tauri/jobs.rs#L181)), characters, publicity, graphic novels and the guide. When
  this file complains about integration, it is not complaining about the brief.

---

## Part 1 — Finish what is already built

Each item here is a backend that works, is registered, is wrapped in `api.js`, and is called by
nothing. The work is a panel or a button, not a feature.

### 1.1 The Suno HTTP engine — an entire module nobody can reach ⭐ highest value

[`commands/suno_api.rs`](src-tauri/commands/suno_api.rs) implements Suno generation over plain HTTP:
cookie → Clerk JWT → generate → poll. All four commands (`suno_generate`, `suno_poll`,
`suno_status`, `save_suno_cookie`) are registered and wrapped. **Nothing calls any of them**, and
`jobs.rs` does not reference the module either — `real_suno` still drives the browser.

Two things are lost by this:

- **[TODOS.md](TODOS.md)'s own recommendation is unimplemented in practice.** The engine-access
  research concluded that an API-shaped Suno path is the sellable one. It got written and then never
  connected to anything.
- **It is the piece that makes Suno work on a phone.** The module's own header says so: Android
  gives a Tauri app one webview, so the desktop trick of driving a hidden second one cannot work
  there — but a pure-HTTP generation path needs no browser at all.

**Wish:** register `suno_api` as a selectable `music_engine` value in `jobs.rs`'s dispatch (beside
`suno`, `acestep`, `heartmula`) and expose it in Settings. Until then the module is 400 lines of
dead weight that reads as shipped.

### 1.2 Transitions can be read but not written

[Transitions.jsx](src/src/pages/Transitions.jsx) calls `listTransitionPresets` and never
`saveTransitionPreset` / `deleteTransitionPreset` — both of which exist
([styles.rs:423](src-tauri/commands/styles.rs#L423), [:437](src-tauri/commands/styles.rs#L437)) and
are wrapped. The page ships the built-in presets read-only; a user can't keep one they liked.

**Wish:** a save/delete row on that page. Compare [SoundStudio.jsx](src/src/pages/SoundStudio.jsx)
and [StyleStudio.jsx](src/src/pages/StyleStudio.jsx), which both wire the full CRUD — Transitions is
the odd one out, which suggests it was simply missed rather than decided.

### 1.3 Style samples accumulate with no way to remove one

`delete_style_sample` ([style_samples.rs:107](src-tauri/commands/style_samples.rs#L107)) is
registered and wrapped; nothing calls it. Generated samples are files on disk, so this is unbounded
growth with no in-app remedy.

**Wish:** a delete affordance in the sample studio, and a "clear all samples" in Settings → Data.

### 1.4 The learnings store writes but is never read back by the UI

[`commands/learnings.rs`](src-tauri/commands/learnings.rs) has 6 commands. Only
`recordLearningSignal` is called (from [Images.jsx](src/src/pages/Images.jsx)). The four readers —
`getProjectLearnings`, `updateProjectLearnings`, `updateUserLearnings`, `learningsLocations` — are
orphaned.

So the app collects taste signals into a store the user cannot see, correct, or empty. That is the
worst configuration of a personalisation feature: it accrues influence over generated output with no
inspection and no undo.

**Wish:** a "What the app has learned about you" panel — list the signals, let one be deleted, let
the whole store be cleared. `learnings_locations` exists precisely to say where the files are.

### 1.5 Remote render is half-connected

4 of 7 commands in [`remote_render.rs`](src-tauri/commands/remote_render.rs) are orphaned:
`buildRenderSpec`, `listRenderJobs`, `recordRenderResult`, `remoteExec`. See
[docs/REMOTE_RENDER.md](docs/REMOTE_RENDER.md) for the intended shape — the audit can't tell whether
this is mid-build or abandoned, and that ambiguity is itself worth resolving in a comment.

### 1.6 Smaller orphans, each a button

| Command | Module | What is missing |
|---|---|---|
| `build_short` | `shorts.rs` | No way to make a Short from a finished video, though the Distribution page is the obvious host |
| `list_publish_times`, `channels_missing_publish_time` | `publish_time.rs` | Per-channel publish scheduling is built and invisible; 2 of 4 commands orphaned |
| `channels_connect_all_urls` | `channels.rs` | Bulk-OAuth-by-URL, which [ARCHITECTURE.md](ARCHITECTURE.md) §3 lists as a feature of the module |
| `start_channel_creation_watcher`, `inject_channel_handle` | `channel_creation.rs` | The brand-new-channel flow [FEATURES.md](FEATURES.md) describes |
| `imagery_text_allowed`, `imagery_print_check` | `imagery_cmd.rs` | Print-safety checks that Print-on-Demand would want |
| `delete_edition` | `graphic_novel.rs` | Editions can be made, not removed |
| `delete_authored_macro` | `macro_author.rs` | Same shape |
| `autosave_status` | `autosave.rs` | Nothing shows whether the git autosave is healthy |
| `vault_put` | `vault.rs` | Vault is written only by specific flows; no generic "store a secret" |
| `kaggle_quota` | `settings.rs` | Redundant — `video_advice` fetches quota live — but see §2.3 |

---

## Part 2 — Pipeline coherence

### 2.1 Video Gen is not in the pipeline — *partly addressed 2026-08-04*

[Workflow.jsx](src/src/pages/Workflow.jsx)'s stage list is lyrics → music → analysis → images →
overlays → video → upload. **`/videogen` appears nowhere in it**, and the word "clip" does not occur
in the file.

Worse, before this audit `generate_video` was a dead end by construction: it returns ComfyUI
`/view?filename=…` URLs pointing at a Cloudflare quick tunnel, [VideoGen.jsx](src/src/pages/VideoGen.jsx)
put them in React state, and nothing persisted them. The URLs died on the next navigation or the next
tunnel rotation — whichever came first. The newest and most expensive capability in the app produced
nothing the app could keep.

**Done in this pass:** clips can now be filed onto a section. That works because the composer was
already ready for them — [`is_animated`](src-tauri/jobs.rs#L2029) in `real_ffmpeg` detects
`.mp4`/`.webm`/`.gif` on a section's `image_url` and lays it down as a moving segment instead of a
still. The missing piece was only ever the assignment.

**Still wished for:**

- **Download the clip rather than referencing it.** Filing a tunnel URL means the video must be
  composed before the session ends. Section assets are remote URLs by existing convention
  (`gen_images` returns URLs too), so this is a systemic durability question, not a VideoGen one —
  but video is where it bites hardest, because those renders cost 10–35 GPU-minutes each.
- **A `videogen` stage in the Workflow orchestrator**, so "hero shot for the chorus, stills
  elsewhere" is a pipeline the app can run rather than a thing done by hand.
- **`Section.is_video` is vestigial** — declared in [models.rs:419](src-tauri/models.rs#L419), set to
  `false` in one place, read nowhere. This pass now sets it truthfully; either give it a reader or
  delete it.

### 2.2 Stage dependencies are implicit and only correct by luck

Each Workflow stage filters songs by a field the previous stage happens to write (`audio_url`,
`status === "analyzed"`, `overlay_local_path`, `status === "video_ready"`). There is no declared
dependency graph, so a stage whose predecessor silently produced nothing reports "queued 0 songs"
and the run continues — reading as success.

**Wish:** let each stage declare `requires`, and have "run all" stop with *"Images produced nothing,
so Video has nothing to assemble"* rather than a row of green ticks over an empty result. This is the
difference between a pipeline and seven buttons in a column.

### 2.3 GPU quota is visible on exactly one page

`video_advice` fetches real Kaggle quota live and VideoGen shows it. But music (`heartmula`,
`acestep`) and images (ComfyUI/FLUX) start Kaggle sessions too, from Workflow and from their own
pages, and none of them shows what is left of the weekly 30 hours. The `idle_guard` exists precisely
because that budget is scarce.

**Wish:** quota in the Shell header or the health banner — one number, every page, since every page
can spend it.

---

## Part 3 — Quality systems that have gone quiet

### 3.1 The i18n gate is passing vacuously ⭐ worth doing before the next release

```
$ npm run i18n:gate    → 0 untranslated of 2461, ceiling 0 — ok
$ npm run i18n:check   → ui-strings.json is stale: +77 / -2
```

Both are true at once, and that is the problem. The gate measures catalogue coverage **against the
committed string inventory**; the inventory has not been re-extracted since the newest UI landed. So
77 strings — Video Gen, the image-style picker, compute providers, Kaggle diagnostics — are
untranslated *and uncounted*. The app ships 16 hand-finished catalogues at 100% and its four newest
screens are English-only, with a green gate over the top.

**Wish:** make `i18n:check` a CI failure, so the inventory cannot drift again. It was left alone in
this pass on purpose: re-extracting adds 77 strings × 16 languages to the backlog and turns the gate
red until they are done, and given the git log shows those catalogues were finished *by hand*, that
is a cost to accept deliberately rather than discover in CI.

### 3.2 Dead code with a clear verdict

- `greet` — Tauri template scaffolding, still registered.
- `get_job` — superseded by `list_jobs`; no caller.
- `oauth_start_loopback` — no caller; unclear whether superseded or unfinished.
- `proj0`, `probe_midjourney_proxy`, `ffmpeg_binary` — `cargo check` names all three as never used.
- `Section.is_video` — see §2.1.

### 3.3 `src-tauri/packaging/node_modules_backup/` — 18 MB of vendored Playwright in git

171 files, tracked, **referenced by nothing** in the Rust, the JS, the configs or the scripts.
`.gitignore` covers `node_modules` but the `_backup` suffix escapes it.

```bash
git ls-files src-tauri/packaging/node_modules_backup | wc -l    # 171
grep -rn "node_modules_backup" src-tauri/src src-tauri/commands src/src scripts   # (nothing)
```

**Wish:** delete it and add the pattern to `.gitignore`. Left in place here because removing tracked
files is a repo-history decision, not an audit's call.

---

## Part 4 — Additions the audit made look worth wanting

### 4.1 A registration test, so the audit does not need repeating

The IPC surface is clean today, and [ARCHITECTURE.md](ARCHITECTURE.md) §3 already notes that manual
registration is easy to desync. A test in [`tests_logic.rs`](src-tauri/tests_logic.rs) that parses
`lib.rs`'s `generate_handler!` block and asserts it matches the `#[command]` definitions would make
that class of bug impossible rather than periodically re-discovered. A companion node test could
assert every `api.*` method resolves to a registered command.

### 4.2 An "unreachable features" check

The 30 orphans in Part 1 were found by a three-line `comm`. As a `npm run audit:orphans` that prints
them with a committed allowlist, a feature could never again be built and left unreachable without
someone saying so out loud. This is the cheapest item in the file and it protects the most.

### 4.3 Route-level code splitting

`index.js` is **1.55 MB** (453 KB gzipped) and every one of 36 pages is in it. The 16 language
catalogues are already split; the app is not. `React.lazy` per route is a contained change against a
measured number.

### 4.4 Make the Workflow page the actual front door

The app's stated purpose is end-to-end automation, and `/workflow` is where that lives — but `/` is
the Dashboard, and the workflow is the second nav item. If chaining the pipeline is the product,
consider whether the first screen should be the one that runs it.

### 4.5 A "what can I do right now?" readiness view

Health, engine availability, quota, OAuth state and job-queue depth are each surfaced somewhere
different (HealthBanner, Insights, VideoGen, Upload, Jobs). A single readiness panel — *"music: ready
· images: server down · upload: 2 channels need sign-in · 4.2 GPU-hours left"* — would answer the
question the user actually opens the app with, and every input already exists.

---

## Appendix — the orphan list in full

`api.*` methods defined in [api.js](src/src/lib/api.js) with zero callers, verified individually:

```
autosaveStatus            buildRenderSpec           buildShort
channelsMissingPublishTime connectAllUrls           deeplinkSetupSteps
deleteAuthoredMacro       deleteEdition             deleteStyleSample
deleteTransitionPreset    falCatalogue              falGenerate
getProjectLearnings       imageryPrintCheck         imageryTextAllowed
injectChannelHandle       kaggleQuota               learningsLocations
listPublishTimes          listRenderJobs            oauthStart
openMjLogin               recordRenderResult        remoteExec
renderBubble              saveSunoCookie            saveTransitionPreset
startChannelCreationWatcher startGoogleIdSignIn     subsSignIn
sunoGenerate              sunoPoll                  sunoStatus
testVideo                 updateProjectLearnings    updateUserLearnings
vaultPut                  webviewListPages
```

Not all are defects. `subsSignIn` and `startGoogleIdSignIn` are alternates to the Google path
[Paywall.jsx](src/src/components/Paywall.jsx) actually uses; `falGenerate` and `falCatalogue` are
redundant because fal.ai is reached through the shared image-API path in
[image_apis.rs](src-tauri/image_apis.rs); `kaggleQuota` is redundant to `video_advice`. The rest are
features waiting for a button.
