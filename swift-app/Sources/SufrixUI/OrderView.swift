// Order screen — the heart of the POS. Per the design language the order screen's
// action bar is the only nav hub (no tabs/shells). Browse the branch-effective
// catalog (served from the local mirror, offline-safe) and build a cart: tap an
// item to add it, adjust quantities, see live totals. On wide layouts (iPad /
// desktop) the cart is a column beside the grid; on phones it's a bottom bar that
// opens a sheet. Tender lands in the next phase.
import SwiftUI

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
            .filter { search.isEmpty || $0.name.localizedCaseInsensitiveContains(search) }
    }

    var body: some View {
        GeometryReader { geo in
            let wide = geo.size.width >= 760
            ZStack {
                theme.colors.bg.ignoresSafeArea()
                VStack(spacing: 0) {
                    OrderTopBar(app: app)
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
            // Phone: the cart sheet swaps its content to tender on checkout.
            .sheet(isPresented: $showCart, onDismiss: { tenderInCart = false; app.dismissReceipt() }) {
                Group {
                    if tenderInCart {
                        TenderView(app: app, onClose: { tenderInCart = false; showCart = false })
                    } else {
                        CartPanel(app: app, onClose: { showCart = false }, onCheckout: { tenderInCart = true })
                    }
                }
                .environment(\.theme, theme)
                .environment(\.localize, t)
            }
            // Wide: tender is a root sheet beside the cart column.
            .sheet(isPresented: $showTenderWide, onDismiss: { app.dismissReceipt() }) {
                TenderView(app: app, onClose: { showTenderWide = false })
                    .environment(\.theme, theme)
                    .environment(\.localize, t)
            }
            // Close-shift flow over the order screen.
            #if os(iOS)
            .fullScreenCover(isPresented: $app.showCloseShift) {
                CloseShiftView(app: app)
                    .environment(\.theme, theme)
                    .environment(\.localize, t)
            }
            #else
            .sheet(isPresented: $app.showCloseShift) {
                CloseShiftView(app: app)
                    .frame(minWidth: 480, minHeight: 600)
                    .environment(\.theme, theme)
                    .environment(\.localize, t)
            }
            #endif
            // Sync center.
            .sheet(isPresented: $app.showSync) {
                SyncView(app: app, onClose: { app.showSync = false })
                    .frame(minWidth: 420, minHeight: 520)
                    .environment(\.theme, theme)
                    .environment(\.localize, t)
            }
            // Order history.
            .sheet(isPresented: $app.showHistory) {
                OrderHistoryView(app: app, onClose: { app.showHistory = false })
                    .frame(minWidth: 460, minHeight: 560)
                    .environment(\.theme, theme)
                    .environment(\.localize, t)
            }
            // Item customization.
            .sheet(item: $app.detailItem) { item in
                ItemDetailView(app: app, item: item, onClose: { app.closeItemDetail() })
                    .frame(minWidth: 460, minHeight: 600)
                    .environment(\.theme, theme)
                    .environment(\.localize, t)
            }
            // Settings.
            .sheet(isPresented: $app.showSettings) {
                SettingsView(app: app, onClose: { app.showSettings = false })
                    .frame(minWidth: 440, minHeight: 560)
                    .environment(\.theme, theme)
                    .environment(\.localize, t)
            }
        }
        .task {
            await app.reconcileShift()
            await app.loadCatalog()
        }
    }

    @ViewBuilder private func catalogColumn(wide: Bool) -> some View {
        if wide {
            // Tablet/desktop: a vertical category rail beside the search + grid.
            HStack(spacing: 0) {
                CategoryRail(categories: app.categories, selected: $selectedCategory)
                Rectangle().fill(theme.colors.borderLight).frame(width: 1)
                VStack(spacing: 0) { searchAndGrid }
            }
        } else {
            // Phone: a horizontal underline-tab strip above the search + grid.
            VStack(spacing: 0) {
                CategoryTabs(categories: app.categories, selected: $selectedCategory)
                searchAndGrid
            }
        }
    }

    @ViewBuilder private var searchAndGrid: some View {
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

// MARK: - Top action bar (the only nav hub)

private struct OrderTopBar: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    var body: some View {
        HStack(spacing: Space.md) {
            SufrixMark(size: 32)
            if let s = app.shift {
                StatusChip(label: s.tellerName, icon: "person.fill", tone: .info)
            }
            Spacer(minLength: 0)
            Button {
                Haptics.selection()
                app.showHistory = true
            } label: {
                Image(systemName: "list.bullet.rectangle")
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(theme.colors.textMuted)
            }
            .buttonStyle(.pressable)
            Button {
                Haptics.selection()
                app.loadOutbox()
                app.showSync = true
            } label: {
                HStack(spacing: 5) {
                    Image(systemName: app.pendingCount > 0 ? "arrow.triangle.2.circlepath" : "checkmark.icloud")
                    if app.pendingCount > 0 { Text("\(app.pendingCount)") }
                }
                .font(.ui(13, .semibold))
                .foregroundStyle(app.pendingCount > 0 ? theme.colors.warning : theme.colors.textMuted)
            }
            .buttonStyle(.pressable)
            Button {
                Haptics.selection()
                app.refreshPending()
                app.showSettings = true
            } label: {
                Image(systemName: "gearshape")
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(theme.colors.textMuted)
            }
            .buttonStyle(.pressable)
            Button {
                Haptics.selection()
                app.errorMessage = nil
                app.showCloseShift = true
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "lock")
                    Text(t("order.close_shift"))
                }
                .font(.ui(13, .semibold))
                .foregroundStyle(theme.colors.textSecondary)
            }
            .buttonStyle(.pressable)
            Button {
                Haptics.selection()
                // You can't sign out mid-shift — close the drawer first.
                if app.hasOpenShift {
                    app.flagError(t("settings.sign_out_shift_open"))
                } else {
                    app.signOut()
                }
            } label: {
                Image(systemName: "rectangle.portrait.and.arrow.right")
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(theme.colors.textMuted)
            }
            .buttonStyle(.pressable)
        }
        .padding(.horizontal, Space.lg)
        .padding(.vertical, Space.md)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) {
            Rectangle().fill(theme.colors.border).frame(height: 1)
        }
    }
}

