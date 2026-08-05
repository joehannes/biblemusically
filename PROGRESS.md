# Progress — the "finish everything" pass

Started **2026-08-04**. A running ledger for a multi-session effort to close out
[TODOS.md](TODOS.md), [BACKLOG.md](BACKLOG.md) and [WISHLIST.md](WISHLIST.md).

**Read this file first in a new session.** It is the hand-off: what is done, what is next, and what
is blocked on something a coding session cannot supply.

## Ground rules for this pass

- Suno stays disconnected on purpose. `commands/suno_api.rs` is not to be wired into the engine
  dispatch yet — the owner wants it behind an admin switch later, not shipped on.
- Commit and push each increment. Never leave the tree dirty at the end of a session.
- Verify before ticking: `npm run build`, `npm run test:unit`, `cargo check`.

## Status board

Legend: ✅ done · 🔄 in progress · ⬜ not started · ⛔ blocked (reason given)

### Wishlist Part 1 — finish what is built

| # | Item | Status |
|---|---|---|
| 1.1 | Suno HTTP engine | ⛔ **deliberately deferred** — admin-gated later, per owner |
| 1.2 | Transitions preset save/delete | ✅ 2026-08-04 |
| 1.3 | Style sample delete + clear-all | ✅ 2026-08-04 |
| 1.4 | Learnings inspection panel | ✅ 2026-08-04 — new `forget_learnings` command + panel on Account |
| 1.5 | Remote render | ✅ 2026-08-04 — verdict: **mid-build**, now finished. `reconcile_render_jobs` + a jobs panel |
| 1.6 | Ten smaller orphans | ✅ 2026-08-04 — all resolved: eight wired, the channel-creation pair **removed as superseded** |

### Wishlist Part 2 — pipeline coherence

| # | Item | Status |
|---|---|---|
| 2.1 | Video Gen: clips file onto sections | ✅ 2026-08-04 (11c1e62) |
| 2.1b | Download clips instead of referencing tunnel URLs | ✅ 2026-08-04 — `rehome_asset` via `local_media`; both the job and the manual path |
| 2.1c | A `videogen` stage in the Workflow orchestrator | ✅ 2026-08-04 — both runners, new `section_clip` job kind |
| 2.2 | Declared stage dependencies (`requires`) | ✅ 2026-08-04 |
| 2.3 | GPU quota visible app-wide | ✅ 2026-08-04 — `GpuQuotaBanner`, modelled on `AiBudgetBanner` |

### TODOS.md — what is actually still open

| Item | Status |
|---|---|
| Mobile: `kernels push` over REST | ✅ 2026-08-04 — `kernel_push` + `start_kaggle_server_http` |
| Mobile: `kernels pull` over REST | ✅ 2026-08-04 — `kernel_pull` |
| Mobile: `kernels output` over REST | ✅ 2026-08-04 — `kernel_output` + `tunnel_url` |
| Mobile: `kernels logs -f` — **answered: it does not** | ✅ 2026-08-04 — `run_monitor_http` polls state instead |
| Mobile: `locate_kaggle()` honesty → `locate_kaggle_opt()` + `require_kaggle_cli()`; `platform_capabilities` now reports `kaggle_cli` | ✅ 2026-08-04 |
| Finish the catalogues (§8) | ✅ 2026-08-05 — all 15 at 100%, audit clean |
| i18n inventory stale; gate passed vacuously | ✅ 2026-08-05 — inventory refreshed, 133 strings translated by hand |

### BACKLOG.md — what is actually still open

| Item | Status |
|---|---|
| Multi-user / team support | ⛔ **needs a product decision** — conflict resolution on concurrent edits is unsolved |
| Mobile feature parity — decide what the phone *should* do | ⛔ **needs a product decision** |

### Infrastructure (items 4–6 of the request)

