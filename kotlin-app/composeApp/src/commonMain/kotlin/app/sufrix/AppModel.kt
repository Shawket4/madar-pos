package app.sufrix

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.sufrix.core.AppRoute
import app.sufrix.core.BranchView
import app.sufrix.core.CartLineView
import app.sufrix.core.CartTotals
import app.sufrix.core.CashMovementView
import app.sufrix.core.CheckoutInput
import app.sufrix.core.CheckoutSplit
import app.sufrix.core.AddonSelection
import app.sufrix.core.BundleComponentSelection
import app.sufrix.core.BundleView
import app.sufrix.core.CategoryView
import app.sufrix.core.ComputedRecipeLineView
import app.sufrix.core.CoreException
import app.sufrix.core.DiscountView
import app.sufrix.core.DeliveryOrderView
import app.sufrix.core.DiagLogView
import app.sufrix.core.DraftView
import app.sufrix.core.ItemAddonView
import app.sufrix.core.LoginMode
import app.sufrix.core.LoginRequest
import app.sufrix.core.MenuItemView
import app.sufrix.core.OrderDetailView
import app.sufrix.core.OrderSummaryView
import app.sufrix.core.OutboxItemView
import app.sufrix.core.PaymentMethodView
import app.sufrix.core.ReceiptView
import app.sufrix.core.SessionSnapshot
import app.sufrix.core.ShiftReportView
import app.sufrix.core.ShiftSummaryView
import app.sufrix.core.ShiftView
import app.sufrix.core.SufrixCore
import app.sufrix.core.TokenStore
import app.sufrix.ui.ChipTone
import app.sufrix.ui.ThemeMode
import app.sufrix.ui.ToastData

/**
 * Host secure-bytes vault — the core's [TokenStore] plus the host-only reads the
 * core doesn't push: the cold-start blob, the device's configured branch, and the
 * theme preference. Implemented per platform (Android filesDir / desktop home).
 */
interface HostVault : TokenStore {
    fun loadBlob(): ByteArray?
    var branchId: String
    var branchName: String
    var themeMode: String
    var locale: String
    var printerHost: String
}

/** Receipt-printing progress for the confirmation screen's Print button. */
enum class PrintState { IDLE, PRINTING, PRINTED, FAILED, NO_PRINTER }

/** Device-setup is two steps: a manager authenticates, then picks the branch. */
enum class SetupPhase { CREDENTIALS, PICK_BRANCH }

/**
 * The host's single source of UI state, shared by Android + desktop. Mirrors the
 * Swift `AppModel`. NO business logic — the online↔offline decision, token
 * custody, localization and validation all live in the core.
 */
class AppModel(val core: SufrixCore, private val vault: HostVault) {
    var session by mutableStateOf<SessionSnapshot?>(null)
        private set
    var isBusy by mutableStateOf(false)
        private set
    var error by mutableStateOf<String?>(null)
    var branchId by mutableStateOf(vault.branchId)
        private set
    var branchName by mutableStateOf(vault.branchName)
        private set
    var reconfiguring by mutableStateOf(false)
        private set
    var setupPhase by mutableStateOf(SetupPhase.CREDENTIALS)
        private set
    var branches by mutableStateOf<List<BranchView>>(emptyList())
        private set
    /** Theme preference — defaults to LIGHT (the original navy palette). */
    var themeMode by mutableStateOf(
        runCatching { ThemeMode.valueOf(vault.themeMode) }.getOrDefault(ThemeMode.LIGHT)
    )
        private set
    /** Active UI locale (en/ar) — changing it re-resolves strings + RTL. */
    var locale by mutableStateOf(vault.locale.ifBlank { core.locale() })
        private set
    /** Drives the settings screen (shown over the order screen). */
    var showSettings by mutableStateOf(false)
    /** Drives the "More" overflow drawer (secondary nav-hub actions). */
    var showMore by mutableStateOf(false)
    /** Network printer address ("host" or "host:port"; default port 9100). Empty
     *  = no printer configured. Set in Settings, persisted in the host vault. */
    var printerHost by mutableStateOf(vault.printerHost)
        private set
    /** Print progress for the receipt confirmation's Print button. */
    var printState by mutableStateOf(PrintState.IDLE)
        private set
    /** The device's current shift (drives OpenShift ↔ Order routing). */
    var shift by mutableStateOf<ShiftView?>(null)
        private set
    /** Carried-over opening-cash suggestion (previous declared closing, minor
     *  units; 0 = none). Prefills the open-shift count field. */
    var suggestedOpeningCashMinor by mutableStateOf(0L)
        private set
    /** Branch-effective catalog (cached; reads always succeed offline). */
    var categories by mutableStateOf<List<CategoryView>>(emptyList())
        private set
    var menuItems by mutableStateOf<List<MenuItemView>>(emptyList())
        private set
    /** The in-progress cart (client-only, kv-persisted in the core). */
    var cartLines by mutableStateOf<List<CartLineView>>(emptyList())
        private set
    var cartTotals by mutableStateOf(CartTotals(0L, 0L, 0L, 0L, 0L))
        private set
    /** Org payment methods (cached) — the tender picker source. */
    var paymentMethods by mutableStateOf<List<PaymentMethodView>>(emptyList())
        private set
    /** Org discounts (cached) — the tender discount picker source. */
    var discounts by mutableStateOf<List<DiscountView>>(emptyList())
        private set
    /** The cart's selected discount id (null = none). */
    var cartDiscountId by mutableStateOf<String?>(null)
        private set
    /** The last placed order's receipt (drives the confirmation screen). */
    var receipt by mutableStateOf<ReceiptView?>(null)
        private set
    var isPlacingOrder by mutableStateOf(false)
        private set

