package app.sufrix

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
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
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.sufrix.core.CartLineView
import app.sufrix.core.CartTotals
import app.sufrix.core.CategoryView
import app.sufrix.core.MenuItemView
import app.sufrix.core.ShiftView
import app.sufrix.ui.ChipTone
import app.sufrix.ui.Money
import app.sufrix.ui.Radii
import app.sufrix.ui.Space
import app.sufrix.ui.StatusChip
import app.sufrix.ui.SufrixButton
import app.sufrix.ui.SufrixFont
import app.sufrix.ui.SufrixMark
import app.sufrix.ui.pressScale
import app.sufrix.ui.sufrixColors
import app.sufrix.ui.t

// Order screen — the heart of the POS. Per the design language the order screen's
// action bar is the only nav hub (no tabs/shells). Browse the branch-effective
// catalog (offline-safe) and build a cart: tap an item to add it, adjust
// quantities, see live totals. On wide layouts (iPad / desktop) the cart is a
// column beside the grid; on phones it's a bottom bar that opens a drawer.
// Mirror of the SwiftUI OrderView.
@Composable
fun OrderScreen(model: AppModel) {
    val c = sufrixColors()
    var selectedCategory by remember { mutableStateOf<String?>(null) }
    var search by remember { mutableStateOf("") }
    var showCart by remember { mutableStateOf(false) }
    val currency = model.session?.currencyCode ?: ""

    // Reconcile the shift (catches a dashboard force-close) and load the catalog
    // + cart (fresh when online, cached otherwise) on appear.
    LaunchedEffect(Unit) {
        model.reconcileShift()
        model.loadCatalog()
    }

    val visible = model.menuItems
        .filter { it.isActive }
        .filter { selectedCategory == null || it.categoryId == selectedCategory }
        .filter { search.isBlank() || it.name.contains(search, ignoreCase = true) }

    BoxWithConstraints(Modifier.fillMaxSize().background(c.bg)) {
        val wide = maxWidth >= 760.dp
        Column(Modifier.fillMaxSize()) {
            OrderTopBar(model)
            if (wide) {
                Row(Modifier.fillMaxSize()) {
                    Box(Modifier.weight(1f).fillMaxHeight()) {
                        CatalogColumn(model.categories, visible, currency, selectedCategory, { selectedCategory = it }, search, { search = it }, model::addToCart)
                    }
                    Box(Modifier.width(1.dp).fillMaxHeight().background(c.border))
                    Box(Modifier.width(340.dp).fillMaxHeight()) {
                        CartPanel(model, currency)
                    }
                }
            } else {
                Box(Modifier.weight(1f).fillMaxWidth()) {
                    CatalogColumn(model.categories, visible, currency, selectedCategory, { selectedCategory = it }, search, { search = it }, model::addToCart)
                }
                CartBar(model, currency) { showCart = true }
            }
        }

        // Phone cart drawer — scrim (tap to dismiss) + a bottom sheet panel.
        if (!wide && showCart) {
            Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.4f))
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { showCart = false })
            Box(
                Modifier.fillMaxWidth().fillMaxHeight(0.88f).align(Alignment.BottomCenter)
                    .clip(RoundedCornerShape(topStart = Radii.lg, topEnd = Radii.lg)).background(c.bg)
                    .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {},
            ) {
                CartPanel(model, currency, onClose = { showCart = false })
            }
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
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
    }
}

// ── Catalog column (category strip + search + grid) ─────────────────────────────
@Composable
private fun CatalogColumn(
    categories: List<CategoryView>,
    items: List<MenuItemView>,
    currency: String,
    selectedCategory: String?,
    onSelect: (String?) -> Unit,
    search: String,
    onSearch: (String) -> Unit,
    onAdd: (MenuItemView) -> Unit,
) {
    Column(Modifier.fillMaxSize()) {
        CategoryStrip(categories, selectedCategory, onSelect)
        SearchField(
            search, onSearch, t("order.search"),
            Modifier.padding(horizontal = Space.lg).padding(bottom = Space.sm),
        )
        Box(Modifier.weight(1f).fillMaxWidth()) {
            ItemGridOrEmpty(items, currency, searching = search.isNotBlank(), onAdd)
        }
    }
}

