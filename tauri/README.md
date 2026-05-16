# Tauri Desktop Wrapper

This directory contains the configuration needed to wrap the existing React frontend as a Tauri desktop application — runnable on your machine (the Emergent web environment cannot compile Tauri).

## Prerequisites (on your local machine)
- Rust (https://rustup.rs)
- Node + yarn
- Platform deps: https://tauri.app/start/prerequisites/

## Quick start
```bash
cd /app/frontend
yarn add -D @tauri-apps/cli @tauri-apps/api
cp -r /app/tauri/src-tauri ./src-tauri
yarn tauri dev          # development desktop window
yarn tauri build        # produce installer (.dmg / .msi / .AppImage)
```

The Tauri shell loads the same React frontend (`/app/frontend`) and points API calls at your backend (set REACT_APP_BACKEND_URL in `.env.production` before `yarn build`).

`src-tauri/tauri.conf.json` is set up for window size 1400x900, dark titlebar, and the bundle identifier `com.lightkid.studio`.
