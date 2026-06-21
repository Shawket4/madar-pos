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
            let wide = geo.size.width >= 760
            ZStack {
                theme.colors.bg.ignoresSafeArea()
                VStack(spacing: 0) {
                    OrderTopBar(app: app)
                    if !app.isOnline {
                        NoticeBanner(icon: "wifi.slash", text: t("chrome.offline_banner"), tone: .warning)
                            .padding(.horizontal, Space.lg)
                            .padding(.top, Space.sm)
                    }
                    if app.syncAuthPaused {
                        NoticeBanner(icon: "lock.circle", text: t("chrome.auth_paused"), tone: .danger)
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
                            CartPanel(app: app, onCheckout: { showTenderWide = true }).frame(width: 340)
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
            .sufrixSheet(isPresented: $showCart, size: .large,
                         onDismiss: { tenderInCart = false; app.dismissReceipt() }) { dismiss in
                Group {
                    if tenderInCart {
                        TenderView(app: app, onClose: { tenderInCart = false; dismiss() })
                    } else {
                        CartPanel(app: app, onClose: dismiss, onCheckout: { tenderInCart = true })
                    }
                }
            }
            // Wide: tender is its own sheet beside the cart column.
            .sufrixSheet(isPresented: $showTenderWide, size: .large,
                         onDismiss: { app.dismissReceipt() }) { dismiss in
                TenderView(app: app, onClose: dismiss)
            }
            // Item customization. The derived binding runs closeItemDetail() on
            // EVERY dismissal route — tap-out, drag-down, or the header ✕.
            .sufrixSheet(item: Binding(get: { app.detailItem },
                                       set: { if $0 == nil { app.closeItemDetail() } }),
                         size: .large) { item, dismiss in
                ItemDetailView(app: app, item: item, onClose: dismiss)
            }
            // Bundle (combo) configuration.
            .sufrixSheet(item: $app.detailBundle, size: .large) { bundle, dismiss in
                BundleDetailView(app: app, bundle: bundle, onClose: dismiss)
            }
            // More — overflow nav hub (close shift, sign out, …).
            .sufrixSheet(isPresented: $app.showMore) { _ in
                MoreDrawer(app: app)
            }
            // Held orders (drafts).
            .sufrixSheet(isPresented: $app.showDrafts) { dismiss in
                DraftsView(app: app, onClose: dismiss)
            }
            // Mid-shift Z-report preview + print.
            .sufrixSheet(isPresented: $app.showReportPreview, size: .large) { dismiss in
                ShiftReportPreviewView(app: app, onClose: dismiss)
            }
            // ── Full-screen routed screens: slide-in over the hub, own back
            // chevron. Reached from the action bar / More drawer. ────────────────
            .appScreen(isPresented: $app.showCloseShift) { _ in
                CloseShiftView(app: app)
            }
            .appScreen(isPresented: $app.showSync) { dismiss in
                SyncView(app: app, onClose: dismiss)
            }
            .appScreen(isPresented: $app.showHistory) { dismiss in
                OrderHistoryView(app: app, onClose: dismiss)
            }
            .appScreen(isPresented: $app.showSettings) { dismiss in
                SettingsView(app: app, onClose: dismiss)
            }
            .appScreen(isPresented: $app.showCashMovements) { dismiss in
                CashMovementsView(app: app, onClose: dismiss)
            }
            .appScreen(isPresented: $app.showShiftHistory) { dismiss in
                ShiftHistoryView(app: app, onClose: dismiss)
            }
            .appScreen(isPresented: $app.showDelivery) { dismiss in
                DeliveryView(app: app, onClose: dismiss)
            }
        }
        .task {
            await app.reconcileShift()
            await app.loadCatalog()
            app.refreshPending()
            await app.loadHistory()
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
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 150, maximum: 200), spacing: 14)], spacing: 14) {
                ForEach(app.bundles, id: \.id) { b in
                    BundleCard(bundle: b, currency: currency) { app.openBundleDetail(b) }
                }
            }
            .padding(Space.lg)
        }
    }
}

// MARK: - Top action bar (the only nav hub)

