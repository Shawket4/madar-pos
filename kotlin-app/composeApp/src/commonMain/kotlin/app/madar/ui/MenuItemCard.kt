package app.madar.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.core.MenuItemView
import coil3.compose.AsyncImage
import kotlin.math.ceil

// The catalog's product card — category-hued gradient hero (monogram + a soft
// decorative ring), a live in-cart quantity badge, a subtle shadow, and a fixed
// footer (category accent dot · name · price). Mirror of the SwiftUI MenuItemCard.
@Composable
fun MenuItemCard(
    item: MenuItemView,
    categoryName: String,
    currency: String,
    inCartQty: Long,
    onTap: () -> Unit,
) {
    val c = madarColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    val style = remember(categoryName, item.name, c.isDark) {
        categoryStyle(categoryName.ifBlank { item.name }, c.isDark)
    }
    val cardShape = RoundedCornerShape(Radii.md)
    Column(
        // Lock every card to ONE shape → uniform heights → even grid rows, regardless
        // of the image loading or the name wrapping (Flutter's childAspectRatio).
        Modifier.fillMaxWidth().aspectRatio(0.94f).pressScale(interaction)
            .elevation(Elevation.CARD, cardShape)
            .clip(cardShape).background(c.surface)
            .border(1.dp, c.borderLight, cardShape)
            .clickable(interactionSource = interaction, indication = null) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress); onTap()
            },
    ) {
        // ── Hero (gradient + monogram + decorative ring + in-cart badge) ──────
        Box(
            // Fills the space above the footer; the card's aspectRatio governs shape.
            Modifier.fillMaxWidth().weight(1f).clipToBounds()
                .background(Brush.linearGradient(listOf(style.bgTop, style.bgBottom))),
            contentAlignment = Alignment.Center,
        ) {
            Box(
                Modifier.size(130.dp).align(Alignment.BottomEnd)
                    .offset(x = if (LocalLayoutDirection.current == LayoutDirection.Rtl) (-46).dp else 46.dp, y = 46.dp)
                    .border(2.dp, style.accent.copy(alpha = 0.16f), CircleShape),
            )
            Text(
                monogram(item.name),
                color = style.accent.copy(alpha = if (c.isDark) 0.7f else 0.55f),
                fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Light, fontSize = 42.sp,
            )
            // Real photo (when present) covers the gradient/monogram once loaded;
            // while loading or on failure Coil draws nothing, so the gradient shows.
            val url = item.imageUrl
            if (!url.isNullOrBlank()) {
                AsyncImage(
                    model = url,
                    contentDescription = null,
                    modifier = Modifier.matchParentSize(),
                    contentScale = ContentScale.Crop,
                )
            }
            if (inCartQty > 0) {
                Box(
                    Modifier.align(Alignment.TopEnd).padding(7.dp)
                        .clip(CircleShape).background(c.accent)
                        .border(1.5.dp, c.surface, CircleShape)
                        .padding(horizontal = 6.dp, vertical = 3.dp),
                ) {
                    Text(
                        "$inCartQty", color = c.textOnAccent, fontFamily = LocalMadarFont.current,
                        fontWeight = FontWeight.Black, fontSize = 12.sp,
                    )
                }
            }
        }
        // ── Footer (accent dot · name · price) ───────────────────────────────
        Row(
            Modifier.fillMaxWidth().height(48.dp).background(c.surface).padding(horizontal = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            Box(Modifier.size(7.dp).clip(CircleShape).background(style.accent))
            Text(
                item.name, color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold,
                fontSize = 12.sp, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
            )
            Text(
                Money.format(item.basePriceMinor, currency), color = c.textSecondary,
                fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 11.sp,
            )
        }
    }
}

private val WHITESPACE = Regex("\\s+")

/** Up to two initials from the item name (Flutter's monogram rule). */
private fun monogram(name: String): String {
    val words = name.split(WHITESPACE).filter { it.isNotBlank() }
    return when {
        words.size >= 2 -> (words[0].take(1) + words[1].take(1)).uppercase()
        words.isNotEmpty() -> words[0].take(2).uppercase()
        else -> "•"
    }
}

// ── Category palette (HSL hue seeded by name — matches SwiftUI Color(h:s:l:)) ──

data class CategoryStyle(val bgTop: Color, val bgBottom: Color, val accent: Color)

/** Theme-aware gradient + accent, hue-seeded by keyword so a family shares a
 *  palette (coffee → warm brown, tea → green, …); unknown names hash to a hue. */
fun categoryStyle(name: String, isDark: Boolean): CategoryStyle {
    val (hue, sat) = categoryHueSat(name)
    return if (isDark) {
        CategoryStyle(
            Color.hsl(hue, sat * 0.55f, 0.175f),
            Color.hsl(hue, sat * 0.60f, 0.13f),
            Color.hsl(hue, sat, 0.62f),
        )
    } else {
        CategoryStyle(
            Color.hsl(hue, sat, 0.945f),
            Color.hsl(hue, sat, 0.875f),
            Color.hsl(hue, sat, 0.40f),
        )
    }
}

private fun categoryHueSat(raw: String): Pair<Float, Float> {
    val n = raw.lowercase()
    fun has(vararg keys: String) = keys.any { n.contains(it) }
    if (has("matcha")) return 130f to 0.45f
    if (has("mocha", "chocolate", "cocoa")) return 16f to 0.45f
    if (has("coffee", "latte", "espresso", "cappuccino", "americano", "macchiato", "cortado", "flat white")) return 28f to 0.40f
    if (has("tea", "chai")) return 140f to 0.38f
    if (has("juice", "lemon", "orange", "mango", "berry", "smoothie")) return 45f to 0.55f
    if (has("water", "sparkling", "soda")) return 205f to 0.38f
    if (has("ice", "iced", "cold", "frapp", "shake")) return 200f to 0.45f
    if (has("pastry", "croissant", "cake", "waffle", "cookie", "muffin", "donut", "brownie")) return 38f to 0.50f
    if (has("sandwich", "burger", "chicken", "wrap", "toast", "bagel", "food")) return 22f to 0.52f
    if (has("affogato", "ice cream", "dessert", "gelato")) return 290f to 0.42f
    // Stable fallback hue from an FNV-1a hash of the name (parity with Swift).
    var hash = 1469598103934665603uL
    for (b in n.encodeToByteArray()) hash = (hash xor (b.toInt() and 0xFF).toULong()) * 1099511628211uL
    return (hash % 360uL).toLong().toFloat() to 0.42f
}

/** A `GridCells` that caps cell width at [maxExtent] (Flutter's
 *  maxCrossAxisExtent) — more columns as the area widens, never giant cards. */
class MaxExtentCells(private val maxExtent: Dp) : GridCells {
    override fun Density.calculateCrossAxisCellSizes(
        availableSize: Int,
        spacing: Int,
    ): List<Int> {
        val maxPx = maxExtent.roundToPx()
        val count = maxOf(1, ceil((availableSize + spacing).toDouble() / (maxPx + spacing)).toInt())
        val totalSpacing = spacing * (count - 1)
        val base = (availableSize - totalSpacing) / count
        val rem = (availableSize - totalSpacing) % count
        return List(count) { base + if (it < rem) 1 else 0 }
    }

    override fun hashCode() = maxExtent.hashCode()
    override fun equals(other: Any?) = other is MaxExtentCells && other.maxExtent == maxExtent
}
