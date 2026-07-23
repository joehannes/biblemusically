# FLUX.1 [schnell] — free Midjourney alternative for BibleMusically

A **free, open-source (Apache-2.0) image generator** you can use instead of Midjourney. It runs
[FLUX.1 schnell](https://huggingface.co/black-forest-labs/FLUX.1-schnell) on a **free Kaggle/Colab
GPU** and exposes a small REST API to the desktop app through a public tunnel. FLUX.1 schnell's
Apache-2.0 license makes its output commercially usable (YouTube monetization included) — unlike
FLUX.1 *dev*, which is non-commercial.

The app calls the same image job path it uses for Midjourney (character images and per-section
images), so once the server URL is configured you switch engines with a single dropdown.

## Quick start (Kaggle, free)

1. **One-time: get Hugging Face access.** FLUX.1 schnell is gated (even though it's Apache-2.0) —
   visit <https://huggingface.co/black-forest-labs/FLUX.1-schnell> logged into HF and click
   **"Agree and access repository"**, then create a read-scope token at
   <https://huggingface.co/settings/tokens>. Without this the notebook's load cell fails with a
   `401 GatedRepoError`.
2. Create a new Kaggle notebook and upload [`notebook/setup_and_serve.ipynb`](notebook/setup_and_serve.ipynb).
3. Turn on the GPU: **Settings → Accelerator → GPU T4 x2**.
4. Add your HF token as a Kaggle secret: **Add-ons → Secrets → Add a new secret**, name it
   `HF_TOKEN`, paste the token, make sure it's attached to this notebook.
5. Run all cells. The last cell prints a public URL like `https://xxxx.trycloudflare.com`.
6. In the desktop app: **Settings → FLUX server URL** → paste that URL → click **Test connection**.
7. Set **Settings → Image engine → FLUX.1 [schnell]**. Generate character/section images as usual.

Colab works the same way (**Runtime → Change runtime type → GPU**) — set `HF_TOKEN` as an
environment variable instead of a Kaggle secret (the notebook falls back to `os.environ['HF_TOKEN']`
when Kaggle secrets aren't available).

## Local GPU alternative (permanent, no tunnel)

With a local GPU (≥16 GB VRAM recommended, or use `enable_model_cpu_offload`), run the same server
code from the notebook on `http://localhost:8002` and point the app there. No rotating URLs.

## How it maps to the app

| App setting | Meaning |
|---|---|
| `image_engine` | `midjourney` or `flux` — chosen in Settings → Image engine |
| `flux_api_url` | Base URL of the FLUX server (tunnel URL or `http://localhost:8002`) |
| `flux_api_key` | Optional; only if you set `API_KEY` in the notebook. Sent as `Authorization: Bearer …` |

Backend integration lives in `real_flux()` in [`src-tauri/jobs.rs`](../../src-tauri/jobs.rs): it POSTs
the section/character prompt to `/generate` and returns the resulting image URLs, which the video
step downloads exactly like Midjourney images.

## REST API (defined by the notebook)

- `POST /generate` with `{ prompt, num_images, steps, width, height }` → `{ "images": ["/images/…png", …] }`
- `GET /images/<id>.png` → the PNG bytes
- `GET /health` → `{ "status": "ok" }` (used by the app's **Test connection** button)
- Auth (optional): `Authorization: Bearer <key>`, enabled by setting `API_KEY` in the notebook.

## Caveats

- **Free tunnel URLs and Kaggle sessions are temporary.** The URL changes each run; re-run and
  re-paste. Because the video-composition step downloads images later, keep the notebook running
  until videos are composed, or the app has downloaded the images.
- **VRAM:** FLUX.1 schnell is large. On a single 16 GB T4 keep `enable_model_cpu_offload()` on
  (slower but fits). With ≥24 GB you can remove it for full speed.
- **Licensing:** FLUX.1 *schnell* is Apache-2.0 and commercially usable. Do **not** swap in FLUX.1
  *dev* for monetized content — its license is non-commercial.
