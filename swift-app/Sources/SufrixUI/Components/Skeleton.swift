// Skeleton placeholders — gently pulsing blocks shown while a list loads, in
// place of a bare spinner. Used by the order-history and shift-history screens.
import SwiftUI

/// A single rounded placeholder bar that pulses its opacity.
struct SkeletonBlock: View {
    @Environment(\.theme) private var theme
    var width: CGFloat? = nil
    var height: CGFloat = 13
    var corner: CGFloat = 6
    @State private var dim = false

    var body: some View {
        RoundedRectangle(cornerRadius: corner, style: .continuous)
            .fill(theme.colors.surfaceAlt)
            .frame(width: width, height: height)
            .opacity(dim ? 0.5 : 1)
            .onAppear {
                withAnimation(.easeInOut(duration: 0.9).repeatForever(autoreverses: true)) { dim = true }
            }
    }
}

/// A card-shaped skeleton standing in for one list row (title + meta + amount).
struct SkeletonRow: View {
    @Environment(\.theme) private var theme

    var body: some View {
        HStack(spacing: Space.md) {
            VStack(alignment: .leading, spacing: 8) {
                SkeletonBlock(width: 130, height: 14)
                SkeletonBlock(width: 80, height: 11)
            }
            Spacer()
            SkeletonBlock(width: 56, height: 14)
        }
        .padding(Space.md)
        .background(theme.colors.surface)
        .overlay(
            RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                .strokeBorder(theme.colors.border, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
    }
}

/// A column of `count` skeleton rows — the loading state for a list screen.
struct SkeletonList: View {
    var count: Int = 6

    var body: some View {
        VStack(spacing: Space.sm) {
            ForEach(0..<count, id: \.self) { _ in SkeletonRow() }
        }
        .frame(maxWidth: 560)
        .frame(maxWidth: .infinity)
        .padding(Space.lg)
    }
}
