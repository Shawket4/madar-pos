package app.madar.ui

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.roundToInt

// Custom modal bottom sheet — the Compose mirror of swift-app Components/
// MadarSheet.swift. The Kotlin app previously hand-rolled this per screen
// (ItemDetailSheet) with no enter/exit animation, and used Material AlertDialog
// elsewhere (Waiter settle), so modals diverged hard from Swift. This is the one
// shared presenter: a scrim + bottom-anchored branded card that slides in on a
// spring, dims the scrim, and supports tap-scrim + drag-down dismissal — then
// animates OUT before unmounting (the piece the old instant-pop sheets missed).
//
// Mount it conditionally like the Swift `.madarSheet(isPresented:)`:
//
//   if (model.showItemSheet) {
//       MadarSheet(onDismiss = { model.showItemSheet = false }) { dismiss ->
//           ItemDetailContent(..., onClose = dismiss)   // close button calls dismiss()
//       }
//   }
//
// `onDismiss` fires AFTER the slide-out completes, so the caller's boolean only
// flips once the card is off-screen — no hard cut.

/** How tall the card may grow (mirrors Swift `SheetSize`). */
enum class SheetSize {
    /** Fills ~88% of the container — the default for medium sheets. */
    AUTO,
    /** Reaches ~94% — big sheets (checkout / tender). */
    LARGE,
    /** Hugs its content up to a 92% cap and scrolls only on overflow — item /
     *  bundle customize sheets that must not stretch into a tall empty void. */
    HUG,
}

@Composable
fun MadarSheet(
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
    size: SheetSize = SheetSize.AUTO,
    maxWidth: Dp = Responsive.sheetMaxWidth,
    content: @Composable ColumnScope.(dismiss: () -> Unit) -> Unit,
) {
    val c = madarColors()
    val density = LocalDensity.current
    val scope = rememberCoroutineScope()

    // Drives the slide + scrim. `shown` is set true once, on first composition, so
    // animateFloatAsState animates the card up from below and fades the scrim in.
    var shown by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { shown = true }

    val shape = RoundedCornerShape(topStart = Radii.xl, topEnd = Radii.xl)

    BoxWithConstraints(modifier.fillMaxSize()) {
        val maxH: Dp = maxHeight * when (size) {
            SheetSize.LARGE -> 0.94f
            SheetSize.HUG -> 0.92f
            SheetSize.AUTO -> 0.88f
        }
        // Fully-hidden translation: the whole container height plus a margin so the
        // shadow clears the bottom edge too.
        val hiddenPx = with(density) { maxHeight.toPx() } + 80f

        // Base slide offset (spring) + the user's live drag offset (separate
        // Animatable so a released-but-not-dismissed drag snaps back smoothly).
        val baseOffset by animateFloatAsState(
            targetValue = if (shown) 0f else hiddenPx,
            animationSpec = MotionSpec.sheet(),
            label = "sheetOffset",
        )
        val dragY = remember { Animatable(0f) }
        val scrimAlpha by animateFloatAsState(
            targetValue = if (shown) Opacity.scrim else 0f,
            animationSpec = MotionSpec.standard(),
            label = "sheetScrim",
        )

        // Animate out, THEN clear the caller's binding (mirrors Swift `close()`).
        fun dismiss() {
            if (!shown) return
            shown = false
            scope.launch { delay(280); onDismiss() }
        }

        // Scrim — tap to dismiss. Sits below the card.
        Box(
            Modifier.fillMaxSize()
                .background(Color.Black.copy(alpha = scrimAlpha))
                .clickable(
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                ) { dismiss() },
        )

        // The card.
        Column(
            Modifier
                .align(Alignment.BottomCenter)
                .widthIn(max = maxWidth)
                .fillMaxWidth()
                .offset { IntOffset(0, (baseOffset + dragY.value).roundToInt()) }
                .then(if (size == SheetSize.HUG) Modifier.heightIn(max = maxH) else Modifier.height(maxH))
                .elevation(Elevation.RAISED, shape)
                .clip(shape)
                .background(c.surface)
                .border(1.dp, c.borderLight, shape),
        ) {
            // Grab handle — drag down past a threshold (or fling) to dismiss.
            Box(
                Modifier.fillMaxWidth()
                    .pointerInput(Unit) {
                        detectVerticalDragGestures(
                            onVerticalDrag = { _, delta ->
                                scope.launch { dragY.snapTo((dragY.value + delta).coerceAtLeast(0f)) }
                            },
                            onDragEnd = {
                                if (dragY.value > maxH.toPx() * 0.28f) dismiss()
                                else scope.launch { dragY.animateTo(0f, MotionSpec.sheet()) }
                            },
                            onDragCancel = {
                                scope.launch { dragY.animateTo(0f, MotionSpec.sheet()) }
                            },
                        )
                    },
                contentAlignment = Alignment.Center,
            ) {
                Box(
                    Modifier.padding(top = Space.md, bottom = Space.sm)
                        .size(width = 40.dp, height = 5.dp)
                        .clip(CircleShape)
                        .background(c.border),
                )
            }

            content(::dismiss)
        }
    }
}
