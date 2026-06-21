//! Category styling — port of the Flutter `CatStyle` (category_style.dart).
//!
//! Maps a category/item name to a themed icon key + a colour palette derived
//! from a per-category seed hue, so light and dark stay coherent without a
//! hand-picked pastel table. All the logic (seed matching + HSL→RGB) lives here
//! so both hosts render byte-identical gradients; the hosts only map the small
//! `icon` key set to their native glyph (SF Symbol / emoji).

/// A resolved category style: an icon key + four hex colours (`#RRGGBB`).
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct CatStyleView {
    /// Icon family key — the host maps it to a platform glyph. One of:
    /// coffee, mocha, bakery, lunch, icecream, drink, tea, water, ice, matcha, cafe.
    pub icon: String,
    pub bg_top: String,
    pub bg_bottom: String,
    pub icon_color: String,
    pub accent: String,
}

/// Resolve the style for `name` in light or dark mode (Flutter `CatStyle.of`).
pub fn category_style(name: &str, dark: bool) -> CatStyleView {
    let (icon, hue, sat) = seed(&name.to_lowercase());
    from_hue(icon, hue, sat, dark)
}

/// (icon key, hue°, saturation 0..1) per category family — mirrors `_seed`.
fn seed(n: &str) -> (&'static str, f64, f64) {
    let has = |needle: &str| n.contains(needle);
    if has("matcha") {
        return ("matcha", 130.0, 0.45);
    }
    if has("latte")
        || has("espresso")
        || has("americano")
        || has("cappuc")
        || has("flat")
        || has("cortado")
        || has("coffee")
        || has("v60")
        || has("blended")
        || has("cold brew")
    {
        return ("coffee", 28.0, 0.38);
    }
    if has("chocolate") || has("mocha") {
        return ("mocha", 8.0, 0.35);
    }
    if has("croissant")
        || has("brownie")
        || has("cookie")
        || has("pastry")
        || has("pastries")
        || has("cake")
        || has("waffle")
    {
        return ("bakery", 40.0, 0.60);
    }
    if has("sandwich") || has("chicken") || has("turkey") || has("food") {
        return ("lunch", 22.0, 0.62);
    }
    if has("affogato") || has("ice cream") {
        return ("icecream", 285.0, 0.42);
    }
    if has("lemon") || has("lemonade") || has("refresher") || has("juice") {
        return ("drink", 52.0, 0.65);
    }
    if has("tea") || has("chai") {
        return ("tea", 140.0, 0.42);
    }
    if has("water") || has("sparkling") {
        return ("water", 210.0, 0.55);
    }
    if has("iced") {
        return ("ice", 200.0, 0.55);
    }
    ("cafe", 24.0, 0.32)
}

/// Build the palette from a seed hue (mirrors `_fromHue`).
fn from_hue(icon: &str, hue: f64, sat: f64, dark: bool) -> CatStyleView {
    let hsl = |s: f64, l: f64| hsl_to_hex(hue, s, l);
    if dark {
        CatStyleView {
            icon: icon.into(),
            bg_top: hsl(sat * 0.55, 0.175),
            bg_bottom: hsl(sat * 0.6, 0.13),
            icon_color: hsl(sat, 0.72),
            accent: hsl(sat, 0.62),
        }
    } else {
        CatStyleView {
            icon: icon.into(),
            bg_top: hsl(sat, 0.945),
            bg_bottom: hsl(sat, 0.875),
            icon_color: hsl(sat, 0.30),
            accent: hsl(sat, 0.40),
        }
    }
}

