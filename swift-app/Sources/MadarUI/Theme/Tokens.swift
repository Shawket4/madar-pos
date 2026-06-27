// Madar design tokens — ported 1:1 from the Flutter `AppTokens` (the source of
// truth: sufrix_pos/lib/core/theme/app_theme.dart). Screens read these via
// `@Environment(\.theme)` and NEVER hardcode hex. Light = the original navy
// palette (navy is the action color); dark = the new terracotta identity.
import SwiftUI

// MARK: - Brand anchors
enum MadarBrand {
    // Madar palette (field names legacy; renamed in the identifier pass).
    static let navy = Color(hex: 0x0D6273)             // Teal deep (primary action)
    static let terracotta = Color(hex: 0x0D6273)       // legacy field → teal deep
    static let terracottaBright = Color(hex: 0x2E94A6) // Teal light (dark-theme accent)
    static let cream = Color(hex: 0xEFF3F4)            // Paper
}

// MARK: - Semantic color set (light + dark variants)
struct MadarColors {
    let bg, surface, surfaceAlt, surfaceRaised, border, borderLight: Color
    let textPrimary, textSecondary, textMuted, textOnAccent: Color
    let accent, accentBg, navy, navyBg: Color
    let success, successBg, danger, dangerBg, warning, warningBg: Color
    let shadow: Color

    static let light = MadarColors(
        bg: Color(hex: 0xEFF3F4), surface: .white, surfaceAlt: Color(hex: 0xE7EEEF),
        surfaceRaised: .white, border: Color(hex: 0xD7E0E1), borderLight: Color(hex: 0xE7EEEF),
        textPrimary: Color(hex: 0x14181E), textSecondary: Color(hex: 0x54636B),
        textMuted: Color(hex: 0x76828B), textOnAccent: .white,
        accent: MadarBrand.navy, accentBg: Color(hex: 0xDCE9EB),
        navy: MadarBrand.navy, navyBg: Color(hex: 0xDCE9EB),
        success: Color(hex: 0x16A34A), successBg: Color(hex: 0xE7F6EC),
        danger: Color(hex: 0xDC2626), dangerBg: Color(hex: 0xFBEAEA),
        warning: Color(hex: 0xB45309), warningBg: Color(hex: 0xF7ECDD),
        shadow: Color(hex: 0x14181E, alpha: 0.05)
    )

