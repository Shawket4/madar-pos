// Open-shift screen — the gate between sign-in and selling. Count the drawer's
// opening cash and open the shift (writes locally + queues; works offline).
import SwiftUI

struct OpenShiftView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    @State private var openingMinor: Int64 = 0

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            ScrollView {
                VStack(spacing: Space.xl) {
                    SufrixMark(size: 56)

                    VStack(spacing: Space.xs) {
                        Text(t("shift.open_title")).font(.ui(24, .heavy)).foregroundStyle(theme.colors.textPrimary)
                        Text(t("shift.opening_desc"))
                            .font(.ui(14)).foregroundStyle(theme.colors.textSecondary)
                            .multilineTextAlignment(.center).fixedSize(horizontal: false, vertical: true)
                    }

                    if let s = app.session {
                        StatusChip(label: "\(t("shift.signed_in_as")) \(s.displayName)", icon: "person", tone: .info)
                    }

                    VStack(alignment: .leading, spacing: Space.sm) {
                        Text(t("shift.opening_cash"))
                            .font(.ui(12, .semibold)).foregroundStyle(theme.colors.textMuted)
                        AmountField(amountMinor: $openingMinor, currencyCode: app.session?.currencyCode ?? "")
                    }

                    if let error = app.errorMessage {
                        NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .danger)
                    }

                    SufrixButton(label: t("shift.open_button"), loading: app.isBusy) {
                        Task { await app.openShift(openingCashMinor: openingMinor) }
                    }
                    Button(t("shift.switch_teller")) { app.signOut() }
                        .buttonStyle(.plain)
                        .font(.ui(12)).foregroundStyle(theme.colors.textMuted)
                }
                .frame(maxWidth: 380)
                .frame(maxWidth: .infinity)
                .padding(.horizontal, Space.xxl)
                .padding(.vertical, 48)
            }
        }
    }
}
