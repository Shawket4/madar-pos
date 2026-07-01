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
                BundleHeader(name: bundle.name, description: bundle.description,
                             priceMinor: bundle.priceMinor, currency: currency, onClose: onClose)
                ScrollView {
                    VStack(alignment: .leading, spacing: Space.md) {
                        SectionTitle(text: t("order.bundle_includes"))
                        ForEach(Array(components.enumerated()), id: \.offset) { idx, c in
                            ComponentTile(
                                comp: c,
                                currency: currency,
                                configurable: item(for: c).map(needsConfig) ?? false,
                                draft: drafts[idx],
                                onTap: { open(idx, c) }
                            )
                        }
                    }
                    .frame(maxWidth: 520)
                    .frame(maxWidth: .infinity)
                    .padding(Space.lg)
                }
                BundleFooter(
                    bundlePriceMinor: bundle.priceMinor,
                    extrasMinor: extrasTotal,
                    liveTotalMinor: liveTotal,
                    currency: currency,
                    canAdd: canAdd,
                    onAdd: addToCart
                )
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

    /// Open the per-component customization sheet (loads the component's addons
    /// into the core first). No-op for fixed components.
    private func open(_ idx: Int, _ c: BundleComponentView) {
        guard let it = item(for: c), needsConfig(it) else { return }
        Haptics.selection()
        _ = app.componentItem(c.itemId) // loads itemAddons for the sheet
        configuring = ConfiguringComponent(id: idx, item: it)
    }

    /// Resolve every component (configured draft, or its default size) into a
    /// bundle selection and record one bundle line via the core.
    private func addToCart() {
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
    }
}

// MARK: - Header

/// The sheet header — name + description, a navy fixed-price badge, and a close
/// affordance. (The scrim, grab handle, and slide animation belong to the host
/// `.madarSheet`.)
private struct BundleHeader: View {
    @Environment(\.theme) private var theme
    let name: String
    let description: String?
    let priceMinor: Int64
    let currency: String
    let onClose: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: Space.sm) {
            VStack(alignment: .leading, spacing: 2) {
                Text(name).font(.ui(18, .heavy)).foregroundStyle(theme.colors.textPrimary)
                if let d = description, !d.isEmpty {
                    Text(d).font(.ui(12)).foregroundStyle(theme.colors.textSecondary).lineLimit(2)
                }
            }
            Spacer(minLength: 0)
            Text(Money.format(priceMinor, currency))
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
}

// MARK: - Component tile

