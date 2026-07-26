# Distribution — what the research actually found

Checked **July 2026**. Everything below is a fact that changes what the app can build, which is why it
is written down rather than left in a commit message. Prices and policies move; re-verify before
paying anybody.

## 1. No self-service distributor has an upload API

This is the finding that shaped the whole module.

| Distributor | Public upload API? |
|---|---|
| DistroKid | **No.** Private B2B integrations only; they are currently hiring API engineers to build one. A reverse-engineered Go wrapper (`distrogo`) exists and is not endorsed. |
| Amuse | No developer documentation published. |
| RouteNote | No developer documentation published. |
| CD Baby | No developer documentation published. |
| TuneCore | No developer documentation published. |
| Symphonic | Partner/label API on request, not self-service. |

APIs *do* exist one tier up — LabelGrid publishes a REST API under `api.labelgrid.com/api/public`
covering catalogue, audio/artwork upload, DSP validation, delivery and royalty read-back, with a
sandbox on every plan; several newer DDEX-delivery platforms advertise similar surfaces. All of them
sell to labels and distributors, not to individual artists, and all require a plan or a sales
conversation.

**So the app automates the part that can be automated properly:** it assembles a complete, correctly
named, metadata-complete release package (numbered audio, square cover, `metadata.csv`,
`release.json`, `AI-CREDITS.txt`, `UPLOAD-STEPS.txt`) and hands the upload itself to a person or to an
AI-authored browser macro. A fake "deliver" button would only fail slower.

## 2. AI-generated music is refused by some distributors and capped by others

This matters more than price here, because everything this studio produces is AI-generated.

| Distributor | AI policy | Rate cap | Excludes | Price (verify) |
|---|---|---|---|---|
| RouteNote | Accepts; asks for links to the tools used | none stated | — | Free (85% royalty) or $10–30/release premium (100%) |
| Symphonic | Accepts fully AI-generated and AI-assisted | none stated | — | Subscription, quote-based |
| UnitedMasters | Does not explicitly restrict AI | none stated | — | Free tier takes 10%; $19.99/$59.99 a year |
| Amuse | Accepts, with discretionary detection | **10 releases / 7 days** | Meta, YouTube Content ID | Free tier, or ~$24–60/year |
| DistroKid | Accepts if you hold the rights and impersonate nobody — **but excludes "mass auto-generated content"** | none stated | — | ~$23–25/year for one artist |
| LANDR | Accepts under strict limits | **12 AI songs / month** | YouTube Content ID, Meta, TikTok, Deezer, Pandora | Subscription |
| **TuneCore** | **Rejects 100% AI-generated works** | — | — | — |
| **CD Baby** | **Rejects fully AI-generated**; accepts AI-assisted with meaningful human authorship | — | — | — |

Two consequences the code enforces:

- The distributor list is sorted AI-policy first, and the two that refuse are shown with the reason
  rather than hidden — finding this out by having a release rejected costs an account, not an upload.
- `pacing()` counts releases already sent to a distributor inside its own rolling window, so a
  scheduler cannot walk into Amuse's ten-per-week or LANDR's twelve-per-month limit.

DistroKid's "no mass auto-generated content" clause deserves a direct read before pointing a
fifty-channel daily schedule at it: a daily generated release is exactly the shape that clause
describes, whatever the rights position.

## 3. Spotify requires an AI disclosure

Spotify rolled out a **DDEX-based AI credits standard** across distributor partners in 2026: AI
contributions to vocals, lyrics, melody and instrumentation must be disclosed. Several distributors
now ask for it in the upload flow, and RouteNote asks for links to the tools.

Every exported package therefore carries `AI-CREDITS.txt`, filled in from what actually made the song
— the music engine per track, the lyrics model from settings, and whether a human edited anything.
Declaring "no AI" on a generated release is the one mistake here that can take down a whole catalogue.

## 4. Short-form platforms: which ones pay

The studio already cuts a vertical short. This is which of them is worth cutting for.

| Platform | Pays per view? | Threshold | Rate |
|---|---|---|---|
| TikTok Creator Rewards | Yes — **only over 1 minute** | programme eligibility | ~$0.50–1.00 per 1,000 qualified views |
| YouTube Shorts | Pooled share of feed ad revenue | 1,000 subs + 10M Shorts views/90 days | ~$0.01–0.10 per 1,000 |
| Facebook Reels | Yes | 10,000 followers + 600,000 plays/60 days | ~$0.005–0.01 per 1,000 |
| Snapchat Spotlight | Revenue share | $100 payout floor, 45 countries | varies |
| Instagram Reels | No per-view payout | 10,000 followers for gifts/subscriptions | — |
| Pinterest | **No.** Creator Rewards ended in 2023 and was not replaced | — | traffic only |

The load-bearing detail is TikTok's one-minute line: a clip under a minute earns **nothing** from
Creator Rewards however well it performs. So `platform_spec("tiktok")` defaults to **75 seconds**,
deliberately past the line — while Shorts stays under 60, because that feed rejects a minute or more.
This is the one place where "make it 60 seconds", the obvious number and the one every other platform
wants, is the expensive choice.

## 5. What the app does with all of this

- `commands/distribution.rs` — the distributor matrix above as data, release/artist storage, release
  pacing, `metadata_csv`, ISRC normalisation, the AI-credits block, and `export_release_package`.
- `commands/compilation.rs` — whole-book compilations: chapter order parsed from song titles, concat
  through the *filter* (not the demuxer, which silently breaks on mismatched inputs), and a
  timestamped description YouTube turns into chapter markers.
- `pages/Distribution.jsx` — releases, compilations, the distributor comparison, and artist profiles.
- Uploads themselves: the Macro Manager. Record the upload once against a distributor's web form and
  the macro replays it for the next release.
