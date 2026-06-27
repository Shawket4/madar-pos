package app.madar.ui

import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

// Responsive layout tokens — the Compose mirror of swift-app Theme/Layout.swift.
// Breakpoints follow the Flutter form-factor rules (tablet ≥ 600, desktop ≥
// 1100); the rebuild screens switch on the container width (BoxWithConstraints
// maxWidth). Values match what the screens already use — naming them keeps the
// two platforms aligned and prevents drift. Named `Responsive` (not `Layout`)
// to match Swift, where `Layout` is a reserved SwiftUI protocol name, and to
// avoid shadowing Compose's `Layout` composable.
object Responsive {
    // Breakpoints (container width)
    val tablet = 600.dp     // ≥ → tablet spacing / wider forms
    val wideTable = 680.dp  // ≥ → table layout (history / shifts)
    val wide = 760.dp       // ≥ → split / side-by-side (login / open-shift / order)
    val desktop = 1100.dp   // ≥ → desktop: cap + center content

    // Content max-widths (caps so content centers, never stretches)
    val formMaxWidth = 520.dp
    val formMaxWidthWide = 600.dp
    val listMaxWidth = 560.dp
    val contentMaxWidth = 880.dp
    val sheetMaxWidth = 600.dp        // Flutter ResponsiveSheet cap
    val sheetCompactMaxWidth = 540.dp // item / bundle customize sheets

    // Split ratio (brand panel ↔ form)
    const val brandPanelRatio = 0.55f

    /** Form column width for the current container width (scales up off phones). */
    fun formWidth(width: Dp): Dp = if (width >= tablet) formMaxWidthWide else formMaxWidth
}
