package app.sufrix.ui

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
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

// The Compose mirror of the SwiftUI component library. Same tokens, same shapes,
// same tactile press (scale + haptic). NOT compiled in this checkout (no gradle /
// Android SDK) — verified at the symbol level against the generated binding.

/// The signature tactile press: scale down while held (Flutter AnimatedPressScale).
@Composable
fun Modifier.pressScale(interaction: MutableInteractionSource, down: Float = 0.97f): Modifier {
    val pressed by interaction.collectIsPressedAsState()
    val s by animateFloatAsState(if (pressed) down else 1f, label = "press")
    return this.scale(s)
}

enum class BtnVariant { PRIMARY, SECONDARY, GHOST, DANGER }

@Composable
fun SufrixButton(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    variant: BtnVariant = BtnVariant.PRIMARY,
    loading: Boolean = false,
    enabled: Boolean = true,
    fullWidth: Boolean = true,
    height: Dp = 52.dp,
) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    val fg = when (variant) {
        BtnVariant.PRIMARY -> c.textOnAccent
        BtnVariant.SECONDARY -> c.accent
        BtnVariant.GHOST -> c.textSecondary
        BtnVariant.DANGER -> c.danger
    }
    val bg = when (variant) {
        BtnVariant.PRIMARY -> c.accent
        BtnVariant.SECONDARY, BtnVariant.GHOST -> Color.Transparent
        BtnVariant.DANGER -> c.dangerBg
    }
    Box(
        modifier
            .then(if (fullWidth) Modifier.fillMaxWidth() else Modifier)
            .height(height)
            .pressScale(interaction)
            .clip(RoundedCornerShape(Radii.md))
            .background(bg)
            .then(if (variant == BtnVariant.SECONDARY) Modifier.border(1.5.dp, c.accent, RoundedCornerShape(Radii.md)) else Modifier)
            .clickable(interactionSource = interaction, indication = null, enabled = enabled && !loading) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick()
            }
            .padding(horizontal = Space.lg),
        contentAlignment = Alignment.Center,
    ) {
        if (loading) {
            CircularProgressIndicator(color = fg, strokeWidth = 2.dp, modifier = Modifier.size(20.dp))
        } else {
            Row(horizontalArrangement = Arrangement.spacedBy(Space.sm), verticalAlignment = Alignment.CenterVertically) {
                if (icon != null) Icon(icon, contentDescription = null, tint = fg, modifier = Modifier.size(18.dp))
                Text(label, color = fg, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 15.5.sp)
            }
        }
    }
}

@Composable
fun SufrixTextField(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    secure: Boolean = false,
    enabled: Boolean = true,
    keyboard: KeyboardType = KeyboardType.Text,
) {
    val c = sufrixColors()
    Row(
        modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(Radii.sm))
            .background(c.surfaceAlt)
            .border(1.dp, c.border, RoundedCornerShape(Radii.sm))
            .padding(horizontal = 14.dp, vertical = 13.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            Icon(icon, contentDescription = null, tint = c.textMuted, modifier = Modifier.size(18.dp))
            Spacer(Modifier.width(Space.md))
        }
        Box(Modifier.weight(1f)) {
            if (value.isEmpty()) {
                Text(placeholder, color = c.textMuted, fontFamily = SufrixFont, fontSize = 15.sp)
            }
            BasicTextField(
                value = value,
                onValueChange = onValueChange,
                enabled = enabled,
                singleLine = true,
                textStyle = TextStyle(color = c.textPrimary, fontFamily = SufrixFont, fontSize = 15.sp),
                visualTransformation = if (secure) PasswordVisualTransformation() else VisualTransformation.None,
                keyboardOptions = KeyboardOptions(keyboardType = keyboard),
                cursorBrush = SolidColor(c.accent),
            )
        }
    }
}

enum class ChipTone { INFO, SUCCESS, WARNING, DANGER, NEUTRAL }

@Composable
private fun ChipTone.fg(c: SufrixColors) = when (this) {
    ChipTone.INFO -> c.navy; ChipTone.SUCCESS -> c.success; ChipTone.WARNING -> c.warning
    ChipTone.DANGER -> c.danger; ChipTone.NEUTRAL -> c.textSecondary
}

@Composable
private fun ChipTone.bg(c: SufrixColors) = when (this) {
    ChipTone.INFO -> c.navyBg; ChipTone.SUCCESS -> c.successBg; ChipTone.WARNING -> c.warningBg
    ChipTone.DANGER -> c.dangerBg; ChipTone.NEUTRAL -> c.surfaceAlt
}

