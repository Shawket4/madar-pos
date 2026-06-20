package app.sufrix

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.sufrix.core.CategoryView
import app.sufrix.core.MenuItemView
import app.sufrix.core.ShiftView
import app.sufrix.ui.Money
import app.sufrix.ui.Radii
import app.sufrix.ui.Space
import app.sufrix.ui.StatusChip
import app.sufrix.ui.ChipTone
import app.sufrix.ui.SufrixFont
import app.sufrix.ui.SufrixMark
import app.sufrix.ui.pressScale
import app.sufrix.ui.sufrixColors
import app.sufrix.ui.t

// Order screen — the heart of the POS. Per the design language the order screen's
// action bar is the only nav hub (no tabs/shells). This phase: browse the
// branch-effective catalog (category strip + item grid), served from the local
// mirror so it works offline. Tap-to-cart + tender land in the next phases.
// Mirror of the SwiftUI OrderView.
@Composable
fun OrderScreen(model: AppModel) {
    val c = sufrixColors()
    var selectedCategory by remember { mutableStateOf<String?>(null) }
    var search by remember { mutableStateOf("") }
    val currency = model.session?.currencyCode ?: ""

    // Reconcile the shift (catches a dashboard force-close) and load the catalog
    // (fresh when online, cached otherwise) on appear.
    LaunchedEffect(Unit) {
        model.reconcileShift()
        model.loadCatalog()
    }

    val visible = model.menuItems
        .filter { it.isActive }
        .filter { selectedCategory == null || it.categoryId == selectedCategory }
        .filter { search.isBlank() || it.name.contains(search, ignoreCase = true) }

    Column(Modifier.fillMaxSize().background(c.bg)) {
        OrderTopBar(model)
        CategoryStrip(model.categories, selectedCategory) { selectedCategory = it }
        SearchField(
            search, { search = it }, t("order.search"),
            Modifier.padding(horizontal = Space.lg).padding(bottom = Space.sm),
        )
        Box(Modifier.fillMaxWidth().weight(1f)) {
            ItemGridOrEmpty(visible, currency, searching = search.isNotBlank())
        }
    }
}

// ── Top action bar (the only nav hub) ───────────────────────────────────────────
@Composable
private fun OrderTopBar(model: AppModel) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    Column(Modifier.fillMaxWidth().background(c.surface)) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.md),
        ) {
            SufrixMark(size = 32.dp)
            model.shift?.let { StatusChip(it.tellerName, ChipTone.INFO) }
            Box(Modifier.weight(1f))
            Text(
                t("home.sign_out"),
                color = c.textSecondary, fontFamily = SufrixFont,
                fontWeight = FontWeight.SemiBold, fontSize = 13.sp,
                modifier = Modifier.pressScale(interaction).clickable(
                    interactionSource = interaction, indication = null,
                ) {
                    haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                    model.signOut()
                },
            )
        }
        // Hairline divider under the bar.
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
    }
}

// ── Category strip ──────────────────────────────────────────────────────────────
@Composable
private fun CategoryStrip(categories: List<CategoryView>, selected: String?, onSelect: (String?) -> Unit) {
    Row(
        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState())
            .padding(horizontal = Space.lg, vertical = Space.md),
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        CategoryChip(t("order.all"), active = selected == null) { onSelect(null) }
        categories.filter { it.isActive }.forEach { cat ->
            CategoryChip(cat.name, active = selected == cat.id) { onSelect(cat.id) }
        }
    }
}

@Composable
private fun CategoryChip(label: String, active: Boolean, onClick: () -> Unit) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    Text(
        label,
        color = if (active) c.textOnAccent else c.textSecondary,
        fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 13.sp,
        modifier = Modifier.pressScale(interaction).clip(CircleShape)
            .background(if (active) c.accent else c.surface)
            .border(1.dp, if (active) Color.Transparent else c.border, CircleShape)
            .clickable(interactionSource = interaction, indication = null) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick()
            }
            .padding(horizontal = Space.lg, vertical = Space.sm),
    )
}

// ── Item grid ───────────────────────────────────────────────────────────────────
@Composable
private fun ItemGridOrEmpty(items: List<MenuItemView>, currency: String, searching: Boolean) {
    val c = sufrixColors()
    if (items.isEmpty()) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.md)) {
                Text(if (searching) "⌕" else "▦", color = c.textMuted, fontSize = 40.sp)
                Text(
                    t(if (searching) "order.empty_search" else "order.empty"),
                    color = c.textSecondary, fontFamily = SufrixFont, fontSize = 14.sp,
                )
            }
        }
    } else {
        LazyVerticalGrid(
            columns = GridCells.Adaptive(minSize = 150.dp),
            modifier = Modifier.fillMaxSize(),
            contentPadding = androidx.compose.foundation.layout.PaddingValues(Space.lg),
            horizontalArrangement = Arrangement.spacedBy(Space.md),
            verticalArrangement = Arrangement.spacedBy(Space.md),
        ) {
            items(items, key = { it.id }) { item -> ItemCard(item, currency) }
        }
    }
}

@Composable
private fun ItemCard(item: MenuItemView, currency: String) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    Column(
        Modifier.fillMaxWidth().pressScale(interaction).clip(RoundedCornerShape(Radii.md))
            .background(c.surface).border(1.dp, c.border, RoundedCornerShape(Radii.md))
            .clickable(interactionSource = interaction, indication = null) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                // Tap-to-add lands with the cart phase.
            }
            .padding(Space.md),
        verticalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Monogram(item.name)
        Text(
            item.name, color = c.textPrimary, fontFamily = SufrixFont,
            fontWeight = FontWeight.SemiBold, fontSize = 14.sp, maxLines = 2,
        )
        Text(
            Money.format(item.basePriceMinor, currency),
            color = c.accent, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 14.sp,
        )
    }
}

/// A branded image stand-in — the item's initial on a tinted tile. (Real menu
/// images get an async loader in a later polish phase, added to both platforms.)
@Composable
private fun Monogram(name: String) {
    val c = sufrixColors()
    val initial = name.trim().take(1).uppercase().ifEmpty { "•" }
    Box(
        Modifier.fillMaxWidth().aspectRatio(1.4f).clip(RoundedCornerShape(Radii.sm)).background(c.accentBg),
        contentAlignment = Alignment.Center,
    ) {
        Text(initial, color = c.accent.copy(alpha = 0.7f), fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 28.sp)
    }
}

// ── Search field ────────────────────────────────────────────────────────────────
@Composable
private fun SearchField(value: String, onChange: (String) -> Unit, placeholder: String, modifier: Modifier = Modifier) {
    val c = sufrixColors()
    Row(
        modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(Radii.sm))
            .padding(horizontal = Space.lg, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Text("⌕", color = c.textMuted, fontSize = 16.sp)
        Box(Modifier.weight(1f)) {
            if (value.isEmpty()) {
                Text(placeholder, color = c.textMuted, fontFamily = SufrixFont, fontSize = 15.sp)
            }
            BasicTextField(
                value = value, onValueChange = onChange, singleLine = true,
                modifier = Modifier.fillMaxWidth(),
                textStyle = TextStyle(color = c.textPrimary, fontFamily = SufrixFont, fontSize = 15.sp),
                cursorBrush = SolidColor(c.accent),
            )
        }
        if (value.isNotEmpty()) {
            Text(
                "✕", color = c.textMuted, fontSize = 14.sp,
                modifier = Modifier.clickable { onChange("") },
            )
        }
    }
}

/** "EGP 500.00" — opening cash, formatted from minor units. */
fun ShiftView.currencyDisplay(code: String): String = Money.format(openingCashMinor, code)
