//! The pricing engine — the single source of truth for order money.
//!
//! Pricing is **client-authoritative** in the Sufrix backend: `create_order`
//! records the POS-supplied `subtotal`/`discount`/`tax`/`total` **verbatim** and
//! only *flags* deviations (it never rejects). So whatever this module computes
//! IS the money, the receipt, and the revenue figure — there is no server safety
//! net. It is therefore a pure function with exhaustive golden-vector tests that
//! mirror the server formula (`orders/handlers.rs` + `discounts/handlers.rs`).
//!
//! Spec: `docs/05-domain-audit.md` §2.2 / PLAN.md. All amounts are integer
//! **minor-units** (piastres; 1 EGP = 100). Rules that MUST hold:
//! - rounding is **ties-away-from-zero** (Dart `double.round()` == Rust `f64::round()`),
//! - order is **subtotal → discount → tax-on-the-discounted-base → total**,
//! - exactly **two** rounding points (percentage discount, tax),
//! - a single org-wide **exclusive** tax rate,
//! - bundle base price is **fixed**; only component addons/optionals add on,
//! - the wire `price_modifier` per addon is the already-resolved charged delta —
//!   this module **trusts it** and never re-derives swap-family deltas.

/// Money in integer minor-units (piastres).
pub type MoneyMinor = i64;

/// Discount basis (mirrors the cart's discount handling).
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscountKind {
    /// No discount.
    None,
    /// `discount_value` is a percentage (e.g. `10` = 10%).
    Percentage,
    /// `discount_value` is a fixed amount in minor-units.
    Fixed,
}

/// A selected addon on a line (or bundle component). `price_modifier` is the
/// CHARGED delta already resolved at selection time (swap families clamp to ≥0
/// upstream); trusted verbatim here.
#[derive(uniffi::Record, Clone, Debug)]
pub struct AddonSel {
    pub price_modifier: MoneyMinor,
    pub quantity: i64,
}

/// A selected optional field. `price` is absolute (`0` == free).
#[derive(uniffi::Record, Clone, Debug)]
pub struct OptionalSel {
    pub price: MoneyMinor,
}

/// One configured component inside a bundle line. Only its addons + optionals
/// add money; the component's base/size price is **never** charged (the bundle's
/// fixed price already covers the components).
#[derive(uniffi::Record, Clone, Debug)]
pub struct BundleComponentSel {
    pub addons: Vec<AddonSel>,
    pub optionals: Vec<OptionalSel>,
}

/// A cart line. For a normal line, `unit_price` is the size-resolved absolute
/// price and extras come from `addons` + `optionals`. For a bundle line, set
/// `is_bundle = true`, `unit_price` = the fixed bundle price, and put the
/// per-component extras in `bundle_components`.
#[derive(uniffi::Record, Clone, Debug)]
pub struct CartLine {
    pub quantity: i64,
    pub unit_price: MoneyMinor,
    pub is_bundle: bool,
    pub addons: Vec<AddonSel>,
    pub optionals: Vec<OptionalSel>,
    pub bundle_components: Vec<BundleComponentSel>,
}

/// Everything needed to price a cart.
#[derive(uniffi::Record, Clone, Debug)]
pub struct PriceCartInput {
    pub lines: Vec<CartLine>,
    pub discount_kind: DiscountKind,
    /// Percentage (when `Percentage`) or fixed minor-units (when `Fixed`).
    pub discount_value: i64,
    /// Decimal fraction, **exclusive** (e.g. `0.14` = 14%). `0.0` = tax-free.
    pub tax_rate: f64,
    /// Cash handed over, if any (for change calc).
    pub amount_tendered: Option<MoneyMinor>,
    /// Cash portion of a tip, subtracted from change so the teller-visible
    /// change equals the recorded change (resolves doc 05 F4). Tip is NOT in
    /// the total. `0` when there's no cash tip.
    pub cash_tip: MoneyMinor,
}

/// The computed breakdown — integer minor-units. This is exactly what the host
/// renders and what the order payload carries.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct PricedBreakdown {
    pub subtotal_minor: MoneyMinor,
    pub discount_minor: MoneyMinor,
    pub taxable_minor: MoneyMinor,
    pub tax_minor: MoneyMinor,
    pub total_minor: MoneyMinor,
    pub change_given_minor: MoneyMinor,
}

/// Matches the cart's `clamp(0, 999999)` change ceiling.
const CHANGE_CAP: MoneyMinor = 999_999;

#[inline]
fn round_money(x: f64) -> MoneyMinor {
    // Dart `double.round()` is ties-away-from-zero; Rust `f64::round()` matches.
    x.round() as MoneyMinor
}

