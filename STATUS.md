# Status Log

A dated log of observed project state, **newest observation first** (reverse-chronological — read top-down for "what's true now", scroll to the bottom for where this app started). Each entry records what was true *as of that date*, based on git history and direct code inspection. For the current feature-by-feature state see [FEATURES.md](FEATURES.md); for open issues see [TODOS.md](TODOS.md); for what could come next see [BACKLOG.md](BACKLOG.md).

---

## 2026-09-03 — Whose voice it is written in, and the shop

**A voice layer, and the research behind its shape.** The obvious design is "write like <author>";
the obvious correction is to drop the names and give only measurable instructions. The evidence
supports neither alone. A bare name produces the model's stereotype of a writer — the caricature
effect measured for persona prompting (CoMPosT, EMNLP 2023; Marked Personas, ACL 2023), and worse for
writers with a strong popular image, exactly backwards from what a picker wants to offer. A bare
style guide is more controllable but less various: explicit guides put a ceiling on diversity where a
name lets the model draw on what it read. Instructions and exemplars together are additive, and
explicit directives control more strongly than demonstrations alone (arXiv:2511.13972).

So `commands/authorial.rs` carries both. 56 traditions — bodies of technique with a place and a
history rather than people, which is accuracy before it is ethics: "the cadence of the King James
Bible" is a describable set of moves and "write like C. S. Lewis" is an impression. Every one of the
sixteen languages has at least three of its own, which is the first time the app has known anything
about those languages beyond how to translate its own buttons. Four surface dials alongside —
sentence rhythm, figuration, concreteness, register — because those are the stylometric dimensions
measurable in a text and therefore the ones a prompt steers reliably. Wired into composing, into
authoring an edition, and into retelling, which inherits the edition's voice.

**Three thin places in the print-on-demand path**, each showing up as a worse product rather than an
error. The art was whichever came first — `break` on the first section image that existed, so a 2:3
poster and a square mug got the same picture, chosen by iteration order. The scale meant two things:
`print_quality` returns a pixel ratio for DPI and it was handed to Printify as `scale`, which is a
fraction of print-area width, so art larger than the area was shrunk into the middle of it with white
around. And every listing said the same hard-coded sentence, so two people selling different things
got byte-identical copy.

All three fixed, with the pure parts tested: art chosen per print area (printable first, then shape,
then size), placement computed from the ratio of the two aspects with fill-or-fit following the
product, crop loss reported, and a flavour — devotional, art prints, wearables, children's, memorial
— setting the register, the categories, the markup and the phrase length. Sizes are read from PNG and
JPEG headers rather than by adding a decoder dependency for two integers. Prices round up to a charm
ending, never down. The other three fulfilment APIs are named with what each is for and what wiring
it up would take, and a test asserts exactly one claims to work.

**Also corrected here:** I overwrote the existing `VoicePicker` — the assistant's speaking voice —
with a component of the same name. The IPC audit caught it as an orphaned command within the minute.

Closing figures: **547 Rust tests, 156 JS tests, build clean, i18n 100% in all fifteen languages,
both static audits clean, zero compiler warnings.**

---

## 2026-09-02 (later) — What the audit asked for, built: rotation, HTTP Suno, a book engine, and a reader

The implementation pass on top of the audit above. Baseline stayed green throughout — the closing
figures are **510 Rust tests, 156 JS tests, `npm run build` clean, i18n 100% in all fifteen
languages, both static audits clean, zero compiler warnings**.

**The compromised signing key is replaceable now, and the audit had understated it.** `SUBS_PUBLIC_KEY`
was one hard-coded constant, so rotating it would have invalidated every entitlement in the field at
once. It is `SUBS_PUBLIC_KEYS` — a list, each entry with an optional `accept_until`, so a new key
ships alongside the old one and the old one expires on a date. `server/deploy.py` gained
`--rotate-key` and `--rotate-admin-token`, and a `refuse_if_tracked()` that runs before anything
reads a credential. Verifying it against the repo rather than against the ledger changed the finding:
the leaked key is the one in service, and a shallow clone had nearly hidden that. `docs/SECURITY-KEY-ROTATION.md`
has the procedure; the last two steps are the owner's, since they need Cloudflare credentials.

