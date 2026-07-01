// All-orders search — a history lookup ACROSS shifts (date range + status +
// teller), paginated. Closes the "operators can't look up a past-shift order"
// gap. Full-screen over the order screen; teller-only. Mirror of OrderSearchScreen.kt.
import SwiftUI
#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

struct OrderSearchView: View {
    @ObservedObject var app: AppModel
    let onClose: () -> Void
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    @State private var status: String? = nil   // nil = all
    @State private var teller = ""
    @State private var days = 7                 // 0 = all time

    private var currency: String { app.session?.currencyCode ?? "" }

    private func run(reset: Bool) {
        let from = days > 0 ? Date(timeIntervalSinceNow: -Double(days) * 86_400).ISO8601Format() : nil
        Task { await app.searchOrders(status: status, teller: teller, payment: nil, fromIso: from, reset: reset) }
    }

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            VStack(spacing: 0) {
                // Header — shared ScreenHeader bar (mirror of OrderSearchScreen).
                // Trailing carries the result count + a copy-to-clipboard CSV export.
                ScreenHeader(t("search.title"), onBack: onClose) {
                    HStack(spacing: Space.md) {
                        if app.orderSearchTotal > 0 {
                            Text("\(app.orderSearchTotal)").font(Typo.title.font).foregroundStyle(theme.colors.textSecondary)
                        }
                        if !app.orderSearchResults.isEmpty {
                            Button(action: exportCsv) {
                                MadarIcon("square.and.arrow.up", size: IconSize.lg)
                                    .foregroundStyle(theme.colors.accent)
                                    .frame(width: Metric.closeButton, height: Metric.closeButton)
                                    .background(theme.colors.accentBg)
                                    .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
                            }.buttonStyle(.pressable)
                        }
                    }
                }.screenHeaderBar()

                OrderSearchFilters(
                    days: $days, status: $status, teller: $teller,
                    isSearching: app.isSearchingOrders, onChange: { run(reset: true) },
                )

                OrderSearchContent(app: app, currency: currency, onLoadMore: { run(reset: false) })
            }
        }
        .task { run(reset: true) }
    }

    // Spreadsheet-friendly export of the current result page → clipboard.
    // RFC-4180 quoting so a comma in a label can't shift columns.
    private func exportCsv() {
        func esc(_ s: String) -> String { "\"" + s.replacingOccurrences(of: "\"", with: "\"\"") + "\"" }
        var out = "Order,Date,Total,Payment,Status\n"
        for o in app.orderSearchResults {
            out += "#\(o.orderNumber.map(String.init) ?? ""),"
            out += esc(o.createdAt) + ","
            out += esc(Money.format(o.totalMinor, currency)) + ","
            out += esc(o.paymentLabel) + ","
            out += esc(o.status) + "\n"
        }
        #if canImport(UIKit)
        UIPasteboard.general.string = out
        #elseif canImport(AppKit)
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(out, forType: .string)
        #endif
        app.showToast(t("search.exported"), icon: "checkmark.circle.fill", tone: .success)
    }
}

// MARK: - Filters — date range, status, and a teller lookup on a raised surface
// block closed off with a hairline (matches the order screen chrome).
private struct OrderSearchFilters: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    @Binding var days: Int
    @Binding var status: String?
    @Binding var teller: String
    let isSearching: Bool
    let onChange: () -> Void

    var body: some View {
        VStack(spacing: Space.md) {
            FlowLayout(spacing: Space.sm) {
                SelectableChip(label: t("search.date_24h"), isSelected: days == 1) { days = 1; onChange() }
                SelectableChip(label: t("search.date_7d"), isSelected: days == 7) { days = 7; onChange() }
                SelectableChip(label: t("search.date_30d"), isSelected: days == 30) { days = 30; onChange() }
                SelectableChip(label: t("order.all"), isSelected: days == 0) { days = 0; onChange() }
            }
            FlowLayout(spacing: Space.sm) {
                SelectableChip(label: t("order.all"), isSelected: status == nil) { status = nil; onChange() }
                SelectableChip(label: t("history.completed"), isSelected: status == "completed", tone: .success) { status = "completed"; onChange() }
                SelectableChip(label: t("history.voided"), isSelected: status == "voided", tone: .danger) { status = "voided"; onChange() }
            }
            HStack(spacing: Space.sm) {
                MadarTextField(placeholder: t("search.teller_hint"), text: $teller, icon: "person")
                MadarButton(label: t("search.title"), icon: "magnifyingglass", loading: isSearching, fullWidth: false) { onChange() }
            }
        }
        .padding(Space.lg).background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }
}

// MARK: - Results — loading / empty / the paginated result list
private struct OrderSearchContent: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let currency: String
    let onLoadMore: () -> Void

    var body: some View {
        if app.isSearchingOrders && app.orderSearchResults.isEmpty {
            VStack { Spacer(); ProgressView().tint(theme.colors.accent); Spacer() }
        } else if app.orderSearchResults.isEmpty {
            EmptyState(icon: "magnifyingglass", title: t("history.no_match"))
        } else {
            ScrollView {
                LazyVStack(spacing: Space.sm) {
                    ForEach(app.orderSearchResults, id: \.id) { o in
                        OrderSearchResultRow(timestamp: app.fmtDateTime(o.createdAt), order: o, currency: currency)
                    }
                    if app.orderSearchHasMore {
                        MadarButton(label: t("search.load_more"), icon: "arrow.down.circle", variant: .outline, loading: app.isSearchingOrders) {
                            onLoadMore()
                        }
                    }
                }.padding(Space.lg)
            }
        }
    }
}

// MARK: - One order result card — number + timestamp leading, bold-teal money
// + a tone chip + payment label trailing.
private struct OrderSearchResultRow: View {
    @Environment(\.theme) private var theme
    let timestamp: String
    let order: OrderSummaryView
    let currency: String

    /// Status → a tone-paired chip color (voided/failed = danger, completed =
    /// success, queued = warning, else neutral).
    private var statusTone: ChipTone {
        switch order.status {
        case "voided", "failed": return .danger
        case "completed": return .success
        case "queued": return .warning
        default: return .neutral
        }
    }

    /// A voided / failed order is dead money — mute + strike its total so the
    /// hero teal reads only on live orders (mirrors the history screen).
    private var dead: Bool { order.status == "voided" || order.status == "failed" }

    var body: some View {
        HStack(alignment: .top, spacing: Space.md) {
            VStack(alignment: .leading, spacing: Space.xs) {
                Text("#\(order.orderNumber.map(String.init) ?? "—")").font(Typo.h3.font).foregroundStyle(theme.colors.textPrimary)
                Text(timestamp).font(Typo.bodySm.font).foregroundStyle(theme.colors.textMuted)
            }
            Spacer(minLength: Space.sm)
            VStack(alignment: .trailing, spacing: Space.sm) {
                // Money is the hero — bold teal; struck + muted once voided.
                Text(Money.format(order.totalMinor, currency))
                    .font(.money(17, .bold))
                    .strikethrough(dead)
                    .foregroundStyle(dead ? theme.colors.textMuted : theme.colors.accent)
                HStack(spacing: Space.sm) {
                    Text(order.paymentLabel).font(Typo.labelSm.font).foregroundStyle(theme.colors.textMuted)
                    StatusChip(label: order.status, tone: statusTone)
                }
            }
        }
        .padding(.horizontal, Space.lg)
        .padding(.vertical, Space.md)
        .background(theme.colors.surface)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous).strokeBorder(theme.colors.borderLight, lineWidth: 1))
        .elevation(.card)
    }
}
