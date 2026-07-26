# TODOs

Concrete, file-anchored issues found while documenting the codebase, first on **2026-07-08**, with
follow-up implementation passes since. Each still-open item below also has an inline `// TODO` or
`// deprecated???` comment at the referenced location. Fixed items keep their original write-up for
context, with a `Fixed —` note.

## Engine access research (2026-07-26) — read this before touching any engine

Three questions were asked and answered. Two of the answers **change the recommendation**, so this
supersedes the "remove Suno/Midjourney" reasoning in the section below it.

### The distinction that matters: who breaches the terms

There are two completely different things that both get called "a third-party API":

| Kind | Who holds the account | Whose terms are breached | Verdict |
|---|---|---|---|
| **Cookie/token automation** (what this app does now for Suno; every OSS MJ proxy) | The user | **The user's** — self-botting / non-interface access | Not shippable in a product sold publicly |
| **Aggregator that runs its own accounts** (sunoapi.org, AIMLAPI, sunor, APIPASS, apiframe, evolink for Suno; Apiframe for MJ) | The provider | The provider's problem, not ours | **Legitimate purchase.** Buy an API, get audio. |

That second row is the thing worth having. Our user is not automating anything, holds no scraped
credential, and has no account to lose. The remaining risk is **vendor risk**, and it is real: if the
provider loses its access, the pipeline breaks overnight. Mitigate by keeping the engine list plural and
the fallback chain working, which the app already does (`music_engine_fallback`).

### Suno — no official API yet, but one is coming, and aggregators work today

- **No self-serve official API** as of July 2026: no key workflow, no published pricing.
- **2026-07-01: Suno's CPO said they are opening a developer API**, starting with a curated partner group,
  and opened an intake form. → **ACTION FOR THE OWNER: fill in that intake form.** This is the legitimate
  route and it costs nothing to be in the queue.
- Enterprise/partner access is invitation-only today.
- **Aggregators charge $0.014–$0.111 per song**, subscriptions from ~$19/month. Most explicitly permit
  commercial use of the output; some attach attribution or tier conditions — read the specific one.
- Suno settled with Warner Music (Nov 2025), which is why any of this is possible at all.

**Recommendation:** add an aggregator-backed `suno_api` engine (API key + base URL, exactly like the
existing paid providers) as the sellable Suno path. Keep the cookie path in the code but hidden (below).

### Midjourney — same structure, same answer

- **No official public API.** Keys exist only behind an **Enterprise dashboard**; developer access is by
  application through Midjourney's site. → **ACTION: apply**, since it costs nothing to ask.
- **Apiframe** and similar run their own Midjourney accounts — "no Discord required, no account bans" —
  which puts the risk on them, not on us.
- Everything else (Selenium against *your* account, Discord user tokens) automates the user's own account
  and is exactly what was rejected. Do not reinstate any of it.

**Recommendation:** Midjourney becomes an optional **aggregator-backed** engine, never a
self-automation one. Since FLUX, Leonardo, Ideogram and Recraft cover the same ground, this is low
priority — but it is no longer forbidden.

### Udio — re-researched properly, and the first pass was wrong in a material way

The first pass said "there is a developer REST API". **That was wrong**, and it matters: the source was an
aggregator's page selling *its own* API under the name "Udio API".

What a more careful check found:

- **There is no official Udio API.** Udio's own help centre: *"We know there's keen interest, but we don't
  currently offer a public API."* No waitlist, no announced timeline. One secondary source claims Pro and
  Enterprise API tiers with Python and Node SDKs — uncorroborated, and contradicted by the more
  authoritative source. Treat it as false until Udio itself says otherwise.
- **Song length is genuinely solved.** v4 (2026) reaches ~10 minutes without drift, up from ~2:10 on v1.5.
  The 30-second era is over.
- **Downloads are disabled right now, but not by design.** Audio, video and stem downloads are
  *temporarily* disabled across **all** plans including Pro, during the licensed-platform rollout that
  followed the **October 2025 Universal Music settlement**. Historically WAV and stems were a
  subscriber-only feature and worked fine. Occasional short download windows get announced; there is no
  guaranteed export.
- **Plans:** Free (100 credits/month), Standard ($10 / 2,400), Pro ($30 / 6,000).
- **Directional signal, not just a pause:** the UMG settlement is pushing Udio toward a consumer streaming
  experience rather than an open developer platform. That is the part that should temper optimism.

