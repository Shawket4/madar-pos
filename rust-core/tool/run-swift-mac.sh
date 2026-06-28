#!/usr/bin/env bash
# Builds + launches the SwiftUI host as a native macOS .app — no Xcode, no
# simulator. Compiles MadarUI against the generated binding + libmadar_core,
# bundles the Cairo fonts and brand assets, and `open`s the app so you can click
# through the real login on your Mac.
#
#   ./tool/run-swift-mac.sh
set -euo pipefail

CORE_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$CORE_DIR"

# 1/4 — cdylib + bindings.
echo "── Building core + bindings…"
[[ -f bindings/swift/MadarCoreFFI.swift ]] || ./tool/build-bindings.sh >/dev/null
cargo build -q -p madar-core

SW="$CORE_DIR/bindings/swift"
DEBUG="$CORE_DIR/target/debug"
RES_SRC="$CORE_DIR/../swift-app/Resources"

# 2/4 — assemble the .app skeleton.
APP="$CORE_DIR/target/MadarPOS.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APP/Contents/Frameworks"

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Madar POS</string>
  <key>CFBundleDisplayName</key><string>Madar POS</string>
  <key>CFBundleExecutable</key><string>MadarPOS</string>
  <key>CFBundleIdentifier</key><string>app.madar.pos.mac</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSPrincipalClass</key><string>NSApplication</string>
  <key>NSAppTransportSecurity</key>
  <dict>
    <key>NSAllowsArbitraryLoads</key><true/>
    <key>NSAllowsLocalNetworking</key><true/>
  </dict>
</dict>
</plist>
PLIST

# Bundled Cairo faces (the app registers the fonts at launch).
cp "$RES_SRC"/Fonts/Cairo-*.ttf "$APP/Contents/Resources/" 2>/dev/null || true
cp "$DEBUG/libmadar_core.dylib" "$APP/Contents/Frameworks/"

# Compile the real-logo asset catalog into the bundle (Assets.car).
if [[ -d "$RES_SRC/Assets.xcassets" ]]; then
  xcrun actool "$RES_SRC/Assets.xcassets" \
    --compile "$APP/Contents/Resources" \
    --platform macosx --minimum-deployment-target 13.0 \
    --output-partial-info-plist "$(mktemp)" >/dev/null 2>&1 \
    || echo "  (actool failed — the logo asset may not render)"
fi

# 3/4 — compile the SwiftUI app (the @main lives in MadarApp.swift).
INC="$(mktemp -d)"
cp "$SW/MadarCoreFFIFFI.h" "$INC/"
cp "$SW/MadarCoreFFIFFI.modulemap" "$INC/module.modulemap"
UI_SOURCES=$(find "$CORE_DIR/../swift-app/Sources/MadarUI" -name '*.swift')

echo "── Compiling SwiftUI app…"
swiftc -O \
  -I "$INC" \
  -L "$DEBUG" -lmadar_core \
  -Xlinker -rpath -Xlinker "@executable_path/../Frameworks" \
  "$SW/MadarCoreFFI.swift" \
  $UI_SOURCES \
  -o "$APP/Contents/MacOS/MadarPOS"

# 4/4 — launch.
echo "── Launching $APP"
open "$APP"
echo "✓ Madar POS is running. (Quit with ⌘Q.)"