| Item | Status |
|---|---|
| **GitHub release workflow** | ✅ 2026-08-05 — was failing at `i18n:check` on a stale inventory; v0.112.0 and v0.113.0 both died there in <40s. Fixed by the catalogue work; v0.114.0's `checks` job passes. |
| ⚠️ Two asset-less **published** releases | ⬜ v0.112.0 and v0.113.0 exist as *published* (not draft) releases with **zero assets**, because the workflow creates the release before the builds run. The update endpoint only ignores *drafts*. Worth deleting. |
| Subscription Worker live | ✅ verified 2026-08-04 — `/` and `/health` 200, `/get` 302 |
| Marketing site exists + deployed to Cloudflare | ✅ verified 2026-08-04 — served by the same Worker from `server/site/index.html` |
| Subscription flow works end-to-end (sign-in → entitlement → verify) | ⬜ needs a real test |
| **Rotate the Ed25519 signing key** | ⬜ ⚠️ the private key was committed to git history up to v0.91.0; anyone with repo access can mint a lifetime entitlement |
| Android build | ⛔ **blocked on toolchain** — needs `NDK_HOME` and JDK 17 (JDK is 26); an environment change, not a code change |

## Next up, in order

Pick the top unfinished one. Each is bounded; none needs a decision from the owner.


### Waiting on the owner, not on effort

These are the ones a coding session should *not* decide by itself:

- **Suno admin switch** — the engine stays disconnected until there is one.
- **Rotate the Ed25519 signing key** — a production credential operation. ⚠️ still outstanding.
- **Subscription flow end-to-end** — needs a real account to test against.
- **Android build** — `NDK_HOME` and JDK 17; an environment change.
- **Multi-user / team**, **mobile parity** — product questions with no obvious right answer.


## Session log

- **2026-08-05 · session 2** — The catalogues. The gate was passing vacuously: it measures coverage
  against the committed inventory, and the inventory had not been re-extracted since the newest UI
  landed, so 133 strings were untranslated *and uncounted*. Inventory refreshed (three classes of junk
  dropped on the way: home-relative paths, template-literal crumbs, shell commands), then all 133
  translated by hand into all fifteen languages. **15/15 at 100%, audit clean** — nothing echoed, no
  placeholder dropped, no runaway length.

- **2026-08-04 · session 1 (iter 14)** — Remote render: the verdict is mid-build, not abandoned, so
  it is finished rather than deleted. Submitting always worked; nothing ever asked for the answer,
  so a job read "running" forever. `reconcile_render_jobs` reads the worker's `BM_RESULT` line out
  of the run log — possible cheaply now that `kernels/output` returns the log — and a panel under
  the provider picker shows the outcome. 350 Rust tests pass (3 new).

- **2026-08-04 · session 1 (iter 13)** — Clip durability. `rehome_asset` copies a generated clip off
  the render server into `local_media`, which already existed for engines that return bytes — so
  nothing downstream changed, the composer still fetches a URL. Added `webm`/`gif` to the servable
  list (SaveWEBM is what the ComfyUI video graphs write, so re-homing would have stored a clip the
  route then refused). Both the `section_clip` job and Video Gen's manual filing go through it.

- **2026-08-04 · session 1 (iter 12)** — `run_monitor_http`: the monitor no longer dead-ends without
  a CLI. TODOS asked whether a phone needs the streamed boot log; the answer is no — what it is
  watched for is one transition, and `kernels/output` carries the log in its response. Same
  `KaggleProgress`, same phases, same probe, quieter log. **All five mobile items are now closed.**

- **2026-08-04 · session 1 (iter 11)** — `kernels output` over REST. The log arrives *inside* the
  output response, so a phone finds its server's tunnel URL with no download and no temp directory —
  which also removes the CLI-only `logs -f` streaming fallback for the RUNNING case. `find_tunnel_url`
  is now shared by both transports, and takes the *last* match. 347 Rust tests pass (3 new).

