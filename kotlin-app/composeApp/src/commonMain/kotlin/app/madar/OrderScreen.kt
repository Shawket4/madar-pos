package app.madar

import androidx.compose.animation.Crossfade
import androidx.compose.foundation.background
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
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
import androidx.compose.foundation.layout.heightIn
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
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.unit.IntOffset
import kotlin.math.roundToInt
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
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
import app.madar.ui.MadarLockupMark
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

    // Memoized: re-filter only when the menu, category, or query actually change —
    // not on every cart edit / connectivity tick. One pass, no intermediate lists.
    val visible = remember(model.menuItems, selectedCategory, search) {
        model.menuItems.filter { item ->
            item.isActive &&
                (selectedCategory == null || item.categoryId == selectedCategory) &&
                (search.isBlank() ||
                    item.name.contains(search, ignoreCase = true) ||
                    (item.description?.contains(search, ignoreCase = true) ?: false))
        }
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
        // Side-rail destinations, grouped into intuitive sections (the rail itself
        // takes data + callbacks, not the model — compose-state-hoisting). Task
        // categories scroll in the middle; system utilities pin to the footer.
        val incomingDest = NavDest("bicycle", t("nav.incoming"), hasNew = model.deliveryHasNew || model.ticketsHasNew) {
            model.error = null; model.clearDeliveryBadge(); model.clearTicketsBadge()
            scope.launch { model.loadDeliveryOrders(); model.loadOpenTickets() }; model.showIncoming = true
        }
        val draftsDest = NavDest("tray.full", t("drafts.title")) { model.loadDrafts(); model.showDrafts = true }
        val historyDest = NavDest("list.bullet.rectangle", t("nav.history")) { model.showHistory = true }
        val searchDest = NavDest("magnifyingglass", t("search.title")) { model.showOrderSearch = true }
        val cashDest = NavDest("banknote", t("cash.title")) { model.error = null; model.showCashMovements = true }
        val shiftsDest = NavDest("clock.arrow.circlepath", t("shifts.title")) { model.showShiftHistory = true }
        val printDest = NavDest("printer", t("shift.print_report")) { model.openShiftReportPreview() }
        val ticketsDest = NavDest("fork.knife", t("waiter.tickets"), hasNew = model.ticketsHasNew) {
            model.clearTicketsBadge(); scope.launch { model.loadOpenTickets() }; model.showTickets = true
        }
        val syncDest = NavDest("arrow.triangle.2.circlepath", t("sync.title")) { model.loadOutbox(); model.showSync = true }
        val settingsDest = NavDest("gearshape", t("settings.title")) { model.refreshPending(); model.showSettings = true }
        val moreDest = NavDest("ellipsis", t("chrome.more")) { model.refreshPending(); model.showMore = true }
        // Task categories — what's navigated between while working. A waiter device
        // only handles tickets, so it gets the single Orders group.
        val railSections = if (isWaiter) listOf(
            NavSection(t("nav.section.orders"), listOf(ticketsDest)),
        ) else listOf(
            NavSection(t("nav.section.orders"), listOf(incomingDest, draftsDest, historyDest, searchDest)),
            NavSection(t("nav.section.money"), listOf(cashDest, shiftsDest, printDest)),
        )
        // System utilities — always reachable, pinned to the rail footer. The phone
        // drawer reuses the same list (minus More, which IS the drawer).
        val systemDests = listOf(syncDest, settingsDest)
        val railFooter = NavSection(t("nav.section.system"), systemDests + moreDest)
        // Phone has no rail (it's cramped) — the top-bar "options" toggle opens a
        // drawer carrying the same grouped nav above the destructive rows.
        Row(Modifier.fillMaxSize()) {
            if (wide) {
                NavRail(railSections, railFooter, Modifier.width(NavRailWidth).fillMaxHeight())
                Box(Modifier.width(1.dp).fillMaxHeight().background(c.border))
            }
            Column(Modifier.weight(1f).fillMaxHeight()) {
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
        }

        // Input firewall — a fullscreen, visually-invisible layer that swallows ALL
        // pointer events whenever a full-screen overlay/sheet is up. It sits ABOVE the
        // rail Row (drawn earlier, so higher z) but BELOW every overlay block below it
        // (drawn later, so higher still) — so an overlay's own controls keep working,
        // while any tap that lands on the overlay but MISSES a consuming child is eaten
        // here instead of falling through to a hidden NavRail tile / catalog / cart at
        // the same pixel. The back buttons live top-left, right over the rail, which is
        // exactly where the fall-through misfires. Gated on model.hasOverlay, which is
        // the union of every model-backed full-screen screen and sheet rendered below
        // (showCloseShift/showSync/showHistory/showOrderSearch/showCashMovements/
        // showShiftHistory/showDrafts/showIncoming/showTickets/showSettings/
        // showReportPreview/previewShiftReport/previewReceipt/detailItem/detailBundle/
        // showReauth/showMore). The local-state overlays not in that predicate
        // (showCart, showTender, showFireDetails) carry their own fullscreen scrim, so
        // they're already sealed.
        if (model.hasOverlay) {
            Box(
                Modifier.fillMaxSize().pointerInput(Unit) {
                    awaitPointerEventScope { while (true) { awaitPointerEvent() } }
                }
            )
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

        // All-orders search (across shifts) — full-screen over the order screen.
        if (model.showOrderSearch) {
            OrderSearchScreen(model)
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

        // A PAST shift's Z-report preview + print (tapped from Past Shifts).
        model.previewShiftReport?.let { ShiftReportPreviewScreen(model, it) { model.previewShiftReport = null } }

        // Receipt preview before (re)printing a past order.
        model.previewReceipt?.let { ReceiptPreviewScreen(model, it) { model.previewReceipt = null } }

        // Item customization sheet.
        model.detailItem?.let { ItemDetailSheet(model, it, onClose = { model.closeItemDetail() }) }

        // Bundle (combo) configuration sheet.
        model.detailBundle?.let { BundleDetailSheet(model, it) { model.closeBundleDetail() } }

        // Token expired mid-shift → re-auth the same teller to resume sync.
        // Self-gating on model.showReauth (defined in a sibling file).
        ReauthScreen(model)

        // "More" overflow drawer — scrim (tap to dismiss) + a side panel that
        // expands right next to the nav rail (offset by the rail width). RTL-aware.
        if (model.showMore) {
            Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.4f))
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { model.showMore = false })
            // A compact menu that pops from the More control: bottom-left by the
            // rail's More tile on tablet; top-left by the top-bar toggle on phone.
            MoreDrawer(
                model, wide,
                // Phone carries the full grouped nav here (no rail); System drops More
                // (this drawer is More). Tablet shows only the destructive rows.
                phoneSections = if (wide) emptyList() else railSections,
                phoneSystem = if (wide) emptyList() else systemDests,
                modifier = Modifier
                    .align(if (wide) Alignment.BottomStart else Alignment.TopStart)
                    .padding(
                        start = if (wide) NavRailWidth + Space.sm else Space.sm,
                        top = if (wide) 0.dp else 60.dp,
                        bottom = if (wide) Space.sm else 0.dp,
                    ),
            )
        }
    }
}

