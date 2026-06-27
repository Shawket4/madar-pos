// Order screen — the heart of the POS. Per the design language the order screen's
// action bar is the only nav hub (no tabs/shells). Browse the branch-effective
// catalog (served from the local mirror, offline-safe) and build a cart: tap an
// item to add it, adjust quantities, see live totals. On wide layouts (iPad /
// desktop) the cart is a column beside the grid; on phones it's a bottom bar that
// opens a sheet. Tender lands in the next phase.
import SwiftUI

/// Synthetic category id for the Combos tab (bundles aren't a real category).
private let kCombosCategory = "__combos__"

extension BundleView: Identifiable {}

struct OrderView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    /// `nil` = the "All" pseudo-category.
    @State private var selectedCategory: String?
    @State private var search = ""
    @State private var showCart = false
    /// Wide layouts open tender as a root sheet; phones swap the cart sheet's
    /// content to tender (avoids unreliable sheet-over-sheet presentation).
    @State private var showTenderWide = false
    @State private var tenderInCart = false

    private var currency: String { app.session?.currencyCode ?? "" }

    // Waiter mode: the cart's checkout FIRES a ticket (or adds a round) instead of
    // opening the cashier tender flow. Same component, different terminal action.
    private var checkoutLabel: String {
        guard app.isWaiterDevice else { return t("order.checkout") }
        return app.activeTicketId != nil ? t("waiter.add_round") : t("waiter.fire")
    }
    private var checkoutIcon: String { app.isWaiterDevice ? "paperplane.fill" : "creditcard" }
    private func onCheckout(wide: Bool) {
        if app.isWaiterDevice {
            Task { await app.fireOrAddRound() }
        } else if wide {
            showTenderWide = true
        } else {
            tenderInCart = true
        }
    }

    private var visibleItems: [MenuItemView] {
        app.menuItems
            .filter { $0.isActive }
            .filter { selectedCategory == nil || $0.categoryId == selectedCategory }
            .filter {
                search.isEmpty
                    || $0.name.localizedCaseInsensitiveContains(search)
                    || ($0.description?.localizedCaseInsensitiveContains(search) ?? false)
            }
    }

    var body: some View {
        GeometryReader { geo in
            let wide = geo.size.width >= Responsive.wide
            ZStack {
                theme.colors.bg.ignoresSafeArea()
                VStack(spacing: 0) {
                    OrderTopBar(app: app, wide: wide)
                    if !app.isOnline {
                        NoticeBanner(icon: "wifi.slash", text: t("chrome.offline_banner"), tone: .warning)
                            .padding(.horizontal, Space.lg)
                            .padding(.top, Space.sm)
                    }
                    if app.syncAuthPaused {
                        Button { app.errorMessage = nil; app.showReauth = true } label: {
                            NoticeBanner(icon: "lock.circle", text: t("chrome.auth_paused"),
                                         tone: .danger, actionLabel: t("chrome.auth_paused_action"))
                        }
                        .buttonStyle(.plain)
                        .padding(.horizontal, Space.lg)
                        .padding(.top, Space.sm)
                    }
                    if abs(app.clockSkewMinutes) >= 5 {
                        NoticeBanner(icon: "clock.badge.exclamationmark",
                                     text: "\(t("chrome.clock_skew")) (\(abs(app.clockSkewMinutes))m)", tone: .warning)
                            .padding(.horizontal, Space.lg)
                            .padding(.top, Space.sm)
                    }
                    if let error = app.errorMessage {
                        NoticeBanner(icon: "exclamationmark.circle", text: error, tone: .danger)
                            .padding(.horizontal, Space.lg)
                            .padding(.top, Space.sm)
                    }
                    if wide {
                        HStack(spacing: 0) {
                            catalogColumn(wide: true)
                            Rectangle().fill(theme.colors.border).frame(width: 1)
                            CartPanel(app: app, checkoutLabel: checkoutLabel, checkoutIcon: checkoutIcon,
                                      onCheckout: { onCheckout(wide: true) }).frame(width: 340)
                        }
                    } else {
                        catalogColumn(wide: false)
                        CartBar(app: app, currency: currency) { showCart = true }
                    }
                }
            }
            // ── Bottom-sheet modals: scrim + draggable card (tap-out / drag-down
            // to dismiss). Custom presenter — macOS .sheet can't do either and
            // stacking many leaves dismiss artifacts. In-tree, so they inherit the
            // theme/localize/RTL/toast environment (no modalChrome needed). ───────
            // Phone: the cart sheet swaps its content to tender on checkout.
            // Width capped to Flutter's ResponsiveSheet (600), not the 720 default.
            .madarSheet(isPresented: $showCart, size: .large, maxWidth: 600,
                         onDismiss: { tenderInCart = false; app.dismissReceipt() }) { dismiss in
                Group {
                    if tenderInCart {
                        TenderView(app: app, onClose: { tenderInCart = false; dismiss() })
                    } else {
                        CartPanel(app: app, onClose: dismiss, checkoutLabel: checkoutLabel, checkoutIcon: checkoutIcon,
                                  onCheckout: {
                                      if app.isWaiterDevice { Task { await app.fireOrAddRound(); dismiss() } }
                                      else { tenderInCart = true }
                                  })
                    }
                }
            }
            // Wide: tender is its own sheet beside the cart column.
            // Width capped to Flutter's CheckoutSheet (ResponsiveSheet, 600).
            .madarSheet(isPresented: $showTenderWide, size: .large, maxWidth: 600,
                         onDismiss: { app.dismissReceipt() }) { dismiss in
                TenderView(app: app, onClose: dismiss)
            }
            // Item customization. The derived binding runs closeItemDetail() on
            // EVERY dismissal route — tap-out, drag-down, or the header ✕.
            .madarSheet(item: Binding(get: { app.detailItem },
                                       set: { if $0 == nil { app.closeItemDetail() } }),
                         size: .hug, maxWidth: 600) { item, dismiss in
                ItemDetailView(app: app, item: item, onClose: dismiss)
            }
            // Bundle (combo) configuration.
            .madarSheet(item: $app.detailBundle, size: .hug, maxWidth: 600) { bundle, dismiss in
                BundleDetailView(app: app, bundle: bundle, onClose: dismiss)
            }
            // More — overflow nav hub (close shift, sign out, …). Capped at the
            // Flutter ResponsiveSheet width (600) so it isn't an over-wide slab.
            .madarSheet(isPresented: $app.showMore, maxWidth: 600) { _ in
                MoreDrawer(app: app, wide: wide)
            }
            // Held orders (drafts).
            .madarSheet(isPresented: $app.showDrafts, maxWidth: 600) { dismiss in
                DraftsView(app: app, onClose: dismiss)
            }
            // Mid-shift Z-report preview + print.
            .madarSheet(isPresented: $app.showReportPreview, size: .large, maxWidth: 600) { dismiss in
                ShiftReportPreviewView(app: app, onClose: dismiss)
            }
            // Token expired mid-shift → re-auth the same teller to resume sync.
            .madarSheet(isPresented: $app.showReauth, size: .hug, maxWidth: 440) { dismiss in
                ReauthView(app: app, onClose: dismiss)
            }
            // ── Full-screen routed screens: ONE route-driven overlay. (Was seven
            // stacked `.appScreen` overlays, each with its own transition +
            // implicit animation — they re-evaluated together and fought during a
            // push, which is the jitter/artifacting. A single transition fixes it.)
            .orderScreenRouter(app: app)
        }
        .task {
            if app.isWaiterDevice {
                // A waiter has no shift/drawer and no orders history — it fires
                // tickets. Load the menu + the open tickets; live ticket/kitchen
                // events arrive on the session-level SSE (subscribed at login).
                await app.loadCatalog()
                await app.loadOpenTickets()
            } else {
                await app.reconcileShift()
                await app.loadCatalog()
                app.refreshPending()
                await app.loadHistory()
            }
        }
        // Connectivity heartbeat — refresh online + clock skew (+ drain) every
        // 15s while the order screen is up; cancelled when it goes away.
        .task {
            while !Task.isCancelled {
                await app.refreshConnectivity()
                try? await Task.sleep(nanoseconds: 15_000_000_000)
            }
        }
    }

    @ViewBuilder private func catalogColumn(wide: Bool) -> some View {
        if wide {
            // Tablet/desktop: a vertical category rail beside the search + grid.
            HStack(spacing: 0) {
                CategoryRail(app: app, categories: app.categories, selected: $selectedCategory, showCombos: !app.bundles.isEmpty)
                Rectangle().fill(theme.colors.borderLight).frame(width: 1)
                VStack(spacing: 0) { searchAndGrid }
            }
        } else {
            // Phone: a horizontal underline-tab strip above the search + grid.
            VStack(spacing: 0) {
                CategoryTabs(app: app, categories: app.categories, selected: $selectedCategory, showCombos: !app.bundles.isEmpty)
                searchAndGrid
            }
        }
    }

    @ViewBuilder private var searchAndGrid: some View {
        if selectedCategory == kCombosCategory {
            bundleGrid
        } else {
            SearchField(text: $search, placeholder: t("order.search"))
                .padding(.horizontal, Space.lg)
                .padding(.top, Space.sm)
                .padding(.bottom, Space.sm)
            ItemGridOrEmpty(
                items: visibleItems, currency: currency, searching: !search.isEmpty,
                categoryName: { id in app.categories.first { $0.id == id }?.name ?? "" },
                cartQty: { itemId in app.cartQtyForItem(itemId) },
                onAdd: { item in app.openItemDetail(item) }
            )
        }
    }

    private var bundleGrid: some View {
        ScrollView {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: Grid.cellMin, maximum: Grid.cellMax), spacing: Grid.gutter)], spacing: Grid.gutter) {
                ForEach(app.bundles, id: \.id) { b in
                    BundleCard(bundle: b, currency: currency) { app.openBundleDetail(b) }
                }
            }
            .padding(Space.md)
        }
    }
}

