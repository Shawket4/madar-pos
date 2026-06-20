// Order history — the current shift's orders: the still-queued sales (shown
// first with a Queued/Failed chip) plus the server's synced orders. Reachable
// from the order action bar; tap a row to see its totals. Works offline (the
// queued ones are always there).
import SwiftUI

struct OrderHistoryView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let onClose: () -> Void

    @State private var expandedId: String?

    private var currency: String { app.session?.currencyCode ?? "" }

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            VStack(spacing: 0) {
                header
                if app.history.isEmpty {
                    VStack(spacing: Space.md) {
                        if app.isLoadingHistory {
                            ProgressView().controlSize(.large)
                        } else {
                            Image(systemName: "tray")
                                .font(.system(size: 40, weight: .light))
                                .foregroundStyle(theme.colors.textMuted)
                            Text(t("history.empty")).font(.ui(14)).foregroundStyle(theme.colors.textSecondary)
                        }
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    ScrollView {
                        VStack(spacing: Space.sm) {
                            ForEach(app.history, id: \.id) { row($0) }
                        }
                        .frame(maxWidth: 560)
                        .frame(maxWidth: .infinity)
                        .padding(Space.lg)
                    }
                }
            }
        }
        .task { await app.loadHistory() }
    }

    private var header: some View {
        HStack(spacing: Space.md) {
            Button { onClose() } label: {
                Image(systemName: "chevron.left").font(.system(size: 17, weight: .semibold))
                    .foregroundStyle(theme.colors.textPrimary)
            }
            .buttonStyle(.pressable)
            Text(t("history.title")).font(.ui(17, .heavy)).foregroundStyle(theme.colors.textPrimary)
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

    private func row(_ item: OrderSummaryView) -> some View {
        Button {
            Haptics.selection()
            withAnimation(Motion.standard) { expandedId = expandedId == item.id ? nil : item.id }
        } label: {
            VStack(spacing: Space.sm) {
                HStack(alignment: .top) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(item.orderNumber.map { "#\($0)" } ?? t("history.order"))
                            .font(.ui(14, .semibold)).foregroundStyle(theme.colors.textPrimary)
                        Text(Self.timeOf(item.createdAt))
                            .font(.ui(11)).foregroundStyle(theme.colors.textMuted)
                    }
                    Spacer(minLength: Space.sm)
                    VStack(alignment: .trailing, spacing: 4) {
                        Text(Money.format(item.totalMinor, currency))
                            .font(.money(15, .bold)).foregroundStyle(theme.colors.textPrimary)
                        statusChip(item)
                    }
                }
                if expandedId == item.id {
                    Rectangle().fill(theme.colors.border).frame(height: 1)
                    detailRow(t("order.subtotal"), Money.format(item.subtotalMinor, currency))
                    detailRow(t("order.tax"), Money.format(item.taxMinor, currency))
                    HStack {
                        Text(item.paymentLabel).font(.ui(12)).foregroundStyle(theme.colors.textSecondary)
                        Spacer()
                    }
                }
            }
            .padding(Space.md)
            .background(theme.colors.surface)
            .overlay(
                RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                    .strokeBorder(theme.colors.border, lineWidth: 1)
            )
            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        }
        .buttonStyle(.pressable(scale: 0.99))
    }

    private func detailRow(_ label: String, _ value: String) -> some View {
        HStack {
            Text(label).font(.ui(12)).foregroundStyle(theme.colors.textSecondary)
            Spacer()
            Text(value).font(.money(12, .semibold)).foregroundStyle(theme.colors.textSecondary)
        }
    }

    @ViewBuilder private func statusChip(_ item: OrderSummaryView) -> some View {
        if item.status == "failed" {
            StatusChip(label: t("history.failed"), tone: .danger)
        } else if item.queued {
            StatusChip(label: t("history.queued"), tone: .warning)
        } else if item.status == "voided" {
            StatusChip(label: t("history.voided"), tone: .danger)
        }
    }

    /// rfc3339 → "HH:MM".
    static func timeOf(_ rfc3339: String) -> String {
        guard let tRange = rfc3339.range(of: "T") else { return rfc3339 }
        return String(rfc3339[tRange.upperBound...].prefix(5))
    }
}
