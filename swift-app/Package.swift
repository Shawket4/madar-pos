// swift-tools-version: 6.0
// Madar POS — iPhone/iPad app (SwiftUI). UI + platform glue ONLY; all logic
// lives in rust-core via the MadarCore xcframework + generated UniFFI bindings.
//
// This SwiftPM package exists so the binding can be exercised from the command
// line on macOS (the `smoketest` executable). The shipping iOS app is an Xcode
// project that links `MadarCore.xcframework` and includes the generated
// `madar_core.swift`; see README.md.
import PackageDescription

let package = Package(
    name: "MadarApp",
    platforms: [.macOS(.v13), .iOS(.v16)],
    products: [
        .library(name: "MadarUI", targets: ["MadarUI"]),
        .executable(name: "smoketest", targets: ["smoketest"]),
    ],
    targets: [
        // SwiftUI screens + view models. Depends on the generated FFI bindings,
        // which are added by the binding-generation step (not committed here).
        .target(name: "MadarUI", path: "Sources/MadarUI"),
        // CLI smoke test: proves greet()/MadarCore work through the FFI.
        .executableTarget(name: "smoketest", path: "Sources/smoketest"),
    ]
)
