// Order history — the current shift's orders: the still-queued sales (shown
// with a Queued/Failed chip) plus the server's synced orders. Reachable from the
// order action bar. Responsive: a sortable data TABLE at width ≥ 680 (mirroring
// the Flutter `_OrderTable`), stacked expandable CARDS below it. Tapping a row
// expands its line detail inline (totals + Print + Void). Works offline (the
// queued ones are always there). Full shift stays in memory; only `visibleLimit`
// rows paint (client-side "show more").
import SwiftUI

// MARK: - Sort model

/// The five sortable table columns. `defaultAscending` is the direction a column
/// starts in the first time it becomes active (only `#`/number ascends by
/// default; everything else descends — newest/biggest first).
private enum OrderSortCol: CaseIterable {
    case number, payment, time, teller, amount
    var defaultAscending: Bool { self == .number ? true : false }
}

/// One sync-status filter axis value + its match rule.
private enum SyncFilter: CaseIterable {
    case all, synced, pending, voided
    func matches(_ o: OrderSummaryView) -> Bool {
        switch self {
        case .all:     return true
        case .synced:  return !o.queued && o.status != "voided"
        case .pending: return o.queued
        case .voided:  return o.status == "voided"
        }
    }
}

/// One order-origin filter axis value + its match rule.
private enum TypeFilter: CaseIterable {
    case all, dineIn, delivery
    func matches(_ o: OrderSummaryView) -> Bool {
        switch self {
        case .all:      return true
        case .dineIn:   return o.orderType != "delivery"
        case .delivery: return o.orderType == "delivery"
        }
    }
}

private let kTableBreakpoint: CGFloat = 680
private let kOrderPageSize = 20
private let kTableMaxWidth: CGFloat = 960

