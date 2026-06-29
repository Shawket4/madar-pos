#!/usr/bin/env bash
# Compile + run the Swift core-FFI test suite (swift-app/Tests/CoreFFITests.swift)
# against libmadar_core — the same proven link path as tool/smoketest-swift.sh,
# but a full assertion suite (pricing math + safety invariants + session
# offline-safety) instead of a single smoke check. Exit code reflects pass/fail.
set -euo pipefail
CORE_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$CORE_DIR"

# Ensure dylib + generated bindings exist.
[[ -f bindings/swift/MadarCoreFFI.swift ]] || ./tool/build-bindings.sh

SW=bindings/swift
INC="$(mktemp -d)"
OUT="$(mktemp -d)"
cp "$SW/MadarCoreFFIFFI.h" "$INC/"
cp "$SW/MadarCoreFFIFFI.modulemap" "$INC/module.modulemap"

echo "── Compiling Swift core-FFI tests…"
swiftc -O -parse-as-library \
  -I "$INC" \
  -L target/debug -lmadar_core \
  -Xlinker -rpath -Xlinker "$CORE_DIR/target/debug" \
  "$SW/MadarCoreFFI.swift" \
  ../swift-app/Tests/CoreFFITests.swift \
  -o "$OUT/coretests"

echo "── Running…"
"$OUT/coretests"
