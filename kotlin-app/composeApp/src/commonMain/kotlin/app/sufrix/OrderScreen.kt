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
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.sufrix.core.CartLineView
import app.sufrix.core.CartTotals
import app.sufrix.core.CategoryView
import app.sufrix.core.MenuItemView
import app.sufrix.core.ShiftView
import app.sufrix.ui.ChipTone
import app.sufrix.ui.MaxExtentCells
import app.sufrix.ui.MenuItemCard
import app.sufrix.ui.Money
import app.sufrix.ui.NoticeBanner
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
    var showTender by remember { mutableStateOf(false) }
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
            model.error?.let {
                Box(Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.sm)) {
                    NoticeBanner(it, ChipTone.DANGER)
                }
            }
            if (wide) {
                Row(Modifier.fillMaxSize()) {
                    Box(Modifier.weight(1f).fillMaxHeight()) {
                        CatalogColumn(
                            model.categories, visible, currency, selectedCategory, { selectedCategory = it }, search, { search = it },
                            categoryName = { id -> model.categories.firstOrNull { it.id == id }?.name ?: "" },
                            cartQty = { itemId -> model.cartQtyForItem(itemId) },
                            wide = true,
                            onAdd = { item -> model.openItemDetail(item) },
                        )
                    }
                    Box(Modifier.width(1.dp).fillMaxHeight().background(c.border))
                    Box(Modifier.width(340.dp).fillMaxHeight()) {
                        CartPanel(model, currency, onCheckout = { showTender = true })
                    }
                }
            } else {
                Box(Modifier.weight(1f).fillMaxWidth()) {
                    CatalogColumn(
                        model.categories, visible, currency, selectedCategory, { selectedCategory = it }, search, { search = it },
                        categoryName = { id -> model.categories.firstOrNull { it.id == id }?.name ?: "" },
                        cartQty = { itemId -> model.cartQtyForItem(itemId) },
                        wide = false,
                        onAdd = { item -> model.openItemDetail(item) },
                    )
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
                CartPanel(model, currency, onClose = { showCart = false }, onCheckout = { showCart = false; showTender = true })
            }
        }

        // Tender overlay (checkout) — covers either layout.
        if (showTender) {
            TenderOverlay(model, currency) { showTender = false; model.dismissReceipt() }
        }

        // Close-shift flow — full-screen over the order screen.
        if (model.showCloseShift) {
            CloseShiftScreen(model)
        }

        // Sync center — full-screen over the order screen.
        if (model.showSync) {
            SyncScreen(model)
        }

        // Order history — full-screen over the order screen.
        if (model.showHistory) {
            OrderHistoryScreen(model)
        }

        // Settings — full-screen over the order screen.
        if (model.showSettings) {
            SettingsScreen(model)
        }

        // Item customization sheet.
        model.detailItem?.let { ItemDetailSheet(model, it) { model.closeItemDetail() } }
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
                "≣", color = c.textMuted, fontSize = 19.sp,
                modifier = Modifier.clickable(
                    interactionSource = remember { MutableInteractionSource() }, indication = null,
                ) {
                    haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                    model.showHistory = true
                },
            )
            Text(
                if (model.pendingCount > 0) "⟳ ${model.pendingCount}" else "✓",
                color = if (model.pendingCount > 0) c.warning else c.textMuted,
                fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 13.sp,
                modifier = Modifier.clickable(
                    interactionSource = remember { MutableInteractionSource() }, indication = null,
                ) {
                    haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                    model.loadOutbox()
                    model.showSync = true
                },
            )
            Text(
                "⚙", color = c.textMuted, fontSize = 18.sp,
                modifier = Modifier.clickable(
                    interactionSource = remember { MutableInteractionSource() }, indication = null,
                ) {
                    haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                    model.refreshPending()
                    model.showSettings = true
                },
            )
            Text(
                t("order.close_shift"),
                color = c.textSecondary, fontFamily = SufrixFont,
                fontWeight = FontWeight.SemiBold, fontSize = 13.sp,
                modifier = Modifier.clickable(
                    interactionSource = remember { MutableInteractionSource() }, indication = null,
                ) {
                    haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                    model.error = null
                    model.showCloseShift = true
                },
            )
            Text(
                t("home.sign_out"),
                color = c.textMuted, fontFamily = SufrixFont,
                fontWeight = FontWeight.SemiBold, fontSize = 13.sp,
                modifier = Modifier.pressScale(interaction).clickable(
                    interactionSource = interaction, indication = null,
                ) {
                    haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                    // You can't sign out mid-shift — close the drawer first.
                    if (model.hasOpenShift) {
                        model.flagError(model.t("settings.sign_out_shift_open"))
                    } else {
                        model.signOut()
                    }
                },
            )
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
    }
}