**Two static audits, so the classes of defect the audit found by reading cannot recur.**
`scripts/audit-secrets.mjs` checks git's *index* by forbidden name and by forbidden content, and was
verified against the real historical blob — caught by both. `scripts/audit-ipc.mjs` finds invoked
commands that are not defined, defined commands that are not registered, and wrappers nothing calls.
Both run in CI, which now also runs on pull requests.

**Suno generates over HTTP; the browser is the fallback.** The HTTP path was implemented and the job
dispatcher never reached it — every Suno run was intercepted into browser automation before it got
there. `real_suno_http` is the `"suno"` arm now. The browser path is not gone: a failed HTTP run
offers it, with the reason on the card.

**Midjourney came back.** It had been hidden behind a flag whose stated reason — that it drives
Discord — described an integration this app replaced. It drives midjourney.com through a signed-in
browser. The flag is gone and the risk note now says what actually happens.

**The sidebar folds to the fifteen stops a song passes through**, expanding to all thirty-five on a
click, defaulting from `audience_level`. Folding rather than filtering, because the app's promise is
that the audience level never withholds a feature.

**A project is asked what it is, and then told what today is for.** `interview.rs` cascades: the
model sees every answer so far and picks the next question worth asking, writing only into the
Brief's own fields, endable at any point, with a fixed fallback set so a spent free tier can still
start a project. `guide_today` answers "what now" from what the project contains — artefacts
counted, not statuses trusted — and every step names a route.

**The composer has something to decide beyond the topic.** Six dials in a closed vocabulary — form,
faithfulness to the source, voice, shape, repetition, register — because a dial handed to a model as
free text has no defined effect. Faithfulness is stated first: for scripture, "quote it", "keep every
claim" and "take it as a starting point" are three different products, and the app had no word for
the difference. A syllable check runs client-side against the song's own median, flagging lines far
outside it, deliberately generous — a checker that cries wolf gets turned off.

**Avatar universes.** A reader written down: an avatar plus the givens their world supplies, across
nine axes ordered by how much each changes a telling. Depth takes them from the front, so a sketch is
three and deepening one later is answering more questions rather than starting over. Neighbours are
derived by naming the axes that move, with held axes restored from the base when the model drifts and
identical siblings dropped. An edition is then *retold* through a universe — written again, not
translated, page for page so the art already made still belongs where it is. The prompt block names
the avatar as one specific person and forbids generalising from the givens: the failure mode is not a
model refusing to write for this reader but one cheerfully writing for an idea of them.

**Volumes.** A book was one song; a project with forty songs had forty EPUBs and no way to say they
belong together. A volume is the manuscript — metadata, an ordered contents of chapters and parts,
front and back matter — assembled in one action from every song that has an edition and then editable
as an ordinary list. Preflight names what a retailer rejects separately from what merely makes the
book worse, and never refuses to build. The EPUB writer grew to match: publisher, description,
rights, subjects, series, pubdate, ISBN, MARC-coded contributors, typed page roles, a nested
contents, a landmarks nav, and per-page audio so a twelve-song volume narrates each chapter with its
own song.

**One section of a lyric can be rewritten** without rolling the song, which is the ordinary move of
writing and had no button. The whole song goes as context so the rewrite can match the metre and
rhymes the other sections set; splitting and splicing are pure and leave every other byte alone. The
composer also takes a universe, so a project's song and its illustrated edition are written for the
same person.

**The map, not just the pages.** Nine stops in the order a song passes through them, each naming a
route, with every stop's doneness computed from what the project contains rather than from having
visited a page — so a journey resumed after a month is correct without having remembered anything,
and a stop reopens by itself when a song is added. Where you are is the first thing unfinished, not
the furthest reached. The strip lives above the page rather than inside one, because a journey that
unmounted on navigation could not survive being followed, and it closes for good on one click.

**The guide is a conversation now.** speak → listen → interpret → apply → speak the next, with
barge-in that stops it the moment somebody talks over it — a grace window first, since the speaker
feeds the microphone and without it the loop talks over itself forever. Two misses on a question and
it hands the question back rather than asking a third time; declining is an answer, not a failure.
Everything it says outside a question lives in a prose catalogue so it ships translated, rather than
asking in German and apologising in English.

**One lyric editor, next to the lyric.** `SectionAnnotator` — the only editor that knows the
engine's tag dialect — sat on the JSON import screen, next to the JSON box it exists to replace and
two pages from where lyrics are made. It is on the song in Music Gen now.

