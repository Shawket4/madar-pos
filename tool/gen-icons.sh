#!/usr/bin/env bash
# Generates the shared Sufrix icon set (Lucide — the clean, MIT, React/shadcn
# icon family) into BOTH apps so iOS and Android render pixel-identical glyphs:
#   - iOS:     swift-app/Resources/Assets.xcassets/Icons/<lucide>.imageset (SVG, template-tinted)
#   - Compose: kotlin-app/.../composeResources/drawable/ic_<lucide>.xml   (stroke-faithful vector drawable)
# and the per-platform name catalogs that map the SF-Symbol names used in the
# code (e.g. "cart", "checkmark.circle") to the shared asset:
#   - swift-app/Sources/SufrixUI/Theme/IconCatalog.swift   (SF name -> asset name)
#   - kotlin-app/.../ui/IconCatalog.kt                     (SF name -> DrawableResource)
# The MAP below is the single source of truth. Re-run after editing it.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUILD="$ROOT/tool/.icon-build"
LUCIDE="$BUILD/node_modules/lucide-static/icons"
XCASSETS="$ROOT/swift-app/Resources/Assets.xcassets/Icons"
DRAWABLE="$ROOT/kotlin-app/composeApp/src/commonMain/composeResources/drawable"
SWIFT_CAT="$ROOT/swift-app/Sources/SufrixUI/Theme/IconCatalog.swift"
KT_CAT="$ROOT/kotlin-app/composeApp/src/commonMain/kotlin/app/sufrix/ui/IconCatalog.kt"
CONV="$ROOT/tool/lucide_to_vd.py"

# SF-Symbol name  ->  Lucide icon name. Covers ui/Icons.kt sfSymbol() + the
# screen-level glyph holdouts. Keep stroke (outline) Lucide names.
read -r -d '' MAP <<'EOF' || true
exclamationmark.circle circle-alert
exclamationmark.triangle triangle-alert
exclamationmark.bubble message-square-warning
xmark x
xmark.circle circle-x
xmark.circle.fill circle-x
printer printer
number hash
tag tag
tag.fill tag
chevron.backward chevron-left
chevron.left chevron-left
chevron.forward chevron-right
chevron.right chevron-right
chevron.down chevron-down
chevron.up chevron-up
checkmark.circle circle-check
checkmark.circle.fill circle-check-big
checkmark check
checkmark.seal badge-check
trash trash-2
trash.fill trash-2
text.bubble message-square
note.text file-text
doc.text file-text
person user
person.fill user
person.crop.circle.badge.clock circle-user
magnifyingglass search
list.bullet.rectangle list
list.bullet list
building.2 building-2
plus plus
minus minus
plus.forwardslash.minus calculator
lock lock
lock.circle lock
lock.open lock-open
clock.arrow.circlepath history
clock clock
clock.badge.checkmark clock
clock.badge.exclamationmark clock
tray.full inbox
tray inbox
tray.and.arrow.down download
rectangle.portrait.and.arrow.right log-out
icloud.and.arrow.up cloud-upload
gearshape settings
bicycle bike
banknote banknote
wifi.slash wifi-off
wifi wifi
storefront store
square.stack.3d.up.fill layers
square.grid.2x2.fill grid-2x2
heart.circle heart
hand.raised hand
envelope mail
ellipsis.circle ellipsis
ellipsis ellipsis
delete.left delete
creditcard credit-card
cart shopping-cart
bag.fill shopping-bag
arrow.triangle.2.circlepath refresh-cw
arrow.right.circle circle-arrow-right
arrow.clockwise rotate-cw
arrow.up chevron-up
arrow.down chevron-down
wallet wallet
bank landmark
building.columns landmark
qrcode qr-code
qr qr-code
play.circle circle-play
gift gift
fork.knife utensils-crossed
cup.and.saucer coffee
croissant croissant
arrow.up.arrow.down arrow-up-down
arrow.down.left arrow-down-left
arrow.up.right arrow-up-right
iphone smartphone
line.3.horizontal menu
square.grid.2x2 grid-2x2
circle circle
largecircle.fill.circle circle-dot
rectangle.split.2x1 columns-2
exclamationmark.triangle.fill triangle-alert
cat.coffee coffee
cat.mocha coffee
cat.tea coffee
cat.bakery croissant
cat.lunch sandwich
cat.icecream ice-cream-cone
cat.drink cup-soda
cat.water glass-water
cat.ice snowflake
cat.matcha leaf
chart.pie pie-chart
receipt receipt
star star
link link
arrow.up.circle circle-arrow-up
arrow.down.circle circle-arrow-down
slider.horizontal.3 sliders-horizontal
shippingbox package
checkmark.icloud cloud-check
line.3.horizontal.decrease.circle list-filter
EOF

