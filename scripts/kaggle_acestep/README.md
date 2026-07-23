# ACE-Step 1.5 — free Suno alternative for BibleMusically

A **free, open-source (MIT-licensed) song generator** you can use instead of Suno. It runs the
[ACE-Step 1.5](https://github.com/ace-step/ACE-Step-1.5) model on a **free Kaggle/Colab GPU** and
exposes its REST API to the desktop app through a public tunnel. Generated songs are commercially
usable (YouTube monetization included), support vocals + lyrics in 50+ languages, and can be up to
**10 minutes long in a single generation** — no stitching needed for your 5–10 minute target.

The app talks to ACE-Step through the same submit → poll → download flow it uses for Suno, so once
the server URL is configured you switch engines with a single dropdown.

## Quick start (Kaggle, free)

1. Create a new Kaggle notebook and upload [`notebook/setup_and_serve.ipynb`](notebook/setup_and_serve.ipynb).
2. Turn on the GPU: **Settings → Accelerator → GPU T4 x2**.
3. Run all cells. The last cell prints a public URL like `https://xxxx.trycloudflare.com`.
4. In the desktop app: **Settings → ACE-Step server URL** → paste that URL → click **Test connection**.
5. Set **Settings → Music engine → ACE-Step 1.5**. Generate songs as usual — jobs now route to ACE-Step.

Colab works the same way (**Runtime → Change runtime type → GPU**).

## Local GPU alternative (permanent, no tunnel)

If you have a local NVIDIA/Apple GPU (~4 GB VRAM minimum):

```bash
git clone https://github.com/ace-step/ACE-Step-1.5
cd ACE-Step-1.5
uv sync            # or: pip install -e .
uv run acestep-api # serves the REST API on http://localhost:8001
```

Then set **ACE-Step server URL** to `http://localhost:8001`. No rotating URLs, always available.

## How it maps to the app

| App setting | Meaning |
|---|---|
| `music_engine` | `suno` or `acestep` — chosen in Settings → Music engine |
| `acestep_api_url` | Base URL of the ACE-Step REST server (tunnel URL or `http://localhost:8001`) |
| `acestep_api_key` | Optional; only if you started the server with `ACESTEP_API_KEY`. Sent as `Authorization: Bearer …` |
| `acestep_duration` | Target song length in seconds (10–600). A song's own `duration` overrides this if set |

The backend integration lives in `real_acestep()` in [`src-tauri/jobs.rs`](../../src-tauri/jobs.rs):
it POSTs to `/release_task`, polls `/query_result` until the track is ready, and resolves the
returned `/v1/audio?path=…` file into an absolute download URL.

## REST API reference (for debugging)

- `POST /release_task` → `{ data: { task_id } }`. Body: `prompt`, `lyrics`, `audio_duration` (10–600),
  `audio_format`, `inference_steps` (8 = turbo), `batch_size`, `use_random_seed`.
- `POST /query_result` with `{ "task_id_list": ["…"] }` → per-task `result` (JSON string) with
  `status` (0 running / 1 done / 2 failed) and `file` (a `/v1/audio?path=…` path).
- `GET /v1/audio?path=…` → the audio file bytes.
- Auth (optional): `Authorization: Bearer <key>`, enabled server-side via `ACESTEP_API_KEY`.

Full spec: <https://github.com/ace-step/ACE-Step-1.5/blob/main/docs/en/API.md>

## Caveats

- **Free tunnel URLs and Kaggle sessions are temporary.** The `trycloudflare.com` URL changes each
  run, and Kaggle sessions stop after prolonged idle. Re-run the notebook and re-paste the URL — the
  app's **Test connection** button tells you if the current URL is still live.
- **Quality:** ACE-Step gets close to Suno for background/thematic music but reviewers still rate
  Suno higher on production polish — expect to cherry-pick a bit more.
- **Licensing:** ACE-Step is MIT and its output is commercially usable; your real constraint is
  YouTube's inauthentic-content policy (add curation/visuals) and AI-content disclosure.
