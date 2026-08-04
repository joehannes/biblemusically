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
| 1.4 | Learnings inspection panel | ⬜ |
| 1.5 | Remote render — resolve mid-build vs abandoned | ⬜ |
| 1.6 | Ten smaller orphans (shorts, publish-time, bulk OAuth, channel creation, print checks, deletes, autosave status, vault_put) | ⬜ |

### Wishlist Part 2 — pipeline coherence

| # | Item | Status |
|---|---|---|
| 2.1 | Video Gen: clips file onto sections | ✅ 2026-08-04 (11c1e62) |
| 2.1b | Download clips instead of referencing tunnel URLs | ⬜ |
| 2.1c | A `videogen` stage in the Workflow orchestrator | ⬜ |
| 2.2 | Declared stage dependencies (`requires`) | ⬜ |
| 2.3 | GPU quota visible app-wide | ⬜ |

### TODOS.md — what is actually still open

| Item | Status |
|---|---|
| Mobile: `kernels push` over REST (blocks starting/stopping any server from a phone) | ⬜ |
| Mobile: `kernels pull` over REST | ⬜ |
| Mobile: `kernels output` over REST | ⬜ |
| Mobile: `kernels logs -f` — decide whether a phone needs the live boot log at all | ⬜ |
| Mobile: `locate_kaggle()` returns `"kaggle"` when absent → say "needs a desktop" instead of a spawn error | ⬜ |
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

## Session log

- **2026-08-04 · session 1** — Audit of the whole app; WISHLIST.md written; Video Gen dead-end fixed
  (2.1). Verified the subscription Worker and marketing site are live. Set up this ledger.
