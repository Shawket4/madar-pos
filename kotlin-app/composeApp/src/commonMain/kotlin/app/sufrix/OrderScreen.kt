package app.sufrix

import androidx.compose.animation.Crossfade
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
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
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.LazyGridState
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
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.unit.IntOffset
import kotlin.math.roundToInt
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.sufrix.core.BundleView
import app.sufrix.core.CartLineView
import app.sufrix.core.CartTotals
import androidx.compose.foundation.focusable
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.isMetaPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import app.sufrix.ui.SufrixColors
import app.sufrix.ui.isRtlLayout
import app.sufrix.core.CatStyleView
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
import app.sufrix.ui.disclosureGlyph
import app.sufrix.ui.pressScale
import app.sufrix.ui.sufrixColors
import app.sufrix.ui.t

/** Synthetic category id for the Combos tab (bundles aren't a real category). */
private const val kCombosCategory = "__combos__"

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
        model.refreshPending()
        model.loadHistory()
    }
    // Connectivity heartbeat — refresh online + clock skew (+ drain) every 15s.
    LaunchedEffect("heartbeat") {
        while (true) {
            model.refreshConnectivity()
            kotlinx.coroutines.delay(15_000)
        }
    }

    val visible = model.menuItems
        .filter { it.isActive }
        .filter { selectedCategory == null || it.categoryId == selectedCategory }
        .filter { search.isBlank() || it.name.contains(search, ignoreCase = true) }

    // Hardware-keyboard shortcut (desktop): Ctrl/⌘+Enter checks out a non-empty
    // cart. The root holds focus; non-matching keys fall through (search still types).
    val checkoutFocus = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { checkoutFocus.requestFocus() } }

    BoxWithConstraints(
        Modifier.fillMaxSize().background(c.bg)
            .focusRequester(checkoutFocus).focusable()
            .onPreviewKeyEvent { e ->
                if (e.type == KeyEventType.KeyDown && (e.isCtrlPressed || e.isMetaPressed) && e.key == Key.Enter) {
                    if (model.cartLines.isNotEmpty()) { showTender = true; true } else false
                } else false
            }
    ) {
        val wide = maxWidth >= 760.dp
        Column(Modifier.fillMaxSize()) {
            OrderTopBar(model)
            if (!model.isOnline) {
                Box(Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.sm)) {
                    NoticeBanner(t("chrome.offline_banner"), ChipTone.WARNING)
                }
            }
            if (kotlin.math.abs(model.clockSkewMinutes) >= 5) {
                Box(Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.sm)) {
                    NoticeBanner("${t("chrome.clock_skew")} (${kotlin.math.abs(model.clockSkewMinutes)}m)", ChipTone.WARNING)
                }
            }
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
                            bundles = model.bundles,
                            onBundleTap = { b -> model.openBundleDetail(b) },
                            catStyle = { name -> model.core.categoryStyle(name, c.isDark) },
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
                        bundles = model.bundles,
                        onBundleTap = { b -> model.openBundleDetail(b) },
                        catStyle = { name -> model.core.categoryStyle(name, c.isDark) },
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

        // Cash In/Out — full-screen over the order screen.
        if (model.showCashMovements) {
            CashMovementsScreen(model)
        }

        // Past shifts — full-screen over the order screen.
        if (model.showShiftHistory) {
            ShiftHistoryScreen(model)
        }

        // Held orders (drafts) — full-screen over the order screen.
        if (model.showDrafts) {
            DraftsScreen(model)
        }

        // Delivery queue — full-screen over the order screen.
        if (model.showDelivery) {
            DeliveryScreen(model)
        }

        // Settings — full-screen over the order screen.
        if (model.showSettings) {
            SettingsScreen(model)
        }

        // Item customization sheet.
        model.detailItem?.let { ItemDetailSheet(model, it, onClose = { model.closeItemDetail() }) }

        // Bundle (combo) configuration sheet.
        model.detailBundle?.let { BundleDetailSheet(model, it) { model.closeBundleDetail() } }

        // "More" overflow drawer — scrim (tap to dismiss) + bottom-sheet panel.
        if (model.showMore) {
            Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.4f))
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { model.showMore = false })
            MoreDrawer(model, Modifier.align(Alignment.BottomCenter))
        }
    }
}

