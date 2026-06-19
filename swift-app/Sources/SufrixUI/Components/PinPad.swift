// The teller PIN pad — the daily hero interaction. Glowing accent dots track
// progress; tactile keys (PressableScale + haptics). Mirrors Flutter `PinPad`.
import SwiftUI

struct PinPad: View {
    @Environment(\.theme) private var theme

    let pin: String
    var maxLength: Int = 6
    let onDigit: (String) -> Void
    let onBackspace: () -> Void

    var body: some View {
        VStack(spacing: Space.xl) {
            dots
            grid
        }
    }

    private var dots: some View {
        HStack(spacing: 12) {
            ForEach(0..<maxLength, id: \.self) { i in
                let filled = i < pin.count
                Circle()
                    .fill(filled ? theme.colors.accent : .clear)
                    .frame(width: 12, height: 12)
                    .overlay(Circle().strokeBorder(filled ? .clear : theme.colors.border, lineWidth: 1.5))
                    .shadow(color: filled ? theme.colors.accent.opacity(0.5) : .clear, radius: 6)
                    .animation(Motion.standard, value: filled)
            }
        }
    }

    private var grid: some View {
        let cols = Array(repeating: GridItem(.flexible(), spacing: 10), count: 3)
        return LazyVGrid(columns: cols, spacing: 10) {
            ForEach(1...9, id: \.self) { digitKey("\($0)") }
            Color.clear.frame(height: 56)
            digitKey("0")
            backspaceKey
        }
    }

    private func digitKey(_ d: String) -> some View {
        Button { Haptics.selection(); onDigit(d) } label: {
            Text(d)
                .font(.ui(21, .semibold))
                .foregroundStyle(theme.colors.textPrimary)
                .frame(maxWidth: .infinity)
                .frame(height: 56)
                .background(theme.colors.surfaceAlt)
                .overlay(
                    RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                        .strokeBorder(theme.colors.border, lineWidth: 1)
                )
                .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
        }
        .buttonStyle(.pressable)
    }

    private var backspaceKey: some View {
        Button { Haptics.selection(); onBackspace() } label: {
            Image(systemName: "delete.left")
                .font(.system(size: 20))
                .foregroundStyle(theme.colors.textMuted)
                .frame(maxWidth: .infinity)
                .frame(height: 56)
        }
        .buttonStyle(.pressable)
    }
}