// MARK: - Top action bar (the only nav hub)

private struct OrderTopBar: View {
    @ObservedObject var app: AppModel
    let wide: Bool
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    private var currency: String { app.session?.currencyCode ?? "" }

    var body: some View {
        // Fills + right-pins when it fits; scrolls horizontally only when the
        // content can't fit the viewport (very narrow phones / split layouts).
        ViewThatFits(in: .horizontal) {
            barRow
            ScrollView(.horizontal, showsIndicators: false) { barRow }
        }
        .padding(.horizontal, Space.lg)
        .padding(.vertical, Space.md)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) {
            Rectangle().fill(theme.colors.border).frame(height: 1)
        }
    }

    @ViewBuilder private var barRow: some View {
        HStack(spacing: Space.sm) {
            MadarMark(size: 32)
            // Phone: the status chips + secondary action buttons don't fit a ~360pt
            // bar, so they collapse into the More drawer (which carries the teller +
            // live stats in its header). Only the logo, sync status, and More stay.
            // A waiter holds no shift, so it shows neither the teller chip nor stats.
            if wide && !app.isWaiterDevice, let s = app.shift {
                StatusChip(label: s.tellerName, icon: "person.fill", tone: .info)
            }
            if wide && !app.isWaiterDevice && app.shift?.isOpen == true { ShiftStatsPill(app: app, currency: currency) }
            Spacer(minLength: 0)
            SyncChip(app: app)
            if app.isWaiterDevice {
                // Waiter's nav: the open-tickets list + settings; the rest is in More.
                barButton(icon: "fork.knife") { Task { await app.loadOpenTickets() }; app.showTickets = true }
                if wide { barButton(icon: "gearshape") { app.refreshPending(); app.showSettings = true } }
            } else if wide {
                syncDataButton
                barButton(icon: "list.bullet.rectangle") { app.showHistory = true }
                barButton(icon: "gearshape") { app.refreshPending(); app.showSettings = true }
            }
            barButton(icon: "ellipsis") { app.refreshPending(); app.showMore = true }
        }
    }

    private func barButton(icon: String, action: @escaping () -> Void) -> some View {
        Button { Haptics.selection(); action() } label: {
            MadarIcon(icon, size: 15)
                .foregroundStyle(theme.colors.textMuted)
                .frame(width: 34, height: 34)
                .background(theme.colors.surfaceAlt)
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                    .strokeBorder(theme.colors.borderLight, lineWidth: 1))
        }
        .buttonStyle(.pressable)
    }

    /// Manual "sync server data" — re-pulls the catalog (menu, add-ons, bundles,
    /// payment methods, discounts). Spins + disables while running. Mirrors
    /// Flutter's top-bar `SyncBtn`.
    private var syncDataButton: some View {
        Button {
            Haptics.selection()
            Task { await app.syncServerData() }
        } label: {
            Group {
                if app.isSyncingData {
                    ProgressView().controlSize(.small).tint(theme.colors.accent)
                } else {
                    MadarIcon("arrow.clockwise", size: 15)
                        .foregroundStyle(theme.colors.textMuted)
                }
            }
            .frame(width: 34, height: 34)
            .background(theme.colors.surfaceAlt)
            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                .strokeBorder(theme.colors.borderLight, lineWidth: 1))
        }
        .buttonStyle(.pressable)
        .disabled(app.isSyncingData)
        .accessibilityLabel(t("chrome.sync_data"))
    }
}

