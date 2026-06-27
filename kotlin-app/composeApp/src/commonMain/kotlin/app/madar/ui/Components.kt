package app.madar.ui

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.resources.Res
// Compose Resources emits these as extension properties on Res.drawable in the
// app.madar.resources package; they must be imported to use Res.drawable.Icon/Logo.
import app.madar.resources.Icon
import app.madar.resources.Logo
import org.jetbrains.compose.resources.painterResource

// The Compose mirror of the refined SwiftUI component library — same tokens,
// shapes, circular PIN keys, flat buttons, focus rings, real logo. NOT compiled
// in this checkout (no gradle / Android SDK); kept in lockstep with Swift.

/// The signature tactile press: scale down while held (Flutter AnimatedPressScale).
@Composable
fun Modifier.pressScale(interaction: MutableInteractionSource, down: Float = 0.97f): Modifier {
    val pressed by interaction.collectIsPressedAsState()
    val s by animateFloatAsState(if (pressed) down else 1f, label = "press")
    return this.scale(s)
}

enum class BtnVariant { PRIMARY, OUTLINE, GHOST, DANGER }

@Composable
fun MadarButton(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    variant: BtnVariant = BtnVariant.PRIMARY,
    loading: Boolean = false,
    enabled: Boolean = true,
    fullWidth: Boolean = true,
    height: Dp = Metric.buttonHeight,
    icon: String? = null,
) {
    val c = madarColors()
    val haptics = rememberHaptics()
    val interaction = remember { MutableInteractionSource() }
    val fg = when (variant) {
        BtnVariant.PRIMARY -> c.textOnAccent
        BtnVariant.DANGER -> Color.White
        BtnVariant.OUTLINE -> c.accent
        BtnVariant.GHOST -> c.textSecondary
    }
    val shape = RoundedCornerShape(Radii.md)
    // Premium fill: the primary CTA gets a soft top-lit gradient (a hint of white
    // at the top edge) so it reads as a lifted, glossy surface rather than a flat
    // slab — paired with the accent glow below. Danger stays solid; outline/ghost
    // are transparent.
    val fill: Brush = when (variant) {
        BtnVariant.PRIMARY ->
            if (enabled && !loading) Brush.verticalGradient(listOf(lerp(c.accent, Color.White, 0.16f), c.accent))
            else SolidColor(c.accent.copy(alpha = Opacity.disabled))
        BtnVariant.DANGER ->
            SolidColor(if (enabled && !loading) c.danger else c.danger.copy(alpha = Opacity.disabled))
        BtnVariant.OUTLINE, BtnVariant.GHOST -> SolidColor(Color.Transparent)
    }
    Box(
        modifier
            .then(if (fullWidth) Modifier.fillMaxWidth() else Modifier)
            .height(height)
            .pressScale(interaction, 0.97f)
            // Soft accent glow on the primary CTA so it reads as the brightest thing.
            .then(if (variant == BtnVariant.PRIMARY && enabled && !loading) Modifier.elevation(Elevation.GLOW, shape) else Modifier)
            .clip(shape)
            .background(fill, shape)
            .then(if (variant == BtnVariant.OUTLINE) Modifier.border(1.5.dp, c.accent, shape) else Modifier)
            .clickable(interactionSource = interaction, indication = null, enabled = enabled && !loading) {
                haptics.impact(); onClick()
            }
            .padding(horizontal = Space.lg),
        contentAlignment = Alignment.Center,
    ) {
        if (loading) {
            CircularProgressIndicator(color = fg, strokeWidth = 2.5.dp, modifier = Modifier.size(20.dp))
        } else {
            Row(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                MadarIcon(icon, tint = fg, size = IconSize.md)
                Text(label, color = fg, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 15.sp, letterSpacing = 0.2.sp)
            }
        }
    }
}

