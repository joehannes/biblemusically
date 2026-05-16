# Lightkid AI Studio — PRD

## Problem
Automate AI music-video creation + multi-channel YouTube publishing from a JSON lyrics file
(topic → multi-language → multi-style → multi-channel pipeline).

## Architecture
- React + shadcn/ui + Tailwind frontend (10 screens + Settings)
- FastAPI + Motor backend, MongoDB persistence
- In-process background worker simulating Suno / Midjourney / FFmpeg / YouTube uploads
- Tauri wrapper config at /app/tauri/ for future desktop packaging

## Implemented
- Screens: Dashboard, Lyrics, MusicGen, Analysis, SectionEditor, Images, Composer, Channels, Upload, Jobs, Settings
- Three themes (Obsidian, Aurora, Vellum) with live switcher
- Lyrics JSON import → parsed annotations → sections
- Audio analysis mock generates per-section timing + mood + image_prompt + suggested effects
- Image gen mock returns 4 variants per section (rotatable)
- 16 FFmpeg effect presets keyed to mood keywords
- YouTube OAuth start URL builder using user-provided client creds; manual refresh-token capture
- Job queue with logs, retry, cancel, copy

## Mocked
- Suno: returns mock audio URL after 2.5s
- Midjourney: returns picsum images as variants
- FFmpeg compose: returns mock video URL
- YouTube upload: returns mock yt id
- OAuth: builds correct auth URL, refresh token paste-in flow

## Backlog (P0/P1/P2)
- P0: Real Suno cookie-based call (studio-api.suno.com /api/generate)
- P0: Real Midjourney via Discord wrapper
- P0: Real FFmpeg compose pipeline
- P1: OAuth callback server-side exchange of code → tokens (currently manual paste)
- P1: Resumable YouTube uploads
- P2: Qwen prompt → effect recommendation feedback loop
- P2: Tauri build verification