/// Live shift totals — "EGP X · N orders" (voided excluded, summed in core).
private struct ShiftStatsPill: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let currency: String

    var body: some View {
        HStack(spacing: 4) {
            Text(Money.format(app.shiftSalesMinor, currency))
                .font(.money(11, .bold)).foregroundStyle(theme.colors.textPrimary)
            Text("·").foregroundStyle(theme.colors.textMuted)
            Text("\(app.shiftOrderCount) \(t("chrome.orders"))")
                .font(.ui(11, .semibold)).foregroundStyle(theme.colors.textSecondary)
        }
        .padding(.horizontal, 10).padding(.vertical, 5)
        .background(theme.colors.surfaceAlt)
        .clipShape(Capsule())
        .overlay(Capsule().strokeBorder(theme.colors.borderLight, lineWidth: 1))
    }
}

/// Sync status chip — offline / queued / stuck / syncing, hidden when idle and
/// fully synced. Taps to the sync center. Mirrors Flutter's SyncStatusChip.
private struct SyncChip: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    private enum State { case offline, stuck, syncing, idle }
    private var state: State {
        if !app.isOnline { return .offline }
        if app.syncFailed > 0 { return .stuck }
        if app.pendingCount > 0 { return .syncing }
        return .idle
    }

    var body: some View {
        if state != .idle {
            Button {
                Haptics.selection()
                app.loadOutbox()
                app.showSync = true
            } label: {
                HStack(spacing: 5) {
                    MadarIcon(icon, size: 12)
                    Text(label).font(.ui(11, .semibold))
                }
                .foregroundStyle(tone)
                .padding(.horizontal, 10).padding(.vertical, 5)
                .background(toneBg)
                .clipShape(Capsule())
            }
            .buttonStyle(.pressable)
        }
    }

    private var label: String {
        switch state {
        case .offline: return app.pendingCount > 0
            ? "\(t("chrome.offline")) · \(app.pendingCount) \(t("chrome.queued"))"
            : t("chrome.offline")
        case .stuck: return "\(t("chrome.needs_attention")) (\(app.syncFailed))"
        case .syncing: return "\(t("chrome.syncing")) (\(app.pendingCount))"
        case .idle: return ""
        }
    }
    private var icon: String {
        switch state {
        case .offline: return "wifi.slash"
        case .stuck: return "exclamationmark.triangle"
        case .syncing: return "arrow.triangle.2.circlepath"
        case .idle: return "checkmark"
        }
    }
    private var tone: Color {
        switch state {
        case .offline, .syncing: return theme.colors.warning
        case .stuck: return theme.colors.danger
        case .idle: return theme.colors.textMuted
        }
    }
    private var toneBg: Color {
        switch state {
        case .offline, .syncing: return theme.colors.warningBg
        case .stuck: return theme.colors.dangerBg
        case .idle: return theme.colors.surfaceAlt
        }
    }
}

/// The "More" overflow drawer — secondary nav-hub actions that don't fit the
/// bar. Mirrors Flutter's ActionDrawer (a shift-status header + action rows).
private struct MoreDrawer: View {
    @ObservedObject var app: AppModel
    let wide: Bool
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    private var currency: String { app.session?.currencyCode ?? "" }

