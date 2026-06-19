// swift-tools-version: 6.0
// Sufrix POS — iPhone/iPad app (SwiftUI). UI + platform glue ONLY; all logic
// lives in rust-core via the SufrixCore xcframework + generated UniFFI bindings.
//
// This SwiftPM package exists so the binding can be exercised from the command
// line on macOS (the `smoketest` executable). The shipping iOS app is an Xcode
// project that links `SufrixCore.xcframework` and includes the generated
// `sufrix_core.swift`; see README.md.
import PackageDescription

let package = Package(
    name: "SufrixApp",
    platforms: [.macOS(.v13), .iOS(.v16)],
    products: [
        .library(name: "SufrixUI", targets: ["SufrixUI"]),
        .executable(name: "smoketest", targets: ["smoketest"]),
    ],
    targets: [
        // SwiftUI screens + view models. Depends on the generated FFI bindings,
        // which are added by the binding-generation step (not committed here).
        .target(name: "SufrixUI", path: "Sources/SufrixUI"),
        // CLI smoke test: proves greet()/SufrixCore work through the FFI.
        .executableTarget(name: "smoketest", path: "Sources/smoketest"),
    ]
)