private struct OrderTopBar: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    private var currency: String { app.session?.currencyCode ?? "" }

    var body: some View {
        HStack(spacing: Space.sm) {
            SufrixMark(size: 32)
            if let s = app.shift {
                StatusChip(label: s.tellerName, icon: "person.fill", tone: .info)
            }
            if app.shift?.isOpen == true { ShiftStatsPill(app: app, currency: currency) }
            Spacer(minLength: 0)
            SyncChip(app: app)
            barButton(icon: "list.bullet.rectangle") { app.showHistory = true }
            barButton(icon: "gearshape") { app.refreshPending(); app.showSettings = true }
            barButton(icon: "ellipsis") { app.refreshPending(); app.showMore = true }
        }
        .padding(.horizontal, Space.lg)
        .padding(.vertical, Space.md)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) {
            Rectangle().fill(theme.colors.border).frame(height: 1)
        }
    }

    private func barButton(icon: String, action: @escaping () -> Void) -> some View {
        Button { Haptics.selection(); action() } label: {
            Image(systemName: icon)
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(theme.colors.textMuted)
                .frame(width: 34, height: 34)
                .background(theme.colors.surfaceAlt)
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                    .strokeBorder(theme.colors.borderLight, lineWidth: 1))
        }
        .buttonStyle(.pressable)
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
                    Image(systemName: icon).font(.system(size: 12, weight: .semibold))
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
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

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
                    }
                    Spacer()
                }
                .padding(Space.md)
                .background(theme.colors.surfaceAlt)
                .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
                .padding(.horizontal, Space.lg)
            }
            VStack(spacing: Space.sm) {
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
                row(icon: "bicycle", label: t("delivery.title"), tone: theme.colors.textPrimary) {
                    app.showMore = false; app.errorMessage = nil; app.showDelivery = true
                }
                row(icon: "lock", label: t("order.close_shift"), tone: theme.colors.danger) {
                    app.showMore = false; app.errorMessage = nil; app.showCloseShift = true
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
        .frame(maxWidth: 460)
        .frame(maxWidth: .infinity)
        .background(theme.colors.bg)
    }

    private func row(icon: String, label: String, tone: Color, action: @escaping () -> Void) -> some View {
        Button { Haptics.selection(); action() } label: {
            HStack(spacing: Space.md) {
                Image(systemName: icon).font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(tone).frame(width: 28)
                Text(label).font(.ui(15, .semibold)).foregroundStyle(tone)
                Spacer()
                Image(systemName: "chevron.forward").font(.system(size: 12, weight: .semibold))
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
                tab(t("order.all"), id: nil, icon: "square.grid.2x2.fill")
                if showCombos { tab(t("order.combos"), id: kCombosCategory, icon: "square.stack.3d.up.fill") }
                ForEach(categories.filter { $0.isActive }, id: \.id) { c in
                    tab(c.name, id: c.id, icon: catSymbol(app.core.categoryStyle(name: c.name, dark: theme.isDark).icon))
                }
            }
            .padding(.horizontal, Space.lg)
        }
        .frame(height: 46)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }

    private func tab(_ label: String, id: String?, icon: String) -> some View {
        let active = selected == id
        return Button {
            Haptics.selection()
            selected = id
        } label: {
            VStack(spacing: 0) {
                Spacer(minLength: 0)
                HStack(spacing: 5) {
                    Image(systemName: icon).font(.system(size: 11, weight: .semibold))
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
                tile(t("order.all"), id: nil, style: nil, fallbackIcon: "square.grid.2x2.fill")
                if showCombos { tile(t("order.combos"), id: kCombosCategory, style: nil, fallbackIcon: "square.stack.3d.up.fill") }
                ForEach(categories.filter { $0.isActive }, id: \.id) { c in
                    tile(c.name, id: c.id, style: app.core.categoryStyle(name: c.name, dark: theme.isDark), fallbackIcon: nil)
                }
            }
            .padding(.vertical, Space.sm)
        }
        .frame(width: 96)
        .background(theme.colors.surface)
    }

    private func tile(_ label: String, id: String?, style: CatStyleView?, fallbackIcon: String?) -> some View {
        let active = selected == id
        let symbol = style.map { catSymbol($0.icon) } ?? (fallbackIcon ?? "tag.fill")
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
                    Image(systemName: symbol)
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(style.map { Color(hex: $0.iconColor) } ?? theme.colors.accent)
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

    // Width-driven columns (≈150–200pt cells) so landscape never yields giant cards.
    private let columns = [GridItem(.adaptive(minimum: 150, maximum: 200), spacing: 14)]

    var body: some View {
        if items.isEmpty {
            VStack(spacing: Space.md) {
                Image(systemName: searching ? "magnifyingglass" : "tray")
                    .font(.system(size: 36, weight: .light))
                    .foregroundStyle(theme.colors.textMuted)
                Text(t(searching ? "order.empty_search" : "order.empty"))
                    .font(.ui(14)).foregroundStyle(theme.colors.textSecondary)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ScrollView {
                LazyVGrid(columns: columns, spacing: 14) {
                    ForEach(items, id: \.id) { item in
                        MenuItemCard(
                            item: item,
                            categoryName: categoryName(item.categoryId),
                            currency: currency,
                            inCartQty: cartQty(item.id)
                        ) { onAdd(item) }
                    }
                }
                .padding(Space.lg)
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
            Image(systemName: "magnifyingglass")
                .font(.system(size: 14))
                .foregroundStyle(theme.colors.textMuted)
            TextField(placeholder, text: $text)
                .textFieldStyle(.plain) // no inner macOS bezel
                .font(.ui(15))
                .foregroundStyle(theme.colors.textPrimary)
            if !text.isEmpty {
                Button {
                    text = ""
                } label: {
                    Image(systemName: "xmark.circle.fill")
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
                .strokeBorder(theme.colors.border, lineWidth: 1)
        )
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
    /// Opens the tender flow.
    var onCheckout: () -> Void

    private var currency: String { app.session?.currencyCode ?? "" }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(t("order.cart")).font(.ui(18, .heavy)).foregroundStyle(theme.colors.textPrimary)
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
                        Image(systemName: "xmark").font(.system(size: 15, weight: .semibold))
                            .foregroundStyle(theme.colors.textMuted)
                    }
                    .buttonStyle(.plain)
                    .padding(.leading, Space.sm)
                }
            }
            .padding(Space.lg)
            Rectangle().fill(theme.colors.border).frame(height: 1)

            // Held-order tabs — flip between parked carts (switching parks the
            // current one first, so nothing is lost). The bottom Hold button stays.
            if !app.drafts.isEmpty {
                HeldOrdersTabs(app: app)
            }

            if app.cartLines.isEmpty {
                VStack(spacing: Space.md) {
                    Image(systemName: "cart")
                        .font(.system(size: 34, weight: .light))
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
                CartFooter(totals: app.cartTotals, currency: currency, onCheckout: onCheckout,
                           onHold: { app.holdCart() })
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
            Image(systemName: active ? "cart.fill" : "tray.full")
                .font(.system(size: 11, weight: .semibold))
            Text(label).font(.ui(12, .semibold)).lineLimit(1)
            if count > 0 {
                Text("\(count)").font(.ui(10, .bold))
                    .padding(.horizontal, 5).padding(.vertical, 1)
                    .background(active ? theme.colors.textOnAccent.opacity(0.25) : theme.colors.surfaceAlt)
                    .clipShape(Capsule())
            }
            if let onClose {
                Button { Haptics.selection(); onClose() } label: {
                    Image(systemName: "xmark").font(.system(size: 9, weight: .bold))
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
                Image(systemName: "trash.fill")
                    .font(.system(size: 16, weight: .bold))
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
                    Text(line.name).font(.ui(14, .semibold))
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
                    .font(.money(13, .bold)).foregroundStyle(theme.colors.accent)
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
                .strokeBorder(theme.colors.border, lineWidth: 1)
        )
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
            Image(systemName: symbol)
                .font(.system(size: 12, weight: .bold))
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
    let onCheckout: () -> Void
    var onHold: (() -> Void)? = nil

    var body: some View {
        VStack(spacing: Space.sm) {
            TotalRow(label: t("order.subtotal"), value: Money.format(totals.subtotalMinor, currency))
            if totals.discountMinor > 0 {
                HStack {
                    Text(t("order.discount")).font(.ui(14, .medium)).foregroundStyle(theme.colors.success)
                    Spacer()
                    Text("−\(Money.format(totals.discountMinor, currency))")
                        .font(.money(14, .semibold)).foregroundStyle(theme.colors.success)
                }
            }
            TotalRow(label: t("order.tax"), value: Money.format(totals.taxMinor, currency))
            TotalRow(label: t("order.total"), value: Money.format(totals.totalMinor, currency), emphasized: true)
            HStack(spacing: Space.sm) {
                if let onHold {
                    Button { Haptics.selection(); onHold() } label: {
                        Image(systemName: "tray.and.arrow.down").font(.system(size: 16, weight: .semibold))
                            .foregroundStyle(theme.colors.accent)
                            .frame(width: 50, height: 50)
                            .background(theme.colors.accentBg)
                            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
                    }
                    .buttonStyle(.pressable(scale: 0.97))
                }
                SufrixButton(label: t("order.checkout"), icon: "creditcard") { onCheckout() }
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
                .font(.ui(emphasized ? 16 : 14, emphasized ? .bold : .medium))
                .foregroundStyle(emphasized ? theme.colors.textPrimary : theme.colors.textSecondary)
            Spacer()
            Text(value)
                .font(.money(emphasized ? 18 : 14, emphasized ? .heavy : .semibold))
                .foregroundStyle(emphasized ? theme.colors.textPrimary : theme.colors.textSecondary)
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
                    Image(systemName: "chevron.up").font(.system(size: 12, weight: .bold))
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

extension View {
    /// Inject the app chrome a modally-presented screen needs. A `.sheet` /
    /// `.fullScreenCover` is hosted in a FRESH environment that does NOT inherit
    /// the presenter's `\.theme`, `\.localize`, or `\.layoutDirection`, so each
    /// modal must re-inject them — including the RTL flip, or Arabic sheets would
    /// render left-to-right while the rest of the app mirrors.
    func modalChrome(_ app: AppModel, _ theme: SufrixTheme, _ t: @escaping (String) -> String) -> some View {
        environment(\.theme, theme)
            .environment(\.localize, t)
            .environment(\.layoutDirection, app.isRTL ? .rightToLeft : .leftToRight)
            .toastHost(app)
    }
}