// MARK: - Category strip

/// Phone: a horizontal underline-tab strip (the Flutter CategoryStrip).
private struct CategoryTabs: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let categories: [CategoryView]
    @Binding var selected: String?

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: Space.lg) {
                tab(t("order.all"), id: nil)
                ForEach(categories.filter { $0.isActive }, id: \.id) { c in
                    tab(c.name, id: c.id)
                }
            }
            .padding(.horizontal, Space.lg)
        }
        .frame(height: 46)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }

    private func tab(_ label: String, id: String?) -> some View {
        let active = selected == id
        return Button {
            Haptics.selection()
            selected = id
        } label: {
            VStack(spacing: 0) {
                Spacer(minLength: 0)
                Text(label)
                    .font(.ui(13, active ? .bold : .medium))
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

/// Tablet/desktop: a 94pt vertical category rail (the Flutter CategoryRail).
private struct CategoryRail: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let categories: [CategoryView]
    @Binding var selected: String?

    var body: some View {
        ScrollView(showsIndicators: false) {
            VStack(spacing: 3) {
                tile(t("order.all"), id: nil)
                ForEach(categories.filter { $0.isActive }, id: \.id) { c in
                    tile(c.name, id: c.id)
                }
            }
            .padding(.vertical, Space.sm)
        }
        .frame(width: 96)
        .background(theme.colors.surface)
    }

    private func tile(_ label: String, id: String?) -> some View {
        let active = selected == id
        return Button {
            Haptics.selection()
            selected = id
        } label: {
            Text(label)
                .font(.ui(10, active ? .bold : .medium))
                .foregroundStyle(active ? theme.colors.accent : theme.colors.textSecondary)
                .multilineTextAlignment(.center)
                .lineLimit(2)
                .frame(maxWidth: .infinity)
                .padding(.vertical, Space.md)
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
                                onEdit: { app.editCartLine(line) }
                            )
                        }
                    }
                    .padding(Space.lg)
                }
                CartFooter(totals: app.cartTotals, currency: currency, onCheckout: onCheckout)
            }
        }
        .background(theme.colors.bg)
    }
}

private struct CartLineRow: View {
    @Environment(\.theme) private var theme
    let line: CartLineView
    let currency: String
    let onDec: () -> Void
    let onInc: () -> Void
    var onEdit: (() -> Void)? = nil

    private var hasModifiers: Bool {
        line.sizeLabel != nil || !line.addons.isEmpty || !line.optionals.isEmpty
    }

    var body: some View {
        HStack(spacing: Space.md) {
            VStack(alignment: .leading, spacing: 4) {
                Text(line.name).font(.ui(14, .semibold))
                    .foregroundStyle(theme.colors.textPrimary).lineLimit(1)
                if hasModifiers { modifierPills }
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

    private func pill(_ text: String, fg: Color, bg: Color) -> some View {
        Text(text)
            .font(.ui(10, .semibold)).foregroundStyle(fg)
            .padding(.horizontal, 7).padding(.vertical, 2)
            .background(bg)
            .clipShape(RoundedRectangle(cornerRadius: 4, style: .continuous))
    }
}

/// A minimal wrapping (flow) layout — pills/chips that wrap to the next line.
private struct FlowLayout: Layout {
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
            SufrixButton(label: t("order.checkout"), icon: "creditcard") { onCheckout() }
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
