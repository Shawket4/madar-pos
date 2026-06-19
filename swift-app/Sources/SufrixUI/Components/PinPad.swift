// The teller PIN pad — the daily hero. Circular tap targets (matches Flutter
// `PinPad`): fixed-size keys in centered rows, a firm press scale (0.92), a
// whisper of shadow; progress dots spring from 12→14 and glow when filled.
import SwiftUI

struct PinPad: View {
    @Environment(\.theme) private var theme

    let pin: String
    var maxLength: Int = 6
    var keySize: CGFloat = 64
    let onDigit: (String) -> Void
    let onBackspace: () -> Void

    private let rows = [["1", "2", "3"], ["4", "5", "6"], ["7", "8", "9"], ["", "0", "⌫"]]

    var body: some View {
        VStack(spacing: Space.md) {
            dots
            ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                HStack(spacing: 14) {
                    ForEach(row, id: \.self) { cell($0) }
                }
            }
        }
    }

    private var dots: some View {
        HStack(spacing: Space.lg) {
            ForEach(0..<maxLength, id: \.self) { i in
                let filled = i < pin.count
                Circle()
                    .fill(filled ? theme.colors.accent : .clear)
                    .frame(width: filled ? 14 : 12, height: filled ? 14 : 12)
                    .overlay(Circle().strokeBorder(filled ? theme.colors.accent : theme.colors.border, lineWidth: 2))
                    .shadow(color: filled ? theme.colors.accent.opacity(0.3) : .clear, radius: 5, y: 1)
                    .animation(.spring(response: 0.3, dampingFraction: 0.6), value: filled)
            }
        }
        .padding(.bottom, Space.sm)
    }

    @ViewBuilder private func cell(_ key: String) -> some View {
        if key.isEmpty {
            Color.clear.frame(width: keySize, height: keySize)
        } else {
            Button {
                Haptics.selection()
                if key == "⌫" { onBackspace() } else { onDigit(key) }
            } label: {
                ZStack {
                    Circle().fill(theme.colors.surface)
                    Circle().strokeBorder(theme.colors.border, lineWidth: 1.5)
                    if key == "⌫" {
                        Image(systemName: "delete.left")
                            .font(.system(size: 22))
                            .foregroundStyle(theme.colors.textSecondary)
                    } else {
                        Text(key).font(.ui(22, .semibold)).foregroundStyle(theme.colors.textPrimary)
                    }
                }
                .frame(width: keySize, height: keySize)
                .shadow(color: theme.colors.shadow, radius: 4, y: 2)
            }
            .buttonStyle(.pressable(scale: 0.92))
        }
    }
}
