package app.madar.ui

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.sin

// In-app realtime alert — the visual companion to the OS notification (the rebuild
// of the Flutter `NewOrderBanner`, generalized to every alerting event). Driven by
// the core's dedup'd alert (RealtimePlayer.postNotification → AppModel.showRealtimeAlert).
// PERSISTENT (stays until dismissed) and rendered like the iOS notification stack:
// a compact collapsed DECK (top card full, the rest peeking behind) that expands
// to a scrollable list on tap — so it overlays the app, never pushes content down,
// and stays small no matter how many pile in.

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

@Composable
fun RealtimeAlertStack(alerts: List<RealtimeAlertData>, onDismiss: (Int) -> Unit, modifier: Modifier = Modifier) {
    if (alerts.isEmpty()) return
    var expanded by remember { mutableStateOf(false) }
    // Nothing left to fan out → fall back to the deck.
    if (alerts.size <= 1 && expanded) expanded = false

    Box(modifier.widthIn(max = 480.dp).fillMaxWidth().padding(Space.md)) {
        if (expanded) {
            ExpandedAlertList(alerts, onDismiss, onCollapse = { expanded = false })
        } else {
            CollapsedAlertDeck(alerts, onDismiss, onExpand = { if (alerts.size > 1) expanded = true })
        }
    }
}

/** iOS-style collapsed deck: the newest alert sits on top at full size; up to two
 *  behind it peek out — scaled down, nudged down, and dimmed — exactly like a
 *  stacked notification group. Tap to fan the stack out. */
@Composable
private fun CollapsedAlertDeck(alerts: List<RealtimeAlertData>, onDismiss: (Int) -> Unit, onExpand: () -> Unit) {
    val deck = alerts.take(3)
    val multi = alerts.size > 1
    Box(Modifier.fillMaxWidth()) {
        // Draw back-to-front so the newest (index 0) lands on top.
        for (d in deck.indices.reversed()) {
            val a = deck[d]
            val front = d == 0
            Box(
                Modifier.fillMaxWidth().graphicsLayer {
                    val s = 1f - 0.05f * d
                    scaleX = s; scaleY = s
                    translationY = 10.dp.toPx() * d
                    alpha = 1f - 0.22f * d
                    transformOrigin = TransformOrigin(0.5f, 0f) // shrink from the top edge → peeks below
                },
            ) {
                RealtimeAlertCard(
                    a, onDismiss,
                    animated = front, showClose = front,
                    onTap = if (front && multi) onExpand else null,
                )
            }
        }
    }
}

/** Fanned-out, scrollable list (newest on top) + a chevron-up to re-collapse.
 *  Capped generously and scrolls — it never pushes the app down (it's an overlay). */
@Composable
private fun ExpandedAlertList(alerts: List<RealtimeAlertData>, onDismiss: (Int) -> Unit, onCollapse: () -> Unit) {
    val c = madarColors()
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val maxH = maxHeight * 0.8f
        LazyColumn(
            Modifier.fillMaxWidth().heightIn(max = maxH),
            verticalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            items(alerts, key = { it.id }) { a ->
                RealtimeAlertCard(a, onDismiss, modifier = Modifier.animateItem())
            }
            item("collapse") {
                Row(Modifier.fillMaxWidth().padding(top = Space.xs), horizontalArrangement = Arrangement.Center) {
                    Box(
                        Modifier.clip(CircleShape).background(c.surfaceAlt)
                            .border(1.dp, c.border, CircleShape)
                            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { onCollapse() }
                            .padding(horizontal = Space.lg, vertical = 6.dp),
                        contentAlignment = Alignment.Center,
                    ) { MadarIcon("chevron.up", tint = c.textSecondary, size = 16.dp) }
                }
            }
        }
    }
}

/** The banner card itself — also used by the screenshot harness for previews.
 *  [animated] runs the looping bounce/wiggle; the peeking deck cards turn it off.
 *  [onTap] (when set) handles a body tap (e.g. expand the deck); the ✕ dismisses. */
@Composable
fun RealtimeAlertCard(
    alert: RealtimeAlertData,
    onDismiss: (Int) -> Unit,
    modifier: Modifier = Modifier,
    animated: Boolean = true,
    showClose: Boolean = true,
    onTap: (() -> Unit)? = null,
) {
    val c = madarColors()
    val haptics = rememberHaptics()
    val shape = RoundedCornerShape(Radii.md)

    // Looping bounce + wiggle (the Flutter LoopingIcon, ported): scale pulses on
    // |sin| and the glyph wiggles on a faster sin. Disabled for peeking deck cards.
    val scale: Float
    val angleDeg: Float
    if (animated) {
        val transition = rememberInfiniteTransition(label = "alert")
        val p by transition.animateFloat(
            initialValue = 0f, targetValue = 1f,
            animationSpec = infiniteRepeatable(tween(900, easing = LinearEasing), RepeatMode.Restart),
            label = "p",
        )
        scale = 1f + 0.18f * abs(sin(p * 2f * PI.toFloat()))
        angleDeg = 0.18f * sin(p * 4f * PI.toFloat()) * (180f / PI.toFloat())
    } else {
        scale = 1f; angleDeg = 0f
    }

    Row(
        modifier.fillMaxWidth()
            .elevation(Elevation.RAISED, shape)
            .clip(shape)
            .background(c.accentBg)
            .border(1.dp, c.accent.copy(alpha = 0.35f), shape)
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {
                haptics.selection(); onTap?.invoke() ?: onDismiss(alert.id)
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
        if (showClose) {
            Box(
                Modifier.size(28.dp).clip(CircleShape)
                    .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { onDismiss(alert.id) },
                contentAlignment = Alignment.Center,
            ) { MadarIcon("xmark", tint = c.textMuted, size = 16.dp) }
        }
    }
}