/** The "More" overflow drawer — secondary nav-hub actions that don't fit the
 *  bar (close shift, settings, sign out). Mirrors Flutter's ActionDrawer. */
@Composable
private fun MoreDrawer(model: AppModel, modifier: Modifier = Modifier) {
    val c = sufrixColors()
    Column(
        modifier.fillMaxWidth().clip(RoundedCornerShape(topStart = Radii.lg, topEnd = Radii.lg)).background(c.bg)
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {}
            .padding(bottom = Space.lg),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(Modifier.padding(top = Space.sm, bottom = Space.md).size(width = 36.dp, height = 4.dp)
            .clip(CircleShape).background(c.border))
        model.shift?.let { s ->
            Row(
                Modifier.fillMaxWidth().padding(horizontal = Space.lg).clip(RoundedCornerShape(Radii.md))
                    .background(c.surfaceAlt).padding(Space.md),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(Space.sm),
            ) {
                Box(Modifier.size(8.dp).clip(CircleShape).background(if (model.isOnline) c.success else c.warning))
                Column {
                    Text(s.tellerName, color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 14.sp)
                    Text(if (model.isOnline) t("chrome.online") else t("chrome.offline"),
                        color = c.textSecondary, fontFamily = SufrixFont, fontSize = 11.sp)
                }
            }
        }
        Column(Modifier.fillMaxWidth().padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            MoreRow("¤", t("cash.title"), c.textPrimary) {
                model.showMore = false; model.error = null; model.showCashMovements = true
            }
            MoreRow("↺", t("shifts.title"), c.textPrimary) {
                model.showMore = false; model.showShiftHistory = true
            }
            MoreRow("⤓", t("drafts.title"), c.textPrimary) {
                model.showMore = false; model.loadDrafts(); model.showDrafts = true
            }
            MoreRow("🛵", t("delivery.title"), c.textPrimary) {
                model.showMore = false; model.error = null; model.showDelivery = true
            }
            MoreRow("🔒", t("order.close_shift"), c.danger) {
                model.showMore = false; model.error = null; model.showCloseShift = true
            }
            MoreRow("⚙", t("settings.title"), c.textPrimary) {
                model.showMore = false; model.refreshPending(); model.showSettings = true
            }
            MoreRow("⎋", t("home.sign_out"), c.textPrimary) {
                // You can't sign out mid-shift — close the drawer first.
                if (model.hasOpenShift) model.flagError(model.t("settings.sign_out_shift_open"))
                else { model.showMore = false; model.signOut() }
            }
        }
    }
}

@Composable
private fun MoreRow(glyph: String, label: String, tone: Color, onClick: () -> Unit) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(c.surface)
            .border(1.dp, c.borderLight, RoundedCornerShape(Radii.sm))
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick()
            }
            .padding(horizontal = Space.md, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        Text(glyph, color = tone, fontSize = 15.sp)
        Text(label, color = tone, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
        Box(Modifier.weight(1f))
        Text(disclosureGlyph(), color = c.textMuted, fontSize = 15.sp)
    }
}

// ── Top action bar (the only nav hub) ───────────────────────────────────────────
@Composable
private fun OrderTopBar(model: AppModel) {
    val c = sufrixColors()
    val currency = model.session?.currencyCode ?: ""
    Column(Modifier.fillMaxWidth().background(c.surface)) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            SufrixMark(size = 32.dp)
            model.shift?.let { StatusChip(it.tellerName, ChipTone.INFO) }
            if (model.shift?.isOpen == true) ShiftStatsPill(model, currency)
            Box(Modifier.weight(1f))
            SyncChip(model)
            BarButton("≣") { model.showHistory = true }
            BarButton("⚙") { model.refreshPending(); model.showSettings = true }
            BarButton("⋯") { model.refreshPending(); model.showMore = true }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
    }
}

