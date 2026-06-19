#!/usr/bin/env bash
# Generates the UniFFI Swift + Kotlin bindings (library mode) from the compiled
# cdylib, into rust-core/bindings/{swift,kotlin}. These are consumed by
# swift-app and kotlin-app respectively.
set -euo pipefail

CORE_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$CORE_DIR"

PROFILE="${PROFILE:-debug}"
[[ "$PROFILE" == "release" ]] && BUILD_FLAG="--release" || BUILD_FLAG=""

# Host dylib extension.
case "$(uname -s)" in
  Darwin) EXT=dylib ;;
  Linux)  EXT=so ;;
  *)      EXT=dll ;;
esac
LIB="$CORE_DIR/target/$PROFILE/libsufrix_core.$EXT"

echo "── Building cdylib ($PROFILE)…"
cargo build $BUILD_FLAG -p sufrix-core

echo "── Generating Swift bindings…"
rm -rf bindings/swift && mkdir -p bindings/swift
cargo run $BUILD_FLAG -p sufrix-core --bin uniffi-bindgen -- generate \
  --library "$LIB" --language swift --out-dir bindings/swift --no-format

echo "── Generating Kotlin bindings…"
rm -rf bindings/kotlin && mkdir -p bindings/kotlin
cargo run $BUILD_FLAG -p sufrix-core --bin uniffi-bindgen -- generate \
  --library "$LIB" --language kotlin --out-dir bindings/kotlin --no-format

echo "Done. Bindings in $CORE_DIR/bindings/{swift,kotlin}"
ls -R bindings
