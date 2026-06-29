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
    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        ThemedRoot(mode: app.themeMode) {
            Group {
                switch app.route {
                case .deviceSetup:
                    // A kitchen-role device bound to a branch but with no station
                    // yet lands on .deviceSetup → show the station picker (not the
                    // manager binding, which is for an unconfigured device).
                    if app.session != nil && app.isKitchenDevice {
                        StationPickerView(app: app)
                    } else {
                        LoginView(app: app)
                    }
                case .login:
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
            .realtimeAlertHost(app)
        }
        // App-level connectivity heartbeat — runs on EVERY route (not just Order /
        // OpenShift), so a KDS / waiter / settings device still drains its outbox on
        // a timer, and a cold start flushes a restored backlog on the first tick.
        // Tied to sign-in so it starts/stops with the session; the core's
        // single-flight drain makes overlap with any screen-level refresh harmless.
        .task(id: app.isSignedIn) {
            guard app.isSignedIn else { return }
            while !Task.isCancelled {
                await app.refreshConnectivity()
                try? await Task.sleep(nanoseconds: 15_000_000_000)
            }
        }
        // Foreground / app-resume drain — independent of the timer so a backgrounded
        // app flushes its backlog the instant it returns to the foreground.
        .onChange(of: scenePhase) { phase in
            if phase == .active, app.isSignedIn {
                Task { await app.refreshConnectivity() }
            }
        }
    }
}
