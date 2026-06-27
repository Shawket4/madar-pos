// The host's single source of UI state. Owns the one `MadarCore` handle and
// the secure vault, mirrors the core's session into `@Published` state, and
// forwards sign-in/out. NO business logic — the online↔offline decision, token
// custody, and validation all live in the core.
import CoreText
import Foundation
import SwiftUI

/// Device-setup is two steps: a manager authenticates, then picks the branch.
enum SetupPhase { case credentials, pickBranch }

/// Receipt-printing progress for the confirmation screen's Print button.
enum PrintState: Equatable { case idle, printing, printed, failed, noPrinter }

@MainActor
final class AppModel: ObservableObject {
    let core: MadarCore
    private let vault = KeychainTokenStore()

    /// The active session, or `nil` when signed out. Drives the root route.
    @Published private(set) var session: SessionSnapshot?
    @Published private(set) var isBusy = false
    @Published var errorMessage: String?

    /// The device's configured branch (set once at provisioning). PIN login
    /// derives the org from it; post-D13 any active org teller may sign in here.
    @Published private(set) var branchId: String
    @Published private(set) var branchName: String
    /// The org's logo URL for this branch (from `BranchView.orgLogoUrl`), shown on
    /// the receipt header. Persisted at branch selection so the receipt brand mark
    /// survives restarts and renders offline (with a bundled asset fallback).
    @Published private(set) var orgLogoUrl: String?

    /// Forces the device-setup (manager) view even on a configured device.
    @Published private(set) var reconfiguring = false
    @Published private(set) var setupPhase: SetupPhase = .credentials
    /// Branches fetched after the manager authenticates (the picker source).
    @Published private(set) var branches: [BranchView] = []
    /// The device's current shift (drives OpenShift ↔ Order routing).
    @Published private(set) var shift: ShiftView?
    /// Carried-over opening-cash suggestion (previous declared closing, minor
    /// units; 0 = none). Prefills the open-shift count field.
    @Published private(set) var suggestedOpeningCashMinor: Int64 = 0
    /// Branch-effective catalog (cached; reads always succeed offline).
    @Published private(set) var categories: [CategoryView] = []
    @Published private(set) var menuItems: [MenuItemView] = []
    /// The in-progress cart (client-only, kv-persisted in the core).
    @Published private(set) var cartLines: [CartLineView] = []
    @Published private(set) var cartTotals: CartTotals = .zero
    /// Org payment methods (cached) — the tender picker source.
    @Published private(set) var paymentMethods: [PaymentMethodView] = []
    /// Org discounts (cached) — the tender discount picker source.
    @Published private(set) var discounts: [DiscountView] = []
    /// The cart's selected discount id (nil = none).
    @Published private(set) var cartDiscountId: String?
    /// The last placed order's receipt (drives the confirmation screen).
    @Published private(set) var receipt: ReceiptView?
    @Published private(set) var isPlacingOrder = false

    // ── transient toast / snackbar ──────────────────────────────────────────
    /// The active toast (nil = none). Rendered by `.toastHost(app)`.
    @Published private(set) var toast: ToastData?
    private var toastAction: (() -> Void)?
    private var toastSeq = 0
    private var toastTask: Task<Void, Never>?

    /// Flash a transient message at the bottom of the screen. Optionally with one
    /// action (e.g. "Undo"); auto-dismisses after `seconds`.
    func showToast(
        _ text: String,
        icon: String? = nil,
        tone: ChipTone = .neutral,
        actionLabel: String? = nil,
        action: (() -> Void)? = nil,
        seconds: Double = 2.6
    ) {
        toastSeq += 1
        let id = toastSeq
        toastAction = action
        withAnimation(Motion.standard) {
            toast = ToastData(id: id, text: text, icon: icon, tone: tone, actionLabel: actionLabel)
        }
        toastTask?.cancel()
        toastTask = Task { @MainActor in
            try? await Task.sleep(nanoseconds: UInt64(seconds * 1_000_000_000))
            if toast?.id == id { withAnimation(Motion.standard) { toast = nil } }
        }
    }

    /// Invoke the active toast's action and dismiss it.
    func runToastAction() {
        let action = toastAction
        toastAction = nil
        toastTask?.cancel()
        withAnimation(Motion.standard) { toast = nil }
        action?()
    }
    /// Theme preference — defaults to light (the original navy palette).
    @Published var themeMode: ThemeMode {
        didSet { UserDefaults.standard.set(themeMode.rawValue, forKey: Self.themeKey) }
    }
    /// Active UI locale (en/ar). Changing it re-resolves strings + RTL in the core
    /// and triggers a re-render (this is @Published).
    @Published var locale: String {
        didSet {
            core.setLocale(locale: locale)
            UserDefaults.standard.set(locale, forKey: Self.localeKey)
            // The cached catalog views were projected under the OLD locale; re-read
            // them (offline, from the mirror) so item/category/payment labels switch
            // language immediately — `*_translations` re-resolve on this read.
            reprojectCatalog()
        }
    }
    /// Drives the settings screen (presented over the order screen).
    @Published var showSettings = false
    /// Drives the "More" overflow drawer (secondary nav-hub actions).
    @Published var showMore = false
    /// The device binding (branch / till / station / printer), owned by the CORE
    /// store. Mirrored here for the UI; mutate it only via `setDevice*` methods.
    @Published private(set) var deviceConfig: DeviceConfigView
    /// Network printer address ("host" or "host:port"; default port 9100). Empty
    /// = no printer configured. Mirror of the core's printer config; set in Settings
    /// via `setDevicePrinter`.
    @Published private(set) var printerHost: String
    /// Printer command dialect — Epson (ESC/POS) vs Star (Star Line Mode). The two
    /// are not byte-compatible; mirror of the core's `printer_brand`.
    @Published private(set) var printerBrand: PrinterBrand
    /// Manual LAN-relay hub address ("host" or "host:port") — an optional fixed peer
    /// used when mDNS auto-discovery can't find devices on this Wi-Fi. Empty = none.
    /// Mirror of the core's `lan_hub`; set in Settings via `setDeviceLanHub`.
    @Published private(set) var lanHub: String
    /// This device/till's short code — the `<DEVICE>` segment of every order_ref
    /// (e.g. T1/W2/K1). Owned + persisted by the core (auto-generates a stable
    /// default on first read); set in Settings, sanitized to short A-Z0-9.
    @Published private(set) var deviceCode: String
    /// Print progress for the receipt confirmation's Print button.
    @Published private(set) var printState: PrintState = .idle

