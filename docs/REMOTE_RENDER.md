# Rendering and uploading off this machine

Research for the goal: **50 channels × one 5–10 min video every other day**, with ffmpeg assembly and
the YouTube upload happening on somebody else's computer, and as little traffic as possible crossing
the user's own connection. Two tracks are wanted and both are covered below: a **free** track that
must work with no card on file, and a **cheap paid** track as an optional upgrade.

Prices and limits verified 2026-07-25; every figure has a source link at the bottom.

## 1. The workload, in numbers

| Quantity | Value | How it was derived |
| --- | --- | --- |
| Videos per day | ~25 | 50 channels ÷ every other day |
| Videos per month | ~750 | 25 × 30 |
| CPU per video | 2–4 min on 2 cores | image slideshow + audio → 1080p30 H.264, `x264 -preset veryfast`; the input is a handful of stills, so it is encode-bound, not decode-bound |
| CPU per month | **50–100 core-hours** | 750 × 3 min × 2 cores |
| Output size per video | 150–400 MB | 1080p30, ~3–6 Mbit/s, 5–10 min |
| Egress per month | **150–300 GB** | 750 uploads to YouTube |
| Ingress per month | 150–450 GB | source images + audio pulled to the worker |
| Peak concurrency needed | 2–4 workers | 25 videos × 3 min = ~75 min of work per day; even one worker fits, concurrency is only for latency |

Two conclusions worth stating before any vendor comparison:

**Compute is not the bottleneck — 50–100 core-hours a month is small.** Egress is the number that
kills free tiers, and ffmpeg encode speed is what decides whether one worker is enough.

**The real ceiling is the YouTube API, not the CPU.** As of the 2025-12-04 change `videos.insert`
costs ~100 units instead of ~1600, and since 2026-06-01 uploads bill against their own bucket of
~100 calls/day rather than the shared 10,000-unit pool. So ~25 uploads/day now fits inside **one**
OAuth client, where before it would have needed five (6 uploads/day each). The existing OAuth client
pool in Channel Manager is still the right shape — it just stopped being mandatory at this volume.
Check the current quota on your own project before scaling: the audit/extension form is free.

## 2. Free track

| Option | What you get free | Fits 750 videos/mo? | Cost of "no" |
| --- | --- | --- | --- |
| **Kaggle CPU sessions** (recommended) | 12 h per session, **5 concurrent CPU sessions**, no weekly CPU cap (the 30 h/week cap is GPU-only), unmetered egress | **Yes, comfortably** — 75 min of work/day against 60 h/day of available session time | — |
| **GitHub Actions, public repo** | Unmetered minutes on standard runners, 2-core, 6 h job limit | **Yes** | Repo must be public |
| **GitHub Actions, private repo** | 2,000 min/mo (Free plan) | No — needs ~2,250 min/mo | $4/mo Pro → 3,000 min, or $0.006/min |
| **Oracle Cloud Always Free** | Ampere A1 VM, **10 TB/mo egress**; A1 allowance dropped to 2 OCPU / 12 GB for free-tier accounts in June 2026 | Yes on paper | A1 capacity is often unavailable in a given home region; idle reclamation applies |
| Hugging Face Spaces | Static Spaces only; compute Spaces now need a paid plan | No | — |
| Google Colab free | Interactive, no scheduling guarantees | No | — |

**Recommendation: Kaggle.** Not because it is the most elegant, but because this app already drives
it — multi-account token store, GPU-denied rotation, notebook push, live log streaming
(`kaggle_monitor.rs`), and tunnels. A CPU-only render notebook is the same machinery with a
different payload, and CPU sessions do not touch the GPU quota that music/image generation needs.
The assets never touch the user's machine: project data and media already sync to a Hugging Face
repo with LFS, so the notebook pulls from there, encodes, uploads to YouTube with the channel's
refresh token, and writes the result back — the laptop only sends a few kilobytes of job JSON.

GitHub Actions is the better fit *if* project repos are on GitHub and public: nothing to keep alive,
`workflow_dispatch` per song, secrets already scoped per repo.

## 3. Paid track (optional upgrade)

| Option | Price | Why it is on the list |
| --- | --- | --- |
| **Hetzner CX22** | ~$4.59/mo — 2 vCPU, 4 GB, **20 TB traffic included** | Cheapest always-on box that swallows the whole egress bill; a plain systemd worker, no platform quirks |
| **Modal** | **$30/mo free credits**, then ~$0.047/core-hour (2× for non-preemptible, 1.5–1.75× outside the default region); egress not currently billed | This workload is ~50–100 core-hours/mo ≈ **$2.50–$5** — inside the free credits. Per-job containers, no idle cost, scales to 25 parallel renders when a backlog needs clearing |
| GitHub Actions Pro | $4/mo (3,000 min) | Only if repos stay private and you want zero infrastructure |
| Oracle pay-as-you-go | Card on file, A1 still free-tier-eligible | Restores 4 OCPU / 24 GB A1 and dodges free-tier capacity denial |

At this volume the paid track is genuinely cheap: **$0–5/month**. Modal is the better *shape*
(serverless, burst-friendly, no machine to babysit); Hetzner is the better *floor* (fixed price, no
metering surprises, 20 TB of traffic).