@Composable
fun MadarTextField(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
    modifier: Modifier = Modifier,
    secure: Boolean = false,
    enabled: Boolean = true,
    keyboard: KeyboardType = KeyboardType.Text,
    icon: String? = null,
) {
    val c = madarColors()
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(Radii.md)
    // Animate the focus ring (glow + border) instead of snapping it, matching
    // Swift's `.animation(Motion.standard, value: focused)`.
    val glow by animateDpAsState(if (focused) 8.dp else 0.dp, MotionSpec.standard(), "fieldGlow")
    val borderWidth by animateDpAsState(if (focused) 2.dp else 1.dp, MotionSpec.standard(), "fieldBorderW")
    val borderColor by animateColorAsState(if (focused) c.accent else c.border, MotionSpec.standard(), "fieldBorderC")
    val fillColor by animateColorAsState(if (focused) c.surface else c.surfaceAlt, MotionSpec.standard(), "fieldFill")
    Row(
        modifier
            .fillMaxWidth()
            // Dim the whole field while disabled (busy state) — parity with Swift.
            .alpha(if (enabled) 1f else Opacity.disabled)
            .shadow(glow, shape, clip = false, ambientColor = c.accent, spotColor = c.accent)
            .clip(shape)
            .background(fillColor)
            .border(borderWidth, borderColor, shape)
            .padding(horizontal = Space.lg, vertical = 16.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) MadarIcon(icon, tint = if (focused) c.accent else c.textMuted, size = IconSize.lg)
        Box(Modifier.weight(1f)) {
            if (value.isEmpty()) {
                Text(placeholder, color = c.textMuted, fontFamily = LocalMadarFont.current, fontSize = 15.sp)
            }
            BasicTextField(
                value = value,
                onValueChange = onValueChange,
                enabled = enabled,
                singleLine = true,
                modifier = Modifier.fillMaxWidth().onFocusChanged { focused = it.isFocused },
                textStyle = TextStyle(color = c.textPrimary, fontFamily = LocalMadarFont.current, fontSize = 15.sp),
                visualTransformation = if (secure) PasswordVisualTransformation() else VisualTransformation.None,
                keyboardOptions = KeyboardOptions(keyboardType = keyboard),
                cursorBrush = SolidColor(c.accent),
            )
        }
    }
}

enum class ChipTone { INFO, ACCENT, SUCCESS, WARNING, DANGER, NEUTRAL }

fun ChipTone.fg(c: MadarColors) = when (this) {
    ChipTone.INFO -> c.navy; ChipTone.ACCENT -> c.accent; ChipTone.SUCCESS -> c.success
    ChipTone.WARNING -> c.warning; ChipTone.DANGER -> c.danger; ChipTone.NEUTRAL -> c.textSecondary
}

fun ChipTone.bg(c: MadarColors) = when (this) {
    ChipTone.INFO -> c.navyBg; ChipTone.ACCENT -> c.accentBg; ChipTone.SUCCESS -> c.successBg
    ChipTone.WARNING -> c.warningBg; ChipTone.DANGER -> c.dangerBg; ChipTone.NEUTRAL -> c.surfaceAlt
}

