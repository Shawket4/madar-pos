// Brand mark + lockup. The 4-blade pinwheel + terracotta center dot echoes the
// real `Icon.svg`; the wordmark renders "Sufrix" in Cairo with the terracotta
// "i"-dot accent.
//
// NOTE: this is a faithful geometric stand-in so the screens are self-contained
// and compile-verifiable. The shipping apps drop in the real vector assets
// (Icon.svg / Logo.svg) via the iOS asset catalog — swap `body` for `Image(...)`.
import SwiftUI

struct SufrixMark: View {
    @Environment(\.theme) private var theme
    var size: CGFloat = 44
    /// Override the blade color (defaults to navy on light / cream on dark).
    var armColor: Color? = nil
    var dotColor: Color = SufrixBrand.terracotta

    var body: some View {
        let arm = armColor ?? (theme.isDark ? SufrixBrand.cream : SufrixBrand.navy)
        ZStack {
            ForEach(0..<4, id: \.self) { i in
                ZStack {
                    RoundedRectangle(cornerRadius: size * 0.06, style: .continuous)
                        .fill(arm)
                        .frame(width: size * 0.40, height: size * 0.19)
                        .offset(x: size * 0.17)
                }
                .frame(width: size, height: size)
                .rotationEffect(.degrees(Double(i) * 90 + 45))
            }
            Circle().fill(dotColor).frame(width: size * 0.17, height: size * 0.17)
        }
        .frame(width: size, height: size)
    }
}

/// Mark + "Sufrix" wordmark, for headers and the brand panel.
struct SufrixLockup: View {
    @Environment(\.theme) private var theme
    var markSize: CGFloat = 30
    var textSize: CGFloat = 26
    /// Optional explicit text color (defaults to the theme's primary text).
    var textColor: Color? = nil

    var body: some View {
        HStack(spacing: Space.md) {
            SufrixMark(size: markSize)
            Text("Sufrix")
                .font(.ui(textSize, .heavy))
                .foregroundStyle(textColor ?? theme.colors.textPrimary)
        }
    }
}