    init() {
        Self.registerFonts()
        var cfg = defaultConfig()
        cfg.dbPath = Self.databasePath()
        cfg.locale = Locale.current.identifier
        // A failed store open is unrecoverable — fail loudly rather than limp on.
        core = try! MadarCore(config: cfg)
        // Device binding lives in the CORE store now (not UserDefaults). Read it
        // once to seed the @Published mirrors; `refreshDeviceConfig()` keeps them
        // current after each mutation.
        let dc = core.deviceConfig()
        deviceConfig = dc
        branchId = dc.branchId ?? ""
        branchName = dc.branchName ?? ""
        reconfiguring = dc.reconfiguring
        printerHost = Self.printerAddress(dc)
        printerBrand = (dc.printerBrand == "star") ? .star : .epson
        lanHub = dc.lanHub ?? ""
        // Prefer the CORE's durable logo URL (persisted in kv from get_branch,
        // refreshed on every data sync) over the host's last-known pref; fall back
        // to the pref before the first sync. `core` is already assigned above.
        let logoStr = core.orgLogoUrl()
            ?? UserDefaults.standard.string(forKey: Self.orgLogoKey).flatMap { $0.isEmpty ? nil : $0 }
        orgLogoUrl = logoStr
        if let s = logoStr { UserDefaults.standard.set(s, forKey: Self.orgLogoKey) }
        // Warm the logo for offline receipts (best-effort; no-op offline). Uses the
        // LOCAL value + a static — calling an instance method or reading `self`
        // here would touch `self` before all stored properties are initialized.
        if let s = logoStr, let url = URL(string: s) { ImageStore.shared.prefetch(url) }
        themeMode = ThemeMode(rawValue: UserDefaults.standard.string(forKey: Self.themeKey) ?? "") ?? .light
        deviceCode = core.deviceCode()
        // Apply the saved locale to the core before any string resolves.
        let savedLocale = UserDefaults.standard.string(forKey: Self.localeKey)
        if let savedLocale { core.setLocale(locale: savedLocale) }
        locale = savedLocale ?? core.locale()

        core.setTokenStore(store: vault)
        if let blob = vault.loadBlob() {
            session = core.restoreSession(blob: blob)
            if session != nil { startRealtime(); startLanRelay() }
        }
        loadShift()
    }

    var isSignedIn: Bool { session != nil }
    /// The screen to show — the core decides ENTIRELY from its own state (device
    /// binding now lives in the core store, not the host). Re-evaluates whenever the
    /// observed @Published state (session, shift, deviceConfig) changes.
    var route: AppRoute { core.appRoute() }
    /// Till bound to a branch → teller PIN login; until then, manager device-setup.
    var isBranchConfigured: Bool { deviceConfig.branchId != nil }

    // ── device config (owned by the CORE store; the host only mirrors + writes) ──

    /// Re-read the device binding from the core into the @Published mirrors so the
    /// UI updates. Called at init and after every `setDevice*` mutation.
    func refreshDeviceConfig() {
        let c = core.deviceConfig()
        deviceConfig = c
        branchId = c.branchId ?? ""
        branchName = c.branchName ?? ""
        reconfiguring = c.reconfiguring
        printerHost = Self.printerAddress(c)
        printerBrand = (c.printerBrand == "star") ? .star : .epson
        lanHub = c.lanHub ?? ""
    }

    /// Whether the LAN relay task is currently running (read live from the core for
    /// the Settings diagnostics row).
    var lanRelayActive: Bool { core.lanActive() }
    /// Number of LAN peers currently discovered (read live for the diagnostics row).
    var lanPeerCount: Int { Int(core.lanPeerCount()) }

    /// Persist a manual LAN hub address (Settings → LAN relay). Empty clears it; the
    /// core registers it immediately if the relay is already running.
    func setDeviceLanHub(_ value: String) {
        try? core.setDeviceLanHub(hub: value.isEmpty ? nil : value)
        refreshDeviceConfig()
    }

    /// Reassemble "host:port" from the core's split printer config (for display +
    /// the Settings field). Empty when no printer is bound.
    static func printerAddress(_ c: DeviceConfigView) -> String {
        guard let host = c.printerHost, !host.isEmpty else { return "" }
        if let port = c.printerPort, port != 9100 { return "\(host):\(port)" }
        return host
    }

    /// Persist this device's printer (Settings → Save). Splits "host:port" and maps
    /// the brand, writing through to the core store, then re-mirrors.
    func setDevicePrinter(host rawHost: String, brand: PrinterBrand) {
        let (host, port) = Self.parsePrinter(rawHost)
        try? core.setDevicePrinter(
            host: host.isEmpty ? nil : host,
            port: port,
            brand: brand == .star ? "star" : "epson")
        refreshDeviceConfig()
    }

    /// Bind this device's till (drawer) — Settings → Till. `nil` = branch default.
    func setDeviceTill(_ tillId: String?) {
        try? core.setDeviceTill(tillId: tillId)
        refreshDeviceConfig()
    }

    /// Bind this device's kitchen station — Settings → Station (KDS devices).
    func setDeviceStation(_ stationId: String?) {
        try? core.setDeviceStation(stationId: stationId)
        refreshDeviceConfig()
    }

    // ── teller ────────────────────────────────────────────────────────────────

    /// Teller sign-in (name + PIN). The core decides online vs offline.
    func signInTeller(name: String, pin: String) async {
        isBusy = true; errorMessage = nil
        defer { isBusy = false }
        do {
            session = try await core.signIn(req: LoginRequest(
                mode: .pin, name: name, pin: pin, branchId: branchId,
                email: nil, password: nil, orgId: nil))
            await reconcileShift()
            startRealtime(); startLanRelay()
        } catch {
            errorMessage = humanMessage(error)
        }
    }

    /// Token expired mid-shift (`syncAuthPaused`): the queued backlog is parked
    /// until a fresh login. This re-authenticates the SAME teller who owns the
    /// open shift (no handover) — `login` then un-parks the queue and drains it.
    @Published var showReauth = false

    func reauth(pin: String) async {
        guard let name = session?.displayName else { return }
        await signInTeller(name: name, pin: pin)
        guard errorMessage == nil else { return }
        showReauth = false
        refreshPending()
        // `login` already drained the backlog on success (drain-before-reconcile);
        // just reflect it.
        showToast(t("chrome.sync_resumed"), icon: "checkmark.circle", tone: .success)
    }

    /// The "switch teller" escape hatch from the re-auth prompt: close the open
    /// shift, then routing falls through to the login screen for a new teller (the
    /// replay endpoint will flush the prior teller's backlog regardless).
    func reauthSwitchTeller() {
        showReauth = false
        showCloseShift = true
    }

    // ── shift ─────────────────────────────────────────────────────────────────
    /// `true` while a shift is OPEN — gates sign-out / device-reconfigure, which
    /// must wait until the drawer is closed and reconciled.
    var hasOpenShift: Bool { shift?.isOpen ?? false }

    /// Surface a guidance/validation message in the active screen's error slot.
    func flagError(_ message: String) { errorMessage = message }
    /// Clear the current error (on screen entry / next user action).
    func clearError() { errorMessage = nil }

    /// Open a shift with the counted opening cash (minor units). `editReason` is
    /// required by the UI only when the count deviates from the carried-over
    /// closing; the server re-derives the deviation authoritatively. The core
    /// writes locally + queues the command (works offline); routing flips to Order.
    func openShift(openingCashMinor: Int64, editReason: String? = nil) async {
        isBusy = true; errorMessage = nil
        defer { isBusy = false }
        do {
            shift = try await core.openShift(openingCashMinor: openingCashMinor, editReason: editReason)
        } catch {
            errorMessage = humanMessage(error)
        }
    }

    /// Prime the open-shift screen: refresh the server prefill when online (so the
    /// carried-over opening-cash suggestion is current), then read it from the
    /// core. Safe + cheap offline (reads the locally-cached suggestion).
    func loadOpenShiftPrefill() async {
        // Show the locally-cached suggestion instantly…
        suggestedOpeningCashMinor = (try? core.suggestedOpeningCashMinor()) ?? 0
        // …then refresh it from the server (last synced declared closing).
        if session?.online == true {
            _ = try? await core.refreshShift()
            suggestedOpeningCashMinor = (try? core.suggestedOpeningCashMinor()) ?? 0
        }
    }

