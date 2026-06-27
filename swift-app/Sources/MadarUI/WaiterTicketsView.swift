// Waiter open-tickets list — a sub-screen over the SHARED order screen. The waiter
// reuses the teller's `OrderView` (full menu/cart + app chrome), FIRING a round
// instead of tendering; this screen lists the branch's open/ready tickets. "Add
// round" returns to the order screen targeting that ticket; "void" cancels it.
// Settlement happens at a cashier/till, never here. All logic is in the core.
import SwiftUI

// The generated `TicketView` carries a stable `id`; conform it so it can drive an
// `item:`-style sheet (the void confirmation).
extension TicketView: Identifiable {}

struct WaiterTicketsListView: View {
    @ObservedObject var app: AppModel
    let onClose: () -> Void
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    @State private var voiding: TicketView?

    private var currency: String { app.session?.currencyCode ?? "" }

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            VStack(spacing: 0) {
                ScreenHeader(t("waiter.tickets"), isLoading: app.isLoadingTickets, onBack: onClose) {
                    Button { Task { await app.loadOpenTickets() } } label: {
                        MadarIcon("arrow.clockwise", size: 16).foregroundStyle(theme.colors.textSecondary)
                    }.buttonStyle(.plain)
                }
                .padding(.horizontal, Space.lg).padding(.vertical, Space.md)
                .background(theme.colors.surface)
                .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }

                if let error = app.errorMessage {
                    NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .warning).padding(Space.lg)
                }
                ticketList
            }
        }
        .task { await app.loadOpenTickets() }
        .madarSheet(item: $voiding, maxWidth: 460) { ticket, dismiss in
            VoidTicketSheet(app: app, ticket: ticket, onClose: dismiss)
        }
    }

    @ViewBuilder private var ticketList: some View {
        if app.openTickets.isEmpty {
            VStack(spacing: Space.md) {
                Spacer()
                MadarIcon("tray", size: 40).foregroundStyle(theme.colors.textMuted)
                Text(t("waiter.no_tickets")).font(.ui(15, .semibold)).foregroundStyle(theme.colors.textSecondary)
                Spacer()
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ScrollView {
                LazyVStack(spacing: Space.sm) {
                    ForEach(app.openTickets, id: \.id) { ticket in
                        TicketRow(app: app, ticket: ticket, currency: currency,
                                  onAddRound: { startRound(ticket) },
                                  onVoid: { voiding = ticket })
                    }
                }
                .padding(Space.lg)
            }
            .refreshable { await app.loadOpenTickets() }
        }
    }

    /// "Add round": target this ticket, then return to the order screen — its cart
    /// Fire button becomes "Add round" and fires the next round into this ticket.
    private func startRound(_ ticket: TicketView) {
        app.clearCart()
        app.activeTicketId = ticket.id
        onClose()
    }
}

private struct TicketRow: View {
    @ObservedObject var app: AppModel
    let ticket: TicketView
    let currency: String
    let onAddRound: () -> Void
    let onVoid: () -> Void
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    var body: some View {
        VStack(alignment: .leading, spacing: Space.xs) {
            HStack {
                Text(ticket.ticketRef ?? t("waiter.ticket")).font(.ui(15, .heavy)).foregroundStyle(theme.colors.textPrimary)
                StatusBadge(status: ticket.status)
                if ticket.queuedOffline {
                    Text(t("waiter.queued")).font(.ui(11, .bold)).foregroundStyle(theme.colors.warning)
                }
                Spacer()
                Text(Money.format(ticket.subtotalMinor, currency)).font(.ui(15, .bold)).foregroundStyle(theme.colors.textPrimary)
            }
            if let name = ticket.customerName, !name.isEmpty {
                Text(name).font(.ui(13)).foregroundStyle(theme.colors.textSecondary)
            }
            if ticket.status == "open" || ticket.status == "ready" {
                HStack(spacing: Space.sm) {
                    MadarButton(label: t("waiter.add_round"), icon: "plus", variant: .outline, fullWidth: false) { onAddRound() }
                    MadarButton(label: t("common.void"), icon: "xmark", variant: .ghost, fullWidth: false) { onVoid() }
                }
            }
        }
        .padding(Space.md)
        .background(theme.colors.surface)
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .overlay(RoundedRectangle(cornerRadius: 12).stroke(theme.colors.border, lineWidth: 1))
    }
}

private struct StatusBadge: View {
    let status: String
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    var body: some View {
        Text(t("ticket.status.\(status)"))
            .font(.ui(11, .bold))
            .padding(.horizontal, 8).padding(.vertical, 3)
            .background(tone.opacity(0.15)).foregroundStyle(tone)
            .clipShape(Capsule())
    }
    private var tone: Color {
        switch status {
        case "ready": return theme.colors.success
        case "queued": return theme.colors.warning
        case "settled": return theme.colors.textMuted
        default: return theme.colors.accent
        }
    }
}

private struct VoidTicketSheet: View {
    @ObservedObject var app: AppModel
    let ticket: TicketView
    let onClose: () -> Void
    @Environment(\.localize) private var t
    @State private var reason = ""

    var body: some View {
        VStack(alignment: .leading, spacing: Space.md) {
            Text(t("waiter.void_title")).font(.ui(17, .heavy))
            MadarTextField(placeholder: t("waiter.void_reason"), text: $reason)
            HStack {
                MadarButton(label: t("common.cancel"), variant: .ghost, fullWidth: false) { onClose() }
                MadarButton(label: t("common.void"), variant: .danger, fullWidth: false) {
                    Task { await app.voidTicket(ticket.id, reason: reason.isEmpty ? nil : reason); onClose() }
                }
            }
        }
        .padding(Space.lg)
    }
}
