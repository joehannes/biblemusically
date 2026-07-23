# ComfyUI — free multi-model image & animation server

The node-based engine that loads many free models and connects them together. This is the fitting
tool for the advanced image/video features (character consistency, style models, animation) — more
capable than the single-model FLUX server in [`scripts/kaggle_flux/`](../kaggle_flux/), which stays
as a lightweight "just make an image" option.

## Feature → mechanism

| You want | ComfyUI mechanism (installed by the notebook) |
|---|---|
| High-quality photoreal | SDXL base; optional FLUX.1 schnell (Apache-2.0) |
| Comic / graphic-novel / style filters | prompt + style LoRAs you drop in `models/loras` (vet licenses) |
| Multiple art models | any checkpoint/LoRA in `models/`; add more via ComfyUI-Manager |
| Character consistency from example/avatar images | IP-Adapter (`ComfyUI_IPAdapter_plus`); InstantID via Manager for faces |
| Nuanced variations keeping the character | IP-Adapter reference + varied prompt / seed / weight |
| Slight animations, music-video motion | AnimateDiff (`ComfyUI-AnimateDiff-Evolved`) + VideoHelperSuite |
| Pose / composition control | `comfyui_controlnet_aux` |

## Run it

1. It's already pushed to your Kaggle account as **`joehannes/biblemusically-comfyui-server`**. Open it, enable GPU, Run All.
2. The last cell prints a `https://xxxx.trycloudflare.com` URL. The **same URL** serves both the ComfyUI web UI (for manual work / adding models via Manager) and the API the app calls.
3. Paste it into the app once the ComfyUI engine is wired up (app-side integration is the next build).

## How the app drives it (Phase 1 — built)

Select **Settings → Image engine → ComfyUI** and paste the server URL. The backend (`real_comfy` in
[`src-tauri/jobs.rs`](../../src-tauri/jobs.rs)) posts a **workflow template** to `/prompt`, polls
`/history/{id}`, and returns `/view` image URLs — same downstream flow as the other engines.

Bundled workflow templates live in [`src-tauri/comfy_workflows/`](../../src-tauri/comfy_workflows/):
- `photoreal_sdxl.json` — SDXL text-to-image, used for section images and any non-character image.
- `character_ipadapter_sdxl.json` — SDXL + IP-Adapter; used automatically for **character** images,
  feeding the character's avatar/example image as the reference so re-generations stay on-model.

Remote-controllable from Settings today: server URL/key, **style preset** (photoreal / comic /
graphic-novel / anime / oil / watercolor), SDXL checkpoint, steps, CFG, size, extra negative prompt,
and character-reference strength (IP-Adapter weight).

**Still to come (later phases):** preset packs, per-YouTube-channel sticky styles persisted
cross-session, and the AnimateDiff animation/overlay workflows (the nodes are already installed on
the server). The `character_ipadapter_sdxl.json` graph uses `IPAdapterUnifiedLoader` /
`IPAdapterAdvanced` node names — if the installed node pack differs, adjust that template or wire it
in the ComfyUI UI once, then export the API-format JSON over the bundled file.

## Licensing (monetization-safe defaults)

- **Safe:** SDXL base (Stability Community License, free under \$1M rev), FLUX.1 *schnell* (Apache-2.0), IP-Adapter & AnimateDiff motion modules (Apache-2.0).
- **Vet yourself:** any Civitai LoRA/checkpoint for comic/art styles — many are non-commercial. Do not put unvetted style models into monetized videos.
- **Avoid for monetization:** FLUX.1 *dev* (non-commercial), Stable Video Diffusion (research license).

## Caveats

- First run may 404 on a model file if a repo path drifts — the server still starts; use ComfyUI-Manager to fetch the correct file.
- Disk: the full stack is large. `DOWNLOAD_FLUX = False` by default so it fits a single T4; turn it on if you have room.
- Free tunnel URLs and Kaggle sessions are temporary — re-run and re-paste when the URL expires.
