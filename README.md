# Lightkid AI Studio

A Tauri 2 desktop app that turns a Bible chapter (or any topic) into fully-produced, multilingual AI music videos and publishes them across a pool of YouTube channels — lyrics → music → images → video → upload, end to end, running locally on your machine.

> Documentation map: this file is the quick start. For the deeper picture, see:
> - [ARCHITECTURE.md](ARCHITECTURE.md) — how the app is put together
> - [FEATURES.md](FEATURES.md) — what each screen actually does (real vs. stubbed)
> - [STATUS.md](STATUS.md) — dated log of where the project is and how it got here
> - [TODOS.md](TODOS.md) — known bugs, dead code, and gaps
> - [BACKLOG.md](BACKLOG.md) — ideas for what's next
> - [WISHLIST.md](WISHLIST.md) — built-but-unreachable features, and what an integration audit turned up

## What it does

1. **Source text** — pull a Bible chapter in one of 20+ languages/translations (`bible-api.com` + `bible.helloao.org`, all public-domain), or paste your own.
2. **Compose lyrics** — an AI Composer (OpenRouter/Qwen) turns that text into song lyrics + Midjourney-style image prompts, per language and per visual style, in one pass.
3. **Generate music** — real Suno generation via your own session cookie.
4. **Analyze & section** — lyrics are auto-split into timed sections with mood/effect suggestions.
5. **Generate images** — real Midjourney generation via a Playwright-driven visible browser (no official API, no proxy service).
6. **Compose video** — FFmpeg stitches section images (with mood-matched pan/zoom/fade effects) to the music track.
7. **Publish** — upload to any number of YouTube channels via real OAuth + the YouTube Data API's resumable upload protocol, with AI-generated titles/descriptions/tags.
8. **Version & back up** — every project can be saved to a local git repo (with dated version tags and branches) and optionally backed up to Google Drive.
9. **Run on autopilot (optional)** — a per-project Scheduler can progress through a Bible book automatically (daily or weekly): fetch the next chapter, generate that chapter's lyrics, and start music generation — then stop, so a human reviews before images/video/upload. The same generation can also be triggered manually any time from the Dashboard's "Daily Content" panel.

Everything runs as a single Tauri process with a bundled MongoDB (`mongod`) sidecar — no server to deploy, no account to sign up for beyond the services you choose to connect (Suno, Midjourney, Google, OpenRouter). A configurable job queue caps how many Suno/Midjourney/FFmpeg/upload jobs run at once, so nothing overwhelms your session cookies or account rate limits.

## Tech stack

- **Desktop shell**: Tauri 2 (Rust)
- **Frontend**: React 19, React Router 7, Vite 7, Tailwind + shadcn/ui
- **Backend**: Rust (`src-tauri/`), async via Tokio, HTTP via `reqwest`
- **Storage**: MongoDB, bundled as a local sidecar (not a hosted service)
- **AI**: OpenRouter (free-tier models: Qwen, Gemma, Llama, Hermes, GPT-OSS, Dolphin Mistral)

## Getting started

```bash
npm install          # root deps (Vite, Tauri CLI, shadcn/radix, etc.)
npm run tauri dev    # launches the Tauri app with hot-reloading frontend
```

The dev server is driven by the root [vite.config.ts](vite.config.ts) (`root: "src"`) — you do not need to `cd src` or run anything inside `src/` separately; the `src/package.json`/`craco.config.js` files there are legacy and unused (see [TODOS.md](TODOS.md)).

On first launch you'll want to visit **Settings** to connect the services you plan to use:

| Service | What you need | Where |
|---|---|---|
| Suno (music) | A session cookie from suno.com | Settings → Suno → "Capture session" or paste manually |
| Midjourney (images) | A logged-in browser profile | Settings → Midjourney → "Capture session" |
| FFmpeg | Installed on your system (`ffmpeg`/`ffprobe` on `PATH`) | The app will warn on launch if missing |
| OpenRouter (AI composer) | A free API key from [openrouter.ai/keys](https://openrouter.ai/keys) | Settings → AI Composer |
| Google/YouTube | An OAuth client (Desktop App type) from Google Cloud Console | Settings → Google OAuth, or per-channel in Channel Manager |
| Google Drive (optional project backup) | Same Google OAuth client, `drive.file` scope | Settings → Google Drive Integration, per project |

## Building

```bash
npm run tauri -- build   # produces a .deb (Linux is currently the only configured bundle target)
```

## Project layout

```
src/                 React frontend (Vite root)
src-tauri/            Rust backend + Tauri config
  commands/           One module per feature domain (songs, channels, oauth, bible, ai, ...)
  packaging/          Node/Playwright automation scripts, bundled as Tauri resources
scripts/kaggle_riffusion/   Standalone (not yet integrated) free long-form music generation service
```

See [ARCHITECTURE.md](ARCHITECTURE.md) for the full breakdown.
