#![no_main]
//! Coverage-guided fuzzing of the cart money engine. Builds a bounded, realistic
//! cart from the raw bytes (carts are bounded in practice — we fuzz the LOGIC and
//! safety invariants, not i64 overflow on absurd values) and asserts the engine
//! never violates them, whatever the input.
use arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use madar_core::pricing::{
    price_cart, AddonSel, CartLine, DiscountKind, OptionalSel, PriceCartInput,
};

fn build_input(u: &mut Unstructured) -> arbitrary::Result<PriceCartInput> {
    let n_lines = u.int_in_range(0..=8)?;
    let mut lines = Vec::with_capacity(n_lines as usize);
    for _ in 0..n_lines {
        let n_add = u.int_in_range(0..=4)?;
        let mut addons = Vec::with_capacity(n_add as usize);
        for _ in 0..n_add {
            addons.push(AddonSel {
                price_modifier: u.int_in_range(0..=100_000)?,
                quantity: u.int_in_range(0..=20)?,
            });
        }
        let n_opt = u.int_in_range(0..=4)?;
        let mut optionals = Vec::with_capacity(n_opt as usize);
        for _ in 0..n_opt {
            optionals.push(OptionalSel { price: u.int_in_range(0..=100_000)? });
        }
        lines.push(CartLine {
            quantity: u.int_in_range(0..=1000)?,
            unit_price: u.int_in_range(0..=1_000_000)?,
            is_bundle: false,
            addons,
            optionals,
            bundle_components: vec![],
        });
    }
    let discount_kind =
        *u.choose(&[DiscountKind::None, DiscountKind::Percentage, DiscountKind::Fixed])?;
    Ok(PriceCartInput {
        lines,
        discount_kind,
        discount_value: u.int_in_range(0..=200)?, // % up to 200 exercises the >100% clamp
        tax_rate: (u.int_in_range(0..=50)? as f64) / 100.0,
        amount_tendered: if u.arbitrary()? {
            Some(u.int_in_range(0..=1_000_000_000)?)
        } else {
            None
        },
        cash_tip: u.int_in_range(0..=100_000)?,
    })
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    if let Ok(input) = build_input(&mut u) {
        let out = price_cart(input);
        assert!(out.discount_minor >= 0, "discount negative");
        assert!(out.discount_minor <= out.subtotal_minor, "discount > subtotal");
        assert_eq!(out.taxable_minor, out.subtotal_minor - out.discount_minor);
        assert!(out.taxable_minor >= 0, "taxable negative");
        assert!(out.tax_minor >= 0, "tax negative");
        assert!(out.total_minor >= 0, "total negative");
        assert!(out.total_minor >= out.taxable_minor, "tax made total < taxable");
        assert!(out.change_given_minor >= 0, "change negative");
        assert!(out.change_given_minor <= 999_999, "change over cap");
    }
});
