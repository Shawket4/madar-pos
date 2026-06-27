package app.madar.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.draw.scale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.sin

// In-app realtime alert — the visual companion to the OS notification (the rebuild
// of the Flutter `NewOrderBanner`, generalized to every alerting event and made
// more polished). Driven by the core's dedup'd alert (RealtimePlayer.postNotification
// → AppModel.showRealtimeAlert). A spring-in accent banner with a looping bounce+
// wiggle icon over a pulsing glow ring; auto-dismisses, or tap / ✕ to clear.

/** One in-app alert. `tag` is the core's `"{event_type}:{id}"`, so its prefix
 *  picks the icon (delivery / kitchen / ticket / ready). */
data class RealtimeAlertData(val id: Int, val title: String, val body: String, val tag: String) {
    private val eventType: String get() = tag.substringBefore(':')
    val icon: String get() = when {
        eventType.contains("ready") -> "bell.fill"
        eventType.startsWith("delivery") -> "bicycle"
        eventType.startsWith("kitchen") -> "flame.fill"
        eventType.startsWith("ticket") -> "fork.knife"
        else -> "bell.fill"
    }
}

private const val DISMISS_MS = 6000L

@Composable
fun RealtimeAlertBanner(alert: RealtimeAlertData?, onDismiss: (Int) -> Unit, modifier: Modifier = Modifier) {
    // Keep the last alert mounted through the exit transition so the card still
    // has content to render while sliding out.
    var last by remember { mutableStateOf(alert) }
    LaunchedEffect(alert?.id) { if (alert != null) last = alert }
    // Auto-dismiss (host-owned, like the toast) — timer keyed on the id so a new
    // alert restarts it and an old timer can't clear a newer banner.
    LaunchedEffect(alert?.id) {
        val a = alert ?: return@LaunchedEffect
        delay(DISMISS_MS)
        onDismiss(a.id)
    }
    AnimatedVisibility(
        visible = alert != null,
        enter = slideInVertically(MotionSpec.sheet()) { -it } + fadeIn(MotionSpec.standard()),
        exit = slideOutVertically(MotionSpec.standard()) { -it } + fadeOut(MotionSpec.standard()),
        modifier = modifier,
    ) {
        last?.let { RealtimeAlertCard(it, onDismiss) }
    }
}

/** The banner card itself — also used by the screenshot harness for previews. */
@Composable
fun RealtimeAlertCard(alert: RealtimeAlertData, onDismiss: (Int) -> Unit) {
    val c = madarColors()
    val haptics = rememberHaptics()
    val shape = RoundedCornerShape(Radii.md)

    // Looping bounce + wiggle (the Flutter LoopingIcon, ported): scale pulses on
    // |sin| and the glyph wiggles on a faster sin — runs the whole time it's up.
    val transition = rememberInfiniteTransition(label = "alert")
    val p by transition.animateFloat(
        initialValue = 0f, targetValue = 1f,
        animationSpec = infiniteRepeatable(tween(900, easing = LinearEasing), RepeatMode.Restart),
        label = "p",
    )
    val scale = 1f + 0.18f * abs(sin(p * 2f * PI.toFloat()))
    val angleDeg = 0.18f * sin(p * 4f * PI.toFloat()) * (180f / PI.toFloat())

    Row(
        Modifier.fillMaxWidth().widthIn(max = 560.dp).padding(Space.md)
            .elevation(Elevation.RAISED, shape)
            .clip(shape)
            .background(c.accentBg)
            .border(1.dp, c.accent.copy(alpha = 0.35f), shape)
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {
                haptics.selection(); onDismiss(alert.id)
            }
            .padding(horizontal = 14.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        // Icon over a pulsing glow ring.
        Box(contentAlignment = Alignment.Center) {
            Box(Modifier.size(34.dp).scale(scale).clip(CircleShape).background(c.accent.copy(alpha = Opacity.subtle)))
            MadarIcon(alert.icon, tint = c.accent, size = 20.dp, modifier = Modifier.scale(scale).rotate(angleDeg))
        }
        Column(Modifier.weight(1f)) {
            Text(
                alert.title, color = c.accent, fontFamily = LocalMadarFont.current,
                fontWeight = FontWeight.ExtraBold, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis,
            )
            if (alert.body.isNotBlank()) {
                Text(
                    alert.body, color = c.textSecondary, fontFamily = LocalMadarFont.current,
                    fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis,
                )
            }
        }
        Box(
            Modifier.size(28.dp).clip(CircleShape)
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { onDismiss(alert.id) },
            contentAlignment = Alignment.Center,
        ) { MadarIcon("xmark", tint = c.textMuted, size = 16.dp) }
    }
}
