# swift-app — Sufrix POS (SwiftUI)

Thin SwiftUI host. **No business logic** — it calls into `rust-core` over UniFFI.

## What's here

- `Sources/SufrixUI/` — the app:
  - `Theme/Tokens.swift` — design tokens (colors light/dark, spacing, radius,
    Cairo type, motion), ported 1:1 from the Flutter `AppTokens`.
  - `Components/` — the shared library: `PressableScale`, `SufrixButton`,
    `SufrixTextField`, `Chips`, `PinPad`, `SufrixMark`/`SufrixLockup`.
  - `LoginView` + `AppModel` — the branch-gated login (manager device-setup →
    teller PIN), `KeychainTokenStore`, `SufrixApp`/`ContentView`.
- `Resources/Assets.xcassets` — the real brand vectors (`SufrixMark`,
  `SufrixWordmark`) with light/dark variants. `Resources/Fonts/` — Cairo.
- `project.yml` — xcodegen spec for the macOS app.
- `Package.swift` — SwiftPM package (kept for the `smoketest` CLI proof).

## Run it (macOS, one command, no Xcode)

```bash
cd ../rust-core && ./tool/run-swift-mac.sh
```

Builds the core, generates the bindings, compiles `SufrixUI` into a native
`SufrixPOS.app` (Cairo + the real logo bundled, the core dylib linked), and
launches it.

## Open the Xcode project

```bash
cd swift-app && xcodegen generate && open SufrixPOS.xcworkspace
```

`SufrixPOS.xcworkspace` → the `SufrixPOS` macOS app target. A pre-build phase
regenerates the Rust bindings + builds `libsufrix_core`, so ⌘R just works.
(`SufrixPOS.xcodeproj` and `Generated/` are gitignored — regenerated from
`project.yml`.) The base API URL is baked from `rust-core/.env` at build time.

## Smoke-test the FFI (CLI)

```bash
cd ../rust-core && ./tool/smoketest-swift.sh      # proves the binding end-to-end
./tool/typecheck-swift-ui.sh                       # type-checks SufrixUI vs the binding
```

## iOS

`./tool/build-ios.sh` builds `SufrixCore.xcframework`; add an iOS target to
`project.yml` (framework dep + the SufrixUI sources) to ship to device/simulator.
