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
            ScreenHeader(t("cash.title"), onBack: onClose).screenHeaderBar()
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

    /// Total in / out / net for the open shift — one card, three columns
    /// (matches Flutter `_SummaryStrip`: a single `SurfaceCard` with a `Row`).
    private var summaryStrip: some View {
        HStack(spacing: Space.sm) {
            stat(t("cash.total_in"), "+ " + Money.format(totalIn, currency), tone: theme.colors.success)
            stat(t("cash.total_out"), "− " + Money.format(totalOut, currency), tone: theme.colors.danger)
            stat(t("cash.net"), (net < 0 ? "−" : "") + Money.format(abs(net), currency),
                 tone: net < 0 ? theme.colors.danger : theme.colors.textPrimary)
        }
        .padding(Space.lg)
        .background(theme.colors.surface)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous).strokeBorder(theme.colors.border, lineWidth: 1))
    }

    private func stat(_ label: String, _ value: String, tone: Color) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(label).font(.ui(12)).foregroundStyle(theme.colors.textSecondary)
            Text(value).font(.money(16, .bold)).foregroundStyle(tone).lineLimit(1).minimumScaleFactor(0.7)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var recordCard: some View {
        VStack(alignment: .leading, spacing: Space.md) {
            HStack(spacing: Space.sm) {
                directionChip(t("cash.in"), active: isIn, tone: theme.colors.success) { isIn = true }
                directionChip(t("cash.out"), active: !isIn, tone: theme.colors.danger) { isIn = false }
            }
            AmountField(amountMinor: $amountMinor, currencyCode: currency)
            MadarTextField(placeholder: t("cash.note"), text: $note, icon: "text.bubble")
            MadarButton(label: t("cash.record"), icon: "plus.forwardslash.minus", loading: app.isBusy) {
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
            // One card, rows separated by hairlines (matches Flutter's single
            // `SurfaceCard(radius: AppRadius.lg)` with `Divider` between rows).
            VStack(spacing: 0) {
                ForEach(Array(app.cashMovements.enumerated()), id: \.element.id) { index, m in
                    if index > 0 { Rectangle().fill(theme.colors.borderLight).frame(height: 1) }
                    movementRow(m)
                }
            }
            .background(theme.colors.surface)
            .clipShape(RoundedRectangle(cornerRadius: Radii.lg, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: Radii.lg, style: .continuous)
                .strokeBorder(theme.colors.border, lineWidth: 1))
        }
    }

    private func movementRow(_ m: CashMovementView) -> some View {
        let positive = m.amountMinor >= 0
        let tone = positive ? theme.colors.success : theme.colors.danger
        let toneBg = positive ? theme.colors.successBg : theme.colors.dangerBg
        return HStack(spacing: Space.md) {
            ZStack {
                Circle().fill(toneBg).frame(width: 38, height: 38)
                MadarIcon(positive ? "arrow.down.left" : "arrow.up.right", size: 18).foregroundStyle(tone)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(m.note.isEmpty ? (positive ? t("cash.in") : t("cash.out")) : m.note)
                    .font(.ui(14, .semibold)).foregroundStyle(theme.colors.textPrimary).lineLimit(1)
                Text(m.movedByName).font(.ui(12)).foregroundStyle(theme.colors.textSecondary).lineLimit(1)
            }
            Spacer(minLength: Space.sm)
            Text("\(positive ? "+" : "−") \(Money.format(abs(m.amountMinor), currency))")
                .font(.money(14, .bold)).foregroundStyle(tone)
        }
        .padding(.horizontal, Space.lg).padding(.vertical, Space.md)
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

    /// Flutter's `_withLocalOpenShift`: prepend the locally-opened-but-unsynced
    /// shift (`app.shift`) to the top of the page if it isn't already present, so
    /// the live shift always shows. `app.shift` is a `ShiftView`, so we project it
    /// onto a `ShiftSummaryView` for the table.
    private var shifts: [ShiftSummaryView] {
        let page = app.shiftHistory
        guard let live = app.shift, live.isOpen,
              !page.contains(where: { $0.id == live.id }) else { return page }
        let pinned = ShiftSummaryView(
            id: live.id,
            branchName: nil,
            tellerName: live.tellerName,
            openedAt: live.openedAt,
            closedAt: nil,
            openingCashMinor: live.openingCashMinor,
            closingDeclaredMinor: nil,
            closingSystemMinor: nil,
            discrepancyMinor: nil,
            status: live.status,
            isOpen: live.isOpen
        )
        return [pinned] + page
    }

    var body: some View {
        GeometryReader { geo in
            // Width-driven, matching Flutter's `compact = maxWidth < 680`.
            let wide = geo.size.width >= Responsive.wideTable
            VStack(spacing: 0) {
                ScreenHeader(t("shifts.title"), onBack: onClose).screenHeaderBar()
                if shifts.isEmpty {
                    emptyState
                } else {
                    ScrollView {
                        let rows = VStack(spacing: wide ? 0 : Space.sm) {
                            if wide { columnHeader }
                            ForEach(Array(shifts.enumerated()), id: \.element.id) { index, s in
                                ShiftRow(app: app, shift: s, currency: currency, wide: wide,
                                         odd: index.isMultiple(of: 2) == false,
                                         expanded: expandedId == s.id) {
                                    withAnimation(Motion.standard) {
                                        expandedId = expandedId == s.id ? nil : s.id
                                    }
                                }
                            }
                        }
                        // Wide: header + rows live in one card (Flutter's single
                        // `SurfaceCard(radius: AppRadius.lg)`); narrow keeps per-row cards.
                        Group {
                            if wide {
                                rows
                                    .background(theme.colors.surface)
                                    .clipShape(RoundedRectangle(cornerRadius: Radii.lg, style: .continuous))
                                    .overlay(RoundedRectangle(cornerRadius: Radii.lg, style: .continuous)
                                        .strokeBorder(theme.colors.border, lineWidth: 1))
                            } else {
                                rows
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
            MadarIcon("clock.arrow.circlepath", size: 36)
                .foregroundStyle(theme.colors.textMuted)
            Text(t("shifts.empty")).font(.ui(14)).foregroundStyle(theme.colors.textSecondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    // Flutter `_Cols`: [status dot 26][Teller flex2][Opened flex2][Closed flex2]
    // [Declared 110 trailing][chevron 44]. The header omits the status-dot label
    // (blank in Flutter) and end-aligns Declared.
    private var columnHeader: some View {
        HStack(spacing: Space.md) {
            Spacer().frame(width: ShiftRow.statusW)
            Text(t("shift.teller")).frame(maxWidth: .infinity, alignment: .leading)
            Text(t("shift.opened_at")).frame(maxWidth: .infinity, alignment: .leading)
            Text(t("shifts.closed")).frame(maxWidth: .infinity, alignment: .leading)
            Text(t("shifts.declared")).frame(width: ShiftRow.declaredW, alignment: .trailing)
            Spacer().frame(width: ShiftRow.chevW)
        }
        // Flutter `_TableHeader`: 42-pt tall, AppSpace.lg horizontal, surfaceAlt fill.
        .font(.ui(11, .semibold)).foregroundStyle(theme.colors.textMuted).textCase(.uppercase)
        .frame(height: 42)
        .padding(.horizontal, Space.lg)
        .background(theme.colors.surfaceAlt)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.borderLight).frame(height: 1) }
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
    /// Even/odd index, for the wide-table zebra striping (Flutter: even rows
    /// transparent, odd rows `surfaceAlt`).
    let odd: Bool
    let expanded: Bool
    let onToggle: () -> Void

    @State private var printing = false

    /// Wide rows alternate surface / surfaceAlt; expanded uses surfaceAlt as the
    /// hover overlay. Narrow cards keep their solid surface.
    private var rowBackground: Color {
        guard wide else { return theme.colors.surface }
        if expanded { return theme.colors.surfaceAlt }
        return odd ? theme.colors.surfaceAlt : theme.colors.surface
    }

    // Fixed column widths (Flutter `_Cols`: statusW = 10+16, declaredW, chevW).
    static let statusW: CGFloat = 26
    static let declaredW: CGFloat = 110
    static let chevW: CGFloat = 44

    /// Status-dot color (Flutter `_statusColor`): open→success, force_closed→danger,
    /// closed/other→muted.
    private var statusColor: Color {
        switch shift.status {
        case "open": return theme.colors.success
        case "force_closed": return theme.colors.danger
        default: return theme.colors.textMuted
        }
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
        .background(rowBackground)
        .overlay(
            RoundedRectangle(cornerRadius: wide ? 0 : Radii.md, style: .continuous)
                .strokeBorder(theme.colors.border, lineWidth: wide ? 0 : 1))
        .overlay(alignment: .bottom) { if wide { Rectangle().fill(theme.colors.borderLight).frame(height: 1) } }
        .clipShape(RoundedRectangle(cornerRadius: wide ? 0 : Radii.md, style: .continuous))
    }

    // Wide: a single table row — columns mirror Flutter `_Cols`:
    // [status dot 26][Teller flex2][Opened flex2][Closed flex2][Declared 110 →][chevron 44].
    // Row height 56 (Flutter `_kRowHeight`).
    private var tableRow: some View {
        HStack(spacing: Space.md) {
            // Status: an 8pt colored dot (NOT a full chip).
            Circle().fill(statusColor).frame(width: 8, height: 8)
                .frame(width: Self.statusW, alignment: .center)
            // Teller: the real teller name (NEW field), not a status chip.
            Text(shift.tellerName ?? "—")
                .font(.ui(14, .semibold)).foregroundStyle(theme.colors.textPrimary).lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
            // Opened.
            Text(app.fmtDateTime(shift.openedAt))
                .font(.ui(13)).foregroundStyle(theme.colors.textSecondary).lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
            // Closed — discrete column; "—" when still open.
            Text(shift.closedAt.map(app.fmtDateTime) ?? "—")
                .font(.ui(13)).foregroundStyle(shift.closedAt == nil ? theme.colors.textMuted : theme.colors.textSecondary).lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
            // Declared cash — right-aligned, muted when nil.
            Text(shift.closingDeclaredMinor.map { Money.format($0, currency) } ?? "—")
                .font(.money(14, .semibold))
                .foregroundStyle(shift.closingDeclaredMinor == nil ? theme.colors.textMuted : theme.colors.textPrimary)
                .lineLimit(1).minimumScaleFactor(0.8)
                .frame(width: Self.declaredW, alignment: .trailing)
            MadarIcon(expanded ? "chevron.down" : "chevron.right", size: 12).foregroundStyle(theme.colors.textMuted)
                .frame(width: Self.chevW, alignment: .center)
        }
        .frame(height: 56)
        .padding(.horizontal, Space.lg)
    }

    // Narrow: a card.
    private var cardRow: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            HStack {
                Text(app.fmtDateShort(shift.openedAt)).font(.ui(14, .bold)).foregroundStyle(theme.colors.textPrimary)
                Spacer()
                statusChip
                MadarIcon(expanded ? "chevron.down" : "chevron.right", size: 12).foregroundStyle(theme.colors.textMuted)
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
                    else { MadarIcon("printer", size: IconSize.sm) }
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
            Text(app.fmtTime(o.createdAt)).font(.ui(11)).foregroundStyle(theme.colors.textMuted)
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

    private func metric(_ label: String, _ value: String, valueColor: Color? = nil) -> some View {
        HStack {
            Text(label).font(.ui(12, .medium)).foregroundStyle(theme.colors.textSecondary)
            Spacer()
            Text(value).font(.money(13, .semibold)).foregroundStyle(valueColor ?? theme.colors.textPrimary)
        }
    }
}