**Verdict: unusable today, for a different reason than first stated.** Not "a walled garden by
philosophy" — Udio used to allow WAV and stems and may again. It is unusable because there is *no API at
all*, and because even the aggregators cannot export what Udio is not currently letting anyone export.

**A suspicion worth carrying:** several "Udio API" vendors describe *"Udio-style music generation"*. That
phrasing suggests some of them are not Udio at all. Before paying one, generate a track and check whether
the output is actually Udio's.

**Re-check trigger:** when Udio restores downloads on Pro, or announces a public API. At ~10 minutes per
track, it would then be the strongest option on this list for whole-book compilations — so it is worth
watching rather than forgetting.

---

## THE TASK LIST — a new session can work straight down this

Ordered so each item stands on what came before. Every one of them is a decision already made; none needs
re-litigating.

### 1. Enforce the trial restriction (do this first)

`require()`, `refusal()` and `cache_key_material()` exist in `commands/subscription.rs` and are tested,
but the call sites do not use them. **Right now the terms promise a restriction the software does not
apply.** Add `require(&state, "…").await?` as the first line of:

- `save_project_version` → `"save_copies"`
- `sync_project_now`, `pull_project_now` → `"remote_sync"`
- `export_release_package` → `"export"`
- `build_epub` → `"export"`
- the Data & Sync export path → `"export"`
- `save_and_push` → `"remote_sync"`

Then encrypt the project cache with `cache_key_material()` so a new trial account cannot open an old
account's projects — the route the whole design exists to close.

### 2. Hide Suno and Midjourney rather than deleting them

The owner's instruction, and the right call: their terms may change, and Suno has an API in progress.

- A settings flag (`show_risky_engines`, default **false**) hides the `suno` and `midjourney` options from
  the engine pickers and from `list_ai_providers` / the engine capability lists.
- The job-runner branches, `real_suno`, the cookie capture route, `midjourney-generator.js` and the `mj_*`
  settings all **stay exactly as they are**. Hidden, not removed.
- When shown, each carries one line saying whose account is at risk and why.
- Defaults already moved to FLUX and ACE-Step (v0.88.x) — leave them there.

### 3. Paid image engines

`fal.ai` and `Leonardo` as new engines, plus Ideogram and Recraft as specialists. Costs and reasoning in
the section below. Each engine's own features (aspect ratio, style presets, negative prompts, SVG output,
upscale) reach the GUI through `lib/engineCapabilities.js` — the seam exists, do not invent a second one.

### 4. Music engines — the decided order

**Owner's decision, 2026-07-26. Not a recommendation to weigh up: this is the order.**

| Rank | Engine | Kind | Notes |
|---|---|---|---|
| 1 | **HeartMuLa** | free, open weights | **The default.** Nothing to breach, no account to lose, runs on a free Kaggle GPU. Defaults already moved (v0.88.x). |
| 2 | **ACE-Step** | free, Apache-2.0 | Right after HeartMuLa. Already integrated. |
| 3 | *other free engines* | free | Riffusion is the obvious next one — it has a real API, and `scripts/kaggle_riffusion/` scaffolding already exists. Stable Audio Open is worth a look. |
| — | **ElevenLabs Music** | **PAID** | Fine as a paid option, and it must be **visibly differentiated as paid** in the picker — a badge and a per-track cost, not just another row in a list. Genuine public API, commercial licensing. |
| ✗ | **Suno** | hidden | Not offered for now. Code stays (see task 2); an official API is reportedly in progress, so this may return. |
| ✗ | **Udio** | rejected | No public API at all, and downloads disabled platform-wide. See the research section. |

The picker must make the free/paid split obvious at a glance. Somebody who picked a paid engine by accident
and found out from an invoice has been treated badly.

`music_engine_fallback` already retries on another engine when one is down — any new engine must register
with it, and the fallback must never silently move somebody from a free engine to a paid one.

### 5. Mobile: the four things that make it whole

- **Markdown links → iframe split-screen** (lower/upper portrait, left/right landscape), with detection of
  the refusal and a system-browser fallback plus one line of explanation. A setting picks the default.
  Google, Kaggle, Meta and GitHub all send `X-Frame-Options: DENY`; a blank half-screen is worse than a
  browser jump.
- **Logins → system browser + deep link** (RFC 8252). This is what makes Kaggle and the social
  connections work on Android. An iframe cannot do it.
