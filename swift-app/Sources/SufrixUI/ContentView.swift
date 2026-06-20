// Signed-in placeholder home. Proves the full auth round-trip from SwiftUI:
// it reads the cached session the core handed back and offers sign-out. Phase 6
// replaces this with the real Shift → Catalog/Order → Cart → Payment → Receipt
// layouts (PLAN §6).
import SwiftUI

struct ContentView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            VStack(spacing: Space.lg) {
                SufrixMark(size: 56)
                Text(t("home.signed_in")).font(.ui(24, .heavy)).foregroundStyle(theme.colors.textPrimary)

                if let s = app.session {
                    StatusChip(
                        label: s.online ? t("home.online") : t("home.offline"),
                        icon: s.online ? "wifi" : "wifi.slash",
                        tone: s.online ? .success : .warning
                    )
                    VStack(spacing: Space.sm) {
                        row(t("home.teller"), s.displayName)
                        row(t("home.role"), s.role)
                        row(t("home.currency"), s.currencyCode)
                    }
                    .padding(.top, Space.sm)
                }

                SufrixButton(label: t("home.sign_out"), variant: .danger, fullWidth: false) { app.signOut() }
                    .padding(.top, Space.sm)
            }
            .padding(Space.xxl)
            .frame(maxWidth: 360)
        }
    }

    private func row(_ label: String, _ value: String) -> some View {
        HStack {
            Text(label).font(.ui(13)).foregroundStyle(theme.colors.textSecondary)
            Spacer()
            Text(value).font(.ui(13, .semibold)).foregroundStyle(theme.colors.textPrimary)
        }
    }
}
