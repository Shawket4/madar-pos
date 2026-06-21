// Cash In/Out + Past Shifts — two manager screens reached from the action bar /
// More drawer, presented as full routed screens (not cramped sheets). Cash
// movements record a signed pay-in / pay-out against the open shift —
// OFFLINE-FIRST (queued through the durable outbox, idempotent on a client_ref) —
// and show a running in/out/net summary. Past Shifts lists the branch's shift
// history as a table (wide) / cards (narrow); each row expands to that shift's
// orders and can reprint the Z-report. All data + rules live in the core.
import SwiftUI

struct CashMovementsView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let onClose: () -> Void

    @State private var isIn = true
    @State private var amountMinor: Int64 = 0
    @State private var note = ""

    private var currency: String { app.session?.currencyCode ?? "" }
    private var canRecord: Bool { amountMinor > 0 && !app.isBusy }

    private var totalIn: Int64 { app.cashMovements.filter { $0.amountMinor > 0 }.reduce(0) { $0 + $1.amountMinor } }
    private var totalOut: Int64 { app.cashMovements.filter { $0.amountMinor < 0 }.reduce(0) { $0 - $1.amountMinor } }
    private var net: Int64 { totalIn - totalOut }

    var body: some View {
        VStack(spacing: 0) {
            ScreenHeader(title: t("cash.title"), onClose: onClose)
            ScrollView {
                VStack(alignment: .leading, spacing: Space.lg) {
                    if let error = app.errorMessage {
                        NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .danger)
                    }
                    if !app.cashMovements.isEmpty { summaryStrip }
                    recordCard
                    movementsList
                }
                .frame(maxWidth: 560)
                .frame(maxWidth: .infinity)
                .padding(Space.lg)
            }
        }
        .background(theme.colors.bg.ignoresSafeArea())
        .task { await app.loadCashMovements() }
    }

    /// Total in / out / net for the open shift.
    private var summaryStrip: some View {
        HStack(spacing: Space.sm) {
            stat(t("cash.total_in"), Money.format(totalIn, currency), tone: theme.colors.success, icon: "arrow.down.circle.fill")
            stat(t("cash.total_out"), Money.format(totalOut, currency), tone: theme.colors.danger, icon: "arrow.up.circle.fill")
            stat(t("cash.net"), (net < 0 ? "−" : "") + Money.format(abs(net), currency),
                 tone: net < 0 ? theme.colors.danger : theme.colors.textPrimary, icon: "equal.circle.fill")
        }
    }

    private func stat(_ label: String, _ value: String, tone: Color, icon: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 5) {
                Image(systemName: icon).font(.system(size: 12)).foregroundStyle(tone)
                Text(label).font(.ui(11, .semibold)).foregroundStyle(theme.colors.textMuted).textCase(.uppercase)
            }
            Text(value).font(.money(16, .heavy)).foregroundStyle(tone).lineLimit(1).minimumScaleFactor(0.7)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(Space.md)
        .background(theme.colors.surface)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous).strokeBorder(theme.colors.border, lineWidth: 1))
    }

    private var recordCard: some View {
        VStack(alignment: .leading, spacing: Space.md) {
            HStack(spacing: Space.sm) {
                directionChip(t("cash.in"), active: isIn, tone: theme.colors.success) { isIn = true }
                directionChip(t("cash.out"), active: !isIn, tone: theme.colors.danger) { isIn = false }
            }
            AmountField(amountMinor: $amountMinor, currencyCode: currency)
            SufrixTextField(placeholder: t("cash.note"), text: $note, icon: "text.bubble")
            SufrixButton(label: t("cash.record"), icon: "plus.forwardslash.minus", loading: app.isBusy) {
                Task {
                    let signed = isIn ? amountMinor : -amountMinor
                    if await app.recordCashMovement(amountMinor: signed, note: note) {
                        amountMinor = 0; note = ""
                    }
                }
            }
            .opacity(canRecord ? 1 : 0.5).allowsHitTesting(canRecord)
        }
        .padding(Space.lg)
        .background(theme.colors.surface)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
            .strokeBorder(theme.colors.border, lineWidth: 1))
    }

    private func directionChip(_ label: String, active: Bool, tone: Color, action: @escaping () -> Void) -> some View {
        Button { Haptics.selection(); action() } label: {
            Text(label).font(.ui(13, .bold))
                .foregroundStyle(active ? theme.colors.textOnAccent : theme.colors.textSecondary)
                .frame(maxWidth: .infinity).padding(.vertical, 10)
                .background(active ? tone : theme.colors.surfaceAlt)
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        }
        .buttonStyle(.pressable(scale: 0.97))
    }

    @ViewBuilder private var movementsList: some View {
        sectionTitle(t("cash.history"))
        if app.cashMovements.isEmpty {
            Text(t("cash.empty")).font(.ui(13)).foregroundStyle(theme.colors.textMuted)
                .frame(maxWidth: .infinity, alignment: .center).padding(.vertical, Space.lg)
        } else {
            VStack(spacing: Space.sm) {
                ForEach(app.cashMovements, id: \.id) { m in movementRow(m) }
            }
        }
    }

    private func movementRow(_ m: CashMovementView) -> some View {
        let positive = m.amountMinor >= 0
        return HStack(spacing: Space.md) {
            Image(systemName: positive ? "arrow.down.circle.fill" : "arrow.up.circle.fill")
                .font(.system(size: 20)).foregroundStyle(positive ? theme.colors.success : theme.colors.danger)
            VStack(alignment: .leading, spacing: 2) {
                Text(m.note.isEmpty ? m.movedByName : m.note)
                    .font(.ui(13, .semibold)).foregroundStyle(theme.colors.textPrimary).lineLimit(1)
                Text(m.movedByName).font(.ui(11)).foregroundStyle(theme.colors.textMuted)
            }
            Spacer(minLength: Space.sm)
            Text("\(positive ? "+" : "−")\(Money.format(abs(m.amountMinor), currency))")
                .font(.money(14, .bold)).foregroundStyle(positive ? theme.colors.success : theme.colors.danger)
        }
        .padding(Space.md)
        .background(theme.colors.surface)
        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
            .strokeBorder(theme.colors.border, lineWidth: 1))
    }

    private func sectionTitle(_ s: String) -> some View {
        Text(s).font(.ui(12, .semibold)).foregroundStyle(theme.colors.textMuted).textCase(.uppercase)
    }
}