echo "▸ Ensuring lucide-static …"
mkdir -p "$BUILD"
if [ ! -d "$LUCIDE" ]; then
  (cd "$BUILD" && npm install lucide-static@1.21.0 --no-audit --no-fund --fetch-timeout=30000 >/dev/null)
fi

echo "▸ Clearing old generated assets …"
rm -rf "$XCASSETS"; mkdir -p "$XCASSETS"
rm -f "$DRAWABLE"/ic_*.xml; mkdir -p "$DRAWABLE"

# Unique lucide names referenced by the MAP (portable dedup — no assoc arrays,
# macOS ships bash 3.2).
UNIQUE_LUCIDE="$(printf '%s\n' "$MAP" | awk 'NF{print $2}' | sort -u)"
gen_asset() {
  local lucide="$1" und="${1//-/_}" svg="$LUCIDE/$1.svg"
  if [ ! -f "$svg" ]; then echo "  ✗ MISSING lucide: $lucide" >&2; MISSING=1; return 0; fi
  # iOS imageset — as a PDF, NOT SVG. Xcode's asset-catalog SVG importer renders
  # FILLS and effectively ignores strokes, so Lucide's stroke-only icons
  # (fill="none") import BLANK. rsvg renders the strokes to a vector PDF (with the
  # currentColor baked to black) which Xcode template-tints correctly.
  mkdir -p "$XCASSETS/$lucide.imageset"
  sed 's/currentColor/#000000/g' "$svg" > "$BUILD/_black.svg"
  rsvg-convert -f pdf -o "$XCASSETS/$lucide.imageset/$lucide.pdf" "$BUILD/_black.svg"
  cat > "$XCASSETS/$lucide.imageset/Contents.json" <<JSON
{
  "images" : [ { "filename" : "$lucide.pdf", "idiom" : "universal" } ],
  "info" : { "author" : "xcode", "version" : 1 },
  "properties" : { "preserves-vector-representation" : true, "template-rendering-intent" : "template" }
}
JSON
  # Compose vector drawable
  python3 "$CONV" "$svg" "$DRAWABLE/ic_$und.xml"
}

MISSING=0
for lucide in $UNIQUE_LUCIDE; do
  gen_asset "$lucide"
done

# ---- Swift catalog: SF name -> asset (lucide) name ----
{
  echo "// GENERATED by tool/gen-icons.sh — do not edit. Maps the SF-Symbol names"
  echo "// used across the SwiftUI code to the shared Lucide asset in Assets.xcassets/Icons."
  echo "let sufrixIconAsset: [String: String] = ["
  while read -r sf lucide; do
    [ -z "$sf" ] && continue
    echo "    \"$sf\": \"$lucide\","
  done <<< "$MAP"
  echo "]"
} > "$SWIFT_CAT"

# ---- Kotlin catalog: SF name -> DrawableResource ----
{
  echo "// GENERATED by tool/gen-icons.sh — do not edit."
  echo "package app.sufrix.ui"
  echo ""
  echo "import org.jetbrains.compose.resources.DrawableResource"
  echo "import app.sufrix.resources.Res"
  # one import per unique drawable
  for lucide in $UNIQUE_LUCIDE; do
    echo "import app.sufrix.resources.ic_${lucide//-/_}"
  done
  echo ""
  echo "/** SF-Symbol name -> shared Lucide drawable. null = unmapped (caller draws nothing). */"
  echo "fun lucideRes(sf: String): DrawableResource? = when (sf) {"
  while read -r sf lucide; do
    [ -z "$sf" ] && continue
    echo "    \"$sf\" -> Res.drawable.ic_${lucide//-/_}"
  done <<< "$MAP"
  echo "    else -> null"
  echo "}"
} > "$KT_CAT"

echo "▸ Done. iOS imagesets: $(ls -1 "$XCASSETS" | wc -l | tr -d ' '), Compose drawables: $(ls -1 "$DRAWABLE"/ic_*.xml | wc -l | tr -d ' ')"
[ "$MISSING" = 1 ] && { echo "✗ some lucide names were missing — fix MAP"; exit 1; } || echo "✓ all icons generated"