    static let dark = MadarColors(
        bg: Color(hex: 0x14181E), surface: Color(hex: 0x1B2128), surfaceAlt: Color(hex: 0x222A32),
        surfaceRaised: Color(hex: 0x262F38), border: Color(hex: 0x313B45), borderLight: Color(hex: 0x232C35),
        textPrimary: Color(hex: 0xEFF3F4), textSecondary: Color(hex: 0xAEB9C0),
        textMuted: Color(hex: 0x76828B), textOnAccent: .white,
        accent: MadarBrand.terracottaBright, accentBg: Color(hex: 0x123038),
        navy: Color(hex: 0x5FB6C7), navyBg: Color(hex: 0x15333B),
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

// MARK: - Icon sizes (semantic; replaces scattered raw point sizes)
/// One scale for glyph sizing so icons line up with the 4-pt grid and with
/// their Kotlin counterparts. Use these for `MadarIcon` and inline symbols.
enum IconSize {
    static let xs: CGFloat = 12, sm: CGFloat = 14, md: CGFloat = 16
    static let lg: CGFloat = 18, xl: CGFloat = 20, xxl: CGFloat = 24
}

// MARK: - Opacity (semantic alphas; replaces literal overlay opacities)
enum Opacity {
    static let subtle: Double = 0.14   // faint tints / decorative rings
    static let border: Double = 0.25   // chip / banner hairline borders
    static let disabled: Double = 0.45 // disabled controls
    static let scrim: Double = 0.45    // sheet / modal scrim
    static let press: Double = 0.08    // press overlay
}

// MARK: - Component metrics (named heights/sizes; replaces repeated literals)
enum Metric {
    static let buttonHeight: CGFloat = 54    // MadarButton
    static let inputHeight: CGFloat = 48     // text fields
    static let amountFieldHeight: CGFloat = 64
    static let tableHeaderHeight: CGFloat = 42
    static let tableRowHeight: CGFloat = 56
    static let iconTile: CGFloat = 38        // leading tone tile (sync/cash rows)
    static let stepper: CGFloat = 30         // qty +/- circle
    static let ingredientBox: CGFloat = 54   // recipe qty box
    static let closeButton: CGFloat = 32     // modal close affordance
    static let pinKey: CGFloat = 64
    static let tracking: CGFloat = 0.6       // uppercase label letter-spacing
}

// MARK: - Menu grid (one standard so cards + gutters aren't eyeballed per screen)
enum Grid {
    static let gutter: CGFloat = Space.lg   // 16 — gap between cards (was an eyeballed 10)
    static let cellMin: CGFloat = 168       // card adaptive min
    static let cellMax: CGFloat = 208       // card adaptive max
    static let padding: CGFloat = Space.lg  // outer grid padding
}

// MARK: - Elevation (soft layered shadows — the depth the flat UI was missing).
// Ports the Flutter design's `of` / `raised` / `primaryGlow`. Light shadows are a
// navy-tinted wash (premium, not muddy grey); dark uses black; the glow is the
// accent. Apply AFTER `clipShape` so the shadow falls outside the shape.
enum Elevation { case none, card, raised, glow }

struct ElevationModifier: ViewModifier {
    @Environment(\.theme) private var theme
    let level: Elevation
    func body(content: Content) -> some View {
        content.shadow(color: color, radius: radius, x: 0, y: yOffset)
    }
    private var color: Color {
        switch level {
        case .none:   return .clear
        case .card:   return theme.isDark ? Color.black.opacity(0.45) : Color(hex: 0x14181E, alpha: 0.07)
        case .raised: return theme.isDark ? Color.black.opacity(0.55) : Color(hex: 0x14181E, alpha: 0.13)
        case .glow:   return theme.colors.accent.opacity(theme.isDark ? 0.55 : 0.38)
        }
    }
    private var radius: CGFloat {
        switch level { case .none: return 0; case .card: return 16; case .raised: return 30; case .glow: return 18 }
    }
    private var yOffset: CGFloat {
        switch level { case .none: return 0; case .card: return 6; case .raised: return 14; case .glow: return 8 }
    }
}

extension View {
    /// Soft depth shadow. `.card` for surfaces, `.raised` for sheets/popovers,
    /// `.glow` for the primary call-to-action.
    func elevation(_ level: Elevation) -> some View { modifier(ElevationModifier(level: level)) }
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
struct MadarTheme {
    let colors: MadarColors
    let isDark: Bool
    static let light = MadarTheme(colors: .light, isDark: false)
    static let dark = MadarTheme(colors: .dark, isDark: true)
}

/// App theme preference. Default is `.light` (the original navy palette);
/// `.dark` is the terracotta identity; `.system` follows the OS.
enum ThemeMode: String, CaseIterable { case light, dark, system }

private struct MadarThemeKey: EnvironmentKey {
    static let defaultValue = MadarTheme.light
}
extension EnvironmentValues {
    var theme: MadarTheme {
        get { self[MadarThemeKey.self] }
        set { self[MadarThemeKey.self] = newValue }
    }
}

/// Localization accessor injected from the core — `Text(t("login.sign_in"))`.
private struct LocalizeKey: EnvironmentKey {
    static let defaultValue: (String) -> String = { $0 }
}
extension EnvironmentValues {
    var localize: (String) -> String {
        get { self[LocalizeKey.self] }
        set { self[LocalizeKey.self] = newValue }
    }
}

/// Injects the token set for the chosen `mode` (default light). Wrap the root so
/// every descendant reads `@Environment(\.theme)`.
struct ThemedRoot<Content: View>: View {
    let mode: ThemeMode
    @Environment(\.colorScheme) private var systemScheme
    @ViewBuilder var content: () -> Content

    var body: some View {
        let isDark: Bool = {
            switch mode {
            case .light: return false
            case .dark: return true
            case .system: return systemScheme == .dark
            }
        }()
        let theme = isDark ? MadarTheme.dark : .light
        content()
            .environment(\.theme, theme)
            .tint(theme.colors.accent)
            .preferredColorScheme(mode == .system ? nil : (isDark ? .dark : .light))
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
