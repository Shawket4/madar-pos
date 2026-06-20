// A big, tabular money input — the opening-cash / tender hero field. Edits a
// major-unit decimal string and reports `amountMinor` (minor units).
import SwiftUI

struct AmountField: View {
    @Environment(\.theme) private var theme
    @FocusState private var focused: Bool

    @Binding var amountMinor: Int64
    var currencyCode: String

    @State private var text = ""

    var body: some View {
        VStack(spacing: 4) {
            Text(currencyCode.uppercased())
                .font(.ui(12, .semibold))
                .foregroundStyle(theme.colors.textMuted)
            TextField("0.00", text: Binding(
                get: { text },
                set: { newValue in
                    text = newValue
                    amountMinor = Self.toMinor(newValue)
                }
            ))
            .font(.money(34, .heavy))
            .multilineTextAlignment(.center)
            .foregroundStyle(theme.colors.textPrimary)
            .focused($focused)
            #if os(iOS)
            .keyboardType(.decimalPad)
            #endif
        }
        .padding(.vertical, Space.lg)
        .frame(maxWidth: .infinity)
        .background(theme.colors.surface)
        .overlay(
            RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                .strokeBorder(focused ? theme.colors.accent : theme.colors.border, lineWidth: focused ? 2 : 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
        .animation(Motion.standard, value: focused)
    }

    /// Parse a major-unit decimal string ("500", "499.50") → minor units.
    static func toMinor(_ s: String) -> Int64 {
        let cleaned = s.filter { $0.isNumber || $0 == "." }
        let major = Double(cleaned) ?? 0
        return Int64((major * 100).rounded())
    }
}