**And the axes closed.** Section count joined the report — the sections collection just was not being
loaded — so nine of C's ten crossings are live. The tenth, thumbnails, is the one thing in the whole
plan that needs new data rather than new code: the app generates thumbnails but never records which
one an upload went out with.

**And the rotation stopped needing a credential, because it never did.** This log and the rotation
doc both said replacing the compromised key required the Cloudflare account. That was wrong. Minting
an Ed25519 pair is local arithmetic; the account is needed only to *deploy* the new private half to
the Worker that signs. Minting had simply inherited the deploy step's requirements by sitting below
`token()` in `deploy.py`'s `main()`. There are two ways to mint now and neither touches the network:
`deploy.py --mint-only`, and a card in Account (source builds only) that keeps the private half in
the app's own vault, hands over the exact `SUBS_PUBLIC_KEYS` edit, and offers the private half as
PKCS#8 — what WebCrypto's `importKey` takes — or as the raw seed. The pair is signed and verified
against itself before either half is shown, because a rotation that shipped a mismatched pair would
lock out every user silently.

It still has to be minted on the machine that will sign with it, which is the one part nobody can do
on somebody's behalf: a public key whose private half lives on a machine that no longer exists is
worse than a compromised one — nothing can sign for it, and every entitlement stops verifying.

**Still the owner's to do:** open Account in a source build, mint, paste the snippet into
`subscription.rs`, and deploy so the server signs with the new private half.

---

## 2026-09-02 — Full-app audit: the voice layer, the IPC seam, and what the guide was not reading

A whole-app pass at **v0.142.0**: 82 Rust files (48k lines), 36k lines of frontend outside the shadcn
primitives, 35 routes, 14 guided flows over 14 pages. Baseline before touching anything was green —
391 Rust tests, 115 JS tests, `npm run build` clean, i18n 100% in all fifteen languages — so this was
an audit for the things a test suite cannot see, not a rescue.

**The voice layer was built on an API this app's own webview does not have.** Two mic buttons — the
Freeform Composer's and the daily guide's in `Tours.jsx` — called `window.SpeechRecognition`
directly, the first under a comment asserting that Tauri's webview is Chromium. On Linux it is
WebKitGTK, which does not implement it, so on the primary desktop platform both could only ever
answer "speech recognition isn't available in this webview" — while `lib/voice.js`, two files away,
already handled exactly that case by recording and transcribing on the backend. Both go through
`listen()` now.

Three more things in the same layer, each affecting the feature the guided experience is built on:

- **Every spoken answer cost eight seconds.** The recorder waited out the full `maxMs` however
  briefly somebody spoke, so most of each clip was silence, sent to be transcribed. `createSilenceGate`
  ends the recording once speech has been heard and then stopped — calibrating a noise floor first so
  a fan does not read as endless speech, never stopping before `minMs`, never on silence alone,
  never past `maxMs`.
- **No language was ever passed.** Both call sites read `voicePrefs().language`, which nothing has
  ever written. It comes from the interface language now, in the two different shapes the two paths
  need: a BCP-47 tag for the recogniser, a language *name* for the sentence `stt_transcribe` builds
  for the model.
- **The assistant spoke English over a German interface.** Spoken lines never become DOM nodes, so
  the runtime translator never saw them. `translateKnown` is a lookup-only catalog read — no request,
  no key, no ledger — and every guide question and option label is already in the shipped inventory.

**The guide read habits and never results.** `guide_proposal` and `guide_templates` read the brief,
the learnings store and the user's past picks in this flow. `performance_report` has ranked
channel/language/style by median views since the analytics loop landed. The two had never met, so the
studio recommended what the user usually picks and never what actually got watched.
`performance_prompt_block` puts a few lines of the second kind in front of both, and the prompt now
says to prefer them and to *say so* when they disagree. Deliberately hard to convince: nothing below
six measured videos, no row below three, rows of three or four labelled thin, every row carrying its
own sample size. (Found while wiring it: `measured_videos` was counted after the list had been
truncated to fifty for display, so two hundred measured uploads reported as fifty.)

**The one place taste could be stated in words was permanently empty.** `learnings_prompt_block`
gives a free-text `preferences` field the last word over every counted tally; the Learnings panel
displayed it; `update_user_learnings` and `update_project_learnings` wrote it — and nothing called
them, in the whole interface. It is a textarea with a Save now. Withdrawing it needed a fix too:
`merge` is a deep merge, so `""` leaves the key in place, and `forget_learnings` only ever touched
`tally` and `signals`.