    /// Reconcile the shift with the server when online (catches an existing open
    /// shift on login, and a dashboard force-close); use the local cache offline.
    /// Drives OpenShift ↔ Order routing.
    func reconcileShift() async {
        guard session?.online == true else { loadShift(); return }
        // Refresh from the server, but NEVER let a transient/network error nuke a
        // good local shift — that's what bounced the teller back to open-shift.
        // Only a successful refresh (the core's authoritative answer, which is
        // nil only on a real force-close) updates the shift.
        if let refreshed = try? await core.refreshShift() {
            shift = refreshed
        } else {
            loadShift()
        }
    }

    private func loadShift() {
        shift = (try? core.currentShift()) ?? nil
    }

    // ── catalog ─────────────────────────────────────────────────────────────────
    /// True while a manual "sync data" pull is running — drives the top-bar
    /// button's spinner + disabled state.
    @Published private(set) var isSyncingData = false

    /// Load the branch-effective catalog: pull a fresh copy when online (best
    /// effort), then read the local mirror (always succeeds, even offline).
    func loadCatalog() async {
        if session?.online == true {
            try? await core.refreshCatalog()
        }
        reprojectCatalog()
        loadCart()
        refreshPending()
    }

    /// Manual "sync server data" — re-pull the branch-effective catalog (menu,
    /// categories, add-ons, bundles, payment methods, discounts) on demand and
    /// re-warm the org-logo cache, surfacing real success/failure (unlike the
    /// best-effort `loadCatalog`). Mirrors Flutter's top-bar refresh button.
    /// Offline is a no-op with a hint; concurrent taps are ignored.
    func syncServerData() async {
        if isSyncingData { return }
        isSyncingData = true
        defer { isSyncingData = false }
        // Ping fresh: the cached `online` flag goes stale (an offline unlock leaves
        // it false, and it only flips on the heartbeat), so gating on it falsely
        // reported "offline" on tap. A live ping is the truth at the moment of sync.
        let online = await core.refreshConnectivity()
        refreshPending()
        guard online else {
            showToast(t("chrome.offline_banner"), icon: "wifi.slash", tone: .warning)
            return
        }
        do {
            try await core.refreshCatalog() // also re-pulls branch + org logo URL into kv
            reprojectCatalog()
            loadCart()
            refreshPending()
            adoptOrgLogoFromCore() // pick up a changed logo URL + re-warm the cache
            showToast(t("chrome.sync_done"), icon: "checkmark.circle", tone: .success)
        } catch {
            showToast(t("chrome.sync_failed"), icon: "exclamationmark.triangle", tone: .danger)
        }
    }

    /// Adopt the org logo URL the core just persisted (durable kv, refreshed by the
    /// catalog sync's branch re-pull) and re-warm the durable image cache, so a
    /// logo changed on the dashboard shows up after a manual sync.
    private func adoptOrgLogoFromCore() {
        if let logo = core.orgLogoUrl(), logo != orgLogoUrl {
            orgLogoUrl = logo
            UserDefaults.standard.set(logo, forKey: Self.orgLogoKey)
        }
        prefetchOrgLogo()
    }

    /// Re-read the catalog projections from the local mirror under the current
    /// locale (no network). Used by `loadCatalog` and on a locale change so the
    /// labels follow the language without a re-fetch.
    func reprojectCatalog() {
        categories = (try? core.listCategories()) ?? []
        menuItems = (try? core.listMenuItems()) ?? []
        paymentMethods = (try? core.listPaymentMethods()) ?? []
        discounts = (try? core.listDiscounts()) ?? []
        loadBundles()
    }

    /// Apply or clear the cart discount (re-reads totals so the UI updates).
    func setDiscount(_ id: String?) {
        if let id { _ = try? core.cartSetDiscount(discountId: id) } else { _ = try? core.cartClearDiscount() }
        cartDiscountId = (try? core.cartDiscountId()) ?? nil
        refreshCartTotals()
    }

    // ── checkout ────────────────────────────────────────────────────────────────
    /// Place the cart as an order via the core (online or queued offline). On
    /// success the core has emptied the cart; we reload it and surface the receipt.
    func placeOrder(
        paymentMethodId: String,
        amountTenderedMinor: Int64,
        tipMinor: Int64 = 0,
        tipPaymentMethodId: String? = nil,
        customerName: String? = nil,
        notes: String? = nil,
        splits: [CheckoutSplit] = []
    ) async {
        isPlacingOrder = true; errorMessage = nil
        defer { isPlacingOrder = false }
        do {
            let input = CheckoutInput(
                paymentMethodId: paymentMethodId,
                amountTenderedMinor: amountTenderedMinor,
                tipMinor: tipMinor,
                tipPaymentMethodId: tipPaymentMethodId,
                customerName: customerName,
                notes: notes,
                splits: splits
            )
            receipt = try await core.checkout(input: input)
            printState = .idle
            loadCart()
            refreshPending()
            // Refresh the stats pill in the background (non-blocking) so the
            // new sale shows without delaying the receipt confirmation.
            Task { await loadHistory() }
            // Auto-print the receipt on checkout — the receipt sheet's Print button
            // is for reprints. Non-blocking; `printCurrentReceipt` no-ops without a
            // printer and never throws, so it can't affect the placed order.
            Task { await printCurrentReceipt() }
        } catch {
            errorMessage = humanMessage(error)
        }
    }

    /// Dismiss the receipt confirmation (back to the catalog).
    func dismissReceipt() { receipt = nil; printState = .idle }

    /// Total quantity of an item already in the cart, summed across its config
    /// variants — drives the catalog card's in-cart badge.
    func cartQtyForItem(_ itemId: String) -> Int64 {
        cartLines.filter { $0.itemId == itemId }.reduce(0) { $0 + $1.qty }
    }

    /// Render the current receipt in the core and stream it to the configured
    /// network printer (best-effort; unverifiable without hardware). All the
    /// layout/bytes live in the core — this only moves them onto the wire.
    func printCurrentReceipt(kickDrawer: Bool = true) async {
        guard let receipt else { return }
        let (host, port) = Self.parsePrinter(printerHost)
        guard !host.isEmpty else { printState = .noPrinter; return }
        printState = .printing
        let bytes = core.renderReceipt(
            receipt: receipt,
            storeName: branchName,
            currency: session?.currencyCode ?? "",
            width: 32,
            brand: printerBrand
        )
        do {
            try await core.sendToPrinter(host: host, port: port, bytes: bytes)
            // Pop the till on a cash sale — only on the original auto-print, not on
            // reprints (a reprint passes kickDrawer: false).
            if kickDrawer, receipt.isCash {
                try? await core.sendToPrinter(host: host, port: port, bytes: core.cashDrawerKick(brand: printerBrand))
            }
            printState = .printed
        } catch {
            printState = .failed
        }
    }

    /// Print the shift report (Z-report) — same printer path as the receipt.
    func printShiftReport() async {
        guard let report = shiftReport else { return }
        let (host, port) = Self.parsePrinter(printerHost)
        guard !host.isEmpty else { printState = .noPrinter; return }
        printState = .printing
        let bytes = core.renderShiftReport(
            report: report,
            storeName: branchName,
            currency: session?.currencyCode ?? "",
            width: 32,
            brand: printerBrand
        )
        do {
            try await core.sendToPrinter(host: host, port: port, bytes: bytes)
            printState = .printed
        } catch {
            printState = .failed
        }
    }

    /// Set this device's till code (e.g. T1). The core sanitizes to short A-Z0-9
    /// and ignores blank; we mirror back whatever it kept.
    func setDeviceCode(_ code: String) {
        core.setDeviceCode(code: code)
        deviceCode = core.deviceCode()
    }

