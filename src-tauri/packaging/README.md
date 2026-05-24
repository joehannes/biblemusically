Midjourney Proxy packaging and autostart helpers

This folder contains templates and installer helpers to integrate the bundled `midjourney-proxy` into platform autostart on first-run or during installation.

Files:
- `install_midjourney_service.sh` - POSIX shell installer for systemd (Linux) and launchd (macOS).
- `install_midjourney_service.ps1` - PowerShell script to register a scheduled task on Windows.

Usage (example):

- For Linux packaged app where resources are installed to `/opt/ai-music-video-studio/resources`:

```sh
sudo /opt/ai-music-video-studio/packaging/install_midjourney_service.sh /opt/ai-music-video-studio/resources
```

- On macOS:

```sh
/Applications/AI\ Music\ Video\ Studio.app/Contents/Resources/packaging/install_midjourney_service.sh /Applications/AI\ Music\ Video\ Studio.app/Contents/Resources
```

- On Windows (run PowerShell as Administrator):

```powershell
.\install_midjourney_service.ps1 -ResourceDir "C:\Program Files\AI Music Video Studio\resources"
```

Notes:
- The installer scripts expect the `run_app.sh` script to be present in the `midjourney-proxy` folder inside the resource directory.
- These are best-effort helpers; final installers (deb, dmg, msi) should call the appropriate script at install time and replace paths as needed.
