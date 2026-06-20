// Order screen — the heart of the POS. Per the design language the order screen's
// action bar is the only nav hub (no tabs/shells). This phase: browse the
// branch-effective catalog (category strip + item grid), served from the local
// mirror so it works offline. Tap-to-cart + tender land in the next phases.
import SwiftUI

struct OrderView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t

    /// `nil` = the "All" pseudo-category.
    @State private var selectedCategory: String?
    @State private var search = ""

    private var currency: String { app.session?.currencyCode ?? "" }

    private var visibleItems: [MenuItemView] {
        app.menuItems
            .filter { $0.isActive }
            .filter { selectedCategory == nil || $0.categoryId == selectedCategory }
            .filter { search.isEmpty || $0.name.localizedCaseInsensitiveContains(search) }
    }

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            VStack(spacing: 0) {
                OrderTopBar(app: app)
                CategoryStrip(categories: app.categories, selected: $selectedCategory)
                SearchField(text: $search, placeholder: t("order.search"))
                    .padding(.horizontal, Space.lg)
                    .padding(.bottom, Space.sm)
                ItemGridOrEmpty(items: visibleItems, currency: currency, searching: !search.isEmpty)
            }
        }
        // Reconcile the shift (catches a dashboard force-close) and load the
        // catalog (fresh when online, cached otherwise) on appear.
        .task {
            await app.reconcileShift()
            await app.loadCatalog()
        }
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
                app.signOut()
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "rectangle.portrait.and.arrow.right")
                    Text(t("home.sign_out"))
                }
                .font(.ui(13, .semibold))
                .foregroundStyle(theme.colors.textSecondary)
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
                        ItemCard(item: item, currency: currency)
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

    var body: some View {
        Button {
            Haptics.impact()
            // Tap-to-add lands with the cart phase.
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

extension ShiftView {
    /// "EGP 500.00" — opening cash, formatted from minor units.
    func currencyDisplay(_ code: String) -> String {
        Money.format(openingCashMinor, code)
    }
}
