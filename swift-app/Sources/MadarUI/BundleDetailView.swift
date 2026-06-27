// Bundle (combo) configuration sheet + the catalog card. A bundle is a fixed
// price covering a set of component items; each component is configured through
// the SAME item-customization sheet (size/addons/optionals) in "configure mode",
// which returns the selection instead of writing to the cart. "Add to cart"
// records one bundle line via the core (cart_add_bundle), where the component
// up-charges are resolved. Mirrors the Flutter BundleDetailSheet + BundleCard.
import SwiftUI

/// A host-only draft of one configured component (what the per-component sheet
/// returns). `extrasMinor` is the resolved addon/optional up-charge, summed into
/// the bundle's live total.
struct BundleComponentDraft {
    var sizeLabel: String?
    var addons: [AddonSelection]
    var optionalIds: [String]
    var extrasMinor: Int64
}

struct BundleDetailView: View {
    @ObservedObject var app: AppModel
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let bundle: BundleView
    let onClose: () -> Void

    /// Per-component config, keyed by the component's index (handles a bundle
    /// that lists the same item twice).
    @State private var drafts: [Int: BundleComponentDraft] = [:]
    @State private var configuring: ConfiguringComponent?

    private var currency: String { app.session?.currencyCode ?? "" }
    private var components: [BundleComponentView] { bundle.components }

    /// A component needs configuring when it has a size choice, addon slots, or
    /// active optionals (Flutter's `componentNeedsConfiguration`).
    private func needsConfig(_ item: MenuItemView) -> Bool {
        item.sizes.count > 1 || !item.addonSlots.isEmpty || item.optionalFields.contains { $0.isActive }
    }
    private func item(for c: BundleComponentView) -> MenuItemView? {
        app.menuItems.first { $0.id == c.itemId }
    }
    /// All configurable components must be configured before adding.
    private var canAdd: Bool {
        components.enumerated().allSatisfy { idx, c in
            guard let it = item(for: c), needsConfig(it) else { return true }
            return drafts[idx] != nil
        }
    }
    private var extrasTotal: Int64 { drafts.values.reduce(0) { $0 + $1.extrasMinor } }
    private var liveTotal: Int64 { bundle.priceMinor + extrasTotal }

    var body: some View {
        ZStack {
            theme.colors.bg.ignoresSafeArea()
            VStack(spacing: 0) {
                header
                ScrollView {
                    VStack(alignment: .leading, spacing: Space.md) {
                        sectionTitle(t("order.bundle_includes"))
                        ForEach(Array(components.enumerated()), id: \.offset) { idx, c in
                            componentTile(idx, c)
                        }
                    }
                    .frame(maxWidth: 520)
                    .frame(maxWidth: .infinity)
                    .padding(Space.lg)
                }
                footer
            }
        }
        // Per-component customization, reusing ItemDetailView in configure mode.
        .madarSheet(item: $configuring, size: .large) { ctx, dismiss in
            ItemDetailView(
                app: app, item: ctx.item, onClose: dismiss,
                configureSeed: drafts[ctx.id],
                onConfigure: { draft in drafts[ctx.id] = draft; dismiss() }
            )
        }
    }

    private var header: some View {
        HStack(alignment: .center, spacing: Space.sm) {
            VStack(alignment: .leading, spacing: 2) {
                Text(bundle.name).font(.ui(19, .bold)).foregroundStyle(theme.colors.textPrimary)
                if let d = bundle.description, !d.isEmpty {
                    Text(d).font(.ui(12)).foregroundStyle(theme.colors.textSecondary).lineLimit(2)
                }
            }
            Spacer(minLength: 0)
            Text(Money.format(bundle.priceMinor, currency))
                .font(.money(14, .bold)).foregroundStyle(theme.colors.navy)
                .frame(height: 32).padding(.horizontal, 10)
                .background(theme.colors.navyBg)
                .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
            Button { onClose() } label: {
                MadarIcon("xmark", size: 14)
                    .foregroundStyle(theme.colors.textMuted)
                    .frame(width: 32, height: 32)
                    .background(theme.colors.surfaceAlt)
                    .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                        .strokeBorder(theme.colors.border, lineWidth: 1))
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, Space.lg).padding(.vertical, Space.md)
        .background(theme.colors.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }

    private func componentTile(_ idx: Int, _ c: BundleComponentView) -> some View {
        let it = item(for: c)
        let configurable = it.map(needsConfig) ?? false
        let draft = drafts[idx]
        let configured = draft != nil
        return Button {
            guard let it, configurable else { return }
            Haptics.selection()
            _ = app.componentItem(c.itemId) // loads itemAddons for the sheet
            configuring = ConfiguringComponent(id: idx, item: it)
        } label: {
            HStack(spacing: Space.md) {
                statusIcon(configurable: configurable, configured: configured)
                VStack(alignment: .leading, spacing: 2) {
                    Text("\(c.quantity)× \(c.itemName)")
                        .font(.ui(14, .semibold)).foregroundStyle(theme.colors.textPrimary)
                    Text(subtitle(configurable: configurable, configured: configured, draft: draft))
                        .font(.ui(12)).foregroundStyle(theme.colors.textSecondary)
                }
                Spacer(minLength: 0)
                if let draft, draft.extrasMinor > 0 {
                    Text("+\(Money.format(draft.extrasMinor, currency))")
                        .font(.money(12, .bold)).foregroundStyle(theme.colors.accent)
                }
                if configurable {
                    MadarIcon("chevron.forward", size: 12)
                        .foregroundStyle(theme.colors.textMuted)
                }
            }
            .padding(.horizontal, Space.md).padding(.vertical, Space.md)
            .background(theme.colors.surface)
            .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                .strokeBorder(configured ? theme.colors.accent.opacity(0.4) : theme.colors.border, lineWidth: 1))
        }
        .buttonStyle(.pressable(scale: 0.99))
        .allowsHitTesting(configurable)
    }