    // ── transient toast / snackbar ──────────────────────────────────────────
    /** The active toast (null = none). Rendered by [ui.ToastHost] at the root. */
    var toast by mutableStateOf<ToastData?>(null)
        private set
    private var toastAction: (() -> Unit)? = null
    private var toastSeq = 0

    /** Flash a transient message at the bottom of the screen, optionally with one
     *  action (e.g. "Undo"). [ui.ToastHost] auto-dismisses after [seconds]. */
    fun showToast(
        text: String,
        tone: ChipTone = ChipTone.NEUTRAL,
        actionLabel: String? = null,
        action: (() -> Unit)? = null,
        seconds: Double = 2.6,
    ) {
        toastSeq += 1
        toastAction = action
        toast = ToastData(toastSeq, text, tone, actionLabel, seconds)
    }

    /** Dismiss the toast if it's still the one with [id] (timer-safe). */
    fun dismissToast(id: Int) {
        if (toast?.id == id) {
            toast = null
            toastAction = null
        }
    }

    /** Invoke the active toast's action and dismiss it. */
    fun runToastAction() {
        val action = toastAction
        toastAction = null
        toast = null
        action?.invoke()
    }

    init {
        core.setTokenStore(vault)
        vault.loadBlob()?.let { session = core.restoreSession(it) }
        // Apply the saved locale to the core before any string resolves.
        vault.locale.takeIf { it.isNotBlank() }?.let { core.setLocale(it) }
        loadShift()
    }

    /** Change the UI locale (en/ar); re-resolves strings + RTL, persists. The
     *  cached catalog was projected under the old locale, so re-read it (offline,
     *  from the mirror) to switch item/category/payment labels immediately. */
    // @JvmName avoids a JVM clash with the `locale` property's (private) setter,
    // which also lowers to setLocale(String); Kotlin call sites are unaffected.
    @JvmName("applyLocale")
    fun setLocale(value: String) {
        core.setLocale(value)
        locale = value
        vault.locale = value
        reprojectCatalog()
    }

    val isSignedIn: Boolean get() = session != null
    val isBranchConfigured: Boolean get() = branchId.isNotBlank()

    // ── localization ──────────────────────────────────────────────────────────
    fun t(key: String): String = core.tr(key)
    val isRTL: Boolean get() = core.isRtl()

    @JvmName("applyThemeMode")
    fun setThemeMode(mode: ThemeMode) {
        themeMode = mode
        vault.themeMode = mode.name
    }

    // ── teller ──────────────────────────────────────────────────────────────
    suspend fun signInTeller(name: String, pin: String) {
        isBusy = true; error = null
        try {
            session = core.signIn(LoginRequest(LoginMode.PIN, name, pin, branchId, null, null, null))
            reconcileShift()
        } catch (e: CoreException) {
            error = humanMessage(e)
        } catch (e: Exception) {
            error = e.message ?: core.tr("err.generic")
        } finally {
            isBusy = false
        }
    }

    // ── shift + routing ────────────────────────────────────────────────────────
    /** The screen to show — the core decides. Reading session/shift/branch here
     *  registers them so Compose recomposes the route when they change. */
    val route: AppRoute
        get() {
            @Suppress("UNUSED_EXPRESSION")
            run { session; shift; branchId; reconfiguring }
            return core.appRoute(isBranchConfigured, reconfiguring)
        }