- **Folder picking → Android SAF.**
- **git via `git2`.** One function, `project_sync::git`, already `#[cfg]`-split; all 26 call sites funnel
  through it. Turn it into a typed API (commit/add/status/log/clone/pull/push/checkout_paths) with a
  desktop implementation (shell out, unchanged) and a mobile one (`git2`). The NDK C-compilation wiring
  already exists from `ring`.

### 6. ComfyUI on Kaggle — models, workflows, and controls built for *this* subject matter

Not a generic model catalogue. The images this app makes are devotional: scripture set to music, on YouTube,
and they have to read as **clean, reverent, and morally unambiguous** while still being able to carry
suspense, mystery and glory. Two researched facts decide almost every design choice below, and both are
easy to get wrong.

#### Fact 1 — FLUX cannot use negative prompts, at all

**FLUX.1 Dev was trained with guidance distillation and runs at CFG 1.** There is no classifier-free
guidance to push away from a negative prompt, so a "things to avoid" field does **nothing** on FLUX. It does
not warn, it does not fail — it silently ignores you.

That is a trap for exactly this use case, because "without getting dark or dirty" is the kind of thing
people naturally put in a negative box. So:

- The GUI must **not show a negative-prompt field for FLUX.** Gate it through `lib/engineCapabilities.js`,
  which already exists for precisely this.
- On FLUX, restraint is expressed **positively** — "modestly dressed, fully clothed, reverent, wholesome,
  serene" — because a detailed positive prompt is the documented substitute.
- A `DynamicThresholdingFull` node does enable CFG and negatives on FLUX, at roughly **2× the generation
  time**. Worth offering as an explicit "strict mode" toggle on a model that needs it, never as a default.
- On SDXL and SD 3.5 negatives work normally, so the same control produces a real negative prompt there.
  **One control, two mechanisms, decided per engine.**

#### Fact 2 — Pony Diffusion is the wrong model for this app

Trained primarily on Derpibooru with **acknowledged bias in its training data**, and it needs
`score_9, score_8_up, score_7_up` plus `rating_safe` *plus* a suppressive negative prompt to reliably stay
wholesome. V7 expanded SFW coverage, which is an improvement to a default that should never have to be
corrected in the first place.

**Recommendation: drop Pony from the curated set.** For a Christian channel, a model whose baseline needs
active suppression is a liability — one forgotten tag on one of fifty daily images is a published mistake.
Use FLUX or an SDXL illustration fine-tune for stylised work instead. If it is ever included, the score and
`rating_safe` tags must be **injected by the app, not typed by the user**, and it must carry a plain warning.

#### The curated models, judged for this subject

| Model | For this app | Negatives | Notes |
|---|---|---|---|
| **FLUX.1 Dev** | **Default.** Cleanest baseline, best prompt adherence — which matters most when the prompt is doing all the moral work | **No** (CFG 1) | Ships UNet + CLIP + VAE as separate files; the loader config differs from a single checkpoint |
| **Juggernaut XL** (SDXL) | Faces, hands, skin, robes, human presence. Where a real negative prompt earns its place | Yes | Less literal with long prompts than FLUX |
| **SD 3.5** | All-rounder, best open text rendering — useful for verse overlays baked into art | Yes | Heavier than SDXL for the gain |
| **Krea 2 RAW / Turbo** | Aesthetic variety; Turbo for drafts, RAW for finals | Verify | New in 2026, least documented |
| **Qwen-Image** | Investigate — strongest open model for text *inside* images | Verify | Would suit verse cards |
| ~~Pony~~ | **Excluded**, see above | — | — |

#### The controls — human grammar over machine parameters

Every control below is one thing an artist would actually say, mapped to real mechanics per engine. None of
them expose a sampler name or a CFG number.

- **Light** — *dawn* / *shafts through cloud* / *candlelit* / *golden hour* / *storm light* / *starlit*.
  Lighting does more devotional work than any other single lever and it is the safest to hand over.
- **Mood** — *serene* / *hopeful* / *solemn* / *awe* / *held breath* / *glory*. Note what is deliberately
  absent: no "dark", no "ominous", no "horror". The vocabulary itself keeps the output clean.
- **Suspense without darkness** — the specific requirement, and it is a *technique*, not a slider toward
  black. Mechanically: dramatic chiaroscuro, weather, vast scale, a small figure against something immense,
  a withheld reveal. Never gore, never occult imagery, never a horror palette. The control should say
  "tension" and produce that, and it is worth a comment in the code explaining why it does not simply
  darken the scene.