    private func statusIcon(configurable: Bool, configured: Bool) -> some View {
        let (symbol, color): (String, Color) = !configurable
            ? ("checkmark.circle.fill", theme.colors.textMuted)
            : configured ? ("checkmark.circle.fill", theme.colors.success)
            : ("slider.horizontal.3", theme.colors.accent)
        return MadarIcon(symbol, size: 16).foregroundStyle(color).frame(width: 22)
    }

    private func subtitle(configurable: Bool, configured: Bool, draft: BundleComponentDraft?) -> String {
        if !configurable { return t("order.bundle_includes") }
        guard configured else { return t("order.configure") }
        var parts: [String] = []
        if let s = draft?.sizeLabel { parts.append(s) }
        let addonCount = draft?.addons.count ?? 0
        let optCount = draft?.optionalIds.count ?? 0
        if addonCount + optCount > 0 { parts.append("+\(addonCount + optCount)") }
        return parts.isEmpty ? t("order.configure") : parts.joined(separator: " · ")
    }

    private var footer: some View {
        let label = canAdd ? t("order.add_to_cart") : t("order.configure")
        return Button {
            guard canAdd else { return }
            Haptics.impact()
            let selections = components.enumerated().map { idx, c -> BundleComponentSelection in
                let d = drafts[idx]
                let defaultSize = item(for: c)?.sizes.first?.label
                return BundleComponentSelection(
                    itemId: c.itemId,
                    sizeLabel: d?.sizeLabel ?? defaultSize,
                    qty: c.quantity,
                    addons: d?.addons ?? [],
                    optionalFieldIds: d?.optionalIds ?? [])
            }
            app.addBundle(bundleId: bundle.id, components: selections)
        } label: {
            HStack {
                Text(label).font(.ui(14, .bold))
                Spacer()
                Text(Money.format(liveTotal, currency)).font(.money(14, .heavy))
            }
            .foregroundStyle(theme.colors.textOnAccent)
            .padding(.horizontal, Space.lg).frame(height: 50).frame(maxWidth: .infinity)
            .background(canAdd ? theme.colors.accent : theme.colors.accent.opacity(0.45))
            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
        }
        .buttonStyle(.pressable(scale: 0.985))
        .allowsHitTesting(canAdd)
        .padding(Space.lg)
        .background(theme.colors.surface)
        .overlay(alignment: .top) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }

    private func sectionTitle(_ s: String) -> some View {
        Text(s).font(.ui(12, .semibold)).foregroundStyle(theme.colors.textMuted).textCase(.uppercase)
    }
}

/// `.sheet(item:)` payload for the per-component customization (index + item).
private struct ConfiguringComponent: Identifiable {
    let id: Int
    let item: MenuItemView
}

/// A combo card in the catalog grid — bundle name, component count, fixed price.
struct BundleCard: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let bundle: BundleView
    let currency: String
    let onTap: () -> Void

    var body: some View {
        Button { Haptics.selection(); onTap() } label: {
            VStack(alignment: .leading, spacing: 0) {
                ZStack(alignment: .topLeading) {
                    LinearGradient(
                        colors: [theme.colors.accent, theme.colors.accent.opacity(0.7)],
                        startPoint: .topLeading, endPoint: .bottomTrailing)
                    if let urlStr = bundle.imageUrl, let url = URL(string: urlStr) {
                        CachedAsyncImage(url: url).opacity(0.55)
                    }
                    StatusChip(label: t("order.combos"), icon: "bag.fill", tone: .accent)
                        .padding(Space.sm)
                }
                .frame(height: 96)
                .clipped()
                VStack(alignment: .leading, spacing: 4) {
                    Text(bundle.name).font(.ui(14, .bold)).foregroundStyle(theme.colors.textPrimary)
                        .lineLimit(1)
                    Text("\(bundle.components.count) \(t("order.bundle_includes"))")
                        .font(.ui(11)).foregroundStyle(theme.colors.textSecondary).lineLimit(1)
                    Text(Money.format(bundle.priceMinor, currency))
                        .font(.money(14, .heavy)).foregroundStyle(theme.colors.accent)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(Space.md)
            }
            .background(theme.colors.surface)
            .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                .strokeBorder(theme.colors.border, lineWidth: 1))
        }
        .buttonStyle(.pressable(scale: 0.98))
    }
}
