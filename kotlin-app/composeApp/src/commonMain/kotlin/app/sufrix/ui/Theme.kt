package app.sufrix.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp

// Sufrix design tokens — ported 1:1 from the Flutter AppTokens (light = original
// navy palette, dark = the new terracotta identity). The Compose mirror of the
// SwiftUI Theme/Tokens.swift. Components read LocalSufrixColors.current; nothing
// hardcodes hex.
data class SufrixColors(
    val bg: Color, val surface: Color, val surfaceAlt: Color, val surfaceRaised: Color,
    val border: Color, val borderLight: Color,
    val textPrimary: Color, val textSecondary: Color, val textMuted: Color, val textOnAccent: Color,
    val accent: Color, val accentBg: Color, val navy: Color, val navyBg: Color,
    val success: Color, val successBg: Color, val danger: Color, val dangerBg: Color,
    val warning: Color, val warningBg: Color, val shadow: Color, val isDark: Boolean,
)

val SufrixLight = SufrixColors(
    bg = Color(0xFFF4F6F8), surface = Color(0xFFFFFFFF), surfaceAlt = Color(0xFFFAF7F2),
    surfaceRaised = Color(0xFFFFFFFF), border = Color(0xFFE5E7EB), borderLight = Color(0xFFF3F4F6),
    textPrimary = Color(0xFF0A2540), textSecondary = Color(0xFF6B7280), textMuted = Color(0xFF9CA3AF),
    textOnAccent = Color(0xFFFFFFFF), accent = Color(0xFF0A2540), accentBg = Color(0xFFE9EEF4),
    navy = Color(0xFF0A2540), navyBg = Color(0xFFE9EEF4),
    success = Color(0xFF16A34A), successBg = Color(0xFFE7F6EC), danger = Color(0xFFDC2626), dangerBg = Color(0xFFFBEAEA),
    warning = Color(0xFFD97706), warningBg = Color(0xFFFBF1E0), shadow = Color(0x0D111827), isDark = false,
)

val SufrixDark = SufrixColors(
    bg = Color(0xFF0A111B), surface = Color(0xFF111B28), surfaceAlt = Color(0xFF152133),
    surfaceRaised = Color(0xFF182436), border = Color(0xFF243349), borderLight = Color(0xFF1B2940),
    textPrimary = Color(0xFFEAF0F7), textSecondary = Color(0xFFA3B3C7), textMuted = Color(0xFF65788F),
    textOnAccent = Color(0xFFFFFFFF), accent = Color(0xFFE07856), accentBg = Color(0xFF33231F),
    navy = Color(0xFF8FB4DD), navyBg = Color(0xFF1A2A3F),
    success = Color(0xFF3BCE7E), successBg = Color(0xFF13291D), danger = Color(0xFFF4655A), dangerBg = Color(0xFF33191B),
    warning = Color(0xFFF0A23F), warningBg = Color(0xFF332512), shadow = Color(0x66000000), isDark = true,
)

val LocalSufrixColors = staticCompositionLocalOf { SufrixLight }

/// Shorthand for the active token set: `val c = sufrixColors()`.
@Composable
fun sufrixColors(): SufrixColors = LocalSufrixColors.current

/// App theme preference. Default = LIGHT (the original navy palette); DARK is the
/// terracotta identity; SYSTEM follows the OS. Mirrors the Swift `ThemeMode`.
enum class ThemeMode { LIGHT, DARK, SYSTEM }

/// Localization accessor injected from the core: `t("login.sign_in")`.
val LocalLocalize = staticCompositionLocalOf<(String) -> String> { { it } }

@Composable
fun t(key: String): String = LocalLocalize.current(key)

object Space {
    val xs = 4.dp; val sm = 8.dp; val md = 12.dp; val lg = 16.dp; val xl = 24.dp; val xxl = 32.dp
}
object Radii {
    val xs = 8.dp; val sm = 12.dp; val md = 16.dp; val lg = 20.dp; val xl = 24.dp; val xxl = 32.dp
}

// Cairo is bundled in the shipping apps; FontFamily.Default is the stand-in here.
val SufrixFont = FontFamily.Default

@Composable
fun SufrixTheme(mode: ThemeMode = ThemeMode.LIGHT, content: @Composable () -> Unit) {
    val dark = when (mode) {
        ThemeMode.LIGHT -> false
        ThemeMode.DARK -> true
        ThemeMode.SYSTEM -> isSystemInDarkTheme()
    }
    val colors = if (dark) SufrixDark else SufrixLight
    CompositionLocalProvider(LocalSufrixColors provides colors) {
        MaterialTheme(content = content)
    }
}
