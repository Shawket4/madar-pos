package app.madar.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

// Shared layout primitives — the Compose mirror of swift-app Components/
// SharedComponents.swift. Identical API + visuals so the two platforms stay in
// lockstep. Tokens-only (Space/Radii/IconSize/Metric/Type) + theme colors.

/** Bordered surface container (Flutter SurfaceCard / Swift MadarCard). */
@Composable
fun MadarCard(
    modifier: Modifier = Modifier,
    padding: PaddingValues = PaddingValues(Space.lg),
    radius: androidx.compose.ui.unit.Dp = Radii.lg,
    spacing: androidx.compose.ui.unit.Dp = Space.md,
    elevated: Boolean = true,
    content: @Composable ColumnScope.() -> Unit,
) {
    val c = madarColors()
    val shape = RoundedCornerShape(radius)
    Column(
        modifier
            .fillMaxWidth()
            .then(if (elevated) Modifier.elevation(Elevation.CARD, shape) else Modifier)
            .clip(shape)
            .background(c.surface)
            .border(1.dp, c.borderLight, shape)
            .padding(padding),
        verticalArrangement = Arrangement.spacedBy(spacing),
        content = content,
    )
}

/** Uppercase muted section label with a signature accent tick, optional leading
 *  icon / trailing count. The 3dp accent capsule gives every section a small
 *  branded anchor instead of a bare grey label (the "bolder" refresh). */
@Composable
fun SectionHeader(text: String, icon: String? = null, trailing: String? = null) {
    val c = madarColors()
    Row(
        Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            MadarIcon(icon, tint = c.accent, size = IconSize.xs)
        } else {
            Box(Modifier.size(width = 3.dp, height = 12.dp).clip(CircleShape).background(c.accent))
        }
        Text(
            text.uppercase(), style = Type.label(), color = c.textSecondary,
            letterSpacing = Motion.trackingSp.sp,
        )
        if (trailing != null) Text(trailing, style = Type.label(), color = c.textSecondary)
    }
}

/** Back chevron + title (+subtitle / loading / trailing). */
@Composable
fun ScreenHeader(
    title: String,
    subtitle: String? = null,
    isLoading: Boolean = false,
    onBack: (() -> Unit)? = null,
    trailing: @Composable RowScope.() -> Unit = {},
) {
    val c = madarColors()
    Row(
        Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(Space.md),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (onBack != null) {
            val interaction = remember { MutableInteractionSource() }
            Box(
                Modifier.size(Metric.closeButton).pressScale(interaction)
                    .clip(RoundedCornerShape(Radii.sm)).background(c.surfaceAlt)
                    .border(1.dp, c.border, RoundedCornerShape(Radii.sm))
                    .clickable(interactionSource = interaction, indication = null) { onBack() },
                contentAlignment = Alignment.Center,
            ) { MadarIcon("chevron.backward", tint = c.textPrimary, size = 17.dp) }
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(title, style = Type.h2(), color = c.textPrimary)
            if (subtitle != null) Text(subtitle, style = Type.bodySm(), color = c.textMuted)
        }
        if (isLoading) CircularProgressIndicator(color = c.accent, strokeWidth = 2.dp, modifier = Modifier.size(18.dp))
        trailing()
    }
}

/** Top-bar chrome for a ScreenHeader: surface fill, padding, bottom hairline. */
@Composable
fun Modifier.screenHeaderBar(): Modifier {
    val c = madarColors()
    return this
        .fillMaxWidth()
        .background(c.surface)
        .drawBehind {
            drawLine(c.border, Offset(0f, size.height), Offset(size.width, size.height), 1f)
        }
        .padding(horizontal = Space.lg, vertical = Space.md)
}

/** Label ↔ value row (tabular money), optional tone / emphasis / leading icon. */
@Composable
fun MetricRow(
    label: String,
    value: String,
    tone: ChipTone? = null,
    emphasize: Boolean = false,
    icon: String? = null,
) {
    val c = madarColors()
    val labelColor = tone?.fg(c) ?: c.textSecondary
    val valueColor = tone?.fg(c) ?: c.textPrimary
    Row(
        Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(Space.md),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) MadarIcon(icon, tint = c.textMuted, size = IconSize.sm)
        Text(label, style = if (emphasize) Type.title() else Type.bodySm(), color = labelColor)
        Box(Modifier.weight(1f))
        Text(
            value,
            style = if (emphasize) Type.moneyLg() else Type.money(),
            color = valueColor,
        )
    }
}

/** Active/inactive toggle chip (payment / tip / filter / quick-cash). */
@Composable
fun SelectableChip(
    label: String,
    isSelected: Boolean,
    onTap: () -> Unit,
    icon: String? = null,
    trailingValue: String? = null,
    tone: ChipTone = ChipTone.ACCENT,
) {
    val c = madarColors()
    val interaction = remember { MutableInteractionSource() }
    val fg = if (isSelected) c.textOnAccent else c.textSecondary
    Row(
        Modifier.pressScale(interaction)
            // Selected chips lift with a soft accent glow so the active filter /
            // payment method pops off the row (the "bolder" refresh).
            .then(if (isSelected) Modifier.elevation(Elevation.GLOW, CircleShape) else Modifier)
            .clip(CircleShape)
            .background(if (isSelected) tone.fg(c) else c.surfaceAlt)
            .border(1.dp, if (isSelected) Color.Transparent else c.border, CircleShape)
            .clickable(interactionSource = interaction, indication = null) { onTap() }
            .padding(horizontal = Space.md, vertical = Space.sm),
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) MadarIcon(icon, tint = fg, size = IconSize.sm)
        Text(label, style = Type.title(), color = fg)
        if (trailingValue != null) Text(trailingValue, style = Type.money(12.sp, FontWeight.Bold), color = fg)
    }
}

/** Centered icon + title (+subtitle) for empty grids / lists. */
@Composable
fun EmptyState(icon: String, title: String, subtitle: String? = null) {
    val c = madarColors()
    Column(
        Modifier.fillMaxSize().padding(Space.xl),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(Space.md, Alignment.CenterVertically),
    ) {
        MadarIcon(icon, tint = c.textMuted, size = 40.dp)
        Text(title, style = Type.h3(), color = c.textPrimary)
        if (subtitle != null) Text(subtitle, style = Type.bodySm(), color = c.textMuted)
    }
}
