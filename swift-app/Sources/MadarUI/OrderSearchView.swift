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
                ScreenHeader(t("search.title"), onBack: onClose) {
                    HStack(spacing: Space.md) {
                        if app.orderSearchTotal > 0 {
                            Text("\(app.orderSearchTotal)").font(.ui(14, .bold)).foregroundStyle(theme.colors.textSecondary)
                        }
                        if !app.orderSearchResults.isEmpty {
                            Button(action: exportCsv) {
                                Image(systemName: "square.and.arrow.up").font(.system(size: 17, weight: .semibold)).foregroundStyle(theme.colors.accent)
                            }.buttonStyle(.plain)
                        }
                    }
                }.screenHeaderBar()

                VStack(spacing: Space.sm) {
                    FlowLayout(spacing: Space.sm) {
                        SelectableChip(label: t("search.date_24h"), isSelected: days == 1) { days = 1; run(reset: true) }
                        SelectableChip(label: t("search.date_7d"), isSelected: days == 7) { days = 7; run(reset: true) }
                        SelectableChip(label: t("search.date_30d"), isSelected: days == 30) { days = 30; run(reset: true) }
                        SelectableChip(label: t("order.all"), isSelected: days == 0) { days = 0; run(reset: true) }
                    }
                    FlowLayout(spacing: Space.sm) {
                        SelectableChip(label: t("order.all"), isSelected: status == nil) { status = nil; run(reset: true) }
                        SelectableChip(label: t("history.completed"), isSelected: status == "completed") { status = "completed"; run(reset: true) }
                        SelectableChip(label: t("history.voided"), isSelected: status == "voided") { status = "voided"; run(reset: true) }
                    }
                    HStack(spacing: Space.sm) {
                        MadarTextField(placeholder: t("search.teller_hint"), text: $teller, icon: "person")
                        MadarButton(label: t("search.title"), icon: "magnifyingglass", loading: app.isSearchingOrders, fullWidth: false) { run(reset: true) }
                    }
                }
                .padding(Space.lg).background(theme.colors.surface)
                .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }

                content
            }
        }
        .task { run(reset: true) }
    }

    @ViewBuilder private var content: some View {
        if app.isSearchingOrders && app.orderSearchResults.isEmpty {
            VStack { Spacer(); ProgressView().tint(theme.colors.accent); Spacer() }
        } else if app.orderSearchResults.isEmpty {
            EmptyState(icon: "magnifyingglass", title: t("history.no_match"))
        } else {
            ScrollView {
                LazyVStack(spacing: Space.sm) {
                    ForEach(app.orderSearchResults, id: \.id) { o in resultRow(o) }
                    if app.orderSearchHasMore {
                        MadarButton(label: t("search.load_more"), icon: "arrow.down.circle", variant: .outline, loading: app.isSearchingOrders) {
                            run(reset: false)
                        }
                    }
                }.padding(Space.lg)
            }
        }
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

    private func resultRow(_ o: OrderSummaryView) -> some View {
        let tone: Color = (o.status == "voided" || o.status == "failed") ? theme.colors.danger
            : o.status == "completed" ? theme.colors.success
            : o.status == "queued" ? theme.colors.warning : theme.colors.textSecondary
        return HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text("#\(o.orderNumber.map(String.init) ?? "—")").font(.ui(16, .bold)).foregroundStyle(theme.colors.textPrimary)
                Text(app.fmtDateTime(o.createdAt)).font(.ui(13)).foregroundStyle(theme.colors.textMuted)
            }
            Spacer()
            VStack(alignment: .trailing, spacing: 2) {
                Text(Money.format(o.totalMinor, currency)).font(.money(15, .bold)).foregroundStyle(theme.colors.textPrimary)
                Text("\(o.status) · \(o.paymentLabel)").font(.ui(11, .semibold)).foregroundStyle(tone)
            }
        }
        .padding(Space.md)
        .background(theme.colors.surface)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous).strokeBorder(theme.colors.borderLight, lineWidth: 1))
    }
}
