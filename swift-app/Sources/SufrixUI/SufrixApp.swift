// Sufrix POS — iOS/iPad app entry point (SwiftUI).
//
// Thin host: owns one `AppModel` (which owns the single `SufrixCore` handle) and
// renders what the core hands it. No business logic lives here — auth, token
// custody and the online↔offline decision are all in rust-core.
//
// The generated `SufrixCoreFFI.swift` binding + the `SufrixCore.xcframework`
// (built by ../rust-core/tool/build-ios.sh) must be added to the app target.
import SwiftUI

@main
struct SufrixApp: App {
    @StateObject private var app = AppModel()

    var body: some Scene {
        WindowGroup {
            RootView(app: app)
        }
    }
}

/// Root route: Login when signed out, the (placeholder) home when signed in.
/// Per PLAN §R11 the host consults this only at deliberate boundaries, never as
/// a side effect of connectivity.
struct RootView: View {
    @ObservedObject var app: AppModel

    var body: some View {
        ThemedRoot(mode: app.themeMode) {
            Group {
                switch app.route {
                case .deviceSetup, .login:
                    LoginView(app: app)
                case .openShift:
                    OpenShiftView(app: app)
                case .order:
                    OrderView(app: app)
                }
            }
            .environment(\.localize, { app.t($0) })
            .environment(\.layoutDirection, app.isRTL ? .rightToLeft : .leftToRight)
        }
    }
}
