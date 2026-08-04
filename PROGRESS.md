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
| 1.5 | Remote render — resolve mid-build vs abandoned | ⬜ |
| 1.6 | Ten smaller orphans | 🔄 `kaggle_quota`, `build_short`, publish-time pair, `channels_connect_all_urls` done; five to go |

### Wishlist Part 2 — pipeline coherence

| # | Item | Status |
|---|---|---|
| 2.1 | Video Gen: clips file onto sections | ✅ 2026-08-04 (11c1e62) |
| 2.1b | Download clips instead of referencing tunnel URLs | ⬜ |
| 2.1c | A `videogen` stage in the Workflow orchestrator | ⬜ |
| 2.2 | Declared stage dependencies (`requires`) | ⬜ |
| 2.3 | GPU quota visible app-wide | ✅ 2026-08-04 — `GpuQuotaBanner`, modelled on `AiBudgetBanner` |

### TODOS.md — what is actually still open

| Item | Status |
|---|---|
| Mobile: `kernels push` over REST (blocks starting/stopping any server from a phone) | ⬜ |
| Mobile: `kernels pull` over REST | ⬜ |
| Mobile: `kernels output` over REST | ⬜ |
| Mobile: `kernels logs -f` — decide whether a phone needs the live boot log at all | ⬜ |
| Mobile: `locate_kaggle()` honesty → `locate_kaggle_opt()` + `require_kaggle_cli()`; `platform_capabilities` now reports `kaggle_cli` | ✅ 2026-08-04 |
| Finish the catalogues (§8) | ⬜ |
| i18n inventory stale by 77 strings; gate passes vacuously | ⬜ |

### BACKLOG.md — what is actually still open

| Item | Status |
|---|---|
| Multi-user / team support | ⛔ **needs a product decision** — conflict resolution on concurrent edits is unsolved |
| Mobile feature parity — decide what the phone *should* do | ⛔ **needs a product decision** |

### Infrastructure (items 4–6 of the request)

| Item | Status |
|---|---|
| Subscription Worker live | ✅ verified 2026-08-04 — `/` and `/health` 200, `/get` 302 |
| Marketing site exists + deployed to Cloudflare | ✅ verified 2026-08-04 — served by the same Worker from `server/site/index.html` |
| Subscription flow works end-to-end (sign-in → entitlement → verify) | ⬜ needs a real test |
| **Rotate the Ed25519 signing key** | ⬜ ⚠️ the private key was committed to git history up to v0.91.0; anyone with repo access can mint a lifetime entitlement |
| Android build | ⛔ **blocked on toolchain** — needs `NDK_HOME` and JDK 17 (JDK is 26); an environment change, not a code change |

## Next up, in order

Pick the top unfinished one. Each is bounded; none needs a decision from the owner.

4. **Channel creation flow** — `start_channel_creation_watcher` + `inject_channel_handle`.
5. **`delete_edition` / `delete_authored_macro`** — two missing deletes beside existing creates.
6. **`autosave_status`** — show whether the git autosave is healthy, next to the Save control.
7. **Print safety** — `imagery_text_allowed` / `imagery_print_check` on Print-on-Demand.
8. **Workflow stage `requires`** (WISHLIST 2.2) — declared dependencies so an empty stage stops the
   run rather than reporting a green tick over nothing.
9. **A `videogen` stage in the Workflow orchestrator** (WISHLIST 2.1c).
10. **Mobile: `kernels push` over REST** — the largest remaining mobile item; TODOS.md has the
    endpoint shape (JSON body, notebook base64 in `text`, *not* multipart).

## Session log

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
