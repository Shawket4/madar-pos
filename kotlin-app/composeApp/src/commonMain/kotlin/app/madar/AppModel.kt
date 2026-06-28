package app.madar

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.madar.core.AppRoute
import app.madar.core.BranchView
import app.madar.core.DeviceConfigView
import app.madar.core.EventListener
import app.madar.core.KdsStationView
import app.madar.core.KdsTicketView
import app.madar.core.RealtimeEvent
import app.madar.core.RealtimePlayer
import app.madar.core.TicketView
import app.madar.core.PrinterBrand
import app.madar.core.CartLineView
import app.madar.core.CartTotals
import app.madar.core.CashMovementView
import app.madar.core.CheckoutInput
import app.madar.core.CheckoutSplit
import app.madar.core.AddonSelection
import app.madar.core.BundleComponentSelection
import app.madar.core.BundleView
import app.madar.core.CategoryView
import app.madar.core.ComputedRecipeLineView
import app.madar.core.CoreException
import app.madar.core.DiscountView
import app.madar.core.DeliveryOrderView
import app.madar.core.DeliverySettingsView
import app.madar.core.DiagLogView
import app.madar.core.DraftView
import app.madar.core.ItemAddonView
import app.madar.core.LoginMode
import app.madar.core.LoginRequest
import app.madar.core.MenuItemView
import app.madar.core.OrderDetailView
import app.madar.core.OrderSummaryView
import app.madar.core.OutboxItemView
import app.madar.core.PaymentMethodView
import app.madar.core.ReceiptView
import app.madar.core.SessionSnapshot
import app.madar.core.ShiftReportView
import app.madar.core.ShiftSummaryView
import app.madar.core.ShiftView
import app.madar.core.MadarCore
import app.madar.core.TimeStyle
import app.madar.core.TokenStore
import app.madar.ui.ChipTone
import app.madar.ui.RealtimeAlertData
import app.madar.ui.ThemeMode
import app.madar.ui.ToastData

/**
 * Host secure-bytes vault — the core's [TokenStore] plus the host-only reads the
 * core doesn't push: the cold-start blob, the device's configured branch, and the
 * theme preference. Implemented per platform (Android filesDir / desktop home).
 */
