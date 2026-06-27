// Close-shift — count the closing drawer and end the shift. Presented over the
// order screen; on a successful close the core marks the shift closed and the
// route flips back to open-shift. Card-based, mirroring the Flutter close screen
// (summary + cash count). Works offline (the close queues).
import SwiftUI

struct CloseShiftView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    @State private var countedMinor: Int64 = 0
    @State private var note = ""

    private var currency: String { app.session?.currencyCode ?? "" }

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            VStack(spacing: 0) {
                header
                ScrollView {
                    VStack(spacing: Space.lg) {
                        if let s = app.shift { summaryCard(s) }
                        cashCard
                        if let r = app.shiftReport { reportCard(r) }
                        if app.shiftReport != nil {
                            MadarButton(label: t("shift.print_report"), icon: "printer", variant: .outline,
                                         loading: app.printState == .printing) {
                                Task { await app.printShiftReport() }
                            }
                        }
                        if let error = app.errorMessage {
                            NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .danger)
                        }
                        MadarButton(label: t("order.close_shift"), icon: "lock", variant: .danger, loading: app.isBusy) {
                            Task { await app.closeShift(closingCashMinor: countedMinor, note: note) }
                        }
                    }
                    .frame(maxWidth: 640)
                    .frame(maxWidth: .infinity)
                    .padding(Space.xl)
                }
                #if os(iOS)
                .scrollDismissesKeyboard(.interactively)
                #endif
            }
        }
        .task { await app.loadShiftReport() }
    }

    private var header: some View {
        HStack(spacing: Space.md) {
            Button { app.errorMessage = nil; app.showCloseShift = false } label: {
                MadarIcon("chevron.backward", size: 17)
                    .foregroundStyle(theme.colors.textPrimary)
            }
            .buttonStyle(.pressable)
            VStack(alignment: .leading, spacing: 1) {
                Text(t("shift.close_title")).font(.ui(17, .heavy)).foregroundStyle(theme.colors.textPrimary)
                Text(t("shift.closing_desc")).font(.ui(12)).foregroundStyle(theme.colors.textSecondary)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, Space.lg)
        .padding(.vertical, Space.md)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }

    private func summaryCard(_ s: ShiftView) -> some View {
        Card {
            CardHeader(icon: "doc.text", title: t("shift.summary"))
            InfoRow(label: t("shift.teller"), value: s.tellerName)
            InfoRow(label: t("shift.opening_cash"), value: Money.format(s.openingCashMinor, currency))
            InfoRow(label: t("shift.opened_at"), value: app.fmtDateTime(s.openedAt))
        }
    }

    private var cashCard: some View {
        Card {
            CardHeader(icon: "banknote", title: t("shift.counted_cash"))
            if let r = app.shiftReport {
                HStack(alignment: .top) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(t("shift.system_cash"))
                            .font(.ui(12, .semibold)).foregroundStyle(theme.colors.textSecondary)
                        Text(t("shift.system_cash_explain"))
                            .font(.ui(11)).foregroundStyle(theme.colors.textMuted)
                    }
                    Spacer(minLength: Space.sm)
                    Text(Money.format(r.expectedCashMinor, currency))
                        .font(.money(20, .heavy)).foregroundStyle(theme.colors.textPrimary)
                }
                .padding(14)
                .background(theme.colors.surfaceAlt)
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
            }
            AmountField(amountMinor: $countedMinor, currencyCode: currency, autofocus: true)
            if let r = app.shiftReport {
                discrepancyBanner(declared: countedMinor, expected: r.expectedCashMinor)
            }
            MadarTextField(placeholder: t("shift.cash_note"), text: $note, icon: "note.text", disabled: app.isBusy)
        }
    }

    /// The Z-report breakdown: per-method sales (with order counts), drawer
    /// pay-in/out, voided total, and the itemised cash movements.
    private func reportCard(_ r: ShiftReportView) -> some View {
        Card {
            CardHeader(icon: "list.bullet.rectangle", title: t("shift.report_title"))
            ShiftReportBreakdown(report: r, currency: currency)
        }
    }

    @ViewBuilder
    private func discrepancyBanner(declared: Int64, expected: Int64) -> some View {
        let diff = declared - expected
        let color: Color = diff == 0 ? theme.colors.success : (diff > 0 ? theme.colors.warning : theme.colors.danger)
        let bg: Color = diff == 0 ? theme.colors.successBg : (diff > 0 ? theme.colors.warningBg : theme.colors.dangerBg)
        let icon = diff == 0 ? "checkmark.circle" : (diff > 0 ? "arrow.up.circle" : "arrow.down.circle")
        let label = diff == 0
            ? t("shift.drawer_matches")
            : (diff > 0
               ? "\(t("shift.drawer_over")) \(Money.format(diff, currency))"
               : "\(t("shift.drawer_short")) \(Money.format(-diff, currency))")
        HStack(spacing: Space.sm) {
            MadarIcon(icon, size: 15).foregroundStyle(color)
            Text(label).font(.ui(13, .semibold)).foregroundStyle(color)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 11)
        .background(bg)
        .overlay(
            RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                .strokeBorder(color.opacity(0.25), lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
    }
}

// MARK: - Card primitives (close-shift summary)

private struct Card<Content: View>: View {
    @Environment(\.theme) private var theme
    @ViewBuilder var content: Content
    var body: some View {
        VStack(alignment: .leading, spacing: Space.md) { content }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(Space.lg)
            .background(theme.colors.surface)
            .overlay(
                RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                    .strokeBorder(theme.colors.borderLight, lineWidth: 1)
            )
            .elevation(.card)
            .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
    }
}

private struct CardHeader: View {
    @Environment(\.theme) private var theme
    let icon: String
    let title: String
    var body: some View {
        HStack(spacing: Space.md) {
            MadarIcon(icon, size: 18)
                .foregroundStyle(theme.colors.navy)
                .frame(width: 36, height: 36)
                .background(theme.colors.navyBg)
                .clipShape(RoundedRectangle(cornerRadius: Radii.xs, style: .continuous))
            Text(title).font(.ui(14, .semibold)).foregroundStyle(theme.colors.textPrimary)
        }
    }
}

private struct InfoRow: View {
    @Environment(\.theme) private var theme
    let label: String
    let value: String
    var body: some View {
        HStack {
            Text(label).font(.ui(13)).foregroundStyle(theme.colors.textSecondary)
            Spacer()
            Text(value).font(.ui(13, .semibold)).foregroundStyle(theme.colors.textPrimary)
        }
    }
}
