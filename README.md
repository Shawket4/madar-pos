# Sufrix POS — Rebuild

A from-scratch rebuild of the Sufrix Point-of-Sale ("Teller") app as a **shared
Rust core** with **thin native UIs**.

## The one rule

All real logic lives in **`rust-core`**, compiled to a native library and
exposed through **UniFFI**. The Swift app and the Kotlin app are **UI and
platform glue only** — they call into the core for everything: data, business
rules, API calls, offline/sync, and printing. If a piece of logic could ever
differ between platforms, that's a bug; it belongs in Rust.

## Layout

```
sufrix-rebuild/
├── rust-core/        # the shared library — all logic, API, store, sync, printing
│   ├── crates/
│   │   ├── sufrix-core/   # the UniFFI library (Swift + Kotlin bindings)
│   │   └── sufrix-api/    # GENERATED openapi client (tool/generate_api.sh) — gitignored
│   ├── tool/             # generate_api.sh, build-bindings.sh, build-ios.sh, build-android.sh
│   └── .env              # base URL + environment, baked in at build time (gitignored)
├── swift-app/        # iPhone + iPad (SwiftUI). UI + platform glue only.
├── kotlin-app/       # Android phone/tablet + desktop (Compose Multiplatform). Glue only.
├── PLAN.md           # the canonical phased roadmap (generated from the spec)
└── README.md
```

## Source of truth

The backend OpenAPI spec (`../SufrixRust/openapi.json`, OpenAPI 3.1.0 — 230
operations, 264 schemas) is the **only** source of truth. The Rust API client is
generated from it with `openapi-generator -g rust` via
`rust-core/tool/generate_api.sh`. No endpoint paths or payloads are hardcoded.

## Build the core (Phase 1)

```bash
cd rust-core
cargo build                 # compiles sufrix-core + runs nothing
cargo test -p sufrix-core   # unit tests
./tool/generate_api.sh      # (re)generate the Rust API client from the spec
./tool/build-bindings.sh    # emit Swift + Kotlin bindings into bindings/
./tool/build-ios.sh         # assemble SufrixCore.xcframework for swift-app
./tool/build-android.sh     # build per-ABI .so + Kotlin bindings (needs Android NDK)
```

## Status

See [PLAN.md](PLAN.md) — **Revision 2** is the current source of truth (offline-first
design finalized; decisions D9–D12; auth simplified to Layer 1 + online-login-per-switch;
backend workstream P0→P2). Cross-repo audits in [docs/](docs/).

Progress:
- **Phase 0 (done):** monorepo + rust-core scaffold; UniFFI bindings generated; the
  Rust→FFI→Swift path proven on macOS (`tool/smoketest-swift.sh`); the Rust API client
  generates + compiles from the 3.1 spec.
- **Phase 1 (in progress):** the **`pricing` engine** is built — pure, client-authoritative,
  12 golden-vector tests mirroring the server formula (ties-away rounding, discount→tax order,
  bundle base + component surcharge, F8 clamp), and proven across the FFI from Swift.
  Next: `store`/outbox (unified idempotency key, F1) + the backend P0 contracts.

> **Toolchain note:** this build machine has Rust (all mobile targets), Java 17,
> Node, and Xcode 26. It does **not** yet have the Android SDK/NDK, so the
> Android target can be scaffolded and the *desktop* Compose target built, but a
> full Android device build needs the SDK installed first.
