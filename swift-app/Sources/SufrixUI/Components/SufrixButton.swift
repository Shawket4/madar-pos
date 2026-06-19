// The shared button. Variants map to the Flutter `AppButton`; primary carries
// the terracotta/navy `primaryGlow`.
import SwiftUI

enum SufrixButtonVariant { case primary, secondary, ghost, danger }

struct SufrixButton: View {
    @Environment(\.theme) private var theme

    let label: String
    var icon: String? = nil
    var variant: SufrixButtonVariant = .primary
    var loading: Bool = false
    var fullWidth: Bool = true
    var height: CGFloat = 52
    let action: () -> Void

    var body: some View {
        Button {
            if !loading { Haptics.selection(); action() }
        } label: {
            HStack(spacing: Space.sm) {
                if loading {
                    ProgressView().controlSize(.small).tint(fg)
                } else {
                    if let icon { Image(systemName: icon).font(.system(size: 16, weight: .semibold)) }
                    Text(label).font(.ui(15.5, .bold))
                }
            }
            .frame(maxWidth: fullWidth ? .infinity : nil)
            .frame(height: height)
            .padding(.horizontal, Space.lg)
            .foregroundStyle(fg)
            .background(bg)
            .overlay(
                RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                    .strokeBorder(borderColor, lineWidth: variant == .secondary ? 1.5 : 0)
            )
            .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
            .shadow(color: glow, radius: 16, x: 0, y: 8)
            .opacity(loading ? 0.85 : 1)
        }
        .buttonStyle(.pressable)
        .disabled(loading)
    }

    private var fg: Color {
        switch variant {
        case .primary: return theme.colors.textOnAccent
        case .secondary: return theme.colors.accent
        case .ghost: return theme.colors.textSecondary
        case .danger: return theme.colors.danger
        }
    }
    private var bg: Color {
        switch variant {
        case .primary: return theme.colors.accent
        case .secondary, .ghost: return .clear
        case .danger: return theme.colors.dangerBg
        }
    }
    private var borderColor: Color { variant == .secondary ? theme.colors.accent : .clear }
    private var glow: Color { variant == .primary ? theme.colors.accent.opacity(0.34) : .clear }
}