- **Restraint** (default **on**) — modest clothing, no gore, no occult symbolism, nothing sensual. A real
  negative prompt on SDXL/SD3.5; positive phrasing on FLUX. Off is a deliberate act, not a default.
- **Figures** — *no faces shown* / *faces shown* / *symbolic only*. "No faces" (backs, silhouettes, hands,
  feet, robes) is the option many Christian channels actually want — it sidesteps depicting Christ's face,
  and it happens to sidestep the face-consistency problem entirely. Worth offering first rather than as an
  afterthought.
- **Era** — *ancient Near East* / *timeless and abstract* / *modern parable*.
- **Symbolism** — *subtle* → *overt* (light, water, bread, vine, dove, lamp, path, door). Subtle by default;
  overt reads as clip-art at fifty images a day.
- **Shape** — the aspect ratios the app already uses: 16:9 video frames, 9:16 shorts, 2:3 book pages, 1:1
  panels and covers. Never a free-text pixel size.

#### Destination presets — the *applied* half of the controls

The artistic controls above are the **voice**. This is the **intent**. They are separate because the same
prompt, at the same aspect ratio, is not the same usable image for two destinations — and the differences
are hard platform constraints, not taste. A user should pick where it is going, once, and never have to
remember any of the numbers below.

Each preset sets shape, resolution, composition rules and which workflow runs. The artistic controls then
layer on top unchanged.

| Destination | Shape & size | The constraint that actually bites |
|---|---|---|
| **YouTube video frame** | 16:9, ≥1920×1080 | Composition must survive the pan-and-zoom the video assembler applies — a subject at the very edge gets cropped out mid-move. Leave margin on all four sides. |
| **YouTube thumbnail** | 16:9, 1280×720 | Must read at **120px wide** in a sidebar. One subject, high contrast, no fine detail. This is a different image from the video frame, not a resized one. |
| **Shorts / Reels / TikTok** | 9:16, 1080×1920 | Platform UI covers roughly the **top 10% and bottom 20%** — captions, handles, buttons. Everything that matters lives in the middle 70%, and the app already burns a link card into the bottom third, so that band must stay visually quiet. |
| **Instagram feed** | 1:1 or 4:5 | Cropped to 1:1 in grid view whatever you upload — so compose for the square even when delivering 4:5. |
| **Ebook page** | 2:3, 1600×2400 | The only destination where fine detail survives, since it is read close up. Keep the inner edge clear for the gutter. |
| **Panel** | 1:1, 1600×1600 | Read on a phone in a stack; needs a single clear focal point per panel. |
| **Release cover** (Spotify et al.) | 1:1, **≥3000×3000** | **No text at all** — the stores draw the title themselves, and burnt-in text is a common rejection. Must read as a thumbnail at 64px. |
| **Printify product art** | Per blueprint, from the catalogue API (typically 3000–4000px) | **Transparent PNG, 300 DPI.** Bold shapes only: DTG printing loses thin lines and light-on-light contrast. The real print-area size per variant is already fetched by `printify_blueprint_detail` — use it rather than a guess, and run `print_quality()` before creating anything. |
| **Publicity cover** | Per platform, from `platform_spec` | Sized for the platform the piece is written for; the article's subject, not the song's cover. |

Two rules that follow from the table and should be enforced, not documented:

- **A destination that forbids text must not receive text.** The release-cover prompt says "no lettering
  anywhere" for a reason (`generate_release_cover` already does this). Any workflow that could add text must
  be unavailable for that destination rather than merely discouraged.
- **A destination with a print resolution must be checked against it before anything is created.** 1024px
  into a 4000px print area is 77 DPI — a refund, not a product. `print_quality()` exists; the preset should
  refuse to proceed rather than reporting it afterwards.

#### Packaged intents — what the user actually picks

One choice that sets destination *and* a sensible voice, because "a shirt" and "a chapter opener" want
different treatment even at the same aspect ratio. These are the presets to ship:

- **Chapter opener** — YouTube frame, dawn light, serene, subtle symbolism, no faces. The workhorse.
- **The turn** — YouTube frame, storm light, held breath, vast scale. The suspense preset, and the one that
  proves tension does not need darkness.
