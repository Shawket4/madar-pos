// Order screen — placeholder. The shift is open; the catalog + cart land next.
// Per the design language the order screen's action bar is the only nav hub.
import SwiftUI

struct OrderView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            VStack(spacing: Space.lg) {
                SufrixMark(size: 56)
                Text(t("order.title")).font(.ui(24, .heavy)).foregroundStyle(theme.colors.textPrimary)

                if let s = app.shift {
                    StatusChip(label: "\(s.tellerName) · \(t("home.online"))", icon: "clock", tone: .success)
                    Text("\(s.currencyDisplay(app.session?.currencyCode ?? ""))")
                        .font(.money(20, .bold)).foregroundStyle(theme.colors.textPrimary)
                }

                Text(t("order.coming_soon"))
                    .font(.ui(14)).foregroundStyle(theme.colors.textSecondary)
                    .multilineTextAlignment(.center)

                SufrixButton(label: t("home.sign_out"), variant: .ghost, fullWidth: false) { app.signOut() }
                    .padding(.top, Space.sm)
            }
            .frame(maxWidth: 380)
            .padding(Space.xxl)
        }
    }
}

extension ShiftView {
    /// "EGP 500.00" — opening cash, formatted from minor units.
    func currencyDisplay(_ code: String) -> String {
        let major = Double(openingCashMinor) / 100
        return "\(code.uppercased()) \(String(format: "%.2f", major))"
    }
}
