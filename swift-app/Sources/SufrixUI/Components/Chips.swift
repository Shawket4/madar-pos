// Status chip + inline notice banner — keyed off a semantic `ChipTone`. Matches
// the Flutter `StatusChip` (bordered pill, w700/11 label) and `_NoticeBanner`.
import SwiftUI

enum ChipTone { case info, accent, success, warning, danger, neutral }

extension ChipTone {
    func fg(_ c: SufrixColors) -> Color {
        switch self {
        case .info: return c.navy
        case .accent: return c.accent
        case .success: return c.success
        case .warning: return c.warning
        case .danger: return c.danger
        case .neutral: return c.textSecondary
        }
    }
    func bg(_ c: SufrixColors) -> Color {
        switch self {
        case .info: return c.navyBg
        case .accent: return c.accentBg
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
    var tone: ChipTone = .neutral

    var body: some View {
        let fg = tone.fg(theme.colors)
        HStack(spacing: 5) {
            if let icon { Image(systemName: icon).font(.system(size: 12, weight: .semibold)) }
            Text(label).font(.ui(11, .bold))
        }
        .foregroundStyle(fg)
        .padding(.horizontal, 10)
        .padding(.vertical, 5)
        .background(tone.bg(theme.colors))
        .overlay(Capsule().strokeBorder(fg.opacity(0.25), lineWidth: 1))
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
        let fg = tone.fg(theme.colors)
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: icon).font(.system(size: 16)).foregroundStyle(fg)
            Text(text)
                .font(.ui(13, bold ? .bold : .medium))
                .foregroundStyle(fg)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(tone.bg(theme.colors))
        .overlay(
            RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                .strokeBorder(fg.opacity(0.25), lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
    }
}