/// A bundle component row — a leading status tone-tile, qty× name + a config
/// summary, the chosen extras up-charge, and a chevron when configurable.
private struct ComponentTile: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let comp: BundleComponentView
    let currency: String
    let configurable: Bool
    let draft: BundleComponentDraft?
    let onTap: () -> Void

    private var configured: Bool { draft != nil }

    var body: some View {
        Button(action: onTap) {
            HStack(spacing: Space.md) {
                statusTile
                VStack(alignment: .leading, spacing: 2) {
                    Text("\(comp.quantity)× \(comp.itemName)")
                        .font(.ui(15, .semibold)).foregroundStyle(theme.colors.textPrimary)
                    if let subtitle {
                        Text(subtitle)
                            .font(.ui(12, configured ? .semibold : .medium))
                            .foregroundStyle(configured ? theme.colors.accent : theme.colors.textSecondary)
                    }
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
            .overlay(RoundedRectangle(cornerRadius: Radii.md, style: .continuous)
                .strokeBorder(configured ? theme.colors.accent.opacity(0.4) : theme.colors.borderLight, lineWidth: 1))
            .elevation(.card)
            .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
        }
        .buttonStyle(.pressable(scale: 0.99))
        .allowsHitTesting(configurable)
    }

    /// Leading tone-tile behind the glyph: a navy "included" ✓ when fixed, success
    /// ✓ once configured, an accent slider glyph (on accentBg) while it needs
    /// configuring.
    private var statusTile: some View {
        let (symbol, fg, bg): (String, Color, Color) = !configurable
            ? ("checkmark.circle.fill", theme.colors.navy, theme.colors.navyBg)
            : configured ? ("checkmark.circle.fill", theme.colors.success, theme.colors.successBg)
            : ("slider.horizontal.3", theme.colors.accent, theme.colors.accentBg)
        return MadarIcon(symbol, size: IconSize.md)
            .foregroundStyle(fg)
            .frame(width: 40, height: 40)
            .background(bg)
            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
    }

    /// Subtitle for configurable rows only: a "Configure" prompt, or the chosen
    /// size · +N once configured. Fixed components carry no subtitle — the section
    /// header already reads "Includes", so a per-row repeat is dead weight.
    private var subtitle: String? {
        guard configurable else { return nil }
        guard configured else { return t("order.configure") }
        var parts: [String] = []
        if let s = draft?.sizeLabel { parts.append(s) }
        let extras = (draft?.addons.count ?? 0) + (draft?.optionalIds.count ?? 0)
        if extras > 0 { parts.append("+\(extras)") }
        return parts.isEmpty ? t("order.configure") : parts.joined(separator: " · ")
    }
}

// MARK: - Footer

/// The sheet footer — a base + extras breakdown above a tinted-teal grand-total
/// block (the live combo price is the hero figure), then the Add-to-cart CTA.
/// Mirrors the Order screen's CartFooter.
private struct BundleFooter: View {
    @Environment(\.theme) private var theme
    @Environment(\.localize) private var t
    let bundlePriceMinor: Int64
    let extrasMinor: Int64
    let liveTotalMinor: Int64
    let currency: String
    let canAdd: Bool
    let onAdd: () -> Void

    var body: some View {
        VStack(spacing: Space.sm) {
            // Base price + (optional) extras — light sub-rows so the total carries weight.
            BundleTotalRow(label: t("order.subtotal"), value: Money.format(bundlePriceMinor, currency))
            if extrasMinor > 0 {
                BundleTotalRow(label: t("order.addon_extra"), value: "+\(Money.format(extrasMinor, currency))")
            }
            // Grand total — tinted teal block, the figure the cashier reads.
            HStack {
                Text(t("order.total")).font(.ui(14, .bold)).foregroundStyle(theme.colors.accent)
                Spacer()
                Text(Money.format(liveTotalMinor, currency))
                    .font(.money(20, .heavy)).foregroundStyle(theme.colors.accent)
            }
            .padding(.horizontal, Space.md)
            .padding(.vertical, Space.md)
            .background(theme.colors.accentBg)
            .clipShape(RoundedRectangle(cornerRadius: Radii.md, style: .continuous))
            MadarButton(label: canAdd ? t("order.add_to_cart") : t("order.configure"),
                        isEnabled: canAdd, action: onAdd)
                .padding(.top, Space.xs)
        }
        .animation(Motion.standard, value: liveTotalMinor)
        .padding(Space.lg)
        .background(theme.colors.surface)
        .overlay(alignment: .top) { Rectangle().fill(theme.colors.border).frame(height: 1) }
    }
}

/// A light subtotal/extras row above the tinted total block.
private struct BundleTotalRow: View {
    @Environment(\.theme) private var theme
    let label: String
    let value: String

    var body: some View {
        HStack {
            Text(label).font(.ui(13, .medium)).foregroundStyle(theme.colors.textSecondary)
            Spacer()
            Text(value).font(.money(13, .semibold)).foregroundStyle(theme.colors.textSecondary)
        }
    }
}

// MARK: - Section title

private struct SectionTitle: View {
    @Environment(\.theme) private var theme
    let text: String

    var body: some View {
        Text(text).font(.ui(12, .semibold)).foregroundStyle(theme.colors.textMuted).textCase(.uppercase)
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