    var body: some View {
        VStack(spacing: 0) {
            Capsule().fill(theme.colors.border).frame(width: 36, height: 4)
                .padding(.top, Space.sm).padding(.bottom, Space.md)
            if let s = app.shift {
                HStack(spacing: Space.sm) {
                    Circle().fill(app.isOnline ? theme.colors.success : theme.colors.warning)
                        .frame(width: 8, height: 8)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(s.tellerName).font(.ui(14, .bold)).foregroundStyle(theme.colors.textPrimary)
                        Text(app.isOnline ? t("chrome.online") : t("chrome.offline"))
                            .font(.ui(11)).foregroundStyle(theme.colors.textSecondary)
                        // Phone: carry the live shift stats here since the bar pill is hidden.
                        if !wide && s.isOpen {
                            Text("\(Money.format(app.shiftSalesMinor, currency)) · \(app.shiftOrderCount) \(t("chrome.orders"))")
                                .font(.ui(11, .semibold)).foregroundStyle(theme.colors.textSecondary)
                        }
                    }
                    Spacer()
                }
                .padding(Space.md)
                .background(theme.colors.surfaceAlt)
                .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
                .padding(.horizontal, Space.lg)
            }
            VStack(spacing: Space.sm) {
                if app.isWaiterDevice {
                    // Waiter: open-tickets list + the sync center. No shift/cash/till rows.
                    row(icon: "fork.knife", label: t("waiter.tickets"), tone: theme.colors.textPrimary) {
                        app.showMore = false; Task { await app.loadOpenTickets() }; app.showTickets = true
                    }
                    row(icon: "arrow.triangle.2.circlepath", label: t("sync.title"), tone: theme.colors.textPrimary) {
                        app.showMore = false; app.loadOutbox(); app.showSync = true
                    }
                } else {
                    // Phone-only: the bar's History / Sync / Sync-data buttons live here instead.
                    if !wide {
                        row(icon: "list.bullet.rectangle", label: t("history.title"), tone: theme.colors.textPrimary) {
                            app.showMore = false; app.showHistory = true
                        }
                        row(icon: "arrow.triangle.2.circlepath", label: t("sync.title"), tone: theme.colors.textPrimary) {
                            app.showMore = false; app.loadOutbox(); app.showSync = true
                        }
                        row(icon: "arrow.clockwise", label: t("chrome.sync_data"), tone: theme.colors.textPrimary) {
                            app.showMore = false; Task { await app.syncServerData() }
                        }
                    }
                    row(icon: "banknote", label: t("cash.title"), tone: theme.colors.textPrimary) {
                        app.showMore = false; app.errorMessage = nil; app.showCashMovements = true
                    }
                    row(icon: "clock.arrow.circlepath", label: t("shifts.title"), tone: theme.colors.textPrimary) {
                        app.showMore = false; app.showShiftHistory = true
                    }
                    row(icon: "printer", label: t("shift.print_report"), tone: theme.colors.textPrimary) {
                        app.showMore = false; app.openShiftReportPreview()
                    }
                    row(icon: "tray.full", label: t("drafts.title"), tone: theme.colors.textPrimary) {
                        app.showMore = false; app.loadDrafts(); app.showDrafts = true
                    }
                    // ONE entry for both delivery + waiter open-tickets (two tabs).
                    row(icon: "bicycle", label: t("incoming.title"), tone: theme.colors.textPrimary) {
                        app.showMore = false; app.errorMessage = nil
                        Task { await app.loadDeliveryOrders(); await app.loadOpenTickets() }
                        app.showIncoming = true
                    }
                    row(icon: "lock", label: t("order.close_shift"), tone: theme.colors.danger) {
                        app.showMore = false; app.errorMessage = nil; app.showCloseShift = true
                    }
                }
                row(icon: "gearshape", label: t("settings.title"), tone: theme.colors.textPrimary) {
                    app.showMore = false; app.refreshPending(); app.showSettings = true
                }
                row(icon: "rectangle.portrait.and.arrow.right", label: t("settings.sign_out"), tone: theme.colors.textPrimary) {
                    // You can't sign out mid-shift — close the drawer first.
                    if app.hasOpenShift {
                        app.flagError(t("settings.sign_out_shift_open"))
                    } else {
                        app.showMore = false; app.signOut()
                    }
                }
            }
            .padding(Space.lg)
            Spacer(minLength: 0)
        }
        .frame(maxWidth: 600)
        .frame(maxWidth: .infinity)
        .background(theme.colors.surfaceAlt)
    }

    private func row(icon: String, label: String, tone: Color, action: @escaping () -> Void) -> some View {
        Button { Haptics.selection(); action() } label: {
            HStack(spacing: Space.md) {
                MadarIcon(icon, size: 15)
                    .foregroundStyle(tone).frame(width: 28)
                Text(label).font(.ui(15, .semibold)).foregroundStyle(tone)
                Spacer()
                MadarIcon("chevron.forward", size: 12)
                    .foregroundStyle(theme.colors.textMuted)
            }
            .padding(.horizontal, Space.md).padding(.vertical, 14)
            .background(theme.colors.surface)
            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                .strokeBorder(theme.colors.borderLight, lineWidth: 1))
        }
        .buttonStyle(.pressable)
    }
}

// MARK: - Category strip

/// Phone: a horizontal underline-tab strip (the Flutter CategoryStrip).
private struct CategoryTabs: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let categories: [CategoryView]
    @Binding var selected: String?
    var showCombos: Bool = false

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: Space.lg) {
                tab(t("order.all"), id: nil, leadingIcon: "square.grid.2x2.fill")
                if showCombos { tab(t("order.combos"), id: kCombosCategory, leadingIcon: "square.stack.3d.up.fill") }
                ForEach(categories.filter { $0.isActive }, id: \.id) { c in
                    tab(c.name, id: c.id,
                        leadingIcon: categoryIconName(app.core.categoryStyle(name: c.name, dark: theme.isDark).icon))
                }
            }
            .padding(.horizontal, Space.md)
        }
        .frame(height: 46)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }

    // Compact strip: a leading icon only when the family is recognized; custom
    // categories lean on their label (no redundant monogram inline).
    private func tab(_ label: String, id: String?, leadingIcon: String?) -> some View {
        let active = selected == id
        return Button {
            Haptics.selection()
            selected = id
        } label: {
            VStack(spacing: 0) {
                Spacer(minLength: 0)
                HStack(spacing: 6) {
                    if let leadingIcon { MadarIcon(leadingIcon, size: IconSize.sm) }
                    Text(label).font(.ui(13, active ? .bold : .medium))
                }
                .foregroundStyle(active ? theme.colors.accent : theme.colors.textMuted)
                Spacer(minLength: 0)
                Rectangle()
                    .fill(active ? theme.colors.accent : Color.clear)
                    .frame(height: 2)
                    .clipShape(RoundedRectangle(cornerRadius: 1))
            }
            .frame(height: 46)
        }
        .buttonStyle(.pressable(scale: 0.97))
    }
}

