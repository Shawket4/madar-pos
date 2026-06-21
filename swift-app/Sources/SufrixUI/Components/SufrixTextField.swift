// Shared text field — matches the Flutter InputDecorationTheme: filled with
// `surface`, sm radius, 1pt border that thickens to a 2pt accent ring on focus,
// h16/v14 content padding, muted hint. Optional leading SF Symbol.
import SwiftUI

struct SufrixTextField: View {
    @Environment(\.theme) private var theme
    @FocusState private var focused: Bool

    let placeholder: String
    @Binding var text: String
    var icon: String? = nil
    var secure: Bool = false
    var disabled: Bool = false
    var keyboard: SufrixKeyboard = .standard
    var caps: SufrixCaps = .none

    var body: some View {
        HStack(spacing: Space.md) {
            if let icon {
                Image(systemName: icon)
                    .font(.system(size: 17))
                    .foregroundStyle(focused ? theme.colors.accent : theme.colors.textMuted)
            }
            field
                .font(.ui(15))
                .foregroundStyle(theme.colors.textPrimary)
                .focused($focused)
                .disabled(disabled)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(theme.colors.surface)
        .overlay(
            RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                .strokeBorder(focused ? theme.colors.accent : theme.colors.border,
                              lineWidth: focused ? 2 : 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        .opacity(disabled ? 0.6 : 1)
        .animation(Motion.standard, value: focused)
    }

    @ViewBuilder private var field: some View {
        // `.plain` is essential: on macOS the default TextField/SecureField style is
        // `.roundedBorder`, which draws its OWN bezel inside our custom border —
        // that's the "multiple outlines" look. Plain lets our overlay be the only ring.
        let base = Group {
            if secure { SecureField(placeholder, text: $text) }
            else { TextField(placeholder, text: $text) }
        }
        .textFieldStyle(.plain)
        #if os(iOS)
        base
            .keyboardType(uiKeyboard)
            .textInputAutocapitalization(caps == .words ? .words : .never)
            .autocorrectionDisabled()
        #else
        base
        #endif
    }

    #if os(iOS)
    private var uiKeyboard: UIKeyboardType {
        switch keyboard {
        case .standard: return .default
        case .email: return .emailAddress
        case .number: return .numberPad
        }
    }
    #endif
}

enum SufrixKeyboard { case standard, email, number }
enum SufrixCaps { case none, words }
