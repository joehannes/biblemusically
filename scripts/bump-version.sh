#!/bin/bash
# ── Version Bump Script ──
# Usage: ./scripts/bump-version.sh [major|minor|patch]
# Example: ./scripts/bump-version.sh patch  (0.11.0 → 0.11.1)
#          ./scripts/bump-version.sh minor  (0.11.0 → 0.12.0)
#          ./scripts/bump-version.sh major  (0.11.0 → 1.0.0)

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

# Read current version from root package.json
CURRENT="$(jq -r '.version' "$ROOT_DIR/package.json")"
echo "Current version: $CURRENT"

IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT"

case "${1:-minor}" in
  major)
    MAJOR=$((MAJOR + 1))
    MINOR=0
    PATCH=0
    ;;
  minor)
    MINOR=$((MINOR + 1))
    PATCH=0
    ;;
  patch)
    PATCH=$((PATCH + 1))
    ;;
  *)
    echo "Usage: $0 [major|minor|patch]"
    exit 1
    ;;
esac

NEW_VERSION="${MAJOR}.${MINOR}.${PATCH}"
echo "New version: $NEW_VERSION"

# Update root package.json
jq --arg v "$NEW_VERSION" '.version = $v' "$ROOT_DIR/package.json" > "$ROOT_DIR/package.json.tmp" && mv "$ROOT_DIR/package.json.tmp" "$ROOT_DIR/package.json"

# Update src/package.json
jq --arg v "$NEW_VERSION" '.version = $v' "$ROOT_DIR/src/package.json" > "$ROOT_DIR/src/package.json.tmp" && mv "$ROOT_DIR/src/package.json.tmp" "$ROOT_DIR/src/package.json"

# Update src-tauri/tauri.conf.json
jq --arg v "$NEW_VERSION" '.version = $v' "$ROOT_DIR/src-tauri/tauri.conf.json" > "$ROOT_DIR/src-tauri/tauri.conf.json.tmp" && mv "$ROOT_DIR/src-tauri/tauri.conf.json.tmp" "$ROOT_DIR/src-tauri/tauri.conf.json"

# Update src-tauri/Cargo.toml — ONLY the version inside [package].
#
# This used to be a bare `s/^version = ...` with a comment claiming [package]'s was the only such
# line at column 0. It was not: a `[dependencies.<name>]` table puts its own `version = "…"` at
# column 0 too, and on 2026-08-04 this rewrote git2's `version = "0.19"` to the app's version —
# a dependency spec that resolves to nothing, so the next build fails with an error naming git2
# rather than anything to do with a version bump. Restrict the substitution to the [package]
# table by only editing between that header and the next one.
sed -i.tmp "/^\s*\[package\]/,/^\s*\[[^]]*\]\s*$/{ s/^version = \".*\"/version = \"${NEW_VERSION}\"/; }" \
  "$ROOT_DIR/src-tauri/Cargo.toml" && rm -f "$ROOT_DIR/src-tauri/Cargo.toml.tmp"

# Refresh Cargo.lock's own package entry to match, if cargo is available.
if command -v cargo >/dev/null 2>&1; then
  (cd "$ROOT_DIR/src-tauri" && cargo check -q --offline 2>/dev/null || cargo check -q 2>/dev/null) || \
    echo "⚠️  Could not run 'cargo check' to refresh Cargo.lock — run it manually before committing."
else
  echo "⚠️  cargo not found on PATH — Cargo.lock will be stale until you run 'cargo check' or 'cargo build' in src-tauri/."
fi

echo "✅ All version files updated to $NEW_VERSION (package.json, src/package.json, tauri.conf.json, Cargo.toml)"