package app.madar.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.hapticfeedback.HapticFeedback
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback

// Centralized tactile feedback — the Compose mirror of Swift `Haptics`
// (PressableScale.swift). One vocabulary for the whole app so every press surface
// buzzes consistently instead of each call site hand-picking a HapticFeedbackType
// (or, worse, firing none). Compose 1.7 exposes only LongPress + TextHandleMove,
// so the four semantic events fold onto those two; richer types arrive in 1.8.
// No-ops on desktop (no haptic hardware) — the LocalHapticFeedback there ignores them.
class Haptics(private val hf: HapticFeedback) {
    /** Light tick — chips, toggles, PIN keys, selection changes. */
    fun selection() = hf.performHapticFeedback(HapticFeedbackType.TextHandleMove)

    /** Medium thud — primary actions: add to cart, place order, confirm. */
    fun impact() = hf.performHapticFeedback(HapticFeedbackType.LongPress)

    /** Positive confirmation — order placed, shift opened, sale finalized. */
    fun success() = hf.performHapticFeedback(HapticFeedbackType.LongPress)

    /** Error nudge — failed validation, blocked action, max reached. */
    fun warning() = hf.performHapticFeedback(HapticFeedbackType.LongPress)
}

/** `val haptics = rememberHaptics()` — capture once, fire `haptics.selection()` etc. */
@Composable
fun rememberHaptics(): Haptics {
    val hf = LocalHapticFeedback.current
    return remember(hf) { Haptics(hf) }
}
