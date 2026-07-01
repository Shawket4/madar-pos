#!/usr/bin/env bash
# Swift visual-verification harness — the mirror of the Kotlin
# `:composeApp:screenshots` gradle task. Renders the REAL refreshed SwiftUI
# screens (Login, OpenShift, StationPicker, Reauth, Order, Tender, ItemDetail,
# BundleDetail, OrderHistory, OrderSearch, CashAndShifts, CloseShift,
# KitchenDisplay, Settings, Sync, Delivery, Incoming, Waiter, Drafts) to PNG
# headlessly: no Xcode, no simulator, no on-screen window.
#
# It compiles the MadarUI sources + the generated UniFFI binding + the harness
# main (tool/screenshot-harness/ScreenshotMain.swift) into one executable, links
# target/debug/libmadar_core, then runs it. The harness builds a standalone
# offline AppModel, seeds a cart through the real core, and rasterizes each
# screen (light + dark) with SwiftUI's ImageRenderer.
#
# The harness lives OUTSIDE swift-app/Sources/MadarUI, so it can never break the
# MadarUI type-check. Output: rust-core/target/swift-screenshots/.
#
#   ./tool/screenshot-swift.sh
set -euo pipefail

CORE_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$CORE_DIR"

SW="$CORE_DIR/bindings/swift"
DEBUG="$CORE_DIR/target/debug"
UI_DIR="$CORE_DIR/../swift-app/Sources/MadarUI"
FONT_DIR="$CORE_DIR/../swift-app/Resources/Fonts"
HARNESS="$CORE_DIR/tool/screenshot-harness/ScreenshotMain.swift"
OUT_DIR="$CORE_DIR/target/swift-screenshots"

# 1/3 — cdylib + bindings (idempotent; only rebuilds if stale).
echo "── Building core + bindings…"
[[ -f "$SW/MadarCoreFFI.swift" ]] || ./tool/build-bindings.sh >/dev/null
cargo build -q -p madar-core

# 2/3 — compile the harness against the real UI + binding, link the dylib.
INC="$(mktemp -d)"
cp "$SW/MadarCoreFFIFFI.h" "$INC/"
cp "$SW/MadarCoreFFIFFI.modulemap" "$INC/module.modulemap"
# Exclude MadarApp.swift — it carries the real `@main`, which would collide with
# the harness's `@main`. Everything else (every screen + component) is compiled.
UI_SOURCES=$(find "$UI_DIR" -name '*.swift' ! -name 'MadarApp.swift')

# Build into a minimal .app bundle. The harness builds a real AppModel, whose
# init wires UNUserNotificationCenter (RealtimeAlertPlayer) — that aborts in a
# bare executable because there's no main bundle. A tiny .app with an Info.plist
# (a real CFBundleIdentifier + bundleURL) satisfies it, while we still run the
# binary directly so it stays headless (no `open`, no window).
APP="$CORE_DIR/target/MadarScreenshots.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
BIN="$APP/Contents/MacOS/MadarScreenshots"
mkdir -p "$OUT_DIR"

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>MadarScreenshots</string>
  <key>CFBundleExecutable</key><string>MadarScreenshots</string>
  <key>CFBundleIdentifier</key><string>app.madar.pos.screenshots</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSPrincipalClass</key><string>NSApplication</string>
</dict>
</plist>
PLIST

echo "── Compiling SwiftUI screenshot harness…"
swiftc -O \
  -I "$INC" \
  -L "$DEBUG" -lmadar_core \
  -Xlinker -rpath -Xlinker "$DEBUG" \
  -framework AppKit -framework SwiftUI \
  "$SW/MadarCoreFFI.swift" \
  $UI_SOURCES \
  "$HARNESS" \
  -o "$BIN"

# 3/3 — run headless (execute the bundled binary directly). The harness
# registers the Cairo faces itself (the .app ships no font resources), so pass
# the font + output dirs.
echo "── Rendering…"
MADAR_FONT_DIR="$FONT_DIR" \
MADAR_OUT_DIR="$OUT_DIR" \
DYLD_LIBRARY_PATH="$DEBUG" \
  "$BIN"

echo "── PNGs in $OUT_DIR:"
ls -la "$OUT_DIR"/*.png 2>/dev/null || { echo "✗ no PNGs produced"; exit 1; }
echo "✓ Swift screenshots written to $OUT_DIR"