struct OrderHistoryView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let onClose: () -> Void

    @State private var expandedId: String?
    @State private var voidTarget: OrderSummaryView?
    @State private var search = ""
    @State private var syncFilter: SyncFilter = .all
    @State private var typeFilter: TypeFilter = .all
    @State private var sortCol: OrderSortCol = .number
    @State private var sortAscending = false          // # defaults to DESC (newest first)
    @State private var visibleLimit = kOrderPageSize

    private var currency: String { app.session?.currencyCode ?? "" }

    // MARK: Filtering / sorting

    private func matchesSearch(_ o: OrderSummaryView) -> Bool {
        guard !search.isEmpty else { return true }
        return (o.orderNumber.map { "\($0)" } ?? "").contains(search)
            || o.paymentLabel.localizedCaseInsensitiveContains(search)
            || (o.tellerName?.localizedCaseInsensitiveContains(search) ?? false)
            || (o.customerName?.localizedCaseInsensitiveContains(search) ?? false)
    }

    /// All rows passing search + both filter axes (AND), then sorted.
    private var filtered: [OrderSummaryView] {
        let base = app.history.filter {
            matchesSearch($0) && syncFilter.matches($0) && typeFilter.matches($0)
        }
        return base.sorted(by: lessThan)
    }

    /// The slice actually painted (client-side pagination).
    private var visible: [OrderSummaryView] { Array(filtered.prefix(visibleLimit)) }

    private func lessThan(_ a: OrderSummaryView, _ b: OrderSummaryView) -> Bool {
        let asc = sortAscending
        switch sortCol {
        case .number:  return cmp(a.orderNumber ?? -1, b.orderNumber ?? -1, asc)
        case .payment: return cmp(a.paymentLabel, b.paymentLabel, asc)
        case .time:    return cmp(a.createdAt, b.createdAt, asc)
        case .teller:  return cmp(a.tellerName ?? "", b.tellerName ?? "", asc)
        case .amount:  return cmp(a.totalMinor, b.totalMinor, asc)
        }
    }
    private func cmp<T: Comparable>(_ a: T, _ b: T, _ asc: Bool) -> Bool { asc ? a < b : a > b }

    /// Re-sort or re-filter → snap the visible page back to the first 20.
    private func resetPage() { visibleLimit = kOrderPageSize }

    private func setSort(_ col: OrderSortCol) {
        Haptics.selection()
        if sortCol == col { sortAscending.toggle() }
        else { sortCol = col; sortAscending = col.defaultAscending }
        resetPage()
    }

    private func toggleExpand(_ id: String) {
        Haptics.selection()
        withAnimation(Motion.standard) { expandedId = expandedId == id ? nil : id }
    }

    // MARK: Body

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            VStack(spacing: 0) {
                header
                if !app.history.isEmpty { filterBar }
                content
            }
        }
        .task { await app.loadHistory() }
        .task(id: expandedId) {
            if let id = expandedId, let o = filtered.first(where: { $0.id == id }), !o.queued {
                await app.loadOrderDetail(id)
            }
        }
        .madarSheet(item: $voidTarget) { order, dismiss in
            VoidSheet(app: app, order: order, onDone: dismiss)
        }
        .madarSheet(item: $app.previewReceipt, size: .large) { r, dismiss in
            ReceiptPreviewSheet(app: app, receipt: r, onClose: dismiss)
        }
    }

    @ViewBuilder private var content: some View {
        if app.isLoadingHistory && app.history.isEmpty {
            ScrollView { SkeletonList() }
        } else if filtered.isEmpty {
            emptyState
        } else {
            GeometryReader { geo in
                ScrollView {
                    VStack(spacing: Space.lg) {
                        statsHeader
                        if geo.size.width >= kTableBreakpoint {
                            table
                        } else {
                            cardList
                        }
                        showMoreFooter
                    }
                    .frame(maxWidth: kTableMaxWidth)
                    .frame(maxWidth: .infinity)
                    .padding(Space.lg)
                }
            }
        }
    }

    private var emptyState: some View {
        VStack(spacing: Space.md) {
            MadarIcon(app.history.isEmpty ? "tray" : "line.3.horizontal.decrease.circle", size: 40)
                .foregroundStyle(theme.colors.textMuted)
            Text(app.history.isEmpty ? t("history.empty") : t("history.no_match"))
                .font(.ui(14)).foregroundStyle(theme.colors.textSecondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    // MARK: Chrome (top bar + search/filter rows)

    private var header: some View {
        HStack(spacing: Space.md) {
            Button { onClose() } label: {
                MadarIcon("chevron.backward", size: 17)
                    .foregroundStyle(theme.colors.textPrimary)
            }
            .buttonStyle(.pressable)
            VStack(alignment: .leading, spacing: 1) {
                Text(t("history.title")).font(.ui(17, .bold)).foregroundStyle(theme.colors.textPrimary)
                if app.shift != nil {
                    Text(t("history.current_shift")).font(.ui(11)).foregroundStyle(theme.colors.textMuted)
                }
            }
            Spacer(minLength: 0)
            if app.isLoadingHistory && !app.history.isEmpty {
                ProgressView().controlSize(.small)
            }
        }
        .padding(.horizontal, Space.lg)
        .padding(.vertical, Space.md)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }

    private var filterBar: some View {
        VStack(spacing: Space.sm) {
            MadarTextField(placeholder: t("history.search"), text: $search, icon: "magnifyingglass")
            // Type axis (origin) — counts reflect search ∩ THIS chip's type rule.
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: Space.sm) {
                    typeChip(t("history.type.all"),      .all,      "slider.horizontal.3")
                    typeChip(t("history.type.dine_in"),  .dineIn,   "fork.knife")
                    typeChip(t("history.type.delivery"), .delivery, "shippingbox")
                }
            }
            // Sync axis — counts reflect search ∩ type ∩ THIS chip's sync rule.
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: Space.sm) {
                    syncChip(t("order.all"),       .all,     "list.bullet",        .accent)
                    syncChip(t("history.synced"),  .synced,  "checkmark.icloud",   .success)
                    syncChip(t("history.queued"),  .pending, "icloud.and.arrow.up", .warning)
                    syncChip(t("history.voided"),  .voided,  "xmark.circle",       .danger)
                }
            }
        }
        .padding(.horizontal, Space.lg).padding(.vertical, Space.sm)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.borderLight).frame(height: 1) }
    }

    private func typeCount(_ f: TypeFilter) -> Int {
        app.history.filter { matchesSearch($0) && f.matches($0) }.count
    }
    private func syncCount(_ f: SyncFilter) -> Int {
        app.history.filter { matchesSearch($0) && typeFilter.matches($0) && f.matches($0) }.count
    }

    private func typeChip(_ label: String, _ value: TypeFilter, _ icon: String) -> some View {
        let active = typeFilter == value
        return chip(label: "\(label) · \(typeCount(value))", icon: icon, active: active, tone: .accent) {
            typeFilter = value; resetPage()
        }
    }
    private func syncChip(_ label: String, _ value: SyncFilter, _ icon: String, _ tone: ChipTone) -> some View {
        let active = syncFilter == value
        return chip(label: "\(label) · \(syncCount(value))", icon: icon, active: active, tone: tone) {
            syncFilter = value; resetPage()
        }
    }

    /// Shared filter-chip pill: filled in its active tone, neutral when off.
    private func chip(label: String, icon: String, active: Bool, tone: ChipTone,
                      _ action: @escaping () -> Void) -> some View {
        Button { Haptics.selection(); action() } label: {
            HStack(spacing: 5) {
                MadarIcon(icon, size: 11)
                Text(label).font(.ui(12, .semibold))
            }
            .foregroundStyle(active ? tone.fg(theme.colors) : theme.colors.textSecondary)
            .padding(.horizontal, 12).padding(.vertical, 6)
            .background(active ? tone.bg(theme.colors) : theme.colors.surfaceAlt)
            .overlay(Capsule().strokeBorder(active ? tone.fg(theme.colors).opacity(0.25) : .clear, lineWidth: 1))
            .clipShape(Capsule())
        }
        .buttonStyle(.pressable(scale: 0.96))
    }

    // MARK: Stats header

    /// `[orders count] | [Total (success)] [· one chip per payment method]`.
    /// Prefers the live shift report; folds over local history otherwise.
    private var statsHeader: some View {
        let nonVoided = app.history.filter { $0.status != "voided" }
        let total = app.shiftReport?.netPaymentsMinor
            ?? nonVoided.reduce(Int64(0)) { $0 + $1.totalMinor }

        return ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: Space.md) {
                stat(t("history.stat.orders"), "\(nonVoided.count)", theme.colors.textPrimary)
                Rectangle().fill(theme.colors.border).frame(width: 1, height: 28)
                stat(t("order.total"), Money.format(total, currency), theme.colors.success)
                ForEach(paymentBreakdown(total: total), id: \.label) { b in
                    StatusChip(
                        label: "\(b.label) · \(Money.format(b.amount, currency)) · \(b.pct)%",
                        tone: b.tone
                    )
                }
            }
            .padding(.horizontal, Space.lg).padding(.vertical, Space.md)
        }
        .background(theme.colors.surface)
        .overlay(
            RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                .strokeBorder(theme.colors.border, lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
    }

    private func stat(_ label: String, _ value: String, _ color: Color) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label).font(.ui(11, .semibold)).foregroundStyle(theme.colors.textMuted).textCase(.uppercase)
            Text(value).font(.money(16, .bold)).foregroundStyle(color)
        }
    }

    private struct PaymentBreakdown { let label: String; let amount: Int64; let pct: Int; let tone: ChipTone }

    /// Per-method totals + share — from the shift report's payment lines when
    /// present, else folded from local (non-voided) history by payment label.
    private func paymentBreakdown(total: Int64) -> [PaymentBreakdown] {
        let denom = max(total, 1)
        if let lines = app.shiftReport?.paymentLines, !lines.isEmpty {
            return lines.map {
                PaymentBreakdown(
                    label: $0.method, amount: $0.totalMinor,
                    pct: Int((Double($0.totalMinor) / Double(denom) * 100).rounded()),
                    tone: $0.isCash ? .success : .info)
            }
        }
        var sums: [String: Int64] = [:]
        var order: [String] = []
        for o in app.history where o.status != "voided" {
            if sums[o.paymentLabel] == nil { order.append(o.paymentLabel) }
            sums[o.paymentLabel, default: 0] += o.totalMinor
        }
        return order.map { label in
            let amt = sums[label] ?? 0
            return PaymentBreakdown(
                label: label, amount: amt,
                pct: Int((Double(amt) / Double(denom) * 100).rounded()),
                tone: label.localizedCaseInsensitiveContains("cash") ? .success : .info)
        }
    }

    // MARK: Wide TABLE

    private var table: some View {
        VStack(spacing: 0) {
            tableHeader
            Rectangle().fill(theme.colors.border).frame(height: 1)
            ForEach(Array(visible.enumerated()), id: \.element.id) { idx, item in
                TableRow(
                    app: app, item: item, currency: currency, zebra: idx.isMultiple(of: 2) == false,
                    expanded: expandedId == item.id,
                    onToggle: { toggleExpand(item.id) },
                    onPrint: { Task { await app.openOrderReceiptPreview(item.id) } },
                    onVoid: { voidTarget = item })
                if idx < visible.count - 1 {
                    Rectangle().fill(theme.colors.borderLight).frame(height: 1)
                }
            }
        }
        .background(theme.colors.surface)
        .overlay(
            RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                .strokeBorder(theme.colors.border, lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
    }

    private var tableHeader: some View {
        HStack(spacing: Space.md) {
            headerCell("#",                       .number, width: 104, align: .leading)
            headerCell(t("order.payment"),        .payment, align: .leading).frame(maxWidth: .infinity, alignment: .leading)
            headerCell(t("history.col.time"),     .time,    align: .leading).frame(maxWidth: .infinity, alignment: .leading)
            headerCell(t("history.col.teller"),   .teller,  align: .leading).frame(maxWidth: .infinity, alignment: .leading)
            headerCell(t("history.col.amount"),   .amount,  width: 110, align: .trailing)
            Color.clear.frame(width: 44, height: 1)
        }
        .padding(.horizontal, Space.md).padding(.vertical, Space.sm)
        .background(theme.colors.surfaceAlt)
    }

    @ViewBuilder
    private func headerCell(_ label: String, _ col: OrderSortCol,
                            width: CGFloat? = nil, align: Alignment) -> some View {
        let active = sortCol == col
        let cell = Button { setSort(col) } label: {
            HStack(spacing: 3) {
                if align == .trailing && active { sortArrow }
                Text(label).font(.ui(11, .bold)).textCase(.uppercase)
                if align != .trailing && active { sortArrow }
            }
            .foregroundStyle(active ? theme.colors.accent : theme.colors.textMuted)
            .frame(maxWidth: width == nil ? .infinity : nil, alignment: align)
        }
        .buttonStyle(.pressable(scale: 0.97))
        if let width { cell.frame(width: width, alignment: align) } else { cell }
    }

    private var sortArrow: some View {
        MadarIcon(sortAscending ? "arrow.up" : "arrow.down", size: 9)
    }

    // MARK: Narrow CARD list

    private var cardList: some View {
        // Lazy — only on-screen cards build their bodies, and the list grows via
        // load-more, so eager VStack would inflate every row up front.
        LazyVStack(spacing: Space.md) {
            ForEach(visible, id: \.id) { item in
                OrderCard(
                    app: app, item: item, currency: currency,
                    expanded: expandedId == item.id,
                    onToggle: { toggleExpand(item.id) },
                    onPrint: { Task { await app.openOrderReceiptPreview(item.id) } },
                    onVoid: { voidTarget = item })
            }
        }
    }

    // MARK: Pagination footer

    @ViewBuilder private var showMoreFooter: some View {
        let remaining = filtered.count - visible.count
        if remaining > 0 {
            Button {
                Haptics.selection()
                withAnimation(Motion.standard) { visibleLimit += kOrderPageSize }
            } label: {
                HStack(spacing: 6) {
                    MadarIcon("chevron.down", size: IconSize.sm)
                    Text(t("history.show_more").replacingOccurrences(of: "{count}", with: "\(min(kOrderPageSize, remaining))"))
                }
                .font(.ui(13, .semibold)).foregroundStyle(theme.colors.accent)
                .frame(maxWidth: .infinity).padding(.vertical, Space.md)
                .background(theme.colors.surface)
                .overlay(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                    .strokeBorder(theme.colors.border, lineWidth: 1))
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
            }
            .buttonStyle(.pressable(scale: 0.98))
        }
    }
}

// MARK: - Wide table row

private struct TableRow: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let item: OrderSummaryView
    let currency: String
    let zebra: Bool            // odd row → surfaceAlt tint
    let expanded: Bool
    let onToggle: () -> Void
    let onPrint: () -> Void
    let onVoid: () -> Void

    private var voided: Bool { item.status == "voided" }
    private var loadingDetail: Bool {
        expanded && !item.queued && !(app.orderDetail?.id == item.id)
    }

    var body: some View {
        VStack(spacing: 0) {
            Button { onToggle() } label: { rowCells }.buttonStyle(.plain)
            if expanded {
                OrderDetailPanel(app: app, item: item, currency: currency,
                                 onPrint: onPrint, onVoid: onVoid)
                    .padding(.horizontal, Space.md).padding(.bottom, Space.md)
            }
        }
        .background(rowBackground)
    }

    private var rowBackground: Color {
        if expanded { return theme.colors.navyBg }
        return zebra ? theme.colors.surfaceAlt : .clear
    }

    private var rowCells: some View {
        HStack(spacing: Space.md) {
            numberCell.frame(width: 104, alignment: .leading)
            paymentCell.frame(maxWidth: .infinity, alignment: .leading)
            Text(app.fmtTime(item.createdAt))
                .font(.ui(13)).foregroundStyle(theme.colors.textSecondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            tellerCell.frame(maxWidth: .infinity, alignment: .leading)
            Text(Money.format(item.totalMinor, currency))
                .font(.money(14, .semibold))
                .strikethrough(voided)
                .foregroundStyle(voided ? theme.colors.textMuted : theme.colors.textPrimary)
                .frame(width: 110, alignment: .trailing)
            chevron.frame(width: 44)
        }
        .opacity(voided ? 0.55 : 1)
        .padding(.horizontal, Space.md).frame(minHeight: 56)
        .contentShape(Rectangle())
    }

    @ViewBuilder private var numberCell: some View {
        if item.queued {
            MadarIcon("icloud.and.arrow.up", size: 16).foregroundStyle(theme.colors.warning)
        } else {
            VStack(alignment: .leading, spacing: 1) {
                Text(item.orderNumber.map { "#\($0)" } ?? t("history.order"))
                    .font(.ui(14, .bold)).foregroundStyle(theme.colors.navy)
                if let ref = item.orderRef {
                    Text(ref).font(.ui(9)).foregroundStyle(theme.colors.textMuted).lineLimit(1)
                }
            }
        }
    }

    private var paymentCell: some View {
        HStack(spacing: 6) {
            PaymentBadge(label: item.paymentLabel, voided: voided)
            if voided { StatusChip(label: t("history.voided"), tone: .danger) }
            else if item.status == "failed" { StatusChip(label: t("history.failed"), tone: .danger) }
            else if item.queued { StatusChip(label: t("history.queued"), icon: "arrow.triangle.2.circlepath", tone: .warning) }
            if let c = item.customerName {
                Text(c).font(.ui(12)).foregroundStyle(theme.colors.textMuted).lineLimit(1)
            }
        }
    }

    private var tellerCell: some View {
        HStack(spacing: 4) {
            MadarIcon("person", size: 12).foregroundStyle(theme.colors.textMuted)
            Text(item.tellerName ?? "—").font(.ui(12)).foregroundStyle(theme.colors.textSecondary).lineLimit(1)
        }
    }

    @ViewBuilder private var chevron: some View {
        if loadingDetail {
            ProgressView().controlSize(.small)
        } else {
            MadarIcon("chevron.down", size: 13).foregroundStyle(theme.colors.textMuted)
                .rotationEffect(.degrees(expanded ? 180 : 0))
        }
    }
}

// MARK: - Narrow card

private struct OrderCard: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let item: OrderSummaryView
    let currency: String
    let expanded: Bool
    let onToggle: () -> Void
    let onPrint: () -> Void
    let onVoid: () -> Void

    private var voided: Bool { item.status == "voided" }
    private var loadingDetail: Bool {
        expanded && !item.queued && !(app.orderDetail?.id == item.id)
    }

    var body: some View {
        VStack(spacing: Space.sm) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 6) {
                        if item.queued {
                            MadarIcon("icloud.and.arrow.up", size: 14).foregroundStyle(theme.colors.warning)
                        }
                        Text(item.orderNumber.map { "#\($0)" } ?? t("history.order"))
                            .font(.ui(14, .bold)).foregroundStyle(theme.colors.navy)
                        Text(app.fmtTime(item.createdAt))
                            .font(.ui(11)).foregroundStyle(theme.colors.textMuted)
                    }
                    HStack(spacing: 6) {
                        PaymentBadge(label: item.paymentLabel, voided: voided)
                        if voided { StatusChip(label: t("history.voided"), tone: .danger) }
                        else if item.status == "failed" { StatusChip(label: t("history.failed"), tone: .danger) }
                        else if item.queued { StatusChip(label: t("history.queued"), tone: .warning) }
                    }
                    if let c = item.customerName {
                        Text(c).font(.ui(12)).foregroundStyle(theme.colors.textMuted).lineLimit(1)
                    }
                    HStack(spacing: 4) {
                        MadarIcon("person", size: 11).foregroundStyle(theme.colors.textMuted)
                        Text(item.tellerName ?? "—").font(.ui(11)).foregroundStyle(theme.colors.textSecondary).lineLimit(1)
                    }
                }
                Spacer(minLength: Space.sm)
                VStack(alignment: .trailing, spacing: 4) {
                    Text(Money.format(item.totalMinor, currency))
                        .font(.money(15, .bold)).strikethrough(voided)
                        .foregroundStyle(voided ? theme.colors.textMuted : theme.colors.textPrimary)
                    if loadingDetail { ProgressView().controlSize(.small) }
                    else {
                        MadarIcon("chevron.down", size: 12).foregroundStyle(theme.colors.textMuted)
                            .rotationEffect(.degrees(expanded ? 180 : 0))
                    }
                }
            }
            .opacity(voided ? 0.55 : 1)
            .contentShape(Rectangle())
            .onTapGesture { onToggle() }

            if expanded {
                Rectangle().fill(theme.colors.border).frame(height: 1)
                OrderDetailPanel(app: app, item: item, currency: currency,
                                 onPrint: onPrint, onVoid: onVoid)
            }
        }
        .padding(.horizontal, Space.lg).padding(.vertical, Space.md)
        .background(expanded ? theme.colors.navyBg : theme.colors.surface)
        .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
            .strokeBorder(theme.colors.border, lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
    }
}

