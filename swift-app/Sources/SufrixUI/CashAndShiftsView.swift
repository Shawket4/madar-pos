// Cash In/Out + Past Shifts — two online-only manager screens reached from the
// "More" drawer. Cash movements record a signed pay-in / pay-out against the open
// shift (never queued); Past Shifts lists the branch's shift history. All data +
// rules live in the core; these views collect input and render. Mirror of the
// Flutter cash-movements + shift-history screens.
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

    var body: some View {
        VStack(spacing: 0) {
            ScreenHeader(title: t("cash.title"), onClose: onClose)
            ScrollView {
                VStack(alignment: .leading, spacing: Space.lg) {
                    if let error = app.errorMessage {
                        NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .danger)
                    }
                    recordCard
                    movementsList
                }
                .frame(maxWidth: 520)
                .frame(maxWidth: .infinity)
                .padding(Space.lg)
            }
        }
        .background(theme.colors.bg.ignoresSafeArea())
        .task { await app.loadCashMovements() }
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

struct ShiftHistoryView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let onClose: () -> Void

    private var currency: String { app.session?.currencyCode ?? "" }

    var body: some View {
        VStack(spacing: 0) {
            ScreenHeader(title: t("shifts.title"), onClose: onClose)
            if app.shiftHistory.isEmpty {
                VStack(spacing: Space.md) {
                    Image(systemName: "clock.arrow.circlepath").font(.system(size: 36, weight: .light))
                        .foregroundStyle(theme.colors.textMuted)
                    Text(t("shifts.empty")).font(.ui(14)).foregroundStyle(theme.colors.textSecondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    VStack(spacing: Space.sm) {
                        ForEach(app.shiftHistory, id: \.id) { s in shiftRow(s) }
                    }
                    .frame(maxWidth: 560).frame(maxWidth: .infinity).padding(Space.lg)
                }
            }
        }
        .background(theme.colors.bg.ignoresSafeArea())
        .task { await app.loadShiftHistory() }
    }

    private func shiftRow(_ s: ShiftSummaryView) -> some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            HStack {
                Text(shortDate(s.openedAt)).font(.ui(14, .bold)).foregroundStyle(theme.colors.textPrimary)
                Spacer()
                StatusChip(label: s.isOpen ? t("shifts.open_now") : t("shifts.closed"),
                           tone: s.isOpen ? .success : .neutral)
            }
            metric(t("shifts.opening"), Money.format(s.openingCashMinor, currency))
            if let declared = s.closingDeclaredMinor {
                metric(t("shifts.declared"), Money.format(declared, currency))
            }
            if let disc = s.discrepancyMinor, disc != 0 {
                metric(t("shifts.discrepancy"),
                       "\(disc > 0 ? "+" : "−")\(Money.format(abs(disc), currency))",
                       valueColor: disc == 0 ? theme.colors.textSecondary : theme.colors.danger)
            }
        }
        .padding(Space.lg)
        .background(theme.colors.surface)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
            .strokeBorder(theme.colors.border, lineWidth: 1))
    }

    private func metric(_ label: String, _ value: String, valueColor: Color? = nil) -> some View {
        HStack {
            Text(label).font(.ui(12, .medium)).foregroundStyle(theme.colors.textSecondary)
            Spacer()
            Text(value).font(.money(13, .semibold)).foregroundStyle(valueColor ?? theme.colors.textPrimary)
        }
    }

    /// Trim an RFC3339 timestamp to "YYYY-MM-DD HH:MM" for the row title.
    private func shortDate(_ rfc: String) -> String {
        let s = rfc.replacingOccurrences(of: "T", with: " ")
        return String(s.prefix(16))
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
            .buttonStyle(.plain)
            Text(title).font(.ui(17, .heavy)).foregroundStyle(theme.colors.textPrimary)
            Spacer()
        }
        .padding(.horizontal, Space.lg).padding(.vertical, Space.md)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }
}