**`npm run audit:ipc`**, so the class of defect above stops being found by hand. The seam between
Rust and `api.js` is a pair of strings: a typo'd command name, a command missing from
`generate_handler!`, and a feature built and never given a button all compile, pass every test, and
fail only when somebody presses the button. The check reads all three statically, runs in CI beside
the i18n gates, and was verified by introducing each defect in turn. Orphans that are decisions live
in `scripts/ipc-orphans.json` with a reason each. Today: 395 commands, 395 registered, none broken,
none unrouted. Three of its findings were not decisions:

- The Kaggle accounts card promised the app "automatically rotates to the next account when a run is
  denied a GPU". It does not — `ensure_account_for_engine` picks an account with room *before* the
  run, and its own doc comment records that it replaced rotation because rotation only ever happened
  after eight minutes of failure and then moved to the next name whether or not it had a GPU minute
  left. `rotate_kaggle_account` was still registered and wrapped, one call from coming back. Removed;
  the copy now describes what happens.
- `kaggle_account_overview` — written, per its doc comment, as "what the Settings screen needs to
  show why a start went where it went" — was never called by that screen, which listed usernames. It
  now shows each account's remaining GPU time and the engines parked on it, with an unreachable
  account reading "quota unknown" rather than zero.
- The Section Editor's empty state said "Generate song audio in Step 8 first!" while the user stood
  on step 8, pointing at its own page instead of Music Gen.

**Zero compiler warnings**, from nineteen. Two of them were real hazards rather than tidiness:
`ComposeRequest` and `SignInRequest` each collided under `commands/mod.rs`'s glob re-exports, which
is a compile error waiting for the first `use commands::*`; `models::ComposeRequest` was a stale
duplicate of the struct `compose_lyrics` actually takes. Also removed: a per-upload channel lookup
left behind by the enrichment batching, whose result went nowhere, and a Tokio runtime built in
`run()` and never entered — a pool of idle threads for the life of the process, while everything
async there goes through `tauri::async_runtime`.

End state: **403 Rust tests, 127 JS tests**, build clean, i18n 100% in fifteen languages, IPC audit
clean. What the audit found and did *not* fix — the beginner's map problem, the four-page lyric
journey, and the combinations the app measures but never crosses — is written up in
[BACKLOG.md](BACKLOG.md) under "Requested 2026-09-02".

---

## 2026-07-25 (later) — OAuth preflight, AI overload fallback, offline GUI translation catalogs

Driven by three reports from using the app: YouTube authorization dying on Google's "Access blocked"
page, Gemini answering "the AI is busy", and the interface flickering between German and English
while OpenRouter's free daily allowance drained away. Verified with `cargo check` (clean) and
`vite build`.

**OAuth: `redirect_uri_mismatch` is now caught before the browser opens.**
- The stored redirect URI is sent **verbatim**. It used to be round-tripped through
  `Url::to_string()`, which turns `http://127.0.0.1:3335` into `http://127.0.0.1:3335/` — a
  different string to Google, which compares redirect URIs exactly. The callback port is also no
  longer re-read from the bound socket unless the client deliberately left the port out (the
  "Desktop app" case, where Google ignores the loopback port).
- `preflight_authorize()` GETs the authorization URL server-side before `open::that`. A rejected
  client/redirect makes Google redirect to its own error page with a base64 `authError` blob;
  that blob is decoded and reported as the actual reason — including the exact URI to register —
  instead of the app waiting out a 120s callback timeout it was never going to receive.
- `validate_oauth_client` runs the same probe, so **Validate** now predicts whether sign-in will
  work rather than only checking that fields are non-empty.
- One redirect default (`src/src/lib/oauthRedirect.js`). The first-run guide suggested port 8765
  while the OAuth panel suggested 3335, so whichever one the user registered, the other flow failed.

**AI: an overloaded provider falls back to Nemotron 3 Ultra instead of failing.**
- `provider_chat` is now a wrapper: on an overload-class error (429/5xx, "overloaded",
  RESOURCE_EXHAUSTED, quota, timeout) it retries once on `nvidia/nemotron-3-ultra-550b-a55b:free`
  via OpenRouter, if a key exists. Configuration errors (400/401/403) are never retried — they would
  fail identically and retrying only hides them.
