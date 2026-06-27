package app.madar.ui

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.unit.dp

// Soft layered shadows — the Compose mirror of swift-app's `Elevation`. The depth
// the flat UI was missing. Light = a Madar-ink wash (premium, not muddy grey),
// dark = black, glow = the accent. `clip = false` so the shadow falls outside the
// shape; the component applies its own clip/background after.
//
// The tint is the Madar ink (#14181E) — the same color Swift's ElevationModifier
// uses (Tokens.swift). The previous value was a legacy navy (#0A2540) left over
// from before the teal rebrand, which gave Compose surfaces a faint cold-blue
// halo the SwiftUI app never had.
enum class Elevation { NONE, CARD, RAISED, GLOW }

private val InkShadow = Color(0xFF14181E)

@Composable
fun Modifier.elevation(level: Elevation, shape: Shape): Modifier {
    val c = madarColors()
    val base = if (c.isDark) Color.Black else InkShadow
    return when (level) {
        Elevation.NONE -> this
        Elevation.CARD -> shadow(
            if (c.isDark) 14.dp else 10.dp, shape, clip = false,
            ambientColor = base, spotColor = base,
        )
        Elevation.RAISED -> shadow(
            if (c.isDark) 30.dp else 22.dp, shape, clip = false,
            ambientColor = base, spotColor = base,
        )
        Elevation.GLOW -> shadow(
            18.dp, shape, clip = false,
            ambientColor = c.accent, spotColor = c.accent,
        )
    }
}
