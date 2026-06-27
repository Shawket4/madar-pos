#!/usr/bin/env bash
# Builds the Android JNI native libs (one .so per ABI) + Kotlin bindings, laid
# out under kotlin-app's jniLibs. Requires the Android NDK (set ANDROID_NDK_HOME)
# and cargo-ndk (`cargo install cargo-ndk`).
#
# NOTE: this machine currently has no Android SDK/NDK installed — see PLAN.md
# §Risks. Install the SDK + NDK, then run this.
set -euo pipefail

CORE_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$CORE_DIR"
PROFILE="${PROFILE:-release}"

: "${ANDROID_NDK_HOME:?Set ANDROID_NDK_HOME to your NDK path}"
command -v cargo-ndk >/dev/null || { echo "cargo install cargo-ndk first"; exit 1; }

JNILIBS="$CORE_DIR/../kotlin-app/composeApp/src/androidMain/jniLibs"
mkdir -p "$JNILIBS"

echo "── Building .so for arm64-v8a, armeabi-v7a, x86_64…"
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -o "$JNILIBS" \
  build $([[ "$PROFILE" == release ]] && echo --release) -p madar-core

echo "── Generating Kotlin bindings…"
GEN="$CORE_DIR/../kotlin-app/composeApp/src/commonMain/kotlin"
mkdir -p "$GEN"
HOST_LIB="$CORE_DIR/target/$PROFILE/libmadar_core.$([[ $(uname) == Darwin ]] && echo dylib || echo so)"
# Always (re)build the host lib so bindings are generated from the CURRENT source.
# A stale dylib here silently emits bindings whose UniFFI checksums don't match
# the freshly-built .so above → runtime "UniFFI API checksum mismatch" on launch.
cargo build $([[ "$PROFILE" == release ]] && echo --release) -p madar-core
cargo run -p madar-core --bin uniffi-bindgen -- generate \
  --library "$HOST_LIB" --language kotlin --out-dir "$GEN" --no-format

echo "Done. .so under $JNILIBS, Kotlin bindings under $GEN"
