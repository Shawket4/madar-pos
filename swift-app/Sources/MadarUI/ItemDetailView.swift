// Item customization sheet — pick size, addons (per slot + global types), and
// optional fields, then add to the cart. ALL prices come pre-resolved from the
// core (list_item_addons charges the swap delta / full price); this view only
// displays them and sums the live total. Mirrors the Flutter ItemDetailSheet.
import SwiftUI

// `.sheet(item:)` needs Identifiable; the record carries an `id`.
extension MenuItemView: Identifiable {}

struct ItemDetailView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let item: MenuItemView
    let onClose: () -> Void
    /// Bundle-component configuration mode: when `onConfigure` is set the footer
    /// SAVES the selection back (no cart write), seeded from `configureSeed`, and
    /// the qty stepper is hidden (the bundle fixes the component count).
    var configureSeed: BundleComponentDraft? = nil
    var onConfigure: ((BundleComponentDraft) -> Void)? = nil
    private var isConfiguring: Bool { onConfigure != nil }

    @State private var size: String?
    @State private var single: [String: String] = [:]       // groupId → addonId
    @State private var multi: [String: [String: Int]] = [:] // groupId → addonId → qty
    @State private var optionals: Set<String> = []
    @State private var qty = 1
    @State private var seeded = false
    /// Override: reveal the FULL org addon catalog (every type), not just the
    /// item's assigned slots + global types. Mirrors the dashboard's "show all".
    @State private var showAll = false
    /// The recipe section is revealed by the header recipe button (Flutter chip).
    @State private var showRecipe = false
    /// Per-group search query (groupId → text), shown only when a group has many
    /// addons so a long list stays scannable. Mirrors the dashboard's filter.
    @State private var addonSearch: [String: String] = [:]
    /// Search query for the optional-fields section (same >4 rule as the groups).
    @State private var optionalSearch = ""
    /// A free-text note for this line (kitchen instructions). Not shown in bundle
    /// component-config mode.
    @State private var notes = ""

    private var currency: String { app.session?.currencyCode ?? "" }

    // MARK: - Groups (slots + unslotted global types)

    private struct Group: Identifiable {
        let id: String
        let title: String
        let addons: [ItemAddonView]
        let isMulti: Bool
        let maxSel: Int?
        let isRequired: Bool
        let minSel: Int
    }

    private var addonsByType: [String: [ItemAddonView]] {
        Dictionary(grouping: app.itemAddons, by: { $0.addonType })
    }

    /// The drink's standard addon types, shown by default; "show all" reveals any
    /// other catalog types.
    private let baseTypes = ["milk_type", "coffee_type", "extra"]

    /// Default view = the item's AVAILABLE add-ons only. When the item declares an
    /// allow-list (`allowedAddonIds`), each type is filtered to those options; an
    /// explicit SLOT always shows its options. When the item has NO allow-list we
    /// show the type's full set (a sensible default, never an empty card). "Show
    /// all" drops the filter and reveals every option of every type.
    private func visibleAddons(_ all: [ItemAddonView], isSlot: Bool) -> [ItemAddonView] {
        if showAll || isSlot { return all }
        let allowed = Set(item.allowedAddonIds)
        if allowed.isEmpty { return all }
        return all.filter { allowed.contains($0.addonItemId) }
    }

    /// True when "Show all" would reveal more than the default view — either the
    /// allow-list is hiding options, or there are addon types off-screen.
    private var hasMore: Bool {
        if !item.allowedAddonIds.isEmpty { return true }
        let slotTypes = Set(item.addonSlots.map { $0.addonType })
        return addonsByType.keys.contains { !slotTypes.contains($0) && !baseTypes.contains($0) }
    }

    private var groups: [Group] {
        var out: [Group] = []
        let slotTypes = Set(item.addonSlots.map { $0.addonType })
        for s in item.addonSlots {
            let addons = visibleAddons(addonsByType[s.addonType] ?? [], isSlot: true)
            if addons.isEmpty { continue }
            let isMulti = (s.maxSelections.map { Int($0) } ?? 2) > 1
            out.append(Group(id: s.id, title: s.label ?? typeLabel(s.addonType), addons: addons,
                             isMulti: isMulti, maxSel: s.maxSelections.map { Int($0) },
                             isRequired: s.isRequired, minSel: Int(s.minSelections)))
        }
        // Standard unslotted drink types (milk/coffee/extra) by default; "show all"
        // appends the rest of the catalog's types.
        var extraTypes = baseTypes
        if showAll {
            let rest = addonsByType.keys.filter { !baseTypes.contains($0) }.sorted()
            extraTypes += rest
        }
        for type in extraTypes where !slotTypes.contains(type) {
            let addons = visibleAddons(addonsByType[type] ?? [], isSlot: false)
            if addons.isEmpty { continue }
            out.append(Group(id: "type:\(type)", title: typeLabel(type), addons: addons,
                             isMulti: type != "milk_type", maxSel: nil, isRequired: false, minSel: 0))
        }
        return out
    }

    private func typeLabel(_ type: String) -> String {
        switch type {
        case "milk_type": return t("order.addon_milk_type")
        case "coffee_type": return t("order.addon_coffee_type")
        case "extra": return t("order.addon_extra")
        default: return type.replacingOccurrences(of: "_", with: " ").capitalized
        }
    }

    // MARK: - Pricing (display only)

    private func unitPrice() -> Int64 {
        if let sz = size, let s = item.sizes.first(where: { $0.label == sz }) { return s.priceMinor }
        return item.basePriceMinor
    }
    private func charged(_ id: String) -> Int64 {
        app.itemAddons.first { $0.addonItemId == id }?.chargedPriceMinor ?? 0
    }
    private func selectedAddons() -> [AddonSelection] {
        var out: [AddonSelection] = []
        for (_, aid) in single { out.append(AddonSelection(addonItemId: aid, qty: 1)) }
        for (_, m) in multi { for (aid, q) in m { out.append(AddonSelection(addonItemId: aid, qty: Int64(q))) } }
        return out
    }
    private var addonsTotal: Int64 { selectedAddons().reduce(0) { $0 + charged($1.addonItemId) * $1.qty } }
    private var optionalsTotal: Int64 {
        item.optionalFields.filter { optionals.contains($0.id) }.reduce(0) { $0 + $1.priceMinor }
    }
    private var headerTotal: Int64 { unitPrice() + addonsTotal + optionalsTotal }
    private var lineTotal: Int64 { headerTotal * Int64(qty) }

    private func selectedCount(_ g: Group) -> Int {
        g.isMulti ? (multi[g.id]?.count ?? 0) : (single[g.id] != nil ? 1 : 0)
    }
    private var firstUnsatisfied: Group? {
        groups.first { $0.isRequired && selectedCount($0) < Swift.max(1, $0.minSel) }
    }

    // MARK: - Selection mutations

    private func toggleSingle(_ g: Group, _ aid: String) {
        if single[g.id] == aid { if !g.isRequired { single[g.id] = nil } } else { single[g.id] = aid }
    }
    private func toggleMulti(_ g: Group, _ aid: String) {
        var m = multi[g.id] ?? [:]
        if m[aid] != nil { m[aid] = nil } else {
            if let mx = g.maxSel, m.count >= mx {
                Haptics.warning()
                app.showToast("\(g.title): \(t("order.max_reached")) (≤\(mx))", icon: "hand.raised", tone: .warning)
                return
            }
            m[aid] = 1
        }
        multi[g.id] = m.isEmpty ? nil : m
    }
    private func incMulti(_ g: Group, _ aid: String) {
        var m = multi[g.id] ?? [:]; m[aid] = (m[aid] ?? 1) + 1; multi[g.id] = m
    }
    private func decMulti(_ g: Group, _ aid: String) {
        var m = multi[g.id] ?? [:]; let cur = m[aid] ?? 1
        if cur <= 1 { m[aid] = nil } else { m[aid] = cur - 1 }
        multi[g.id] = m.isEmpty ? nil : m
    }

    // MARK: - Body

    var body: some View {
        VStack(spacing: 0) {
            header
            // Hug the content for a simple item (no tall empty void); scroll only
            // when the options overflow. Mirrors Flutter's min-height bottom sheet.
            ViewThatFits(in: .vertical) {
                optionsContent
                ScrollView { optionsContent }
            }
            footer
        }
        // Gentle cream content area so the (white) grabber + header read as one
        // continuous top, and the white option cards still pop. No floating box.
        .background(theme.colors.surfaceAlt)
        .onAppear(perform: seed)
    }

    private var optionsContent: some View {
        VStack(alignment: .leading, spacing: Space.md) {
            if showRecipe && !computedRecipe.isEmpty { recipeSection }
            if !item.sizes.isEmpty { sizeSection }
            ForEach(groups) { groupCard($0) }
            // "Show all add-ons" only when there's actually more to reveal.
            if hasMore { showAllToggle }
            if !item.optionalFields.isEmpty { optionalsSection }
            if !isConfiguring { notesSection }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, Space.xl)
        .padding(.top, Space.lg)
        .padding(.bottom, Space.sm)
    }

    /// Restore a saved addon (id + qty) into the right group — by its TYPE →
    /// slot / global `type:` bucket, NOT the on-screen `groups` (which the
    /// allowlist / "show all" filter may hide), so a selection is never dropped.
    private func placeAddon(_ addonItemId: String, qty: Int) {
        guard let type = app.itemAddons.first(where: { $0.addonItemId == addonItemId })?.addonType
        else { return }
        if let slot = item.addonSlots.first(where: { $0.addonType == type }) {
            if (slot.maxSelections.map { Int($0) } ?? 2) > 1 {
                var m = multi[slot.id] ?? [:]; m[addonItemId] = qty; multi[slot.id] = m
            } else {
                single[slot.id] = addonItemId
            }
        } else {
            let gid = "type:\(type)"
            if type != "milk_type" {
                var m = multi[gid] ?? [:]; m[addonItemId] = qty; multi[gid] = m
            } else {
                single[gid] = addonItemId
            }
        }
    }

    private func seedDefaults() {
        if size == nil { size = item.sizes.first?.label }
        if let dm = item.defaultMilkAddonId { single["type:milk_type"] = dm }
    }

    private func seed() {
        guard !seeded else { return }
        seeded = true
        if isConfiguring {
            // Bundle component: seed from the saved draft, else defaults.
            if let draft = configureSeed {
                size = draft.sizeLabel ?? item.sizes.first?.label
                for a in draft.addons { placeAddon(a.addonItemId, qty: Int(a.qty)) }
                optionals = Set(draft.optionalIds)
            } else {
                seedDefaults()
            }
        } else if let line = app.detailEditLine {
            // Edit mode: reconstruct the selection from the existing cart line.
            size = line.sizeLabel ?? item.sizes.first?.label
            for a in line.addons { placeAddon(a.addonItemId, qty: Int(a.qty)) }
            optionals = Set(line.optionals.map { $0.optionalFieldId })
            notes = line.notes ?? ""
            qty = Swift.max(1, Int(line.qty))
        } else {
            seedDefaults()
        }
    }

    private var header: some View {
        HStack(alignment: .top, spacing: Space.md) {
            VStack(alignment: .leading, spacing: 2) {
                Text(item.name).font(.ui(18, .heavy)).foregroundStyle(theme.colors.textPrimary)
                if let d = item.description, !d.isEmpty {
                    Text(d).font(.ui(12)).foregroundStyle(theme.colors.textSecondary).lineLimit(2)
                }
            }
            Spacer(minLength: 0)
            HStack(spacing: Space.sm) {
                Text(Money.format(headerTotal, currency))
                    .font(.money(14, .bold)).foregroundStyle(theme.colors.navy)
                    .frame(height: 32)
                    .padding(.horizontal, 10)
                    .background(theme.colors.navyBg)
                    .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
                if !item.recipes.isEmpty {
                    Button { withAnimation(Motion.standard) { showRecipe.toggle() } } label: {
                        MadarIcon("list.bullet.rectangle", size: 13)
                            .foregroundStyle(showRecipe ? theme.colors.textOnAccent : theme.colors.accent)
                            .frame(width: 32, height: 32)
                            .background(showRecipe ? theme.colors.accent : theme.colors.accentBg)
                            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
                    }
                    .buttonStyle(.plain)
                }
                Button { onClose() } label: {
                    MadarIcon("xmark", size: 14)
                        .foregroundStyle(theme.colors.textMuted)
                        .frame(width: 32, height: 32)
                        .background(theme.colors.surfaceAlt)
                        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
                        .overlay(
                            RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                                .strokeBorder(theme.colors.borderLight, lineWidth: 1)
                        )
                        .elevation(.card)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, Space.xl)
        .padding(.vertical, Space.md)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }

    /// The effective recipe for the current selection — the core applies size,
    /// milk/coffee swaps, additive addons (× qty) and optional contributions, so
    /// the panel reflects exactly what the teller configured.
    private var computedRecipe: [ComputedRecipeLineView] {
        app.recipePreview(itemId: item.id, sizeLabel: size, addons: selectedAddons(), optionalIds: Array(optionals))
    }

    private var recipeSection: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            sectionTitle(t("order.recipe"))
            VStack(spacing: Space.sm) {
                ForEach(Array(computedRecipe.enumerated()), id: \.offset) { _, r in
                    recipeRow(r)
                }
            }
        }
    }

    /// One ingredient row — a card with a fixed quantity box on the left, the name
    /// taking the middle, and the source chip pinned to the RIGHT so every chip
    /// lines up in a column (Flutter `_RecipeIngredientRow`). Base ingredients get
    /// the navy-tinted card.
    private func recipeRow(_ r: ComputedRecipeLineView) -> some View {
        HStack(spacing: Space.md) {
            VStack(spacing: 0) {
                Text(fmtQty(r.quantity)).font(.ui(14, .bold)).foregroundStyle(theme.colors.textPrimary)
                Text(r.unit).font(.ui(10, .semibold)).foregroundStyle(theme.colors.textMuted)
            }
            .frame(width: 54)
            .padding(.vertical, 6)
            .background(theme.colors.surface)
            .clipShape(RoundedRectangle(cornerRadius: Radii.xs, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: Radii.xs, style: .continuous)
                .strokeBorder(theme.colors.borderLight, lineWidth: 1))

            Text(r.ingredientName)
                .font(.ui(14, r.isBase ? .bold : .semibold))
                .foregroundStyle(theme.colors.textPrimary)
                .frame(maxWidth: .infinity, alignment: .leading)

            StatusChip(label: r.sourceLabel.uppercased(), tone: r.isBase ? .accent : .neutral)
        }
        .padding(.horizontal, Space.md).padding(.vertical, Space.md)
        .background(r.isBase ? theme.colors.navyBg : theme.colors.surface)
        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
            .strokeBorder(r.isBase ? theme.colors.navy.opacity(0.25) : theme.colors.border, lineWidth: 1))
    }

    private func fmtQty(_ q: Double) -> String {
        q == q.rounded() ? String(Int(q)) : String(format: "%g", q)
    }

    private var showAllToggle: some View {
        Button {
            Haptics.selection()
            withAnimation(Motion.standard) { showAll.toggle() }
        } label: {
            HStack(spacing: 6) {
                MadarIcon(showAll ? "chevron.up" : "plus", size: 12)
                Text(showAll ? t("order.show_assigned_addons") : t("order.show_all_addons"))
                    .font(.ui(13, .semibold))
            }
            .foregroundStyle(theme.colors.accent)
            .frame(maxWidth: .infinity)
            .padding(.vertical, Space.sm)
        }
        .buttonStyle(.pressable)
    }

    private var sizeSection: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            sectionTitle(t("order.size"))
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: Space.sm) {
                    ForEach(item.sizes, id: \.id) { s in
                        selectChip(label: s.label, sub: Money.format(s.priceMinor, currency),
                                   active: size == s.label) { size = s.label }
                    }
                }
            }
        }
    }

    /// Addons matching the group's live search query (case-insensitive). When no
    /// query is set the full list shows; selected chips always stay visible so a
    /// filter never hides an active selection.
    private func filteredAddons(_ g: Group) -> [ItemAddonView] {
        let q = (addonSearch[g.id] ?? "").trimmingCharacters(in: .whitespaces).lowercased()
        if q.isEmpty { return g.addons }
        return g.addons.filter { a in
            a.name.lowercased().contains(q)
                || (g.isMulti ? multi[g.id]?[a.addonItemId] != nil : single[g.id] == a.addonItemId)
        }
    }

    /// One addon group, rendered as a bordered surface card (Flutter `AddonCard`):
    /// a dotted uppercase header with required / max / count chips, an optional
    /// search field (>5 options), then the option chips.
    private func groupCard(_ g: Group) -> some View {
        VStack(alignment: .leading, spacing: Space.md) {
            HStack(spacing: Space.sm) {
                Circle().fill(theme.colors.accent).frame(width: 8, height: 8)
                Text(g.title.uppercased())
                    .font(.ui(11, .bold)).tracking(0.6)
                    .foregroundStyle(theme.colors.textSecondary)
                if g.isRequired { StatusChip(label: t("order.required"), tone: .danger) }
                if g.isMulti, let mx = g.maxSel { StatusChip(label: "≤\(mx)", tone: .neutral) }
                Spacer(minLength: 0)
                let count = selectedCount(g)
                if count > 0 { StatusChip(label: "\(count)", tone: .accent) }
            }
            if g.addons.count > 5 {
                MadarTextField(
                    placeholder: t("order.search_addons"),
                    text: Binding(get: { addonSearch[g.id] ?? "" },
                                  set: { addonSearch[g.id] = $0 }),
                    icon: "magnifyingglass"
                )
            }
            FlowLayout(spacing: Space.sm) {
                ForEach(filteredAddons(g), id: \.addonItemId) { a in addonChip(g, a) }
            }
        }
        .padding(Space.md)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(theme.colors.surface)
        .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
            .strokeBorder(theme.colors.border, lineWidth: 1))
    }

    @ViewBuilder private func addonChip(_ g: Group, _ a: ItemAddonView) -> some View {
        let selected = g.isMulti ? (multi[g.id]?[a.addonItemId] != nil) : (single[g.id] == a.addonItemId)
        if g.isMulti, selected {
            qtyChip(g, a, qty: multi[g.id]?[a.addonItemId] ?? 1)
        } else {
            optionChip(g, a, selected: selected)
        }
    }

    /// A selectable addon chip (Flutter OptionChip): accent fill when selected.
    private func optionChip(_ g: Group, _ a: ItemAddonView, selected: Bool) -> some View {
        Button {
            Haptics.selection()
            if g.isMulti { toggleMulti(g, a.addonItemId) } else { toggleSingle(g, a.addonItemId) }
        } label: {
            HStack(spacing: 6) {
                if g.isMulti, !selected {
                    MadarIcon("plus", size: 10).opacity(0.6)
                }
                Text(a.name).font(.ui(13, .semibold))
                if a.chargedPriceMinor > 0 { pricePill(a.chargedPriceMinor, on: selected) }
            }
            .foregroundStyle(selected ? theme.colors.textOnAccent : theme.colors.textPrimary)
            .padding(.horizontal, 12).padding(.vertical, 9)
            .background(selected ? theme.colors.accent : theme.colors.surfaceAlt)
            .overlay(
                RoundedRectangle(cornerRadius: Radii.xs, style: .continuous)
                    .strokeBorder(selected ? Color.clear : theme.colors.border, lineWidth: 1)
            )
            .clipShape(RoundedRectangle(cornerRadius: Radii.xs, style: .continuous))
        }
        .buttonStyle(.pressable(scale: 0.97))
    }

    /// A selected multi-select chip with an inline qty stepper (Flutter QtyChip).
    private func qtyChip(_ g: Group, _ a: ItemAddonView, qty: Int) -> some View {
        HStack(spacing: 2) {
            chipStep("minus") { decMulti(g, a.addonItemId) }
            VStack(spacing: 0) {
                Text(a.name).font(.ui(12, .semibold))
                if a.chargedPriceMinor > 0 {
                    Text("+\(Money.format(a.chargedPriceMinor * Int64(qty), currency))")
                        .font(.money(9.5, .bold)).opacity(0.85)
                }
            }
            Text("\(qty)").font(.ui(11, .heavy))
                .padding(.horizontal, 6).padding(.vertical, 2)
                .background(theme.colors.textOnAccent.opacity(0.22))
                .clipShape(Capsule())
            chipStep("plus") { incMulti(g, a.addonItemId) }
        }
        .foregroundStyle(theme.colors.textOnAccent)
        .padding(.horizontal, 4).padding(.vertical, 3)
        .background(theme.colors.accent)
        .clipShape(RoundedRectangle(cornerRadius: Radii.xs, style: .continuous))
    }

    private func chipStep(_ symbol: String, _ action: @escaping () -> Void) -> some View {
        Button { Haptics.selection(); action() } label: {
            MadarIcon(symbol, size: 11)
                .foregroundStyle(theme.colors.textOnAccent).frame(width: 24, height: 32)
        }
        .buttonStyle(.plain)
    }

    /// The little "+price" pill inside a chip.
    private func pricePill(_ minor: Int64, on: Bool) -> some View {
        Text("+\(Money.format(minor, currency))")
            .font(.money(10.5, .bold))
            .padding(.horizontal, 6).padding(.vertical, 2)
            .background(on ? theme.colors.textOnAccent.opacity(0.2) : theme.colors.accentBg)
            .foregroundStyle(on ? theme.colors.textOnAccent : theme.colors.accent)
            .clipShape(Capsule())
    }

    private var activeOptionals: [OptionalFieldView] { item.optionalFields.filter { $0.isActive } }

    /// Active optionals matching the search query; selected ones always stay.
    private var filteredOptionals: [OptionalFieldView] {
        let q = optionalSearch.trimmingCharacters(in: .whitespaces).lowercased()
        if q.isEmpty { return activeOptionals }
        return activeOptionals.filter { $0.name.lowercased().contains(q) || optionals.contains($0.id) }
    }

    private var optionalsSection: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            sectionTitle(t("order.optionals"))
            if activeOptionals.count > 4 {
                MadarTextField(placeholder: t("order.search_addons"), text: $optionalSearch, icon: "magnifyingglass")
            }
            FlowLayout(spacing: Space.sm) {
                ForEach(filteredOptionals, id: \.id) { f in
                    optionalChip(f)
                }
            }
        }
    }

    private func optionalChip(_ f: OptionalFieldView) -> some View {
        let on = optionals.contains(f.id)
        return Button {
            Haptics.selection()
            if on { optionals.remove(f.id) } else { optionals.insert(f.id) }
        } label: {
            HStack(spacing: 6) {
                MadarIcon(on ? "checkmark.circle.fill" : "circle", size: 12)
                Text(f.name).font(.ui(13, .semibold))
                if f.priceMinor > 0 { pricePill(f.priceMinor, on: on) }
            }
            .foregroundStyle(on ? theme.colors.textOnAccent : theme.colors.textPrimary)
            .padding(.horizontal, 12).padding(.vertical, 9)
            .background(on ? theme.colors.accent : theme.colors.surfaceAlt)
            .overlay(
                RoundedRectangle(cornerRadius: Radii.xs, style: .continuous)
                    .strokeBorder(on ? Color.clear : theme.colors.border, lineWidth: 1)
            )
            .clipShape(RoundedRectangle(cornerRadius: Radii.xs, style: .continuous))
        }
        .buttonStyle(.pressable(scale: 0.97))
    }

    private var notesSection: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            sectionTitle(t("order.notes"))
            MadarTextField(placeholder: t("order.notes_hint"), text: $notes, icon: "text.bubble")
        }
    }

    private var footer: some View {
        let unsatisfied = firstUnsatisfied
        let canAdd = unsatisfied == nil
        let label = if let unsatisfied {
            "\(t("order.select_prefix")) \(unsatisfied.title)"
        } else if isConfiguring {
            t("order.save_component")
        } else {
            app.detailEditKey == nil ? t("order.add_to_cart") : t("order.update_item")
        }
        let footerPrice = isConfiguring ? (addonsTotal + optionalsTotal) : lineTotal
        return VStack(spacing: Space.md) {
            // Prominent total block — tinted teal, the figure tellers look at
            // (mirrors the cart's grand-total block in OrderView.CartFooter).
            HStack {
                Text(t("order.total"))
                    .font(.ui(14, .bold)).foregroundStyle(theme.colors.accent)
                Spacer()
                Text(Money.format(footerPrice, currency))
                    .font(.money(20, .heavy)).foregroundStyle(theme.colors.accent)
                    .contentTransition(.numericText())
            }
            .padding(.horizontal, Space.md)
            .padding(.vertical, Space.md)
            .background(theme.colors.accentBg)
            .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
            HStack(spacing: Space.md) {
                if !isConfiguring {
                    miniStepper(qty, dec: { qty = Swift.max(1, qty - 1) }, inc: { qty = Swift.min(99, qty + 1) }, large: true)
                }
                Button {
                    commit(canAdd: canAdd)
                } label: {
                    Text(label).font(.ui(14, .bold))
                        .foregroundStyle(theme.colors.textOnAccent)
                        .frame(height: 50)
                        .frame(maxWidth: .infinity)
                        .background(canAdd ? theme.colors.accent : theme.colors.accent.opacity(0.45))
                        .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
                }
                .buttonStyle(.pressable(scale: 0.985))
                .allowsHitTesting(canAdd)
            }
        }
        .animation(Motion.standard, value: footerPrice)
        .padding(.horizontal, Space.xl)
        .padding(.vertical, Space.md)
        .background(theme.colors.surface)
        .overlay(alignment: .top) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }

    /// Commit the configured line — either save the bundle-component draft back to
    /// the host (configure mode) or write the line to the cart. No-op until the
    /// required groups are satisfied.
    private func commit(canAdd: Bool) {
        guard canAdd else { return }
        Haptics.impact()
        if let onConfigure {
            onConfigure(BundleComponentDraft(
                sizeLabel: size, addons: selectedAddons(),
                optionalIds: Array(optionals), extrasMinor: addonsTotal + optionalsTotal))
        } else {
            app.addConfigured(itemId: item.id, sizeLabel: size, addons: selectedAddons(),
                              optionalIds: Array(optionals), qty: Int64(qty),
                              notes: notes.isEmpty ? nil : notes)
        }
    }

    // MARK: - Small parts

    private func sectionTitle(_ s: String) -> some View {
        // Confident section label that matches the addon-group card headers — an
        // accent dot + bold uppercase with tracking — so SIZE / NOTE / OPTIONALS
        // read with the same authority as the bordered groups (not a faint muted
        // afterthought).
        HStack(spacing: Space.sm) {
            Circle().fill(theme.colors.accent).frame(width: 8, height: 8)
            Text(s).font(.ui(11, .bold)).tracking(0.6)
                .foregroundStyle(theme.colors.textSecondary)
                .textCase(.uppercase)
        }
    }

    private func selectChip(label: String, sub: String?, active: Bool, action: @escaping () -> Void) -> some View {
        Button { Haptics.selection(); action() } label: {
            VStack(spacing: 1) {
                Text(label).font(.ui(13, .semibold))
                if let sub { Text(sub).font(.money(11)).opacity(0.8) }
            }
            .foregroundStyle(active ? theme.colors.textOnAccent : theme.colors.textPrimary)
            .padding(.horizontal, Space.lg).padding(.vertical, Space.sm)
            .background(active ? theme.colors.accent : theme.colors.surface)
            .overlay(
                RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                    .strokeBorder(active ? Color.clear : theme.colors.border, lineWidth: 1)
            )
            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        }
        .buttonStyle(.pressable)
    }

    private func miniStepper(_ value: Int, dec: @escaping () -> Void, inc: @escaping () -> Void, large: Bool = false) -> some View {
        HStack(spacing: Space.sm) {
            stepBtn("minus", action: dec)
            Text("\(value)").font(.ui(large ? 16 : 14, .bold))
                .foregroundStyle(theme.colors.textPrimary).frame(minWidth: large ? 24 : 18)
            stepBtn("plus", action: inc)
        }
    }
    private func stepBtn(_ symbol: String, action: @escaping () -> Void) -> some View {
        Button { Haptics.selection(); action() } label: {
            MadarIcon(symbol, size: 12)
                .foregroundStyle(theme.colors.textPrimary)
                .frame(width: 30, height: 30)
                .background(theme.colors.surfaceAlt)
                .clipShape(Circle())
                .overlay(Circle().strokeBorder(theme.colors.border, lineWidth: 1))
        }
        .buttonStyle(.pressable(scale: 0.9))
    }
}
