package app.madar.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import org.jetbrains.compose.resources.Font
import app.madar.resources.Res
import app.madar.resources.Cairo_Regular
import app.madar.resources.Cairo_Medium
import app.madar.resources.Cairo_SemiBold
import app.madar.resources.Cairo_Bold
import app.madar.resources.Cairo_ExtraBold

// Madar design tokens — ported 1:1 from the Flutter AppTokens (light = original
// navy palette, dark = the new terracotta identity). The Compose mirror of the
// SwiftUI Theme/Tokens.swift. Components read LocalMadarColors.current; nothing
// hardcodes hex.
data class MadarColors(
    val bg: Color, val surface: Color, val surfaceAlt: Color, val surfaceRaised: Color,
    val border: Color, val borderLight: Color,
    val textPrimary: Color, val textSecondary: Color, val textMuted: Color, val textOnAccent: Color,
    val accent: Color, val accentBg: Color, val navy: Color, val navyBg: Color,
    val success: Color, val successBg: Color, val danger: Color, val dangerBg: Color,
    val warning: Color, val warningBg: Color, val shadow: Color, val isDark: Boolean,
)

// Madar palette — Teal deep #0D6273 (primary) / Teal light #2E94A6 / Ink #14181E /
// Paper #EFF3F4 / Slate #76828B. Light = ink-on-paper with a teal accent; dark =
// paper-on-ink with the brighter teal. (Field names still read "navy"/"madar*" —
// renamed in the identifier pass; only the values are Madar here.)
val MadarLight = MadarColors(
    bg = Color(0xFFEFF3F4), surface = Color(0xFFFFFFFF), surfaceAlt = Color(0xFFE7EEEF),
    surfaceRaised = Color(0xFFFFFFFF), border = Color(0xFFD7E0E1), borderLight = Color(0xFFE7EEEF),
    textPrimary = Color(0xFF14181E), textSecondary = Color(0xFF54636B), textMuted = Color(0xFF76828B),
    textOnAccent = Color(0xFFFFFFFF), accent = Color(0xFF0D6273), accentBg = Color(0xFFDCE9EB),
    navy = Color(0xFF0D6273), navyBg = Color(0xFFDCE9EB),
    success = Color(0xFF16A34A), successBg = Color(0xFFE7F6EC), danger = Color(0xFFDC2626), dangerBg = Color(0xFFFBEAEA),
    warning = Color(0xFFB45309), warningBg = Color(0xFFF7ECDD), shadow = Color(0x0D14181E), isDark = false,
)

val MadarDark = MadarColors(
    bg = Color(0xFF14181E), surface = Color(0xFF1B2128), surfaceAlt = Color(0xFF222A32),
    surfaceRaised = Color(0xFF262F38), border = Color(0xFF313B45), borderLight = Color(0xFF232C35),
    textPrimary = Color(0xFFEFF3F4), textSecondary = Color(0xFFAEB9C0), textMuted = Color(0xFF76828B),
    textOnAccent = Color(0xFFFFFFFF), accent = Color(0xFF2E94A6), accentBg = Color(0xFF123038),
    navy = Color(0xFF5FB6C7), navyBg = Color(0xFF15333B),
    success = Color(0xFF3BCE7E), successBg = Color(0xFF13291D), danger = Color(0xFFF4655A), dangerBg = Color(0xFF33191B),
    warning = Color(0xFFF0A23F), warningBg = Color(0xFF332512), shadow = Color(0x66000000), isDark = true,
)

val LocalMadarColors = staticCompositionLocalOf { MadarLight }

/// Shorthand for the active token set: `val c = madarColors()`.
@Composable
fun madarColors(): MadarColors = LocalMadarColors.current

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
    val pill = 999.dp
}

// Menu grid — one standard so cards + gutters match Swift's `Grid` (no eyeballing).
object Grid {
    val gutter = Space.lg   // 16 — gap between cards (was an eyeballed 14)
    val cellMax = 208.dp    // adaptive max cell width
    val padding = Space.lg  // outer grid padding
}

// Icon sizes (semantic; mirrors Swift IconSize). Use for SfIcon/MadarIcon and
// inline glyphs so icon sizing stays on one scale across both platforms.
object IconSize {
    val xs = 12.dp; val sm = 14.dp; val md = 16.dp; val lg = 18.dp; val xl = 20.dp; val xxl = 24.dp
}

// Semantic alphas (mirrors Swift Opacity) — replaces literal overlay opacities.
object Opacity {
    const val subtle = 0.14f   // faint tints / decorative rings
    const val border = 0.25f   // chip / banner hairline borders
    const val disabled = 0.45f // disabled controls
    const val scrim = 0.45f    // sheet / modal scrim
    const val press = 0.08f    // press overlay
}

// Named component metrics (mirrors Swift Metric) — replaces repeated literals.
object Metric {
    val buttonHeight = 54.dp
    val inputHeight = 48.dp
    val amountFieldHeight = 64.dp
    val tableHeaderHeight = 42.dp
    val tableRowHeight = 56.dp
    val iconTile = 38.dp
    val stepper = 30.dp
    val ingredientBox = 54.dp
    val closeButton = 32.dp
    val pinKey = 64.dp
}

// The signature tactile press + standard timings (mirrors Swift Motion). The
// spring specs are created where used (animateFloatAsState etc.); these are the
// shared constants so both platforms agree on feel.
object Motion {
    const val pressScale = 0.97f
    const val pressScaleKey = 0.92f   // PIN keys press a touch deeper
    const val trackingSp = 0.6f       // uppercase label letter-spacing (in sp)
}

// Cairo — bundled in composeResources/font. Built into a FontFamily inside the
// theme (the compose-resources `Font()` builder is @Composable) and provided via
// LocalMadarFont so every Text reads the real brand face. Replaces the old
// `FontFamily.Default` stand-in that left the entire Compose app on the system
// font (a major divergence from the SwiftUI app, which uses Cairo).
val LocalMadarFont = staticCompositionLocalOf<FontFamily> { FontFamily.Default }

@Composable
private fun cairoFamily(): FontFamily = FontFamily(
    Font(Res.font.Cairo_Regular, FontWeight.Normal),
    Font(Res.font.Cairo_Medium, FontWeight.Medium),
    Font(Res.font.Cairo_SemiBold, FontWeight.SemiBold),
    Font(Res.font.Cairo_Bold, FontWeight.Bold),
    Font(Res.font.Cairo_ExtraBold, FontWeight.ExtraBold),
)

@Composable
fun MadarTheme(mode: ThemeMode = ThemeMode.LIGHT, content: @Composable () -> Unit) {
    val dark = when (mode) {
        ThemeMode.LIGHT -> false
        ThemeMode.DARK -> true
        ThemeMode.SYSTEM -> isSystemInDarkTheme()
    }
    val colors = if (dark) MadarDark else MadarLight
    val cairo = cairoFamily()
    CompositionLocalProvider(
        LocalMadarColors provides colors,
        LocalMadarFont provides cairo,
    ) {
        MaterialTheme(content = content)
    }
}
