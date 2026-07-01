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
                        MadarIcon("arrow.clockwise", size: IconSize.md).foregroundStyle(theme.colors.textSecondary)
                    }.buttonStyle(.plain)
                }
                .screenHeaderBar()

                if let error = app.errorMessage {
                    NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .warning)
                        .padding(.horizontal, Space.lg).padding(.top, Space.sm)
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
            EmptyState(icon: "tray", title: t("waiter.no_tickets"))
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

/// Open-ticket card on the waiter board — a status-tinted header strip (ref + state
/// + bold-teal total) over a body with the covering customer and inline "Add round"
/// / "Void" actions. Mirrors the Delivery open-order card so the two boards match.
private struct TicketRow: View {
    @ObservedObject var app: AppModel
    let ticket: TicketView
    let currency: String
    let onAddRound: () -> Void
    let onVoid: () -> Void
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    private var isLive: Bool { ticket.status == "open" || ticket.status == "ready" }
    private var customerName: String? {
        guard let name = ticket.customerName, !name.isEmpty else { return nil }
        return name
    }

    var body: some View {
        VStack(spacing: 0) {
            statusStrip
            if customerName != nil || isLive {
                VStack(alignment: .leading, spacing: Space.sm) {
                    // Covering customer — leading person tone-tile + name (mirrors the
                    // Delivery card's customer header).
                    if let name = customerName {
                        HStack(spacing: Space.sm) {
                            MadarIcon("person.fill", size: IconSize.md)
                                .foregroundStyle(theme.colors.accent)
                                .frame(width: 34, height: 34)
                                .background(theme.colors.accentBg)
                                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
                            Text(name).font(Typo.title.font).foregroundStyle(theme.colors.textPrimary)
                            Spacer(minLength: 0)
                        }
                    }
                    if isLive {
                        // Two equal-width actions (matches Kotlin's weight(1f) split).
                        HStack(spacing: Space.sm) {
                            MadarButton(label: t("waiter.add_round"), icon: "plus", variant: .outline) { onAddRound() }
                            MadarButton(label: t("common.void"), icon: "xmark", variant: .ghost) { onVoid() }
                        }
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(Space.md)
            }
        }
        .background(theme.colors.surface)
        .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous).strokeBorder(theme.colors.borderLight, lineWidth: 1))
        .elevation(.card)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
    }

    // Status-tinted header strip — fixed height so every card's body starts at the
    // same y; status dot + bold ref + state lead, money is the hero on the trailing
    // edge in a tinted teal block.
    private var statusStrip: some View {
        let tint = ticketStatusTint(ticket.status, theme.colors)
        return HStack(spacing: Space.sm) {
            Circle().fill(tint.fg).frame(width: 8, height: 8)
            Text(ticket.ticketRef ?? t("waiter.ticket"))
                .font(.ui(19, .heavy)).foregroundStyle(theme.colors.textPrimary).lineLimit(1)
            TicketStatusChip(status: ticket.status)
            if ticket.queuedOffline {
                StatusChip(label: t("waiter.queued"), icon: "tray.and.arrow.up", tone: .warning)
            }
            Spacer()
            Text(Money.format(ticket.subtotalMinor, currency))
                .font(.money(16, .heavy)).foregroundStyle(theme.colors.accent)
                .padding(.horizontal, Space.md).padding(.vertical, 7)
                .background(theme.colors.accentBg)
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        }
        .padding(.horizontal, Space.md)
        .frame(height: 56)
        .frame(maxWidth: .infinity)
        .background(tint.bg)
    }
}

/// Status pill for a ticket — maps the ticket state to a shared `StatusChip` tone
/// (ready → success, queued → warning, settled → neutral, else accent).
struct TicketStatusChip: View {
    let status: String
    @Environment(\.localize) private var t

    private var tone: ChipTone {
        switch status {
        case "ready": return .success
        case "queued": return .warning
        case "settled": return .neutral
        default: return .accent
        }
    }

    var body: some View {
        StatusChip(label: t("ticket.status.\(status)"), tone: tone)
    }
}

/// Ticket status → (foreground, tinted-background) for the card's header strip.
/// Mirrors the Delivery/Kitchen tint pattern so the ticket state reads at a glance.
private func ticketStatusTint(_ status: String, _ c: MadarColors) -> (fg: Color, bg: Color) {
    switch status {
    case "ready": return (c.success, c.successBg)
    case "queued": return (c.warning, c.warningBg)
    case "settled": return (c.textSecondary, c.surfaceAlt)
    default: return (c.accent, c.accentBg)
    }
}

private struct VoidTicketSheet: View {
    @ObservedObject var app: AppModel
    let ticket: TicketView
    let onClose: () -> Void
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    @State private var reason = ""

    var body: some View {
        VStack(alignment: .leading, spacing: Space.md) {
            Text(t("waiter.void_title")).font(Typo.h2.font).foregroundStyle(theme.colors.textPrimary)
            if let ref = ticket.ticketRef {
                Text(ref).font(.ui(13)).foregroundStyle(theme.colors.textSecondary)
            }
            MadarTextField(placeholder: t("waiter.void_reason"), text: $reason, icon: "exclamationmark.bubble")
            // Two equal-width actions (matches Kotlin's weight(1f) split).
            HStack(spacing: Space.sm) {
                MadarButton(label: t("common.cancel"), variant: .ghost) { onClose() }
                MadarButton(label: t("common.void"), icon: "xmark", variant: .danger) { confirmVoid() }
            }
        }
        .padding(Space.lg)
    }

    private func confirmVoid() {
        Task {
            await app.voidTicket(ticket.id, reason: reason.isEmpty ? nil : reason)
            onClose()
        }
    }
}