/** The "More" overflow drawer — secondary nav-hub actions that don't fit the
 *  bar (close shift, settings, sign out). Mirrors Flutter's ActionDrawer. */
@Composable
private fun MoreDrawer(
    model: AppModel,
    wide: Boolean,
    phoneSections: List<NavSection> = emptyList(),
    phoneSystem: List<NavDest> = emptyList(),
    modifier: Modifier = Modifier,
) {
    val c = madarColors()
    val currency = model.session?.currencyCode ?: ""
    Column(
        // A compact floating menu card that hugs its content — a couple of items on
        // tablet, the full nav on phone. Capped height; scrolls only if it gets tall.
        modifier.width(260.dp).heightIn(max = 560.dp)
            .elevation(Elevation.RAISED, RoundedCornerShape(Radii.lg))
            .clip(RoundedCornerShape(Radii.lg)).background(c.surface)
            .border(1.dp, c.borderLight, RoundedCornerShape(Radii.lg))
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {}
            .verticalScroll(rememberScrollState())
            .padding(Space.sm),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        model.shift?.let { s ->
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md))
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
            Modifier.fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            // Phone: the rail's grouped destinations live here (there's no rail on
            // phone) — each task category under its caption. On wide they're exposed
            // in the rail, so these are empty and only the destructive rows remain.
            phoneSections.forEach { section ->
                MoreCaption(section.title)
                section.items.forEach { d ->
                    MoreRow(d.glyph, d.label, c.textPrimary) { model.showMore = false; d.onClick() }
                }
            }
            if (phoneSystem.isNotEmpty()) {
                MoreCaption(t("nav.section.system"))
                phoneSystem.forEach { d ->
                    MoreRow(d.glyph, d.label, c.textPrimary) { model.showMore = false; d.onClick() }
                }
            }
            if (!model.isWaiterDevice) {
                MoreRow("lock", t("order.close_shift"), c.danger) {
                    model.showMore = false; model.error = null; model.showCloseShift = true
                }
            }
            MoreRow("rectangle.portrait.and.arrow.right", t("home.sign_out"), c.textPrimary) {
                // You can't sign out mid-shift — close the drawer first.
                if (model.hasOpenShift) model.flagError(model.t("settings.sign_out_shift_open"))
                else { model.showMore = false; model.signOut() }
            }
        }
    }
}