interface HostVault : TokenStore {
    fun loadBlob(): ByteArray?
    var branchId: String
    var branchName: String
    var orgLogoUrl: String?
    var themeMode: String
    var locale: String
    var printerHost: String
    var printerBrand: String
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
class AppModel(val core: MadarCore, private val vault: HostVault, private val player: RealtimePlayer) {
    var session by mutableStateOf<SessionSnapshot?>(null)
        private set
    var isBusy by mutableStateOf(false)
        private set
    var error by mutableStateOf<String?>(null)
    /** The device binding (branch / till / station / printer) — owned by the CORE
     *  store now, not the host vault. Mirrored here; mutate only via setDevice*. */
    var deviceConfig by mutableStateOf(core.deviceConfig())
        private set
    var branchId by mutableStateOf(core.deviceConfig().branchId ?: "")
        private set
    var branchName by mutableStateOf(core.deviceConfig().branchName ?: "")
        private set
    /** The org's logo URL for this branch, shown on the receipt header. Prefer the
     *  CORE's durable kv value (persisted from get_branch, refreshed on every data
     *  sync) over the host's last-known vault pref, so it survives restarts, renders
     *  offline, and picks up a logo changed on the dashboard after a manual sync. */
    var orgLogoUrl by mutableStateOf(core.orgLogoUrl() ?: vault.orgLogoUrl)
        private set
    var reconfiguring by mutableStateOf(core.deviceConfig().reconfiguring)
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
    var printerHost by mutableStateOf(printerAddress(core.deviceConfig()))
        private set
    /** Printer command dialect — Epson (ESC/POS) vs Star (Star Line Mode); the two
     *  are not byte-compatible. Mirror of the core device config. */
    var printerBrand by mutableStateOf(
        if (core.deviceConfig().printerBrand == "star") PrinterBrand.STAR else PrinterBrand.EPSON
    )
        private set
    /** Manual LAN-relay hub address ("host" or "host:port") — an optional fixed peer
     *  used when mDNS auto-discovery can't find devices on this Wi-Fi. Empty = none.
     *  Mirror of the core device config; set in Settings via [setLanHub]. */
    var lanHub by mutableStateOf(core.deviceConfig().lanHub ?: "")
        private set
    /** This till's device code (the <DEVICE> segment of every order_ref, e.g.
     *  T1/W2/K1). Custody lives in the core — Settings sets it, the core persists. */
    var deviceCode by mutableStateOf(core.deviceCode())
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
        icon: String? = null,
    ) {
        toastSeq += 1
        toastAction = action
        toast = ToastData(toastSeq, text, tone, actionLabel, seconds, icon)
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

    // ── timestamp formatting (branch-tz, via the core) ───────────────────────────
    // Every displayed timestamp renders in the BRANCH's timezone, not the device's
    // local time. The core owns the tz + format strings; these just thread style.
    fun fmtTime(rfc: String) = core.formatTime(rfc, TimeStyle.TIME)
    fun fmtDateShort(rfc: String) = core.formatTime(rfc, TimeStyle.DATE_SHORT)
    fun fmtDateTime(rfc: String) = core.formatTime(rfc, TimeStyle.DATE_TIME)
    fun fmtReceipt(rfc: String) = core.formatTime(rfc, TimeStyle.RECEIPT)

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
            startLanRelay()
        } catch (e: CoreException) {
            error = humanMessage(e)
        } catch (e: Exception) {
            error = e.message ?: core.tr("err.generic")
        } finally {
            isBusy = false
        }
    }

    // ── re-auth (token expired mid-shift) ────────────────────────────────────────
    /** Present the re-auth PIN sheet. The outbox parked on a 401 (token expired);
     *  the auth-paused banner taps into this. */
    var showReauth by mutableStateOf(false)

    /** Re-authenticate the SAME teller who owns the open shift (no handover) so
     *  `login` un-parks the queue and drains it. Mirrors Swift `reauth`. */
    suspend fun reauth(pin: String) {
        val name = session?.displayName ?: return
        signInTeller(name, pin)
        if (error != null) return
        showReauth = false
        refreshPending()
        // `login` already drained the backlog on success — just reflect it.
        showToast(t("chrome.sync_resumed"), ChipTone.SUCCESS, icon = "checkmark.circle")
    }

    /** The "switch teller" escape from the re-auth prompt: close the open shift,
     *  then routing falls through to login for a new teller (replay flushes the
     *  prior teller's backlog regardless). Mirrors Swift `reauthSwitchTeller`. */
    fun reauthSwitchTeller() {
        showReauth = false
        showCloseShift = true
    }

    // ── shift + routing ────────────────────────────────────────────────────────
    /** The screen to show — the core decides. Reading session/shift/branch here
     *  registers them so Compose recomposes the route when they change. */
    val route: AppRoute
        get() {
            @Suppress("UNUSED_EXPRESSION")
            run { session; shift; deviceConfig }
            return core.appRoute()
        }

    // ── device config (owned by the CORE store; the host only mirrors + writes) ──
    /** Re-read the device binding from the core into the mirrored state. */
    fun refreshDeviceConfig() {
        val c = core.deviceConfig()
        deviceConfig = c
        branchId = c.branchId ?: ""
        branchName = c.branchName ?: ""
        reconfiguring = c.reconfiguring
        printerHost = printerAddress(c)
        printerBrand = if (c.printerBrand == "star") PrinterBrand.STAR else PrinterBrand.EPSON
        lanHub = c.lanHub ?: ""
    }
    /** Persist this device's printer (Settings). Splits "host:port"; maps the brand. */
    fun setDevicePrinter(host: String, brand: PrinterBrand) {
        val (h, p) = parsePrinter(host)
        runCatching { core.setDevicePrinter(h.ifBlank { null }, p, if (brand == PrinterBrand.STAR) "star" else "epson") }
        refreshDeviceConfig()
    }
    /** Persist a manual LAN hub address (Settings → LAN relay). Empty clears it; the
     *  core registers it live if the relay is already running. The `@JvmName` avoids
     *  a JVM-signature clash with the `lanHub` property's generated `setLanHub`. */
    @JvmName("applyLanHub")
    fun setLanHub(value: String) {
        runCatching { core.setDeviceLanHub(value.ifBlank { null }) }
        refreshDeviceConfig()
    }
    /** Whether the LAN relay task is currently running (Settings diagnostics row). */
    val lanRelayActive: Boolean get() = core.lanActive()
    /** Number of LAN peers currently discovered (Settings diagnostics row). */
    val lanPeerCount: Int get() = core.lanPeerCount().toInt()
    /** Bind this device's till (drawer); null = the branch default. */
    fun setDeviceTill(tillId: String?) { runCatching { core.setDeviceTill(tillId) }; refreshDeviceConfig() }
    /** Bind this device's kitchen station (KDS devices). */
    fun setDeviceStation(stationId: String?) { runCatching { core.setDeviceStation(stationId) }; refreshDeviceConfig() }
    /** Whether THIS device's session role is kitchen / waiter. */
    val isKitchenDevice: Boolean get() = session?.role == "kitchen"
    val isWaiterDevice: Boolean get() = session?.role == "waiter"

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
    /** True while a manual "sync data" pull is running — drives the top-bar
     *  button's spinner + disabled state. */
    var isSyncingData by mutableStateOf(false)
        private set

    /** Load the branch-effective catalog: pull a fresh copy when online (best
     *  effort), then read the local mirror (always succeeds, even offline). */
    suspend fun loadCatalog() {
        if (session?.online == true) runCatching { core.refreshCatalog() }
        reprojectCatalog()
        loadCart()
        refreshPending()
    }

    /** Manual "sync server data" — re-pull the branch-effective catalog (menu,
     *  categories, add-ons, bundles, payment methods, discounts) on demand,
     *  surfacing real success/failure (unlike the best-effort loadCatalog).
     *  Mirrors Flutter's top-bar refresh button. Offline is a no-op with a hint;
     *  concurrent taps are ignored. */
    suspend fun refreshServerData() {
        if (isSyncingData) return
        isSyncingData = true
        try {
            // Ping fresh: the cached `online` flag goes stale (an offline unlock
            // leaves it false, and it only flips on the heartbeat), so gating on it
            // falsely reported "offline" on tap. A live ping is the truth on tap.
            val online = core.refreshConnectivity()
            refreshPending()
            if (!online) {
                showToast(t("chrome.offline_banner"), ChipTone.WARNING, icon = "wifi.slash")
                return
            }
            core.refreshCatalog() // also re-pulls branch + org logo URL into kv
            reprojectCatalog()
            loadCart()
            refreshPending()
            adoptOrgLogoFromCore() // pick up a changed logo URL (Coil re-fetches on display)
            showToast(t("chrome.sync_done"), ChipTone.SUCCESS, icon = "checkmark.circle")
        } catch (e: Exception) {
            showToast(t("chrome.sync_failed"), ChipTone.DANGER, icon = "exclamationmark.triangle")
        } finally {
            isSyncingData = false
        }
    }

    /** Adopt the org logo URL the core just persisted (durable kv, refreshed by the
     *  catalog sync's branch re-pull), so a logo changed on the dashboard shows up
     *  after a manual sync. Coil re-fetches the bytes when the URL changes. */
    private fun adoptOrgLogoFromCore() {
        val logo = core.orgLogoUrl()
        if (!logo.isNullOrBlank() && logo != orgLogoUrl) {
            orgLogoUrl = logo
            vault.orgLogoUrl = logo
        }
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
            // Auto-print the receipt on checkout — the receipt sheet's Print button
            // is for REPRINTS. `printReceipt` no-ops with no printer configured and
            // swallows its own errors (sets PrintState.FAILED), so it can never fail
            // the placed order.
            printReceipt()
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
        // Device printer now lives in the CORE store; keep the current brand.
        setDevicePrinter(value, printerBrand)
    }

    /** Set the printer dialect (Settings); persisted in the core device config. */
    @JvmName("applyPrinterBrand")
    fun setPrinterBrand(value: PrinterBrand) {
        setDevicePrinter(printerHost, value)
    }

    /** Set this till's device code (Settings); the core persists it and is the
     *  source of truth, so re-read it back rather than echo the input. */
    @JvmName("applyDeviceCode")
    fun setDeviceCode(code: String) {
        core.setDeviceCode(code)
        deviceCode = core.deviceCode()
    }

    /** Render the current receipt in the core and stream it to the configured
     *  network printer (best-effort; unverifiable without hardware). All the
     *  layout/bytes live in the core — this only moves them onto the wire. */
    suspend fun printReceipt(kickDrawer: Boolean = true) {
        val r = receipt ?: return
        val (host, port) = parsePrinter(printerHost)
        if (host.isBlank()) { printState = PrintState.NO_PRINTER; return }
        printState = PrintState.PRINTING
        val bytes = core.renderReceipt(r, branchName, session?.currencyCode ?: "", 32u, printerBrand)
        printState = try {
            core.sendToPrinter(host, port, bytes)
            // Pop the till on a cash sale — only on the original auto-print, not on
            // reprints (a reprint passes kickDrawer = false).
            if (kickDrawer && r.isCash) runCatching { core.sendToPrinter(host, port, core.cashDrawerKick(printerBrand)) }
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
        val bytes = core.renderShiftReport(report, branchName, session?.currencyCode ?: "", 32u, printerBrand)
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
    /** Outbox parked on a 401 — the host prompts a re-login to resume syncing. */
    var syncAuthPaused by mutableStateOf(false)
        private set

    /** Refresh the sync chrome signals (chip counts + online) in one local read. */
    fun refreshPending() {
        runCatching { core.syncStatus() }.getOrNull()?.let {
            pendingCount = it.pending.toInt()
            syncFailed = it.failed.toInt()
            isOnline = it.online
            syncAuthPaused = it.authPaused
        }
    }
    /** Connectivity heartbeat — ping (updates online + skew + drains), then
     *  re-read the chrome. Called on appear + on a 15s timer by the order screen. */
    suspend fun refreshConnectivity() {
        if (session == null) return
        val wasOnline = isOnline
        core.refreshConnectivity()
        clockSkewMinutes = core.clockSkewMinutes()
        refreshPending()
        // On an offline→online transition, re-adopt the server's authoritative
        // shift (the core drained the backlog during the ping). Prevents a teller
        // who opened/closed offline from being stranded on the wrong route once
        // the network returns. Mirrors the Swift refreshConnectivity.
        if (!wasOnline && isOnline) {
            reconcileShift()
            startLanRelay() // the bundle (LAN secret) may have just synced
        }
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
        val bytes = core.renderOrderReceipt(id, branchName, session?.currencyCode ?: "", 32u, printerBrand)
        printState = try {
            core.sendToPrinter(host, port, bytes)
            PrintState.PRINTED
        } catch (e: Exception) {
            PrintState.FAILED
        }
    }

    // ── receipt preview (history reprint) ─────────────────────────────────────
    /** A past order projected to a ReceiptView, driving the preview sheet. */
    var previewReceipt by mutableStateOf<ReceiptView?>(null)
    /** Fetch + project a synced order so the teller can preview before reprinting. */
    suspend fun openOrderReceiptPreview(orderId: String) {
        previewReceipt = runCatching { core.orderReceiptView(orderId) }.getOrNull()
    }
    /** Print an arbitrary ReceiptView (the preview sheet's Print). Toast-driven. */
    suspend fun printReceiptView(r: ReceiptView) {
        val (host, port) = parsePrinter(printerHost)
        if (host.isBlank()) { showToast(t("receipt.no_printer"), ChipTone.WARNING, icon = "exclamationmark.triangle"); return }
        val bytes = core.renderReceipt(r, branchName, session?.currencyCode ?: "", 32u, printerBrand)
        try {
            core.sendToPrinter(host, port, bytes)
            if (r.isCash) runCatching { core.sendToPrinter(host, port, core.cashDrawerKick(printerBrand)) }
            showToast(t("receipt.printed"), ChipTone.SUCCESS, icon = "checkmark.circle")
        } catch (e: Exception) {
            showToast(t("receipt.print_failed"), ChipTone.DANGER, icon = "xmark.circle")
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

    /** Drives the mid-shift Z-report preview sheet (print without closing). */
    var showReportPreview by mutableStateOf(false)
    /** Open the mid-shift report preview: reset stale print state, then show it. */
    fun openShiftReportPreview() {
        printState = PrintState.IDLE
        showReportPreview = true
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

    /** A past shift's synced orders, lazily loaded on row expansion + cached. */
    var shiftOrders by mutableStateOf<Map<String, List<OrderSummaryView>>>(emptyMap())
        private set
    var loadingShiftOrders by mutableStateOf<Set<String>>(emptySet())
        private set
    suspend fun loadOrdersForShift(shiftId: String) {
        if (shiftOrders.containsKey(shiftId)) return
        loadingShiftOrders = loadingShiftOrders + shiftId
        val orders = runCatching { core.listOrdersForShift(shiftId) }.getOrDefault(emptyList())
        shiftOrders = shiftOrders + (shiftId to orders)
        loadingShiftOrders = loadingShiftOrders - shiftId
    }
    /** Fetch + print a PAST shift's Z-report (history per-row print). Toast-driven. */
    suspend fun reprintShiftReport(shiftId: String) {
        val (host, port) = parsePrinter(printerHost)
        if (host.isBlank()) { showToast(t("receipt.no_printer"), ChipTone.WARNING, icon = "exclamationmark.triangle"); return }
        try {
            val report = core.shiftReportFor(shiftId)
            val bytes = core.renderShiftReport(report, branchName, session?.currencyCode ?: "", 32u, printerBrand)
            core.sendToPrinter(host, port, bytes)
            showToast(t("receipt.printed"), ChipTone.SUCCESS, icon = "checkmark.circle")
        } catch (e: Exception) {
            showToast(t("receipt.print_failed"), ChipTone.DANGER, icon = "xmark.circle")
        }
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
        showToast("${t("order.removed")} ${line.name}", ChipTone.NEUTRAL, t("order.undo"), { undoRemoveCartLine() }, 4.0, icon = "trash")
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

    /** Tab-style switch to a held order: park the current cart first (if any) so
     *  nothing is lost, then load the selected held order into the cart. */
    fun switchToHeldOrder(id: String) {
        if (cartLines.isNotEmpty()) {
            val name = java.time.LocalTime.now().toString().take(5)
            runCatching { core.holdCart(name) }
        }
        restoreDraft(id)
    }

    // ── delivery orders (online; teller works the live branch queue) ──────────────
    var deliveryOrders by mutableStateOf<List<DeliveryOrderView>>(emptyList())
        private set
    var isLoadingDelivery by mutableStateOf(false)
        private set
    var deliveryActiveOnly by mutableStateOf(true)

    private val activeStatusFilter = "received,confirmed,preparing,ready,out_for_delivery"

    /** The branch's delivery accepting settings (per-channel auto/open/closed). */
    var deliverySettings by mutableStateOf<DeliverySettingsView?>(null)
        private set

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
        deliverySettings = runCatching { core.deliverySettings() }.getOrNull()
    }
    /** Cycle a channel's accepting override: auto → open → closed → auto. */
    suspend fun cycleAccepting(channel: String, current: String) {
        val next = if (current == "auto") "open" else if (current == "open") "closed" else "auto"
        isBusy = true; error = null
        try { deliverySettings = core.deliverySetAccepting(channel, next) }
        catch (e: CoreException) { error = humanMessage(e) }
        finally { isBusy = false }
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
            val ref = res.orderRef?.let { " · $it" } ?: ""
            // Surface oversold warnings instead of dropping them: replaying the
            // frozen delivery snapshot into a real sale can oversell stock, and the
            // teller must SEE that (was silently discarded — res.warnings ignored).
            if (res.warnings.isNotEmpty()) {
                showToast(t("delivery.finalized") + ref + " — " + res.warnings.joinToString("; "), ChipTone.WARNING, icon = "exclamationmark.triangle")
            } else {
                showToast(t("delivery.finalized") + ref, ChipTone.SUCCESS, icon = "checkmark.circle")
            }
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
        // The device binding is written to the CORE store (clears reconfiguring).
        runCatching { core.setDeviceBranch(branch.id, branch.name) }
        refreshDeviceConfig()
        orgLogoUrl = branch.orgLogoUrl?.takeIf { it.isNotBlank() }
        vault.orgLogoUrl = orgLogoUrl
        runCatching { core.logout(false) }
        session = null
        setupPhase = SetupPhase.CREDENTIALS
        branches = emptyList()
        error = null
    }

    fun beginReconfigure() {
        runCatching { core.startReconfigure() }; refreshDeviceConfig()
        setupPhase = SetupPhase.CREDENTIALS; branches = emptyList(); error = null
    }
    fun cancelReconfigure() {
        // Re-confirm the existing branch to drop the reconfigure flag.
        deviceConfig.branchId?.let { runCatching { core.setDeviceBranch(it, deviceConfig.branchName) } }
        refreshDeviceConfig()
        setupPhase = SetupPhase.CREDENTIALS; branches = emptyList(); error = null
        runCatching { core.logout(false) }; session = null
    }

    // ── realtime bus (ONE SSE per device) ────────────────────────────────────────
    /** Live connection state from the core's EventListener (reconnect banner). */
    var realtimeConnected by mutableStateOf(false)
        private set
    /** Monotonic ticks bumped per topic event — screens reload via LaunchedEffect.
     *  Compose snapshot state is thread-safe, so the bridge writes these directly. */
    var kitchenTick by mutableStateOf(0); private set
    var ticketTick by mutableStateOf(0); private set
    var deliveryTick by mutableStateOf(0); private set
    private var realtimeBridge: RealtimeBridge? = null

    // ── in-app realtime alert banner (the visual companion to the OS notification) ──
    /** The active in-app alerts (empty = none), newest first. Rendered at the app
     *  root as a persistent iOS-style stack, alongside the OS notification + ping +
     *  haptic. They stay until the teller dismisses each one. */
    var realtimeAlerts by mutableStateOf<List<RealtimeAlertData>>(emptyList())
        private set
    private var alertSeq = 0
    /** Raise an in-app alert (newest on top). Called from [bannerPlayer] on the
     *  core's thread; snapshot state is thread-safe so this is safe off the main
     *  thread. Dedups by tag (the core already dedups, but guard LAN+cloud re-delivery). */
    fun showRealtimeAlert(title: String, body: String, tag: String) {
        if (realtimeAlerts.any { it.tag == tag }) return
        alertSeq += 1
        realtimeAlerts = listOf(RealtimeAlertData(alertSeq, title, body, tag)) + realtimeAlerts
    }
    fun dismissRealtimeAlert(id: Int) { realtimeAlerts = realtimeAlerts.filterNot { it.id == id } }
    /** Tapping an alert opens the Orders surface on the relevant tab (delivery vs
     *  open-tickets) and clears that alert. */
    fun openOrdersFromAlert(alert: RealtimeAlertData) {
        incomingTab = if (alert.tag.startsWith("delivery")) 0 else 1
        showIncoming = true
        dismissRealtimeAlert(alert.id)
    }
    /** Wraps the injected platform [player] so an alert ALSO raises the in-app
     *  banner — fired at the SAME deduped point the core posts the OS notification,
     *  so the banner, chime, haptic and notification stay in lockstep. */
    private val bannerPlayer: RealtimePlayer by lazy {
        object : RealtimePlayer {
            override fun playPing() = player.playPing()
            override fun postNotification(title: String, body: String, tag: String) {
                player.postNotification(title, body, tag)
                showRealtimeAlert(title, body, tag)
            }
            override fun haptic() = player.haptic()
        }
    }

    /** Open the device's ONE session-level realtime subscription. The CORE owns the
     *  policy — it derives the topics from the signed-in role, refreshes the right
     *  board via the bridge (tick counters), and raises deduped, localized alerts via
     *  the injected [player] (ping + OS notification + haptic + in-app banner).
     *  Idempotent (the core no-ops if already running). Call after login / on boot. */
    suspend fun startRealtime() {
        if (session == null || deviceConfig.branchId == null) return
        val bridge = RealtimeBridge(this)
        realtimeBridge = bridge
        runCatching { core.startRealtime(bridge, bannerPlayer) }
    }
    /**
     * Bring up the device-level LAN offline relay (Phase E). Idempotent + self-
     * guarding: no-ops if not signed in or no LAN secret is cached yet. Called after
     * a session is established (login), on regaining connectivity (the bundle/secret
     * may have just synced), and when a realtime consumer screen opens, so a till
     * advertises its open shift to the LAN gate and KDS/waiter get the LAN fast path.
     * Torn down in [signOut].
     */
    suspend fun startLanRelay() { runCatching { core.lanStart() } }

    fun unsubscribeRealtime() {
        core.unsubscribeRealtime(); realtimeBridge = null; realtimeConnected = false
    }
    /** Bridge → model (may be off the main thread; snapshot state is thread-safe). */
    fun onRealtimeEvent(event: RealtimeEvent) {
        when {
            event.eventType.startsWith("kitchen.") -> kitchenTick++
            event.eventType.startsWith("ticket.") -> ticketTick++
            event.eventType.startsWith("delivery.") -> deliveryTick++
        }
    }
    fun onRealtimeConnection(connected: Boolean) { realtimeConnected = connected }

    // ── Kitchen Display (KDS) ─────────────────────────────────────────────────────
    var kdsTickets by mutableStateOf<List<KdsTicketView>>(emptyList()); private set
    var kdsStations by mutableStateOf<List<KdsStationView>>(emptyList()); private set

    suspend fun loadKds() {
        runCatching { core.kdsList(deviceConfig.stationId) }.getOrNull()?.let { kdsTickets = it }
    }
    suspend fun loadKdsStations() { kdsStations = runCatching { core.kdsListStations() }.getOrDefault(emptyList()) }
    suspend fun bumpKdsItem(itemId: String) {
        runCatching { core.kdsBump(itemId); loadKds() }
            .onFailure { if (it is CoreException) showToast(humanMessage(it), tone = ChipTone.DANGER) }
    }
    suspend fun unbumpKdsItem(itemId: String) {
        runCatching { core.kdsUnbump(itemId); loadKds() }
            .onFailure { if (it is CoreException) showToast(humanMessage(it), tone = ChipTone.DANGER) }
    }

    // ── waiter open tickets ───────────────────────────────────────────────────────
    var openTickets by mutableStateOf<List<TicketView>>(emptyList()); private set
    var activeTicketId by mutableStateOf<String?>(null)
    /** Unified teller "Orders" surface — delivery + waiter open-tickets to settle,
     *  two tabs, one entry, fed by the one SSE. Replaces the old separate delivery
     *  and settle-tickets screens. */
    var showIncoming by mutableStateOf(false)
    /** Which tab the unified Orders surface opens on (0 = delivery, 1 = tickets). */
    var incomingTab by mutableStateOf(0)
    /** Waiter's own open-tickets list (a sub-screen over the shared order screen). */
    var showTickets by mutableStateOf(false)

    suspend fun loadOpenTickets() {
        runCatching { core.listOpenTickets() }.getOrNull()?.let { openTickets = it }
    }
    /** Waiter checkout: fire the cart as a NEW ticket, or add it as a ROUND to the
     *  targeted `activeTicketId`. Clears the target on success. */
    suspend fun fireOrAddRound() {
        val ok = activeTicketId?.let { addRound(it) } ?: fireTicket()
        if (ok) activeTicketId = null
    }
    suspend fun fireTicket(customerName: String? = null): Boolean {
        isBusy = true; error = null
        return try {
            val fired = core.fireTicket(null, customerName, null, null)
            loadCart(); loadOpenTickets()
            showToast(t("waiter.fired") + if (fired.queuedOffline) " · " + t("waiter.queued") else "", tone = ChipTone.SUCCESS)
            true
        } catch (e: CoreException) { error = humanMessage(e); false } finally { isBusy = false }
    }
    suspend fun addRound(ticketId: String): Boolean {
        isBusy = true; error = null
        return try {
            core.addTicketRound(ticketId); loadCart(); loadOpenTickets()
            showToast(t("waiter.fired"), tone = ChipTone.SUCCESS); true
        } catch (e: CoreException) { error = humanMessage(e); false } finally { isBusy = false }
    }
    suspend fun voidTicket(ticketId: String, reason: String?) {
        runCatching { core.voidTicket(ticketId, reason); loadOpenTickets() }
            .onFailure { if (it is CoreException) showToast(humanMessage(it), tone = ChipTone.DANGER) }
    }
    suspend fun settleTicket(ticketId: String, paymentMethodId: String, amountTenderedMinor: Long? = null): Boolean {
        val shiftId = shift?.id ?: run { error = t("waiter.need_shift"); return false }
        isBusy = true; error = null
        return try {
            core.settleTicket(ticketId, shiftId, paymentMethodId, amountTenderedMinor, null, null, null, null, null)
            loadOpenTickets(); loadHistory()
            showToast(t("waiter.settled"), tone = ChipTone.SUCCESS); true
        } catch (e: CoreException) { error = humanMessage(e); false } finally { isBusy = false }
    }

    // ── Back navigation ───────────────────────────────────────────────────────────
    /** Whether a sub-screen / overlay is open — so the system back should CLOSE it
     *  rather than pop the Activity out of the app. */
    val hasOverlay: Boolean
        get() = showMore || showReauth || detailBundle != null || detailItem != null ||
            previewReceipt != null || showReportPreview || showSettings || showTickets ||
            showIncoming || showDrafts || showShiftHistory ||
            showCashMovements || showHistory || showSync || showCloseShift

    /** Close the topmost open overlay (in visual z-order). Returns true if it consumed
     *  the back. The host's BackHandler delegates here so back navigates WITHIN the app
     *  instead of exiting it. */
    fun goBack(): Boolean {
        when {
            showMore -> showMore = false
            showReauth -> showReauth = false
            detailBundle != null -> closeBundleDetail()
            detailItem != null -> closeItemDetail()
            previewReceipt != null -> previewReceipt = null
            showReportPreview -> showReportPreview = false
            showSettings -> showSettings = false
            showTickets -> showTickets = false
            showIncoming -> showIncoming = false
            showDrafts -> showDrafts = false
            showShiftHistory -> showShiftHistory = false
            showCashMovements -> showCashMovements = false
            showHistory -> showHistory = false
            showSync -> showSync = false
            showCloseShift -> showCloseShift = false
            else -> return false
        }
        return true
    }

    fun signOut() {
        unsubscribeRealtime()
        core.lanStop()
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

/** Reassemble "host:port" from the core's split printer config (the Settings field).
 *  Empty when no printer is bound. */
private fun printerAddress(c: DeviceConfigView): String {
    val host = c.printerHost?.takeIf { it.isNotBlank() } ?: return ""
    val port = c.printerPort
    return if (port != null && port != 9100.toUShort()) "$host:$port" else host
}

/** The core's realtime sink (ONE per device). The core calls these from its SSE
 *  task off the main thread; Compose snapshot state is thread-safe so the model
 *  writes it directly and screens reload via `LaunchedEffect(tick)`. */
class RealtimeBridge(private val model: AppModel) : EventListener {
    override fun onEvent(event: RealtimeEvent) = model.onRealtimeEvent(event)
    override fun onConnectionChanged(connected: Boolean) = model.onRealtimeConnection(connected)
}
