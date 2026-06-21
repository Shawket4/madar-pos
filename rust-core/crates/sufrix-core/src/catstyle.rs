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
}
