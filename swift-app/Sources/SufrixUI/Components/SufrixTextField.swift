// Shared text field — tokenized, with an optional leading SF Symbol. Mirrors the
// Flutter input decoration (filled surface, sm radius, muted icon). The keyboard
// / capitalization hints are platform-agnostic in the API and only applied on iOS.
import SwiftUI

enum SufrixKeyboard { case standard, email, number }
enum SufrixCaps { case none, words }

struct SufrixTextField: View {
    @Environment(\.theme) private var theme

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
                    .foregroundStyle(theme.colors.textMuted)
            }
            field
                .font(.ui(15))
                .foregroundStyle(theme.colors.textPrimary)
                .disabled(disabled)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 13)
        .background(theme.colors.surfaceAlt)
        .overlay(
            RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                .strokeBorder(theme.colors.border, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        .opacity(disabled ? 0.6 : 1)
    }

    @ViewBuilder private var field: some View {
        let base = Group {
            if secure { SecureField(placeholder, text: $text) }
            else { TextField(placeholder, text: $text) }
        }
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
