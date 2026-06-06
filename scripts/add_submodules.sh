#!/usr/bin/env bash
set -euo pipefail

echo "This script will remove any local midjourney-proxy folder and add the requested git submodules."
echo "It modifies your repository state; review before running."

if [ ! -d .git ]; then
  echo "Not a git repository (no .git found). Run this from the repo root." >&2
  exit 1
fi

echo "Submodule management for midjourney-proxy and browsh has been removed."
echo "The project now uses a Playwright-driven visible browser flow for Midjourney operations."
echo "If you still need external projects, add them manually to your repository."
