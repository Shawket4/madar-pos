//! Cart — client-only, in-progress order state. The cart is NOT a wire model: it
//! lives entirely on the device, persists in `kv` so a half-built order survives
//! an app restart, and is the input to the pricing engine (`pricing::price_cart`,
//! the money source of truth). Totals are NEVER computed here — this module owns
//! line identity + quantity bookkeeping and delegates all money to `pricing`.
//!
//! This first cut keys a line by `item_id` (one line per menu item). Modifiers
//! (size, addons, optionals) land later: the line will then be keyed by a
//! signature over the full selection and `StoredLine` grows those fields — the
//! persisted shape is forward-compatible (extra fields default in).

use serde::{Deserialize, Serialize};

use crate::error::CoreResult;
use crate::pricing::{self, DiscountKind, PriceCartInput};
use crate::store::Store;

/// kv key — the whole cart is one JSON array.
pub(crate) const K_CART: &str = "cart:lines";

/// The persisted cart line. Kept minimal + forward-compatible: new optional
/// fields (size_id, addons…) can be added without breaking older blobs.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct StoredLine {
    item_id: String,
    name: String,
    unit_price_minor: i64,
    qty: i64,
}

/// A cart line as the host renders it (with the derived line total).
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct CartLineView {
    pub item_id: String,
    pub name: String,
    pub unit_price_minor: i64,
    pub qty: i64,
    pub line_total_minor: i64,
}

/// The priced cart summary the host shows in the cart panel + action-bar badge.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct CartTotals {
    /// Sum of quantities — the badge count on the cart button.
    pub item_count: i64,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub total_minor: i64,
}

// ── persistence ──────────────────────────────────────────────────────────────

/// Load the cart; an absent key (or a stale/corrupt blob) reads as empty so the
/// POS never wedges on a bad cache.
fn load(store: &Store) -> CoreResult<Vec<StoredLine>> {
    match store.kv_get(K_CART)? {
        Some(json) => Ok(serde_json::from_str(&json).unwrap_or_default()),
        None => Ok(Vec::new()),
    }
}

fn save(store: &Store, lines: &[StoredLine]) -> CoreResult<()> {
    store.kv_put(K_CART, &serde_json::to_string(lines)?)
}

fn view(lines: &[StoredLine]) -> Vec<CartLineView> {
    lines
        .iter()
        .map(|l| CartLineView {
            item_id: l.item_id.clone(),
            name: l.name.clone(),
            unit_price_minor: l.unit_price_minor,
            qty: l.qty,
            line_total_minor: l.unit_price_minor * l.qty,
        })
        .collect()
}

// ── operations (store in, updated views out) ─────────────────────────────────

/// All current cart lines (empty when none) — always succeeds offline.
pub(crate) fn lines(store: &Store) -> CoreResult<Vec<CartLineView>> {
    Ok(view(&load(store)?))
}

/// Add one unit of `item_id`: merge into the existing line (qty + 1) if present,
/// else append a new qty=1 line. Returns the updated lines.
pub(crate) fn add(
    store: &Store,
    item_id: &str,
    name: &str,
    unit_price_minor: i64,
) -> CoreResult<Vec<CartLineView>> {
    let mut lines = load(store)?;
    match lines.iter_mut().find(|l| l.item_id == item_id) {
        Some(l) => l.qty += 1,
        None => lines.push(StoredLine {
            item_id: item_id.to_string(),
            name: name.to_string(),
            unit_price_minor,
            qty: 1,
        }),
    }
    save(store, &lines)?;
    Ok(view(&lines))
}

/// Set the absolute quantity for a line; `qty <= 0` removes it.
pub(crate) fn set_qty(store: &Store, item_id: &str, qty: i64) -> CoreResult<Vec<CartLineView>> {
    let mut lines = load(store)?;
    if qty <= 0 {
        lines.retain(|l| l.item_id != item_id);
    } else if let Some(l) = lines.iter_mut().find(|l| l.item_id == item_id) {
        l.qty = qty;
    }
    save(store, &lines)?;
    Ok(view(&lines))
}

