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

    @State private var size: String?
    @State private var single: [String: String] = [:]       // groupId → addonId
    @State private var multi: [String: [String: Int]] = [:] // groupId → addonId → qty
    @State private var optionals: Set<String> = []
    @State private var qty = 1
    @State private var seeded = false
    /// Override: reveal the FULL org addon catalog (every type), not just the
    /// item's assigned slots + global types. Mirrors the dashboard's "show all".
    @State private var showAll = false

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

    /// Global types shown by default; "show all" expands to EVERY catalog type.
    private let baseTypes = ["milk_type", "coffee_type", "extra"]

    /// Default view shows only the item's allowed addons (the dashboard model);
    /// "show all" drops the allowlist filter to reveal the full catalog.
    private func visibleAddons(_ all: [ItemAddonView]) -> [ItemAddonView] {
        let allowed = Set(item.allowedAddonIds)
        if showAll || allowed.isEmpty { return all }
        return all.filter { allowed.contains($0.addonItemId) }
    }

    private var groups: [Group] {
        var out: [Group] = []
        let slotTypes = Set(item.addonSlots.map { $0.addonType })
        for s in item.addonSlots {
            let addons = visibleAddons(addonsByType[s.addonType] ?? [])
            if addons.isEmpty { continue }
            let isMulti = (s.maxSelections.map { Int($0) } ?? 2) > 1
            out.append(Group(id: s.id, title: s.label ?? typeLabel(s.addonType), addons: addons,
                             isMulti: isMulti, maxSel: s.maxSelections.map { Int($0) },
                             isRequired: s.isRequired, minSel: Int(s.minSelections)))
        }
        // Unslotted global types, then (when overriding) every remaining type.
        var extraTypes = baseTypes
        if showAll {
            let rest = addonsByType.keys.filter { !baseTypes.contains($0) }.sorted()
            extraTypes += rest
        }
        for type in extraTypes where !slotTypes.contains(type) {
            let addons = visibleAddons(addonsByType[type] ?? [])
            if addons.isEmpty { continue }
            out.append(Group(id: "type:\(type)", title: typeLabel(type), addons: addons,
                             isMulti: type != "milk_type", maxSel: nil, isRequired: false, minSel: 0))
        }
        return out
    }

    /// True when "show all" would reveal more than the default view: a per-item
    /// allowlist is hiding addons, or there are addon types not on screen.
    private var hasHiddenAddonTypes: Bool {
        if !item.allowedAddonIds.isEmpty { return true }
        let slotTypes = Set(item.addonSlots.map { $0.addonType })
        return addonsByType.keys.contains { !slotTypes.contains($0) && !baseTypes.contains($0) }
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
            if let mx = g.maxSel, m.count >= mx { return }
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
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            VStack(spacing: 0) {
                header
                ScrollView {
                    VStack(alignment: .leading, spacing: Space.lg) {
                        if !item.sizes.isEmpty { sizeSection }
                        ForEach(groups) { groupCard($0) }
                        if showAll || hasHiddenAddonTypes { showAllToggle }
                        if !item.optionalFields.isEmpty { optionalsSection }
                        if !visibleRecipes.isEmpty { recipeSection }
                    }
                    .frame(maxWidth: 560)
                    .frame(maxWidth: .infinity)
                    .padding(Space.lg)
                }
                footer
            }
        }
        .onAppear(perform: seed)
    }

    private func group(forAddon addonId: String) -> Group? {
        groups.first { g in g.addons.contains { $0.addonItemId == addonId } }
    }

    private func seed() {
        guard !seeded else { return }
        seeded = true
        if let line = app.detailEditLine {
            // Edit mode: reconstruct the selection from the existing line.
            size = line.sizeLabel ?? item.sizes.first?.label
            for a in line.addons {
                guard let g = group(forAddon: a.addonItemId) else { continue }
                if g.isMulti {
                    var m = multi[g.id] ?? [:]; m[a.addonItemId] = Int(a.qty); multi[g.id] = m
                } else {
                    single[g.id] = a.addonItemId
                }
            }
            optionals = Set(line.optionals.map { $0.optionalFieldId })
            qty = Swift.max(1, Int(line.qty))
        } else {
            if size == nil { size = item.sizes.first?.label }
            if let dm = item.defaultMilkAddonId { single["type:milk_type"] = dm }
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
            Text(Money.format(headerTotal, currency))
                .font(.money(14, .bold)).foregroundStyle(theme.colors.navy)
                .padding(.horizontal, 10).padding(.vertical, 5)
                .background(theme.colors.navyBg)
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
            Button { onClose() } label: {
                Image(systemName: "xmark").font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(theme.colors.textMuted)
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, Space.lg)
        .padding(.vertical, Space.md)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }

    /// The recipe lines for the current size (size-specific + size-agnostic).
    private var visibleRecipes: [RecipeLineView] {
        item.recipes.filter { $0.sizeLabel == nil || $0.sizeLabel == size }
    }

    private var recipeSection: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            sectionTitle(t("order.recipe"))
            VStack(spacing: 0) {
                let lines = visibleRecipes
                ForEach(Array(lines.enumerated()), id: \.offset) { idx, r in
                    HStack {
                        Text(r.ingredientName).font(.ui(13)).foregroundStyle(theme.colors.textPrimary)
                        Spacer(minLength: Space.sm)
                        Text("\(fmtQty(r.quantity)) \(r.unit)")
                            .font(.money(12, .semibold)).foregroundStyle(theme.colors.textSecondary)
                    }
                    .padding(.vertical, 9)
                    if idx < lines.count - 1 { Rectangle().fill(theme.colors.borderLight).frame(height: 1) }
                }
            }
            .padding(.horizontal, Space.md)
            .background(theme.colors.surface)
            .overlay(
                RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                    .strokeBorder(theme.colors.border, lineWidth: 1)
            )
            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        }
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
                Image(systemName: showAll ? "chevron.up" : "plus.circle")
                    .font(.system(size: 12, weight: .semibold))
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

    private func groupCard(_ g: Group) -> some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            HStack(spacing: Space.sm) {
                sectionTitle(g.title)
                if g.isRequired {
                    Text("•").foregroundStyle(theme.colors.danger)
                }
            }
            VStack(spacing: 0) {
                ForEach(Array(g.addons.enumerated()), id: \.element.addonItemId) { idx, a in
                    addonRow(g, a)
                    if idx < g.addons.count - 1 { Rectangle().fill(theme.colors.borderLight).frame(height: 1) }
                }
            }
            .padding(.horizontal, Space.md)
            .background(theme.colors.surface)
            .overlay(
                RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                    .strokeBorder(theme.colors.border, lineWidth: 1)
            )
            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        }
    }

    private func addonRow(_ g: Group, _ a: ItemAddonView) -> some View {
        let selected = g.isMulti ? (multi[g.id]?[a.addonItemId] != nil) : (single[g.id] == a.addonItemId)
        let qtyVal = multi[g.id]?[a.addonItemId] ?? 1
        return HStack(spacing: Space.md) {
            Image(systemName: g.isMulti
                  ? (selected ? "checkmark.square.fill" : "square")
                  : (selected ? "largecircle.fill.circle" : "circle"))
                .foregroundStyle(selected ? theme.colors.accent : theme.colors.textMuted)
            Text(a.name).font(.ui(14)).foregroundStyle(theme.colors.textPrimary)
            Spacer(minLength: Space.sm)
            if a.chargedPriceMinor > 0 {
                Text("+\(Money.format(a.chargedPriceMinor, currency))")
                    .font(.money(12, .semibold)).foregroundStyle(theme.colors.textSecondary)
            }
            if g.isMulti && selected {
                miniStepper(qtyVal, dec: { decMulti(g, a.addonItemId) }, inc: { incMulti(g, a.addonItemId) })
            }
        }
        .padding(.vertical, 11)
        .contentShape(Rectangle())
        .onTapGesture {
            Haptics.selection()
            if g.isMulti { toggleMulti(g, a.addonItemId) } else { toggleSingle(g, a.addonItemId) }
        }
    }

    private var optionalsSection: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            sectionTitle(t("order.optionals"))
            VStack(spacing: 0) {
                let fields = item.optionalFields.filter { $0.isActive }
                ForEach(Array(fields.enumerated()), id: \.element.id) { idx, f in
                    let on = optionals.contains(f.id)
                    HStack(spacing: Space.md) {
                        Image(systemName: on ? "checkmark.square.fill" : "square")
                            .foregroundStyle(on ? theme.colors.accent : theme.colors.textMuted)
                        Text(f.name).font(.ui(14)).foregroundStyle(theme.colors.textPrimary)
                        Spacer(minLength: Space.sm)
                        if f.priceMinor > 0 {
                            Text("+\(Money.format(f.priceMinor, currency))")
                                .font(.money(12, .semibold)).foregroundStyle(theme.colors.textSecondary)
                        }
                    }
                    .padding(.vertical, 11)
                    .contentShape(Rectangle())
                    .onTapGesture {
                        Haptics.selection()
                        if optionals.contains(f.id) { optionals.remove(f.id) } else { optionals.insert(f.id) }
                    }
                    if idx < fields.count - 1 { Rectangle().fill(theme.colors.borderLight).frame(height: 1) }
                }
            }
            .padding(.horizontal, Space.md)
            .background(theme.colors.surface)
            .overlay(
                RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                    .strokeBorder(theme.colors.border, lineWidth: 1)
            )
            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        }
    }

    private var footer: some View {
        let unsatisfied = firstUnsatisfied
        let canAdd = unsatisfied == nil
        let label = canAdd
            ? (app.detailEditKey == nil ? t("order.add_to_cart") : t("order.update_item"))
            : "\(t("order.select_prefix")) \(unsatisfied!.title)"
        return HStack(spacing: Space.md) {
            miniStepper(qty, dec: { qty = Swift.max(1, qty - 1) }, inc: { qty = Swift.min(99, qty + 1) }, large: true)
            Button {
                guard canAdd else { return }
                Haptics.impact()
                app.addConfigured(itemId: item.id, sizeLabel: size, addons: selectedAddons(),
                                  optionalIds: Array(optionals), qty: Int64(qty), notes: nil)
            } label: {
                HStack {
                    Text(label).font(.ui(14, .bold))
                    Spacer()
                    Text(Money.format(lineTotal, currency)).font(.money(14, .heavy))
                }
                .foregroundStyle(theme.colors.textOnAccent)
                .padding(.horizontal, Space.lg)
                .frame(height: 50)
                .frame(maxWidth: .infinity)
                .background(canAdd ? theme.colors.accent : theme.colors.accent.opacity(0.45))
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
            }
            .buttonStyle(.pressable(scale: 0.985))
            .allowsHitTesting(canAdd)
        }
        .padding(Space.lg)
        .background(theme.colors.surface)
        .overlay(alignment: .top) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }

    // MARK: - Small parts

    private func sectionTitle(_ s: String) -> some View {
        Text(s).font(.ui(12, .semibold)).foregroundStyle(theme.colors.textMuted)
            .textCase(.uppercase)
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
            Image(systemName: symbol).font(.system(size: 12, weight: .bold))
                .foregroundStyle(theme.colors.textPrimary)
                .frame(width: 30, height: 30)
                .background(theme.colors.surfaceAlt)
                .clipShape(Circle())
                .overlay(Circle().strokeBorder(theme.colors.border, lineWidth: 1))
        }
        .buttonStyle(.pressable(scale: 0.9))
    }
}
