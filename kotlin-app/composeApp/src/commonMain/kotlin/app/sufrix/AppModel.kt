package app.sufrix

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.sufrix.core.AppRoute
import app.sufrix.core.BranchView
import app.sufrix.core.CartLineView
import app.sufrix.core.CartTotals
import app.sufrix.core.CategoryView
import app.sufrix.core.CoreException
import app.sufrix.core.LoginMode
import app.sufrix.core.LoginRequest
import app.sufrix.core.MenuItemView
import app.sufrix.core.OutboxItemView
import app.sufrix.core.PaymentMethodView
import app.sufrix.core.ReceiptView
import app.sufrix.core.SessionSnapshot
import app.sufrix.core.ShiftView
import app.sufrix.core.SufrixCore
import app.sufrix.core.TokenStore
import app.sufrix.ui.ThemeMode

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
}

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
    /** The device's current shift (drives OpenShift ↔ Order routing). */
    var shift by mutableStateOf<ShiftView?>(null)
        private set
    /** Branch-effective catalog (cached; reads always succeed offline). */
    var categories by mutableStateOf<List<CategoryView>>(emptyList())
        private set
    var menuItems by mutableStateOf<List<MenuItemView>>(emptyList())
        private set
    /** The in-progress cart (client-only, kv-persisted in the core). */
    var cartLines by mutableStateOf<List<CartLineView>>(emptyList())
        private set
    var cartTotals by mutableStateOf(CartTotals(0L, 0L, 0L, 0L))
        private set
    /** Org payment methods (cached) — the tender picker source. */
    var paymentMethods by mutableStateOf<List<PaymentMethodView>>(emptyList())
        private set
    /** The last placed order's receipt (drives the confirmation screen). */
    var receipt by mutableStateOf<ReceiptView?>(null)
        private set
    var isPlacingOrder by mutableStateOf(false)
        private set

    init {
        core.setTokenStore(vault)
        vault.loadBlob()?.let { session = core.restoreSession(it) }
        loadShift()
    }

    val isSignedIn: Boolean get() = session != null
    val isBranchConfigured: Boolean get() = branchId.isNotBlank()

    // ── localization ──────────────────────────────────────────────────────────
    fun t(key: String): String = core.tr(key)
    val isRTL: Boolean get() = core.isRtl()

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

    /** Open a shift with the counted opening cash (minor units). Works offline. */
    suspend fun openShift(openingCashMinor: Long) {
        isBusy = true; error = null
        try {
            shift = core.openShift(openingCashMinor)
        } catch (e: CoreException) {
            error = humanMessage(e)
        } catch (e: Exception) {
            error = e.message ?: core.tr("err.generic")
        } finally {
            isBusy = false
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
        categories = runCatching { core.listCategories() }.getOrDefault(emptyList())
        menuItems = runCatching { core.listMenuItems() }.getOrDefault(emptyList())
        paymentMethods = runCatching { core.listPaymentMethods() }.getOrDefault(emptyList())
        loadCart()
        refreshPending()
    }

    // ── checkout ───────────────────────────────────────────────────────────────
    /** Place the cart as an order via the core (online or queued offline). On
     *  success the core has emptied the cart; reload it and surface the receipt. */
    suspend fun placeOrder(paymentMethodId: String, amountTenderedMinor: Long) {
        isPlacingOrder = true; error = null
        try {
            receipt = core.checkout(paymentMethodId, amountTenderedMinor)
            loadCart()
            refreshPending()
        } catch (e: CoreException) {
            error = humanMessage(e)
        } catch (e: Exception) {
            error = e.message ?: core.tr("err.generic")
        } finally {
            isPlacingOrder = false
        }
    }

    /** Dismiss the receipt confirmation (back to the catalog). */
    fun dismissReceipt() { receipt = null }

    // ── sync center (outbox) ─────────────────────────────────────────────────────
    var showSync by mutableStateOf(false)
    var outbox by mutableStateOf<List<OutboxItemView>>(emptyList())
        private set
    /** Queued/in-flight command count — the sync chip badge. */
    var pendingCount by mutableStateOf(0)
        private set

    fun refreshPending() {
        pendingCount = runCatching { core.pendingOutboxCount().toInt() }.getOrDefault(0)
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

    // ── close shift ────────────────────────────────────────────────────────────
    /** Drives the close-shift screen (shown over the order screen). */
    var showCloseShift by mutableStateOf(false)

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

    // ── cart ───────────────────────────────────────────────────────────────────
    /** Add one unit of [item]. Sync (the core just touches kv) so the tap feels
     *  instant; the core merges into the matching line. */
    fun addToCart(item: MenuItemView) = applyCart { core.cartAdd(item.id, item.name, item.basePriceMinor) }
    fun setCartQty(itemId: String, qty: Long) = applyCart { core.cartSetQty(itemId, qty) }
    fun removeCartLine(itemId: String) = applyCart { core.cartRemove(itemId) }
    fun clearCart() {
        runCatching { core.cartClear() }
        cartLines = emptyList()
        refreshCartTotals()
    }

    private fun loadCart() {
        cartLines = runCatching { core.cartLines() }.getOrDefault(emptyList())
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
        cartTotals = runCatching { core.cartTotals() }.getOrDefault(CartTotals(0L, 0L, 0L, 0L))
    }

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
        cartTotals = CartTotals(0L, 0L, 0L, 0L)
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