- **Glory** — YouTube frame, shafts through cloud, awe, overt symbolism. For the passage that earns it.
- **Thumbnail** — one subject, high contrast, readable at 120px, no scripture text baked in.
- **Short hook** — 9:16, subject in the middle 70%, bottom third kept quiet for the link card.
- **Wearable phrase** — Printify, transparent, bold shapes, two colours or fewer, sized to the blueprint.
  Pairs with the eight-word phrase limit `print_phrase()` already enforces.
- **Mug / hard surface** — Printify, higher DPI demand than fabric, so it refuses low-resolution art rather
  than warning about it.
- **Album cover** — 1:1, ≥3000px, no text, reads at thumbnail size.
- **Book page** — 2:3, fine detail allowed, gutter margin respected.
- **Study card** — SD 3.5 or Qwen-Image specifically, because this is the one case where text *inside* the
  image is the point.

Each preset states in one line what it is for and what it will refuse — a preset that silently drops a
constraint is worse than no preset, because the user stops checking.

#### The workflows — the half that is worth more than checkpoints

- text-to-image at each of the four aspect ratios above;
- **character-consistent** generation from the existing `appearance_tags`, via IPAdapter/ControlNet plus a
  fixed seed, so a face survives a whole book — and a "no faces" path that needs none of it;
- **upscale and detail passes for print**, where a 1024px image is currently **77 DPI and a refund** (see
  `print_quality` in `commands/printify.rs`);
- img2img and inpainting, to fix one panel without regenerating a chapter;
- **transparent background** for the Printify art path;
- a **draft/final** pair per model (Turbo or few-step for drafts, full for finals), because reviewing fifty
  images a day at final quality wastes the GPU hours the free tier gives you.

#### Deployment

Same shape as the existing engine notebooks: Kaggle notebook plus a cloudflared tunnel, model files fetched
on start with the checkpoint auto-resolve that already exists (v0.51.0). Insist on `.safetensors`
throughout — it loads faster and cannot contain executable code, so a model download never becomes a code
download.

#### The copy is half the job

"Juggernaut XL" tells an artist nothing. "Photographic — best for faces, hands and fabric; slower; ignores
long instructions" tells them everything. Every model needs: what it is for, what it is bad at, roughly how
long it takes on the free GPU, whether a "things to avoid" field will do anything, and its aspect-ratio
sweet spot.

### 6b. The typography layer — words on products, and speech bubbles in panels

**Decided: render the words with real fonts, never generate them.** One mechanism serves both the Printify
phrases and the graphic-novel bubbles, because it is the same problem twice.

#### Why not an image model, even a good one

Ideogram 3.0, Recraft and Qwen-Image are genuinely good at text — and "good" means *occasional* letter
errors. On a mug that is a **returned order**, not a retry, and at fifty products a day nobody proofreads
every one. It compounds with resolution: generated images are 1024–2048px against print areas of
3000–4000px, and upscaled letterforms go soft in a way that reads as cheap where soft scenery does not. And
the phrase can never be edited afterwards without regenerating the artwork.

**AI makes the artwork; typography lays the words on top.** That is how merch and comics are actually made,
and it is already how this app draws the link card on shorts.

#### The transport already exists

`magick` and `convert` are **already in `ALLOWED_TOOLS`** (`commands/remote_exec.rs`), and `find_font()`
already exists in `commands/shorts.rs`. So text compositing runs locally on desktop and on Modal from a
phone with no new machinery — the same path the shorts cutter uses. This is a smaller job than it looks.

Render as **SVG, rasterise once** at the exact target size: real vector layout, text-on-path for curves, and
Recraft's genuine SVG output could drop into the same pipeline for ornaments.

#### Font licensing — the part that can actually cost money

- **Google Fonts are almost entirely SIL OFL or Apache 2.0.** Both permit commercial use including print on
  merchandise. ~1,800 families, and that is the safe pool.
- **Selling a product with text rendered in a font is not distributing the font.** Always fine.
- **Bundling the font file in the app *is* distribution** — OFL permits it if the licence ships alongside. So
  bundle a curated dozen with their `OFL.txt`, or fetch from Google Fonts at runtime.
- **Never let a user point the app at an arbitrary font file without saying this:** many "free font" sites
  mean free for *personal* use, and merchandise is precisely what they exclude.

#### Decoration method changes the design, and Printify tells us which

The variants response carries `decoration_method` — observed values include `dtf` and **`embroidery`**.
Embroidery cannot do gradients, fine detail or many colours at all; a design that looks lovely on DTF becomes
an unrecognisable blob stitched. DTG loses thin strokes and light-on-light generally.