@Composable
fun StatusChip(label: String, tone: ChipTone = ChipTone.NEUTRAL, icon: String? = null) {
    val c = madarColors()
    val fg = tone.fg(c)
    Row(
        Modifier.clip(CircleShape).background(tone.bg(c)).border(1.dp, fg.copy(alpha = 0.25f), CircleShape)
            .padding(horizontal = 10.dp, vertical = 5.dp),
        horizontalArrangement = Arrangement.spacedBy(5.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // A leading SF-Symbol icon when given (parity with SwiftUI StatusChip),
        // otherwise the signature tone dot.
        if (icon != null) SfIcon(icon, tint = fg, size = 12.dp)
        else Box(Modifier.size(6.dp).clip(CircleShape).background(fg))
        Text(label, color = fg, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 11.sp,
            maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

@Composable
fun NoticeBanner(
    text: String,
    tone: ChipTone = ChipTone.WARNING,
    bold: Boolean = false,
    icon: String? = null,
    actionLabel: String? = null,
    onAction: (() -> Unit)? = null,
) {
    val c = madarColors()
    val fg = tone.fg(c)
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(tone.bg(c))
            .border(1.dp, fg.copy(alpha = Opacity.border), RoundedCornerShape(Radii.sm))
            .padding(horizontal = 14.dp, vertical = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) MadarIcon(icon, tint = fg, size = IconSize.md)
        // The message takes all remaining width (weight fills) so it never wraps
        // prematurely. The old code gave the text `weight(1f, fill=false)` AND a
        // sibling `Spacer(weight(1f))`, splitting the row 50/50 and forcing short
        // messages to wrap into the empty right half — the banner bug.
        Text(text, color = fg, fontFamily = LocalMadarFont.current,
            fontWeight = if (bold) FontWeight.Bold else FontWeight.Medium, fontSize = 13.sp,
            modifier = Modifier.weight(1f))
        // Trailing call-to-action pill (parity with SwiftUI NoticeBanner). The
        // caller wires onAction; signals the banner is tappable (e.g. auth-paused).
        if (actionLabel != null) {
            Row(
                Modifier.clip(CircleShape).background(fg.copy(alpha = 0.12f))
                    .then(if (onAction != null) Modifier.clickable { onAction() } else Modifier)
                    .padding(horizontal = 10.dp, vertical = 5.dp),
                horizontalArrangement = Arrangement.spacedBy(4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(actionLabel, color = fg, fontFamily = LocalMadarFont.current,
                    fontWeight = FontWeight.Bold, fontSize = 12.sp)
                MadarIcon("chevron.right", tint = fg, size = 10.dp)
            }
        }
    }
}

@Composable
fun PinPad(pin: String, maxLength: Int = 6, keySize: Dp = 64.dp, onDigit: (String) -> Unit, onBackspace: () -> Unit) {
    val c = madarColors()
    // Force LTR so the numeric keypad keeps 1-2-3 / 4-5-6 order in Arabic — POS &
    // phone convention. Without this the digit rows and dots mirror in RTL
    // (parity with Swift's `.environment(\.layoutDirection, .leftToRight)`).
    CompositionLocalProvider(LocalLayoutDirection provides LayoutDirection.Ltr) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.md)) {
            // Dots — spring 12→14, accent glow when filled (mirrors Swift).
            Row(
                Modifier.padding(bottom = Space.sm),
                horizontalArrangement = Arrangement.spacedBy(Space.lg),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                repeat(maxLength) { i ->
                    val filled = i < pin.length
                    val d by animateDpAsState(if (filled) 14.dp else 12.dp, MotionSpec.bouncy(), "dot")
                    Box(
                        Modifier.size(d)
                            .then(if (filled) Modifier.shadow(6.dp, CircleShape, clip = false, ambientColor = c.accent, spotColor = c.accent) else Modifier)
                            .clip(CircleShape)
                            .background(if (filled) c.accent else Color.Transparent)
                            .border(2.dp, if (filled) c.accent else c.border, CircleShape),
                    )
                }
            }
            val rows = listOf(listOf("1", "2", "3"), listOf("4", "5", "6"), listOf("7", "8", "9"), listOf("", "0", "<"))
            rows.forEach { row ->
                Row(horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                    row.forEach { key -> PinKey(key, keySize, onDigit, onBackspace) }
                }
            }
        }
    }
}

@Composable
private fun PinKey(key: String, size: Dp, onDigit: (String) -> Unit, onBackspace: () -> Unit) {
    val c = madarColors()
    val haptics = rememberHaptics()
    val interaction = remember { MutableInteractionSource() }
    val shape = CircleShape
    if (key.isEmpty()) {
        Box(Modifier.size(size))
        return
    }
    Box(
        Modifier.size(size).pressScale(interaction, Motion.pressScaleKey)
            // Subtle raise so the keypad reads as a set of physical keys, not flat
            // discs (parity with Swift's per-key shadow).
            .elevation(Elevation.CARD, shape)
            .clip(shape).background(c.surface)
            .border(1.5.dp, c.border, shape)
            .clickable(interactionSource = interaction, indication = null) {
                haptics.selection()
                if (key == "<") onBackspace() else onDigit(key)
            },
        contentAlignment = Alignment.Center,
    ) {
        if (key == "<") {
            MadarIcon("delete.left", tint = c.textSecondary, size = 22.dp)
        } else {
            Text(
                key, color = c.textPrimary,
                fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 22.sp,
            )
        }
    }
}

/// Brand mark — the REAL Icon asset (default-light renders navy; dark-variant TODO).
@Composable
fun MadarMark(size: Dp = 44.dp, alpha: Float = 1f) {
    androidx.compose.foundation.Image(
        painter = painterResource(Res.drawable.Icon),
        contentDescription = "Madar",
        modifier = Modifier.size(size),
        contentScale = ContentScale.Fit,
        alpha = alpha,
    )
}

/// Full "Madar" wordmark (real Logo asset).
@Composable
fun MadarLockup(height: Dp = 30.dp) {
    androidx.compose.foundation.Image(
        painter = painterResource(Res.drawable.Logo),
        contentDescription = "Madar",
        modifier = Modifier.height(height),
        contentScale = ContentScale.Fit,
    )
}