// MARK: - Past shifts

struct ShiftHistoryView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let onClose: () -> Void

    @State private var expandedId: String?

    private var currency: String { app.session?.currencyCode ?? "" }

    var body: some View {
        GeometryReader { geo in
            let wide = geo.size.width >= 720
            VStack(spacing: 0) {
                ScreenHeader(title: t("shifts.title"), onClose: onClose)
                if app.shiftHistory.isEmpty {
                    emptyState
                } else {
                    ScrollView {
                        VStack(spacing: wide ? 0 : Space.sm) {
                            if wide { columnHeader }
                            ForEach(app.shiftHistory, id: \.id) { s in
                                ShiftRow(app: app, shift: s, currency: currency, wide: wide,
                                         expanded: expandedId == s.id) {
                                    withAnimation(Motion.standard) {
                                        expandedId = expandedId == s.id ? nil : s.id
                                    }
                                }
                            }
                        }
                        .frame(maxWidth: 880).frame(maxWidth: .infinity).padding(Space.lg)
                    }
                }
            }
        }
        .background(theme.colors.bg.ignoresSafeArea())
        .task { await app.loadShiftHistory() }
        .task(id: expandedId) {
            if let id = expandedId { await app.loadOrdersForShift(id) }
        }
    }

    private var emptyState: some View {
        VStack(spacing: Space.md) {
            Image(systemName: "clock.arrow.circlepath").font(.system(size: 36, weight: .light))
                .foregroundStyle(theme.colors.textMuted)
            Text(t("shifts.empty")).font(.ui(14)).foregroundStyle(theme.colors.textSecondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var columnHeader: some View {
        HStack(spacing: Space.md) {
            Text(t("shift.opened_at")).frame(maxWidth: .infinity, alignment: .leading)
            Text(t("shift.teller")).frame(width: 120, alignment: .leading)
            Text(t("shifts.opening")).frame(width: 100, alignment: .trailing)
            Text(t("shifts.declared")).frame(width: 100, alignment: .trailing)
            Text(t("shifts.discrepancy")).frame(width: 100, alignment: .trailing)
            Spacer().frame(width: 24)
        }
        .font(.ui(11, .semibold)).foregroundStyle(theme.colors.textMuted).textCase(.uppercase)
        .padding(.horizontal, Space.md).padding(.vertical, Space.sm)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }
}

/// A shift row that renders as a table row (wide) or a card (narrow) and expands
/// to show that shift's orders + a reprint-report action.
private struct ShiftRow: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let shift: ShiftSummaryView
    let currency: String
    let wide: Bool
    let expanded: Bool
    let onToggle: () -> Void

    @State private var printing = false

    private var discrepancyColor: Color {
        guard let d = shift.discrepancyMinor, d != 0 else { return theme.colors.textSecondary }
        return theme.colors.danger
    }

    var body: some View {
        VStack(spacing: 0) {
            Button { Haptics.selection(); onToggle() } label: {
                if wide { tableRow } else { cardRow }
            }
            .buttonStyle(.plain)
            if expanded {
                expansion
                    .padding(.horizontal, wide ? Space.md : Space.md)
                    .padding(.bottom, Space.md)
            }
        }
        .background(theme.colors.surface)
        .overlay(
            RoundedRectangle(cornerRadius: wide ? 0 : Radii.md, style: .continuous)
                .strokeBorder(theme.colors.border, lineWidth: wide ? 0 : 1))
        .overlay(alignment: .bottom) { if wide { Rectangle().fill(theme.colors.borderLight).frame(height: 1) } }
        .clipShape(RoundedRectangle(cornerRadius: wide ? 0 : Radii.md, style: .continuous))
    }

    // Wide: a single table row.
    private var tableRow: some View {
        HStack(spacing: Space.md) {
            Text(Self.shortDate(shift.openedAt)).font(.ui(13, .semibold)).foregroundStyle(theme.colors.textPrimary)
                .frame(maxWidth: .infinity, alignment: .leading)
            statusChip.frame(width: 120, alignment: .leading)
            money(shift.openingCashMinor).frame(width: 100, alignment: .trailing)
            money(shift.closingDeclaredMinor).frame(width: 100, alignment: .trailing)
            Text(shift.discrepancyMinor.map { ($0 > 0 ? "+" : ($0 < 0 ? "−" : "")) + Money.format(abs($0), currency) } ?? "—")
                .font(.money(13, .semibold)).foregroundStyle(discrepancyColor)
                .frame(width: 100, alignment: .trailing)
            Image(systemName: expanded ? "chevron.down" : "chevron.right")
                .font(.system(size: 12, weight: .semibold)).foregroundStyle(theme.colors.textMuted)
                .frame(width: 24)
        }
        .padding(.horizontal, Space.md).padding(.vertical, Space.md)
    }

    // Narrow: a card.
    private var cardRow: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            HStack {
                Text(Self.shortDate(shift.openedAt)).font(.ui(14, .bold)).foregroundStyle(theme.colors.textPrimary)
                Spacer()
                statusChip
                Image(systemName: expanded ? "chevron.down" : "chevron.right")
                    .font(.system(size: 12, weight: .semibold)).foregroundStyle(theme.colors.textMuted)
            }
            metric(t("shifts.opening"), Money.format(shift.openingCashMinor, currency))
            if let declared = shift.closingDeclaredMinor {
                metric(t("shifts.declared"), Money.format(declared, currency))
            }
            if let disc = shift.discrepancyMinor, disc != 0 {
                metric(t("shifts.discrepancy"),
                       "\(disc > 0 ? "+" : "−")\(Money.format(abs(disc), currency))", valueColor: theme.colors.danger)
            }
        }
        .padding(Space.lg)
    }

    // Expansion: that shift's orders + reprint.
    @ViewBuilder private var expansion: some View {
        Rectangle().fill(theme.colors.border).frame(height: 1).padding(.bottom, Space.sm)
        HStack {
            Text(t("shifts.orders")).font(.ui(12, .semibold)).foregroundStyle(theme.colors.textMuted).textCase(.uppercase)
            Spacer()
            Button {
                printing = true
                Task { await app.reprintShiftReport(shift.id); printing = false }
            } label: {
                HStack(spacing: 5) {
                    if printing { ProgressView().controlSize(.small) }
                    else { Image(systemName: "printer") }
                    Text(t("shift.print_report"))
                }
                .font(.ui(12, .semibold)).foregroundStyle(theme.colors.accent)
            }
            .buttonStyle(.pressable)
            .disabled(printing)
        }
        if app.loadingShiftOrders.contains(shift.id) {
            ProgressView().controlSize(.small).frame(maxWidth: .infinity).padding(.vertical, Space.sm)
        } else if let orders = app.shiftOrders[shift.id], !orders.isEmpty {
            VStack(spacing: 4) {
                ForEach(orders, id: \.id) { o in orderRow(o) }
            }
            .padding(.top, Space.xs)
        } else {
            Text(t("shifts.no_orders")).font(.ui(12)).foregroundStyle(theme.colors.textMuted)
                .frame(maxWidth: .infinity, alignment: .leading).padding(.vertical, Space.sm)
        }
    }

    private func orderRow(_ o: OrderSummaryView) -> some View {
        HStack(spacing: Space.sm) {
            Text(o.orderNumber.map { "#\($0)" } ?? t("history.order"))
                .font(.ui(12, .semibold)).foregroundStyle(theme.colors.textPrimary)
            Text(OrderHistoryView.timeOf(o.createdAt)).font(.ui(11)).foregroundStyle(theme.colors.textMuted)
            if o.status == "voided" { StatusChip(label: t("history.voided"), tone: .danger) }
            Spacer(minLength: Space.sm)
            Text(o.paymentLabel).font(.ui(11)).foregroundStyle(theme.colors.textMuted)
            Text(Money.format(o.totalMinor, currency)).font(.money(12, .bold)).foregroundStyle(theme.colors.textPrimary)
        }
        .padding(.vertical, 5).padding(.horizontal, Space.sm)
        .background(theme.colors.surfaceAlt)
        .clipShape(RoundedRectangle(cornerRadius: Radii.xs, style: .continuous))
    }

    private var statusChip: some View {
        StatusChip(label: shift.isOpen ? t("shifts.open_now") : t("shifts.closed"),
                   tone: shift.isOpen ? .success : .neutral)
    }

    private func money(_ m: Int64?) -> some View {
        Text(m.map { Money.format($0, currency) } ?? "—")
            .font(.money(13, .semibold)).foregroundStyle(theme.colors.textPrimary)
    }

    private func metric(_ label: String, _ value: String, valueColor: Color? = nil) -> some View {
        HStack {
            Text(label).font(.ui(12, .medium)).foregroundStyle(theme.colors.textSecondary)
            Spacer()
            Text(value).font(.money(13, .semibold)).foregroundStyle(valueColor ?? theme.colors.textPrimary)
        }
    }

    /// Trim an RFC3339 timestamp to "YYYY-MM-DD HH:MM".
    static func shortDate(_ rfc: String) -> String {
        String(rfc.replacingOccurrences(of: "T", with: " ").prefix(16))
    }
}

/// A back-chevron header shared by the cash + shifts screens.
private struct ScreenHeader: View {
    @Environment(\.theme) private var theme
    let title: String
    let onClose: () -> Void

    var body: some View {
        HStack(spacing: Space.md) {
            Button { onClose() } label: {
                Image(systemName: "chevron.backward").font(.system(size: 17, weight: .semibold))
                    .foregroundStyle(theme.colors.textPrimary)
            }
            .buttonStyle(.pressable)
            Text(title).font(.ui(17, .heavy)).foregroundStyle(theme.colors.textPrimary)
            Spacer()
        }
        .padding(.horizontal, Space.lg).padding(.vertical, Space.md)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }
}