fn line_total(line: &CartLine) -> MoneyMinor {
    let extras: MoneyMinor = if line.is_bundle {
        line.bundle_components
            .iter()
            .map(|c| {
                c.addons.iter().map(|a| a.price_modifier * a.quantity).sum::<MoneyMinor>()
                    + c.optionals.iter().map(|o| o.price).sum::<MoneyMinor>()
            })
            .sum()
    } else {
        line.addons.iter().map(|a| a.price_modifier * a.quantity).sum::<MoneyMinor>()
            + line.optionals.iter().map(|o| o.price).sum::<MoneyMinor>()
    };
    (line.unit_price + extras) * line.quantity
}

/// Price a cart — pure; the money source of truth (see module docs).
///
/// FFI entry: hosts call this for live cart totals and to fill the order payload
/// they submit. Send the full breakdown on every order.
#[uniffi::export]
pub fn price_cart(input: PriceCartInput) -> PricedBreakdown {
    let subtotal: MoneyMinor = input.lines.iter().map(line_total).sum();

    // Discount, clamped to [0, subtotal] in EVERY branch (doc 05 F8: a >100%
    // percentage must not drive the total negative; fixed is capped likewise).
    let discount: MoneyMinor = match input.discount_kind {
        DiscountKind::None => 0,
        DiscountKind::Percentage => {
            // Match Dart eval order: (subtotal * value) as int → / 100.0 → round.
            let raw = round_money((subtotal * input.discount_value) as f64 / 100.0);
            raw.clamp(0, subtotal)
        }
        DiscountKind::Fixed => input.discount_value.clamp(0, subtotal),
    };

    let taxable = subtotal - discount;
    let tax = round_money(taxable as f64 * input.tax_rate);
    let total = taxable + tax;

    let change_given = match input.amount_tendered {
        None => 0,
        Some(t) => (t - total - input.cash_tip).clamp(0, CHANGE_CAP),
    };

    PricedBreakdown {
        subtotal_minor: subtotal,
        discount_minor: discount,
        taxable_minor: taxable,
        tax_minor: tax,
        total_minor: total,
        change_given_minor: change_given,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(unit: MoneyMinor, qty: i64) -> CartLine {
        CartLine {
            quantity: qty,
            unit_price: unit,
            is_bundle: false,
            addons: vec![],
            optionals: vec![],
            bundle_components: vec![],
        }
    }

    fn cart(lines: Vec<CartLine>, kind: DiscountKind, value: i64, tax: f64) -> PriceCartInput {
        PriceCartInput {
            lines,
            discount_kind: kind,
            discount_value: value,
            tax_rate: tax,
            amount_tendered: None,
            cash_tip: 0,
        }
    }

    #[test]
    fn simple_qty_tax_and_change() {
        let mut c = cart(vec![line(1000, 2)], DiscountKind::None, 0, 0.14);
        c.amount_tendered = Some(2500);
        let b = price_cart(c);
        assert_eq!(b.subtotal_minor, 2000);
        assert_eq!(b.discount_minor, 0);
        assert_eq!(b.taxable_minor, 2000);
        assert_eq!(b.tax_minor, 280); // round(2000 * 0.14)
        assert_eq!(b.total_minor, 2280);
        assert_eq!(b.change_given_minor, 220);
    }

    #[test]
    fn line_with_addons_and_optionals() {
        let l = CartLine {
            quantity: 1,
            unit_price: 1500,
            is_bundle: false,
            addons: vec![
                AddonSel { price_modifier: 500, quantity: 1 },
                AddonSel { price_modifier: 250, quantity: 2 },
            ],
            optionals: vec![OptionalSel { price: 300 }],
            bundle_components: vec![],
        };
        let b = price_cart(cart(vec![l], DiscountKind::None, 0, 0.0));
        // extras = 500 + 250*2 + 300 = 1300 ; (1500 + 1300) * 1
        assert_eq!(b.subtotal_minor, 2800);
        assert_eq!(b.total_minor, 2800);
    }

    #[test]
    fn percentage_discount_then_tax() {
        let b = price_cart(cart(vec![line(1000, 1)], DiscountKind::Percentage, 10, 0.14));
        assert_eq!(b.discount_minor, 100); // round(1000 * 10 / 100)
        assert_eq!(b.taxable_minor, 900);
        assert_eq!(b.tax_minor, 126); // round(900 * 0.14)
        assert_eq!(b.total_minor, 1026);
    }

    #[test]
    fn fixed_discount_normal() {
        let b = price_cart(cart(vec![line(1000, 1)], DiscountKind::Fixed, 250, 0.14));
        assert_eq!(b.discount_minor, 250);
        assert_eq!(b.taxable_minor, 750);
        assert_eq!(b.tax_minor, 105); // round(750 * 0.14)
        assert_eq!(b.total_minor, 855);
    }

    #[test]
    fn fixed_discount_caps_at_subtotal_100pct_off() {
        let b = price_cart(cart(vec![line(500, 1)], DiscountKind::Fixed, 800, 0.0));
        assert_eq!(b.discount_minor, 500); // clamped to subtotal
        assert_eq!(b.taxable_minor, 0);
        assert_eq!(b.total_minor, 0);
    }

    #[test]
    fn percentage_over_100_clamps_no_negative_total() {
        // doc 05 F8: a >100% percentage must clamp, never go negative.
        let b = price_cart(cart(vec![line(1000, 1)], DiscountKind::Percentage, 150, 0.14));
        assert_eq!(b.discount_minor, 1000); // round(1500) clamped to 1000
        assert_eq!(b.taxable_minor, 0);
        assert_eq!(b.tax_minor, 0);
        assert_eq!(b.total_minor, 0);
    }

    #[test]
    fn zero_tax_total_equals_taxable() {
        let b = price_cart(cart(vec![line(1234, 1)], DiscountKind::None, 0, 0.0));
        assert_eq!(b.tax_minor, 0);
        assert_eq!(b.total_minor, 1234);
    }

    #[test]
    fn rounding_is_ties_away_from_zero() {
        // 25 * 10 / 100 = 2.5 exactly → must round to 3 (away from zero), not 2.
        let b = price_cart(cart(vec![line(25, 1)], DiscountKind::Percentage, 10, 0.0));
        assert_eq!(b.discount_minor, 3);
        assert_eq!(b.total_minor, 22);
    }

    #[test]
    fn bundle_line_fixed_base_plus_component_extras() {
        let comp1 = BundleComponentSel {
            addons: vec![AddonSel { price_modifier: 200, quantity: 1 }],
            optionals: vec![OptionalSel { price: 150 }],
        };
        let comp2 = BundleComponentSel {
            addons: vec![],
            optionals: vec![OptionalSel { price: 100 }],
        };
        let bundle = CartLine {
            quantity: 2,
            unit_price: 5000, // fixed bundle price
            is_bundle: true,
            addons: vec![],
            optionals: vec![],
            bundle_components: vec![comp1, comp2],
        };
        let b = price_cart(cart(vec![bundle], DiscountKind::None, 0, 0.0));
        // extras = (200 + 150) + (100) = 450 ; (5000 + 450) * 2
        assert_eq!(b.subtotal_minor, 10_900);
        assert_eq!(b.total_minor, 10_900);
    }

    #[test]
    fn change_subtracts_cash_tip() {
        let mut c = cart(vec![line(1000, 1)], DiscountKind::None, 0, 0.0);
        c.amount_tendered = Some(1500);
        c.cash_tip = 200;
        let b = price_cart(c);
        assert_eq!(b.total_minor, 1000);
        assert_eq!(b.change_given_minor, 300); // 1500 - 1000 - 200
    }

    #[test]
    fn change_never_negative() {
        let mut c = cart(vec![line(1000, 1)], DiscountKind::None, 0, 0.0);
        c.amount_tendered = Some(900);
        let b = price_cart(c);
        assert_eq!(b.change_given_minor, 0);
    }

    #[test]
    fn empty_cart_is_zero() {
        let b = price_cart(cart(vec![], DiscountKind::None, 0, 0.14));
        assert_eq!(b, PricedBreakdown {
            subtotal_minor: 0,
            discount_minor: 0,
            taxable_minor: 0,
            tax_minor: 0,
            total_minor: 0,
            change_given_minor: 0,
        });
    }

    // ── additional golden vectors ────────────────────────────────────────────

    #[test]
    fn subtotal_sums_across_multiple_lines() {
        // 3 lines: 1000×2 + 500×1 + 250×4 = 2000 + 500 + 1000 = 3500.
        let b = price_cart(cart(
            vec![line(1000, 2), line(500, 1), line(250, 4)],
            DiscountKind::None,
            0,
            0.0,
        ));
        assert_eq!(b.subtotal_minor, 3500);
        assert_eq!(b.total_minor, 3500);
    }

    #[test]
    fn percentage_discount_rounds_half_up_at_minor_boundary() {
        // subtotal 5 × 10% = (5*10)/100 = 0.5 → ties-away → 1 (not 0).
        let b = price_cart(cart(vec![line(5, 1)], DiscountKind::Percentage, 10, 0.0));
        assert_eq!(b.discount_minor, 1);
        assert_eq!(b.taxable_minor, 4);
        assert_eq!(b.total_minor, 4);
    }

    #[test]
    fn percentage_discount_below_half_rounds_down() {
        // subtotal 4 × 10% = (4*10)/100 = 0.4 → rounds to 0.
        let b = price_cart(cart(vec![line(4, 1)], DiscountKind::Percentage, 10, 0.0));
        assert_eq!(b.discount_minor, 0);
        assert_eq!(b.total_minor, 4);
    }

    #[test]
    fn percentage_discount_exactly_100_zeroes_total() {
        let b = price_cart(cart(vec![line(1234, 3)], DiscountKind::Percentage, 100, 0.14));
        assert_eq!(b.subtotal_minor, 3702);
        assert_eq!(b.discount_minor, 3702); // 100% off → whole subtotal
        assert_eq!(b.taxable_minor, 0);
        assert_eq!(b.tax_minor, 0);
        assert_eq!(b.total_minor, 0);
    }

    #[test]
    fn percentage_discount_zero_value_is_no_discount() {
        let b = price_cart(cart(vec![line(1000, 1)], DiscountKind::Percentage, 0, 0.14));
        assert_eq!(b.discount_minor, 0);
        assert_eq!(b.taxable_minor, 1000);
        assert_eq!(b.tax_minor, 140);
        assert_eq!(b.total_minor, 1140);
    }

    #[test]
    fn fixed_discount_zero_value_is_no_discount() {
        let b = price_cart(cart(vec![line(1000, 1)], DiscountKind::Fixed, 0, 0.0));
        assert_eq!(b.discount_minor, 0);
        assert_eq!(b.total_minor, 1000);
    }

    #[test]
    fn fixed_discount_equal_to_subtotal_zeroes_total() {
        let b = price_cart(cart(vec![line(1000, 1)], DiscountKind::Fixed, 1000, 0.14));
        assert_eq!(b.discount_minor, 1000);
        assert_eq!(b.taxable_minor, 0);
        assert_eq!(b.tax_minor, 0);
        assert_eq!(b.total_minor, 0);
    }

    #[test]
    fn negative_discount_value_clamps_to_zero() {
        // A stray negative fixed value must never INCREASE the total.
        let b = price_cart(cart(vec![line(1000, 1)], DiscountKind::Fixed, -500, 0.0));
        assert_eq!(b.discount_minor, 0);
        assert_eq!(b.total_minor, 1000);
        // Same guard on the percentage path (clamp lower bound is 0).
        let b = price_cart(cart(vec![line(1000, 1)], DiscountKind::Percentage, -50, 0.0));
        assert_eq!(b.discount_minor, 0);
        assert_eq!(b.total_minor, 1000);
    }

    #[test]
    fn tax_rounds_half_away_from_zero_at_minor_boundary() {
        // taxable 5 × 0.10 = 0.5 → ties-away → 1.
        let b = price_cart(cart(vec![line(5, 1)], DiscountKind::None, 0, 0.10));
        assert_eq!(b.tax_minor, 1);
        assert_eq!(b.total_minor, 6);
    }

    #[test]
    fn tax_below_half_rounds_down_at_minor_boundary() {
        // taxable 4 × 0.10 = 0.4 → rounds to 0.
        let b = price_cart(cart(vec![line(4, 1)], DiscountKind::None, 0, 0.10));
        assert_eq!(b.tax_minor, 0);
        assert_eq!(b.total_minor, 4);
    }

    #[test]
    fn change_is_tendered_minus_total_no_tip() {
        let mut c = cart(vec![line(1000, 1)], DiscountKind::None, 0, 0.14);
        c.amount_tendered = Some(2000);
        let b = price_cart(c);
        assert_eq!(b.total_minor, 1140);
        assert_eq!(b.change_given_minor, 860); // 2000 - 1140
    }

    #[test]
    fn change_exact_payment_is_zero() {
        let mut c = cart(vec![line(1000, 1)], DiscountKind::None, 0, 0.0);
        c.amount_tendered = Some(1000);
        let b = price_cart(c);
        assert_eq!(b.change_given_minor, 0);
    }

    #[test]
    fn cash_tip_exceeding_overpayment_clamps_change_to_zero() {
        // Tendered exact, then a cash tip → change can't go negative.
        let mut c = cart(vec![line(1000, 1)], DiscountKind::None, 0, 0.0);
        c.amount_tendered = Some(1000);
        c.cash_tip = 200;
        let b = price_cart(c);
        assert_eq!(b.change_given_minor, 0); // (1000 - 1000 - 200).clamp(0, ..) = 0
    }

    #[test]
    fn no_tender_means_no_change_even_with_cash_tip() {
        // amount_tendered None (card path) → change is 0 regardless of cash_tip.
        let mut c = cart(vec![line(1000, 1)], DiscountKind::None, 0, 0.0);
        c.cash_tip = 200;
        let b = price_cart(c);
        assert_eq!(b.change_given_minor, 0);
    }

    #[test]
    fn change_is_capped_at_the_change_ceiling() {
        // A wildly-large tender must clamp to the CHANGE_CAP, never overflow.
        let mut c = cart(vec![line(100, 1)], DiscountKind::None, 0, 0.0);
        c.amount_tendered = Some(i64::MAX / 2);
        let b = price_cart(c);
        assert_eq!(b.change_given_minor, CHANGE_CAP);
    }

    #[test]
    fn bundle_extras_scale_with_bundle_qty_not_component_qty() {
        // Two bundle lines (qty 3) each with one component up-charge of 250.
        let comp = BundleComponentSel {
            addons: vec![AddonSel { price_modifier: 250, quantity: 1 }],
            optionals: vec![],
        };
        let bundle = CartLine {
            quantity: 3,
            unit_price: 4000,
            is_bundle: true,
            addons: vec![],
            optionals: vec![],
            bundle_components: vec![comp],
        };
        let b = price_cart(cart(vec![bundle], DiscountKind::None, 0, 0.0));
        // (4000 + 250) × 3 = 12_750.
        assert_eq!(b.subtotal_minor, 12_750);
    }

    #[test]
    fn bundle_ignores_top_level_addons_optionals() {
        // For a bundle line, only bundle_components add money; the line's own
        // addons/optionals are NOT charged (the line_total bundle branch).
        let bundle = CartLine {
            quantity: 1,
            unit_price: 5000,
            is_bundle: true,
            addons: vec![AddonSel { price_modifier: 9999, quantity: 5 }], // ignored
            optionals: vec![OptionalSel { price: 8888 }],                  // ignored
            bundle_components: vec![BundleComponentSel { addons: vec![], optionals: vec![] }],
        };
        let b = price_cart(cart(vec![bundle], DiscountKind::None, 0, 0.0));
        assert_eq!(b.subtotal_minor, 5000); // only the fixed base
    }

    #[test]
    fn addon_quantity_multiplies_the_modifier() {
        // A normal line: unit 1000 + addon(price 300 × qty 3) = 1900.
        let l = CartLine {
            quantity: 1,
            unit_price: 1000,
            is_bundle: false,
            addons: vec![AddonSel { price_modifier: 300, quantity: 3 }],
            optionals: vec![],
            bundle_components: vec![],
        };
        let b = price_cart(cart(vec![l], DiscountKind::None, 0, 0.0));
        assert_eq!(b.subtotal_minor, 1900);
    }

    #[test]
    fn large_amounts_do_not_overflow_i64() {
        // 1_000_000 minor (10k EGP) × 50 lines worth, 14% tax — well within i64.
        let b = price_cart(cart(vec![line(1_000_000, 50)], DiscountKind::None, 0, 0.14));
        assert_eq!(b.subtotal_minor, 50_000_000);
        assert_eq!(b.tax_minor, 7_000_000);
        assert_eq!(b.total_minor, 57_000_000);
    }

    #[test]
    fn zero_unit_price_line_contributes_only_its_extras() {
        // A free item with a paid optional: 0 base + 300 optional, qty 2 → 600.
        let l = CartLine {
            quantity: 2,
            unit_price: 0,
            is_bundle: false,
            addons: vec![],
            optionals: vec![OptionalSel { price: 300 }],
            bundle_components: vec![],
        };
        let b = price_cart(cart(vec![l], DiscountKind::None, 0, 0.0));
        assert_eq!(b.subtotal_minor, 600);
    }

    #[test]
    fn full_golden_vector_discount_then_tax_then_change() {
        // End-to-end: subtotal 2000, 10% discount → 200, taxable 1800, 14% tax →
        // round(252.0)=252, total 2052, tendered 3000, cash tip 100 → change 848.
        let mut c = cart(vec![line(1000, 2)], DiscountKind::Percentage, 10, 0.14);
        c.amount_tendered = Some(3000);
        c.cash_tip = 100;
        let b = price_cart(c);
        assert_eq!(b.subtotal_minor, 2000);
        assert_eq!(b.discount_minor, 200);
        assert_eq!(b.taxable_minor, 1800);
        assert_eq!(b.tax_minor, 252);
        assert_eq!(b.total_minor, 2052);
        assert_eq!(b.change_given_minor, 848); // 3000 - 2052 - 100
    }
}
