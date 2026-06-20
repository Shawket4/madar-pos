// Open-shift — the gate between sign-in and selling, and the continuation of the
// login moment: login confirms WHO you are, this confirms WHAT'S in the drawer.
// A name-first greeting, one isolated hero count field (auto-focused), and a
// single loud primary. On iPad/desktop it splits into the same BrandPanel as
// login; on iPhone it's one calm centered column.
import SwiftUI

struct OpenShiftView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme

    @State private var openingMinor: Int64 = 0

    var body: some View {
        GeometryReader { geo in
            let wide = geo.size.width >= 760
            ZStack {
                theme.colors.bg.ignoresSafeArea()
                if wide {
                    HStack(spacing: 0) {
                        BrandPanel().frame(width: geo.size.width * 0.5)
                        formColumn(showLogo: false)
                    }
                } else {
                    formColumn(showLogo: true)
                }
            }
        }
    }

    @ViewBuilder private func formColumn(showLogo: Bool) -> some View {
        ScrollView {
            OpenShiftForm(app: app, openingMinor: $openingMinor, showLogo: showLogo)
                .frame(maxWidth: 400)
                .frame(maxWidth: .infinity)
                .padding(.horizontal, Space.xxl)
                .padding(.vertical, 48)
        }
        #if os(iOS)
        .scrollDismissesKeyboard(.interactively)
        #endif
    }
}

private struct OpenShiftForm: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    @Binding var openingMinor: Int64
    var showLogo: Bool

    var body: some View {
        // spacing 0 + explicit per-gap padding → deliberate hierarchy (the hero
        // count field gets the isolating Space.xxl gap above it).
        VStack(spacing: 0) {
            if showLogo { SufrixMark(size: 56) }

            // ── Greeting (the teller's name IS the hero) ──────────────────────
            VStack(spacing: Space.xs) {
                Text(t("shift.welcome"))
                    .font(.ui(15, .medium)).foregroundStyle(theme.colors.textSecondary)
                Text(app.session?.displayName ?? t("shift.open_title"))
                    .font(.ui(28, .heavy)).foregroundStyle(theme.colors.textPrimary)
                    .multilineTextAlignment(.center)
                if !app.branchName.isEmpty {
                    StatusChip(label: app.branchName, icon: "building.2", tone: .info)
                        .padding(.top, Space.xs)
                }
            }
            .padding(.top, showLogo ? Space.xl : 0)

            // ── Hero count field (the one thing the teller must do) ───────────
            VStack(spacing: Space.md) {
                Text(t("shift.opening_cash"))
                    .font(.ui(13, .semibold)).foregroundStyle(theme.colors.textSecondary)
                    .frame(maxWidth: .infinity)
                AmountField(amountMinor: $openingMinor,
                            currencyCode: app.session?.currencyCode ?? "",
                            autofocus: true)
                Text(t("shift.opening_hint"))
                    .font(.ui(12)).foregroundStyle(theme.colors.textMuted)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(.top, Space.xxl)

            // ── Error (next to the action that triggers it) ───────────────────
            if let error = app.errorMessage {
                NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .danger)
                    .padding(.top, Space.xl)
            }

            // ── Primary action ───────────────────────────────────────────────
            SufrixButton(label: t("shift.open_button"), icon: "lock.open", loading: app.isBusy) {
                Task { await app.openShift(openingCashMinor: openingMinor) }
            }
            .padding(.top, app.errorMessage == nil ? Space.xl : Space.md)

            // ── Recessive exit ───────────────────────────────────────────────
            SufrixButton(label: t("shift.switch_teller"), variant: .ghost) { app.signOut() }
                .padding(.top, Space.sm)
        }
    }
}
