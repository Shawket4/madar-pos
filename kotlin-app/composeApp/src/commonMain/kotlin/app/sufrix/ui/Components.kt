package app.sufrix.ui

import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.sufrix.resources.Res
// Compose Resources emits these as extension properties on Res.drawable in the
// app.sufrix.resources package; they must be imported to use Res.drawable.Icon/Logo.
import app.sufrix.resources.Icon
import app.sufrix.resources.Logo
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
fun SufrixButton(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    variant: BtnVariant = BtnVariant.PRIMARY,
    loading: Boolean = false,
    enabled: Boolean = true,
    fullWidth: Boolean = true,
    height: Dp = 50.dp,
) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    val fg = when (variant) {
        BtnVariant.PRIMARY -> c.textOnAccent
        BtnVariant.DANGER -> Color.White
        BtnVariant.OUTLINE -> c.accent
        BtnVariant.GHOST -> c.textSecondary
    }
    val base = when (variant) {
        BtnVariant.PRIMARY -> c.accent
        BtnVariant.DANGER -> c.danger
        BtnVariant.OUTLINE, BtnVariant.GHOST -> Color.Transparent
    }
    val bg = if (enabled && !loading) base else base.copy(alpha = 0.45f)
    Box(
        modifier
            .then(if (fullWidth) Modifier.fillMaxWidth() else Modifier)
            .height(height)
            .pressScale(interaction, 0.975f)
            .clip(RoundedCornerShape(Radii.sm))
            .background(bg)
            .then(if (variant == BtnVariant.OUTLINE) Modifier.border(1.5.dp, c.accent, RoundedCornerShape(Radii.sm)) else Modifier)
            .clickable(interactionSource = interaction, indication = null, enabled = enabled && !loading) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick()
            }
            .padding(horizontal = Space.lg),
        contentAlignment = Alignment.Center,
    ) {
        if (loading) {
            CircularProgressIndicator(color = fg, strokeWidth = 2.5.dp, modifier = Modifier.size(20.dp))
        } else {
            Text(label, color = fg, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 14.sp)
        }
    }
}

@Composable
fun SufrixTextField(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
    modifier: Modifier = Modifier,
    secure: Boolean = false,
    enabled: Boolean = true,
    keyboard: KeyboardType = KeyboardType.Text,
) {
    val c = sufrixColors()
    var focused by remember { mutableStateOf(false) }
    Row(
        modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(Radii.sm))
            .background(c.surface)
            .border(if (focused) 2.dp else 1.dp, if (focused) c.accent else c.border, RoundedCornerShape(Radii.sm))
            .padding(horizontal = 16.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.fillMaxWidth()) {
            if (value.isEmpty()) {
                Text(placeholder, color = c.textMuted, fontFamily = SufrixFont, fontSize = 15.sp)
            }
            BasicTextField(
                value = value,
                onValueChange = onValueChange,
                enabled = enabled,
                singleLine = true,
                modifier = Modifier.fillMaxWidth().onFocusChanged { focused = it.isFocused },
                textStyle = TextStyle(color = c.textPrimary, fontFamily = SufrixFont, fontSize = 15.sp),
                visualTransformation = if (secure) PasswordVisualTransformation() else VisualTransformation.None,
                keyboardOptions = KeyboardOptions(keyboardType = keyboard),
                cursorBrush = SolidColor(c.accent),
            )
        }
    }
}

enum class ChipTone { INFO, ACCENT, SUCCESS, WARNING, DANGER, NEUTRAL }

@Composable
private fun ChipTone.fg(c: SufrixColors) = when (this) {
    ChipTone.INFO -> c.navy; ChipTone.ACCENT -> c.accent; ChipTone.SUCCESS -> c.success
    ChipTone.WARNING -> c.warning; ChipTone.DANGER -> c.danger; ChipTone.NEUTRAL -> c.textSecondary
}

@Composable
private fun ChipTone.bg(c: SufrixColors) = when (this) {
    ChipTone.INFO -> c.navyBg; ChipTone.ACCENT -> c.accentBg; ChipTone.SUCCESS -> c.successBg
    ChipTone.WARNING -> c.warningBg; ChipTone.DANGER -> c.dangerBg; ChipTone.NEUTRAL -> c.surfaceAlt
}

@Composable
fun StatusChip(label: String, tone: ChipTone = ChipTone.NEUTRAL) {
    val c = sufrixColors()
    val fg = tone.fg(c)
    Row(
        Modifier.clip(CircleShape).background(tone.bg(c)).border(1.dp, fg.copy(alpha = 0.25f), CircleShape)
            .padding(horizontal = 10.dp, vertical = 5.dp),
        horizontalArrangement = Arrangement.spacedBy(5.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(6.dp).clip(CircleShape).background(fg))
        Text(label, color = fg, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 11.sp)
    }
}

@Composable
fun NoticeBanner(text: String, tone: ChipTone = ChipTone.WARNING, bold: Boolean = false) {
    val c = sufrixColors()
    val fg = tone.fg(c)
    Box(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(tone.bg(c))
            .border(1.dp, fg.copy(alpha = 0.25f), RoundedCornerShape(Radii.sm))
            .padding(horizontal = 14.dp, vertical = 12.dp),
    ) {
        Text(text, color = fg, fontFamily = SufrixFont,
            fontWeight = if (bold) FontWeight.Bold else FontWeight.Medium, fontSize = 13.sp)
    }
}

@Composable
fun PinPad(pin: String, maxLength: Int = 6, keySize: Dp = 64.dp, onDigit: (String) -> Unit, onBackspace: () -> Unit) {
    val c = sufrixColors()
    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.md)) {
        // Dots — spring 12→14, glow when filled.
        Row(horizontalArrangement = Arrangement.spacedBy(Space.lg), verticalAlignment = Alignment.CenterVertically) {
            repeat(maxLength) { i ->
                val filled = i < pin.length
                val d by animateDpAsState(if (filled) 14.dp else 12.dp, spring(), "dot")
                Box(
                    Modifier.size(d).clip(CircleShape)
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

@Composable
private fun PinKey(key: String, size: Dp, onDigit: (String) -> Unit, onBackspace: () -> Unit) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    if (key.isEmpty()) {
        Box(Modifier.size(size))
        return
    }
    Box(
        Modifier.size(size).pressScale(interaction, 0.92f).clip(CircleShape).background(c.surface)
            .border(1.5.dp, c.border, CircleShape)
            .clickable(interactionSource = interaction, indication = null) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                if (key == "<") onBackspace() else onDigit(key)
            },
        contentAlignment = Alignment.Center,
    ) {
        Text(
            if (key == "<") "⌫" else key,
            color = if (key == "<") c.textSecondary else c.textPrimary,
            fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 22.sp,
        )
    }
}

/// Brand mark — the REAL Icon asset (default-light renders navy; dark-variant TODO).
@Composable
fun SufrixMark(size: Dp = 44.dp, alpha: Float = 1f) {
    androidx.compose.foundation.Image(
        painter = painterResource(Res.drawable.Icon),
        contentDescription = "Sufrix",
        modifier = Modifier.size(size),
        contentScale = ContentScale.Fit,
        alpha = alpha,
    )
}

/// Full "Sufrix" wordmark (real Logo asset).
@Composable
fun SufrixLockup(height: Dp = 30.dp) {
    androidx.compose.foundation.Image(
        painter = painterResource(Res.drawable.Logo),
        contentDescription = "Sufrix",
        modifier = Modifier.height(height),
        contentScale = ContentScale.Fit,
    )
}