## 4. Recommended architecture

The same job contract serves every backend, so the choice becomes a setting rather than a rewrite:

```
app (laptop/phone)                remote worker (Kaggle | Actions | Modal | VPS)
──────────────────                ─────────────────────────────────────────────
create render job         ──▶     pull assets from the project's git/LFS remote
  { song_id, sections,            ffmpeg: stills + audio → 1080p H.264
    images[], audio, subs,        (optional) burn overlays/subtitles
    channel_id, youtube:{...} }   upload via YouTube resumable API
                                  push result + logs back to the repo
poll job status           ◀──     job JSON updated in the repo
```

Design points that make this work in practice:

- **Assets move remote-to-remote.** The worker pulls from the same Hugging Face/GitHub LFS remote the
  app already syncs to, so the local connection carries job descriptions, not gigabytes.
- **Credentials stay scoped.** The worker needs one channel's refresh token plus the OAuth client id
  and secret, injected as environment/secret material for that job only — the vault already stores
  them per channel.
- **Idempotent by song id.** A retried job must not create a second YouTube video; the upload step
  checks for an existing `youtube_video_id` on the song before inserting.
- **CPU-only.** Nothing here needs a GPU, which is what keeps the free tiers viable — and keeps
  Kaggle's GPU quota free for music and image generation.

## 5. Status — implemented in v0.69.0

| Piece | Where | State |
| --- | --- | --- |
| Job spec builder | `src-tauri/commands/remote_render.rs` → `build_render_spec` | Done. Resolves every asset to a URL the worker can fetch, or refuses with the reason why. |
| Worker (ffmpeg + resumable YouTube upload) | `scripts/remote/render_worker.py` | Done. Stdlib-only, idempotent per song, reports `BM_RESULT` + `result.json`. |
| Kaggle launcher | `launch_kaggle` | Done. Pushes a private CPU kernel per job; no GPU quota used. |
| GitHub Actions launcher | `launch_actions` + `scripts/remote/bm-render.yml` | Done. `write_render_workflow` installs the workflow into the project repo; the job travels as a dispatch input. |
| Modal | `scripts/remote/modal_app.py` + `launch_http` | Done. `modal deploy` once, paste the endpoint into Settings. |
| Your own worker (VPS) | `launch_http` | Done. Any HTTP endpoint that accepts the spec. |
| Provider choice in the UI | Settings → Remote rendering; the Video Composer's guided flow | Done, saved as `remote_render_provider`. |
| Job list / result ingest | `list_render_jobs`, `record_render_result` | Done — a finished upload writes `youtube_video_id` back onto the song. |

Still open:

- **Kaggle result pickup is manual.** The kernel prints `BM_RESULT {…}` and writes
  `/kaggle/working/result.json`, but the app does not yet poll `kaggle kernels output` to ingest it.
  A Kaggle render therefore reports success on *submission*; the song's publish state is written when
  `record_render_result` is called (the Actions and HTTP paths can call back on their own).
- **No batch submit.** One song per submission; a "render everything that's ready" sweep is the
  obvious next step now that the per-job path exists.
- **Subtitles** are accepted by the worker but never populated by the spec builder.

### Trying it without the app

The worker is deliberately runnable by hand — that is also how it was tested:

```bash
python3 scripts/remote/render_worker.py job.json                     # render, and upload if the spec says so
BM_KEEP_OUTPUT=1 python3 scripts/remote/render_worker.py job.json    # keep the .mp4 in the working directory
```

## Sources

- [Kaggle notebook session limits (12 h CPU, 5 concurrent CPU sessions)](https://www.kaggle.com/docs/notebooks)
- [Kaggle: maximum batch CPU session count](https://www.kaggle.com/discussions/product-feedback/483684)
- [GitHub Actions billing — free minutes and per-minute rates](https://docs.github.com/billing/managing-billing-for-github-actions/about-billing-for-github-actions)
- [GitHub Actions pricing changes, Dec 2025](https://github.blog/changelog/2025-12-16-coming-soon-simpler-pricing-and-a-better-experience-for-github-actions/)
- [Oracle Cloud Always Free resources](https://docs.oracle.com/en-us/iaas/Content/FreeTier/freetier_topic-Always_Free_Resources.htm)
- [Oracle free tier breakdown incl. 2026 A1 reduction](https://fullmetalbrackets.com/blog/oci-free-tier-breakdown)
- [Modal free tier — $30/month credits](https://aicreditmart.com/ai-credits-providers/modal-free-tier-how-to-get-30-month-in-compute-credits-2026/)
- [Modal pricing explained (2026 CPU rates, multipliers)](https://www.beam.cloud/blog/modal-pricing-explained)
- [Hetzner Cloud pricing 2026 — CX22, 20 TB traffic](https://bestusavps.com/reviews/hetzner/)
- [Hugging Face Spaces overview / plan requirements](https://huggingface.co/docs/hub/en/spaces-overview)
- [YouTube Data API quota system](https://developers.google.com/youtube/v3/getting-started)
- [YouTube API quota changes and upload costs, 2026](https://www.getphyllo.com/post/youtube-api-limits-how-to-calculate-api-usage-cost-and-fix-exceeded-api-quota)
