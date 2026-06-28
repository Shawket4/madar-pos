package app.madar

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
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import kotlinx.coroutines.launch
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
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.core.BundleView
import app.madar.core.CartLineView
import app.madar.core.CartTotals
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
import app.madar.ui.MadarColors
import app.madar.ui.isRtlLayout
import app.madar.core.CatStyleView
import app.madar.core.CategoryView
import app.madar.core.MenuItemView
import app.madar.core.ShiftView
import app.madar.ui.ChipTone
import app.madar.ui.MaxExtentCells
import app.madar.ui.MenuItemCard
import app.madar.ui.Money
import app.madar.ui.NoticeBanner
import app.madar.ui.Radii
import app.madar.ui.Space
import app.madar.ui.FitOrScrollRow
import app.madar.ui.Elevation
import app.madar.ui.elevation
import app.madar.ui.StatusChip
import app.madar.ui.MadarIcon
import app.madar.ui.IconSize
import app.madar.ui.MadarButton
import app.madar.ui.LocalMadarFont
import app.madar.ui.MadarMark
import app.madar.ui.disclosureGlyph
import app.madar.ui.pressScale
import app.madar.ui.madarColors
import app.madar.ui.t
import app.madar.ui.Responsive
import app.madar.ui.Grid
import coil3.compose.AsyncImage
import androidx.compose.ui.layout.ContentScale

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
    val c = madarColors()
    var selectedCategory by remember { mutableStateOf<String?>(null) }
    var search by remember { mutableStateOf("") }
    var showCart by remember { mutableStateOf(false) }
    var showTender by remember { mutableStateOf(false) }
    var showFireDetails by remember { mutableStateOf(false) }
    val currency = model.session?.currencyCode ?: ""
    val scope = rememberCoroutineScope()
    val isWaiter = model.isWaiterDevice
    // Waiter mode: the cart's terminal action FIRES a ticket (or adds a round)
    // instead of opening the cashier tender flow — same component, different action.
    val checkoutLabel = when {
        !isWaiter -> t("order.checkout")
        model.activeTicketId != null -> t("waiter.add_round")
        else -> t("waiter.fire")
    }
    val checkoutIcon = if (isWaiter) "paperplane.fill" else "creditcard"
    // Waiter firing a NEW ticket → collect dine-in details (customer, table, covers,
    // notes) first; adding a round to an existing ticket fires straight away.
    fun doCheckout() {
        if (isWaiter) {
            if (model.activeTicketId == null) showFireDetails = true else scope.launch { model.fireOrAddRound() }
        } else showTender = true
    }

    // Reconcile the shift (catches a dashboard force-close) and load the catalog
    // + cart on appear. A waiter holds no shift/history — it fires tickets, so it
    // loads the open tickets + subscribes to live ticket/kitchen events instead.
    LaunchedEffect(Unit) {
        if (isWaiter) {
            model.loadCatalog()
            model.loadOpenTickets()
            // Live ticket/kitchen events arrive on the session-level SSE (login-time).
        } else {
            model.reconcileShift()
            model.loadCatalog()
            model.refreshPending()
            model.loadHistory()
        }
    }
    // A waiter board reacts to live ticket events via the shared subscription.
    if (isWaiter) LaunchedEffect(model.ticketTick) { model.loadOpenTickets() }
    // Connectivity heartbeat — refresh online + clock skew (+ drain) every 15s.
    LaunchedEffect("heartbeat") {
        while (true) {
            model.refreshConnectivity()
            kotlinx.coroutines.delay(15_000)
        }
    }
    // The local cart / tender sheets take back FIRST; when neither is open this is
    // disabled and the root BackHandler closes any open model overlay instead.
    BackHandlerCompat(enabled = showTender || showCart) {
        if (showTender) showTender = false else showCart = false
    }

    val visible = model.menuItems
        .filter { it.isActive }
        .filter { selectedCategory == null || it.categoryId == selectedCategory }
        .filter {
            search.isBlank() ||
                it.name.contains(search, ignoreCase = true) ||
                (it.description?.contains(search, ignoreCase = true) ?: false)
        }

    // Hardware-keyboard shortcut (desktop): Ctrl/⌘+Enter checks out a non-empty
    // cart. The root holds focus; non-matching keys fall through (search still types).
    val checkoutFocus = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { checkoutFocus.requestFocus() } }

    BoxWithConstraints(
        Modifier.fillMaxSize().background(c.bg)
            .focusRequester(checkoutFocus).focusable()
            .onPreviewKeyEvent { e ->
                if (e.type == KeyEventType.KeyDown && (e.isCtrlPressed || e.isMetaPressed) && e.key == Key.Enter) {
                    if (model.cartLines.isNotEmpty()) { doCheckout(); true } else false
                } else false
            }
    ) {
        val wide = maxWidth >= Responsive.wide
        Column(Modifier.fillMaxSize()) {
            OrderTopBar(model, wide)
            if (!model.isOnline) {
                Box(Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.sm)) {
                    NoticeBanner(t("chrome.offline_banner"), ChipTone.WARNING, icon = "wifi.slash")
                }
            }
            if (model.syncAuthPaused) {
                // Tappable — opens the re-auth sheet to resume sync (mirrors Swift's
                // Button-wrapped banner with a trailing call-to-action pill).
                Box(Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.sm)) {
                    AuthPausedBanner { model.error = null; model.showReauth = true }
                }
            }
            if (kotlin.math.abs(model.clockSkewMinutes) >= 5) {
                Box(Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.sm)) {
                    NoticeBanner("${t("chrome.clock_skew")} (${kotlin.math.abs(model.clockSkewMinutes)}m)", ChipTone.WARNING, icon = "clock.badge.exclamationmark")
                }
            }
            model.error?.let {
                Box(Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.sm)) {
                    NoticeBanner(it, ChipTone.DANGER, icon = "exclamationmark.circle")
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
                        CartPanel(model, currency, checkoutLabel = checkoutLabel, checkoutIcon = checkoutIcon,
                                  onCheckout = { doCheckout() })
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
                Modifier.widthIn(max = 600.dp).fillMaxWidth().fillMaxHeight(0.88f).align(Alignment.BottomCenter)
                    .clip(RoundedCornerShape(topStart = Radii.lg, topEnd = Radii.lg)).background(c.bg)
                    .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {},
            ) {
                CartPanel(model, currency, onClose = { showCart = false }, checkoutLabel = checkoutLabel,
                          checkoutIcon = checkoutIcon, onCheckout = { showCart = false; doCheckout() })
            }
        }

        // Tender overlay (checkout) — covers either layout.
        if (showTender) {
            TenderOverlay(model, currency) { showTender = false; model.dismissReceipt() }
        }

        // Waiter fire-details sheet — optional dine-in capture before firing.
        if (showFireDetails) {
            app.madar.ui.MadarSheet(onDismiss = { showFireDetails = false }, size = app.madar.ui.SheetSize.HUG, maxWidth = 480.dp) { dismiss ->
                FireDetailsSheet(model, scope) { dismiss() }
            }
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

        // Unified "Orders" surface — delivery + waiter open-tickets in two tabs,
        // full-screen over the order screen (replaces the separate delivery and
        // settle-tickets screens).
        if (model.showIncoming) {
            IncomingScreen(model)
        }

        // Waiter's open-tickets list — full-screen over the order screen.
        if (model.showTickets) {
            WaiterTicketsListScreen(model)
        }

        // Settings — full-screen over the order screen.
        if (model.showSettings) {
            SettingsScreen(model)
        }

        // Mid-shift Z-report preview + print (no need to close the shift).
        if (model.showReportPreview) {
            ShiftReportPreviewScreen(model) { model.showReportPreview = false }
        }

        // Receipt preview before (re)printing a past order.
        model.previewReceipt?.let { ReceiptPreviewScreen(model, it) { model.previewReceipt = null } }

        // Item customization sheet.
        model.detailItem?.let { ItemDetailSheet(model, it, onClose = { model.closeItemDetail() }) }

        // Bundle (combo) configuration sheet.
        model.detailBundle?.let { BundleDetailSheet(model, it) { model.closeBundleDetail() } }

        // Token expired mid-shift → re-auth the same teller to resume sync.
        // Self-gating on model.showReauth (defined in a sibling file).
        ReauthScreen(model)

        // "More" overflow drawer — scrim (tap to dismiss) + bottom-sheet panel.
        if (model.showMore) {
            Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.4f))
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { model.showMore = false })
            MoreDrawer(model, wide, Modifier.align(Alignment.BottomCenter))
        }
    }
}

