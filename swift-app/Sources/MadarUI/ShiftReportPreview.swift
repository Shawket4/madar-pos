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
            // Opening float (drawer carry-over) — the base the Expected cash builds
            // on. Flutter's Z-report Section A shows it explicitly; mirror that.
            totalRow(t("shift.opening_cash"), Money.format(report.openingCashMinor, currency))
            // Opening mismatch — the counted opening float differed from the
            // suggested (last close); show the signed difference + the reason.
            if report.openingCashWasEdited {
                if let orig = report.openingCashOriginalMinor {
                    let diff = report.openingCashMinor - orig
                    totalRow(
                        t("shift.opening_mismatch"),
                        (diff < 0 ? "−" : "+") + Money.format(abs(diff), currency),
                        tone: diff == 0 ? nil : theme.colors.warning
                    )
                }
                if let reason = report.openingCashEditReason, !reason.isEmpty {
                    Text("\(t("shift.opening_reason_label")): \(reason)")
                        .font(.ui(11, .regular))
                        .foregroundStyle(theme.colors.textSecondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            totalRow(t("shift.expected_cash"), Money.format(report.expectedCashMinor, currency), emphasized: true)
            // Reconciliation — counted drawer + over/short, once the shift is closed
            // (declared cash present). Mirrors the printed Z-report.
            if let declared = report.closingCashDeclaredMinor {
                totalRow(t("shift.counted_cash"), Money.format(declared, currency), emphasized: true)
                let diff = report.expectedCashMinor - declared
                if diff == 0 {
                    totalRow(t("shift.difference"), Money.format(0, currency), tone: theme.colors.success)
                } else if diff > 0 {
                    totalRow(t("shift.drawer_short"), Money.format(diff, currency), tone: theme.colors.danger)
                } else {
                    totalRow(t("shift.drawer_over"), Money.format(-diff, currency), tone: theme.colors.warning)
                }
            }
        }
    }

    private func methodRow(_ p: ShiftReportPaymentLine) -> some View {
        VStack(spacing: 5) {
            HStack {
                HStack(spacing: 6) {
                    MadarIcon(p.isCash ? "banknote" : "creditcard", size: 12).foregroundStyle(theme.colors.textMuted)
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
    /// A specific PAST shift's report (from Past Shifts), or nil to show the current
    /// shift's report (loaded on appear). The preview is fully on-screen → no printer
    /// needed.
    var report: ShiftReportView? = nil
    let onClose: () -> Void

    private var currency: String { app.session?.currencyCode ?? "" }
    private var shown: ShiftReportView? { report ?? app.shiftReport }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(t("shift.report_title")).font(.ui(18, .bold)).foregroundStyle(theme.colors.textPrimary)
                    if let teller = report?.tellerName ?? app.shift?.tellerName {
                        Text(teller).font(.ui(13)).foregroundStyle(theme.colors.textSecondary)
                    }
                }
                Spacer()
                if let r = shown {
                    StatusChip(label: r.fromServer ? t("chrome.online") : t("chrome.offline"),
                               tone: r.fromServer ? .success : .warning)
                }
            }
            .padding(.horizontal, Space.lg).padding(.vertical, Space.md)
            .background(theme.colors.surface)
            .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }

            ScrollView {
                VStack(spacing: Space.lg) {
                    if let r = shown {
                        ShiftReportBreakdown(report: r, currency: currency)
                    } else {
                        ProgressView().padding(Space.xl)
                    }
                }
                .frame(maxWidth: 460).frame(maxWidth: .infinity)
                .padding(.horizontal, Space.lg).padding(.top, Space.lg).padding(.bottom, Space.xl)
            }

            VStack(spacing: Space.sm) {
                printControl
                MadarButton(label: t("common.done"), variant: .outline) { onClose() }
            }
            .padding(Space.lg)
            .background(theme.colors.surface)
            .overlay(alignment: .top) { Rectangle().fill(theme.colors.border).frame(height: 1) }
        }
        .task { if report == nil { await app.loadShiftReport() } }
    }

    @ViewBuilder private var printControl: some View {
        switch app.printState {
        case .printed:
            StatusChip(label: t("receipt.printed"), icon: "checkmark.circle", tone: .success).frame(maxWidth: .infinity)
        case .noPrinter:
            StatusChip(label: t("receipt.no_printer"), icon: "exclamationmark.triangle", tone: .warning).frame(maxWidth: .infinity)
        default:
            MadarButton(
                label: app.printState == .failed ? t("receipt.print_failed") : t("shift.print_report"),
                icon: "printer",
                loading: app.printState == .printing
            ) {
                Task { if let r = shown { await app.printReportView(r) } }
            }
        }
    }
}
