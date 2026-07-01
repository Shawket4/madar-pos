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

    // Raised header surface: back + title, then the live segmented tab bar.
    private var header: some View {
        VStack(spacing: Space.md) {
            HStack(spacing: Space.md) {
                Button { onClose() } label: {
                    MadarIcon("chevron.backward", size: IconSize.xl).foregroundStyle(theme.colors.textPrimary)
                }
                .buttonStyle(.pressable)
                Text(t("incoming.title")).font(.ui(17, .heavy)).foregroundStyle(theme.colors.textPrimary)
                Spacer()
            }
            IncomingTabBar(
                deliveryLabel: t("delivery.title"), deliveryCount: deliveryCount,
                ticketLabel: t("waiter.title"), ticketCount: ticketCount,
                selection: $app.incomingTab
            )
        }
        .padding(.horizontal, Space.lg).padding(.vertical, Space.md)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }
}

/// The Incoming tab bar — a teal segmented control with live per-tab count
/// badges. Replaces the stock `Picker(.segmented)` so it reads in the Madar
/// design language (teal active fill, on-accent count pills, press-scale).
private struct IncomingTabBar: View {
    @Environment(\.theme) private var theme
    let deliveryLabel: String
    let deliveryCount: Int
    let ticketLabel: String
    let ticketCount: Int
    @Binding var selection: Int

    var body: some View {
        HStack(spacing: Space.xs) {
            IncomingTab(label: deliveryLabel, count: deliveryCount, active: selection == 0) { selection = 0 }
            IncomingTab(label: ticketLabel, count: ticketCount, active: selection == 1) { selection = 1 }
        }
        .padding(Space.xs)
        .background(theme.colors.surfaceAlt)
        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
    }
}

/// One segment — label + an optional count pill, teal fill when active. Mirrors
/// the held-orders tab idiom (active = on-accent count pill, idle = surface).
private struct IncomingTab: View {
    @Environment(\.theme) private var theme
    let label: String
    let count: Int
    let active: Bool
    let onTap: () -> Void

    private var fg: Color { active ? theme.colors.textOnAccent : theme.colors.textSecondary }

    var body: some View {
        Button(action: onTap) {
            HStack(spacing: Space.sm) {
                Text(label).font(.ui(15, active ? .bold : .semibold))
                if count > 0 {
                    Text("\(count)")
                        .font(.ui(11, .bold))
                        .padding(.horizontal, Space.xs + 2).padding(.vertical, 1)
                        .background(active ? theme.colors.textOnAccent.opacity(Opacity.border) : theme.colors.surface)
                        .clipShape(Capsule())
                }
            }
            .foregroundStyle(fg)
            .frame(maxWidth: .infinity)
            .padding(.horizontal, Space.md).padding(.vertical, Space.sm)
            .background(active ? theme.colors.accent : Color.clear)
            .clipShape(RoundedRectangle(cornerRadius: Radii.xs, style: .continuous))
        }
        .buttonStyle(.pressable)
    }
}