/** The "More" overflow drawer — secondary nav-hub actions that don't fit the
 *  bar (close shift, settings, sign out). Mirrors Flutter's ActionDrawer. */
@Composable
private fun MoreDrawer(model: AppModel, wide: Boolean, modifier: Modifier = Modifier) {
    val c = madarColors()
    val scope = rememberCoroutineScope()
    val currency = model.session?.currencyCode ?: ""
    Column(
        // Fixed sheet height (~60% of the screen) with the row list scrolling inside,
        // so the drawer is a consistent size regardless of how many rows the role has.
        modifier.widthIn(max = 600.dp).fillMaxWidth().fillMaxHeight(0.6f)
            .clip(RoundedCornerShape(topStart = Radii.lg, topEnd = Radii.lg)).background(c.bg)
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
                    Text(s.tellerName, color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 14.sp)
                    Text(if (model.isOnline) t("chrome.online") else t("chrome.offline"),
                        color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 11.sp)
                    // Phone: carry the live shift stats here since the bar pill is hidden.
                    if (!wide && s.isOpen) {
                        Text("${Money.format(model.shiftSalesMinor, currency)} · ${model.shiftOrderCount} ${t("chrome.orders")}",
                            color = c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 11.sp)
                    }
                }
            }
        }
        Column(
            Modifier.fillMaxWidth().weight(1f).verticalScroll(rememberScrollState()).padding(Space.lg),
            verticalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            if (model.isWaiterDevice) {
                // Waiter: open-tickets list + the sync center. No shift/cash/till rows.
                MoreRow("fork.knife", t("waiter.tickets"), c.textPrimary) {
                    model.showMore = false; scope.launch { model.loadOpenTickets() }; model.showTickets = true
                }
                MoreRow("arrow.triangle.2.circlepath", t("sync.title"), c.textPrimary) {
                    model.showMore = false; model.loadOutbox(); model.showSync = true
                }
            } else {
                // Phone-only: the bar's History / Sync / Sync-data buttons live here instead.
                if (!wide) {
                    MoreRow("list.bullet.rectangle", t("history.title"), c.textPrimary) {
                        model.showMore = false; model.showHistory = true
                    }
                    MoreRow("arrow.triangle.2.circlepath", t("sync.title"), c.textPrimary) {
                        model.showMore = false; model.loadOutbox(); model.showSync = true
                    }
                    MoreRow("arrow.clockwise", t("chrome.sync_data"), c.textPrimary) {
                        model.showMore = false; scope.launch { model.refreshServerData() }
                    }
                }
                MoreRow("banknote", t("cash.title"), c.textPrimary) {
                    model.showMore = false; model.error = null; model.showCashMovements = true
                }
                MoreRow("clock.arrow.circlepath", t("shifts.title"), c.textPrimary) {
                    model.showMore = false; model.showShiftHistory = true
                }
                MoreRow("printer", t("shift.print_report"), c.textPrimary) {
                    model.showMore = false; model.openShiftReportPreview()
                }
                MoreRow("tray.full", t("drafts.title"), c.textPrimary) {
                    model.showMore = false; model.loadDrafts(); model.showDrafts = true
                }
                // ONE entry for both delivery + waiter open-tickets (two tabs).
                MoreRow("bicycle", t("incoming.title"), c.textPrimary) {
                    model.showMore = false; model.error = null
                    scope.launch { model.loadDeliveryOrders(); model.loadOpenTickets() }
                    model.showIncoming = true
                }
                MoreRow("lock", t("order.close_shift"), c.danger) {
                    model.showMore = false; model.error = null; model.showCloseShift = true
                }
            }
            MoreRow("gearshape", t("settings.title"), c.textPrimary) {
                model.showMore = false; model.refreshPending(); model.showSettings = true
            }
            MoreRow("rectangle.portrait.and.arrow.right", t("home.sign_out"), c.textPrimary) {
                // You can't sign out mid-shift — close the drawer first.
                if (model.hasOpenShift) model.flagError(model.t("settings.sign_out_shift_open"))
                else { model.showMore = false; model.signOut() }
            }
        }
    }
}