// ── Category strip ──────────────────────────────────────────────────────────────
@Composable
private fun CategoryStrip(cats: List<CategoryView>, selected: String?, onSelect: (String?) -> Unit) {
    Row(
        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState())
            .padding(horizontal = Space.lg, vertical = Space.md),
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        CategoryChip(t("order.all"), active = selected == null) { onSelect(null) }
        cats.filter { it.isActive }.forEach { cat ->
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
private fun ItemGridOrEmpty(items: List<MenuItemView>, currency: String, searching: Boolean, onAdd: (MenuItemView) -> Unit) {
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
            contentPadding = PaddingValues(Space.lg),
            horizontalArrangement = Arrangement.spacedBy(Space.md),
            verticalArrangement = Arrangement.spacedBy(Space.md),
        ) {
            items(items, key = { it.id }) { item -> ItemCard(item, currency) { onAdd(item) } }
        }
    }
}

@Composable
private fun ItemCard(item: MenuItemView, currency: String, onAdd: () -> Unit) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    Column(
        Modifier.fillMaxWidth().pressScale(interaction).clip(RoundedCornerShape(Radii.md))
            .background(c.surface).border(1.dp, c.border, RoundedCornerShape(Radii.md))
            .clickable(interactionSource = interaction, indication = null) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress); onAdd()
            }
            .padding(Space.md),
        verticalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Monogram(item.name)
        Text(item.name, color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, maxLines = 2)
        Text(Money.format(item.basePriceMinor, currency), color = c.accent, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 14.sp)
    }
}

/// A branded image stand-in — the item's initial on a tinted tile.
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
            Text("✕", color = c.textMuted, fontSize = 14.sp, modifier = Modifier.clickable { onChange("") })
        }
    }
}

// ── Cart panel (wide column + phone drawer) ─────────────────────────────────────
@Composable
private fun CartPanel(model: AppModel, currency: String, onClose: (() -> Unit)? = null) {
    val c = sufrixColors()
    Column(Modifier.fillMaxSize().background(c.bg)) {
        Row(
            Modifier.fillMaxWidth().padding(Space.lg),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            Text(t("order.cart"), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 18.sp)
            if (model.cartTotals.itemCount > 0) StatusChip("${model.cartTotals.itemCount}", ChipTone.ACCENT)
            Box(Modifier.weight(1f))
            if (model.cartLines.isNotEmpty()) {
                Text(
                    t("order.clear"), color = c.danger, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 13.sp,
                    modifier = Modifier.clickable { model.clearCart() },
                )
            }
            if (onClose != null) {
                Text("✕", color = c.textMuted, fontSize = 16.sp, modifier = Modifier.padding(start = Space.sm).clickable { onClose() })
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))

        if (model.cartLines.isEmpty()) {
            Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                Text(t("order.cart_empty"), color = c.textSecondary, fontFamily = SufrixFont, fontSize = 14.sp)
            }
        } else {
            LazyColumn(
                Modifier.weight(1f).fillMaxWidth(),
                contentPadding = PaddingValues(Space.lg),
                verticalArrangement = Arrangement.spacedBy(Space.sm),
            ) {
                items(model.cartLines, key = { it.itemId }) { line ->
                    CartLineRow(
                        line, currency,
                        onDec = { model.setCartQty(line.itemId, line.qty - 1) },
                        onInc = { model.setCartQty(line.itemId, line.qty + 1) },
                    )
                }
            }
            CartFooter(model.cartTotals, currency)
        }
    }
}