/** A squircle icon-glyph button for the action bar (matches the chip radius). */
@Composable
private fun BarButton(glyph: String, onClick: () -> Unit) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
    Box(
        Modifier.size(34.dp).clip(RoundedCornerShape(Radii.sm)).background(c.surfaceAlt)
            .border(1.dp, c.borderLight, RoundedCornerShape(Radii.sm))
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick()
            },
        contentAlignment = Alignment.Center,
    ) {
        Text(glyph, color = c.textMuted, fontSize = 16.sp)
    }
}

/** Live shift totals — "EGP X · N orders" (voided excluded, summed in core). */
@Composable
private fun ShiftStatsPill(model: AppModel, currency: String) {
    val c = sufrixColors()
    Row(
        Modifier.clip(CircleShape).background(c.surfaceAlt).border(1.dp, c.borderLight, CircleShape)
            .padding(horizontal = 10.dp, vertical = 5.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Text(Money.format(model.shiftSalesMinor, currency), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 11.sp)
        Text("·", color = c.textMuted, fontSize = 11.sp)
        Text("${model.shiftOrderCount} ${t("chrome.orders")}", color = c.textSecondary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 11.sp)
    }
}

/** Sync status chip — offline / stuck / syncing, hidden when idle + fully
 *  synced. Taps to the sync center. Mirrors Flutter's SyncStatusChip. */
@Composable
private fun SyncChip(model: AppModel) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
    val state = when {
        !model.isOnline -> "offline"
        model.syncFailed > 0 -> "stuck"
        model.pendingCount > 0 -> "syncing"
        else -> "idle"
    }
    if (state == "idle") return
    val label = when (state) {
        "offline" -> if (model.pendingCount > 0) "${t("chrome.offline")} · ${model.pendingCount} ${t("chrome.queued")}" else t("chrome.offline")
        "stuck" -> "${t("chrome.needs_attention")} (${model.syncFailed})"
        else -> "${t("chrome.syncing")} (${model.pendingCount})"
    }
    val glyph = if (state == "syncing") "⟳" else "⚠"
    val fg = if (state == "stuck") c.danger else c.warning
    val bg = if (state == "stuck") c.dangerBg else c.warningBg
    Row(
        Modifier.clip(CircleShape).background(bg)
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress); model.loadOutbox(); model.showSync = true
            }
            .padding(horizontal = 10.dp, vertical = 5.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        Text(glyph, color = fg, fontSize = 12.sp)
        Text(label, color = fg, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 11.sp)
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
    bundles: List<BundleView>,
    onBundleTap: (BundleView) -> Unit,
    catStyle: (String) -> CatStyleView,
) {
    val c = sufrixColors()
    val showCombos = bundles.isNotEmpty()
    val combos = selectedCategory == kCombosCategory
    if (wide) {
        // Tablet/desktop: a vertical category rail beside the search + grid.
        Row(Modifier.fillMaxSize()) {
            CategoryRail(categories, selectedCategory, onSelect, catStyle, showCombos)
            Box(Modifier.width(1.dp).fillMaxHeight().background(c.borderLight))
            Column(Modifier.weight(1f).fillMaxHeight()) {
                if (combos) {
                    Box(Modifier.weight(1f).fillMaxWidth()) { BundleGrid(bundles, currency, onBundleTap) }
                } else {
                    SearchField(search, onSearch, t("order.search"), Modifier.padding(Space.lg))
                    Box(Modifier.weight(1f).fillMaxWidth()) {
                        ItemGridOrEmpty(items, currency, search.isNotBlank(), categoryName, cartQty, selectedCategory, onAdd)
                    }
                }
            }
        }
    } else {
        // Phone: a horizontal underline-tab strip above the search + grid.
        Column(Modifier.fillMaxSize()) {
            CategoryTabs(categories, selectedCategory, onSelect, catStyle, showCombos)
            if (combos) {
                Box(Modifier.weight(1f).fillMaxWidth()) { BundleGrid(bundles, currency, onBundleTap) }
            } else {
                SearchField(search, onSearch, t("order.search"), Modifier.padding(Space.lg))
                Box(Modifier.weight(1f).fillMaxWidth()) {
                    ItemGridOrEmpty(items, currency, search.isNotBlank(), categoryName, cartQty, selectedCategory, onAdd)
                }
            }
        }
    }
}

