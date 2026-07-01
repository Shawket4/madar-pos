// The brand half of the wide (iPad / desktop) split layout — shared by Login and
// Open-Shift so the two screens read as one continuous onboarding act. Cream
// surface, a faded watermark mark, the lockup, the headline/tagline, and a quiet
// footer. All colors come from the theme tokens.
import SwiftUI

struct BrandPanel: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    @Environment(\.layoutDirection) private var dir

    var body: some View {
        ZStack {
            theme.colors.surfaceAlt.ignoresSafeArea()
            // Faded watermark mark.
            MadarMark(size: 360)
                .opacity(0.05)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
                .offset(x: dir == .rightToLeft ? -80 : 80, y: 80)
                .clipped()

            VStack(alignment: .leading, spacing: 0) {
                MadarLockup(height: 28)
                Spacer()
                Text(t("brand.headline"))
                    .font(.ui(44, .black))
                    .foregroundStyle(theme.colors.textPrimary)
                    .lineSpacing(6)
                Text(t("brand.tagline"))
                    .font(.ui(15)).foregroundStyle(theme.colors.textSecondary)
                    .lineSpacing(4)
                    .frame(maxWidth: 300, alignment: .leading)
                    .padding(.top, Space.lg)
                Spacer()
                HStack(spacing: Space.sm) {
                    Circle().fill(theme.colors.accent).frame(width: 6, height: 6)
                    Text("© 2026 Madar").font(.ui(12)).foregroundStyle(theme.colors.textMuted)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
            .padding(48)
        }
    }
}