    /** `true` while a shift is OPEN — gates sign-out / device-reconfigure. */
    val hasOpenShift: Boolean get() = shift?.isOpen ?: false

    /** Surface a guidance/validation message in the active screen's error slot. */
    fun flagError(message: String) { error = message }
    /** Clear the current error (on screen entry / next user action). */
    fun clearError() { error = null }

    /**
     * Open a shift with the counted opening cash (minor units). `editReason` is
     * required by the UI only when the count deviates from the carried-over
     * closing; the server re-derives the deviation. Works offline.
     */
    suspend fun openShift(openingCashMinor: Long, editReason: String? = null) {
        isBusy = true; error = null
        try {
            shift = core.openShift(openingCashMinor, editReason)
        } catch (e: CoreException) {
            error = humanMessage(e)
        } catch (e: Exception) {
            error = e.message ?: core.tr("err.generic")
        } finally {
            isBusy = false
        }
    }

    /** Prime the open-shift screen: refresh the server prefill when online, then
     *  read the carried-over opening-cash suggestion from the core (cheap + safe
     *  offline — reads the locally-cached suggestion). */
    suspend fun loadOpenShiftPrefill() {
        // Show the locally-cached suggestion instantly…
        suggestedOpeningCashMinor = runCatching { core.suggestedOpeningCashMinor() }.getOrDefault(0L)
        // …then refresh it from the server (last synced declared closing).
        if (session?.online == true) {
            runCatching { core.refreshShift() }
            suggestedOpeningCashMinor = runCatching { core.suggestedOpeningCashMinor() }.getOrDefault(0L)
        }
    }

    /** Reconcile the shift with the server when online (existing shift on login,
     *  dashboard force-close); use the local cache offline. */
    suspend fun reconcileShift() {
        shift = if (session?.online == true) {
            // Never let a transient/network refresh error nuke a good local shift
            // — that's what bounced the teller back to open-shift. Fall back to the
            // local cache; only a successful refresh updates the shift.
            runCatching { core.refreshShift() }.getOrElse { runCatching { core.currentShift() }.getOrNull() }
        } else {
            runCatching { core.currentShift() }.getOrNull()
        }
    }

    fun loadShift() {
        shift = runCatching { core.currentShift() }.getOrNull()
    }

    // ── catalog ────────────────────────────────────────────────────────────────
    /** Load the branch-effective catalog: pull a fresh copy when online (best
     *  effort), then read the local mirror (always succeeds, even offline). */
    suspend fun loadCatalog() {
        if (session?.online == true) runCatching { core.refreshCatalog() }
        reprojectCatalog()
        loadCart()
        refreshPending()
    }

    /** Re-read the catalog projections from the local mirror under the current
     *  locale (no network). Used by loadCatalog and on a locale change so the
     *  labels follow the language without a re-fetch. */
    fun reprojectCatalog() {
        categories = runCatching { core.listCategories() }.getOrDefault(emptyList())
        menuItems = runCatching { core.listMenuItems() }.getOrDefault(emptyList())
        paymentMethods = runCatching { core.listPaymentMethods() }.getOrDefault(emptyList())
        discounts = runCatching { core.listDiscounts() }.getOrDefault(emptyList())
        loadBundles()
    }

    /** Apply or clear the cart discount (re-reads totals so the UI updates). */
    fun setDiscount(id: String?) {
        if (id != null) runCatching { core.cartSetDiscount(id) } else runCatching { core.cartClearDiscount() }
        cartDiscountId = runCatching { core.cartDiscountId() }.getOrNull()
        refreshCartTotals()
    }

    // ── checkout ───────────────────────────────────────────────────────────────
    /** Place the cart as an order via the core (online or queued offline). On
     *  success the core has emptied the cart; reload it and surface the receipt. */
    suspend fun placeOrder(
        paymentMethodId: String,
        amountTenderedMinor: Long,
        tipMinor: Long = 0L,
        tipPaymentMethodId: String? = null,
        customerName: String? = null,
        notes: String? = null,
        splits: List<CheckoutSplit> = emptyList(),
    ) {
        isPlacingOrder = true; error = null
        try {
            val input = CheckoutInput(paymentMethodId, amountTenderedMinor, tipMinor, tipPaymentMethodId, customerName, notes, splits)
            receipt = core.checkout(input)
            printState = PrintState.IDLE
            loadCart()
            refreshPending()
            // Refresh the stats pill (the receipt already shows via reactive state).
            loadHistory()
        } catch (e: CoreException) {
            error = humanMessage(e)
        } catch (e: Exception) {
            error = e.message ?: core.tr("err.generic")
        } finally {
            isPlacingOrder = false
        }
    }

