#!/usr/bin/env bash
set -euo pipefail

echo "Build / prepare sidecar binaries for packaging and local dev"

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT_DIR"

echo "Ensure submodules are initialized"
git submodule update --init --recursive

if [ -d "browsh" ]; then
  echo "Preparing Browsh..."
  # Browsh build procedure varies. Try common workflows but don't fail hard.
  if [ -f "browsh/Cargo.toml" ] && command -v cargo >/dev/null 2>&1; then
    echo "Building browsh via cargo (release)..."
    (cd browsh && cargo build --release) || echo "cargo build failed — you may need to follow browsh build docs"
    if [ -f "browsh/target/release/browsh" ]; then
      mkdir -p binaries
      cp browsh/target/release/browsh binaries/browsh || true
      echo "Copied built browsh to binaries/browsh"
    fi
  elif [ -f "browsh/Makefile" ]; then
    echo "Running make in browsh..."
    (cd browsh && make) || echo "make failed — consult browsh docs"
    if [ -f "browsh/browsh" ]; then
      mkdir -p binaries
      cp browsh/browsh binaries/browsh || true
    fi
  else
    echo "No automatic build recipe found for browsh — please build it manually and copy the executable to ./binaries/browsh"
  fi
else
  echo "browsh submodule not found — run scripts/add_submodules.sh and then re-run this script"
fi

echo "Midjourney proxy notes:"
echo "The midjourney-proxy project is usually run via its run_app.sh or docker compose."
echo "This script does not auto-build it; packaging should include the midjourney-proxy release binary or use the provided scripts in midjourney-proxy/scripts."

echo "Setup complete. Verify that ./binaries/browsh exists if you expect browsh to autostart."
