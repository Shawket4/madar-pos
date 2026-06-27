// Shared layout primitives — extracted from patterns that recurred inline across
// many screens (cards, section labels, screen headers, label↔value rows,
// selectable chips, empty states). One definition here keeps spacing, typography
// and tone consistent everywhere and mirrors the Compose `ui/SharedComponents.kt`
// 1:1. Built only from tokens (Space/Radii/IconSize/Metric/Typo) + theme colors.
import SwiftUI

// MARK: - MadarCard — bordered surface container (Flutter SurfaceCard)
struct MadarCard<Content: View>: View {
    @Environment(\.theme) private var theme
    var padding: CGFloat = Space.lg
    var radius: CGFloat = Radii.lg
    var spacing: CGFloat = Space.md
    var alignment: HorizontalAlignment = .leading
    var elevated: Bool = true
    @ViewBuilder var content: () -> Content

    var body: some View {
        VStack(alignment: alignment, spacing: spacing) { content() }
            .padding(padding)
            .frame(maxWidth: .infinity, alignment: alignment == .leading ? .leading : .center)
            .background(theme.colors.surface)
            .clipShape(RoundedRectangle(cornerRadius: radius, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: radius, style: .continuous)
                    .strokeBorder(theme.colors.borderLight, lineWidth: 1)
            )
            .elevation(elevated ? .card : .none)
    }
}

// MARK: - SectionHeader — uppercase muted label, optional leading icon / trailing count
struct SectionHeader: View {
    @Environment(\.theme) private var theme
    let text: String
    var icon: String? = nil
    var trailing: String? = nil

    var body: some View {
        HStack(spacing: Space.sm) {
            // Signature accent tick (or accent-tinted icon) — a small branded
            // anchor on every section instead of a bare grey label.
            if let icon {
                MadarIcon(icon, size: IconSize.xs).foregroundStyle(theme.colors.accent)
            } else {
                Capsule().fill(theme.colors.accent).frame(width: 3, height: 12)
            }
            Text(text.uppercased())
                .font(Typo.label.font).tracking(Metric.tracking)
                .foregroundStyle(theme.colors.textSecondary)
            if let trailing {
                Text(trailing).font(Typo.label.font).foregroundStyle(theme.colors.textSecondary)
            }
            Spacer(minLength: 0)
        }
    }
}

// MARK: - ScreenHeader — back chevron + title (+subtitle / loading / trailing)
struct ScreenHeader<Trailing: View>: View {
    @Environment(\.theme) private var theme
    let title: String
    var subtitle: String? = nil
    var isLoading: Bool = false
    var onBack: (() -> Void)? = nil
    @ViewBuilder var trailing: () -> Trailing

    init(_ title: String, subtitle: String? = nil, isLoading: Bool = false,
         onBack: (() -> Void)? = nil, @ViewBuilder trailing: @escaping () -> Trailing = { EmptyView() }) {
        self.title = title; self.subtitle = subtitle; self.isLoading = isLoading
        self.onBack = onBack; self.trailing = trailing
    }

    var body: some View {
        HStack(spacing: Space.md) {
            if let onBack {
                Button { Haptics.selection(); onBack() } label: {
                    MadarIcon("chevron.backward", size: IconSize.lg)
                        .foregroundStyle(theme.colors.textPrimary)
                        .frame(width: Metric.closeButton, height: Metric.closeButton)
                        .background(theme.colors.surfaceAlt)
                        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
                        .overlay(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                            .strokeBorder(theme.colors.border, lineWidth: 1))
                }.buttonStyle(.pressable)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(Typo.h2.font).foregroundStyle(theme.colors.textPrimary)
                if let subtitle {
                    Text(subtitle).font(Typo.bodySm.font).foregroundStyle(theme.colors.textMuted)
                }
            }
            Spacer(minLength: Space.sm)
            if isLoading { ProgressView().controlSize(.small).tint(theme.colors.accent) }
            trailing()
        }
    }
}

/// Top-bar chrome for a `ScreenHeader`: surface fill, standard padding, bottom
/// hairline. Use on screens that present the header as a pinned bar.
extension View {
    func screenHeaderBar() -> some View { modifier(ScreenHeaderBar()) }
}
private struct ScreenHeaderBar: ViewModifier {
    @Environment(\.theme) private var theme
    func body(content: Content) -> some View {
        content
            .padding(.horizontal, Space.lg).padding(.vertical, Space.md)
            .frame(maxWidth: .infinity)
            .background(theme.colors.surface)
            .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }
}

// MARK: - MetricRow — label ↔ value (tabular money), optional tone / emphasis / icon
struct MetricRow: View {
    @Environment(\.theme) private var theme
    let label: String
    let value: String
    var tone: ChipTone? = nil
    var emphasize: Bool = false
    var icon: String? = nil

    var body: some View {
        HStack(spacing: Space.md) {
            if let icon { MadarIcon(icon, size: IconSize.sm).foregroundStyle(theme.colors.textMuted) }
            Text(label)
                .font(emphasize ? Typo.title.font : Typo.bodySm.font)
                .foregroundStyle(tone?.fg(theme.colors) ?? theme.colors.textSecondary)
            Spacer(minLength: Space.sm)
            Text(value)
                .font(emphasize ? Typo.moneyLg.font : Typo.money.font)
                .foregroundStyle(tone?.fg(theme.colors) ?? theme.colors.textPrimary)
        }
    }
}

// MARK: - SelectableChip — active/inactive toggle (payment/tip/filter/quick-cash)
struct SelectableChip: View {
    @Environment(\.theme) private var theme
    let label: String
    var icon: String? = nil
    var trailingValue: String? = nil
    let isSelected: Bool
    var tone: ChipTone = .accent
    let onTap: () -> Void

    var body: some View {
        Button { Haptics.selection(); onTap() } label: {
            HStack(spacing: Space.sm) {
                if let icon { MadarIcon(icon, size: IconSize.sm) }
                Text(label).font(Typo.title.font)
                if let trailingValue { Text(trailingValue).font(.money(12, .bold)) }
            }
            .foregroundStyle(isSelected ? theme.colors.textOnAccent : theme.colors.textSecondary)
            .padding(.horizontal, Space.md).padding(.vertical, Space.sm)
            .background(isSelected ? tone.fg(theme.colors) : theme.colors.surfaceAlt)
            .clipShape(Capsule())
            .overlay(Capsule().strokeBorder(isSelected ? Color.clear : theme.colors.border, lineWidth: 1))
            // Selected chips lift with a soft accent glow so the active filter /
            // payment method pops (matches Compose).
            .elevation(isSelected ? .glow : .none)
        }.buttonStyle(.pressable)
    }
}

// MARK: - EmptyState — centered icon + title (+subtitle) for empty grids/lists
struct EmptyState: View {
    @Environment(\.theme) private var theme
    let icon: String
    let title: String
    var subtitle: String? = nil

    var body: some View {
        VStack(spacing: Space.md) {
            MadarIcon(icon, size: 40).foregroundStyle(theme.colors.textMuted)
            Text(title).font(Typo.h3.font).foregroundStyle(theme.colors.textPrimary)
            if let subtitle {
                Text(subtitle).font(Typo.bodySm.font)
                    .foregroundStyle(theme.colors.textMuted).multilineTextAlignment(.center)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(Space.xl)
    }
}
