#!/usr/bin/env bash
set -euo pipefail

echo "This script will remove any local midjourney-proxy folder and add the requested git submodules."
echo "It modifies your repository state; review before running."

if [ ! -d .git ]; then
  echo "Not a git repository (no .git found). Run this from the repo root." >&2
  exit 1
fi

echo "Removing local midjourney-proxy folder (if exists) to prepare for submodule add..."
rm -rf midjourney-proxy

echo "Adding midjourney-proxy submodule..."
git submodule add https://github.com/trueai-org/midjourney-proxy.git midjourney-proxy || true

echo "Adding browsh submodule..."
git submodule add https://github.com/browsh-org/browsh.git browsh || true

echo "Initializing and updating submodules..."
git submodule update --init --recursive

echo "Done. You may want to run scripts/setup_sidecars.sh next to build or install binaries."