- Each switch leaves a notice (`take_ai_notices`, drained on the existing 2.5s poll in `store.jsx`)
  and toasts "gemini:… was busy — used nvidia/nemotron-… instead", so an automatic provider change
  is never silent. Repeats within a minute collapse into one notice with a count.
- Three call sites that talked to OpenRouter directly (`uploads.rs` metadata, `channel_settings.rs`
  translation and flavor) now route through `provider_chat`, so they inherit the fallback, the
  provider choice and the timeout/token settings.

**GUI translation: catalogs, and an end to the request leak.**
- The leak: the translator keyed each node's original text forever, so when React reused a text node
  for different content it kept re-applying a stale translation — React wrote English, it wrote the
  old translation back, forever. That was the flicker. Worse, every changing string (counts,
  timestamps) was a cache miss, and a cache miss was an AI request: a 10-minute session burned
  OpenRouter's **50 free requests/day**.
- Now the set of translatable strings is fixed: `scripts/extract-ui-strings.mjs` extracts the 1,581
  literal strings the interface is built from into `src/src/i18n/ui-strings.json`, and only those
  (plus a small, budgeted allowance for static-looking strings it missed) may reach the AI. Runtime
  data — anything with a digit, a URL, markup, or over 80 characters — is never sent.
- German, Spanish, Portuguese and Russian ship as catalogs in `src/src/i18n/<code>.json`
  (`scripts/build-i18n-catalogs.mjs` generates them; `scripts/i18n-core-catalog.py` holds the
  hand-translated core). Switching to those is instant, offline, and costs **zero** AI requests.
  Coverage today: de 1484/1581, es 348, pt 172, ru 172 — the builder is resumable and fills the rest
  (`npm run i18n:build`) once a free-tier quota resets.
- Hidden panels are covered: the observer watches `document.body` instead of `#root`, so dialogs,
  toasts and the tour veil — all portalled outside `#root`, which is why tours stayed English — are
  translated, as are `placeholder`/`title`/`aria-label` attributes. For non-bundled languages the
  whole inventory is prefetched in the background so a panel opened later is already translated.
- Spend guards: each string is requested at most once per session even if the answer is useless, a
  provider failure pauses translation for 5 minutes instead of retrying every render, and a
  localStorage ledger caps translation at 24 requests/day across sessions.

**Research, not yet implemented.** [docs/REMOTE_RENDER.md](docs/REMOTE_RENDER.md) works out what it
takes to run ffmpeg and the YouTube upload off this machine for 50 channels (~750 videos/month):
the workload is 50–100 core-hours and 150–300 GB egress per month, the free answer is Kaggle CPU
sessions (5 concurrent, 12 h each, already integrated) or GitHub Actions on a public repo, and the
paid answer is Modal (inside its $30/month credits) or a $4.59 Hetzner CX22. It also notes that the
real ceiling is the YouTube API upload bucket, not CPU.

---

## 2026-07-25 — MongoDB retired, project data in git, remote sync, social presence, insights

A large pass driven by three requests: (1) make all persistence JSON files stored in each project's
own git repo and remove every database dependency, (2) implement the outstanding TODOS/BACKLOG
items, (3) find a free git host that can also hold the generated assets and sync automatically.
Verified with `cargo check` and `cargo test --lib` (30 tests, all passing) throughout; the frontend
was syntax-checked per change and built once at the end.

**Persistence: no database, anywhere.**
- `src-tauri/store.rs` is a JSON document store exposing the exact slice of the MongoDB API the
  codebase used — `find_one`/`find`/`insert_one`/`insert_many`/`update_one`/`update_many`/
  `delete_one`/`delete_many`/`count_documents`, the `.sort()`/`.limit()`/`.projection()`/`.upsert()`
  builders, and cursors that implement `Stream`. Because all ~380 call sites used
  `Collection<Document>` with `bson::doc! {…}` filters, the migration was a storage swap, not a
  query rewrite. Filters support `$set/$push/$unset/$inc/$addToSet/$pull/$in/$nin/$ne/$exists/$or/
  $and/$regex/$type/$gt/$lt/$elemMatch/$size/$all`.
- **`bson` stays, `mongodb` is gone.** bson is a serialization crate — no client, no sockets, no
  server — and it cross-compiles to Android, which the driver plus a native `mongod` never could.