    /** Dismiss the receipt confirmation (back to the catalog). */
    fun dismissReceipt() { receipt = null; printState = PrintState.IDLE }

    /** Total quantity of an item already in the cart, summed across its config
     *  variants — drives the catalog card's in-cart badge. */
    fun cartQtyForItem(itemId: String): Long = cartLines.filter { it.itemId == itemId }.sumOf { it.qty }

    /** Set the network printer address (Settings); persisted in the host vault. */
    @JvmName("applyPrinterHost")
    fun setPrinterHost(value: String) {
        printerHost = value
        vault.printerHost = value
    }

    /** Render the current receipt in the core and stream it to the configured
     *  network printer (best-effort; unverifiable without hardware). All the
     *  layout/bytes live in the core — this only moves them onto the wire. */
    suspend fun printReceipt() {
        val r = receipt ?: return
        val (host, port) = parsePrinter(printerHost)
        if (host.isBlank()) { printState = PrintState.NO_PRINTER; return }
        printState = PrintState.PRINTING
        val bytes = core.renderReceipt(r, branchName, session?.currencyCode ?: "", 32u)
        printState = try {
            core.sendToPrinter(host, port, bytes)
            // Pop the till on a cash sale (the original print, not a reprint).
            if (r.isCash) runCatching { core.sendToPrinter(host, port, core.cashDrawerKick()) }
            PrintState.PRINTED
        } catch (e: Exception) {
            PrintState.FAILED
        }
    }

    /** Print the shift report (Z-report) — same printer path as the receipt. */
    suspend fun printShiftReport() {
        val report = shiftReport ?: return
        val (host, port) = parsePrinter(printerHost)
        if (host.isBlank()) { printState = PrintState.NO_PRINTER; return }
        printState = PrintState.PRINTING
        val bytes = core.renderShiftReport(report, branchName, session?.currencyCode ?: "", 32u)
        printState = try {
            core.sendToPrinter(host, port, bytes)
            PrintState.PRINTED
        } catch (e: Exception) {
            PrintState.FAILED
        }
    }

    /** Split "host" / "host:port" → (host, port); default JetDirect port 9100. */
    private fun parsePrinter(raw: String): Pair<String, UShort> {
        val default: UShort = 9100.toUShort()
        val trimmed = raw.trim()
        val colon = trimmed.lastIndexOf(':')
        if (colon < 0) return trimmed to default
        val host = trimmed.substring(0, colon)
        val port = trimmed.substring(colon + 1).toUShortOrNull() ?: default
        return host to port
    }

    // ── sync center (outbox) ─────────────────────────────────────────────────────
    var showSync by mutableStateOf(false)
    var outbox by mutableStateOf<List<OutboxItemView>>(emptyList())
        private set
    /** Queued/in-flight command count — the sync chip badge. */
    var pendingCount by mutableStateOf(0)
        private set
    /** Dead/stuck command count — the "needs attention" chip + danger badge. */
    var syncFailed by mutableStateOf(0)
        private set
    /** Session connectivity — drives the offline banner + sync chip state. */
    var isOnline by mutableStateOf(true)
        private set
    /** Server-vs-device clock skew in minutes (drives the clock-skew banner). */
    var clockSkewMinutes by mutableStateOf(0)
        private set

    /** Refresh the sync chrome signals (chip counts + online) in one local read. */
    fun refreshPending() {
        runCatching { core.syncStatus() }.getOrNull()?.let {
            pendingCount = it.pending.toInt()
            syncFailed = it.failed.toInt()
            isOnline = it.online
        }
    }
    /** Connectivity heartbeat — ping (updates online + skew + drains), then
     *  re-read the chrome. Called on appear + on a 15s timer by the order screen. */
    suspend fun refreshConnectivity() {
        if (session == null) return
        core.refreshConnectivity()
        clockSkewMinutes = core.clockSkewMinutes()
        refreshPending()
    }
    fun loadOutbox() {
        outbox = runCatching { core.listOutbox() }.getOrDefault(emptyList())
        refreshPending()
    }
    suspend fun retryOutbox() {
        runCatching { core.retryOutbox() }
        loadOutbox()
    }
    fun discardOutboxItem(id: String) {
        runCatching { core.discardOutboxItem(id) }
        loadOutbox()
    }