@Composable
private fun CartLineRow(line: CartLineView, currency: String, onDec: () -> Unit, onInc: () -> Unit) {
    val c = sufrixColors()
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(Radii.sm)).padding(Space.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(line.name, color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, maxLines = 1)
            Text(Money.format(line.lineTotalMinor, currency), color = c.accent, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 13.sp)
        }
        // The minus button removes the line at qty 1 (the remove affordance).
        QtyStepper(line.qty, onDec, onInc)
    }
}

@Composable
private fun QtyStepper(qty: Long, onDec: () -> Unit, onInc: () -> Unit) {
    val c = sufrixColors()
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
        StepButton(if (qty <= 1) "✕" else "−", danger = qty <= 1, onClick = onDec)
        Text("$qty", color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 15.sp, modifier = Modifier.widthIn(min = 18.dp))
        StepButton("+", danger = false, onClick = onInc)
    }
}

@Composable
private fun StepButton(glyph: String, danger: Boolean, onClick: () -> Unit) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    Box(
        Modifier.size(30.dp).pressScale(interaction, 0.9f).clip(CircleShape).background(c.surfaceAlt)
            .border(1.dp, c.border, CircleShape)
            .clickable(interactionSource = interaction, indication = null) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick()
            },
        contentAlignment = Alignment.Center,
    ) {
        Text(glyph, color = if (danger) c.danger else c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 15.sp)
    }
}

@Composable
private fun CartFooter(totals: CartTotals, currency: String) {
    val c = sufrixColors()
    Column(
        Modifier.fillMaxWidth().background(c.surface).padding(Space.lg),
        verticalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        TotalRow(t("order.subtotal"), Money.format(totals.subtotalMinor, currency))
        TotalRow(t("order.tax"), Money.format(totals.taxMinor, currency))
        TotalRow(t("order.total"), Money.format(totals.totalMinor, currency), emphasized = true)
        SufrixButton(t("order.checkout"), { /* Tender flow lands next phase. */ })
    }
}

@Composable
private fun TotalRow(label: String, value: String, emphasized: Boolean = false) {
    val c = sufrixColors()
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(
            label, color = if (emphasized) c.textPrimary else c.textSecondary, fontFamily = SufrixFont,
            fontWeight = if (emphasized) FontWeight.Bold else FontWeight.Medium, fontSize = if (emphasized) 16.sp else 14.sp,
        )
        Box(Modifier.weight(1f))
        Text(
            value, color = if (emphasized) c.textPrimary else c.textSecondary, fontFamily = SufrixFont,
            fontWeight = if (emphasized) FontWeight.Black else FontWeight.SemiBold, fontSize = if (emphasized) 18.sp else 14.sp,
        )
    }
}

// ── Phone bottom cart bar ───────────────────────────────────────────────────────
@Composable
private fun CartBar(model: AppModel, currency: String, onOpen: () -> Unit) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    if (model.cartTotals.itemCount > 0) {
        Row(
            Modifier.fillMaxWidth().padding(Space.md).pressScale(interaction, 0.985f)
                .clip(RoundedCornerShape(Radii.md)).background(c.accent)
                .clickable(interactionSource = interaction, indication = null) {
                    haptic.performHapticFeedback(HapticFeedbackType.LongPress); onOpen()
                }
                .padding(horizontal = Space.lg).height(56.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.md),
        ) {
            Text("${model.cartTotals.itemCount} ${t("order.items")}", color = c.textOnAccent.copy(alpha = 0.9f), fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
            Box(Modifier.weight(1f))
            Text(t("order.view_cart"), color = c.textOnAccent, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 14.sp)
            Text(Money.format(model.cartTotals.totalMinor, currency), color = c.textOnAccent, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 15.sp)
            Text("⌃", color = c.textOnAccent, fontSize = 14.sp)
        }
    }
}

/** "EGP 500.00" — opening cash, formatted from minor units. */
fun ShiftView.currencyDisplay(code: String): String = Money.format(openingCashMinor, code)