/// Tablet/desktop: a 94pt vertical category rail (the Flutter CategoryRail). Each
/// tile carries a category-styled gradient icon badge above its label.
private struct CategoryRail: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let categories: [CategoryView]
    @Binding var selected: String?
    var showCombos: Bool = false

    var body: some View {
        ScrollView(showsIndicators: false) {
            VStack(spacing: 3) {
                tile(t("order.all"), id: nil, style: nil, imageUrl: nil, fixedIcon: "square.grid.2x2.fill")
                if showCombos { tile(t("order.combos"), id: kCombosCategory, style: nil, imageUrl: nil, fixedIcon: "square.stack.3d.up.fill") }
                ForEach(categories.filter { $0.isActive }, id: \.id) { c in
                    tile(c.name, id: c.id, style: app.core.categoryStyle(name: c.name, dark: theme.isDark), imageUrl: c.imageUrl, fixedIcon: nil)
                }
            }
            .padding(.vertical, Space.sm)
        }
        .frame(width: 96)
        .background(theme.colors.surface)
    }

    private func tile(_ label: String, id: String?, style: CatStyleView?, imageUrl: String?, fixedIcon: String?) -> some View {
        let active = selected == id
        let iconColor = style.map { Color(hex: $0.iconColor) } ?? theme.colors.accent
        return Button {
            Haptics.selection()
            selected = id
        } label: {
            VStack(spacing: 5) {
                ZStack {
                    RoundedRectangle(cornerRadius: 11, style: .continuous)
                        .fill(LinearGradient(
                            colors: style.map { [Color(hex: $0.bgTop), Color(hex: $0.bgBottom)] } ?? [theme.colors.accentBg, theme.colors.accentBg],
                            startPoint: .topLeading, endPoint: .bottomTrailing))
                    // Base layer — fixed icon (All/Combos) → family icon → monogram.
                    // ALWAYS drawn, so a missing OR failed-to-load category image
                    // still shows something (the previous if/else fell through to
                    // nothing when an image url was present but didn't load).
                    if let fixedIcon {
                        MadarIcon(fixedIcon, size: IconSize.md).foregroundStyle(iconColor)
                    } else if let key = style?.icon, let icon = categoryIconName(key) {
                        MadarIcon(icon, size: IconSize.md).foregroundStyle(iconColor)
                    } else {
                        Text(categoryMonogram(label)).font(.ui(15, .bold)).foregroundStyle(iconColor)
                    }
                    // Overlay — the uploaded image covers the base once it loads.
                    if fixedIcon == nil, let s = imageUrl, let u = URL(string: s) {
                        CachedAsyncImage(url: u)
                            .frame(width: 38, height: 38)
                            .clipShape(RoundedRectangle(cornerRadius: 11, style: .continuous))
                    }
                }
                .frame(width: 38, height: 38)
                .overlay(
                    RoundedRectangle(cornerRadius: 11, style: .continuous)
                        .strokeBorder(active ? theme.colors.accent : Color.clear, lineWidth: 2))
                Text(label)
                    .font(.ui(10, active ? .bold : .medium))
                    .foregroundStyle(active ? theme.colors.accent : theme.colors.textSecondary)
                    .multilineTextAlignment(.center)
                    .lineLimit(2)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, Space.sm)
            .padding(.horizontal, Space.xs)
            .background(active ? theme.colors.accentBg : Color.clear)
            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        }
        .buttonStyle(.pressable(scale: 0.95))
        .padding(.horizontal, Space.sm)
    }
}

// MARK: - Item grid

private struct ItemGridOrEmpty: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let items: [MenuItemView]
    let currency: String
    let searching: Bool
    let categoryName: (String?) -> String
    let cartQty: (String) -> Int64
    let onAdd: (MenuItemView) -> Void

    // Width-driven columns (≈150–220pt cells) so landscape never yields giant cards.
    // Mirrors Flutter's SliverGridDelegateWithMaxCrossAxisExtent(maxCrossAxisExtent: 220).
    private let columns = [GridItem(.adaptive(minimum: Grid.cellMin, maximum: Grid.cellMax), spacing: Grid.gutter)]

    var body: some View {
        if items.isEmpty {
            VStack(spacing: Space.md) {
                MadarIcon(searching ? "magnifyingglass" : "tray", size: 36)
                    .foregroundStyle(theme.colors.textMuted)
                Text(t(searching ? "order.empty_search" : "order.empty"))
                    .font(.ui(14)).foregroundStyle(theme.colors.textSecondary)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ScrollView {
                LazyVGrid(columns: columns, spacing: Grid.gutter) {
                    ForEach(items, id: \.id) { item in
                        MenuItemCard(
                            item: item,
                            categoryName: categoryName(item.categoryId),
                            currency: currency,
                            inCartQty: cartQty(item.id)
                        ) { onAdd(item) }
                    }
                }
                .padding(Space.md)
            }
        }
    }
}

// MARK: - Search field

private struct SearchField: View {
    @Environment(\.theme) private var theme
    @Binding var text: String
    let placeholder: String

    var body: some View {
        HStack(spacing: Space.sm) {
            MadarIcon("magnifyingglass", size: 14)
                .foregroundStyle(theme.colors.textMuted)
            TextField(placeholder, text: $text)
                .textFieldStyle(.plain) // no inner macOS bezel
                .font(.ui(15))
                .foregroundStyle(theme.colors.textPrimary)
            if !text.isEmpty {
                Button {
                    text = ""
                } label: {
                    MadarIcon("xmark.circle.fill", size: IconSize.lg)
                        .foregroundStyle(theme.colors.textMuted)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, Space.md)
        .frame(height: 40)
        .background(theme.colors.surfaceAlt)
        .overlay(
            RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                .strokeBorder(theme.colors.borderLight, lineWidth: 1)
        )
        .elevation(.card)
        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
    }
}

// MARK: - Cart panel (wide column + phone sheet)

private struct CartPanel: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    /// Set when shown as a phone sheet (shows a close affordance).
    var onClose: (() -> Void)? = nil
    /// Checkout button label/icon — "Checkout" (teller tender) or "Fire"/"Add round"
    /// (waiter). Defaults keep the teller call sites unchanged.
    var checkoutLabel: String? = nil
    var checkoutIcon: String? = nil
    /// The terminal cart action (tender, or fire a ticket).
    var onCheckout: () -> Void

    private var currency: String { app.session?.currencyCode ?? "" }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(t("order.cart")).font(.ui(17, .bold)).foregroundStyle(theme.colors.textPrimary)
                if app.cartTotals.itemCount > 0 {
                    StatusChip(label: "\(app.cartTotals.itemCount)", tone: .accent)
                }
                Spacer()
                if !app.cartLines.isEmpty {
                    Button(t("order.clear")) { app.clearCart() }
                        .buttonStyle(.plain)
                        .font(.ui(13, .semibold))
                        .foregroundStyle(theme.colors.danger)
                }
                if let onClose {
                    Button { onClose() } label: {
                        MadarIcon("xmark", size: 15)
                            .foregroundStyle(theme.colors.textMuted)
                    }
                    .buttonStyle(.plain)
                    .padding(.leading, Space.sm)
                }
            }
            .padding(.horizontal, Space.lg)
            .padding(.vertical, 14)
            Rectangle().fill(theme.colors.border).frame(height: 1)

