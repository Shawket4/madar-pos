// The shared button — matches the Flutter `AppButton`: flat, radius-sm, a tactile
// press scale, weight-700 label. No glow (the Flutter button is flat; boldness
// comes from the palette + type, not effects).
import SwiftUI

enum SufrixButtonVariant { case primary, outline, ghost, danger }

struct SufrixButton: View {
    @Environment(\.theme) private var theme

    let label: String
    var icon: String? = nil
    var variant: SufrixButtonVariant = .primary
    var loading: Bool = false
    var fullWidth: Bool = true
    var height: CGFloat = 50
    let action: () -> Void

    private var enabled: Bool { !loading }

    var body: some View {
        Button {
            if enabled { Haptics.selection(); action() }
        } label: {
            content
                .frame(maxWidth: fullWidth ? .infinity : nil)
                .frame(height: height)
                .padding(.horizontal, Space.lg)
                .background(background)
                .overlay(
                    RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                        .strokeBorder(borderColor, lineWidth: variant == .outline ? 1.5 : 0)
                )
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        }
        .buttonStyle(.pressable(scale: 0.975))
        .disabled(loading)
    }

    @ViewBuilder private var content: some View {
        if loading {
            ProgressView().controlSize(.small).tint(fg)
        } else {
            HStack(spacing: Space.sm) {
                if let icon { Image(systemName: icon).font(.system(size: 17, weight: .semibold)) }
                Text(label).font(.ui(14, .bold))
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
    private var background: Color {
        let base: Color
        switch variant {
        case .primary: base = theme.colors.accent
        case .danger: base = theme.colors.danger
        case .outline, .ghost: base = .clear
        }
        return enabled ? base : base.opacity(0.45)
    }
    private var borderColor: Color { variant == .outline ? theme.colors.accent : .clear }
}