// MARK: - Shared expanded detail (line items + totals + Print/Void)

private struct OrderDetailPanel: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let item: OrderSummaryView
    let currency: String
    let onPrint: () -> Void
    let onVoid: () -> Void

    private var canVoid: Bool { !item.queued && item.status != "voided" }
    private var canPrint: Bool { !item.queued && item.status != "voided" }

    var body: some View {
        VStack(spacing: Space.sm) {
            if let d = app.orderDetail, d.id == item.id {
                ForEach(Array(d.lines.enumerated()), id: \.offset) { _, line in lineRow(line) }
                Rectangle().fill(theme.colors.borderLight).frame(height: 1)
                detailRow(t("order.subtotal"), Money.format(d.subtotalMinor, currency))
                if d.discountMinor > 0 {
                    detailRow(t("order.discount"), "− " + Money.format(d.discountMinor, currency),
                              color: theme.colors.success)
                }
                detailRow(t("order.tax"), Money.format(d.taxMinor, currency))
            } else {
                // Queued/offline order, or detail not yet loaded — fall back to summary totals.
                detailRow(t("order.subtotal"), Money.format(item.subtotalMinor, currency))
                detailRow(t("order.tax"), Money.format(item.taxMinor, currency))
            }
            HStack(spacing: Space.md) {
                Text(item.paymentLabel).font(.ui(12)).foregroundStyle(theme.colors.textSecondary)
                Spacer()
                if canPrint {
                    actionButton(t("receipt.print"), icon: "printer", color: theme.colors.accent, action: onPrint)
                }
                if canVoid {
                    actionButton(t("void.action"), icon: "trash", color: theme.colors.danger, action: onVoid)
                }
            }
        }
    }

    private func actionButton(_ label: String, icon: String, color: Color, action: @escaping () -> Void) -> some View {
        Button { Haptics.selection(); action() } label: {
            HStack(spacing: 5) { MadarIcon(icon, size: IconSize.xs); Text(label) }
                .font(.ui(12, .semibold)).foregroundStyle(color)
        }
        .buttonStyle(.pressable)
    }

    private func detailRow(_ label: String, _ value: String, color: Color? = nil) -> some View {
        HStack {
            Text(label).font(.ui(13)).foregroundStyle(theme.colors.textSecondary)
            Spacer()
            Text(value).font(.money(13, .semibold)).foregroundStyle(color ?? theme.colors.textPrimary)
        }
    }

    private func lineRow(_ line: OrderDetailLineView) -> some View {
        let mods = ([line.sizeLabel].compactMap { $0 } + line.addons + line.optionals)
        return HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 1) {
                Text("\(line.qty)× \(line.name)").font(.ui(13, .semibold)).foregroundStyle(theme.colors.textPrimary)
                if !mods.isEmpty {
                    Text(mods.joined(separator: " · ")).font(.ui(11)).foregroundStyle(theme.colors.textMuted).lineLimit(2)
                }
            }
            Spacer(minLength: Space.sm)
            Text(Money.format(line.lineTotalMinor, currency)).font(.money(13, .semibold)).foregroundStyle(theme.colors.textPrimary)
        }
    }
}

// MARK: - Payment badge

/// A colored payment pill (not a StatusChip): tinted bg @ ~14%, colored label.
/// Voided → muted/surfaceAlt. Color is keyed off the label text (cash → green,
/// card → violet, mixed → amber, else → navy) since the summary carries only the
/// resolved label string.
private struct PaymentBadge: View {
    @Environment(\.theme) private var theme
    let label: String
    var voided: Bool = false

    private var tint: Color {
        let l = label.lowercased()
        if l.contains("cash") || l.contains("نقد") { return theme.colors.success }
        if l.contains("card") || l.contains("بطاق") { return Color(hex: 0x7C3AED) }
        if l.contains("mixed") || l.contains("مختلط") { return theme.colors.warning }
        return theme.colors.navy
    }

    var body: some View {
        let fg = voided ? theme.colors.textMuted : tint
        Text(label)
            .font(.ui(11, .semibold)).foregroundStyle(fg)
            .padding(.horizontal, 8).padding(.vertical, 3)
            .background(voided ? theme.colors.surfaceAlt : tint.opacity(0.14))
            .clipShape(Capsule())
    }
}
