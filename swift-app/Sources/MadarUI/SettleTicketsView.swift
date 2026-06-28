// Settle open tickets — the cashier/till side of the waiter flow. Lists the
// branch's open/ready tickets and settles a chosen one into a paid order on the
// CURRENT open shift (the core replays the ticket's frozen lines through the
// order path, so it lands as a normal dine-in sale). All logic is in the core.
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
                        Button { settling = ticket } label: {
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(ticket.ticketRef ?? t("waiter.ticket"))
                                        .font(.ui(15, .heavy)).foregroundStyle(theme.colors.textPrimary)
                                    if let name = ticket.customerName, !name.isEmpty {
                                        Text(name).font(.ui(12)).foregroundStyle(theme.colors.textSecondary)
                                    }
                                }
                                Spacer()
                                Text(Money.format(ticket.subtotalMinor, currency))
                                    .font(.ui(15, .bold)).foregroundStyle(theme.colors.textPrimary)
                                MadarIcon("chevron.forward", size: 14).foregroundStyle(theme.colors.textMuted)
                            }
                            .padding(Space.md)
                            .background(theme.colors.surface)
                            .clipShape(RoundedRectangle(cornerRadius: 12))
                            .overlay(RoundedRectangle(cornerRadius: 12).stroke(theme.colors.border, lineWidth: 1))
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(Space.lg)
            }
            .refreshable { await app.loadOpenTickets() }
        }
    }
}

private struct SettleSheet: View {
    @ObservedObject var app: AppModel
    let ticket: TicketView
    let onClose: () -> Void
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    @State private var methodId: String?
    @State private var tipMinor: Int64 = 0
    @State private var tenderedMinor: Int64 = 0

    private var currency: String { app.session?.currencyCode ?? "" }
    private var isCash: Bool { app.paymentMethods.first(where: { $0.id == methodId })?.isCash ?? false }

    var body: some View {
        VStack(alignment: .leading, spacing: Space.md) {
            Text(ticket.ticketRef ?? t("waiter.ticket")).font(.ui(18, .heavy)).foregroundStyle(theme.colors.textPrimary)

            ScrollView {
                VStack(spacing: Space.xs) {
                    ForEach(ticket.lines.indices, id: \.self) { i in
                        let line = ticket.lines[i]
                        HStack {
                            Text("\(line.qty)× \(line.name)").font(.ui(13)).foregroundStyle(theme.colors.textSecondary)
                                .strikethrough(line.voided)
                            Spacer()
                            Text(Money.format(line.lineTotalMinor, currency)).font(.ui(13, .semibold)).foregroundStyle(theme.colors.textPrimary)
                        }
                    }
                }
            }
            .frame(maxHeight: 220)

            HStack {
                Text(t("tender.total")).font(.ui(14, .semibold)).foregroundStyle(theme.colors.textSecondary)
                Spacer()
                Text(Money.format(ticket.subtotalMinor, currency)).font(.ui(17, .heavy)).foregroundStyle(theme.colors.textPrimary)
            }

            Text(t("tender.method")).font(.ui(13, .semibold)).foregroundStyle(theme.colors.textSecondary)
            FlowLayout(spacing: Space.sm) {
                ForEach(app.paymentMethods, id: \.id) { pm in
                    Button { methodId = pm.id } label: {
                        Text(pm.name)
                            .font(.ui(14, .semibold))
                            .padding(.horizontal, Space.md).padding(.vertical, Space.sm)
                            .background((methodId == pm.id ? theme.colors.accent : theme.colors.surfaceAlt))
                            .foregroundStyle(methodId == pm.id ? theme.colors.textOnAccent : theme.colors.textPrimary)
                            .clipShape(Capsule())
                    }.buttonStyle(.plain)
                }
            }

            // Optional tip.
            Text(t("order.tip")).font(.ui(13, .semibold)).foregroundStyle(theme.colors.textSecondary)
            AmountField(amountMinor: $tipMinor, currencyCode: currency)
            // Cash: amount tendered → change due.
            if isCash {
                Text(t("order.cash_received")).font(.ui(13, .semibold)).foregroundStyle(theme.colors.textSecondary)
                AmountField(amountMinor: $tenderedMinor, currencyCode: currency)
                if tenderedMinor > 0 {
                    HStack {
                        Text(t("order.change_due")).font(.ui(15, .semibold)).foregroundStyle(theme.colors.textSecondary)
                        Spacer()
                        Text(Money.format(max(0, tenderedMinor - (ticket.subtotalMinor + tipMinor)), currency))
                            .font(.money(20, .heavy)).foregroundStyle(theme.colors.accent)
                    }
                }
            }

            MadarButton(label: t("waiter.settle"), icon: "checkmark.circle", loading: app.isBusy) {
                guard let id = methodId, !app.isBusy else { return }
                Task {
                    let ok = await app.settleTicket(
                        ticket.id, paymentMethodId: id,
                        amountTenderedMinor: isCash && tenderedMinor > 0 ? tenderedMinor : nil,
                        tipMinor: tipMinor, tipPaymentMethodId: tipMinor > 0 ? id : nil)
                    if ok { onClose() }
                }
            }
            .opacity(methodId == nil ? 0.5 : 1)
        }
        .padding(Space.lg)
        .onAppear { methodId = app.paymentMethods.first?.id }
    }
}