    // ── order history ────────────────────────────────────────────────────────────
    var showHistory by mutableStateOf(false)
    var history by mutableStateOf<List<OrderSummaryView>>(emptyList())
        private set
    var isLoadingHistory by mutableStateOf(false)
        private set
    /** Live shift totals for the action-bar pill, derived from `history`. */
    var shiftSalesMinor by mutableStateOf(0L)
        private set
    var shiftOrderCount by mutableStateOf(0)
        private set
    /** The fetched lines for the expanded history row (null = none/queued). */
    var orderDetail by mutableStateOf<OrderDetailView?>(null)
        private set

    /** Load an order's lines for the expanded history row (best-effort). */
    suspend fun loadOrderDetail(id: String) {
        orderDetail = runCatching { core.orderDetail(id) }.getOrNull()
    }

    /** Re-render and re-print a past order — same printer path as the receipt. */
    suspend fun reprintOrder(id: String) {
        val (host, port) = parsePrinter(printerHost)
        if (host.isBlank()) { printState = PrintState.NO_PRINTER; return }
        printState = PrintState.PRINTING
        val bytes = core.renderOrderReceipt(id, branchName, session?.currencyCode ?: "", 32u)
        printState = try {
            core.sendToPrinter(host, port, bytes)
            PrintState.PRINTED
        } catch (e: Exception) {
            PrintState.FAILED
        }
    }

    /** Load the current shift's orders (synced + queued). Best-effort. Also
     *  refreshes the stats pill from the same list (voided excluded, in core). */
    suspend fun loadHistory() {
        isLoadingHistory = true
        history = runCatching { core.listShiftOrders() }.getOrDefault(emptyList())
        val stats = core.shiftStats(history)
        shiftSalesMinor = stats.salesMinor
        shiftOrderCount = stats.orderCount.toInt()
        isLoadingHistory = false
    }

    /** Void a synced order (queues offline). Reloads history on success so the
     *  row flips to Voided. Returns whether it succeeded (the sheet dismisses). */
    suspend fun voidOrder(orderId: String, reason: String, note: String?, restoreInventory: Boolean = true): Boolean {
        isBusy = true; error = null
        return try {
            core.voidOrder(orderId, reason, note?.takeIf { it.isNotBlank() }, restoreInventory)
            loadHistory()
            refreshPending()
            true
        } catch (e: CoreException) {
            error = humanMessage(e); false
        } catch (e: Exception) {
            error = e.message ?: core.tr("err.generic"); false
        } finally {
            isBusy = false
        }
    }

    // ── close shift ────────────────────────────────────────────────────────────
    /** Drives the close-shift screen (shown over the order screen). */
    var showCloseShift by mutableStateOf(false)
    /** The current shift's report (expected cash + breakdown), loaded on close. */
    var shiftReport by mutableStateOf<ShiftReportView?>(null)
        private set

    /** Load the shift report (best-effort) for the close-shift system-cash row. */
    suspend fun loadShiftReport() {
        shiftReport = runCatching { core.shiftReport() }.getOrNull()
    }

    /** Close the open shift with the counted cash + optional note. On success the
     *  core marks the shift closed, so the route flips back to open-shift. */
    suspend fun closeShift(closingCashMinor: Long, note: String?) {
        isBusy = true; error = null
        try {
            core.closeShift(closingCashMinor, note?.takeIf { it.isNotBlank() })
            loadShift()           // now closed → route flips to open-shift
            showCloseShift = false
        } catch (e: CoreException) {
            error = humanMessage(e)
        } catch (e: Exception) {
            error = e.message ?: core.tr("err.generic")
        } finally {
            isBusy = false
        }
    }

    // ── cash movements + shift history (online) ──────────────────────────────────
    var showCashMovements by mutableStateOf(false)
    var showShiftHistory by mutableStateOf(false)
    var cashMovements by mutableStateOf<List<CashMovementView>>(emptyList())
        private set
    var shiftHistory by mutableStateOf<List<ShiftSummaryView>>(emptyList())
        private set
    var isLoadingCash by mutableStateOf(false)
        private set
    var isLoadingShifts by mutableStateOf(false)
        private set