// ── Catalog column (category nav + search + grid) ───────────────────────────────
@Composable
private fun CatalogColumn(
    categories: List<CategoryView>,
    items: List<MenuItemView>,
    currency: String,
    selectedCategory: String?,
    onSelect: (String?) -> Unit,
    search: String,
    onSearch: (String) -> Unit,
    categoryName: (String?) -> String,
    cartQty: (String) -> Long,
    wide: Boolean,
    onAdd: (MenuItemView) -> Unit,
) {
    val c = sufrixColors()
    if (wide) {
        // Tablet/desktop: a vertical category rail beside the search + grid.
        Row(Modifier.fillMaxSize()) {
            CategoryRail(categories, selectedCategory, onSelect)
            Box(Modifier.width(1.dp).fillMaxHeight().background(c.borderLight))
            Column(Modifier.weight(1f).fillMaxHeight()) {
                SearchField(search, onSearch, t("order.search"), Modifier.padding(Space.lg))
                Box(Modifier.weight(1f).fillMaxWidth()) {
                    ItemGridOrEmpty(items, currency, search.isNotBlank(), categoryName, cartQty, onAdd)
                }
            }
        }
    } else {
        // Phone: a horizontal underline-tab strip above the search + grid.
        Column(Modifier.fillMaxSize()) {
            CategoryTabs(categories, selectedCategory, onSelect)
            SearchField(search, onSearch, t("order.search"), Modifier.padding(Space.lg))
            Box(Modifier.weight(1f).fillMaxWidth()) {
                ItemGridOrEmpty(items, currency, search.isNotBlank(), categoryName, cartQty, onAdd)
            }
        }
    }
}

// ── Category navigation (phone underline tabs · wide vertical rail) ──────────────
@Composable
private fun CategoryTabs(cats: List<CategoryView>, selected: String?, onSelect: (String?) -> Unit) {
    val c = sufrixColors()
    Column(Modifier.fillMaxWidth().background(c.surface)) {
        Row(
            Modifier.fillMaxWidth().height(46.dp).horizontalScroll(rememberScrollState())
                .padding(horizontal = Space.lg),
            horizontalArrangement = Arrangement.spacedBy(Space.lg),
        ) {
            CategoryTab(t("order.all"), selected == null) { onSelect(null) }
            cats.filter { it.isActive }.forEach { cat ->
                CategoryTab(cat.name, selected == cat.id) { onSelect(cat.id) }
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
    }
}

@Composable
private fun CategoryTab(label: String, active: Boolean, onClick: () -> Unit) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    Column(
        Modifier.fillMaxHeight().pressScale(interaction)
            .clickable(interactionSource = interaction, indication = null) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick()
            },
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(Modifier.weight(1f), contentAlignment = Alignment.Center) {
            Text(
                label, color = if (active) c.accent else c.textMuted, fontFamily = SufrixFont,
                fontWeight = if (active) FontWeight.Bold else FontWeight.Medium, fontSize = 13.sp,
            )
        }
        Box(Modifier.fillMaxWidth().height(2.dp).background(if (active) c.accent else Color.Transparent))
    }
}

@Composable
private fun CategoryRail(cats: List<CategoryView>, selected: String?, onSelect: (String?) -> Unit) {
    val c = sufrixColors()
    Column(
        Modifier.width(96.dp).fillMaxHeight().background(c.surface)
            .verticalScroll(rememberScrollState()).padding(vertical = Space.sm),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        RailTile(t("order.all"), selected == null) { onSelect(null) }
        cats.filter { it.isActive }.forEach { cat ->
            RailTile(cat.name, selected == cat.id) { onSelect(cat.id) }
        }
    }
}