@Composable
private fun MoreRow(glyph: String, label: String, tone: Color, onClick: () -> Unit) {
    val c = madarColors()
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
        MadarIcon(glyph, tint = tone, size = IconSize.lg)
        Text(label, color = tone, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
        Box(Modifier.weight(1f))
        MadarIcon("chevron.right", tint = c.textMuted, size = IconSize.md)
    }
}

/** The auth-paused notice as a tappable banner — danger tone with a trailing
 *  call-to-action pill (text + chevron). Mirrors Swift's Button-wrapped
 *  NoticeBanner(tone: .danger, actionLabel:). Built inline because the shared
 *  NoticeBanner has no actionLabel param. */
@Composable
private fun AuthPausedBanner(onClick: () -> Unit) {
    val c = madarColors()
    val haptic = LocalHapticFeedback.current
    val fg = c.danger
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.sm)).background(c.dangerBg)
            .border(1.dp, fg.copy(alpha = 0.25f), RoundedCornerShape(Radii.sm))
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick()
            }
            .padding(horizontal = 14.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        MadarIcon("lock", tint = fg, size = IconSize.md)
        Text(t("chrome.auth_paused"), color = fg, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Medium, fontSize = 13.sp)
        Box(Modifier.weight(1f))
        Row(
            Modifier.clip(CircleShape).background(fg.copy(alpha = 0.12f)).padding(horizontal = 10.dp, vertical = 5.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(t("chrome.auth_paused_action"), color = fg, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 12.sp)
            MadarIcon("chevron.right", tint = fg, size = IconSize.xs)
        }
    }
}

