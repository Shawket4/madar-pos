// The catalog's product card. A category-hued gradient hero (monogram + a soft
// decorative ring), an in-cart quantity badge, a subtle shadow, and a fixed
// footer (category accent dot · name · price). Mirrors the Flutter MenuCard's
// fallback treatment but adds the live in-cart badge. Real photos get an async
// loader in a later pass (both platforms together); the gradient is the default.
import SwiftUI

struct MenuItemCard: View {
    @Environment(\.theme) private var theme
    let item: MenuItemView
    /// Resolved category name — seeds the hue so a family shares a palette.
    let categoryName: String
    let currency: String
    /// Total quantity of this item already in the cart (0 = no badge).
    let inCartQty: Int64
    let onTap: () -> Void

    private var style: CategoryStyle {
        categoryStyle(categoryName.isEmpty ? item.name : categoryName, isDark: theme.isDark)
    }

    var body: some View {
        Button {
            Haptics.selection()
            onTap()
        } label: {
            VStack(spacing: 0) {
                hero
                footer
            }
            .background(theme.colors.surface)
            .clipShape(RoundedRectangle(cornerRadius: Radii.sm, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: Radii.sm, style: .continuous)
                    .strokeBorder(theme.colors.borderLight, lineWidth: 1)
            )
            .shadow(color: theme.colors.shadow, radius: 5, x: 0, y: 2)
        }
        .buttonStyle(.pressable)
    }

    private var hero: some View {
        ZStack {
            LinearGradient(colors: [style.bgTop, style.bgBottom],
                           startPoint: .topLeading, endPoint: .bottomTrailing)
            // A soft ornamental ring bleeding off the bottom-trailing corner.
            Circle()
                .strokeBorder(style.accent.opacity(0.16), lineWidth: 2)
                .frame(width: 130, height: 130)
                .offset(x: 46, y: 46)
            Text(monogram)
                .font(.system(size: 42, weight: .thin))
                .foregroundStyle(style.accent.opacity(theme.isDark ? 0.7 : 0.55))
                .kerning(1.5)
            if inCartQty > 0 {
                Text("\(inCartQty)")
                    .font(.ui(12, .heavy))
                    .foregroundStyle(theme.colors.textOnAccent)
                    .frame(minWidth: 14)
                    .padding(.horizontal, 6).padding(.vertical, 3)
                    .background(theme.colors.accent)
                    .clipShape(Capsule())
                    .overlay(Capsule().strokeBorder(theme.colors.surface, lineWidth: 1.5))
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)
                    .padding(7)
            }
        }
        .aspectRatio(1.25, contentMode: .fit)
        .frame(maxWidth: .infinity)
        .clipped()
    }

    private var footer: some View {
        HStack(spacing: Space.sm) {
            Circle().fill(style.accent).frame(width: 7, height: 7)
            Text(item.name)
                .font(.ui(12, .semibold))
                .foregroundStyle(theme.colors.textPrimary)
                .lineLimit(2)
                .multilineTextAlignment(.leading)
                .frame(maxWidth: .infinity, alignment: .leading)
            Text(Money.format(item.basePriceMinor, currency))
                .font(.money(11, .bold))
                .foregroundStyle(theme.colors.textSecondary)
        }
        .padding(.horizontal, 10)
        .frame(height: 48)
        .frame(maxWidth: .infinity)
        .background(theme.colors.surface)
    }

    /// Up to two initials from the item name (Flutter's monogram rule).
    private var monogram: String {
        let words = item.name.split(whereSeparator: { $0.isWhitespace }).filter { !$0.isEmpty }
        if words.count >= 2 {
            return (String(words[0].prefix(1)) + String(words[1].prefix(1))).uppercased()
        }
        if let w = words.first { return String(w.prefix(2)).uppercased() }
        return "•"
    }
}

// MARK: - Category palette (HSL hue seeded by name — matches Compose Color.hsl)

struct CategoryStyle {
    let bgTop: Color
    let bgBottom: Color
    let accent: Color
}

/// A theme-aware gradient + accent for a category/item, hue-seeded by keyword
/// (coffee → warm brown, tea → green, …) so a family shares a recognizable
/// palette; unknown names fall back to a stable hash hue.
func categoryStyle(_ name: String, isDark: Bool) -> CategoryStyle {
    let (hue, sat) = categoryHueSat(name)
    if isDark {
        return CategoryStyle(
            bgTop: Color(h: hue, s: sat * 0.55, l: 0.175),
            bgBottom: Color(h: hue, s: sat * 0.60, l: 0.13),
            accent: Color(h: hue, s: sat, l: 0.62)
        )
    }
    return CategoryStyle(
        bgTop: Color(h: hue, s: sat, l: 0.945),
        bgBottom: Color(h: hue, s: sat, l: 0.875),
        accent: Color(h: hue, s: sat, l: 0.40)
    )
}

private func categoryHueSat(_ raw: String) -> (Double, Double) {
    let n = raw.lowercased()
    func has(_ keys: [String]) -> Bool { keys.contains { n.contains($0) } }
    if has(["matcha"]) { return (130, 0.45) }
    if has(["mocha", "chocolate", "cocoa"]) { return (16, 0.45) }
    if has(["coffee", "latte", "espresso", "cappuccino", "americano", "macchiato", "cortado", "flat white"]) { return (28, 0.40) }
    if has(["tea", "chai"]) { return (140, 0.38) }
    if has(["juice", "lemon", "orange", "mango", "berry", "smoothie"]) { return (45, 0.55) }
    if has(["water", "sparkling", "soda"]) { return (205, 0.38) }
    if has(["ice", "iced", "cold", "frapp", "shake"]) { return (200, 0.45) }
    if has(["pastry", "croissant", "cake", "waffle", "cookie", "muffin", "donut", "brownie"]) { return (38, 0.50) }
    if has(["sandwich", "burger", "chicken", "wrap", "toast", "bagel", "food"]) { return (22, 0.52) }
    if has(["affogato", "ice cream", "dessert", "gelato"]) { return (290, 0.42) }
    // Stable fallback hue from an FNV-1a hash of the name.
    var hash: UInt64 = 1469598103934665603
    for b in n.utf8 { hash = (hash ^ UInt64(b)) &* 1099511628211 }
    return (Double(hash % 360), 0.42)
}

extension Color {
    /// HSL → Color (h in degrees 0–360, s/l in 0–1). Matches Compose's
    /// `Color.hsl`, so the two hosts render identical category palettes.
    init(h: Double, s: Double, l: Double) {
        let c = (1 - abs(2 * l - 1)) * s
        let hp = (h.truncatingRemainder(dividingBy: 360) + 360).truncatingRemainder(dividingBy: 360) / 60
        let x = c * (1 - abs(hp.truncatingRemainder(dividingBy: 2) - 1))
        let (r1, g1, b1): (Double, Double, Double)
        switch hp {
        case 0..<1: (r1, g1, b1) = (c, x, 0)
        case 1..<2: (r1, g1, b1) = (x, c, 0)
        case 2..<3: (r1, g1, b1) = (0, c, x)
        case 3..<4: (r1, g1, b1) = (0, x, c)
        case 4..<5: (r1, g1, b1) = (x, 0, c)
        default: (r1, g1, b1) = (c, 0, x)
        }
        let m = l - c / 2
        self.init(red: r1 + m, green: g1 + m, blue: b1 + m)
    }
}