So the renderer must **read the decoration method and adapt**: minimum stroke weight, a colour-count cap for
embroidery, no hairline serifs at small sizes, and a refusal rather than a warning when the phrase cannot
survive the process.

#### Layout templates — typography without layout looks like a document

Stacked centred · big word + subline · two lines with a rule · arc and curve (`-distort Arc`, or SVG
`textPath`) · circular badge · left-aligned block · **verse + reference** ("Be still and know" /
*Psalm 46:10*), which is the shape most of this app's phrases actually want.

Guards on every one: transparent PNG, 300 DPI at the blueprint's **real** print area (already fetched by
`printify_blueprint_detail`), safe margins inside it, and `print_quality()` consulted before anything is
created.

#### Speech bubbles — and why compositing beats baking them in

The `panels` register in `commands/graphic_novel.rs` already produces a caption (≤12 words) and a line of
dialogue per page. **The data exists; only the rendering is missing.** No comic-specific image model is
needed — `jobs.rs` already has `comic` and `graphic_novel` style prefixes for the artwork itself.

Compositing is not a compromise here, it is better on four counts:

1. **The text stays editable.** A typo is a re-render of the bubble, not of the panel.
2. **Translation.** This app already ships sixteen languages and generates per-language songs. With the
   bubble composited, *one artwork serves every language* — re-render the words, keep the picture. Baked-in
   text would mean regenerating every panel per language.
3. **Placement is a decision made after seeing the art** — a bubble must not cover a face.
4. **Reading order** is layout, not generation.

**In the EPUB, use HTML and CSS rather than a rasterised overlay.** The pages are already XHTML, so a bubble
positioned over the image with CSS gives **selectable text, screen-reader accessibility, and reflow** — all
of which a PNG destroys. Rasterise the same SVG only for exported images and print.

Bubble kinds: rounded rect with a tail (speech) · scalloped cloud (thought) · jagged (shout) · plain
rectangle (narration or caption box). Auto-size to the text, then place the tail toward a speaker anchor.

**Compose the panel *for* the bubble.** The art prompt should request negative space where the words will go
— "uncluttered upper third", "clear sky upper left". That is a real technique and it is the difference
between a bubble that sits in the composition and one that vandalises it. The `panels` register's art prompt
should say so.

Lettering conventions are worth respecting: comic lettering is conventionally all-caps in a lettering face
rather than a body face. Comic Neue is OFL; there are proper lettering faces in the Google pool.

### 7. The rest of the subscription surface

Feedback view with templates and share-to-social; the T&C in the Welcome Guide (the `Markdown` component
already renders it); analytics event wiring through `track_events`; Hotjar as an opt-in only.

### 8. Finish the catalogues

93% across fifteen languages, ~150–300 strings each. **By hand, not Gemini** — the owner asked for this
explicitly.

### 9. Android build

Never yet attempted. Unsigned APK for sideloading first (`docs/INSTALL.md` covers the system toggle), then
a signed AAB. **The Play Store submission needs the owner**: a developer account, agreements accepted in
person, a listing, screenshots and a content rating. Nothing about that can be automated.

### 10. A short link

Needs a domain (~$10/yr, free to attach to the Worker). `workers.dev` subdomains are fixed to the account
name and cannot be made short. Until then, add a `/get` path as an alias.

## Open milestones (2026-07-26, decided) — the commercial pass

Decisions already taken, so a new session does not re-litigate them.

### Removed on purpose: Midjourney. Under review: Suno.

**Midjourney is out.** Every route to it (trueai-org/midjourney-proxy, novicezk/midjourney-proxy,
imagineapi, midjourney-ui) drives Midjourney's Discord channel with a Discord **user** token. That is
self-botting, against Discord's terms, and a termination takes the Midjourney subscription with it. Not a
risk to ship inside a product sold publicly. Defaults moved to FLUX (v0.88.x); the remaining work is
removing the Playwright path (`packaging/midjourney-generator.js`, the `midjourney` job branch, the
`mj_*` settings) rather than leaving dead code that looks supported.

**Suno carries the same class of risk** and the same reasoning applies — its terms restrict access to its
own interface, and the cookie path is outside that. The difference is whose account dies: the user's Suno
subscription rather than a Discord account. Owner's decision pending. The recommendation on the table:
keep it, **off by default, opt-in, warned in the UI**, with ACE-Step as the default and a licensed API as
the premium tier — or remove it for consistency with Midjourney.