- Project *content* (songs, sections, characters, uploads, assets, derivatives) is written to
  `<project_folder>/data/*.json`, inside the project's own git repo; global data (settings,
  projects, channels, oauth clients, presets, jobs) stays under `<config>/studio-lightkid/data`.
  Reads union the shards so callers can't tell, and writes route to the owning project — sections
  and uploads resolve theirs through their song.
- `project_sync.rs` makes every project folder a git repo at creation (with `.gitignore`, and LFS
  tracking when git-lfs exists) and a 45-second sweeper commits whatever the store flagged dirty.
- `mongo_import.rs` carries existing MongoDB data over **without depending on the driver**: it
  starts the legacy `mongod` on a scratch port and speaks ~120 lines of the OP_MSG wire protocol
  (`find`/`getMore`/`listCollections`) using bson alone. Idempotent (skips documents whose
  `_id`/`id` already exists), ordered so projects and songs land before the documents that
  reference them, and it never deletes the old directory — only an explicit action in
  Settings → Data does that. The `mongod` sidecar, its bundled 219 MB binary and its shell
  capabilities are all removed from the build.

**Remote sync + assets (free tiers checked against each provider's own docs, July 2026).**
Hugging Face is the default: 100 GB free private storage, native git-LFS, 500 GB/file hard cap.
Also GitLab (10 GiB/project incl. LFS), GitHub (repo free, 1 GB LFS), Codeberg (750 MiB + 1.5 GiB)
and any custom git URL. Two media strategies per project: keep it in the repo via LFS, or offload to
Internet Archive and keep only `data/assets.json` (path → URL + SHA-256 + size) in git, with
`restore_project_assets` fetching it back into a fresh clone. Tokens live in the new encrypted
vault and reach git through a credential helper fed by environment variables — never in
`.git/config`, never in argv, and scrubbed from error text.

**Credential vault** (`vault.rs`) — XChaCha20-Poly1305 with two honest modes: an Argon2id
passphrase key held only in memory, or a machine key in a `0600` file, with the UI stating exactly
what each protects against rather than implying more. Switching to a passphrase re-encrypts every
entry and deletes the machine key.

**Social presence** (`commands/social.rs`) — connect Mastodon/Bluesky/Telegram/Discord, ingest the
user's own posts and favourites from the open-protocol platforms, distil a taste profile with the
free in-app AI, and inject it into `compose_lyrics` and `compose_assist` so generated work lands in
the creator's voice. `ideate_next` answers "what should I make next?" from the profile, the
learnings and the project's recent output. `derive_song_versions` cuts a centre-cropped vertical
short with FFmpeg plus an image post and a teaser, and publishing is implemented for the four open
platforms; review-gated (Meta, TikTok) and paid (X) ones explain themselves instead of failing
silently. Nothing is ever published by a timer.

**Insights** (`commands/insights.rs`) — the pipeline board (every song by stage, per project /
language / style, plus anything unfinished for a week), the upload-analytics feedback loop (YouTube
view counts pulled back through the existing OAuth, ranked by *median* views so one runaway video
isn't mistaken for a strategy), and a quota report for YouTube uploads, Kaggle GPU accounts,
AI requests and disk.

**TODOS.md cleared.** Dead code deleted after verifying nothing referenced it (`main.rs`,
`test_warp.rs`, the unregistered second git implementation `commands/project_git.rs`,
`lib/tauri-api.js`, `youtube-channel-discovery.js`, craco config + CRA scripts, `plugins/
health-check`). Engine health is now surfaced by a persistent banner — but only for the engines the
current settings actually use, so it can't become wallpaper. Music generation gained a configurable
fallback engine. The two schedule fields are merged: `schedule` is derived from `schedule_config` on
every save, so the dashboard can't contradict the automation. The scheduler's AI calls take a job
semaphore permit. Characters can be attached to sections with their appearance tags and prompt
injected in one action. `tests_logic.rs` covers the pure logic (annotation pairing, mood derivation,
effect suggestion, JSON recovery, slugs, secret masking).

**Mobile + cross-platform.** The unconditional `webkit2gtk` import in `commands/webview.rs` was the
single thing keeping the crate from compiling anywhere but Linux — both its call sites were already
`cfg(target_os = "linux")`. That import is now gated, `rfd` is a desktop-only dependency with its
three call sites guarded, and the bundler targets macOS/Windows/RPM/AppImage as well as `.deb`.
Producing an actual `.apk` still needs an NDK and JDK 17 on this machine, which is a toolchain
install, not a code change.

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
