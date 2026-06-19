# swift-app — Sufrix POS (iPhone / iPad)

Thin SwiftUI host. **No business logic** — it calls into `rust-core` over UniFFI.

## What's here (Phase 1)

- `Package.swift` — SwiftPM package with `SufrixUI` (the screens) and a
  `smoketest` executable that exercises the FFI from the command line.
- `Sources/SufrixUI/` — `SufrixApp` (entry) + `ContentView` (Phase-1 placeholder).
- `Sources/smoketest/` — CLI proof, run by `../rust-core/tool/smoketest-swift.sh`.

## Prove the binding works (macOS, no Xcode)

```bash
cd ../rust-core && ./tool/smoketest-swift.sh
```

Compiles the generated `SufrixCoreFFI.swift` + `smoketest` against
`libsufrix_core` and runs it.

## Build the real iOS app

1. Build the framework + bindings:
   ```bash
   cd ../rust-core && ./tool/build-ios.sh        # -> target/SufrixCore.xcframework + swift glue
   ```
2. Create an iOS App target in Xcode (or `xcodegen`), then:
   - Add `SufrixCore.xcframework` (Embed & Sign).
   - Add the generated `SufrixCoreFFI.swift` to the target.
   - Add `Sources/SufrixUI/*.swift`.
3. Run on a simulator/device.

> The shipping app is an Xcode project (not committed yet) wrapping this package.
> Phase 6 builds out the real Login → Shift → Order → Cart → Payment → Receipt
> screens per PLAN.md §6.