            // Held-order tabs — flip between parked carts (switching parks the
            // current one first, so nothing is lost). The bottom Hold button stays.
            if !app.drafts.isEmpty {
                HeldOrdersTabs(app: app)
            }

            if app.cartLines.isEmpty {
                VStack(spacing: Space.md) {
                    MadarIcon("cart", size: 34)
                        .foregroundStyle(theme.colors.textMuted)
                    Text(t("order.cart_empty"))
                        .font(.ui(14)).foregroundStyle(theme.colors.textSecondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    VStack(spacing: Space.sm) {
                        ForEach(app.cartLines, id: \.key) { line in
                            CartLineRow(
                                line: line, currency: currency,
                                onDec: { app.setCartQty(line.key, line.qty - 1) },
                                onInc: { app.setCartQty(line.key, line.qty + 1) },
                                // Bundles aren't re-editable in place (reconfigure by
                                // removing + re-adding); only plain lines reopen the sheet.
                                onEdit: line.bundleId == nil ? { app.editCartLine(line) } : nil,
                                onSwipeDelete: { app.swipeRemoveCartLine(line) }
                            )
                            .transition(.move(edge: .leading).combined(with: .opacity))
                        }
                    }
                    .padding(Space.lg)
                    .animation(Motion.standard, value: app.cartLines.count)
                }
                CartFooter(totals: app.cartTotals, currency: currency,
                           checkoutLabel: checkoutLabel, checkoutIcon: checkoutIcon,
                           onCheckout: onCheckout, onHold: { app.holdCart() })
            }
        }
        .background(theme.colors.bg)
        .onAppear { app.loadDrafts() }
    }
}

/// Held-order tabs above the cart — the active cart plus a tab per parked order.
/// Tapping a held tab parks the current cart, then loads that one (lossless).
private struct HeldOrdersTabs: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: Space.sm) {
                tab(label: t("drafts.current"), count: Int(app.cartTotals.itemCount), active: true, onTap: nil, onClose: nil)
                ForEach(app.drafts, id: \.id) { d in
                    tab(label: d.name, count: Int(d.itemCount), active: false,
                        onTap: { app.switchToHeldOrder(d.id) },
                        onClose: { app.discardDraft(d.id) })
                }
            }
            .padding(.horizontal, Space.lg).padding(.vertical, Space.sm)
        }
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.borderLight).frame(height: 1) }
    }

    private func tab(label: String, count: Int, active: Bool, onTap: (() -> Void)?, onClose: (() -> Void)?) -> some View {
        HStack(spacing: 6) {
            MadarIcon(active ? "cart.fill" : "tray.full", size: 11)
            Text(label).font(.ui(12, .semibold)).lineLimit(1)
            if count > 0 {
                Text("\(count)").font(.ui(10, .bold))
                    .padding(.horizontal, 5).padding(.vertical, 1)
                    .background(active ? theme.colors.textOnAccent.opacity(0.25) : theme.colors.surfaceAlt)
                    .clipShape(Capsule())
            }
            if let onClose {
                Button { Haptics.selection(); onClose() } label: {
                    MadarIcon("xmark", size: 9)
                        .foregroundStyle(active ? theme.colors.textOnAccent : theme.colors.textMuted)
                }
                .buttonStyle(.plain)
            }
        }
        .foregroundStyle(active ? theme.colors.textOnAccent : theme.colors.textSecondary)
        .padding(.horizontal, 12).padding(.vertical, 7)
        .background(active ? theme.colors.accent : theme.colors.surfaceAlt)
        .clipShape(Capsule())
        .overlay(Capsule().strokeBorder(active ? Color.clear : theme.colors.border, lineWidth: 1))
        .contentShape(Capsule())
        .onTapGesture { if let onTap { Haptics.selection(); onTap() } }
    }
}

