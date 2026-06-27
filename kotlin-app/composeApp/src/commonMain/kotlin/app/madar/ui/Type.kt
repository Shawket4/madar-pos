package app.madar.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.sp

// Semantic type scale — the Compose mirror of swift-app Theme/Typography.swift
// (`Typo`). Each style binds the real Cairo family (LocalMadarFont) so screens
// stop hand-passing fontFamily/weight/size and stay 1:1 with SwiftUI. Money
// styles request tabular figures ("tnum") so amount columns line up — the
// equivalent of Swift's `.monospacedDigit()`.
object Type {
    @Composable @ReadOnlyComposable
    private fun base(
        size: TextUnit,
        weight: FontWeight,
        tabular: Boolean = false,
        tracking: TextUnit = TextUnit.Unspecified,
    ): TextStyle =
        TextStyle(
            fontFamily = LocalMadarFont.current,
            fontWeight = weight,
            fontSize = size,
            letterSpacing = tracking,
            fontFeatureSettings = if (tabular) "tnum" else null,
        )

    // Bolder, more confident scale (the "go bolder" refresh): hero titles read
    // bigger and tighter, card titles step up, and a new `display` style gives
    // grand totals real presence. Kept in lockstep with Swift `Typo`.
    @Composable @ReadOnlyComposable fun display() = base(34.sp, FontWeight.ExtraBold, tracking = (-0.5).sp) // hero numbers / grand totals
    @Composable @ReadOnlyComposable fun h1() = base(30.sp, FontWeight.ExtraBold, tracking = (-0.4).sp)      // hero / screen titles
    @Composable @ReadOnlyComposable fun h2() = base(22.sp, FontWeight.Bold, tracking = (-0.2).sp)           // section / sheet titles
    @Composable @ReadOnlyComposable fun h3() = base(17.sp, FontWeight.SemiBold)     // card titles
    @Composable @ReadOnlyComposable fun title() = base(15.sp, FontWeight.SemiBold)  // emphasized rows
    @Composable @ReadOnlyComposable fun body() = base(14.sp, FontWeight.Medium)     // default body
    @Composable @ReadOnlyComposable fun bodySm() = base(13.sp, FontWeight.Normal)   // secondary body
    @Composable @ReadOnlyComposable fun label() = base(12.sp, FontWeight.SemiBold)  // uppercase labels
    @Composable @ReadOnlyComposable fun labelSm() = base(11.sp, FontWeight.SemiBold) // chips / dense labels
    @Composable @ReadOnlyComposable fun money(size: TextUnit = 14.sp, weight: FontWeight = FontWeight.Bold) =
        base(size, weight, tabular = true)
    @Composable @ReadOnlyComposable fun moneyLg() = base(24.sp, FontWeight.ExtraBold, tabular = true)
    @Composable @ReadOnlyComposable fun moneyDisplay() = base(34.sp, FontWeight.ExtraBold, tabular = true)
}
