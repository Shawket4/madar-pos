// Settle open tickets — the cashier/till side of the waiter flow. Lists the
// branch's open/ready tickets and settles a chosen one into a paid order on the
// CURRENT open shift (the core replays the ticket's frozen lines through the
// order path, so it lands as a normal dine-in sale). All logic is in the core.
//
// The settle sheet is a TWO-STEP flow over ONE ticket: first the real order
// details (`TicketDetailsView` — the frozen lines + money + covers), then the
// SAME shared checkout drawer (`CheckoutDrawer`) the main cashier uses — no more
// mirrored settle UI. The drawer's terminal action settles the ticket via
// `app.settleTicket`.
import SwiftUI

// Settle open tickets body — the "Open tickets" tab of the unified Orders surface.
// No nav header of its own (IncomingView owns back + title + the tab bar). Live
// via the shared `app.openTickets` (onRealtimeEvent → loadOpenTickets refreshes
// it, so a waiter's fire/round from another device appears here instantly).
struct SettleBody: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    @State private var settling: TicketView?

    private var currency: String { app.session?.currencyCode ?? "" }
    private var settleable: [TicketView] {
        app.openTickets.filter { $0.status == "open" || $0.status == "ready" }
    }

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            VStack(spacing: 0) {
                if let error = app.errorMessage {
                    NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .warning).padding(Space.lg)
                }
                content
            }
        }
        .task { await app.loadOpenTickets() }
        // The full-height settle drawer: details → shared CheckoutDrawer.
        .madarSheet(item: $settling, size: .large, maxWidth: 560) { ticket, dismiss in
            SettleSheet(app: app, ticket: ticket, onClose: dismiss)
        }
    }

    @ViewBuilder private var content: some View {
        if settleable.isEmpty {
            VStack(spacing: Space.md) {
                Spacer()
                MadarIcon("tray", size: 40).foregroundStyle(theme.colors.textMuted)
                Text(t("waiter.no_tickets")).font(.ui(15, .semibold)).foregroundStyle(theme.colors.textSecondary)
                Spacer()
            }
        } else {
            ScrollView {
                LazyVStack(spacing: Space.sm) {
                    ForEach(settleable) { ticket in
                        SettleTicketCard(app: app, ticket: ticket, currency: currency) { settling = ticket }
                    }
                }
                .frame(maxWidth: 620).frame(maxWidth: .infinity).padding(Space.lg)
            }
            .refreshable { await app.loadOpenTickets() }
        }
    }
}

// MARK: - Ticket card (settle side)

/// A settleable ticket on the till board — the SAME card language as the waiter
/// board and delivery queue: a status-tinted header strip (ref + state + bold-teal
/// total) over a body with the covering customer + a "View & settle" action that
/// opens the details→checkout sheet. (P3: consistent look across both Orders tabs.)
private struct SettleTicketCard: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let ticket: TicketView
    let currency: String
    let onSettle: () -> Void

    private var customerName: String? {
        guard let name = ticket.customerName, !name.isEmpty else { return nil }
        return name
    }
    private var lineCount: Int { ticket.lines.filter { !$0.voided }.count }

    var body: some View {
        VStack(spacing: 0) {
            statusStrip
            VStack(alignment: .leading, spacing: Space.sm) {
                if let name = customerName {
                    HStack(spacing: Space.sm) {
                        MadarIcon("person.fill", size: IconSize.md)
                            .foregroundStyle(theme.colors.accent)
                            .frame(width: 34, height: 34)
                            .background(theme.colors.accentBg)
                            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
                        Text(name).font(.ui(16, .bold)).foregroundStyle(theme.colors.textPrimary)
                        Spacer(minLength: 0)
                    }
                }
                // The waiter who opened the ticket — so the teller sees who took it.
                if let waiter = ticket.waiterName, !waiter.isEmpty {
                    Text("\(t("order.waiter")): \(waiter)")
                        .font(.ui(11, .semibold)).foregroundStyle(theme.colors.textSecondary)
                }
                // Item-count meta so the card previews contents at a glance.
                Text("\(lineCount) \(t("order.items"))")
                    .font(.ui(11, .semibold)).foregroundStyle(theme.colors.textMuted)
                MadarButton(label: t("waiter.settle"), icon: "arrow.right.circle") { onSettle() }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(Space.md)
        }
        .background(theme.colors.surface)
        .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous).strokeBorder(theme.colors.borderLight, lineWidth: 1))
        .elevation(.card)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
    }

    // Status-tinted header strip — mirrors the waiter board's ticket card so the
    // two boards read identically.
    private var statusStrip: some View {
        let tint = settleStatusTint(ticket.status, theme.colors)
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

/// Ticket status → (foreground, tinted-background) for the settle card's header
/// strip (mirrors the waiter board's tint).
private func settleStatusTint(_ status: String, _ c: MadarColors) -> (fg: Color, bg: Color) {
    switch status {
    case "ready": return (c.success, c.successBg)
    case "queued": return (c.warning, c.warningBg)
    case "settled": return (c.textSecondary, c.surfaceAlt)
    default: return (c.accent, c.accentBg)
    }
}

// MARK: - Settle sheet (details → shared checkout drawer)

/// The two-step settle sheet: STEP 1 shows the real order details (frozen lines +
/// money + covers) with a "Settle" button; STEP 2 hands off to the SHARED
/// `CheckoutDrawer`, whose terminal action settles the ticket into a paid order.
private struct SettleSheet: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let ticket: TicketView
    let onClose: () -> Void

    private enum Step { case details, checkout }
    @State private var step: Step = .details

    private var currency: String { app.session?.currencyCode ?? "" }

    var body: some View {
        switch step {
        case .details:
            detailsStep
        case .checkout:
            // The SAME drawer the main cashier uses. `.flat` summary (the ticket
            // carries just a subtotal); the terminal settles the ticket. No cart
            // discount edit and no customer capture (the ticket already knows its
            // covering customer).
            CheckoutDrawer(
                app: app,
                title: ticket.ticketRef ?? t("waiter.ticket"),
                total: ticket.subtotalMinor,
                currency: currency,
                busy: app.isBusy,
                terminalLabel: t("waiter.settle"),
                terminalIcon: "checkmark.circle",
                errorMessage: app.errorMessage,
                summary: .flat,
                showCartDiscount: false,
                showCustomerCapture: false,
                onClose: onClose,
                onTerminal: { input in
                    let ok = await app.settleTicket(
                        ticket.id,
                        paymentMethodId: input.paymentMethodId,
                        amountTenderedMinor: input.isCash && input.amountTenderedMinor > 0 ? input.amountTenderedMinor : nil,
                        tipMinor: input.tipMinor,
                        tipPaymentMethodId: input.tipMinor > 0 ? (input.tipPaymentMethodId ?? input.paymentMethodId) : nil)
                    if ok { onClose() }
                })
        }
    }

    private var detailsStep: some View {
        VStack(spacing: 0) {
            ScrollView {
                TicketDetailsView(ticket: ticket, currency: currency)
                    .frame(maxWidth: 552).frame(maxWidth: .infinity)
                    .padding(.horizontal, Space.xl)
                    .padding(.top, Space.md)
                    .padding(.bottom, Space.lg)
            }
            // Advance to the shared checkout drawer.
            VStack(spacing: Space.sm) {
                MadarButton(label: t("waiter.settle"), icon: "arrow.right.circle") {
                    withAnimation(Motion.standard) { step = .checkout }
                }
            }
            .padding(Space.lg)
            .background(theme.colors.surface)
            .overlay(alignment: .top) { Rectangle().fill(theme.colors.border).frame(height: 1) }
        }
    }
}
