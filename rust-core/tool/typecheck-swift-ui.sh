#!/usr/bin/env bash
# Type-checks the SwiftUI host (swift-app/Sources/SufrixUI) against the real
# generated UniFFI binding — no Xcode, no simulator. Catches drift between the
# core's FFI surface and the screens that consume it. Compiles nothing to disk;
# `swiftc -typecheck` only resolves types.
set -euo pipefail

CORE_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$CORE_DIR"

SW=bindings/swift
[[ -f "$SW/SufrixCoreFFI.swift" ]] || ./tool/build-bindings.sh

INC="$(mktemp -d)"
cp "$SW/SufrixCoreFFIFFI.h" "$INC/"
cp "$SW/SufrixCoreFFIFFI.modulemap" "$INC/module.modulemap"

echo "── Type-checking SufrixUI against the binding…"
UI_SOURCES=$(find ../swift-app/Sources/SufrixUI -name '*.swift')
swiftc -typecheck -parse-as-library -I "$INC" \
  "$SW/SufrixCoreFFI.swift" \
  $UI_SOURCES

echo "✓ SwiftUI host type-checks against the current FFI surface"