@Composable
fun StatusChip(label: String, tone: ChipTone = ChipTone.INFO) {
    val c = sufrixColors()
    Row(
        Modifier.clip(CircleShape).background(tone.bg(c)).padding(horizontal = 11.dp, vertical = 6.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(6.dp).clip(CircleShape).background(tone.fg(c)))
        Text(label, color = tone.fg(c), fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
    }
}

@Composable
fun NoticeBanner(text: String, tone: ChipTone = ChipTone.WARNING, bold: Boolean = false) {
    val c = sufrixColors()
    Box(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(tone.bg(c))
            .border(1.dp, tone.fg(c).copy(alpha = 0.25f), RoundedCornerShape(Radii.sm))
            .padding(horizontal = 14.dp, vertical = 12.dp),
    ) {
        Text(
            text, color = tone.fg(c), fontFamily = SufrixFont,
            fontWeight = if (bold) FontWeight.Bold else FontWeight.Medium, fontSize = 13.sp,
        )
    }
}

@Composable
fun PinPad(pin: String, maxLength: Int = 6, onDigit: (String) -> Unit, onBackspace: () -> Unit) {
    val c = sufrixColors()
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(Space.xl),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            repeat(maxLength) { i ->
                val filled = i < pin.length
                Box(
                    Modifier.size(12.dp).clip(CircleShape)
                        .background(if (filled) c.accent else Color.Transparent)
                        .then(if (filled) Modifier else Modifier.border(1.5.dp, c.border, CircleShape)),
                )
            }
        }
        val rows = listOf(
            listOf("1", "2", "3"), listOf("4", "5", "6"),
            listOf("7", "8", "9"), listOf("", "0", "<"),
        )
        Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            rows.forEach { row ->
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    row.forEach { key ->
                        Box(Modifier.weight(1f)) { PinKey(key, onDigit, onBackspace) }
                    }
                }
            }
        }
    }
}

@Composable
private fun PinKey(key: String, onDigit: (String) -> Unit, onBackspace: () -> Unit) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    when (key) {
        "" -> Box(Modifier.height(56.dp))
        "<" -> Box(
            Modifier.fillMaxWidth().height(56.dp).pressScale(interaction)
                .clickable(interactionSource = interaction, indication = null) {
                    haptic.performHapticFeedback(HapticFeedbackType.LongPress); onBackspace()
                },
            contentAlignment = Alignment.Center,
        ) { Text("⌫", color = c.textMuted, fontFamily = SufrixFont, fontSize = 22.sp) }
        else -> Box(
            Modifier.fillMaxWidth().height(56.dp).pressScale(interaction)
                .clip(RoundedCornerShape(Radii.md)).background(c.surfaceAlt)
                .border(1.dp, c.border, RoundedCornerShape(Radii.md))
                .clickable(interactionSource = interaction, indication = null) {
                    haptic.performHapticFeedback(HapticFeedbackType.LongPress); onDigit(key)
                },
            contentAlignment = Alignment.Center,
        ) { Text(key, color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 21.sp) }
    }
}

/// Brand mark — 4-blade pinwheel + terracotta dot (geometric stand-in for the
/// real Icon.svg, which drops in via Compose resources in the shipping apps).
@Composable
fun SufrixMark(size: Dp = 44.dp, armColor: Color? = null, dotColor: Color = Color(0xFFC25B3F)) {
    val c = sufrixColors()
    val arm = armColor ?: if (c.isDark) Color(0xFFFAF7F2) else Color(0xFF0A2540)
    Box(Modifier.size(size), contentAlignment = Alignment.Center) {
        repeat(4) { i ->
            Box(Modifier.size(size).rotate(i * 90f + 45f), contentAlignment = Alignment.Center) {
                Box(
                    Modifier.size(width = size * 0.40f, height = size * 0.19f)
                        .offset(x = size * 0.17f)
                        .clip(RoundedCornerShape(size * 0.06f))
                        .background(arm),
                )
            }
        }
        Box(Modifier.size(size * 0.17f).clip(CircleShape).background(dotColor))
    }
}

@Composable
fun SufrixLockup(markSize: Dp = 30.dp, textSize: Int = 26, textColor: Color? = null) {
    val c = sufrixColors()
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md)) {
        SufrixMark(size = markSize)
        Text("Sufrix", color = textColor ?: c.textPrimary, fontFamily = SufrixFont,
            fontWeight = FontWeight.Black, fontSize = textSize.sp)
    }
}