    /// Split "host" / "host:port" → (host, port); default JetDirect port 9100.
    private static func parsePrinter(_ raw: String) -> (String, UInt16) {
        let trimmed = raw.trimmingCharacters(in: .whitespaces)
        guard let colon = trimmed.lastIndex(of: ":") else { return (trimmed, 9100) }
        let host = String(trimmed[..<colon])
        let port = UInt16(trimmed[trimmed.index(after: colon)...]) ?? 9100
        return (host, port)
    }

    // ── sync center (outbox) ────────────────────────────────────────────────────
    @Published var showSync = false
    @Published private(set) var outbox: [OutboxItemView] = []
    /// Queued/in-flight command count — the sync chip badge.
    @Published private(set) var pendingCount: Int = 0
    /// Dead/stuck command count — the "needs attention" chip + danger badge.
    @Published private(set) var syncFailed: Int = 0
    /// Session connectivity — drives the offline banner + sync chip state.
    @Published private(set) var isOnline: Bool = true
    /// Server-vs-device clock skew in minutes (drives the clock-skew banner).
    @Published private(set) var clockSkewMinutes: Int = 0
    /// Outbox parked on a 401 — the host prompts a re-login to resume syncing.
    @Published private(set) var syncAuthPaused: Bool = false

    /// Refresh the sync chrome signals (chip counts + online) in one local read.
    func refreshPending() {
        if let s = try? core.syncStatus() {
            pendingCount = Int(s.pending)
            syncFailed = Int(s.failed)
            isOnline = s.online
            syncAuthPaused = s.authPaused
        }
    }
    /// The connectivity heartbeat — pings the server (updating online + clock
    /// skew + draining the outbox), then re-reads the chrome. Called on appear
    /// and on a timer by the order screen. Online-aware: no-op effect offline.
    func refreshConnectivity() async {
        guard session != nil else { return }
        let wasOnline = isOnline
        _ = await core.refreshConnectivity()
        clockSkewMinutes = Int(core.clockSkewMinutes())
        refreshPending() // updates `isOnline` from the fresh sync status
        // Just regained connectivity → re-check the server for an ACTIVE shift and
        // adopt it. Without this, a teller who signed in offline (or whose cache
        // was cleared) stays stranded on the open-shift screen and could open a
        // DUPLICATE shift even though one is already open server-side.
        if !wasOnline && isOnline {
            await reconcileShift()
            startRealtime(); startLanRelay() // the bundle (LAN secret) may have just synced
        }
    }
    /// Load the full outbox (for the sync sheet) + the count.
    func loadOutbox() {
        outbox = (try? core.listOutbox()) ?? []
        refreshPending()
    }
    /// Requeue every failed command and try to send now.
    func retryOutbox() async {
        try? await core.retryOutbox()
        loadOutbox()
    }
    /// Discard a single failed command.
    func discardOutboxItem(_ id: String) {
        _ = try? core.discardOutboxItem(id: id)
        loadOutbox()
    }

    // ── order history ───────────────────────────────────────────────────────────
    @Published var showHistory = false
    @Published private(set) var history: [OrderSummaryView] = []
    @Published private(set) var isLoadingHistory = false
    /// Live shift totals for the action-bar pill, derived from `history`.
    @Published private(set) var shiftSalesMinor: Int64 = 0
    @Published private(set) var shiftOrderCount: Int = 0

    /// Load the current shift's orders (synced + queued). Best-effort. Also
    /// refreshes the stats pill from the same list (voided excluded, in core).
    func loadHistory() async {
        isLoadingHistory = true
        defer { isLoadingHistory = false }
        history = (try? await core.listShiftOrders()) ?? []
        let stats = core.shiftStats(orders: history)
        shiftSalesMinor = stats.salesMinor
        shiftOrderCount = Int(stats.orderCount)
    }

    /// The expanded history row's fetched detail (lines + modifiers).
    @Published private(set) var orderDetail: OrderDetailView?

    /// Fetch a synced order's line detail for the expanded row.
    func loadOrderDetail(_ id: String) async {
        orderDetail = (try? await core.orderDetail(orderId: id))
    }
    /// Reprint a synced order's receipt to the configured printer.
    func reprintOrder(_ id: String) async {
        let (host, port) = Self.parsePrinter(printerHost)
        guard !host.isEmpty else { printState = .noPrinter; return }
        printState = .printing
        do {
            let bytes = try await core.renderOrderReceipt(
                orderId: id, storeName: branchName, currency: session?.currencyCode ?? "", width: 32, brand: printerBrand)
            try await core.sendToPrinter(host: host, port: port, bytes: bytes)
            printState = .printed
        } catch {
            printState = .failed
        }
    }

    // ── receipt preview (history reprint) ─────────────────────────────────────
    /// A past order projected to a ReceiptView, driving the preview sheet.
    @Published var previewReceipt: ReceiptView?
    /// Fetch + project a synced order so the teller can preview before reprinting.
    func openOrderReceiptPreview(_ orderId: String) async {
        previewReceipt = try? await core.orderReceiptView(orderId: orderId)
    }
    /// Print an arbitrary ReceiptView (the preview sheet's Print). Toast-driven.
    func printReceiptView(_ receipt: ReceiptView) async {
        let (host, port) = Self.parsePrinter(printerHost)
        guard !host.isEmpty else {
            showToast(t("receipt.no_printer"), icon: "exclamationmark.triangle", tone: .warning); return
        }
        let bytes = core.renderReceipt(
            receipt: receipt, storeName: branchName, currency: session?.currencyCode ?? "", width: 32, brand: printerBrand)
        do {
            try await core.sendToPrinter(host: host, port: port, bytes: bytes)
            if receipt.isCash {
                try? await core.sendToPrinter(host: host, port: port, bytes: core.cashDrawerKick(brand: printerBrand))
            }
            showToast(t("receipt.printed"), icon: "checkmark.circle", tone: .success)
        } catch {
            showToast(t("receipt.print_failed"), icon: "xmark.circle", tone: .danger)
        }
    }

    /// Void a synced order (queues offline). Reloads history on success so the
    /// row flips to Voided. Returns whether it succeeded (the sheet dismisses).
    func voidOrder(orderId: String, reason: String, note: String?, restoreInventory: Bool = true) async -> Bool {
        isBusy = true; errorMessage = nil
        defer { isBusy = false }
        do {
            try await core.voidOrder(orderId: orderId, reason: reason,
                                     note: note?.isEmpty == true ? nil : note,
                                     restoreInventory: restoreInventory)
            await loadHistory()
            refreshPending()
            return true
        } catch {
            errorMessage = humanMessage(error)
            return false
        }
    }

    // ── close shift ───────────────────────────────────────────────────────────
    /// Drives the close-shift screen (presented over the order screen).
    @Published var showCloseShift = false
    /// The current shift's report (expected cash + breakdown), loaded on close.
    @Published private(set) var shiftReport: ShiftReportView?

    /// Load the shift report (best-effort) for the close-shift system-cash row.
    func loadShiftReport() async {
        shiftReport = try? await core.shiftReport()
    }

    /// Drives the mid-shift Z-report preview sheet (print without closing).
    @Published var showReportPreview = false
    /// Open the mid-shift report preview: reset any stale print state, then show
    /// the sheet (which loads the report on appear).
    func openShiftReportPreview() {
        printState = .idle
        showReportPreview = true
    }

