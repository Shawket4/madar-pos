package app.madar.ui

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** A single rounded placeholder bar that gently pulses — mirrors the Swift
 *  SkeletonBlock. Shown while a list loads, in place of a bare spinner. */
@Composable
fun SkeletonBlock(width: Dp? = null, height: Dp = 13.dp, corner: Dp = 6.dp, modifier: Modifier = Modifier) {
    val c = madarColors()
    val transition = rememberInfiniteTransition(label = "skeleton")
    val a by transition.animateFloat(
        initialValue = 1f,
        targetValue = 0.5f,
        animationSpec = infiniteRepeatable(tween(900), RepeatMode.Reverse),
        label = "alpha",
    )
    Box(
        (if (width != null) modifier.width(width) else modifier)
            .height(height).alpha(a).clip(RoundedCornerShape(corner)).background(c.surfaceAlt),
    )
}

/** A card-shaped skeleton standing in for one list row. */
@Composable
fun SkeletonRow() {
    val c = madarColors()
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(Radii.sm)).padding(Space.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SkeletonBlock(width = 130.dp, height = 14.dp)
            SkeletonBlock(width = 80.dp, height = 11.dp)
        }
        SkeletonBlock(width = 56.dp, height = 14.dp)
    }
}

/** A column of [count] skeleton rows — the loading state for a list screen. */
@Composable
fun SkeletonList(count: Int = 6) {
    Column(
        Modifier.widthIn(max = 560.dp).fillMaxWidth().padding(Space.lg),
        verticalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        repeat(count) { SkeletonRow() }
    }
}