// ── Top action bar (the only nav hub) ───────────────────────────────────────────
@Composable
private fun OrderTopBar(model: AppModel, wide: Boolean) {
    val c = madarColors()
    val currency = model.session?.currencyCode ?: ""
    val scope = rememberCoroutineScope()
    val isWaiter = model.isWaiterDevice
    Column(Modifier.fillMaxWidth().background(c.surface)) {
        // Fills + right-pins when it fits; scrolls horizontally only when the
        // content can't fit the viewport (very narrow phones / split layouts).
        FitOrScrollRow(
            Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            MadarMark(size = 32.dp)
            // Phone: the status chips + secondary action buttons don't fit a ~360dp
            // bar, so they collapse into the More drawer (which carries the teller +
            // live stats in its header). Only the logo, sync status, and More stay.
            // A waiter holds no shift, so it shows neither the teller chip nor stats.
            if (wide && !isWaiter) {
                model.shift?.let { StatusChip(it.tellerName, ChipTone.INFO, icon = "person.fill") }
                if (model.shift?.isOpen == true) ShiftStatsPill(model, currency)
            }
            Box(Modifier.weight(1f))
            SyncChip(model)
            if (isWaiter) {
                // Waiter's nav: catalog sync (the SAME button as the teller), the
                // open-tickets list, and settings; the rest is in More.
                SyncDataButton(model)
                BarButton("fork.knife") { scope.launch { model.loadOpenTickets() }; model.showTickets = true }
                if (wide) BarButton("gearshape") { model.refreshPending(); model.showSettings = true }
            } else if (wide) {
                SyncDataButton(model)
                BarButton("list.bullet.rectangle") { model.showHistory = true }
                BarButton("gearshape") { model.refreshPending(); model.showSettings = true }
            }
            BarButton("ellipsis") { model.refreshPending(); model.showMore = true }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
    }
}

/** Manual "sync server data" — re-pulls the catalog (menu, add-ons, bundles,
 *  payment methods, discounts). Spins + disables while running. Mirrors Flutter's
 *  top-bar SyncBtn. */
@Composable
private fun SyncDataButton(model: AppModel) {
    val c = madarColors()
    val haptic = LocalHapticFeedback.current
    val scope = rememberCoroutineScope()
    Box(
        Modifier.size(34.dp).clip(RoundedCornerShape(Radii.sm)).background(c.surfaceAlt)
            .border(1.dp, c.borderLight, RoundedCornerShape(Radii.sm))
            .clickable(
                enabled = !model.isSyncingData,
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
            ) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                scope.launch { model.refreshServerData() }
            },
        contentAlignment = Alignment.Center,
    ) {
        if (model.isSyncingData) {
            CircularProgressIndicator(color = c.accent, strokeWidth = 2.dp, modifier = Modifier.size(16.dp))
        } else {
            MadarIcon("arrow.triangle.2.circlepath", tint = c.textMuted, size = IconSize.md)
        }
    }
}

/** A squircle icon-glyph button for the action bar (matches the chip radius). */
@Composable
private fun BarButton(glyph: String, onClick: () -> Unit) {
    val c = madarColors()
    val haptic = LocalHapticFeedback.current
    Box(
        Modifier.size(34.dp).clip(RoundedCornerShape(Radii.sm)).background(c.surfaceAlt)
            .border(1.dp, c.borderLight, RoundedCornerShape(Radii.sm))
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick()
            },
        contentAlignment = Alignment.Center,
    ) {
        MadarIcon(glyph, tint = c.textMuted, size = IconSize.lg)
    }
}

