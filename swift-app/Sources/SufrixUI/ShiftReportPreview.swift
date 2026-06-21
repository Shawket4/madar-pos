// Shift / Z-report preview — a rich on-screen breakdown of the current shift,
// reused in two places:
//   • mid-shift, as a sheet with a Print button (no need to close the shift), and
//   • inside the close-shift flow, as the report breakdown.
// All numbers come from the core's ShiftReportView (report_view / offline
// fallback). Per-method sales show an order count + a proportional bar so the mix
// reads at a glance, mirroring the Flutter ShiftReportPreviewSheet.
import SwiftUI

/// The embeddable report body (payment mix + drawer movements + totals).
struct ShiftReportBreakdown: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let report: ShiftReportView
    let currency: String

    private var maxLine: Int64 {
        max(1, report.paymentLines.map(\.totalMinor).max() ?? 1)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: Space.md) {
            // Per-method sales with proportional bars.
            if report.paymentLines.isEmpty {
                Text(t("history.empty")).font(.ui(12)).foregroundStyle(theme.colors.textMuted)
            } else {
                VStack(spacing: Space.sm) {
                    ForEach(Array(report.paymentLines.enumerated()), id: \.offset) { _, p in
                        methodRow(p)
                    }
                }
            }

            Rectangle().fill(theme.colors.border).frame(height: 1)

            // Drawer movements.
            if report.cashInMinor > 0 {
                totalRow(t("shift.cash_in"), Money.format(report.cashInMinor, currency), tone: theme.colors.success)
            }
            if report.cashOutMinor > 0 {
                totalRow(t("shift.cash_out"), "−\(Money.format(report.cashOutMinor, currency))", tone: theme.colors.danger)
            }
            if !report.cashMovements.isEmpty {
                VStack(spacing: 3) {
                    ForEach(Array(report.cashMovements.enumerated()), id: \.offset) { _, m in
                        HStack {
                            Text(m.note.isEmpty ? m.movedByName : m.note)
                                .font(.ui(11)).foregroundStyle(theme.colors.textMuted).lineLimit(1)
                            Spacer(minLength: Space.sm)
                            Text((m.amountMinor < 0 ? "−" : "+") + Money.format(abs(m.amountMinor), currency))
                                .font(.money(11, .semibold))
                                .foregroundStyle(m.amountMinor < 0 ? theme.colors.danger : theme.colors.success)
                        }
                    }
                }
                .padding(.leading, Space.sm)
            }

            if report.voidedAmountMinor > 0 {
                totalRow(t("history.voided"), "−\(Money.format(report.voidedAmountMinor, currency))", tone: theme.colors.danger)
            }

            Rectangle().fill(theme.colors.border).frame(height: 1)

            totalRow(t("shift.payments"), Money.format(report.totalPaymentsMinor, currency))
            totalRow(t("shift.expected_cash"), Money.format(report.expectedCashMinor, currency), emphasized: true)
        }
    }

    private func methodRow(_ p: ShiftReportPaymentLine) -> some View {
        VStack(spacing: 5) {
            HStack {
                HStack(spacing: 6) {
                    Image(systemName: p.isCash ? "banknote" : "creditcard")
                        .font(.system(size: 12)).foregroundStyle(theme.colors.textMuted)
                    Text(p.method).font(.ui(13, .semibold)).foregroundStyle(theme.colors.textPrimary)
                    Text("· \(p.orderCount)").font(.ui(11)).foregroundStyle(theme.colors.textMuted)
                }
                Spacer()
                Text(Money.format(p.totalMinor, currency))
                    .font(.money(13, .bold)).foregroundStyle(theme.colors.textPrimary)
            }
            // Proportional bar.
            GeometryReader { geo in
                let frac = max(0.02, Double(p.totalMinor) / Double(maxLine))
                ZStack(alignment: .leading) {
                    Capsule().fill(theme.colors.surfaceAlt).frame(height: 5)
                    Capsule().fill(p.isCash ? theme.colors.success : theme.colors.accent)
                        .frame(width: geo.size.width * frac, height: 5)
                }
            }
            .frame(height: 5)
        }
    }

    private func totalRow(_ label: String, _ value: String, tone: Color? = nil, emphasized: Bool = false) -> some View {
        HStack {
            Text(label)
                .font(.ui(emphasized ? 15 : 13, emphasized ? .bold : .medium))
                .foregroundStyle(emphasized ? theme.colors.textPrimary : theme.colors.textSecondary)
            Spacer()
            Text(value)
                .font(.money(emphasized ? 16 : 13, emphasized ? .heavy : .semibold))
                .foregroundStyle(tone ?? theme.colors.textPrimary)
        }
    }
}

/// Mid-shift report preview sheet — Print without closing the shift.
struct ShiftReportPreviewView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let onClose: () -> Void

    private var currency: String { app.session?.currencyCode ?? "" }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 1) {
                    Text(t("shift.report_title")).font(.ui(18, .heavy)).foregroundStyle(theme.colors.textPrimary)
                    if let s = app.shift {
                        Text(s.tellerName).font(.ui(12)).foregroundStyle(theme.colors.textSecondary)
                    }
                }
                Spacer()
                if let r = app.shiftReport {
                    StatusChip(label: r.fromServer ? t("chrome.online") : t("chrome.offline"),
                               tone: r.fromServer ? .success : .warning)
                }
            }
            .padding(.horizontal, Space.lg).padding(.bottom, Space.md)

            ScrollView {
                VStack(spacing: Space.lg) {
                    if let r = app.shiftReport {
                        ShiftReportBreakdown(report: r, currency: currency)
                    } else {
                        ProgressView().padding(Space.xl)
                    }
                }
                .frame(maxWidth: 460).frame(maxWidth: .infinity)
                .padding(.horizontal, Space.lg).padding(.bottom, Space.lg)
            }

            VStack(spacing: Space.sm) {
                printControl
                SufrixButton(label: t("common.done"), variant: .outline) { onClose() }
            }
            .padding(Space.lg)
            .background(theme.colors.surface)
            .overlay(alignment: .top) { Rectangle().fill(theme.colors.border).frame(height: 1) }
        }
        .task { await app.loadShiftReport() }
    }

    @ViewBuilder private var printControl: some View {
        switch app.printState {
        case .printed:
            StatusChip(label: t("receipt.printed"), icon: "checkmark.circle", tone: .success).frame(maxWidth: .infinity)
        case .noPrinter:
            StatusChip(label: t("receipt.no_printer"), icon: "exclamationmark.triangle", tone: .warning).frame(maxWidth: .infinity)
        default:
            SufrixButton(
                label: app.printState == .failed ? t("receipt.print_failed") : t("shift.print_report"),
                icon: "printer",
                loading: app.printState == .printing
            ) {
                Task { await app.printShiftReport() }
            }
        }
    }
}
