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
                CloseShiftHeader { app.errorMessage = nil; app.showCloseShift = false }
                ScrollView {
                    VStack(spacing: Space.lg) {
                        if let s = app.shift { summaryCard(s) }
                        cashCard
                        if let r = app.shiftReport { reportCard(r) }
                        if app.shiftReport != nil {
                            // Preview the Z-report (paper layout) before printing — works
                            // with no printer; the Print lives inside the preview.
                            MadarButton(label: t("shift.print_report"), icon: "printer", variant: .outline) {
                                app.openShiftReportPreview()
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

    private func summaryCard(_ s: ShiftView) -> some View {
        Card {
            CardHeader(icon: "doc.text", title: t("shift.summary"))
            InfoRow(label: t("shift.teller"), value: s.tellerName)
            // Opening cash is money — give it the hero treatment (bold teal, tabular).
            InfoRow(label: t("shift.opening_cash"), value: Money.format(s.openingCashMinor, currency), money: true)
            InfoRow(label: t("shift.opened_at"), value: app.fmtDateTime(s.openedAt))
        }
    }

    private var cashCard: some View {
        Card {
            CardHeader(icon: "banknote", title: t("shift.counted_cash"))
            // System (expected) cash — the figure the count is measured against, so
            // it gets the hero money treatment in a tinted teal block (mirrors the
            // order screen's grand-total block).
            if let r = app.shiftReport {
                ExpectedCashBlock(expected: r.expectedCashMinor, currency: currency)
            }
            AmountField(amountMinor: $countedMinor, currencyCode: currency, autofocus: true)
            if let r = app.shiftReport {
                DiscrepancyBanner(declared: countedMinor, expected: r.expectedCashMinor, currency: currency)
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
}

// MARK: - Header

private struct CloseShiftHeader: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let onBack: () -> Void

    var body: some View {
        HStack(spacing: Space.md) {
            Button(action: onBack) {
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
}

// MARK: - Cash blocks

/// The system-expected cash — bold teal money in a tinted teal block, the figure
/// the declared count is reconciled against.
private struct ExpectedCashBlock: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let expected: Int64
    let currency: String

    var body: some View {
        HStack(spacing: Space.sm) {
            VStack(alignment: .leading, spacing: 2) {
                Text(t("shift.system_cash"))
                    .font(.ui(12, .bold)).foregroundStyle(theme.colors.accent)
                Text(t("shift.system_cash_explain"))
                    .font(.ui(11, .medium)).foregroundStyle(theme.colors.textMuted)
            }
            Spacer(minLength: Space.sm)
            Text(Money.format(expected, currency))
                .font(.money(20, .heavy)).foregroundStyle(theme.colors.accent)
        }
        .padding(.horizontal, Space.lg)
        .padding(.vertical, 14)
        .background(theme.colors.accentBg)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
    }
}

/// Live drawer variance — matches / over / short, toned to the result.
private struct DiscrepancyBanner: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let declared: Int64
    let expected: Int64
    let currency: String

    private var diff: Int64 { declared - expected }

    private var color: Color {
        diff == 0 ? theme.colors.success : (diff > 0 ? theme.colors.warning : theme.colors.danger)
    }
    private var bg: Color {
        diff == 0 ? theme.colors.successBg : (diff > 0 ? theme.colors.warningBg : theme.colors.dangerBg)
    }
    private var icon: String {
        diff == 0 ? "checkmark.circle" : (diff > 0 ? "arrow.up.circle" : "arrow.down.circle")
    }
    private var label: String {
        diff == 0
            ? t("shift.drawer_matches")
            : (diff > 0
               ? "\(t("shift.drawer_over")) \(Money.format(diff, currency))"
               : "\(t("shift.drawer_short")) \(Money.format(-diff, currency))")
    }

    var body: some View {
        HStack(spacing: 10) {
            MadarIcon(icon, size: 16).foregroundStyle(color)
            Text(label).font(.ui(13, .medium)).foregroundStyle(color)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .background(bg)
        .overlay(
            RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                .strokeBorder(color.opacity(Opacity.border), lineWidth: 1)
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
        HStack(spacing: Space.sm) {
            // Leading teal tone-tile behind the glyph — matches the confident
            // Kitchen/Order/Sync header (accentBg + accent icon, 34×34, Radii.sm).
            MadarIcon(icon, size: 18)
                .foregroundStyle(theme.colors.accent)
                .frame(width: 34, height: 34)
                .background(theme.colors.accentBg)
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
            Text(title).font(.ui(17, .bold)).foregroundStyle(theme.colors.textPrimary)
        }
    }
}

private struct InfoRow: View {
    @Environment(\.theme) private var theme
    let label: String
    let value: String
    var money: Bool = false
    var body: some View {
        HStack {
            Text(label).font(.ui(13)).foregroundStyle(theme.colors.textSecondary)
            Spacer()
            // Money values are the hero — bold teal, tabular figures; everything
            // else stays a quiet semibold primary.
            if money {
                Text(value).font(.money(14, .bold)).foregroundStyle(theme.colors.accent)
            } else {
                Text(value).font(.ui(13, .semibold)).foregroundStyle(theme.colors.textPrimary)
            }
        }
    }
}
