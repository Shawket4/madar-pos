// Sufrix design tokens — ported 1:1 from the Flutter `AppTokens` (the source of
// truth: sufrix_pos/lib/core/theme/app_theme.dart). Screens read these via
// `@Environment(\.theme)` and NEVER hardcode hex. Light = the original navy
// palette (navy is the action color); dark = the new terracotta identity.
import SwiftUI

// MARK: - Brand anchors
enum SufrixBrand {
    static let navy = Color(hex: 0x0A2540)
    static let terracotta = Color(hex: 0xC25B3F)
    static let terracottaBright = Color(hex: 0xE07856) // dark-theme accent
    static let cream = Color(hex: 0xFAF7F2)
}

// MARK: - Semantic color set (light + dark variants)
struct SufrixColors {
    let bg, surface, surfaceAlt, surfaceRaised, border, borderLight: Color
    let textPrimary, textSecondary, textMuted, textOnAccent: Color
    let accent, accentBg, navy, navyBg: Color
    let success, successBg, danger, dangerBg, warning, warningBg: Color
    let shadow: Color

    static let light = SufrixColors(
        bg: Color(hex: 0xF4F6F8), surface: .white, surfaceAlt: Color(hex: 0xFAF7F2),
        surfaceRaised: .white, border: Color(hex: 0xE5E7EB), borderLight: Color(hex: 0xF3F4F6),
        textPrimary: Color(hex: 0x0A2540), textSecondary: Color(hex: 0x6B7280),
        textMuted: Color(hex: 0x9CA3AF), textOnAccent: .white,
        accent: SufrixBrand.navy, accentBg: Color(hex: 0xE9EEF4),
        navy: SufrixBrand.navy, navyBg: Color(hex: 0xE9EEF4),
        success: Color(hex: 0x16A34A), successBg: Color(hex: 0xE7F6EC),
        danger: Color(hex: 0xDC2626), dangerBg: Color(hex: 0xFBEAEA),
        warning: Color(hex: 0xD97706), warningBg: Color(hex: 0xFBF1E0),
        shadow: Color(hex: 0x111827, alpha: 0.05)
    )

    static let dark = SufrixColors(
        bg: Color(hex: 0x0A111B), surface: Color(hex: 0x111B28), surfaceAlt: Color(hex: 0x152133),
        surfaceRaised: Color(hex: 0x182436), border: Color(hex: 0x243349), borderLight: Color(hex: 0x1B2940),
        textPrimary: Color(hex: 0xEAF0F7), textSecondary: Color(hex: 0xA3B3C7),
        textMuted: Color(hex: 0x65788F), textOnAccent: .white,
        accent: SufrixBrand.terracottaBright, accentBg: Color(hex: 0x33231F),
        navy: Color(hex: 0x8FB4DD), navyBg: Color(hex: 0x1A2A3F),
        success: Color(hex: 0x3BCE7E), successBg: Color(hex: 0x13291D),
        danger: Color(hex: 0xF4655A), dangerBg: Color(hex: 0x33191B),
        warning: Color(hex: 0xF0A23F), warningBg: Color(hex: 0x332512),
        shadow: Color(hex: 0x000000, alpha: 0.40)
    )
}

// MARK: - Scales (4-pt spacing, radius, motion) — match Flutter AppSpace/AppRadius
enum Space {
    static let xs: CGFloat = 4, sm: CGFloat = 8, md: CGFloat = 12
    static let lg: CGFloat = 16, xl: CGFloat = 24, xxl: CGFloat = 32
}
enum Radii {
    static let xs: CGFloat = 8, sm: CGFloat = 12, md: CGFloat = 16
    static let lg: CGFloat = 20, xl: CGFloat = 24, xxl: CGFloat = 32, pill: CGFloat = 999
}
enum Motion {
    /// The signature tactile press (Flutter `AnimatedPressScale`).
    static let pressScale: CGFloat = 0.97
    static let press = Animation.spring(response: 0.22, dampingFraction: 0.7)
    static let standard = Animation.easeOut(duration: 0.22)
}

// MARK: - Typography (bundled Cairo; tabular figures for money)
extension Font {
    /// General UI text. Picks the explicit Cairo face for the weight (custom
    /// fonts don't reliably honor `.weight()`); falls back to system if Cairo
    /// isn't registered.
    static func ui(_ size: CGFloat, _ weight: Font.Weight = .medium) -> Font {
        .custom(cairoFace(weight), size: size)
    }
    /// Monetary amounts — bold + tabular so totals line up.
    static func money(_ size: CGFloat = 16, _ weight: Font.Weight = .bold) -> Font {
        .custom(cairoFace(weight), size: size).monospacedDigit()
    }
}

private func cairoFace(_ w: Font.Weight) -> String {
    if w == .black || w == .heavy { return "Cairo-ExtraBold" }
    if w == .bold { return "Cairo-Bold" }
    if w == .semibold { return "Cairo-SemiBold" }
    if w == .medium { return "Cairo-Medium" }
    return "Cairo-Regular"
}

// MARK: - Theme handle + environment injection
struct SufrixTheme {
    let colors: SufrixColors
    let isDark: Bool
    static let light = SufrixTheme(colors: .light, isDark: false)
    static let dark = SufrixTheme(colors: .dark, isDark: true)
}

private struct SufrixThemeKey: EnvironmentKey {
    static let defaultValue = SufrixTheme.light
}
extension EnvironmentValues {
    var theme: SufrixTheme {
        get { self[SufrixThemeKey.self] }
        set { self[SufrixThemeKey.self] = newValue }
    }
}

/// Injects the light/dark token set based on the system color scheme. Wrap the
/// root in this so every descendant reads `@Environment(\.theme)`.
struct ThemedRoot<Content: View>: View {
    @Environment(\.colorScheme) private var scheme
    @ViewBuilder var content: () -> Content
    var body: some View {
        let theme: SufrixTheme = scheme == .dark ? .dark : .light
        content()
            .environment(\.theme, theme)
            .tint(theme.colors.accent)
    }
}

// MARK: - Color hex helper
extension Color {
    init(hex: UInt32, alpha: Double = 1) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255,
            opacity: alpha
        )
    }
}
