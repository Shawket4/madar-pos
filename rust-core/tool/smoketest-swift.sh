#!/usr/bin/env bash
# Phase-1 proof: compiles the generated Swift UniFFI bindings + the swift-app
# smoke test against libsufrix_core and RUNS it on macOS, proving the whole
# Rust -> UniFFI -> Swift pipeline works end to end. No Xcode project needed.
set -euo pipefail

CORE_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$CORE_DIR"

# Ensure dylib + bindings exist.
[[ -f bindings/swift/SufrixCoreFFI.swift ]] || ./tool/build-bindings.sh

SW=bindings/swift
INC="$(mktemp -d)"
OUT="$(mktemp -d)"
cp "$SW/SufrixCoreFFIFFI.h" "$INC/"
cp "$SW/SufrixCoreFFIFFI.modulemap" "$INC/module.modulemap"

echo "── Compiling Swift smoke test…"
swiftc -O \
  -I "$INC" \
  -L target/debug -lsufrix_core \
  -Xlinker -rpath -Xlinker "$CORE_DIR/target/debug" \
  "$SW/SufrixCoreFFI.swift" \
  ../swift-app/Sources/smoketest/main.swift \
  -o "$OUT/smoketest"

echo "── Running…"
"$OUT/smoketest"