/** Live shift totals — "EGP X · N orders" (voided excluded, summed in core). */
@Composable
private fun ShiftStatsPill(model: AppModel, currency: String) {
    val c = madarColors()
    Row(
        Modifier.clip(CircleShape).background(c.surfaceAlt).border(1.dp, c.borderLight, CircleShape)
            .padding(horizontal = 10.dp, vertical = 5.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Text(Money.format(model.shiftSalesMinor, currency), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 11.sp)
        Text("·", color = c.textMuted, fontSize = 11.sp)
        Text("${model.shiftOrderCount} ${t("chrome.orders")}", color = c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 11.sp)
    }
}

/** Sync status chip — offline / stuck / syncing, hidden when idle + fully
 *  synced. Taps to the sync center. Mirrors Flutter's SyncStatusChip. */
@Composable
private fun SyncChip(model: AppModel) {
    val c = madarColors()
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
    val glyph = if (state == "syncing") "arrow.triangle.2.circlepath" else "exclamationmark.triangle"
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
        MadarIcon(glyph, tint = fg, size = IconSize.xs)
        Text(label, color = fg, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 11.sp)
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
    val c = madarColors()
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
        columns = MaxExtentCells(Grid.cellMax),
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(Space.lg),
        horizontalArrangement = Arrangement.spacedBy(Grid.gutter),
        verticalArrangement = Arrangement.spacedBy(Grid.gutter),
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
    val c = madarColors()
    Column(Modifier.fillMaxWidth().background(c.surface)) {
        Row(
            Modifier.fillMaxWidth().height(46.dp).horizontalScroll(rememberScrollState())
                .padding(horizontal = Space.md),
            horizontalArrangement = Arrangement.spacedBy(Space.lg),
        ) {
            CategoryTab(t("order.all"), "square.grid.2x2.fill", selected == null) { onSelect(null) }
            if (showCombos) CategoryTab(t("order.combos"), "square.stack.3d.up.fill", selected == kCombosCategory) { onSelect(kCombosCategory) }
            cats.filter { it.isActive }.forEach { cat ->
                CategoryTab(cat.name, categoryIconName(catStyle(cat.name).icon), selected == cat.id) { onSelect(cat.id) }
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
    }
}

@Composable
private fun CategoryTab(label: String, icon: String?, active: Boolean, onClick: () -> Unit) {
    val c = madarColors()
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
                if (icon != null) MadarIcon(icon, tint = if (active) c.accent else c.textMuted, size = IconSize.sm)
                Text(
                    label, color = if (active) c.accent else c.textMuted, fontFamily = LocalMadarFont.current,
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
    val c = madarColors()
    Column(
        Modifier.width(96.dp).fillMaxHeight().background(c.surface)
            .verticalScroll(rememberScrollState()).padding(vertical = Space.sm),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        RailTile(t("order.all"), null, null, "square.grid.2x2.fill", selected == null) { onSelect(null) }
        if (showCombos) RailTile(t("order.combos"), null, null, "square.stack.3d.up.fill", selected == kCombosCategory) { onSelect(kCombosCategory) }
        cats.filter { it.isActive }.forEach { cat ->
            val style = catStyle(cat.name)
            RailTile(cat.name, style, cat.imageUrl, null, selected == cat.id) { onSelect(cat.id) }
        }
    }
}

@Composable
private fun RailTile(label: String, style: CatStyleView?, imageUrl: String?, fixedIcon: String?, active: Boolean, onClick: () -> Unit) {
    val c = madarColors()
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
            // Base — fixed icon (All/Combos) → family icon → monogram. ALWAYS drawn,
            // so a missing/failed category image still shows something. The image
            // overlays it when loaded.
            val iconColor = if (style != null) hexColor(style.iconColor) else c.accent
            val famIcon = style?.icon?.let { categoryIconName(it) }
            when {
                fixedIcon != null -> MadarIcon(fixedIcon, tint = iconColor, size = IconSize.md)
                famIcon != null -> MadarIcon(famIcon, tint = iconColor, size = IconSize.md)
                else -> Text(categoryMonogram(label), color = iconColor, fontFamily = LocalMadarFont.current,
                    fontWeight = FontWeight.Bold, fontSize = 15.sp)
            }
            if (fixedIcon == null && imageUrl != null) {
                AsyncImage(model = imageUrl, contentDescription = null,
                    modifier = Modifier.size(38.dp).clip(RoundedCornerShape(11.dp)), contentScale = ContentScale.Crop)
            }
        }
        Text(
            label, color = if (active) c.accent else c.textSecondary, fontFamily = LocalMadarFont.current,
            fontWeight = if (active) FontWeight.Bold else FontWeight.Medium, fontSize = 10.sp,
            textAlign = TextAlign.Center, maxLines = 2, overflow = TextOverflow.Ellipsis,
        )
    }
}

/** Core CatStyleView.icon key → shared Lucide icon name; null for the 'cafe'
 *  default (custom category) → caller shows the monogram instead. */
private fun categoryIconName(key: String): String? = when (key) {
    "coffee", "mocha", "tea", "bakery", "lunch", "icecream", "drink", "water", "ice", "matcha" -> "cat.$key"
    else -> null
}

/** Up to two initials for a category name (matches the item monogram rule). */
private fun categoryMonogram(name: String): String {
    val w = name.split(Regex("\\s+")).filter { it.isNotEmpty() }
    return when {
        w.size >= 2 -> (w[0].take(1) + w[1].take(1)).uppercase()
        w.isNotEmpty() -> w[0].take(2).uppercase()
        else -> "•"
    }
}

/** `#RRGGBB` → Compose Color (opaque). Pairs with the core's CatStyleView. */
internal fun hexColor(hex: String): Color {
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
    val c = madarColors()
    // One scroll state per category — switching back restores the scroll position
    // (and the data never reloads; it's an in-memory filter).
    val scrollStates = remember { mutableMapOf<String?, LazyGridState>() }
    val gridState = scrollStates.getOrPut(gridKey) { LazyGridState() }
    if (items.isEmpty()) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.md)) {
                MadarIcon(if (searching) "magnifyingglass" else "tray", tint = c.textMuted, size = 36.dp)
                Text(
                    t(if (searching) "order.empty_search" else "order.empty"),
                    color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 14.sp,
                )
            }
        }
    } else {
        LazyVerticalGrid(
            columns = MaxExtentCells(Grid.cellMax),
            state = gridState,
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(Space.lg),
            horizontalArrangement = Arrangement.spacedBy(Grid.gutter),
            verticalArrangement = Arrangement.spacedBy(Grid.gutter),
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
    val c = madarColors()
    Row(
        modifier.fillMaxWidth().height(40.dp).elevation(Elevation.CARD, RoundedCornerShape(Radii.sm)).clip(RoundedCornerShape(Radii.sm)).background(c.surfaceAlt)
            .border(1.dp, c.borderLight, RoundedCornerShape(Radii.sm))
            .padding(horizontal = Space.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        MadarIcon("magnifyingglass", tint = c.textMuted, size = IconSize.md)
        Box(Modifier.weight(1f)) {
            if (value.isEmpty()) {
                Text(placeholder, color = c.textMuted, fontFamily = LocalMadarFont.current, fontSize = 15.sp)
            }
            BasicTextField(
                value = value, onValueChange = onChange, singleLine = true,
                modifier = Modifier.fillMaxWidth(),
                textStyle = TextStyle(color = c.textPrimary, fontFamily = LocalMadarFont.current, fontSize = 15.sp),
                cursorBrush = SolidColor(c.accent),
            )
        }
        if (value.isNotEmpty()) {
            MadarIcon("xmark", tint = c.textMuted, size = IconSize.sm, modifier = Modifier.clickable { onChange("") })
        }
    }
}

// ── Cart panel (wide column + phone drawer) ─────────────────────────────────────
@Composable
private fun CartPanel(model: AppModel, currency: String, onClose: (() -> Unit)? = null,
                      checkoutLabel: String? = null, checkoutIcon: String? = null, onCheckout: () -> Unit) {
    val c = madarColors()
    LaunchedEffect(Unit) { model.loadDrafts() }
    Column(Modifier.fillMaxSize().background(c.bg)) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = 14.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            Text(t("order.cart"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 17.sp)
            if (model.cartTotals.itemCount > 0) StatusChip("${model.cartTotals.itemCount}", ChipTone.ACCENT)
            Box(Modifier.weight(1f))
            if (model.cartLines.isNotEmpty()) {
                Text(
                    t("order.clear"), color = c.danger, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp,
                    modifier = Modifier.clickable { model.clearCart() },
                )
            }
            if (onClose != null) {
                MadarIcon("xmark", tint = c.textMuted, size = IconSize.md, modifier = Modifier.padding(start = Space.sm).clickable { onClose() })
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))

        // Held-order tabs — flip between parked carts (switching parks the current
        // one first, so nothing is lost). The bottom Hold button stays.
        if (model.drafts.isNotEmpty()) HeldOrdersTabs(model)

        if (model.cartLines.isEmpty()) {
            Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                Text(t("order.cart_empty"), color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 14.sp)
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
            CartFooter(model.cartTotals, currency, onCheckout, onHold = { model.holdCart() },
                       checkoutLabel = checkoutLabel, checkoutIcon = checkoutIcon)
        }
    }
}

/** Held-order tabs above the cart — Current + a tab per parked order. Tapping a
 *  held tab parks the current cart first, then loads that one (lossless). */
@Composable
private fun HeldOrdersTabs(model: AppModel) {
    val c = madarColors()
    Column(Modifier.fillMaxWidth().background(c.surface)) {
        Row(
            Modifier.fillMaxWidth().horizontalScroll(rememberScrollState())
                .padding(horizontal = Space.lg, vertical = Space.sm),
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            HeldTab(t("drafts.current"), model.cartTotals.itemCount.toInt(), active = true, onTap = null, onClose = null)
            model.drafts.forEach { d ->
                HeldTab(d.name, d.itemCount.toInt(), active = false,
                    onTap = { model.switchToHeldOrder(d.id) },
                    onClose = { model.discardDraft(d.id) })
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.borderLight))
    }
}

@Composable
private fun HeldTab(label: String, count: Int, active: Boolean, onTap: (() -> Unit)?, onClose: (() -> Unit)?) {
    val c = madarColors()
    val fg = if (active) c.textOnAccent else c.textSecondary
    Row(
        Modifier.clip(CircleShape).background(if (active) c.accent else c.surfaceAlt)
            .border(1.dp, if (active) Color.Transparent else c.border, CircleShape)
            .then(if (onTap != null) Modifier.clickable { onTap() } else Modifier)
            .padding(horizontal = 12.dp, vertical = 7.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(label, color = fg, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 12.sp, maxLines = 1)
        if (count > 0) {
            Box(
                Modifier.clip(CircleShape)
                    .background(if (active) c.textOnAccent.copy(alpha = 0.25f) else c.surface)
                    .padding(horizontal = 5.dp, vertical = 1.dp),
            ) {
                Text("$count", color = fg, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 10.sp)
            }
        }
        if (onClose != null) {
            MadarIcon("xmark", tint = fg, size = 10.dp, modifier = Modifier.clickable { onClose() })
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
    val c = madarColors()
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
                MadarIcon("trash", tint = androidx.compose.ui.graphics.Color.White, size = 18.dp)
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
    c: app.madar.ui.MadarColors,
    isBundle: Boolean,
    hasModifiers: Boolean,
    onDec: () -> Unit,
    onInc: () -> Unit,
    onEdit: (() -> Unit)?,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier.fillMaxWidth().elevation(Elevation.CARD, RoundedCornerShape(Radii.sm)).clip(RoundedCornerShape(Radii.sm)).background(c.surface)
            .border(1.dp, c.borderLight, RoundedCornerShape(Radii.sm)).padding(Space.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        Column(
            Modifier.weight(1f).then(if (onEdit != null) Modifier.clickable { onEdit() } else Modifier),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(line.name, color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp, maxLines = 1)
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
            line.notes?.takeIf { it.isNotBlank() }?.let {
                Text("“$it”", color = c.textMuted, fontFamily = LocalMadarFont.current, fontStyle = FontStyle.Italic, fontSize = 11.sp, maxLines = 2)
            }
            Text(Money.format(line.lineTotalMinor, currency), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 13.sp)
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
    val c = madarColors()
    Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
        line.bundleComponents.forEach { comp ->
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text("${comp.qty}× ${comp.name}", color = c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Medium, fontSize = 11.sp)
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
        text, color = fg, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 10.sp,
        modifier = Modifier.clip(RoundedCornerShape(4.dp)).background(bg).padding(horizontal = 7.dp, vertical = 2.dp),
    )
}

@Composable
private fun QtyStepper(qty: Long, onDec: () -> Unit, onInc: () -> Unit) {
    val c = madarColors()
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
        StepButton(if (qty <= 1) "trash" else "minus", danger = qty <= 1, onClick = onDec)
        Text("$qty", color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 15.sp, modifier = Modifier.widthIn(min = 18.dp))
        StepButton("plus", danger = false, onClick = onInc)
    }
}

@Composable
private fun StepButton(glyph: String, danger: Boolean, onClick: () -> Unit) {
    val c = madarColors()
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
        MadarIcon(glyph, tint = if (danger) c.danger else c.textPrimary, size = IconSize.sm)
    }
}

@Composable
private fun CartFooter(totals: CartTotals, currency: String, onCheckout: () -> Unit, onHold: (() -> Unit)? = null,
                       checkoutLabel: String? = null, checkoutIcon: String? = null) {
    val c = madarColors()
    val haptic = LocalHapticFeedback.current
    Column(
        Modifier.fillMaxWidth().background(c.surface).padding(Space.lg),
        verticalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.border))
        TotalRow(t("order.subtotal"), Money.format(totals.subtotalMinor, currency))
        if (totals.discountMinor > 0) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(t("order.discount"), color = c.success, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Medium, fontSize = 13.sp)
                Box(Modifier.weight(1f))
                Text("−${Money.format(totals.discountMinor, currency)}", color = c.success, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
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
                    MadarIcon("tray.and.arrow.down", tint = c.accent, size = 18.dp)
                }
            }
            Box(Modifier.weight(1f)) {
                MadarButton(checkoutLabel ?: t("order.checkout"), { onCheckout() }, icon = checkoutIcon ?: "creditcard")
            }
        }
    }
}

