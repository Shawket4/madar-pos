#!/usr/bin/env bash
# Fast type-check of the SwiftUI app (no link, no launch). Compiles all MadarUI
# sources + the generated UniFFI binding against the C module map. Use this to
# verify Swift edits without the full run-swift-mac.sh build+launch cycle.
set -euo pipefail
CORE_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$CORE_DIR"
[[ -f bindings/swift/MadarCoreFFI.swift ]] || ./tool/build-bindings.sh >/dev/null
SW="$CORE_DIR/bindings/swift"
INC="$(mktemp -d)"
cp "$SW/MadarCoreFFIFFI.h" "$INC/"
cp "$SW/MadarCoreFFIFFI.modulemap" "$INC/module.modulemap"
UI=()
while IFS= read -r f; do UI+=("$f"); done < <(find "$CORE_DIR/../swift-app/Sources/MadarUI" -name '*.swift')
swiftc -typecheck -I "$INC" "$SW/MadarCoreFFI.swift" "${UI[@]}"
echo "✓ SwiftUI type-check passed"
