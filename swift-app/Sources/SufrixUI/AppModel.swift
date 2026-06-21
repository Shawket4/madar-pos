// The host's single source of UI state. Owns the one `SufrixCore` handle and
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
    let core: SufrixCore
    private let vault = KeychainTokenStore()

    /// The active session, or `nil` when signed out. Drives the root route.
    @Published private(set) var session: SessionSnapshot?
    @Published private(set) var isBusy = false
    @Published var errorMessage: String?

    /// The device's configured branch (set once at provisioning). PIN login
    /// derives the org from it; post-D13 any active org teller may sign in here.
    @Published private(set) var branchId: String
    @Published private(set) var branchName: String

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
    /// Network printer address ("host" or "host:port"; default port 9100). Empty
    /// = no printer configured. Set in Settings, persisted in UserDefaults.
    @Published var printerHost: String {
        didSet { UserDefaults.standard.set(printerHost, forKey: Self.printerKey) }
    }
    /// Print progress for the receipt confirmation's Print button.
    @Published private(set) var printState: PrintState = .idle

    init() {
        Self.registerFonts()
        var cfg = defaultConfig()
        cfg.dbPath = Self.databasePath()
        cfg.locale = Locale.current.identifier
        // A failed store open is unrecoverable — fail loudly rather than limp on.
        core = try! SufrixCore(config: cfg)
        branchId = UserDefaults.standard.string(forKey: Self.branchKey) ?? ""
        branchName = UserDefaults.standard.string(forKey: Self.branchNameKey) ?? ""
        themeMode = ThemeMode(rawValue: UserDefaults.standard.string(forKey: Self.themeKey) ?? "") ?? .light
        printerHost = UserDefaults.standard.string(forKey: Self.printerKey) ?? ""
        // Apply the saved locale to the core before any string resolves.
        let savedLocale = UserDefaults.standard.string(forKey: Self.localeKey)
        if let savedLocale { core.setLocale(locale: savedLocale) }
        locale = savedLocale ?? core.locale()

        core.setTokenStore(store: vault)
        if let blob = vault.loadBlob() {
            session = core.restoreSession(blob: blob)
        }
        loadShift()
    }

    var isSignedIn: Bool { session != nil }
    /// The screen to show — the core decides; this re-evaluates whenever the
    /// observed @Published state (session, shift, branch, reconfiguring) changes.
    var route: AppRoute {
        core.appRoute(branchConfigured: isBranchConfigured, reconfiguring: reconfiguring)
    }
    /// Till bound to a branch → teller PIN login; until then, manager device-setup.
    var isBranchConfigured: Bool { !branchId.trimmingCharacters(in: .whitespaces).isEmpty }

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
        } catch {
            errorMessage = humanMessage(error)
        }
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
    func printCurrentReceipt() async {
        guard let receipt else { return }
        let (host, port) = Self.parsePrinter(printerHost)
        guard !host.isEmpty else { printState = .noPrinter; return }
        printState = .printing
        let bytes = core.renderReceipt(
            receipt: receipt,
            storeName: branchName,
            currency: session?.currencyCode ?? "",
            width: 32
        )
        do {
            try await core.sendToPrinter(host: host, port: port, bytes: bytes)
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
            width: 32
        )
        do {
            try await core.sendToPrinter(host: host, port: port, bytes: bytes)
            printState = .printed
        } catch {
            printState = .failed
        }
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

    /// Refresh the sync chrome signals (chip counts + online) in one local read.
    func refreshPending() {
        if let s = try? core.syncStatus() {
            pendingCount = Int(s.pending)
            syncFailed = Int(s.failed)
            isOnline = s.online
        }
    }
    /// The connectivity heartbeat — pings the server (updating online + clock
    /// skew + draining the outbox), then re-reads the chrome. Called on appear
    /// and on a timer by the order screen. Online-aware: no-op effect offline.
    func refreshConnectivity() async {
        guard session != nil else { return }
        _ = await core.refreshConnectivity()
        clockSkewMinutes = Int(core.clockSkewMinutes())
        refreshPending()
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
                orderId: id, storeName: branchName, currency: session?.currencyCode ?? "", width: 32)
            try await core.sendToPrinter(host: host, port: port, bytes: bytes)
            printState = .printed
        } catch {
            printState = .failed
        }
    }

    /// Void a synced order (queues offline). Reloads history on success so the
    /// row flips to Voided. Returns whether it succeeded (the sheet dismisses).
    func voidOrder(orderId: String, reason: String, note: String?) async -> Bool {
        isBusy = true; errorMessage = nil
        defer { isBusy = false }
        do {
            try await core.voidOrder(orderId: orderId, reason: reason,
                                     note: note?.isEmpty == true ? nil : note,
                                     restoreInventory: true)
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
    func discardDraft(_ id: String) { _ = try? core.discardDraft(id: id); loadDrafts() }

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
        branchId = branch.id
        branchName = branch.name
        UserDefaults.standard.set(branchId, forKey: Self.branchKey)
        UserDefaults.standard.set(branchName, forKey: Self.branchNameKey)
        try? core.logout(wipeOutbox: false)
        session = nil
        reconfiguring = false
        setupPhase = .credentials
        branches = []
        errorMessage = nil
    }

    func beginReconfigure() {
        reconfiguring = true; setupPhase = .credentials; branches = []; errorMessage = nil
    }
    func cancelReconfigure() {
        reconfiguring = false; setupPhase = .credentials; branches = []; errorMessage = nil
        try? core.logout(wipeOutbox: false)
        session = nil
    }

    func signOut() {
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

    private static let branchKey = "sufrix.branch_id"
    private static let branchNameKey = "sufrix.branch_name"
    private static let themeKey = "sufrix.theme"
    private static let localeKey = "sufrix.locale"
    private static let printerKey = "sufrix.printer"

    /// App-private SQLite path under Application Support.
    private static func databasePath() -> String {
        let fm = FileManager.default
        let dir = (try? fm.url(for: .applicationSupportDirectory, in: .userDomainMask,
                               appropriateFor: nil, create: true))
            ?? fm.temporaryDirectory
        return dir.appendingPathComponent("sufrix.sqlite").path
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