@Composable
private fun TotalRow(label: String, value: String, emphasized: Boolean = false) {
    val c = madarColors()
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(
            label, color = if (emphasized) c.textPrimary else c.textSecondary, fontFamily = LocalMadarFont.current,
            fontWeight = if (emphasized) FontWeight.Bold else FontWeight.Medium, fontSize = if (emphasized) 15.sp else 13.sp,
        )
        Box(Modifier.weight(1f))
        if (emphasized) {
            // The grand total animates on change (mirrors the Flutter slide+fade).
            // Flutter's grand total is accent-tinted; the lighter sub-rows stay muted.
            Crossfade(targetState = value, label = "total") { v ->
                Text(v, color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 18.sp)
            }
        } else {
            Text(value, color = c.textSecondary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
        }
    }
}

// ── Phone bottom cart bar ───────────────────────────────────────────────────────
@Composable
private fun CartBar(model: AppModel, currency: String, onOpen: () -> Unit) {
    val c = madarColors()
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
            // The item-count is secondary: let it flex + ellipsize so the CTA and
            // total (the essential bits) always stay on-screen on a narrow phone or
            // with a long localized label.
            Text("${model.cartTotals.itemCount} ${t("order.items")}", color = c.textOnAccent.copy(alpha = 0.9f), fontFamily = LocalMadarFont.current, fontWeight = FontWeight.SemiBold, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
            Text(t("order.view_cart"), color = c.textOnAccent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 14.sp, maxLines = 1)
            Text(Money.format(model.cartTotals.totalMinor, currency), color = c.textOnAccent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 15.sp, maxLines = 1)
            MadarIcon("chevron.up", tint = c.textOnAccent, size = 14.dp)
        }
    }
}

