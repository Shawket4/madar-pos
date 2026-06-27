// Unified "Orders" surface (teller): delivery + waiter open-tickets in ONE place,
// two tabs, fed by the ONE session-level SSE. Replaces the separate delivery and
// settle-tickets screens. Both bodies are live via shared @Published state
// (onRealtimeEvent → loadDeliveryOrders / loadOpenTickets), and new incoming work
// pings + notifies via the core's realtime alert path — so a waiter firing on
// another device reaches the teller here instantly.
import SwiftUI

struct IncomingView: View {
    @ObservedObject var app: AppModel
    let onClose: () -> Void
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    private var deliveryCount: Int { app.deliveryOrders.count }
    private var ticketCount: Int {
        app.openTickets.filter { $0.status == "open" || $0.status == "ready" }.count
    }

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            VStack(spacing: 0) {
                header
                Group {
                    if app.incomingTab == 0 { DeliveryBody(app: app) }
                    else { SettleBody(app: app) }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        // Seed both lists so each tab's count badge is right from the start.
        .task { await app.loadDeliveryOrders(); await app.loadOpenTickets() }
    }

    private var header: some View {
        VStack(spacing: Space.sm) {
            HStack(spacing: Space.md) {
                Button { onClose() } label: {
                    MadarIcon("chevron.backward", size: 17).foregroundStyle(theme.colors.textPrimary)
                }.buttonStyle(.plain)
                Text(t("incoming.title")).font(.ui(17, .heavy)).foregroundStyle(theme.colors.textPrimary)
                Spacer()
            }
            Picker("", selection: $app.incomingTab) {
                Text(label(t("delivery.title"), deliveryCount)).tag(0)
                Text(label(t("waiter.title"), ticketCount)).tag(1)
            }
            .pickerStyle(.segmented)
        }
        .padding(.horizontal, Space.lg).padding(.vertical, Space.md)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }

    private func label(_ s: String, _ n: Int) -> String { n > 0 ? "\(s) (\(n))" : s }
}
