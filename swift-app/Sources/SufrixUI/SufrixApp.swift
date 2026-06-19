// Sufrix POS — iOS/iPad app entry point (SwiftUI).
//
// Thin host: it owns a single `SufrixCore` handle (from rust-core) and renders
// what the core hands it. No business logic lives here.
//
// The generated `SufrixCoreFFI.swift` binding + the `SufrixCore.xcframework`
// (built by ../rust-core/tool/build-ios.sh) must be added to the app target.
import SwiftUI

@main
struct SufrixApp: App {
    /// The one core handle, created at launch and kept for the app lifetime.
    /// Phase 2 fills `dbPath` with the app-support directory and wires auth.
    @State private var core = SufrixCore.fromEnv()

    var body: some Scene {
        WindowGroup {
            ContentView(core: core)
        }
    }
}
