// Brand mark + wordmark — the REAL vector assets (Icon/Logo), recolored per
// theme via the asset catalog's light/dark variants (just like Flutter's
// SufrixLongLogo recolors). Never redrawn.
//
// Assets live in swift-app/Resources/Assets.xcassets (SufrixMark / SufrixWordmark);
// the Xcode app compiles the catalog, the macOS run bundles it via actool.
import SwiftUI

/// The pinwheel mark (no square): navy arms on light, cream on dark, terracotta dot.
struct SufrixMark: View {
    var size: CGFloat = 44

    var body: some View {
        Image("SufrixMark")
            .resizable()
            .interpolation(.high)
            .scaledToFit()
            .frame(width: size, height: size)
    }
}

/// The full "Sufrix" wordmark (navy on light, cream on dark, terracotta "i"-dot).
struct SufrixLockup: View {
    var height: CGFloat = 30

    var body: some View {
        Image("SufrixWordmark")
            .resizable()
            .interpolation(.high)
            .aspectRatio(contentMode: .fit)
            .frame(height: height)
    }
}
