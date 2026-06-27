// The shared button — solid, generously tall (54), md radius, weight-700 label.
// The primary variant carries a soft accent GLOW so the call-to-action reads as
// the brightest thing on screen (the rebuild's "depth via shadow + primaryGlow"
// direction). Outline/ghost stay flat.
import SwiftUI

enum MadarButtonVariant { case primary, outline, ghost, danger }

struct MadarButton: View {
    @Environment(\.theme) private var theme

    let label: String
    var icon: String? = nil
    var variant: MadarButtonVariant = .primary
    var loading: Bool = false
    /// Caller-controlled enable gate (mirrors the Compose `MadarButton`). When
    /// false the button dims, drops its glow, and ignores taps — so call sites can
    /// `MadarButton(..., enabled: canRecord)` instead of bolting on external
    /// `.opacity`/`.allowsHitTesting` (the cross-platform asymmetry the audit flagged).
    var isEnabled: Bool = true
    var fullWidth: Bool = true
    var height: CGFloat = Metric.buttonHeight
    let action: () -> Void

    private var active: Bool { isEnabled && !loading }
    private var isSolid: Bool { variant == .primary || variant == .danger }

    var body: some View {
        Button {
            if active { Haptics.selection(); action() }
        } label: {
            content
                .frame(maxWidth: fullWidth ? .infinity : nil)
                .frame(height: height)
                .padding(.horizontal, Space.lg)
                .background(background)
                .overlay(
                    RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                        .strokeBorder(borderColor, lineWidth: variant == .outline ? 1.5 : 0)
                )
                .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
                .elevation(variant == .primary && active ? .glow : .none)
        }
        .buttonStyle(.pressable(scale: 0.97))
        .disabled(!active)
    }

    @ViewBuilder private var content: some View {
        if loading {
            ProgressView().controlSize(.small).tint(fg)
        } else {
            HStack(spacing: Space.sm) {
                if let icon { MadarIcon(icon, size: IconSize.md) }
                Text(label).font(.ui(15, .bold)).kerning(0.2)
            }
            .foregroundStyle(fg)
        }
    }

    private var fg: Color {
        switch variant {
        case .primary: return theme.colors.textOnAccent
        case .danger: return .white
        case .outline: return theme.colors.accent
        case .ghost: return theme.colors.textSecondary
        }
    }
    // Premium fill: the primary CTA gets a soft top-lit sheen (a white gradient
    // fading over the top edge) so it reads as a lifted, glossy surface rather
    // than a flat slab — paired with the accent glow. Mirrors the Compose gradient.
    @ViewBuilder private var background: some View {
        switch variant {
        case .primary:
            if active {
                theme.colors.accent
                    .overlay(LinearGradient(colors: [.white.opacity(0.18), .clear],
                                            startPoint: .top, endPoint: .center))
            } else {
                theme.colors.accent.opacity(Opacity.disabled)
            }
        case .danger:
            active ? theme.colors.danger : theme.colors.danger.opacity(Opacity.disabled)
        case .outline, .ghost:
            Color.clear
        }
    }
    private var borderColor: Color { variant == .outline ? theme.colors.accent : .clear }
}