/// Remove a line entirely.
pub(crate) fn remove(store: &Store, item_id: &str) -> CoreResult<Vec<CartLineView>> {
    let mut lines = load(store)?;
    lines.retain(|l| l.item_id != item_id);
    save(store, &lines)?;
    Ok(view(&lines))
}

/// Empty the cart (e.g. after checkout or on sign-out).
pub(crate) fn clear(store: &Store) -> CoreResult<()> {
    save(store, &[])
}

/// Price the cart at `tax_rate` (an exclusive decimal fraction) via the pricing
/// engine. No discount in this phase — that arrives with the tender flow.
pub(crate) fn totals(store: &Store, tax_rate: f64) -> CoreResult<CartTotals> {
    let lines = load(store)?;
    let item_count = lines.iter().map(|l| l.qty).sum();
    let priced = pricing::price_cart(PriceCartInput {
        lines: lines
            .iter()
            .map(|l| pricing::CartLine {
                quantity: l.qty,
                unit_price: l.unit_price_minor,
                is_bundle: false,
                addons: vec![],
                optionals: vec![],
                bundle_components: vec![],
            })
            .collect(),
        discount_kind: DiscountKind::None,
        discount_value: 0,
        tax_rate,
        amount_tendered: None,
        cash_tip: 0,
    });
    Ok(CartTotals {
        item_count,
        subtotal_minor: priced.subtotal_minor,
        tax_minor: priced.tax_minor,
        total_minor: priced.total_minor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        Store::open("").unwrap()
    }

    #[test]
    fn add_merges_and_appends() {
        let s = store();
        assert!(lines(&s).unwrap().is_empty());

        add(&s, "a", "Latte", 5000).unwrap();
        let v = add(&s, "a", "Latte", 5000).unwrap(); // same item → qty 2
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].qty, 2);
        assert_eq!(v[0].line_total_minor, 10_000);

        let v = add(&s, "b", "Tea", 3000).unwrap(); // new item → second line
        assert_eq!(v.len(), 2);
        assert_eq!(v[1].item_id, "b");
        assert_eq!(v[1].qty, 1);
    }

    #[test]
    fn set_qty_updates_and_zero_removes() {
        let s = store();
        add(&s, "a", "Latte", 5000).unwrap();
        let v = set_qty(&s, "a", 4).unwrap();
        assert_eq!(v[0].qty, 4);
        assert_eq!(v[0].line_total_minor, 20_000);

        let v = set_qty(&s, "a", 0).unwrap(); // qty 0 removes the line
        assert!(v.is_empty());

        // Setting qty on a missing line is a no-op, not an error.
        let v = set_qty(&s, "ghost", 3).unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn remove_and_clear() {
        let s = store();
        add(&s, "a", "Latte", 5000).unwrap();
        add(&s, "b", "Tea", 3000).unwrap();
        let v = remove(&s, "a").unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].item_id, "b");

        clear(&s).unwrap();
        assert!(lines(&s).unwrap().is_empty());
    }

    #[test]
    fn persists_across_reload() {
        let s = store();
        add(&s, "a", "Latte", 5000).unwrap();
        add(&s, "a", "Latte", 5000).unwrap();
        // A fresh view read goes back through kv — same data.
        let v = lines(&s).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].qty, 2);
    }

    #[test]
    fn totals_price_through_the_engine() {
        let s = store();
        add(&s, "a", "Latte", 1000).unwrap(); // qty 1
        set_qty(&s, "a", 2).unwrap(); // 2 × 1000 = 2000
        add(&s, "b", "Tea", 500).unwrap(); // + 500 → subtotal 2500
        let t = totals(&s, 0.14).unwrap();
        assert_eq!(t.item_count, 3);
        assert_eq!(t.subtotal_minor, 2500);
        assert_eq!(t.tax_minor, 350); // round(2500 * 0.14)
        assert_eq!(t.total_minor, 2850);
    }

    #[test]
    fn empty_cart_totals_are_zero() {
        let s = store();
        let t = totals(&s, 0.14).unwrap();
        assert_eq!(
            t,
            CartTotals { item_count: 0, subtotal_minor: 0, tax_minor: 0, total_minor: 0 }
        );
    }
}