/** A small uppercase section caption inside the phone "More" drawer — groups the
 *  rows beneath it into a labelled category (mirrors the side rail's captions). */
@Composable
private fun MoreCaption(title: String) {
    val c = madarColors()
    Text(
        title.uppercase(), color = c.textMuted, fontFamily = LocalMadarFont.current,
        fontWeight = FontWeight.SemiBold, fontSize = 10.sp, letterSpacing = 0.8.sp,
        modifier = Modifier.fillMaxWidth().padding(start = Space.xs, top = Space.xs, bottom = 2.dp),
    )
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
// `internal` (not file-private) so the OpenShift screen reuses the SAME banner —
// a teller waiting there sees + recovers a genuine session expiry too.
@Composable
internal fun AuthPausedBanner(onClick: () -> Unit) {
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

// ── Persistent side navigation rail ─────────────────────────────────────────────
// Width owned by the caller (per compose-modifier-and-layout-style: the parent
// places, the component structures).
private val NavRailWidth = 80.dp

/** A leading-edge nav destination: glyph + label + tap. */
internal class NavDest(val glyph: String, val label: String, val hasNew: Boolean = false, val onClick: () -> Unit)

/** A labelled group of rail destinations — the rail and the phone drawer both
 *  render these as a caption + its tiles, so the two stay in lockstep. */
internal class NavSection(val title: String, val items: List<NavDest>)

/** The persistent side rail — destinations grouped into labelled sections. The
 *  task categories ([sections]) scroll in the middle; the [footer] (system
 *  utilities) pins to the bottom. The caller sets the rail's width/height via
 *  [modifier]; the rail only paints its surface + content. */
@Composable
internal fun NavRail(
    sections: List<NavSection>,
    footer: NavSection,
    modifier: Modifier = Modifier,
) {
    val c = madarColors()
    Column(
        modifier = modifier.background(c.surface).padding(vertical = Space.sm),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        MadarLockupMark(width = 64.dp)
        Box(Modifier.height(Space.sm))
        RailDivider()
        Column(
            Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState()).padding(vertical = Space.xs),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            sections.forEachIndexed { i, section ->
                RailCaption(section.title, top = if (i == 0) 0.dp else Space.sm)
                section.items.forEach { NavRailItem(it.glyph, it.label, it.onClick, it.hasNew, Modifier.fillMaxWidth()) }
            }
        }
        RailDivider()
        RailCaption(footer.title, top = 0.dp)
        footer.items.forEach { NavRailItem(it.glyph, it.label, it.onClick, it.hasNew, Modifier.fillMaxWidth()) }
    }
}

@Composable
private fun RailDivider() {
    val c = madarColors()
    Box(Modifier.fillMaxWidth().padding(horizontal = Space.md, vertical = Space.xs).height(1.dp).background(c.borderLight))
}

/** A tiny uppercase caption heading a rail section — what turns the flat list
 *  into intuitive, scannable categories. */
@Composable
private fun RailCaption(title: String, top: Dp) {
    val c = madarColors()
    Text(
        title.uppercase(), color = c.textMuted, fontFamily = LocalMadarFont.current,
        fontWeight = FontWeight.SemiBold, fontSize = 8.sp, letterSpacing = 0.6.sp,
        textAlign = TextAlign.Center, maxLines = 1, overflow = TextOverflow.Ellipsis,
        modifier = Modifier.fillMaxWidth().padding(top = top, bottom = 2.dp, start = 2.dp, end = 2.dp),
    )
}

@Composable
private fun NavRailItem(glyph: String, label: String, onClick: () -> Unit, hasNew: Boolean = false, modifier: Modifier = Modifier) {
    val c = madarColors()
    val haptic = LocalHapticFeedback.current
    val interaction = remember { MutableInteractionSource() }
    Column(
        modifier = modifier
            .pressScale(interaction)
            .clip(RoundedCornerShape(Radii.sm))
            .clickable(interactionSource = interaction, indication = null) {
                haptic.performHapticFeedback(HapticFeedbackType.LongPress); onClick()
            }
            .padding(vertical = 6.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Box(
            // A live SSE event for this module tints the tile + pulses a dot.
            Modifier.size(36.dp).clip(RoundedCornerShape(Radii.sm)).background(if (hasNew) c.accentBg else c.surfaceAlt),
            contentAlignment = Alignment.Center,
        ) {
            MadarIcon(glyph, tint = if (hasNew) c.accent else c.textSecondary, size = IconSize.lg)
            if (hasNew) {
                // A pulsing accent dot at the top-end corner (opacity breathes 1↔0.25).
                val pulse = rememberInfiniteTransition(label = "railBadge")
                val alpha by pulse.animateFloat(
                    initialValue = 1f, targetValue = 0.25f,
                    animationSpec = infiniteRepeatable(tween(750, easing = LinearEasing), RepeatMode.Reverse),
                    label = "railBadgeAlpha",
                )
                Box(
                    Modifier.align(Alignment.TopEnd).padding(3.dp).size(8.dp)
                        .clip(CircleShape).background(c.accent.copy(alpha = alpha)),
                )
            }
        }
        Text(
            label, color = if (hasNew) c.accent else c.textSecondary, fontFamily = LocalMadarFont.current,
            fontWeight = if (hasNew) FontWeight.SemiBold else FontWeight.Medium, fontSize = 10.sp, textAlign = TextAlign.Center,
            maxLines = 1, overflow = TextOverflow.Ellipsis,
        )
    }
}

// ── Top status bar (navigation now lives in the side rail) ───────────────────────
@Composable
private fun OrderTopBar(model: AppModel, wide: Boolean) {
    val c = madarColors()
    val currency = model.session?.currencyCode ?: ""
    val isWaiter = model.isWaiterDevice
    Column(Modifier.fillMaxWidth().background(c.surface)) {
        // Fills + right-pins when it fits; scrolls horizontally only when the
        // content can't fit the viewport (very narrow phones / split layouts).
        FitOrScrollRow(
            Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
            // Phone: no side rail — a leading "options" toggle opens the nav drawer.
            if (!wide) {
                Box(
                    Modifier.size(36.dp).clip(RoundedCornerShape(Radii.sm)).background(c.surfaceAlt)
                        .border(1.dp, c.borderLight, RoundedCornerShape(Radii.sm))
                        .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {
                            model.refreshPending(); model.showMore = true
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    MadarIcon("line.3.horizontal", tint = c.textPrimary, size = IconSize.lg)
                }
            }
            // Status — teller (wide), live shift totals, and sync state.
            if (!isWaiter) {
                if (wide) model.shift?.let { StatusChip(it.tellerName, ChipTone.INFO, icon = "person.fill") }
                if (model.shift?.isOpen == true) ShiftStatsPill(model, currency)
            }
            Box(Modifier.weight(1f))
            SyncChip(model)
            SyncDataButton(model)
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
    val showCombos = bundles.isNotEmpty()
    val combos = selectedCategory == kCombosCategory
    // Categories sit on TOP of the menu (a horizontal tab strip) at every width —
    // the old vertical side rail is gone. On wide, the cart panel lives in the
    // parent Row; here we only lay out the catalog.
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

/** Core CatStyleView.icon key → shared Lucide icon name; null for the 'cafe'
 *  default (custom category) → caller shows the monogram instead. */
private fun categoryIconName(key: String): String? = when (key) {
    "coffee", "mocha", "tea", "bakery", "lunch", "icecream", "drink", "water", "ice", "matcha" -> "cat.$key"
    else -> null
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
internal fun CartLineRow(
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
        Text("$qty", color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 15.sp, textAlign = TextAlign.Center, modifier = Modifier.widthIn(min = 24.dp))
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
internal fun CartFooter(totals: CartTotals, currency: String, onCheckout: () -> Unit, onHold: (() -> Unit)? = null,
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
        // Prominent total block — tinted teal, the figure tellers look at. The
        // sub-rows above stay light so the grand total carries the weight.
        Row(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(Radii.md)).background(c.accentBg)
                .padding(horizontal = Space.md, vertical = Space.md),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(t("order.total"), color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Bold, fontSize = 14.sp)
            Box(Modifier.weight(1f))
            Crossfade(targetState = Money.format(totals.totalMinor, currency), label = "total") { v ->
                Text(v, color = c.accent, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 20.sp)
            }
        }
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