    /// Close the open shift with the counted cash + optional note. On success the
    /// core marks the shift closed, so the route flips back to open-shift.
    func closeShift(closingCashMinor: Int64, note: String?) async {
        isBusy = true; errorMessage = nil
        defer { isBusy = false }
        do {
            try await core.closeShift(closingCashMinor: closingCashMinor,
                                      cashNote: note?.isEmpty == true ? nil : note)
            loadShift()          // now closed → app_route flips to open-shift
            showCloseShift = false
        } catch {
            errorMessage = humanMessage(error)
        }
    }

    // ── cart ──────────────────────────────────────────────────────────────────
    /// Add one unit of `item`. Sync (the core just touches kv) so the tap feels
    /// instant; the core merges into the matching line.
    func addToCart(_ item: MenuItemView) {
        applyCart { try core.cartAdd(itemId: item.id, name: item.name, unitPriceMinor: item.basePriceMinor) }
    }
    func setCartQty(_ itemId: String, _ qty: Int64) {
        applyCart { try core.cartSetQty(itemId: itemId, qty: qty) }
    }
    func removeCartLine(_ itemId: String) {
        applyCart { try core.cartRemove(itemId: itemId) }
    }
    /// Swipe-to-delete: remove the whole line and offer an Undo toast that
    /// restores it (the core stashes the removed line).
    func swipeRemoveCartLine(_ line: CartLineView) {
        applyCart { try core.cartRemove(itemId: line.key) }
        Haptics.warning()
        showToast(
            "\(t("order.removed")) \(line.name)",
            icon: "trash",
            tone: .neutral,
            actionLabel: t("order.undo"),
            action: { [weak self] in self?.undoRemoveCartLine() },
            seconds: 4.0
        )
    }
    func undoRemoveCartLine() {
        applyCart { try core.cartRestoreRemoved() }
    }
    func clearCart() {
        try? core.cartClear()
        cartLines = []
        refreshCartTotals()
    }

    private func loadCart() {
        cartLines = (try? core.cartLines()) ?? []
        cartDiscountId = (try? core.cartDiscountId()) ?? nil
        refreshCartTotals()
    }
    /// Run a cart mutation that returns the new lines, then refresh totals.
    private func applyCart(_ op: () throws -> [CartLineView]) {
        guard let lines = try? op() else { return }
        cartLines = lines
        refreshCartTotals()
    }
    private func refreshCartTotals() {
        cartTotals = (try? core.cartTotals()) ?? .zero
    }

    // ── item customization ──────────────────────────────────────────────────────
    /// Non-nil = the customization sheet is open for this item.
    @Published var detailItem: MenuItemView?
    /// The cart line key being edited (nil = adding a new line).
    @Published var detailEditKey: String?
    /// The cart line being edited (seeds the sheet), nil when adding fresh.
    @Published var detailEditLine: CartLineView?
    /// The item's addons with charged prices resolved by the core (for the sheet).
    @Published private(set) var itemAddons: [ItemAddonView] = []

    /// Whether tapping `item` should open the customization sheet vs add directly.
    func hasOptions(_ item: MenuItemView) -> Bool {
        !item.sizes.isEmpty || !item.addonSlots.isEmpty || !item.optionalFields.isEmpty
    }

    func openItemDetail(_ item: MenuItemView, editKey: String? = nil, editLine: CartLineView? = nil) {
        detailEditKey = editKey
        detailEditLine = editLine
        itemAddons = (try? core.listItemAddons(itemId: item.id)) ?? []
        detailItem = item
    }
    /// Re-open the sheet for a configured cart line so the teller can change it.
    func editCartLine(_ line: CartLineView) {
        // Any cart line is editable — reopens the customization sheet seeded from
        // the line; addConfigured removes the old line (by its key) and re-adds.
        guard let item = menuItems.first(where: { $0.id == line.itemId }) else { return }
        openItemDetail(item, editKey: line.key, editLine: line)
    }
    func closeItemDetail() { detailItem = nil; detailEditKey = nil; detailEditLine = nil }

    /// Live recipe preview for the current selection — the core derives the
    /// effective ingredients (base by size, milk/coffee swaps, additive addons,
    /// optional contributions). Pure + cheap, so the sheet recomputes per toggle.
    func recipePreview(itemId: String, sizeLabel: String?, addons: [AddonSelection], optionalIds: [String]) -> [ComputedRecipeLineView] {
        (try? core.computeRecipe(itemId: itemId, sizeLabel: sizeLabel, addons: addons, optionalFieldIds: optionalIds)) ?? []
    }

    /// Add (or, in edit mode, replace) a configured line. The core resolves the
    /// charged prices from the catalog; we just pass the selection.
    func addConfigured(itemId: String, sizeLabel: String?, addons: [AddonSelection],
                       optionalIds: [String], qty: Int64, notes: String?) {
        if let key = detailEditKey { _ = try? core.cartRemove(itemId: key) }
        _ = try? core.cartAddConfigured(itemId: itemId, sizeLabel: sizeLabel, addons: addons,
                                        optionalFieldIds: optionalIds, qty: qty, notes: notes)
        loadCart()
        refreshPending()
        closeItemDetail()
    }

    // ── bundles / combos ─────────────────────────────────────────────────────────
    /// Available bundles (status active + within their date/time window) — the
    /// Combos section of the catalog.
    @Published private(set) var bundles: [BundleView] = []
    /// Non-nil = the bundle configuration sheet is open.
    @Published var detailBundle: BundleView?

    func loadBundles() {
        bundles = (try? core.availableBundles(nowRfc3339: Self.nowRFC3339())) ?? []
    }
    func openBundleDetail(_ b: BundleView) { detailBundle = b }
    func closeBundleDetail() { detailBundle = nil }

    /// Resolve a bundle component's `MenuItemView` and load its addons into
    /// `itemAddons` so the per-component sheet (ItemDetailView) can render them.
    func componentItem(_ itemId: String) -> MenuItemView? {
        guard let item = menuItems.first(where: { $0.id == itemId }) else { return nil }
        itemAddons = (try? core.listItemAddons(itemId: itemId)) ?? []
        return item
    }

    /// Add a configured bundle to the cart — the core resolves each component's
    /// charged extras and records one bundle line at the fixed bundle price.
    func addBundle(bundleId: String, components: [BundleComponentSelection]) {
        _ = try? core.cartAddBundle(bundleId: bundleId, components: components, qty: 1)
        loadCart()
        refreshPending()
        closeBundleDetail()
    }

    /// Local time as RFC3339 with a colon offset, so the core gates bundle
    // ── diagnostics (Settings → recent sync warnings) ────────────────────────────
    @Published private(set) var diagnostics: [DiagLogView] = []
    func loadDiagnostics() { diagnostics = core.recentLogs() }
    func clearDiagnostics() { core.clearLogs(); diagnostics = [] }