private struct CartLineRow: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    @Environment(\.layoutDirection) private var dir
    let line: CartLineView
    let currency: String
    let onDec: () -> Void
    let onInc: () -> Void
    var onEdit: (() -> Void)? = nil
    /// Swipe-to-delete the whole line (nil disables the gesture).
    var onSwipeDelete: (() -> Void)? = nil

    @State private var dragX: CGFloat = 0

    private var isBundle: Bool { line.bundleId != nil }
    private var hasModifiers: Bool {
        line.sizeLabel != nil || !line.addons.isEmpty || !line.optionals.isEmpty
    }
    /// Delete-swipe direction: left in LTR, right in RTL.
    private var swipeSign: CGFloat { dir == .rightToLeft ? 1 : -1 }

    var body: some View {
        ZStack {
            // Red delete affordance revealed under the row as it slides away.
            if onSwipeDelete != nil && dragX != 0 {
                MadarIcon("trash.fill", size: 16)
                    .foregroundStyle(theme.colors.textOnAccent)
                    .frame(maxWidth: .infinity, maxHeight: .infinity,
                           alignment: dir == .rightToLeft ? .leading : .trailing)
                    .padding(.horizontal, Space.xl)
                    .background(theme.colors.danger)
                    .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
            }
            rowContent
                .offset(x: dragX)
                .gesture(swipeGesture)
        }
    }

    private var swipeGesture: some Gesture {
        DragGesture(minimumDistance: 14)
            .onChanged { v in
                guard onSwipeDelete != nil else { return }
                guard abs(v.translation.width) > abs(v.translation.height) else { return }
                let raw = v.translation.width
                // Only track movement in the delete direction.
                if (swipeSign < 0 && raw < 0) || (swipeSign > 0 && raw > 0) {
                    dragX = max(-120, min(120, raw))
                }
            }
            .onEnded { _ in
                if abs(dragX) > 72 {
                    onSwipeDelete?() // removes the line → the row leaves the list
                    dragX = 0
                } else {
                    withAnimation(Motion.standard) { dragX = 0 }
                }
            }
    }

    private var rowContent: some View {
        HStack(spacing: Space.md) {
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    Text(line.name).font(.ui(13, .semibold))
                        .foregroundStyle(theme.colors.textPrimary).lineLimit(1)
                    if isBundle { StatusChip(label: t("order.combos"), tone: .accent) }
                }
                if isBundle { bundleBreakdown } else if hasModifiers { modifierPills }
                if let note = line.notes, !note.isEmpty {
                    Text("“\(note)”")
                        .font(.ui(11)).italic().foregroundStyle(theme.colors.textMuted)
                        .lineLimit(2)
                }
                Text(Money.format(line.lineTotalMinor, currency))
                    .font(.money(13, .bold)).foregroundStyle(theme.colors.textPrimary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
            .onTapGesture { onEdit?() }
            Spacer(minLength: Space.sm)
            // The minus button removes the line at qty 1 (the remove affordance).
            QtyStepper(qty: line.qty, onDec: onDec, onInc: onInc)
        }
        .padding(Space.md)
        .background(theme.colors.surface)
        .overlay(
            RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                .strokeBorder(theme.colors.borderLight, lineWidth: 1)
        )
        .elevation(.card)
        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
    }

    private var modifierPills: some View {
        FlowLayout(spacing: 4) {
            if let s = line.sizeLabel {
                pill(s, fg: theme.colors.textSecondary, bg: theme.colors.surfaceAlt)
            }
            ForEach(line.addons, id: \.addonItemId) { a in
                pill(a.qty > 1 ? "\(a.name) ×\(a.qty)" : a.name, fg: theme.colors.navy, bg: theme.colors.navyBg)
            }
            ForEach(line.optionals, id: \.optionalFieldId) { o in
                pill(o.name, fg: theme.colors.warning, bg: theme.colors.warningBg)
            }
        }
    }

    /// A bundle line lists its components (qty × name) with each component's
    /// chosen addons/optionals as sub-pills.
    private var bundleBreakdown: some View {
        VStack(alignment: .leading, spacing: 3) {
            ForEach(Array(line.bundleComponents.enumerated()), id: \.offset) { _, c in
                VStack(alignment: .leading, spacing: 2) {
                    Text("\(c.qty)× \(c.name)")
                        .font(.ui(11, .medium)).foregroundStyle(theme.colors.textSecondary)
                    if !c.addons.isEmpty || !c.optionals.isEmpty {
                        FlowLayout(spacing: 4) {
                            ForEach(c.addons, id: \.addonItemId) { a in
                                pill(a.qty > 1 ? "\(a.name) ×\(a.qty)" : a.name, fg: theme.colors.navy, bg: theme.colors.navyBg)
                            }
                            ForEach(c.optionals, id: \.optionalFieldId) { o in
                                pill(o.name, fg: theme.colors.warning, bg: theme.colors.warningBg)
                            }
                        }
                    }
                }
            }
        }
    }

    private func pill(_ text: String, fg: Color, bg: Color) -> some View {
        Text(text)
            .font(.ui(10, .semibold)).foregroundStyle(fg)
            .padding(.horizontal, 7).padding(.vertical, 2)
            .background(bg)
            .clipShape(RoundedRectangle(cornerRadius: 4, style: .continuous))
    }
}

/// A minimal wrapping (flow) layout — pills/chips that wrap to the next line.
struct FlowLayout: Layout {
    var spacing: CGFloat = 4

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let maxWidth = proposal.width ?? .infinity
        var x: CGFloat = 0, y: CGFloat = 0, rowHeight: CGFloat = 0
        for v in subviews {
            let size = v.sizeThatFits(.unspecified)
            if x > 0, x + size.width > maxWidth { x = 0; y += rowHeight + spacing; rowHeight = 0 }
            x += size.width + spacing
            rowHeight = Swift.max(rowHeight, size.height)
        }
        return CGSize(width: proposal.width ?? x, height: y + rowHeight)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x: CGFloat = 0, y: CGFloat = 0, rowHeight: CGFloat = 0
        for v in subviews {
            let size = v.sizeThatFits(.unspecified)
            if x > 0, x + size.width > bounds.width { x = 0; y += rowHeight + spacing; rowHeight = 0 }
            v.place(at: CGPoint(x: bounds.minX + x, y: bounds.minY + y), anchor: .topLeading, proposal: .unspecified)
            x += size.width + spacing
            rowHeight = Swift.max(rowHeight, size.height)
        }
    }
}

private struct QtyStepper: View {
    @Environment(\.theme) private var theme
    let qty: Int64
    let onDec: () -> Void
    let onInc: () -> Void

    var body: some View {
        HStack(spacing: Space.sm) {
            StepButton(symbol: qty <= 1 ? "trash" : "minus", action: onDec)
            Text("\(qty)").font(.ui(15, .bold))
                .foregroundStyle(theme.colors.textPrimary)
                .frame(minWidth: 18)
            StepButton(symbol: "plus", action: onInc)
        }
    }
}

private struct StepButton: View {
    @Environment(\.theme) private var theme
    let symbol: String
    let action: () -> Void

    var body: some View {
        Button {
            Haptics.selection()
            action()
        } label: {
            MadarIcon(symbol, size: 12)
                .foregroundStyle(symbol == "trash" ? theme.colors.danger : theme.colors.textPrimary)
                .frame(width: 30, height: 30)
                .background(theme.colors.surfaceAlt)
                .clipShape(Circle())
                .overlay(Circle().strokeBorder(theme.colors.border, lineWidth: 1))
        }
        .buttonStyle(.pressable(scale: 0.9))
    }
}

