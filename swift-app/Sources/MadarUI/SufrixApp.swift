// Madar POS — iOS/iPad app entry point (SwiftUI).
//
// Thin host: owns one `AppModel` (which owns the single `MadarCore` handle) and
// renders what the core hands it. No business logic lives here — auth, token
// custody and the online↔offline decision are all in rust-core.
//
// The generated `MadarCoreFFI.swift` binding + the `MadarCore.xcframework`
// (built by ../rust-core/tool/build-ios.sh) must be added to the app target.
import SwiftUI

@main
struct MadarApp: App {
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
                case .order, .waiterTickets:
                    // The waiter uses the SAME order component as the teller — the
                    // full menu/cart + app chrome (top bar, More-drawer nav). It runs
                    // in "fire" mode (fire a ticket instead of tendering); the
                    // open-tickets list is a sub-screen reached from the top bar.
                    OrderView(app: app)
                case let .kitchenDisplay(stationId):
                    KitchenDisplayView(app: app, stationId: stationId)
                }
            }
            .environment(\.localize, { app.t($0) })
            .environment(\.layoutDirection, app.isRTL ? .rightToLeft : .leftToRight)
            .toastHost(app)
        }
    }
}