@Composable
private fun RailTile(label: String, active: Boolean, onClick: () -> Unit) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    Text(
        label, color = if (active) c.accent else c.textSecondary, fontFamily = SufrixFont,
        fontWeight = if (active) FontWeight.Bold else FontWeight.Medium, fontSize = 10.sp,
        textAlign = TextAlign.Center, maxLines = 2, overflow = TextOverflow.Ellipsis,
        modifier = Modifier.fillMaxWidth().padding(horizontal = Space.sm).pressScale(interaction)
            .clip(RoundedCornerShape(Radii.sm)).background(if (active) c.accentBg else Color.Transparent)
            .clickable(interactionSource = interaction, indication = null) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick()
            }
            .padding(vertical = Space.md, horizontal = Space.xs),
    )
}

// ── Item grid ───────────────────────────────────────────────────────────────────
@Composable
private fun ItemGridOrEmpty(
    items: List<MenuItemView>,
    currency: String,
    searching: Boolean,
    categoryName: (String?) -> String,
    cartQty: (String) -> Long,
    onAdd: (MenuItemView) -> Unit,
) {
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
            columns = MaxExtentCells(200.dp),
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(Space.lg),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            items(items, key = { it.id }) { item ->
                MenuItemCard(item, categoryName(item.categoryId), currency, cartQty(item.id)) { onAdd(item) }
            }
        }
    }
}

// ── Search field ────────────────────────────────────────────────────────────────
@Composable
private fun SearchField(value: String, onChange: (String) -> Unit, placeholder: String, modifier: Modifier = Modifier) {
    val c = sufrixColors()
    Row(
        modifier.fillMaxWidth().height(40.dp).clip(RoundedCornerShape(Radii.sm)).background(c.surfaceAlt)
            .border(1.dp, c.border, RoundedCornerShape(Radii.sm))
            .padding(horizontal = Space.md),
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
private fun CartPanel(model: AppModel, currency: String, onClose: (() -> Unit)? = null, onCheckout: () -> Unit) {
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
                items(model.cartLines, key = { it.key }) { line ->
                    val onEdit: (() -> Unit)? = if (line.key != line.itemId) { { model.editCartLine(line) } } else null
                    CartLineRow(
                        line, currency,
                        onDec = { model.setCartQty(line.key, line.qty - 1) },
                        onInc = { model.setCartQty(line.key, line.qty + 1) },
                        onEdit = onEdit,
                    )
                }
            }
            CartFooter(model.cartTotals, currency, onCheckout)
        }
    }
}

@Composable
private fun CartLineRow(line: CartLineView, currency: String, onDec: () -> Unit, onInc: () -> Unit, onEdit: (() -> Unit)? = null) {
    val c = sufrixColors()
    val summary = configSummary(line)
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(Radii.sm)).padding(Space.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Column(
            Modifier.weight(1f).then(if (onEdit != null) Modifier.clickable { onEdit() } else Modifier),
            verticalArrangement = Arrangement.spacedBy(3.dp),
        ) {
            Text(line.name, color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, maxLines = 1)
            if (summary != null) Text(summary, color = c.textSecondary, fontFamily = SufrixFont, fontSize = 11.sp, maxLines = 2)
            Text(Money.format(line.lineTotalMinor, currency), color = c.accent, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 13.sp)
        }
        // The minus button removes the line at qty 1 (the remove affordance).
        QtyStepper(line.qty, onDec, onInc)
    }
}

/** "Large · Oat milk · Extra shot ×2 · Vanilla" — the line's config, compact. */
private fun configSummary(line: CartLineView): String? {
    val parts = buildList {
        line.sizeLabel?.let { add(it) }
        line.addons.forEach { add(if (it.qty > 1) "${it.name} ×${it.qty}" else it.name) }
        line.optionals.forEach { add(it.name) }
    }
    return if (parts.isEmpty()) null else parts.joinToString(" · ")
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
private fun CartFooter(totals: CartTotals, currency: String, onCheckout: () -> Unit) {
    val c = sufrixColors()
    Column(
        Modifier.fillMaxWidth().background(c.surface).padding(Space.lg),
        verticalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        TotalRow(t("order.subtotal"), Money.format(totals.subtotalMinor, currency))
        if (totals.discountMinor > 0) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(t("order.discount"), color = c.success, fontFamily = SufrixFont, fontWeight = FontWeight.Medium, fontSize = 14.sp)
                Box(Modifier.weight(1f))
                Text("−${Money.format(totals.discountMinor, currency)}", color = c.success, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
            }
        }
        TotalRow(t("order.tax"), Money.format(totals.taxMinor, currency))
        TotalRow(t("order.total"), Money.format(totals.totalMinor, currency), emphasized = true)
        SufrixButton(t("order.checkout"), { onCheckout() })
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
