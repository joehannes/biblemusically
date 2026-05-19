This folder is intended to contain platform-specific FFmpeg binaries and optional plugins used by the Tauri bundle.

How to use
- Place your platform ffmpeg binaries here before building the Tauri installer. Recommended layout:
  - `binaries/ffmpeg/ffmpeg` (Linux executable)
  - `binaries/ffmpeg/ffprobe` (Linux executable)
  - `binaries/ffmpeg/ffmpeg.exe` (Windows)
  - `binaries/ffmpeg/ffprobe.exe` (Windows)
  - `binaries/ffmpeg/plugins/` (optional plugin files)

Suggested sources
- Linux static builds: https://johnvansickle.com/ffmpeg/
- Windows static builds: https://www.gyan.dev/ffmpeg/builds/
- macOS builds: build with Homebrew or include a macOS binary

Automation
We provide helper download scripts in this folder to fetch a common Linux static build. Review them before running.

Security
- Only include binaries you trust. Shipping third-party binaries may have licensing implications.

If you want me to download and place builds automatically, I can add automated fetch steps to the build pipeline. Request which platforms to include.
