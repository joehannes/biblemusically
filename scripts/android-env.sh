#!/usr/bin/env bash
# ── Android build environment ─────────────────────────────────────────────────
# Source this before any Android command:  source scripts/android-env.sh
#
# Everything here is a fact about *this machine*, discovered on 2026-07-25:
#
#  * NDK 29.0.13846066 is already installed under $ANDROID_HOME/ndk — nothing to download.
#  * The Android Gradle Plugin this project pins (8.11.0, Gradle 8.14.3) needs JDK 17.
#    The system default is JDK 26 (Homebrew), which AGP rejects, so JAVA_HOME is forced.
#  * `rustc` on PATH is Homebrew's, and it cannot see rustup's Android std libraries
#    ("can't find crate for `core`"). Cargo resolves rustc from PATH, so RUSTC must be
#    pointed at rustup's copy explicitly — even when calling rustup's cargo directly.
#  * The NDK ships only API-versioned clang wrappers (aarch64-linux-android24-clang).
#    cc-rs looks for an unversioned `aarch64-linux-android-clang`, which does not exist,
#    so the CC_*/AR_* variables below are what let C-compiling crates (ring, under
#    rustls/reqwest) build at all.

set -a

ANDROID_HOME="${ANDROID_HOME:-$HOME/Android}"
ANDROID_SDK_ROOT="$ANDROID_HOME"

# Newest installed NDK, so this keeps working after an SDK-manager upgrade.
NDK_HOME="$(ls -d "$ANDROID_HOME"/ndk/*/ 2>/dev/null | sort -V | tail -1)"
NDK_HOME="${NDK_HOME%/}"
ANDROID_NDK_HOME="$NDK_HOME"
ANDROID_NDK_ROOT="$NDK_HOME"

JAVA_HOME="${ANDROID_JAVA_HOME:-/usr/lib/jvm/java-17-openjdk-amd64}"

# rustup's toolchain, not Homebrew's.
_RUSTUP_TC="$HOME/.rustup/toolchains/stable-x86_64-unknown-linux-gnu"
if [ -x "$_RUSTUP_TC/bin/rustc" ]; then
  RUSTC="$_RUSTUP_TC/bin/rustc"
fi

# Per-target C toolchain. API 24 matches the minSdk in gen/android.
_NDK_BIN="$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"
_API=24
CC_aarch64_linux_android="$_NDK_BIN/aarch64-linux-android$_API-clang"
CXX_aarch64_linux_android="$_NDK_BIN/aarch64-linux-android$_API-clang++"
AR_aarch64_linux_android="$_NDK_BIN/llvm-ar"
RANLIB_aarch64_linux_android="$_NDK_BIN/llvm-ranlib"
CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"

CC_armv7_linux_androideabi="$_NDK_BIN/armv7a-linux-androideabi$_API-clang"
CXX_armv7_linux_androideabi="$_NDK_BIN/armv7a-linux-androideabi$_API-clang++"
AR_armv7_linux_androideabi="$_NDK_BIN/llvm-ar"
RANLIB_armv7_linux_androideabi="$_NDK_BIN/llvm-ranlib"
CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER="$CC_armv7_linux_androideabi"

CC_i686_linux_android="$_NDK_BIN/i686-linux-android$_API-clang"
CXX_i686_linux_android="$_NDK_BIN/i686-linux-android$_API-clang++"
AR_i686_linux_android="$_NDK_BIN/llvm-ar"
RANLIB_i686_linux_android="$_NDK_BIN/llvm-ranlib"
CARGO_TARGET_I686_LINUX_ANDROID_LINKER="$CC_i686_linux_android"

CC_x86_64_linux_android="$_NDK_BIN/x86_64-linux-android$_API-clang"
CXX_x86_64_linux_android="$_NDK_BIN/x86_64-linux-android$_API-clang++"
AR_x86_64_linux_android="$_NDK_BIN/llvm-ar"
RANLIB_x86_64_linux_android="$_NDK_BIN/llvm-ranlib"
CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="$CC_x86_64_linux_android"

set +a

PATH="$JAVA_HOME/bin:$_NDK_BIN:$ANDROID_HOME/platform-tools:$PATH"
export PATH

echo "Android env ready:"
echo "  NDK_HOME  = $NDK_HOME"
echo "  JAVA_HOME = $JAVA_HOME  ($("$JAVA_HOME/bin/java" -version 2>&1 | head -1))"
echo "  RUSTC     = ${RUSTC:-<PATH default>}"
echo
echo "Then, from the repo root:"
echo "  npx tauri android build --debug --target aarch64   # one ABI, fastest"
echo "  npx tauri android build                            # release, all four ABIs"
