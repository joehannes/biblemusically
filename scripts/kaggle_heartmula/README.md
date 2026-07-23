# HeartMuLa — free Suno-competitor music engine

A third free music engine (alongside Suno and ACE-Step). [HeartMuLa](https://github.com/HeartMuLa/heartlib)
is a 2026 open-source music foundation model — **Apache-2.0 across all components** (gen model, codec,
tokenizer), so its output is commercially usable / YouTube-safe. Multilingual (EN/ZH/JA/KO/ES), runs on
consumer GPUs.

## Run it

1. Pushed to your Kaggle account as **`joehannes/biblemusically-heartmula-server`**. Open it, enable GPU, Run All.
2. The last cell prints a `https://xxxx.trycloudflare.com` URL.
3. App → **Settings → Music engine → HeartMuLa**, paste the URL, **Test connection**.

## How it maps to the app

| App setting | Meaning |
|---|---|
| `music_engine` | `heartmula` selects it |
| `heartmula_api_url` | tunnel URL or `http://localhost:8003` |
| `heartmula_api_key` | optional; only if you set `API_KEY` in the notebook |
| `acestep_duration` | shared target song length (seconds) |

The notebook wraps HeartMuLa's `HeartMuLaGenPipeline` in a FastAPI server that speaks the **same task
API as ACE-Step** (`POST /release_task` → `POST /query_result` → `GET /v1/audio`), so the backend
reuses the ACE-Step client (`generate_song_api` in `src-tauri/jobs.rs`). The app sends the song's
`styles` as tags and its `lyrics` (with `[Verse]`/`[Chorus]` markers) as lyrics.

## Honest first-run caveats

- **HeartMuLa ships no official server.** This notebook warm-loads `HeartMuLaGenPipeline` and wraps it.
  The exact import path / `from_pretrained` signature can vary by release — if the last cell errors on
  the import, adjust it per the [heartlib README](https://github.com/HeartMuLa/heartlib) (documented API:
  `HeartMuLaGenPipeline.from_pretrained(model_path, version="3B", ...)`, then `pipe({"lyrics":..., "tags":...}, save_path=..., max_audio_length_ms=...)`).
- **Speed:** a full song on a single T4 can take a few minutes; the app polls ~5 minutes then times out.
  Keep `acestep_duration` modest (e.g. 120–180s) until you know your GPU's throughput.
- Not yet validated end-to-end on a live GPU here — expect to tweak the import once on first run.