- **2026-08-04 · session 1 (iter 10)** — `kernels pull` and `kernels push` over REST. A phone can now
  start a server, not only watch one. **TODOS.md was wrong** that the source is base64: Kaggle's own
  client sends it as a plain string — verified against `kaggle_api_extended.py`. Two undocumented
  requirements found and handled: cell `source` must be joined to one string, outputs must be
  stripped. 344 Rust tests pass (5 new).

- **2026-08-04 · session 1 (iter 9)** — Hero clips are a pipeline stage. New `section_clip` job kind
  (clip generation was only ever a 40-minute blocking command); `generate_clip` extracted from the
  Tauri command so the job runner can call it. Added to **both** sequencers — the frontend list and
  the backend `ALL_STEPS` — with a shared hook rule matching `shorts.rs`. Off by default: expensive,
  not irreversible. 10 workflow_run tests pass (2 new).

- **2026-08-04 · session 1 (iter 8)** — Workflow stages now declare `requires`, asked before
  `pending`. The two produce the same count and mean opposite things: a run whose images never
  rendered used to finish with a column of green ticks over an empty result. A stage whose input
  never arrived is now amber and stops the run.

- **2026-08-04 · session 1 (iter 7)** — Print safety. `imagery_print_check` now runs against the
  blueprint's real print area when a product is opened, replacing a hand-waved "at least half that"
  with the destination's own DPI floor and its advice; `imagery_text_allowed` explains why product
  art carries no words. **Wishlist 1.6 is now complete.**

- **2026-08-04 · session 1 (iter 6)** — `autosave_status` now drives the Save button's dot, which is
  what its own doc comment always said it was for. Polled every 20s so the dot can *clear* when the
  45s background sweep commits on its own.

- **2026-08-04 · session 1 (iter 5)** — The two missing deletes. Editions could be written and built
  but never removed, so a first attempt stayed in the picker forever; the confirm says page art is
  *not* swept up with it rather than implying a tidier delete than it is. Authored macros were a
  truncated sentence of the first four names — now a full list, each removable.

- **2026-08-04 · session 1 (iter 4)** — Channel-creation flow: **removed, not wired**.
  `start_channel_creation_watcher` waits on a POST to `127.0.0.1:3340` that YouTube's own page can
  never send, so it could only ever time out after five minutes; both api.js wrappers passed wrong
  argument names, proving neither had been called once. `import_channel_by_handle` already does the
  job properly and has a polished flow behind it. Deleted the module, its two registrations and the
  two broken wrappers.

- **2026-08-04 · session 1 (iter 3)** — `channels_connect_all_urls` surfaced on Channels as
  "Sign-in links": the third connect tier, for when the loopback redirect cannot work (a phone) or
  the browser is signed into the wrong Google account. Offered automatically when both automated
  tiers connect nothing, and reachable deliberately.

- **2026-08-04 · session 1 (iter 2)** — Publish-time scheduling. Found that `publish_time` was
  stored by three commands and *read by nothing* — a channel's publishing hour was decoration. Now
  real: `next_publish_instant` (new, + `chrono-tz`) resolves the channel's local hour against the
  IANA zone, and `real_youtube_upload` hands it to YouTube as `status.publishAt`. Fleet view added
  to the Upload page. 9 publish_time tests pass (5 new).

- **2026-08-04 · session 1 (cont.)** — `build_short` surfaced on Social as "Cut from the hook"; it supersedes `derive_song_versions`' simpler `cut_vertical_short` and had no caller.
- **2026-08-04 · session 1** — Audit of the whole app; WISHLIST.md written. Fixed: Video Gen
  dead-end (2.1), Transitions preset save/delete (1.2), style-sample delete + clear-all (1.3),
  learnings panel + new `forget_learnings` (1.4), GPU quota banner (2.3, and `kaggle_quota`'s first
  caller), Kaggle CLI honesty on mobile (`locate_kaggle_opt`/`require_kaggle_cli`, plus a real
  transport bug where the fallback string counted as a present CLI). Verified the subscription
  Worker and marketing site are live. Commits: 11c1e62, 574701b, 8880c49, dd2bcad, 74027db.
