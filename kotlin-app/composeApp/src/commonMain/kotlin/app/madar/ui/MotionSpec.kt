package app.madar.ui

import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.EaseInOut
import androidx.compose.animation.core.EaseOut
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset

// Shared motion vocabulary — the Compose mirror of Swift's `Motion` animation
// specs (Tokens.swift). The `Motion` object in Theme.kt holds the scalar
// constants (press scale, tracking); this holds the actual AnimationSpecs so both
// platforms animate with the same feel. SwiftUI specs are response/damping
// springs; these are tuned to match: press = snappy, sheet = smooth+settled,
// standard = a quick eased tween for color/opacity/cross-fades.
object MotionSpec {
    /** Tactile press rebound — buttons, chips, cards (Swift spring 0.22/0.7). */
    fun <T> press(): SpringSpecT<T> = spring(dampingRatio = 0.72f, stiffness = 620f)

    /** Bottom-sheet + overlay slide (Swift spring 0.34/0.9) — smooth, barely overshoots. */
    fun <T> sheet(): SpringSpecT<T> = spring(dampingRatio = 0.9f, stiffness = 300f)

    /** A touch bouncier spring for value pops (PIN dots, qty steppers, badges). */
    fun <T> bouncy(): SpringSpecT<T> = spring(dampingRatio = Spring.DampingRatioMediumBouncy, stiffness = 520f)

    /** Standard eased tween — color, opacity, border, cross-fades (Swift easeOut
     *  0.22). FiniteAnimationSpec so it also feeds slide/fade transitions. */
    fun <T> standard(): FiniteAnimationSpec<T> = tween(durationMillis = 220, easing = EaseOut)

    /** Slower content cross-fade (route/tab swaps). */
    fun <T> gentle(): FiniteAnimationSpec<T> = tween(durationMillis = 300, easing = EaseInOut)
}

// `spring()` returns SpringSpec<T> which is a FiniteAnimationSpec — alias keeps the
// call sites terse and lets animate*AsState/Animatable accept them directly.
private typealias SpringSpecT<T> = FiniteAnimationSpec<T>

/** A vertical slide+fade offset for list/card entrance — shared so staggered
 *  list reveals feel identical to the SwiftUI `.transition(.move+.opacity)`. */
fun slideOffset(progress: Float, distance: Dp, density: androidx.compose.ui.unit.Density): IntOffset =
    with(density) { IntOffset(0, ((1f - progress) * distance.toPx()).toInt()) }
