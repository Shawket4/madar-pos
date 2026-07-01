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
    @State private var showFireDetails = false

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
            // New ticket → collect dine-in details first; a round fires straight away.
            if app.activeTicketId == nil { showFireDetails = true }
            else { Task { await app.fireOrAddRound() } }
        } else if wide {
            showTenderWide = true
        } else {
            tenderInCart = true
        }
    }

    private var visibleItems: [MenuItemView] {
        // Single pass — no intermediate arrays per body eval.
        app.menuItems.filter { item in
            item.isActive
                && (selectedCategory == nil || item.categoryId == selectedCategory)
                && (search.isEmpty
                    || item.name.localizedCaseInsensitiveContains(search)
                    || (item.description?.localizedCaseInsensitiveContains(search) ?? false))
        }
    }

    var body: some View {
        GeometryReader { geo in
            let wide = geo.size.width >= Responsive.wide
            ZStack {
                theme.colors.bg.ignoresSafeArea()
                HStack(spacing: 0) {
                    // Persistent leading nav rail on tablet; phone collapses it into
                    // a top "options" toggle (the rail is too cramped on a phone).
                    if wide {
                        NavRail(app: app, wide: wide)
                        Rectangle().fill(theme.colors.border).frame(width: 1)
                    }
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
                // More — overflow nav hub that expands as a panel right next to the
                // rail (offset by the rail width). Scrim taps to dismiss. RTL-aware.
                if app.showMore {
                    Color.black.opacity(Opacity.scrim)
                        .ignoresSafeArea()
                        .onTapGesture { withAnimation(Motion.standard) { app.showMore = false } }
                        .transition(.opacity)
                    VStack(spacing: 0) {
                        if wide { Spacer(minLength: 0) }
                        HStack(spacing: 0) {
                            // Pops from the More control: by the rail's More tile on
                            // tablet (bottom-left); by the top-bar toggle on phone.
                            Color.clear.frame(width: wide ? 80 + Space.sm : Space.sm)
                            MoreDrawer(app: app, wide: wide)
                            Spacer(minLength: 0)
                        }
                        if !wide { Spacer(minLength: 0) }
                    }
                    .padding(.vertical, Space.sm)
                    .padding(.top, wide ? 0 : 56)
                    .transition(.move(edge: .leading))
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
                                      if app.isWaiterDevice {
                                          if app.activeTicketId == nil { dismiss(); showFireDetails = true }
                                          else { Task { await app.fireOrAddRound(); dismiss() } }
                                      } else { tenderInCart = true }
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
            // Waiter fire-details — optional dine-in capture before firing.
            .madarSheet(isPresented: $showFireDetails, size: .hug, maxWidth: 480) { dismiss in
                FireDetailsView(app: app, onDone: dismiss)
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
            // Held orders (drafts).
            .madarSheet(isPresented: $app.showDrafts, maxWidth: 600) { dismiss in
                DraftsView(app: app, onClose: dismiss)
            }
            // Mid-shift Z-report preview + print.
            .madarSheet(isPresented: $app.showReportPreview, size: .large, maxWidth: 600) { dismiss in
                ShiftReportPreviewView(app: app, onClose: dismiss)
            }
            // A PAST shift's Z-report preview + print (tapped from Past Shifts).
            .madarSheet(isPresented: $app.showPastReportPreview, size: .large, maxWidth: 600) { dismiss in
                ShiftReportPreviewView(app: app, report: app.previewShiftReport, onClose: dismiss)
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
        // Categories sit on TOP of the menu (a horizontal tab strip) at every width —
        // the old vertical side rail is gone. On wide, the cart panel lives in the
        // parent HStack; here we only lay out the catalog.
        VStack(spacing: 0) {
            CategoryTabs(app: app, categories: app.categories, selected: $selectedCategory, showCombos: !app.bundles.isEmpty)
            searchAndGrid
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

// MARK: - Side navigation rail

/// A leading-edge nav destination: glyph + label + tap.
private struct NavDest {
    let glyph: String
    let label: String
    var hasNew = false
    let action: () -> Void
}

/// A labelled group of rail destinations — the rail and the phone drawer both
/// render these as a caption + its tiles, so the two stay in lockstep.
private struct NavSection {
    let title: String
    let items: [NavDest]
}

/// The persistent side rail — secondary destinations exposed as tappable tiles on
/// the leading edge instead of buried in a bottom sheet. Frequent destinations
/// scroll in the middle; settings + more pin to the bottom.
private struct NavRail: View {
    @ObservedObject var app: AppModel
    let wide: Bool
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    var body: some View {
        VStack(spacing: 0) {
            // The Madar wordmark ("madar", no orbit); the asset catalog swaps to the
            // paper variant automatically in dark mode. Sized by width for the rail.
            Image("MadarLockup").resizable().scaledToFit().frame(width: 64)
                .padding(.vertical, Space.sm)
            divider
            ScrollView(showsIndicators: false) {
                VStack(spacing: 2) {
                    // Task categories — each group under its caption.
                    ForEach(Array(sections.enumerated()), id: \.offset) { item in
                        NavRailCaption(title: item.element.title, topPad: item.offset == 0 ? 0 : Space.sm)
                        ForEach(item.element.items, id: \.label) { NavRailItem(dest: $0) }
                    }
                }
                .padding(.vertical, Space.xs)
            }
            divider
            // System utilities — pinned to the footer.
            NavRailCaption(title: footer.title, topPad: 0)
            ForEach(footer.items, id: \.label) { NavRailItem(dest: $0) }
        }
        .padding(.vertical, Space.sm)
        .frame(width: 80)
        .frame(maxHeight: .infinity)
        .background(theme.colors.surface)
    }

    private var divider: some View {
        Rectangle().fill(theme.colors.borderLight).frame(height: 1)
            .padding(.horizontal, Space.md).padding(.vertical, Space.xs)
    }

    // Task categories scroll in the middle; system utilities (incl. More) pin to
    // the footer.
    private var sections: [NavSection] { orderNavSections(app, t) }
    private var footer: NavSection { orderNavSystem(app, t, includeMore: true) }
}

/// A tiny uppercase caption heading a rail section — what turns the flat list
/// into intuitive, scannable categories.
private struct NavRailCaption: View {
    let title: String
    var topPad: CGFloat = 0
    @Environment(\.theme) private var theme

    var body: some View {
        Text(title.uppercased())
            .font(.ui(8, .semibold))
            .tracking(0.6)
            .foregroundStyle(theme.colors.textMuted)
            .lineLimit(1)
            .frame(maxWidth: .infinity)
            .padding(.top, topPad)
            .padding(.bottom, 2)
            .padding(.horizontal, 2)
    }
}

// Shared nav destinations — the rail (tablet) and the phone "options" drawer both
// build these groups here, so the two stay in lockstep. Task categories scroll;
// system utilities (orderNavSystem) pin to the footer.
@MainActor
private func orderNavSections(_ app: AppModel, _ t: (String) -> String) -> [NavSection] {
    if app.isWaiterDevice {
        // A waiter device only handles tickets — the single Orders group.
        return [
            NavSection(title: t("nav.section.orders"), items: [
                NavDest(glyph: "fork.knife", label: t("waiter.tickets"), hasNew: app.ticketsHasNew) { app.clearTicketsBadge(); Task { await app.loadOpenTickets() }; app.showTickets = true },
            ]),
        ]
    }
    return [
        // Orders — the order lifecycle: inbox → held → completed → look-up.
        NavSection(title: t("nav.section.orders"), items: [
            NavDest(glyph: "bicycle", label: t("nav.incoming"), hasNew: app.deliveryHasNew || app.ticketsHasNew) {
                app.errorMessage = nil
                app.clearDeliveryBadge(); app.clearTicketsBadge()
                Task { await app.loadDeliveryOrders(); await app.loadOpenTickets() }
                app.showIncoming = true
            },
            NavDest(glyph: "tray.full", label: t("drafts.title")) { app.loadDrafts(); app.showDrafts = true },
            NavDest(glyph: "list.bullet.rectangle", label: t("nav.history")) { app.showHistory = true },
            NavDest(glyph: "magnifyingglass", label: t("search.title")) { app.showOrderSearch = true },
        ]),
        // Money — cash drawer & shift reconciliation.
        NavSection(title: t("nav.section.money"), items: [
            NavDest(glyph: "banknote", label: t("cash.title")) { app.errorMessage = nil; app.showCashMovements = true },
            NavDest(glyph: "clock.arrow.circlepath", label: t("shifts.title")) { app.showShiftHistory = true },
            NavDest(glyph: "printer", label: t("shift.print_report")) { app.openShiftReportPreview() },
        ]),
    ]
}

// System utilities — always reachable. The rail footer includes More; the phone
// drawer drops it (the drawer IS More).
@MainActor
private func orderNavSystem(_ app: AppModel, _ t: (String) -> String, includeMore: Bool) -> NavSection {
    var items: [NavDest] = [
        NavDest(glyph: "arrow.triangle.2.circlepath", label: t("sync.title")) { app.loadOutbox(); app.showSync = true },
        orderNavSettings(app, t),
    ]
    if includeMore {
        items.append(NavDest(glyph: "ellipsis", label: t("chrome.more")) { app.refreshPending(); withAnimation(Motion.standard) { app.showMore = true } })
    }
    return NavSection(title: t("nav.section.system"), items: items)
}

@MainActor
private func orderNavSettings(_ app: AppModel, _ t: (String) -> String) -> NavDest {
    NavDest(glyph: "gearshape", label: t("settings.title")) { app.refreshPending(); app.showSettings = true }
}

private struct NavRailItem: View {
    let dest: NavDest
    @Environment(\.theme) private var theme
    @State private var pulse = false

    var body: some View {
        Button { Haptics.selection(); dest.action() } label: {
            VStack(spacing: 4) {
                MadarIcon(dest.glyph, size: 18)
                    .foregroundStyle(dest.hasNew ? theme.colors.accent : theme.colors.textSecondary)
                    .frame(width: 36, height: 36)
                    .background(dest.hasNew ? theme.colors.accentBg : theme.colors.surfaceAlt)
                    .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
                    .overlay(alignment: .topTrailing) {
                        // A live SSE event for this module → a pulsing accent dot.
                        if dest.hasNew {
                            Circle().fill(theme.colors.accent)
                                .frame(width: 8, height: 8)
                                .padding(3)
                                .opacity(pulse ? 0.25 : 1)
                                .animation(.easeInOut(duration: 0.75).repeatForever(autoreverses: true), value: pulse)
                        }
                    }
                Text(dest.label)
                    .font(.ui(10, dest.hasNew ? .semibold : .medium))
                    .foregroundStyle(dest.hasNew ? theme.colors.accent : theme.colors.textSecondary)
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 6)
            .contentShape(Rectangle())
        }
        .buttonStyle(.pressable)
        .onAppear { pulse = dest.hasNew }
        .onChange(of: dest.hasNew) { pulse = $0 }
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
            // Phone: no side rail — a leading "options" toggle opens the nav drawer.
            if !wide {
                Button { app.refreshPending(); withAnimation(Motion.standard) { app.showMore = true } } label: {
                    MadarIcon("line.3.horizontal", size: 18)
                        .foregroundStyle(theme.colors.textPrimary)
                        .frame(width: 36, height: 36)
                        .background(theme.colors.surfaceAlt)
                        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
                        .overlay(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                            .strokeBorder(theme.colors.borderLight, lineWidth: 1))
                }
                .buttonStyle(.pressable)
            }
            // Status — teller (wide), live shift totals, and sync state.
            if !app.isWaiterDevice {
                if wide, let s = app.shift {
                    StatusChip(label: s.tellerName, icon: "person.fill", tone: .info)
                }
                if app.shift?.isOpen == true { ShiftStatsPill(app: app, currency: currency) }
            }
            Spacer(minLength: 0)
            SyncChip(app: app)
            syncDataButton
        }
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
        VStack(spacing: Space.sm) {
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
                .frame(maxWidth: .infinity)
                .background(theme.colors.surfaceAlt)
                .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
            }
            // Few items on tablet → hug; the full nav on phone → cap + scroll.
            if wide {
                VStack(spacing: Space.sm) { drawerRows }
            } else {
                ScrollView(showsIndicators: false) { VStack(spacing: Space.sm) { drawerRows } }
                    .frame(maxHeight: 460)
            }
        }
        .padding(Space.sm)
        .frame(width: 260)
        .background(theme.colors.surface)
        .clipShape(RoundedRectangle(cornerRadius: Radii.lg, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Radii.lg, style: .continuous).strokeBorder(theme.colors.borderLight, lineWidth: 1))
        .elevation(.raised)
    }

    private var phoneSystem: NavSection { orderNavSystem(app, t, includeMore: false) }

    @ViewBuilder private var drawerRows: some View {
        // Phone: the rail's grouped destinations live here (no rail on phone) — each
        // task category under its caption, then System. Tablet shows only the
        // destructive rows (the groups are exposed in the rail).
        if !wide {
            ForEach(Array(orderNavSections(app, t).enumerated()), id: \.offset) { item in
                caption(item.element.title)
                ForEach(Array(item.element.items.enumerated()), id: \.offset) { sub in
                    row(icon: sub.element.glyph, label: sub.element.label, tone: theme.colors.textPrimary) {
                        app.showMore = false; sub.element.action()
                    }
                }
            }
            caption(phoneSystem.title)
            ForEach(Array(phoneSystem.items.enumerated()), id: \.offset) { sub in
                row(icon: sub.element.glyph, label: sub.element.label, tone: theme.colors.textPrimary) {
                    app.showMore = false; sub.element.action()
                }
            }
        }
        if !app.isWaiterDevice {
            row(icon: "lock", label: t("order.close_shift"), tone: theme.colors.danger) {
                app.showMore = false; app.errorMessage = nil; app.showCloseShift = true
            }
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

    // A left-aligned uppercase caption heading a drawer group — mirrors the rail's
    // section captions so phone and tablet read the same.
    private func caption(_ title: String) -> some View {
        Text(title.uppercased())
            .font(.ui(10, .semibold))
            .tracking(0.8)
            .foregroundStyle(theme.colors.textMuted)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, Space.xs)
            .padding(.top, Space.xs)
            .padding(.bottom, 2)
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
                .multilineTextAlignment(.center)
                .frame(minWidth: 24, alignment: .center)
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
            // Prominent total block — tinted teal, the figure tellers look at. The
            // sub-rows above stay light so the grand total carries the weight.
            HStack {
                Text(t("order.total")).font(.ui(14, .bold)).foregroundStyle(theme.colors.accent)
                Spacer()
                Text(Money.format(totals.totalMinor, currency))
                    .font(.money(20, .heavy)).foregroundStyle(theme.colors.accent)
            }
            .padding(.horizontal, Space.md)
            .padding(.vertical, Space.md)
            .background(theme.colors.accentBg)
            .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
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

    private enum Route: Equatable { case closeShift, sync, history, search, settings, cash, shiftHistory, incoming, tickets }

    private var active: Route? {
        if app.showCloseShift { return .closeShift }
        if app.showSync { return .sync }
        if app.showHistory { return .history }
        if app.showOrderSearch { return .search }
        if app.showSettings { return .settings }
        if app.showCashMovements { return .cash }
        if app.showShiftHistory { return .shiftHistory }
        if app.showIncoming { return .incoming }
        if app.showTickets { return .tickets }
        return nil
    }

    private func dismiss() {
        app.showCloseShift = false; app.showSync = false; app.showHistory = false
        app.showOrderSearch = false
        app.showSettings = false; app.showCashMovements = false
        app.showShiftHistory = false; app.showIncoming = false
        app.showTickets = false
    }

    func body(content: Content) -> some View {
        content.overlay {
            ZStack {
                if active != nil {
                    // Input firewall — a fullscreen, invisible hit-test barrier that
                    // swallows every touch while a full-screen route is up. It sits
                    // BELOW the routeView in this ZStack (lower zIndex), so the route's
                    // own controls (back button, list rows) still receive taps, but any
                    // tap that lands on the route yet MISSES a control is eaten here
                    // instead of falling through to a hidden NavRail tile / catalog /
                    // cart at the same pixel underneath. The back buttons sit top-left,
                    // right over the rail, which is exactly where the fall-through
                    // misfires. The .madarSheet overlays already carry their own
                    // hit-blocking scrim; the routed screens (this overlay) did not.
                    Color.clear
                        .contentShape(Rectangle())
                        .onTapGesture {}
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .ignoresSafeArea()
                        .zIndex(10)
                }
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
        // Global receipt preview — shown over ANY route, so the shared ReceiptPaper +
        // Print sheet is reachable from the Order History reprint AND a past-shift
        // order tap (and anywhere else), printer connected or not.
        .madarSheet(item: $app.previewReceipt, size: .large) { r, dismiss in
            ReceiptPreviewSheet(app: app, receipt: r, onClose: dismiss)
        }
    }

    @ViewBuilder private func routeView(_ r: Route) -> some View {
        switch r {
        case .closeShift:   CloseShiftView(app: app)
        case .sync:         SyncView(app: app, onClose: dismiss)
        case .history:      OrderHistoryView(app: app, onClose: dismiss)
        case .search:       OrderSearchView(app: app, onClose: dismiss)
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

/// Dine-in capture before a waiter fires a NEW ticket: customer, table, covers,
/// kitchen notes — all optional, all now passed to the core (was firing blank).
private struct FireDetailsView: View {
    @ObservedObject var app: AppModel
    let onDone: () -> Void
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    @State private var customer = ""
    @State private var table = ""
    @State private var notes = ""
    @State private var covers = 0

    var body: some View {
        VStack(alignment: .leading, spacing: Space.md) {
            Text(t("waiter.fire")).font(.ui(22, .bold)).foregroundStyle(theme.colors.textPrimary)
            MadarTextField(placeholder: t("waiter.customer_optional"), text: $customer, icon: "person")
            MadarTextField(placeholder: t("waiter.table"), text: $table, icon: "tablecells")
            HStack(spacing: Space.md) {
                Text(t("waiter.covers")).font(.ui(15, .semibold)).foregroundStyle(theme.colors.textSecondary)
                Spacer()
                stepBox("minus") { if covers > 0 { covers -= 1 } }
                Text("\(covers)").font(.ui(17, .semibold)).foregroundStyle(theme.colors.textPrimary).frame(minWidth: 28)
                stepBox("plus") { covers += 1 }
            }
            MadarTextField(placeholder: t("order.notes_hint"), text: $notes, icon: "text.bubble")
            MadarButton(label: t("waiter.fire"), icon: "paperplane.fill", loading: app.isBusy) {
                Task {
                    await app.fireOrAddRound(
                        tableId: table.isEmpty ? nil : table,
                        customerName: customer.isEmpty ? nil : customer,
                        notes: notes.isEmpty ? nil : notes,
                        guestCount: covers > 0 ? Int32(covers) : nil)
                    onDone()
                }
            }
        }
        .padding(Space.lg)
    }

    private func stepBox(_ icon: String, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            MadarIcon(icon, size: IconSize.md).foregroundStyle(theme.colors.textPrimary)
                .frame(width: 36, height: 36)
                .background(theme.colors.surfaceAlt)
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                    .strokeBorder(theme.colors.border, lineWidth: 1))
        }.buttonStyle(.plain)
    }
}
