package app.madar.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay

/** The data half of a toast (the action lives on the model). */
data class ToastData(
    val id: Int,
    val text: String,
    val tone: ChipTone = ChipTone.NEUTRAL,
    val actionLabel: String? = null,
    val seconds: Double = 2.6,
    val icon: String? = null,
)

/** A transient bottom banner mirroring the Swift toast — one optional action,
 *  auto-dismiss after [ToastData.seconds]. Render once at the root, above the
 *  route + any sheets (it's the last child of the root Box, so it draws on top).
 *  Decoupled from AppModel: the host wires the action/dismiss callbacks. */
@Composable
fun ToastHost(toast: ToastData?, onAction: () -> Unit, onDismiss: (Int) -> Unit) {
    val c = madarColors()
    // Keep the last shown payload so the exit animation has content to render
    // after `toast` flips back to null.
    var shown by remember { mutableStateOf<ToastData?>(null) }
    if (toast != null) shown = toast

    LaunchedEffect(toast?.id) {
        val cur = toast ?: return@LaunchedEffect
        delay((cur.seconds * 1000).toLong())
        onDismiss(cur.id)
    }

    val accent = when (shown?.tone ?: ChipTone.NEUTRAL) {
        ChipTone.INFO -> c.navy
        ChipTone.ACCENT -> c.accent
        ChipTone.SUCCESS -> c.success
        ChipTone.WARNING -> c.warning
        ChipTone.DANGER -> c.danger
        ChipTone.NEUTRAL -> c.textSecondary
    }

    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.BottomCenter) {
        AnimatedVisibility(
            visible = toast != null,
            enter = fadeIn() + slideInVertically { it / 2 },
            exit = fadeOut() + slideOutVertically { it / 2 },
        ) {
            val data = shown
            if (data != null) {
                Row(
                    Modifier.padding(bottom = 40.dp).widthIn(max = 460.dp)
                        .clip(RoundedCornerShape(999.dp)).background(c.surfaceRaised)
                        .border(1.dp, c.border, RoundedCornerShape(999.dp))
                        .padding(horizontal = Space.lg, vertical = Space.md),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(Space.sm),
                ) {
                    if (data.icon != null) SfIcon(data.icon, tint = accent, size = 16.dp)
                    Text(data.text, color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
                    if (data.actionLabel != null) {
                        Spacer(Modifier.widthIn(min = Space.sm))
                        Text(
                            data.actionLabel,
                            color = accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 13.sp,
                            modifier = Modifier.clickable { onAction() }.padding(PaddingValues(start = Space.xs)),
                        )
                    }
                }
            }
        }
    }
}
