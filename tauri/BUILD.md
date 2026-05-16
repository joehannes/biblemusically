# Build Lightkid AI Studio as a single desktop installer

Tauri produces real installers (.msi / .dmg / .deb / .AppImage) bundling:
- the React frontend (pre-built static assets)
- the FastAPI Python backend (as a PyInstaller sidecar)
- ffmpeg + ffprobe (as sidecar binaries — no system install needed)

This must run on YOUR machine (the Emergent cloud container has no Rust toolchain).

## 1. Prerequisites (your machine)
- Rust toolchain: https://rustup.rs
- Node 18+ and yarn
- Python 3.11 + PyInstaller (`pip install pyinstaller`)
- Platform deps: https://tauri.app/start/prerequisites/
- Download static ffmpeg + ffprobe binaries from https://www.gyan.dev/ffmpeg/builds/ (Win), https://evermeet.cx/ffmpeg/ (Mac), or your distro repos (Linux)

## 2. One-time setup
```bash
git clone <this repo> lightkid && cd lightkid
# wire Tauri into the existing React app
cd frontend && yarn add -D @tauri-apps/cli @tauri-apps/api && cd ..
cp -r tauri/src-tauri frontend/
mkdir -p frontend/src-tauri/binaries
```

## 3. Build the Python backend as a sidecar
```bash
cd backend
pip install pyinstaller
pyinstaller --onefile --name python-backend --add-data ".:." server.py
# rename per target triple expected by Tauri:
# macOS arm64:  python-backend-aarch64-apple-darwin
# macOS x64:    python-backend-x86_64-apple-darwin
# Win64:        python-backend-x86_64-pc-windows-msvc.exe
# Linux x64:    python-backend-x86_64-unknown-linux-gnu
cp dist/python-backend ../frontend/src-tauri/binaries/python-backend-$(rustc -vV | sed -n 's|host: ||p')
```

## 4. Drop in ffmpeg + ffprobe sidecars
```bash
# rename same way; e.g. on macOS arm:
cp /opt/homebrew/bin/ffmpeg   frontend/src-tauri/binaries/ffmpeg-aarch64-apple-darwin
cp /opt/homebrew/bin/ffprobe  frontend/src-tauri/binaries/ffprobe-aarch64-apple-darwin
chmod +x frontend/src-tauri/binaries/*
```

## 5. Run / Build
```bash
cd frontend
# point production to localhost (sidecar serves the backend)
echo "REACT_APP_BACKEND_URL=http://127.0.0.1:8001" > .env.production
# dev with hot reload
yarn tauri dev
# production installer
yarn tauri build
# artifact:
#   macOS:  src-tauri/target/release/bundle/dmg/*.dmg
#   Win:    src-tauri/target/release/bundle/msi/*.msi
#   Linux:  src-tauri/target/release/bundle/appimage/*.AppImage
```

## 6. Notes about MongoDB inside the installer
The simplest portable option is to bundle MongoDB-Community-Server-Standalone
(or replace `motor` with `tinydb`/`sqlite` for an embedded store). For a true
zero-install experience, run a local single-node MongoDB inside a sidecar binary
or switch the persistence layer.

## 7. Auto-update
Set `tauri.updater` in `tauri.conf.json` (https://tauri.app/v1/guides/distribution/updater/) once your release endpoint is ready.
