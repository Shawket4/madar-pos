// Status chip + inline notice banner — both keyed off a semantic `ChipTone`,
// mirroring the Flutter `StatusChip` / `_NoticeBanner`.
import SwiftUI

enum ChipTone { case info, success, warning, danger, neutral }

extension ChipTone {
    func fg(_ c: SufrixColors) -> Color {
        switch self {
        case .info: return c.navy
        case .success: return c.success
        case .warning: return c.warning
        case .danger: return c.danger
        case .neutral: return c.textSecondary
        }
    }
    func bg(_ c: SufrixColors) -> Color {
        switch self {
        case .info: return c.navyBg
        case .success: return c.successBg
        case .warning: return c.warningBg
        case .danger: return c.dangerBg
        case .neutral: return c.surfaceAlt
        }
    }
}

struct StatusChip: View {
    @Environment(\.theme) private var theme
    let label: String
    var icon: String? = nil
    var tone: ChipTone = .info

    var body: some View {
        HStack(spacing: 6) {
            if let icon {
                Image(systemName: icon).font(.system(size: 12, weight: .semibold))
            } else {
                Circle().fill(tone.fg(theme.colors)).frame(width: 6, height: 6)
            }
            Text(label).font(.ui(12, .semibold))
        }
        .foregroundStyle(tone.fg(theme.colors))
        .padding(.horizontal, 11)
        .padding(.vertical, 6)
        .background(tone.bg(theme.colors))
        .clipShape(Capsule())
    }
}

struct NoticeBanner: View {
    @Environment(\.theme) private var theme
    let icon: String
    let text: String
    var tone: ChipTone = .warning
    var bold: Bool = false

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: icon).font(.system(size: 16)).foregroundStyle(tone.fg(theme.colors))
            Text(text)
                .font(.ui(13, bold ? .bold : .medium))
                .foregroundStyle(tone.fg(theme.colors))
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(tone.bg(theme.colors))
        .overlay(
            RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                .strokeBorder(tone.fg(theme.colors).opacity(0.25), lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
    }
}
