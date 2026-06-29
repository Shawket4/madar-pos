#!/usr/bin/env bash
#
# Local pre-push gate for madar-core (the UniFFI POS core), modelled on the
# MadarRust backend's scripts/preflight.sh.
#
#   rust-core/scripts/preflight.sh             # FAST: fmt + clippy + lib tests (incl. proptests)
#   rust-core/scripts/preflight.sh --mutants   # + cargo-mutants on the lines you changed
#   rust-core/scripts/preflight.sh --fuzz       # + cargo-fuzz smoke on price_cart/verify_pin (nightly)
#   rust-core/scripts/preflight.sh --android   # + Android (cargo ndk arm64) build check
#   rust-core/scripts/preflight.sh --ios        # + iOS (aarch64-apple-ios) build check
#   rust-core/scripts/preflight.sh --all
#
# The lib tests are pure (money/order/pin engines) and need NO backend. The
# tests/ integration suite drives a LIVE dev backend, so it is deliberately NOT
# in this gate — run it by hand against a running server.
#
# Install as a hook:  ln -sf ../../scripts/preflight.sh .git/hooks/pre-push
set -uo pipefail
cd "$(dirname "$0")/.."   # rust-core/
export PATH="$HOME/.cargo/bin:$PATH"

M=0 A=0 I=0 F=0
for a in "$@"; do case "$a" in
  --mutants) M=1;; --android) A=1;; --ios) I=1;; --fuzz) F=1;;
  --all) M=1; A=1; I=1; F=1;;
  -h|--help) sed -n '2,19p' "$0"; exit 0;;
  *) echo "unknown flag: $a" >&2; exit 2;;
esac; done

FAILED=(); WARNED=(); SKIPPED=()
hdr(){ printf '\n\033[1m── %s ──\033[0m\n' "$1"; }
have(){ command -v "$1" >/dev/null 2>&1; }

hdr "rustfmt --check"
if cargo fmt --all --check; then echo "✓ formatted"
elif [ "${STRICT:-0}" = 1 ]; then FAILED+=("fmt"); else WARNED+=("fmt (run: cargo fmt)"); fi

hdr "clippy"
if cargo clippy --all-targets 2>&1 | tail -15; [ "${PIPESTATUS[0]}" = 0 ]; then echo "✓ clippy"
elif [ "${STRICT:-0}" = 1 ]; then FAILED+=("clippy"); else WARNED+=("clippy"); fi

hdr "cargo test --lib  (GATE: unit + proptests, no backend)"
if cargo test --lib; then echo "✓ tests pass"; else FAILED+=("test"); fi

# ── opt-in: mutation testing on changed lines (money/order engines) ───────────
if [ $M = 1 ]; then
  hdr "cargo-mutants --in-diff"
  if ! have cargo-mutants; then SKIPPED+=("mutants: cargo install cargo-mutants cargo-nextest")
  else
    base="$(git merge-base HEAD origin/main 2>/dev/null || git rev-parse HEAD~1 2>/dev/null || echo HEAD)"
    git diff "$base"...HEAD > /tmp/madar-core-preflight.diff 2>/dev/null || git diff HEAD > /tmp/madar-core-preflight.diff
    if [ ! -s /tmp/madar-core-preflight.diff ]; then SKIPPED+=("mutants: no diff vs $base")
    elif cargo mutants --in-diff /tmp/madar-core-preflight.diff --jobs 2; then echo "✓ no surviving mutants"
    else WARNED+=("mutants: survivors — see mutants.out/"); fi
  fi
fi

# ── opt-in: cargo-fuzz smoke on the pure money/pin engines (nightly) ──────────
if [ $F = 1 ]; then
  hdr "cargo-fuzz smoke (15s/target, nightly)  (GATE on crash)"
  if ! have rustup || ! rustup toolchain list 2>/dev/null | grep -q nightly; then
    SKIPPED+=("fuzz: nightly toolchain not installed (rustup toolchain install nightly)")
  elif ! have cargo-fuzz; then SKIPPED+=("fuzz: cargo install cargo-fuzz")
  else
    fz_fail=0
    for t in price_cart verify_pin; do
      echo "  fuzzing $t…"
      ( cd crates/madar-core && cargo +nightly fuzz run "$t" -- -max_total_time=15 ) >/dev/null 2>&1 \
        || { echo "  ✗ $t produced a crash"; fz_fail=1; }
    done
    [ $fz_fail = 0 ] && echo "✓ no crashes" || FAILED+=("fuzz")
  fi
fi

# ── opt-in: cross-platform build checks (the core MUST build everywhere) ──────
if [ $A = 1 ]; then
  hdr "Android build check (cargo ndk arm64)"
  if ! have cargo-ndk || [ -z "${ANDROID_NDK_HOME:-}" ]; then
    SKIPPED+=("android: needs ANDROID_NDK_HOME + 'cargo install cargo-ndk' (see android-cross-compile notes)")
  elif cargo ndk -t arm64-v8a check -p madar-core; then echo "✓ android arm64 builds"
  else FAILED+=("android build"); fi
fi
if [ $I = 1 ]; then
  hdr "iOS build check (aarch64-apple-ios)"
  if ! rustup target list --installed 2>/dev/null | grep -q aarch64-apple-ios; then
    SKIPPED+=("ios: rustup target add aarch64-apple-ios")
  elif cargo check -p madar-core --target aarch64-apple-ios; then echo "✓ ios builds"
  else FAILED+=("ios build"); fi
fi

hdr "preflight summary"
for w in "${WARNED[@]:-}";  do [ -n "$w" ] && printf '  \033[33m⚠ %s\033[0m\n' "$w"; done
for s in "${SKIPPED[@]:-}"; do [ -n "$s" ] && printf '  \033[2m• skipped: %s\033[0m\n' "$s"; done
if [ "${#FAILED[@]}" -gt 0 ] && [ -n "${FAILED[0]:-}" ]; then
  for f in "${FAILED[@]}"; do printf '  \033[31m✗ %s\033[0m\n' "$f"; done
  printf '\n\033[1;31mPREFLIGHT FAILED.\033[0m\n'; exit 1
fi
printf '\n\033[1;32mPREFLIGHT PASSED.\033[0m\n'