    /// windows in the till's timezone (the till sits at the branch).
    static func nowRFC3339() -> String {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withColonSeparatorInTimeZone]
        f.timeZone = .current
        return f.string(from: Date())
    }

    // ── cash movements + shift history (online) ───────────────────────────────────
    @Published var showCashMovements = false
    @Published var showShiftHistory = false
    @Published private(set) var cashMovements: [CashMovementView] = []
    @Published private(set) var shiftHistory: [ShiftSummaryView] = []
    @Published private(set) var isLoadingCash = false
    @Published private(set) var isLoadingShifts = false

    /// The open shift's cash movements (online read).
    func loadCashMovements() async {
        isLoadingCash = true; defer { isLoadingCash = false }
        cashMovements = (try? await core.listCashMovements()) ?? []
    }
    /// Record a pay-in (amount > 0) or pay-out (amount < 0). Reloads the list on
    /// success; surfaces the error otherwise. Returns whether it succeeded.
    func recordCashMovement(amountMinor: Int64, note: String) async -> Bool {
        isBusy = true; errorMessage = nil
        defer { isBusy = false }
        do {
            _ = try await core.recordCashMovement(amountMinor: amountMinor, note: note)
            await loadCashMovements()
            return true
        } catch {
            errorMessage = humanMessage(error)
            return false
        }
    }
    /// Past shifts for the branch (online read).
    func loadShiftHistory() async {
        isLoadingShifts = true; defer { isLoadingShifts = false }
        shiftHistory = (try? await core.listShifts()) ?? []
    }

    /// A past shift's synced orders, lazily loaded on row expansion + cached.
    @Published private(set) var shiftOrders: [String: [OrderSummaryView]] = [:]
    @Published private(set) var loadingShiftOrders: Set<String> = []
    func loadOrdersForShift(_ shiftId: String) async {
        guard shiftOrders[shiftId] == nil else { return }
        loadingShiftOrders.insert(shiftId)
        defer { loadingShiftOrders.remove(shiftId) }
        shiftOrders[shiftId] = (try? await core.listOrdersForShift(shiftId: shiftId)) ?? []
    }

    /// Fetch + print a PAST shift's Z-report (history per-row print). Toast-driven
    /// so several rows can print independently without a shared spinner.
    func reprintShiftReport(_ shiftId: String) async {
        let (host, port) = Self.parsePrinter(printerHost)
        guard !host.isEmpty else {
            showToast(t("receipt.no_printer"), icon: "exclamationmark.triangle", tone: .warning); return
        }
        do {
            let report = try await core.shiftReportFor(shiftId: shiftId)
            let bytes = core.renderShiftReport(
                report: report, storeName: branchName, currency: session?.currencyCode ?? "", width: 32, brand: printerBrand)
            try await core.sendToPrinter(host: host, port: port, bytes: bytes)
            showToast(t("receipt.printed"), icon: "checkmark.circle", tone: .success)
        } catch {
            showToast(t("receipt.print_failed"), icon: "xmark.circle", tone: .danger)
        }
    }

    // ── drafts / held orders ──────────────────────────────────────────────────────
    @Published var showDrafts = false
    @Published private(set) var drafts: [DraftView] = []

    func loadDrafts() { drafts = (try? core.listDrafts()) ?? [] }
    /// Park the current cart as a held order, auto-named by time of day.
    func holdCart() {
        let f = DateFormatter(); f.dateFormat = "HH:mm"
        _ = try? core.holdCart(name: f.string(from: Date()))
        loadCart(); loadDrafts()
    }
    func restoreDraft(_ id: String) {
        cartLines = (try? core.restoreDraft(id: id)) ?? cartLines
        cartDiscountId = (try? core.cartDiscountId()) ?? nil
        refreshCartTotals(); loadDrafts()
    }
    /// Tab-style switch to a held order: park the current cart first (if any) so
    /// nothing is lost, then load the selected held order into the cart.
    func switchToHeldOrder(_ id: String) {
        if !cartLines.isEmpty {
            let f = DateFormatter(); f.dateFormat = "HH:mm"
            _ = try? core.holdCart(name: f.string(from: Date()))
        }
        restoreDraft(id)
    }
    func discardDraft(_ id: String) { _ = try? core.discardDraft(id: id); loadDrafts() }

    // ── delivery orders (online; teller works the live branch queue) ──────────────
    /// Unified teller "Orders" surface — delivery + waiter open-tickets to settle,
    /// two tabs, one entry, fed by the one SSE. Replaces the old separate screens.
    @Published var showIncoming = false
    /// Which tab the unified Orders surface opens on (0 = delivery, 1 = tickets).
    @Published var incomingTab = 0
    @Published private(set) var deliveryOrders: [DeliveryOrderView] = []
    @Published private(set) var isLoadingDelivery = false
    /// Active-only filter (hide delivered/cancelled) vs the full queue.
    @Published var deliveryActiveOnly = true {
        didSet { Task { await loadDeliveryOrders() } }
    }

    private var activeStatusFilter: String { "received,confirmed,preparing,ready,out_for_delivery" }

    /// The branch's delivery accepting settings (per-channel auto/open/closed).
    @Published private(set) var deliverySettings: DeliverySettingsView?

    /// The branch delivery queue (online). Active-only by default.
    func loadDeliveryOrders() async {
        isLoadingDelivery = true; defer { isLoadingDelivery = false }
        let status: String? = deliveryActiveOnly ? activeStatusFilter : nil
        do { deliveryOrders = try await core.listDeliveryOrders(status: status) }
        catch { errorMessage = humanMessage(error) }
        deliverySettings = try? await core.deliverySettings()
    }
    /// Cycle a channel's accepting override: auto → open → closed → auto.
    func cycleAccepting(channel: String, current: String) async {
        let next = current == "auto" ? "open" : (current == "open" ? "closed" : "auto")
        isBusy = true; errorMessage = nil; defer { isBusy = false }
        do { deliverySettings = try await core.deliverySetAccepting(channel: channel, mode: next) }
        catch { errorMessage = humanMessage(error) }
    }
    /// Advance one lifecycle step (Confirm → Preparing → … → Delivered).
    func advanceDelivery(_ o: DeliveryOrderView) async {
        isBusy = true; errorMessage = nil; defer { isBusy = false }
        do { _ = try await core.deliveryAdvanceStatus(id: o.id, current: o.status); await loadDeliveryOrders() }
        catch { errorMessage = humanMessage(error) }
    }
    /// Add extra prep time (multiples of 5).
    func addDeliveryPrep(_ o: DeliveryOrderView, minutes: Int32 = 5) async {
        do { _ = try await core.deliverySetPrepTime(id: o.id, extraMinutes: minutes); await loadDeliveryOrders() }
        catch { errorMessage = humanMessage(error) }
    }
    /// Cancel a delivery order (optionally restocking ingredients).
    func cancelDelivery(_ o: DeliveryOrderView, reason: String?, restoreInventory: Bool) async -> Bool {
        isBusy = true; errorMessage = nil; defer { isBusy = false }
        do { _ = try await core.deliveryCancel(id: o.id, reason: reason, restoreInventory: restoreInventory); await loadDeliveryOrders(); return true }
        catch { errorMessage = humanMessage(error); return false }
    }
    /// Finalize into a real sale on the open shift, charged to a payment method.
    func finalizeDelivery(_ o: DeliveryOrderView, paymentMethodId: String) async -> Bool {
        isBusy = true; errorMessage = nil; defer { isBusy = false }
        do {
            let res = try await core.deliveryFinalize(id: o.id, paymentMethodId: paymentMethodId)
            await loadDeliveryOrders()
            let ref = res.orderRef.map { " · \($0)" } ?? ""
            // Surface oversold warnings instead of dropping them: replaying the
            // frozen delivery snapshot into a real sale can oversell stock, and the
            // teller must SEE that (was silently discarded — res.warnings ignored).
            if !res.warnings.isEmpty {
                showToast(t("delivery.finalized") + ref + " — " + res.warnings.joined(separator: "; "),
                          icon: "exclamationmark.triangle", tone: .warning)
            } else {
                showToast(t("delivery.finalized") + ref, icon: "checkmark.circle", tone: .success)
            }
            return true
        } catch { errorMessage = humanMessage(error); return false }
    }

    // ── realtime bus (ONE SSE per device) ────────────────────────────────────────
    /// Live connection state from the core's EventListener — drives the
    /// "reconnecting" banner on the KDS / waiter boards (bump disabled while down).
    @Published private(set) var realtimeConnected = false
    private var realtimeBridge: RealtimeBridge?
    /// The pure platform alert primitive the core calls (ping + notification + haptic).
    private let alertPlayer = RealtimeAlertPlayer()

    // ── in-app realtime alert banner (the visual companion to the OS notification) ──
    /// The active in-app alert (nil = none). Rendered at the app root, alongside
    /// the OS notification + ping + haptic. Mirrors the Flutter NewOrderBanner,
    /// generalized to every alerting event.
    @Published var realtimeAlert: RealtimeAlert?
    private var alertSeq = 0
    /// Raise the in-app banner (+ auto-dismiss). Called from `alertPlayer` on the
    /// main actor at the same deduped point the core posts the OS notification.
    func showRealtimeBanner(_ title: String, _ body: String, _ tag: String) {
        alertSeq += 1
        let id = alertSeq
        realtimeAlert = RealtimeAlert(id: id, title: title, body: body, tag: tag)
        DispatchQueue.main.asyncAfter(deadline: .now() + 6) { [weak self] in
            self?.dismissRealtimeAlert(id)
        }
    }
    func dismissRealtimeAlert(_ id: Int) {
        if realtimeAlert?.id == id { realtimeAlert = nil }
    }

    /// Open the device's ONE session-level realtime subscription. The CORE owns all
    /// the policy — it derives the topics from the signed-in role, refreshes the right
    /// board via the bridge, and raises deduped, localized alerts via `alertPlayer`
    /// (ping + OS notification + haptic). Call after login / on connectivity-regain;
    /// it replaces any prior subscription. The hosts no longer pick topics per screen.
    func startRealtime() {
        guard session != nil, deviceConfig.branchId != nil else { return }
        RealtimeAlertPlayer.requestAuthorization()
        alertPlayer.owner = self // raise the in-app banner alongside the OS notification
        let bridge = RealtimeBridge(owner: self)
        realtimeBridge = bridge
        Task { try? await core.startRealtime(listener: bridge, player: alertPlayer) }
    }
    /// Bring up the device-level LAN offline relay (Phase E). Idempotent + self-
    /// guarding: no-ops if not signed in or no LAN secret is cached yet. Called after
    /// every session is established (login / offline unlock / cold restore) and on
    /// regaining connectivity, so a till advertises its open shift to the LAN gate and
    /// KDS/waiter devices get the instant LAN fast path. Torn down in `signOut`.
    func startLanRelay() { Task { try? await core.lanStart() } }

    func unsubscribeRealtime() {
        core.unsubscribeRealtime()
        realtimeBridge = nil
        realtimeConnected = false
    }
    /// Bridge → model (already on @MainActor): refresh the surface the event touches.
    func onRealtimeEvent(_ event: RealtimeEvent) {
        let type = event.eventType
        if type.hasPrefix("kitchen.") { Task { await loadKds() } }
        else if type.hasPrefix("ticket.") { Task { await loadOpenTickets() } }
        else if type.hasPrefix("delivery.") { Task { await loadDeliveryOrders() } }
    }
    func onRealtimeConnection(_ connected: Bool) { realtimeConnected = connected }

    // ── Kitchen Display (KDS) ─────────────────────────────────────────────────────
    @Published private(set) var kdsTickets: [KdsTicketView] = []
    @Published private(set) var kdsStations: [KdsStationView] = []
    @Published private(set) var isLoadingKds = false

    /// Load the KDS feed for the device's configured station (best-effort; cached so
    /// the board survives a reconnect). Pushed by the realtime `kitchen.*` events.
    func loadKds() async {
        isLoadingKds = true; defer { isLoadingKds = false }
        if let feed = try? await core.kdsList(stationId: deviceConfig.stationId) { kdsTickets = feed }
    }
    func loadKdsStations() async { kdsStations = (try? await core.kdsListStations()) ?? [] }
    /// Bump a kitchen line (mark done at its station); a ticket goes ready when all
    /// its lines are bumped. Reloads the feed on success.
    func bumpKdsItem(_ itemId: String) async {
        do { try await core.kdsBump(itemId: itemId); await loadKds() }
        catch { showToast(humanMessage(error), icon: "exclamationmark.triangle", tone: .danger) }
    }
    func unbumpKdsItem(_ itemId: String) async {
        do { try await core.kdsUnbump(itemId: itemId); await loadKds() }
        catch { showToast(humanMessage(error), icon: "exclamationmark.triangle", tone: .danger) }
    }
    // NOTE: station-chit PRINTING is deferred with the core's `render_kitchen_ticket`
    // (ESC/POS) — the KDS board + bump are fully functional without it.

    // ── waiter open tickets ───────────────────────────────────────────────────────
    @Published private(set) var openTickets: [TicketView] = []
    @Published private(set) var isLoadingTickets = false
    /// The ticket being built/settled in the active sheet (nil = none).
    @Published var activeTicketId: String?
    /// Drives the WAITER's open-tickets list (a sub-screen over the shared order
    /// screen — the waiter reuses `OrderView`, firing instead of tendering).
    @Published var showTickets = false

    /// Waiter checkout: fire the current cart as a NEW ticket, or — when an
    /// `activeTicketId` is targeted (from "add round" on the tickets list) — add it
    /// as a ROUND to that ticket. Clears the target on success.
    func fireOrAddRound() async {
        let ok: Bool
        if let id = activeTicketId { ok = await addRound(ticketId: id) } else { ok = await fireTicket() }
        if ok { activeTicketId = nil }
    }

    /// Load the branch's open/ready tickets (server + still-queued fire overlay).
    func loadOpenTickets() async {
        isLoadingTickets = true; defer { isLoadingTickets = false }
        if let list = try? await core.listOpenTickets() { openTickets = list }
    }
    /// FIRE the current cart as a NEW open ticket (waiter). Clears the cart on success.
    func fireTicket(tableId: String? = nil, customerName: String? = nil, notes: String? = nil, guestCount: Int32? = nil) async -> Bool {
        isBusy = true; errorMessage = nil; defer { isBusy = false }
        do {
            let fired = try await core.fireTicket(tableId: tableId, customerName: customerName, notes: notes, guestCount: guestCount)
            loadCart(); await loadOpenTickets()
            showToast(t("waiter.fired") + (fired.queuedOffline ? " · " + t("waiter.queued") : ""), icon: "paperplane.fill", tone: .success)
            return true
        } catch { errorMessage = humanMessage(error); return false }
    }
    /// Add the current cart as a ROUND to an existing ticket.
    func addRound(ticketId: String) async -> Bool {
        isBusy = true; errorMessage = nil; defer { isBusy = false }
        do {
            _ = try await core.addTicketRound(ticketId: ticketId)
            loadCart(); await loadOpenTickets()
            showToast(t("waiter.fired"), icon: "paperplane.fill", tone: .success)
            return true
        } catch { errorMessage = humanMessage(error); return false }
    }
    /// Void an open ticket (and pull its kitchen tickets off the KDS).
    func voidTicket(_ ticketId: String, reason: String?) async {
        do { _ = try await core.voidTicket(ticketId: ticketId, reason: reason); await loadOpenTickets() }
        catch { showToast(humanMessage(error), icon: "exclamationmark.triangle", tone: .danger) }
    }
    /// SETTLE a ticket into a paid order on the cashier's open shift (till action).
    func settleTicket(_ ticketId: String, paymentMethodId: String, amountTenderedMinor: Int64?,
                      tipMinor: Int64 = 0, tipPaymentMethodId: String? = nil, discountId: String? = nil) async -> Bool {
        guard let shiftId = shift?.id else { errorMessage = t("waiter.need_shift"); return false }
        isBusy = true; errorMessage = nil; defer { isBusy = false }
        do {
            _ = try await core.settleTicket(
                ticketId: ticketId, shiftId: shiftId, paymentMethodId: paymentMethodId,
                amountTenderedMinor: amountTenderedMinor, tipMinor: tipMinor == 0 ? nil : tipMinor,
                tipPaymentMethodId: tipPaymentMethodId, discountId: discountId, discountType: nil, discountValue: nil)
            await loadOpenTickets(); await loadHistory()
            showToast(t("waiter.settled"), icon: "checkmark.circle", tone: .success)
            return true
        } catch { errorMessage = humanMessage(error); return false }
    }

    /// Whether THIS device is a Kitchen Display (its signed-in role is `kitchen`).
    var isKitchenDevice: Bool { session?.role == "kitchen" }
    /// Whether THIS device's teller is acting as a waiter (role `waiter`).
    var isWaiterDevice: Bool { session?.role == "waiter" }

    // ── device setup (manager) ──────────────────────────────────────────────────

    /// Step 1: a manager authenticates (online), then we load the org's branches
    /// for the picker. The manager session is kept only to fetch branches; it's
    /// dropped when the branch is bound (the POS is teller-only).
    func authenticateManager(email: String, password: String) async {
        isBusy = true; errorMessage = nil
        defer { isBusy = false }
        do {
            _ = try await core.login(req: LoginRequest(
                mode: .email, name: nil, pin: nil, branchId: nil,
                email: email, password: password, orgId: nil))
            branches = try await core.listBranches()
            setupPhase = .pickBranch
        } catch {
            errorMessage = humanMessage(error)
            try? core.logout(wipeOutbox: false)
            session = nil
        }
    }

    /// Step 2: bind the till to `branch`, drop the manager session, and leave the
    /// cached org bundle warm so tellers can unlock offline.
    func bindBranch(_ branch: BranchView) {
        // The device binding is written to the CORE store (clears reconfiguring).
        try? core.setDeviceBranch(branchId: branch.id, branchName: branch.name)
        refreshDeviceConfig()
        orgLogoUrl = branch.orgLogoUrl.flatMap { $0.isEmpty ? nil : $0 }
        UserDefaults.standard.set(orgLogoUrl ?? "", forKey: Self.orgLogoKey)
        prefetchOrgLogo() // warm the durable cache now (online) so it prints offline
        try? core.logout(wipeOutbox: false)
        session = nil
        setupPhase = .credentials
        branches = []
        errorMessage = nil
    }

    func beginReconfigure() {
        try? core.startReconfigure()
        refreshDeviceConfig()
        setupPhase = .credentials; branches = []; errorMessage = nil
    }
    func cancelReconfigure() {
        // Re-confirm the existing branch to drop the reconfigure flag (no-op if the
        // device was never bound — routing stays on device-setup either way).
        if let bid = deviceConfig.branchId {
            try? core.setDeviceBranch(branchId: bid, branchName: deviceConfig.branchName)
        }
        refreshDeviceConfig()
        setupPhase = .credentials; branches = []; errorMessage = nil
        try? core.logout(wipeOutbox: false)
        session = nil
    }

    func signOut() {
        unsubscribeRealtime()
        core.lanStop()
        try? core.logout(wipeOutbox: false)
        session = nil
        shift = nil
        cartLines = []
        cartTotals = .zero
        receipt = nil
        errorMessage = nil
    }

    // ── localization ────────────────────────────────────────────────────────────
    /// Localized UI string (from the core's shared i18n table).
    func t(_ key: String) -> String { core.tr(key: key) }
    /// Whether the active locale is right-to-left (host flips layout direction).
    var isRTL: Bool { core.isRtl() }

    // ── time formatting (branch timezone) ─────────────────────────────────────────
    /// Format an RFC3339 timestamp in the BRANCH's timezone via the core, NOT the
    /// device's local time. Styles mirror `TimeStyle`: `.time` ("hh:mm a"),
    /// `.dateShort` ("MMM d"), `.dateTime` ("MMM d, hh:mm a"), `.receipt`
    /// ("dd/MM/yyyy hh:mm a"). Use these at every display site instead of hand-rolled
    /// string-trimming or a device-local `DateFormatter`.
    func fmtTime(_ rfc: String) -> String { core.formatTime(rfc3339: rfc, style: .time) }
    func fmtDateShort(_ rfc: String) -> String { core.formatTime(rfc3339: rfc, style: .dateShort) }
    func fmtDateTime(_ rfc: String) -> String { core.formatTime(rfc3339: rfc, style: .dateTime) }
    func fmtReceipt(_ rfc: String) -> String { core.formatTime(rfc3339: rfc, style: .receipt) }

    // ── plumbing ────────────────────────────────────────────────────────────────

    /// Map the coarse `CoreError` to something a teller can read. Host-generated
    /// messages are localized; server-provided ones (auth/validation/server) pass
    /// through as the backend sent them.
    func humanMessage(_ error: Error) -> String {
        guard let e = error as? CoreError else { return error.localizedDescription }
        switch e {
        case .Offline: return t("err.offline_no_setup")
        case let .Unauthenticated(message): return message
        case let .Validation(_, message): return message
        case let .Server(_, _, message): return message
        case .Transient: return t("err.network")
        case .Forbidden: return t("err.not_allowed")
        case let .Internal(message): return message.isEmpty ? t("err.generic") : message
        }
    }

    /// Warm the durable image cache with the org logo so the receipt renders it
    /// even after a long offline stretch. Fire-and-forget; no-op without a logo.
    private func prefetchOrgLogo() {
        guard let s = orgLogoUrl, let url = URL(string: s) else { return }
        ImageStore.shared.prefetch(url)
    }

    // Device binding (branch/printer/till/station) now lives in the CORE store; the
    // host keeps only presentation prefs here.
    private static let orgLogoKey = "madar.org_logo_url"
    private static let themeKey = "madar.theme"
    private static let localeKey = "madar.locale"

    /// App-private SQLite path under Application Support.
    private static func databasePath() -> String {
        let fm = FileManager.default
        let dir = (try? fm.url(for: .applicationSupportDirectory, in: .userDomainMask,
                               appropriateFor: nil, create: true))
            ?? fm.temporaryDirectory
        return dir.appendingPathComponent("madar.sqlite").path
    }

    /// Register the bundled Cairo faces so `Font.custom("Cairo-…")` resolves
    /// (the run-on-mac bundle ships them in Resources; the iOS app can also use
    /// Info.plist UIAppFonts). Best-effort — falls back to the system font.
    private static func registerFonts() {
        let faces = ["Cairo-Regular", "Cairo-Medium", "Cairo-SemiBold", "Cairo-Bold", "Cairo-ExtraBold"]
        for face in faces {
            if let url = Bundle.main.url(forResource: face, withExtension: "ttf") {
                CTFontManagerRegisterFontsForURL(url as CFURL, .process, nil)
            }
        }
    }
}
