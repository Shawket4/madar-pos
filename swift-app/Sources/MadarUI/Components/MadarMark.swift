// Brand mark + wordmark — the REAL vector assets (Icon/Logo), recolored per
// theme via the asset catalog's light/dark variants (just like Flutter's
// MadarLongLogo recolors). Never redrawn.
//
// Assets live in swift-app/Resources/Assets.xcassets (MadarMark / MadarWordmark);
// the Xcode app compiles the catalog, the macOS run bundles it via actool.
import SwiftUI

/// The pinwheel mark (no square): navy arms on light, cream on dark, terracotta dot.
struct MadarMark: View {
    var size: CGFloat = 44

    var body: some View {
        Image("MadarMark")
            .resizable()
            .interpolation(.high)
            .scaledToFit()
            .frame(width: size, height: size)
    }
}

/// The full "Madar" wordmark (navy on light, cream on dark, terracotta "i"-dot).
struct MadarLockup: View {
    var height: CGFloat = 30

    var body: some View {
        Image("MadarWordmark")
            .resizable()
            .interpolation(.high)
            .aspectRatio(contentMode: .fit)
            .frame(height: height)
    }
}