/** The combo grid — bundle cards in the same adaptive layout as the item grid. */
@Composable
private fun BundleGrid(bundles: List<BundleView>, currency: String, onBundleTap: (BundleView) -> Unit) {
    LazyVerticalGrid(
        columns = MaxExtentCells(200.dp),
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(Space.lg),
        horizontalArrangement = Arrangement.spacedBy(14.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        items(bundles, key = { it.id }) { b ->
            BundleCard(b, currency) { onBundleTap(b) }
        }
    }
}

// ── Category navigation (phone underline tabs · wide vertical rail) ──────────────
@Composable
private fun CategoryTabs(
    cats: List<CategoryView>,
    selected: String?,
    onSelect: (String?) -> Unit,
    catStyle: (String) -> CatStyleView,
    showCombos: Boolean = false,
) {
    val c = sufrixColors()
    Column(Modifier.fillMaxWidth().background(c.surface)) {
        Row(
            Modifier.fillMaxWidth().height(46.dp).horizontalScroll(rememberScrollState())
                .padding(horizontal = Space.lg),
            horizontalArrangement = Arrangement.spacedBy(Space.lg),
        ) {
            CategoryTab(t("order.all"), "🍽️", selected == null) { onSelect(null) }
            if (showCombos) CategoryTab(t("order.combos"), "🎁", selected == kCombosCategory) { onSelect(kCombosCategory) }
            cats.filter { it.isActive }.forEach { cat ->
                CategoryTab(cat.name, catEmoji(catStyle(cat.name).icon), selected == cat.id) { onSelect(cat.id) }
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
    }
}

@Composable
private fun CategoryTab(label: String, emoji: String, active: Boolean, onClick: () -> Unit) {
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
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                Text(emoji, fontSize = 12.sp)
                Text(
                    label, color = if (active) c.accent else c.textMuted, fontFamily = SufrixFont,
                    fontWeight = if (active) FontWeight.Bold else FontWeight.Medium, fontSize = 13.sp,
                )
            }
        }
        Box(Modifier.fillMaxWidth().height(2.dp).background(if (active) c.accent else Color.Transparent))
    }
}

@Composable
private fun CategoryRail(
    cats: List<CategoryView>,
    selected: String?,
    onSelect: (String?) -> Unit,
    catStyle: (String) -> CatStyleView,
    showCombos: Boolean = false,
) {
    val c = sufrixColors()
    Column(
        Modifier.width(96.dp).fillMaxHeight().background(c.surface)
            .verticalScroll(rememberScrollState()).padding(vertical = Space.sm),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        RailTile(t("order.all"), null, "🍽️", selected == null) { onSelect(null) }
        if (showCombos) RailTile(t("order.combos"), null, "🎁", selected == kCombosCategory) { onSelect(kCombosCategory) }
        cats.filter { it.isActive }.forEach { cat ->
            val style = catStyle(cat.name)
            RailTile(cat.name, style, catEmoji(style.icon), selected == cat.id) { onSelect(cat.id) }
        }
    }
}

@Composable
private fun RailTile(label: String, style: CatStyleView?, emoji: String, active: Boolean, onClick: () -> Unit) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    val gradient = if (style != null) {
        Brush.linearGradient(listOf(hexColor(style.bgTop), hexColor(style.bgBottom)))
    } else {
        Brush.linearGradient(listOf(c.accentBg, c.accentBg))
    }
    Column(
        Modifier.fillMaxWidth().padding(horizontal = Space.sm).pressScale(interaction)
            .clip(RoundedCornerShape(Radii.sm)).background(if (active) c.accentBg else Color.Transparent)
            .clickable(interactionSource = interaction, indication = null) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick()
            }
            .padding(vertical = Space.sm, horizontal = Space.xs),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        Box(
            Modifier.size(38.dp).clip(RoundedCornerShape(11.dp)).background(gradient)
                .border(if (active) 2.dp else 0.dp, if (active) c.accent else Color.Transparent, RoundedCornerShape(11.dp)),
            contentAlignment = Alignment.Center,
        ) {
            Text(emoji, fontSize = 16.sp)
        }
        Text(
            label, color = if (active) c.accent else c.textSecondary, fontFamily = SufrixFont,
            fontWeight = if (active) FontWeight.Bold else FontWeight.Medium, fontSize = 10.sp,
            textAlign = TextAlign.Center, maxLines = 2, overflow = TextOverflow.Ellipsis,
        )
    }
}