    /** The open shift's cash movements (online read). */
    suspend fun loadCashMovements() {
        isLoadingCash = true
        cashMovements = runCatching { core.listCashMovements() }.getOrDefault(emptyList())
        isLoadingCash = false
    }

    /** Record a pay-in (amount > 0) or pay-out (amount < 0). Reloads the list on
     *  success; surfaces the error otherwise. Returns whether it succeeded. */
    suspend fun recordCashMovement(amountMinor: Long, note: String): Boolean {
        isBusy = true; error = null
        return try {
            core.recordCashMovement(amountMinor, note)
            loadCashMovements()
            true
        } catch (e: CoreException) {
            error = humanMessage(e); false
        } catch (e: Exception) {
            error = e.message ?: core.tr("err.generic"); false
        } finally {
            isBusy = false
        }
    }

    /** Past shifts for the branch (online read). */
    suspend fun loadShiftHistory() {
        isLoadingShifts = true
        shiftHistory = runCatching { core.listShifts() }.getOrDefault(emptyList())
        isLoadingShifts = false
    }

    // ── cart ───────────────────────────────────────────────────────────────────
    /** Add one unit of [item]. Sync (the core just touches kv) so the tap feels
     *  instant; the core merges into the matching line. */
    fun addToCart(item: MenuItemView) = applyCart { core.cartAdd(item.id, item.name, item.basePriceMinor) }
    fun setCartQty(itemId: String, qty: Long) = applyCart { core.cartSetQty(itemId, qty) }
    fun removeCartLine(itemId: String) = applyCart { core.cartRemove(itemId) }
    /** Swipe-to-delete: remove the whole line and offer an Undo toast. */
    fun swipeRemoveCartLine(line: CartLineView) {
        applyCart { core.cartRemove(line.key) }
        showToast("${t("order.removed")} ${line.name}", ChipTone.NEUTRAL, t("order.undo"), { undoRemoveCartLine() }, 4.0)
    }
    fun undoRemoveCartLine() = applyCart { core.cartRestoreRemoved() }
    fun clearCart() {
        runCatching { core.cartClear() }
        cartLines = emptyList()
        refreshCartTotals()
    }

    private fun loadCart() {
        cartLines = runCatching { core.cartLines() }.getOrDefault(emptyList())
        cartDiscountId = runCatching { core.cartDiscountId() }.getOrNull()
        refreshCartTotals()
    }
    /** Run a cart mutation that returns the new lines, then refresh totals. */
    private fun applyCart(op: () -> List<CartLineView>) {
        runCatching { op() }.getOrNull()?.let {
            cartLines = it
            refreshCartTotals()
        }
    }
    private fun refreshCartTotals() {
        cartTotals = runCatching { core.cartTotals() }.getOrDefault(CartTotals(0L, 0L, 0L, 0L, 0L))
    }

    // ── drafts / held orders ──────────────────────────────────────────────────────
    /** Drives the held-orders screen (shown over the order screen). */
    var showDrafts by mutableStateOf(false)
    var drafts by mutableStateOf<List<DraftView>>(emptyList())
        private set

    fun loadDrafts() { drafts = runCatching { core.listDrafts() }.getOrDefault(emptyList()) }

    /** Park the current cart as a held order, auto-named by time of day. */
    fun holdCart() {
        val name = java.time.LocalTime.now().toString().take(5)
        runCatching { core.holdCart(name) }
        loadCart(); loadDrafts()
    }

    /** Restore a held order into the cart (replacing the current one). */
    fun restoreDraft(id: String) {
        cartLines = runCatching { core.restoreDraft(id) }.getOrDefault(cartLines)
        cartDiscountId = runCatching { core.cartDiscountId() }.getOrNull()
        refreshCartTotals()
        loadDrafts()
    }

    fun discardDraft(id: String) {
        runCatching { core.discardDraft(id) }
        loadDrafts()
    }

    // ── delivery orders (online; teller works the live branch queue) ──────────────
    var showDelivery by mutableStateOf(false)
    var deliveryOrders by mutableStateOf<List<DeliveryOrderView>>(emptyList())
        private set
    var isLoadingDelivery by mutableStateOf(false)
        private set
    var deliveryActiveOnly by mutableStateOf(true)

    private val activeStatusFilter = "received,confirmed,preparing,ready,out_for_delivery"