private struct CartFooter: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let totals: CartTotals
    let currency: String
    var checkoutLabel: String? = nil
    var checkoutIcon: String? = nil
    let onCheckout: () -> Void
    var onHold: (() -> Void)? = nil

    var body: some View {
        VStack(spacing: Space.sm) {
            TotalRow(label: t("order.subtotal"), value: Money.format(totals.subtotalMinor, currency))
            if totals.discountMinor > 0 {
                HStack {
                    Text(t("order.discount")).font(.ui(13, .medium)).foregroundStyle(theme.colors.success)
                    Spacer()
                    Text("−\(Money.format(totals.discountMinor, currency))")
                        .font(.money(13, .semibold)).foregroundStyle(theme.colors.success)
                }
            }
            TotalRow(label: t("order.tax"), value: Money.format(totals.taxMinor, currency))
            TotalRow(label: t("order.total"), value: Money.format(totals.totalMinor, currency), emphasized: true)
            HStack(spacing: Space.sm) {
                if let onHold {
                    Button { Haptics.selection(); onHold() } label: {
                        MadarIcon("tray.and.arrow.down", size: 16)
                            .foregroundStyle(theme.colors.accent)
                            .frame(width: 50, height: 50)
                            .background(theme.colors.accentBg)
                            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
                    }
                    .buttonStyle(.pressable(scale: 0.97))
                }
                MadarButton(label: checkoutLabel ?? t("order.checkout"), icon: checkoutIcon ?? "creditcard") { onCheckout() }
                    // Hardware-keyboard shortcut (iPad/Mac): ⌘↩ to check out.
                    .keyboardShortcut(.return, modifiers: .command)
            }
            .padding(.top, Space.xs)
        }
        .animation(Motion.standard, value: totals.totalMinor)
        .padding(Space.lg)
        .background(theme.colors.surface)
        .overlay(alignment: .top) {
            Rectangle().fill(theme.colors.border).frame(height: 1)
        }
    }
}

private struct TotalRow: View {
    @Environment(\.theme) private var theme
    let label: String
    let value: String
    var emphasized = false

    var body: some View {
        HStack {
            Text(label)
                .font(.ui(emphasized ? 15 : 13, emphasized ? .bold : .medium))
                .foregroundStyle(emphasized ? theme.colors.textPrimary : theme.colors.textSecondary)
            Spacer()
            Text(value)
                .font(.money(emphasized ? 18 : 13, emphasized ? .heavy : .semibold))
                // Flutter's grand total is accent-tinted (`money(18, w800, t.accent)`);
                // the lighter sub-rows stay muted.
                .foregroundStyle(emphasized ? theme.colors.accent : theme.colors.textSecondary)
                .contentTransition(.numericText())
        }
    }
}

// MARK: - Phone bottom cart bar

private struct CartBar: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let currency: String
    let onOpen: () -> Void

    var body: some View {
        if app.cartTotals.itemCount > 0 {
            Button {
                Haptics.selection()
                onOpen()
            } label: {
                HStack(spacing: Space.md) {
                    Text("\(app.cartTotals.itemCount) \(t("order.items"))")
                        .font(.ui(13, .semibold))
                        .foregroundStyle(theme.colors.textOnAccent.opacity(0.9))
                    Spacer()
                    Text(t("order.view_cart")).font(.ui(14, .bold))
                        .foregroundStyle(theme.colors.textOnAccent)
                    Text(Money.format(app.cartTotals.totalMinor, currency))
                        .font(.money(15, .heavy))
                        .foregroundStyle(theme.colors.textOnAccent)
                    MadarIcon("chevron.up", size: 12)
                        .foregroundStyle(theme.colors.textOnAccent)
                }
                .padding(.horizontal, Space.lg)
                .frame(height: 56)
                .background(theme.colors.accent)
                .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
            }
            .buttonStyle(.pressable(scale: 0.985))
            .padding(Space.md)
        }
    }
}

extension ShiftView {
    /// "EGP 500.00" — opening cash, formatted from minor units.
    func currencyDisplay(_ code: String) -> String {
        Money.format(openingCashMinor, code)
    }
}


// MARK: - Order-hub screen router
// One overlay presents whichever full-screen route is active (only one bool is
// ever true; first match wins). Replaces seven stacked `.appScreen` overlays —
// a single slide transition + a single animation keyed on the route, so the
// pushes no longer fight each other (the navigation jitter / artifacting).
private struct OrderScreenRouter: ViewModifier {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme

    private enum Route: Equatable { case closeShift, sync, history, settings, cash, shiftHistory, incoming, tickets }

    private var active: Route? {
        if app.showCloseShift { return .closeShift }
        if app.showSync { return .sync }
        if app.showHistory { return .history }
        if app.showSettings { return .settings }
        if app.showCashMovements { return .cash }
        if app.showShiftHistory { return .shiftHistory }
        if app.showIncoming { return .incoming }
        if app.showTickets { return .tickets }
        return nil
    }

    private func dismiss() {
        app.showCloseShift = false; app.showSync = false; app.showHistory = false
        app.showSettings = false; app.showCashMovements = false
        app.showShiftHistory = false; app.showIncoming = false
        app.showTickets = false
    }

    func body(content: Content) -> some View {
        content.overlay {
            ZStack {
                if let active {
                    routeView(active)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .background(theme.colors.bg.ignoresSafeArea())
                        .transition(.move(edge: .trailing))
                        .zIndex(20)
                }
            }
            .animation(Motion.standard, value: active)
        }
    }

    @ViewBuilder private func routeView(_ r: Route) -> some View {
        switch r {
        case .closeShift:   CloseShiftView(app: app)
        case .sync:         SyncView(app: app, onClose: dismiss)
        case .history:      OrderHistoryView(app: app, onClose: dismiss)
        case .settings:     SettingsView(app: app, onClose: dismiss)
        case .cash:         CashMovementsView(app: app, onClose: dismiss)
        case .shiftHistory: ShiftHistoryView(app: app, onClose: dismiss)
        case .incoming:     IncomingView(app: app, onClose: dismiss)
        case .tickets:      WaiterTicketsListView(app: app, onClose: dismiss)
        }
    }
}

extension View {
    func orderScreenRouter(app: AppModel) -> some View { modifier(OrderScreenRouter(app: app)) }
}
