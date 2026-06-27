// A big, tabular money input — the opening-cash / tender hero field. Edits a
// major-unit decimal string and reports `amountMinor` (minor units).
import SwiftUI

struct AmountField: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    @FocusState private var focused: Bool

    @Binding var amountMinor: Int64
    var currencyCode: String
    /// Raise the keyboard on appear (the hero count field; off by default so
    /// existing call sites are unchanged).
    var autofocus: Bool = false

    @State private var text = ""
    /// The last value WE emitted — distinguishes an external prefill from the
    /// teller's own typing, so prefills reflect without a focus race.
    @State private var lastEmitted: Int64 = 0

    var body: some View {
        // One contained row: a muted currency prefix on the left, the big tabular
        // amount filling the rest. (Was a tiny label stacked over a 34pt number,
        // which sprawled and read as unbalanced.)
        HStack(spacing: Space.sm) {
            Text(currencyCode.uppercased())
                .font(.ui(15, .bold))
                .foregroundStyle(theme.colors.textMuted)
            TextField("0.00", text: Binding(
                get: { text },
                set: { newValue in
                    text = newValue
                    let minor = Self.toMinor(newValue)
                    lastEmitted = minor
                    amountMinor = minor
                }
            ))
            .font(.money(28, .heavy))
            .textFieldStyle(.plain) // suppress macOS's default rounded-border bezel
            .foregroundStyle(theme.colors.textPrimary)
            .focused($focused)
            #if os(iOS)
            .keyboardType(.decimalPad)
            // The decimalPad has no return key — give it a Done dismiss.
            .toolbar {
                ToolbarItemGroup(placement: .keyboard) {
                    Spacer()
                    Button(t("common.done")) { focused = false }
                }
            }
            #endif
        }
        .padding(.horizontal, Space.lg)
        .frame(height: 64)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(theme.colors.surface)
        .overlay(
            RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                .strokeBorder(focused ? theme.colors.accent : theme.colors.border, lineWidth: focused ? 2 : 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
        .animation(Motion.standard, value: focused)
        .onAppear {
            // Seed the display from any pre-set amount (e.g. carried-over opening).
            lastEmitted = amountMinor
            if text.isEmpty && amountMinor != 0 { text = Self.toText(amountMinor) }
            // Let the navigation transition settle before raising the pad.
            if autofocus {
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) { focused = true }
            }
        }
        .onChange(of: amountMinor) { newValue in
            // Reflect an EXTERNAL amount change (a carried-over prefill arriving
            // async) without clobbering the teller's own typing.
            if newValue != lastEmitted {
                text = newValue == 0 ? "" : Self.toText(newValue)
                lastEmitted = newValue
            }
        }
    }

    /// Parse a major-unit decimal string ("500", "499.50") → minor units.
    static func toMinor(_ s: String) -> Int64 {
        let cleaned = s.filter { $0.isNumber || $0 == "." }
        let major = Double(cleaned) ?? 0
        return Int64((major * 100).rounded())
    }

    /// Render minor units as a major-unit string ("48000" → "480", "49950" → "499.50").
    static func toText(_ minor: Int64) -> String {
        let major = Double(minor) / 100
        return major == major.rounded() ? String(Int64(major)) : String(format: "%.2f", major)
    }
}