    /** The branch delivery queue (online). Active-only by default. */
    suspend fun loadDeliveryOrders() {
        isLoadingDelivery = true
        try {
            val status = if (deliveryActiveOnly) activeStatusFilter else null
            deliveryOrders = core.listDeliveryOrders(status)
        } catch (e: CoreException) {
            error = humanMessage(e)
        } finally {
            isLoadingDelivery = false
        }
    }
    /** Advance one lifecycle step (Confirm → Preparing → … → Delivered). */
    suspend fun advanceDelivery(o: DeliveryOrderView) {
        isBusy = true; error = null
        try { core.deliveryAdvanceStatus(o.id, o.status); loadDeliveryOrders() }
        catch (e: CoreException) { error = humanMessage(e) }
        finally { isBusy = false }
    }
    /** Add extra prep time (multiples of 5). */
    suspend fun addDeliveryPrep(o: DeliveryOrderView, minutes: Int = 5) {
        try { core.deliverySetPrepTime(o.id, minutes); loadDeliveryOrders() }
        catch (e: CoreException) { error = humanMessage(e) }
    }
    /** Cancel a delivery order (optionally restocking ingredients). */
    suspend fun cancelDelivery(o: DeliveryOrderView, reason: String?, restoreInventory: Boolean): Boolean {
        isBusy = true; error = null
        return try { core.deliveryCancel(o.id, reason, restoreInventory); loadDeliveryOrders(); true }
        catch (e: CoreException) { error = humanMessage(e); false }
        finally { isBusy = false }
    }
    /** Finalize into a real sale on the open shift, charged to a payment method. */
    suspend fun finalizeDelivery(o: DeliveryOrderView, paymentMethodId: String): Boolean {
        isBusy = true; error = null
        return try {
            val res = core.deliveryFinalize(o.id, paymentMethodId)
            loadDeliveryOrders()
            showToast(t("delivery.finalized") + (res.orderRef?.let { " · $it" } ?: ""), ChipTone.SUCCESS)
            true
        } catch (e: CoreException) { error = humanMessage(e); false }
        finally { isBusy = false }
    }

    // ── diagnostics (Settings → recent sync warnings) ─────────────────────────────
    var diagnostics by mutableStateOf<List<DiagLogView>>(emptyList())
        private set
    fun loadDiagnostics() { diagnostics = core.recentLogs() }
    fun clearDiagnostics() { core.clearLogs(); diagnostics = emptyList() }

    // ── item customization ───────────────────────────────────────────────────────
    /** Non-null = the customization sheet is open for this item. */
    var detailItem by mutableStateOf<MenuItemView?>(null)
    /** The cart line key being edited (null = adding a new line). */
    var detailEditKey by mutableStateOf<String?>(null)
    /** The cart line being edited (seeds the sheet), null when adding fresh. */
    var detailEditLine by mutableStateOf<CartLineView?>(null)
    /** The item's addons with charged prices resolved by the core (for the sheet). */
    var itemAddons by mutableStateOf<List<ItemAddonView>>(emptyList())
        private set

    /** Whether tapping [item] should open the customization sheet vs add directly. */
    fun hasOptions(item: MenuItemView): Boolean =
        item.sizes.isNotEmpty() || item.addonSlots.isNotEmpty() || item.optionalFields.isNotEmpty()

    fun openItemDetail(item: MenuItemView, editKey: String? = null, editLine: CartLineView? = null) {
        detailEditKey = editKey
        detailEditLine = editLine
        itemAddons = runCatching { core.listItemAddons(item.id) }.getOrDefault(emptyList())
        detailItem = item
    }
    /** Re-open the sheet for a configured cart line so the teller can change it. */
    fun editCartLine(line: CartLineView) {
        // Any cart line is editable — reopens the customization sheet seeded from
        // the line; addConfigured removes the old line (by its key) and re-adds.
        val item = menuItems.firstOrNull { it.id == line.itemId } ?: return
        openItemDetail(item, line.key, line)
    }
    fun closeItemDetail() { detailItem = null; detailEditKey = null; detailEditLine = null }

    /** Live recipe preview for the current selection — the core derives the
     *  effective ingredients (base by size, milk/coffee swaps, additive addons,
     *  optional contributions). Pure + cheap, so the sheet recomputes per toggle. */
    fun recipePreview(itemId: String, sizeLabel: String?, addons: List<AddonSelection>, optionalIds: List<String>): List<ComputedRecipeLineView> =
        runCatching { core.computeRecipe(itemId, sizeLabel, addons, optionalIds) }.getOrDefault(emptyList())