/** "EGP 500.00" — opening cash, formatted from minor units. */
fun ShiftView.currencyDisplay(code: String): String = Money.format(openingCashMinor, code)

/** Dine-in capture before a waiter fires a NEW ticket: customer, table, covers,
 *  kitchen notes — all optional, all now passed to the core (was firing blank). */
@Composable
private fun androidx.compose.foundation.layout.ColumnScope.FireDetailsSheet(
    model: AppModel,
    scope: kotlinx.coroutines.CoroutineScope,
    onDone: () -> Unit,
) {
    val c = madarColors()
    var customer by remember { mutableStateOf("") }
    var table by remember { mutableStateOf("") }
    var notes by remember { mutableStateOf("") }
    var covers by remember { mutableStateOf(0) }
    Column(Modifier.fillMaxWidth().padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.md)) {
        Text(t("waiter.fire"), style = app.madar.ui.Type.h2(), color = c.textPrimary)
        app.madar.ui.MadarTextField(customer, { customer = it }, t("waiter.customer_optional"), icon = "person")
        app.madar.ui.MadarTextField(table, { table = it }, t("waiter.table"), icon = "tablecells")
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md)) {
            Text(t("waiter.covers"), style = app.madar.ui.Type.title(), color = c.textSecondary, modifier = Modifier.weight(1f))
            FireStepBox("minus") { if (covers > 0) covers-- }
            Text("$covers", style = app.madar.ui.Type.h3(), color = c.textPrimary, modifier = Modifier.widthIn(min = 28.dp), textAlign = TextAlign.Center)
            FireStepBox("plus") { covers++ }
        }
        app.madar.ui.MadarTextField(notes, { notes = it }, t("order.notes_hint"), icon = "text.bubble")
        MadarButton(t("waiter.fire"), {
            scope.launch {
                model.fireOrAddRound(
                    customerName = customer.ifBlank { null },
                    tableId = table.ifBlank { null },
                    notes = notes.ifBlank { null },
                    guestCount = if (covers > 0) covers else null,
                )
                onDone()
            }
        }, loading = model.isBusy, icon = "paperplane.fill")
    }
}

@Composable
private fun FireStepBox(icon: String, onClick: () -> Unit) {
    val c = madarColors()
    Box(
        Modifier.size(36.dp).clip(RoundedCornerShape(Radii.sm)).background(c.surfaceAlt)
            .border(1.dp, c.border, RoundedCornerShape(Radii.sm)).clickable { onClick() },
        contentAlignment = Alignment.Center,
    ) { MadarIcon(icon, tint = c.textPrimary, size = IconSize.md) }
}