Legitimate music alternatives, in order of readiness: **ACE-Step** (Apache-2.0, already integrated),
**HeartMuLa** (integrated), **Riffusion** (real API now; `scripts/kaggle_riffusion/` scaffolding exists),
**ElevenLabs Music** (public API, commercial licensing — the strongest paid candidate).

### Paid image engines to add

Researched July 2026.

| Engine | Why | Cost |
|---|---|---|
| **Leonardo** — primary | A meta-platform with one API: Flux.2 Pro, Ideogram 3.0, Recraft, Nano Banana Pro, Seedream 4.5 plus its own Lucid/Phoenix. One subscription instead of six accounts. | $12/mo for 8,500 credits ≈ **$0.007/image**; API from a $5 non-expiring credit |
| **fal.ai** — pay-per-use | No subscription, widest model list, free starter credits | ~$0.06/image Flux Pro, $0.03 Seedream V4 |
| **Ideogram 3.0** — specialist | Best text *inside* images by a distance | $0.03–0.09 |
| **Recraft V4** — specialist | #1 for logos, and real SVG output | $0.04 raster / $0.08 vector |

Each engine's own features (aspect ratios, style presets, negative prompts, vector output, upscale)
should reach the GUI through the existing `engineCapabilities.js` seam rather than a new mechanism.

### ComfyUI on Kaggle — expand it

More preconfigured checkpoints with **artist-readable** descriptions: what each one is actually good at,
what it is bad at, and what it costs in time. Selectable in the GUI, transparent about ability rather than
listing model names. The per-model config already exists (v0.51.0 checkpoint auto-resolve); this is
catalogue work plus honest copy.

### Markdown links on mobile — split screen, with a fallback that admits the limit

Side-by-side beats jumping back and forth, so: try an **iframe split-screen** (lower/upper on portrait,
left/right on landscape), detect the refusal, and fall back to the system browser with one line of
explanation. A setting picks the default.

The limit is real and must not be hidden: Google, Kaggle, Meta and GitHub all send `X-Frame-Options:
DENY`. A blank half-screen with no explanation is worse than a browser jump.

### Logins on mobile — deep links, decided

System browser plus a deep link back, per RFC 8252. This is what makes Kaggle and the social connections
work on Android. An iframe cannot: those pages refuse framing by design.

### Still open from earlier passes

1. **Gate the export/save call sites.** The highest-value item: `require()` and the entitlement-derived
   cache key exist, but `save_project_version`, `sync_project_now`, `export_release_package`, `build_epub`
   and the data export do not call them yet — so the trial restriction is *described* in the terms and not
   *enforced* in the app. Fix this before anything else.
2. **Encrypt the project cache** with `cache_key_material()`, which closes the "new trial, re-import"
   route the whole design rests on.
3. **Mobile git via `git2`.** One function, `project_sync::git`, already `#[cfg]`-split; 26 call sites all
   funnel through it. Becomes a typed API (commit/add/status/log/clone/pull/push/checkout_paths) with a
   desktop and a mobile implementation.
4. **Android SAF folder picking.**
5. **Feedback view** with templates and share-to-social; **T&C in the Welcome Guide**; **analytics event
   wiring**; **Hotjar opt-in**.
6. **Catalogues at 93%** — to be finished by hand, not by Gemini.
7. **Android build** — never yet attempted; signed AAB and Play Store submission need the owner's
   developer account.
8. **A short link** needs a domain (~$10/yr, free to attach to the Worker). `workers.dev` subdomains are
   fixed to the account name and cannot be short.

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

**Extended in v0.86.0:** the five persisted channel-wide settings (topic, branding, about, content style,
upload schedule) now opt in alongside the Project Brief. Deliberately *not* annotated: the new-channel
form and the tag staging boxes — those are forms and filters, and staging them would write half-typed
values into the project. Remaining views are the same one-line-per-field pass.

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

**Closed in v0.86.0:** Media Overlays are generated. One SMIL per page, timed from the song's own
analysed section starts where there are any and by equal division otherwise — and the result reports
which of the two it got, because a guess that reads along roughly must not be advertised as synced.
Validated externally: overlays present in the archive, manifested in the OPF, well-formed XML, and each
`text src` pointing at an id the page really carries (a target that does not exist makes an overlay
silently do nothing). Granularity is paragraph-level; word-level would need forced alignment.

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
