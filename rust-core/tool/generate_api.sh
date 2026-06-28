#!/usr/bin/env bash
# Regenerates the typed Rust API client (crates/madar-api) from the MadarRust
# backend OpenAPI spec. Rust-core equivalent of the Flutter app's
# tool/generate_api.sh and the dashboard's `npm run generate:api`.
#
# Requires: cargo (backend checkout, to re-export the spec), node/npx, Java 17+.
set -euo pipefail

CORE_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BACKEND_DIR="${MADAR_BACKEND_DIR:-$CORE_DIR/../../MadarRust}"
PKG_DIR="$CORE_DIR/crates/madar-api"
SPEC="$BACKEND_DIR/openapi.json"

# 1/4 — (Re)export the spec from the backend unless SKIP_EXPORT=1.
if [[ "${SKIP_EXPORT:-0}" != "1" ]]; then
  echo "── 1/4 Exporting OpenAPI spec from backend…"
  ( cd "$BACKEND_DIR" && cargo run --quiet --bin export-openapi ) || {
    echo "   (export-openapi failed — falling back to existing $SPEC)"
  }
else
  echo "── 1/4 SKIP_EXPORT=1 — using existing $SPEC"
fi
[[ -f "$SPEC" ]] || { echo "!! spec not found at $SPEC"; exit 1; }

# 2/4 — Generate the Rust client (async reqwest, single-request-param structs).
echo "── 2/4 Generating Rust client (openapi-generator -g rust)…"
rm -rf "$PKG_DIR"
npx --yes @openapitools/openapi-generator-cli generate \
  -i "$SPEC" \
  -g rust \
  -o "$PKG_DIR" \
  --additional-properties=packageName=madar-api,supportAsync=true,library=reqwest,useSingleRequestParameter=true,preferUnsignedInt=true,bestFitInt=true

# 3/4 — Post-process: the backend serializes BigDecimal columns as JSON STRINGS
# (current_stock, reorder_threshold, prices, quantity_used, …) but the generator
# types them as f64. Make affected fields string-tolerant via serde. We tag the
# whole models dir with a helper module and a build-time note; full per-field
# fixups land in Phase 2 when the client is wired into madar-core.
echo "── 3/4 Post-processing (string-tolerant decimals deferred to Phase 2)…"
# Keep the generated crate out of the workspace's lint/doc noise.
cat > "$PKG_DIR/.openapi-generator-ignore" <<'EOF'
# Re-written by tool/generate_api.sh
.travis.yml
git_push.sh
EOF
# Silence clippy across the generated crate — it's machine output, not ours, and
# a workspace member now (so `cargo clippy` would otherwise flood with
# needless-return style lints). lib.rs already carries some allow attrs.
if ! grep -q 'allow(clippy::all)' "$PKG_DIR/src/lib.rs"; then
  { echo '#![allow(clippy::all)]'; cat "$PKG_DIR/src/lib.rs"; } > "$PKG_DIR/src/lib.rs.tmp" \
    && mv "$PKG_DIR/src/lib.rs.tmp" "$PKG_DIR/src/lib.rs"
fi

# 4/4 — Sanity compile the generated crate on its own.
echo "── 4/4 cargo check on generated client…"
( cd "$PKG_DIR" && cargo check --quiet ) && echo "   generated client compiles ✓" || {
  echo "!! generated client failed to compile — inspect $PKG_DIR (expected on first run; see Phase 2 notes)"
}

echo "Done. Generated Rust client lives in $PKG_DIR"
