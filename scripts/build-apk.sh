#!/usr/bin/env bash
# ── A sideloadable APK, named and placed so it can just be sent to somebody ────
#
#   npm run build:apk           # arm64 — every Android phone since about 2017
#   npm run build:apk -- --all  # all four ABIs, roughly four times the size
#
# Not a Play Store build. Nothing here talks to Google: the output is a signed file that installs
# after the one "allow from this source" toggle Android asks for, which is what makes it possible to
# hand the app to somebody over a download link. docs/INSTALL.md explains that toggle to them.
#
# Three things this does that `npx tauri android build` on its own does not:
#
#   * **The icon.** `tauri android init` scaffolds `gen/android` from a template carrying Tauri's own
#     cyan-and-yellow logo, and never looks at `src-tauri/icons/android/` again. The real icons are
#     copied in below on every build, so a regenerated project cannot silently ship the default one —
#     which is exactly what the first installable APK did.
#   * **The name.** Gradle calls everything `app-universal-release.apk`, so two versions in a
#     downloads folder are indistinguishable and the second overwrites the first. The output is named
#     for the app and the version instead.
#   * **The place.** `src-tauri/gen/android/app/build/outputs/apk/universal/release/` is not a path
#     anybody wants to be told to look in.

set -euo pipefail
cd "$(dirname "$0")/.."

ALL_ABIS=0
for arg in "$@"; do
  case "$arg" in
    --all) ALL_ABIS=1 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

VERSION="$(node -p "require('./package.json').version")"
OUT_DIR="release"
OUT="$OUT_DIR/lightkid_studio_${VERSION}.apk"

# shellcheck source=/dev/null
source scripts/android-env.sh

# The real launcher icons, over the template's. Idempotent, and deliberately every build rather than
# once by hand: `tauri android init` regenerates this directory and would quietly undo it.
#
# This brings the adaptive icon too (mipmap-anydpi-v26/ic_launcher.xml plus the background colour it
# references), which the template omits entirely — without it Android 8 and later ignore the raster
# icons and fall back to the template's own, which is how a correct icon can still look wrong.
echo "Icons: syncing src-tauri/icons/android → gen/android"
cp -r src-tauri/icons/android/. src-tauri/gen/android/app/src/main/res/

if [ "$ALL_ABIS" = "1" ]; then
  echo "Building $VERSION for all ABIs"
  npx tauri android build --apk
else
  echo "Building $VERSION for arm64-v8a"
  npx tauri android build --apk --target aarch64
fi

# Newest APK gradle produced, rather than a hardcoded path: the filename depends on whether the build
# was per-ABI or universal, and guessing wrong fails after the ten minutes that matter.
BUILT="$(find src-tauri/gen/android/app/build/outputs/apk -name '*.apk' -newermt '-30 minutes' \
         -print0 2>/dev/null | xargs -0 ls -t 2>/dev/null | head -1 || true)"
if [ -z "$BUILT" ]; then
  echo "No APK was produced — the build above did not fail, but nothing landed in outputs/apk." >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
cp "$BUILT" "$OUT"

# An unsigned APK cannot be installed on Android at all — "unsigned" is not something the user can
# allow, unlike an unknown source. Better to say so here than to let somebody discover it on a phone.
if command -v apksigner >/dev/null 2>&1; then
  if apksigner verify "$OUT" >/dev/null 2>&1; then
    SIGNER="$(apksigner verify --print-certs "$OUT" 2>/dev/null | grep -m1 'certificate DN' || true)"
    echo "Signed: ${SIGNER:-yes}"
  else
    echo
    echo "WARNING: this APK is NOT signed and Android will refuse to install it."
    echo "         Expected a keystore at src-tauri/gen/android/app/keystore.properties."
    echo
  fi
fi

echo
echo "  $OUT  ($(du -h "$OUT" | cut -f1))"
echo
echo "Send that file. On the phone: open it, then Settings → Allow from this source."
