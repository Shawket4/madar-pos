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
                    if wide {
                        HStack(spacing: 0) {
                            catalogColumn
                            Rectangle().fill(theme.colors.border).frame(width: 1)
                            CartPanel(app: app, onCheckout: { showTenderWide = true }).frame(width: 340)
                        }
                    } else {
                        catalogColumn
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
        }
        .task {
            await app.reconcileShift()
            await app.loadCatalog()
        }
    }

    private var catalogColumn: some View {
        VStack(spacing: 0) {
            CategoryStrip(categories: app.categories, selected: $selectedCategory)
            SearchField(text: $search, placeholder: t("order.search"))
                .padding(.horizontal, Space.lg)
                .padding(.bottom, Space.sm)
            ItemGridOrEmpty(items: visibleItems, currency: currency, searching: !search.isEmpty) { item in
                app.addToCart(item)
            }
        }
        .frame(maxWidth: .infinity)
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
                app.signOut()
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

private struct CategoryStrip: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let categories: [CategoryView]
    @Binding var selected: String?

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: Space.sm) {
                CategoryChip(label: t("order.all"), active: selected == nil) {
                    selected = nil
                }
                ForEach(categories.filter { $0.isActive }, id: \.id) { c in
                    CategoryChip(label: c.name, active: selected == c.id) {
                        selected = c.id
                    }
                }
            }
            .padding(.horizontal, Space.lg)
            .padding(.vertical, Space.md)
        }
    }
}

private struct CategoryChip: View {
    @Environment(\.theme) private var theme
    let label: String
    let active: Bool
    let action: () -> Void

    var body: some View {
        Button {
            Haptics.selection()
            action()
        } label: {
            Text(label)
                .font(.ui(13, .semibold))
                .foregroundStyle(active ? theme.colors.textOnAccent : theme.colors.textSecondary)
                .padding(.horizontal, Space.lg)
                .padding(.vertical, Space.sm)
                .background(active ? theme.colors.accent : theme.colors.surface)
                .overlay(
                    Capsule().strokeBorder(active ? Color.clear : theme.colors.border, lineWidth: 1)
                )
                .clipShape(Capsule())
        }
        .buttonStyle(.pressable)
        .animation(Motion.standard, value: active)
    }
}

// MARK: - Item grid

private struct ItemGridOrEmpty: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let items: [MenuItemView]
    let currency: String
    let searching: Bool
    let onAdd: (MenuItemView) -> Void

    private let columns = [GridItem(.adaptive(minimum: 150), spacing: Space.md)]

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
                LazyVGrid(columns: columns, spacing: Space.md) {
                    ForEach(items, id: \.id) { item in
                        ItemCard(item: item, currency: currency) { onAdd(item) }
                    }
                }
                .padding(Space.lg)
            }
        }
    }
}

private struct ItemCard: View {
    @Environment(\.theme) private var theme
    let item: MenuItemView
    let currency: String
    let onAdd: () -> Void

    var body: some View {
        Button {
            Haptics.impact()
            onAdd()
        } label: {
            VStack(alignment: .leading, spacing: Space.sm) {
                Monogram(name: item.name)
                Text(item.name)
                    .font(.ui(14, .semibold))
                    .foregroundStyle(theme.colors.textPrimary)
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)
                    .frame(maxWidth: .infinity, alignment: .leading)
                Text(Money.format(item.basePriceMinor, currency))
                    .font(.money(14, .bold))
                    .foregroundStyle(theme.colors.accent)
            }
            .padding(Space.md)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(theme.colors.surface)
            .overlay(
                RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                    .strokeBorder(theme.colors.border, lineWidth: 1)
            )
            .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
        }
        .buttonStyle(.pressable)
    }
}

/// A branded image stand-in — the item's initial on a tinted tile. (Real menu
/// images get an async loader in a later polish phase, added to both platforms.)
private struct Monogram: View {
    @Environment(\.theme) private var theme
    let name: String

    private var initial: String {
        String(name.trimmingCharacters(in: .whitespaces).prefix(1)).uppercased()
    }

    var body: some View {
        RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
            .fill(theme.colors.accentBg)
            .aspectRatio(1.4, contentMode: .fit)
            .overlay(
                Text(initial.isEmpty ? "•" : initial)
                    .font(.ui(28, .heavy))
                    .foregroundStyle(theme.colors.accent.opacity(0.7))
            )
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
        .padding(.horizontal, Space.lg)
        .padding(.vertical, 12)
        .background(theme.colors.surface)
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
                        ForEach(app.cartLines, id: \.itemId) { line in
                            CartLineRow(
                                line: line, currency: currency,
                                onDec: { app.setCartQty(line.itemId, line.qty - 1) },
                                onInc: { app.setCartQty(line.itemId, line.qty + 1) }
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

    var body: some View {
        HStack(spacing: Space.md) {
            VStack(alignment: .leading, spacing: 2) {
                Text(line.name).font(.ui(14, .semibold))
                    .foregroundStyle(theme.colors.textPrimary).lineLimit(1)
                Text(Money.format(line.lineTotalMinor, currency))
                    .font(.money(13, .bold)).foregroundStyle(theme.colors.accent)
            }
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
            TotalRow(label: t("order.tax"), value: Money.format(totals.taxMinor, currency))
            TotalRow(label: t("order.total"), value: Money.format(totals.totalMinor, currency), emphasized: true)
            SufrixButton(label: t("order.checkout"), icon: "creditcard") { onCheckout() }
                .padding(.top, Space.xs)
        }
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