    /** Add (or, in edit mode, replace) a configured line. The core resolves the
     *  charged prices from the catalog; we just pass the selection. */
    fun addConfigured(itemId: String, sizeLabel: String?, addons: List<AddonSelection>, optionalIds: List<String>, qty: Long, notes: String?) {
        detailEditKey?.let { runCatching { core.cartRemove(it) } }
        runCatching { core.cartAddConfigured(itemId, sizeLabel, addons, optionalIds, qty, notes) }
        loadCart()
        refreshPending()
        closeItemDetail()
    }

    // ── bundles / combos ───────────────────────────────────────────────────────
    /** Available bundles (status active + within their date/time window) — the
     *  Combos section of the catalog. */
    var bundles by mutableStateOf<List<BundleView>>(emptyList())
        private set
    /** Non-null = the bundle configuration sheet is open. */
    var detailBundle by mutableStateOf<BundleView?>(null)

    fun loadBundles() {
        bundles = runCatching { core.availableBundles(nowRfc3339()) }.getOrDefault(emptyList())
    }
    fun openBundleDetail(b: BundleView) { detailBundle = b }
    fun closeBundleDetail() { detailBundle = null }

    /** Resolve a bundle component's [MenuItemView] and load its addons into
     *  [itemAddons] so the per-component sheet (ItemDetailSheet) can render them. */
    fun componentItem(itemId: String): MenuItemView? {
        val item = menuItems.firstOrNull { it.id == itemId } ?: return null
        itemAddons = runCatching { core.listItemAddons(itemId) }.getOrDefault(emptyList())
        return item
    }

    /** Add a configured bundle to the cart — the core resolves each component's
     *  charged extras and records one bundle line at the fixed bundle price. */
    fun addBundle(bundleId: String, components: List<BundleComponentSelection>) {
        runCatching { core.cartAddBundle(bundleId, components, 1L) }
        loadCart()
        refreshPending()
        closeBundleDetail()
    }

    /** Local time as RFC3339 with a colon offset, so the core gates bundle windows
     *  in the till's timezone (the till sits at the branch). OffsetDateTime.toString()
     *  yields e.g. 2026-06-21T11:00:00+02:00, which chrono parses. */
    private fun nowRfc3339(): String = java.time.OffsetDateTime.now().toString()

    // ── device setup (manager) ────────────────────────────────────────────────
    suspend fun authenticateManager(email: String, password: String) {
        isBusy = true; error = null
        try {
            core.login(LoginRequest(LoginMode.EMAIL, null, null, null, email, password, null))
            branches = core.listBranches()
            setupPhase = SetupPhase.PICK_BRANCH
        } catch (e: CoreException) {
            error = humanMessage(e); runCatching { core.logout(false) }; session = null
        } catch (e: Exception) {
            error = e.message ?: core.tr("err.generic"); runCatching { core.logout(false) }; session = null
        } finally {
            isBusy = false
        }
    }

    fun bindBranch(branch: BranchView) {
        branchId = branch.id
        branchName = branch.name
        vault.branchId = branchId
        vault.branchName = branchName
        runCatching { core.logout(false) }
        session = null
        reconfiguring = false
        setupPhase = SetupPhase.CREDENTIALS
        branches = emptyList()
        error = null
    }

    fun beginReconfigure() {
        reconfiguring = true; setupPhase = SetupPhase.CREDENTIALS; branches = emptyList(); error = null
    }
    fun cancelReconfigure() {
        reconfiguring = false; setupPhase = SetupPhase.CREDENTIALS; branches = emptyList(); error = null
        runCatching { core.logout(false) }; session = null
    }

    fun signOut() {
        runCatching { core.logout(false) }
        session = null
        shift = null
        cartLines = emptyList()
        cartTotals = CartTotals(0L, 0L, 0L, 0L, 0L)
        receipt = null
        error = null
    }

    /** Host-side errors localized; server messages pass through. */
    private fun humanMessage(e: CoreException): String = when (e) {
        is CoreException.Offline -> core.tr("err.offline_no_setup")
        is CoreException.Unauthenticated -> e.message ?: core.tr("err.generic")
        is CoreException.Validation -> e.message ?: core.tr("err.generic")
        is CoreException.Server -> e.message ?: core.tr("err.generic")
        is CoreException.Transient -> core.tr("err.network")
        is CoreException.Forbidden -> core.tr("err.not_allowed")
        is CoreException.Internal -> e.message?.takeIf { it.isNotBlank() } ?: core.tr("err.generic")
    }
}