/// HSL (hue° 0..360, sat/light 0..1) → `#RRGGBB`. Matches Flutter's
/// `HSLColor.fromAHSL(1, h, s, l).toColor()`.
fn hsl_to_hex(h: f64, s: f64, l: f64) -> String {
    let h = h.rem_euclid(360.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    format!("#{:02X}{:02X}{:02X}", to(r1), to(g1), to(b1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_families_to_icons() {
        assert_eq!(category_style("Iced Latte", false).icon, "coffee");
        assert_eq!(category_style("Matcha Latte", false).icon, "matcha"); // matcha wins over latte
        assert_eq!(category_style("Croissant", false).icon, "bakery");
        assert_eq!(category_style("Chicken Sandwich", false).icon, "lunch");
        assert_eq!(category_style("Mystery", false).icon, "cafe");
    }

    #[test]
    fn hsl_to_hex_matches_known_points() {
        assert_eq!(hsl_to_hex(0.0, 1.0, 0.5), "#FF0000"); // pure red
        assert_eq!(hsl_to_hex(120.0, 1.0, 0.5), "#00FF00"); // pure green
        assert_eq!(hsl_to_hex(0.0, 0.0, 1.0), "#FFFFFF"); // white
        assert_eq!(hsl_to_hex(0.0, 0.0, 0.0), "#000000"); // black
    }

    #[test]
    fn light_and_dark_differ_and_are_valid_hex() {
        let light = category_style("Latte", false);
        let dark = category_style("Latte", true);
        assert_ne!(light.bg_top, dark.bg_top);
        for hex in [&light.bg_top, &light.accent, &dark.icon_color] {
            assert_eq!(hex.len(), 7);
            assert!(hex.starts_with('#'));
            assert!(hex[1..].chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    // ── full icon-family coverage (every `seed` branch) ─────────────────────

    #[test]
    fn maps_every_seed_family() {
        // matcha (precedence: before latte/coffee — also covered below explicitly)
        assert_eq!(category_style("matcha", false).icon, "matcha");
        // coffee family — a representative from each keyword.
        for n in ["Latte", "Espresso", "Americano", "Cappuccino", "Flat White",
                  "Cortado", "Drip Coffee", "V60", "Blended", "Cold Brew"] {
            assert_eq!(category_style(n, false).icon, "coffee", "{n}");
        }
        // mocha family.
        assert_eq!(category_style("Hot Chocolate", false).icon, "mocha");
        assert_eq!(category_style("Mocha", false).icon, "mocha");
        // bakery family.
        for n in ["Croissant", "Brownie", "Cookie", "Pastry", "Pastries", "Cake", "Waffle"] {
            assert_eq!(category_style(n, false).icon, "bakery", "{n}");
        }
        // lunch family.
        for n in ["Sandwich", "Chicken Wrap", "Turkey Club", "Comfort Food"] {
            assert_eq!(category_style(n, false).icon, "lunch", "{n}");
        }
        // icecream family.
        assert_eq!(category_style("Affogato", false).icon, "icecream");
        assert_eq!(category_style("Ice Cream", false).icon, "icecream");
        // drink family.
        for n in ["Lemon", "Lemonade", "Berry Refresher", "Orange Juice"] {
            assert_eq!(category_style(n, false).icon, "drink", "{n}");
        }
        // tea family.
        assert_eq!(category_style("Green Tea", false).icon, "tea");
        assert_eq!(category_style("Chai", false).icon, "tea");
        // water family.
        assert_eq!(category_style("Water", false).icon, "water");
        assert_eq!(category_style("Sparkling", false).icon, "water");
        // ice family ("iced" but NOT matched earlier by a more specific keyword).
        assert_eq!(category_style("Iced", false).icon, "ice");
    }

    #[test]
    fn matcha_beats_latte_precedence() {
        // "Matcha Latte" contains both "matcha" and "latte"; matcha is checked
        // first, so it must win.
        assert_eq!(category_style("Matcha Latte", false).icon, "matcha");
        assert_eq!(category_style("Iced Matcha", false).icon, "matcha");
    }

    #[test]
    fn iced_coffee_resolves_to_coffee_not_ice() {
        // "Iced Latte" contains both "iced" and "latte"; the coffee branch is
        // checked before the ice branch, so coffee wins (regression-style guard).
        assert_eq!(category_style("Iced Latte", false).icon, "coffee");
        assert_eq!(category_style("Iced Americano", false).icon, "coffee");
    }

    #[test]
    fn unknown_name_falls_back_to_cafe() {
        assert_eq!(category_style("", false).icon, "cafe");
        assert_eq!(category_style("Zzzbloop", false).icon, "cafe");
        assert_eq!(category_style("Mystery", true).icon, "cafe");
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert_eq!(category_style("LATTE", false).icon, "coffee");
        assert_eq!(category_style("MaTcHa", false).icon, "matcha");
        assert_eq!(category_style("CROISSANT", false).icon, "bakery");
    }

    // ── hsl_to_hex extra known points & properties ──────────────────────────

    #[test]
    fn hsl_to_hex_more_known_points() {
        assert_eq!(hsl_to_hex(240.0, 1.0, 0.5), "#0000FF"); // pure blue
        assert_eq!(hsl_to_hex(60.0, 1.0, 0.5), "#FFFF00"); // yellow
        assert_eq!(hsl_to_hex(180.0, 1.0, 0.5), "#00FFFF"); // cyan
        assert_eq!(hsl_to_hex(300.0, 1.0, 0.5), "#FF00FF"); // magenta
        // 50% grey: s=0 → all channels equal, l=0.5 → 128 (0x80).
        assert_eq!(hsl_to_hex(0.0, 0.0, 0.5), "#808080");
    }

    #[test]
    fn hsl_to_hex_hue_wraps_modulo_360() {
        // hue is normalized via rem_euclid(360); 360 == 0, 480 == 120, and a
        // negative hue wraps to the positive equivalent.
        assert_eq!(hsl_to_hex(360.0, 1.0, 0.5), hsl_to_hex(0.0, 1.0, 0.5));
        assert_eq!(hsl_to_hex(480.0, 1.0, 0.5), hsl_to_hex(120.0, 1.0, 0.5));
        assert_eq!(hsl_to_hex(-120.0, 1.0, 0.5), hsl_to_hex(240.0, 1.0, 0.5));
    }

    #[test]
    fn hsl_to_hex_always_seven_char_uppercase_hex() {
        // Sweep a range of hues/sats/lights; output is always `#RRGGBB` uppercase.
        for h in [0.0, 45.0, 130.0, 200.0, 285.0, 359.0] {
            for s in [0.0, 0.3, 0.65, 1.0] {
                for l in [0.0, 0.13, 0.5, 0.945, 1.0] {
                    let hex = hsl_to_hex(h, s, l);
                    assert_eq!(hex.len(), 7, "{h} {s} {l} -> {hex}");
                    assert!(hex.starts_with('#'));
                    assert!(
                        hex[1..].chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase()),
                        "{hex} not uppercase hex"
                    );
                }
            }
        }
    }

    #[test]
    fn palette_light_vs_dark_diverges_across_all_fields() {
        let light = category_style("Espresso", false);
        let dark = category_style("Espresso", true);
        assert_eq!(light.icon, dark.icon); // same family
        assert_ne!(light.bg_top, dark.bg_top);
        assert_ne!(light.bg_bottom, dark.bg_bottom);
        assert_ne!(light.icon_color, dark.icon_color);
        assert_ne!(light.accent, dark.accent);
    }

    #[test]
    fn category_style_is_deterministic() {
        // Same input → byte-identical palette (both hosts depend on this).
        assert_eq!(category_style("Cappuccino", true), category_style("Cappuccino", true));
        assert_eq!(category_style("Lemonade", false), category_style("Lemonade", false));
    }
}
