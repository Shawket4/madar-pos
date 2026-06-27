#!/usr/bin/env bash
# Builds the iOS .xcframework (device + simulator) and the Swift bindings, then
# assembles MadarCore.xcframework consumed by swift-app.
#
# Output: rust-core/target/MadarCore.xcframework  +  bindings/swift/*
set -euo pipefail

CORE_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$CORE_DIR"
PROFILE="${PROFILE:-release}"
[[ "$PROFILE" == "release" ]] && BUILD_FLAG="--release" || BUILD_FLAG=""

DEVICE=aarch64-apple-ios
SIM_ARM=aarch64-apple-ios-sim
SIM_X86=x86_64-apple-ios

echo "── Building staticlib for device + simulator…"
cargo build $BUILD_FLAG -p madar-core --target "$DEVICE"
cargo build $BUILD_FLAG -p madar-core --target "$SIM_ARM"
cargo build $BUILD_FLAG -p madar-core --target "$SIM_X86"

OUT="$CORE_DIR/target"
SIM_UNI="$OUT/ios-sim-universal"
mkdir -p "$SIM_UNI"
lipo -create \
  "$OUT/$SIM_ARM/$PROFILE/libmadar_core.a" \
  "$OUT/$SIM_X86/$PROFILE/libmadar_core.a" \
  -output "$SIM_UNI/libmadar_core.a"

echo "── Generating Swift bindings + modulemap…"
GEN="$OUT/swift-gen"
rm -rf "$GEN" && mkdir -p "$GEN"
cargo run $BUILD_FLAG -p madar-core --bin uniffi-bindgen -- generate \
  --library "$OUT/$DEVICE/$PROFILE/libmadar_core.a" \
  --language swift --out-dir "$GEN" --no-format

# UniFFI emits <module>FFI.modulemap + a .h + the .swift. The xcframework needs
# a headers dir containing the generated header + modulemap renamed module.modulemap.
HEADERS="$OUT/ios-headers"
rm -rf "$HEADERS" && mkdir -p "$HEADERS"
cp "$GEN"/*.h "$HEADERS"/ 2>/dev/null || true
cp "$GEN"/*FFI.modulemap "$HEADERS/module.modulemap" 2>/dev/null || true

echo "── Assembling xcframework…"
rm -rf "$OUT/MadarCore.xcframework"
xcodebuild -create-xcframework \
  -library "$OUT/$DEVICE/$PROFILE/libmadar_core.a" -headers "$HEADERS" \
  -library "$SIM_UNI/libmadar_core.a" -headers "$HEADERS" \
  -output "$OUT/MadarCore.xcframework"

echo "Done: $OUT/MadarCore.xcframework"
echo "Swift glue: $GEN/*.swift  (add to swift-app target)"
