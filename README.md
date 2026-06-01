# Here are your Instructions

## Submodules & Sidecars

This repository now expects `midjourney-proxy` and `browsh` to be added as git submodules and provides helper scripts to prepare sidecar binaries.

- **Add submodules:** Run `scripts/add_submodules.sh` from the repository root. This removes a local `midjourney-proxy` folder (if present) and adds the two submodules.
- **Prepare sidecars:** Run `scripts/setup_sidecars.sh` to build or copy the `browsh` binary to `./binaries/browsh` (if applicable) and to get guidance for `midjourney-proxy` packaging.
- **Tauri integration:** The Tauri backend is updated to attempt to autostart a `browsh` sidecar (if present) and will continue to auto-detect and autostart `midjourney-proxy` if available next to the executable or in the packaged resources.

Notes:
- You must run the `scripts/add_submodules.sh` script in a git repository (it will call `git submodule add`).
- Building browsh may require platform-specific toolchains; follow the browsh project build docs if automatic steps fail.
- Token/cookie management for Suno and Midjourney is handled by the backend; you still need to provide credentials/cookies via Settings or environment variables as described in the app settings UI.