/** Core `CatStyleView.icon` key → emoji (Compose's glyph for category icons). */
private fun catEmoji(key: String): String = when (key) {
    "coffee", "mocha", "tea", "cafe" -> "☕"
    "bakery" -> "🥐"
    "lunch" -> "🥪"
    "icecream" -> "🍨"
    "drink" -> "🥤"
    "water" -> "💧"
    "ice" -> "🧊"
    "matcha" -> "🍵"
    else -> "☕"
}

/** `#RRGGBB` → Compose Color (opaque). Pairs with the core's CatStyleView. */
private fun hexColor(hex: String): Color {
    val s = hex.removePrefix("#")
    return Color(("FF$s").toLong(16))
}

// ── Item grid ───────────────────────────────────────────────────────────────────
@Composable
private fun ItemGridOrEmpty(
    items: List<MenuItemView>,
    currency: String,
    searching: Boolean,
    categoryName: (String?) -> String,
    cartQty: (String) -> Long,
    gridKey: String?,
    onAdd: (MenuItemView) -> Unit,
) {
    val c = sufrixColors()
    // One scroll state per category — switching back restores the scroll position
    // (and the data never reloads; it's an in-memory filter).
    val scrollStates = remember { mutableMapOf<String?, LazyGridState>() }
    val gridState = scrollStates.getOrPut(gridKey) { LazyGridState() }
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
            state = gridState,
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(Space.lg),
            horizontalArrangement = Arrangement.spacedBy(14.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
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
                    // Bundles aren't re-editable in place (reconfigure by removing +
                    // re-adding); only plain lines reopen the customization sheet.
                    val onEdit: (() -> Unit)? = if (line.bundleId == null) { { model.editCartLine(line) } } else null
                    CartLineRow(
                        line, currency,
                        onDec = { model.setCartQty(line.key, line.qty - 1) },
                        onInc = { model.setCartQty(line.key, line.qty + 1) },
                        onEdit = onEdit,
                        onSwipeDelete = { model.swipeRemoveCartLine(line) },
                    )
                }
            }
            CartFooter(model.cartTotals, currency, onCheckout, onHold = { model.holdCart() })
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun CartLineRow(
    line: CartLineView,
    currency: String,
    onDec: () -> Unit,
    onInc: () -> Unit,
    onEdit: (() -> Unit)? = null,
    onSwipeDelete: (() -> Unit)? = null,
) {
    val c = sufrixColors()
    val isBundle = line.bundleId != null
    val hasModifiers = line.sizeLabel != null || line.addons.isNotEmpty() || line.optionals.isNotEmpty()
    // Swipe-to-delete: track a horizontal offset via `draggable` (NOT pointerInput).
    // Delete direction is leftward in LTR, rightward in RTL.
    val isRtl = isRtlLayout()
    val sign = if (isRtl) 1f else -1f
    val thresholdPx = with(LocalDensity.current) { 72.dp.toPx() }
    var offsetX by remember(line.key) { mutableStateOf(0f) }
    val dragState = rememberDraggableState { delta ->
        val next = offsetX + delta
        offsetX = if (sign < 0f) next.coerceIn(-260f, 0f) else next.coerceIn(0f, 260f)
    }

    Box(Modifier.fillMaxWidth()) {
        if (onSwipeDelete != null && offsetX != 0f) {
            Box(
                Modifier.matchParentSize().clip(RoundedCornerShape(Radii.sm)).background(c.danger)
                    .padding(horizontal = Space.xl),
                contentAlignment = if (isRtl) Alignment.CenterStart else Alignment.CenterEnd,
            ) {
                Text("🗑", fontSize = 18.sp)
            }
        }
        CartLineRowBody(
            line, currency, c, isBundle, hasModifiers, onDec, onInc, onEdit,
            modifier = Modifier
                .offset { IntOffset(offsetX.roundToInt(), 0) }
                .then(
                    if (onSwipeDelete != null) {
                        Modifier.draggable(
                            state = dragState,
                            orientation = Orientation.Horizontal,
                            onDragStopped = {
                                if (kotlin.math.abs(offsetX) > thresholdPx) onSwipeDelete() else offsetX = 0f
                            },
                        )
                    } else Modifier,
                ),
        )
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun CartLineRowBody(
    line: CartLineView,
    currency: String,
    c: app.sufrix.ui.SufrixColors,
    isBundle: Boolean,
    hasModifiers: Boolean,
    onDec: () -> Unit,
    onInc: () -> Unit,
    onEdit: (() -> Unit)?,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(Radii.sm)).padding(Space.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Column(
            Modifier.weight(1f).then(if (onEdit != null) Modifier.clickable { onEdit() } else Modifier),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(line.name, color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, maxLines = 1)
                if (isBundle) StatusChip(t("order.combos"), ChipTone.ACCENT)
            }
            if (isBundle) {
                BundleBreakdown(line)
            } else if (hasModifiers) {
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    line.sizeLabel?.let { Pill(it, c.textSecondary, c.surfaceAlt) }
                    line.addons.forEach { Pill(if (it.qty > 1) "${it.name} ×${it.qty}" else it.name, c.navy, c.navyBg) }
                    line.optionals.forEach { Pill(it.name, c.warning, c.warningBg) }
                }
            }
            Text(Money.format(line.lineTotalMinor, currency), color = c.accent, fontFamily = SufrixFont, fontWeight = FontWeight.Bold, fontSize = 13.sp)
        }
        // The minus button removes the line at qty 1 (the remove affordance).
        QtyStepper(line.qty, onDec, onInc)
    }
}

/** A bundle line lists its components (qty × name) with each component's chosen
 *  addons/optionals as sub-pills. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun BundleBreakdown(line: CartLineView) {
    val c = sufrixColors()
    Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
        line.bundleComponents.forEach { comp ->
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text("${comp.qty}× ${comp.name}", color = c.textSecondary, fontFamily = SufrixFont, fontWeight = FontWeight.Medium, fontSize = 11.sp)
                if (comp.addons.isNotEmpty() || comp.optionals.isNotEmpty()) {
                    FlowRow(
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                        verticalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        comp.addons.forEach { Pill(if (it.qty > 1) "${it.name} ×${it.qty}" else it.name, c.navy, c.navyBg) }
                        comp.optionals.forEach { Pill(it.name, c.warning, c.warningBg) }
                    }
                }
            }
        }
    }
}

/** A compact modifier chip in the cart row (size / addon / optional). */
@Composable
private fun Pill(text: String, fg: Color, bg: Color) {
    Text(
        text, color = fg, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 10.sp,
        modifier = Modifier.clip(RoundedCornerShape(4.dp)).background(bg).padding(horizontal = 7.dp, vertical = 2.dp),
    )
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
private fun CartFooter(totals: CartTotals, currency: String, onCheckout: () -> Unit, onHold: (() -> Unit)? = null) {
    val c = sufrixColors()
    val haptic = LocalHapticFeedback.current
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
        Row(
            Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            if (onHold != null) {
                val interaction = remember { MutableInteractionSource() }
                Box(
                    Modifier.size(50.dp).pressScale(interaction, 0.97f).clip(RoundedCornerShape(Radii.sm))
                        .background(c.accentBg)
                        .clickable(interactionSource = interaction, indication = null) {
                            haptic.performHapticFeedback(HapticFeedbackType.LongPress); onHold()
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    Text("⤓", color = c.accent, fontWeight = FontWeight.SemiBold, fontSize = 18.sp)
                }
            }
            Box(Modifier.weight(1f)) {
                SufrixButton(t("order.checkout"), { onCheckout() })
            }
        }
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
        if (emphasized) {
            // The grand total animates on change (mirrors the Flutter slide+fade).
            Crossfade(targetState = value, label = "total") { v ->
                Text(v, color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 18.sp)
            }
        } else {
            Text(value, color = c.textSecondary, fontFamily = SufrixFont, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
        }
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
