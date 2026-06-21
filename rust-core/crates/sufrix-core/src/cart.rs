//! Cart — client-only, in-progress order state. The cart is NOT a wire model: it
//! lives entirely on the device, persists in `kv` so a half-built order survives
//! an app restart, and is the input to the pricing engine (`pricing::price_cart`,
//! the money source of truth). This module owns line identity + the *charged*
//! price resolution (size, addon swap-delta vs additive, optionals — mirroring
//! the Flutter ItemDetailSheet), recording the resolved prices VERBATIM so an
//! offline receipt equals the server's record. Totals still go through `pricing`.
//!
//! A line is keyed by a SIGNATURE over its full selection (item + size + addons +
//! optionals + notes) so identical configs merge and distinct ones don't. A
//! simple (option-less) line's signature is just its `item_id`, so the basic
//! add/qty/remove path is unchanged. `StoredLine` is forward-compatible: the
//! modifier fields default in, so older blobs still load.

use serde::{Deserialize, Serialize};
use sufrix_api::models;

use crate::error::CoreResult;
use crate::menu;
use crate::pricing::{self, DiscountKind, PriceCartInput};
use crate::store::Store;

/// kv key — the whole cart is one JSON array.
pub(crate) const K_CART: &str = "cart:lines";
/// kv key — the selected discount id (empty = none).
pub(crate) const K_DISCOUNT: &str = "cart:discount";

#[derive(Serialize, Deserialize, Clone, Debug)]
struct StoredAddon {
    addon_item_id: String,
    name: String,
    /// The CHARGED delta (swap families) or full price (additive), per unit.
    price_modifier_minor: i64,
    qty: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct StoredOptional {
    optional_field_id: String,
    name: String,
    price_minor: i64,
}

/// The persisted cart line. New modifier fields default in (forward-compatible).
/// Opaque to callers — only `resolve_line`/`add_resolved` construct/consume it.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct StoredLine {
    item_id: String,
    name: String,
    /// Size-resolved absolute unit price (before addons/optionals).
    unit_price_minor: i64,
    qty: i64,
    #[serde(default)]
    size_label: Option<String>,
    #[serde(default)]
    addons: Vec<StoredAddon>,
    #[serde(default)]
    optionals: Vec<StoredOptional>,
    #[serde(default)]
    notes: Option<String>,
}

/// A host-supplied addon choice (id + how many). The CORE resolves its price.
#[derive(uniffi::Record, Clone, Debug)]
pub struct AddonSelection {
    pub addon_item_id: String,
    pub qty: i64,
}

/// An addon offered for an item, with its CHARGED price already resolved (swap
/// delta / full) — so the customization sheet just displays it, no pricing rules
/// in the UI. Grouped by `addon_type` by the host (per slot / global card).
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct ItemAddonView {
    pub addon_item_id: String,
    pub name: String,
    pub addon_type: String,
    pub charged_price_minor: i64,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct CartAddonView {
    pub addon_item_id: String,
    pub name: String,
    pub qty: i64,
    pub price_modifier_minor: i64,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct CartOptionalView {
    pub optional_field_id: String,
    pub name: String,
    pub price_minor: i64,
}

/// A cart line as the host renders it (with the derived line total).
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct CartLineView {
    /// Stable line key (the selection signature) — use for set_qty/remove/edit.
    pub key: String,
    pub item_id: String,
    pub name: String,
    pub size_label: Option<String>,
    pub addons: Vec<CartAddonView>,
    pub optionals: Vec<CartOptionalView>,
    pub notes: Option<String>,
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
    pub discount_minor: i64,
    pub tax_minor: i64,
    pub total_minor: i64,
}

// ── persistence ──────────────────────────────────────────────────────────────

fn load(store: &Store) -> CoreResult<Vec<StoredLine>> {
    match store.kv_get(K_CART)? {
        Some(json) => Ok(serde_json::from_str(&json).unwrap_or_default()),
        None => Ok(Vec::new()),
    }
}

fn save(store: &Store, lines: &[StoredLine]) -> CoreResult<()> {
    store.kv_put(K_CART, &serde_json::to_string(lines)?)
}

// ── pricing helpers (the line-money rules, mirrored from cart.dart) ───────────

fn line_extras(l: &StoredLine) -> i64 {
    l.addons.iter().map(|a| a.price_modifier_minor * a.qty).sum::<i64>()
        + l.optionals.iter().map(|o| o.price_minor).sum::<i64>()
}

fn line_total(l: &StoredLine) -> i64 {
    (l.unit_price_minor + line_extras(l)) * l.qty
}

/// The line key: a deterministic signature over the full selection. Option-less
/// lines key by `item_id` so the basic add/qty/remove path stays stable.
fn signature(l: &StoredLine) -> String {
    if l.size_label.is_none() && l.addons.is_empty() && l.optionals.is_empty() && l.notes.is_none() {
        return l.item_id.clone();
    }
    let mut addons: Vec<String> =
        l.addons.iter().map(|a| format!("{}:{}", a.addon_item_id, a.qty)).collect();
    addons.sort();
    let mut opts: Vec<String> = l.optionals.iter().map(|o| o.optional_field_id.clone()).collect();
    opts.sort();
    format!(
        "{}|{}|{}|{}|{}",
        l.item_id,
        l.size_label.as_deref().unwrap_or(""),
        addons.join(","),
        opts.join(","),
        l.notes.as_deref().unwrap_or(""),
    )
}

fn view(lines: &[StoredLine]) -> Vec<CartLineView> {
    lines
        .iter()
        .map(|l| CartLineView {
            key: signature(l),
            item_id: l.item_id.clone(),
            name: l.name.clone(),
            size_label: l.size_label.clone(),
            addons: l
                .addons
                .iter()
                .map(|a| CartAddonView {
                    addon_item_id: a.addon_item_id.clone(),
                    name: a.name.clone(),
                    qty: a.qty,
                    price_modifier_minor: a.price_modifier_minor,
                })
                .collect(),
            optionals: l
                .optionals
                .iter()
                .map(|o| CartOptionalView {
                    optional_field_id: o.optional_field_id.clone(),
                    name: o.name.clone(),
                    price_minor: o.price_minor,
                })
                .collect(),
            notes: l.notes.clone(),
            unit_price_minor: l.unit_price_minor,
            qty: l.qty,
            line_total_minor: line_total(l),
        })
        .collect()
}

/// Charged addon price: swap families (milk_type) pay only the delta over the
/// item's default-milk base (clamped ≥0); everything else (additive, and
/// coffee_type which carries no base in the catalog) pays the full default —
/// exactly the Flutter `_adjustedPrice`.
fn adjusted_addon_price(a: &menu::AddonItemView, milk_base: i64) -> i64 {
    if a.addon_type == "milk_type" {
        (a.default_price_minor - milk_base).max(0)
    } else {
        a.default_price_minor
    }
}

/// Resolve a configured line's charged prices from the cached catalog. PURE so
/// the money rules are exhaustively unit-testable. Unknown addon/optional ids
/// are dropped (defensive — a stale cache must not wedge a sale).
pub(crate) fn resolve_line(
    item: &menu::MenuItemView,
    addon_catalog: &[menu::AddonItemView],
    size_label: Option<String>,
    addon_sels: &[AddonSelection],
    optional_ids: &[String],
    qty: i64,
    notes: Option<String>,
) -> StoredLine {
    let unit_price = match &size_label {
        Some(lbl) => item
            .sizes
            .iter()
            .find(|s| &s.label == lbl)
            .map(|s| s.price_minor)
            .unwrap_or(item.base_price_minor),
        None => item.base_price_minor,
    };
    let milk_base = item
        .default_milk_addon_id
        .as_ref()
        .and_then(|id| addon_catalog.iter().find(|a| &a.id == id))
        .map(|a| a.default_price_minor)
        .unwrap_or(0);

    let addons = addon_sels
        .iter()
        .filter_map(|sel| {
            let a = addon_catalog.iter().find(|x| x.id == sel.addon_item_id)?;
            Some(StoredAddon {
                addon_item_id: a.id.clone(),
                name: a.name.clone(),
                price_modifier_minor: adjusted_addon_price(a, milk_base),
                qty: sel.qty.max(1),
            })
        })
        .collect();

    let optionals = optional_ids
        .iter()
        .filter_map(|oid| {
            let o = item.optional_fields.iter().find(|f| &f.id == oid)?;
            Some(StoredOptional {
                optional_field_id: o.id.clone(),
                name: o.name.clone(),
                price_minor: o.price_minor,
            })
        })
        .collect();

    StoredLine {
        item_id: item.id.clone(),
        name: item.name.clone(),
        unit_price_minor: unit_price,
        qty: qty.max(1),
        size_label,
        addons,
        optionals,
        notes,
    }
}

/// Every active addon offered for `item`, with its charged price resolved (the
/// swap rule lives here, not in the UI). The host groups by `addon_type`.
pub(crate) fn item_addons(
    item: &menu::MenuItemView,
    addon_catalog: &[menu::AddonItemView],
) -> Vec<ItemAddonView> {
    let milk_base = item
        .default_milk_addon_id
        .as_ref()
        .and_then(|id| addon_catalog.iter().find(|a| &a.id == id))
        .map(|a| a.default_price_minor)
        .unwrap_or(0);
    addon_catalog
        .iter()
        .filter(|a| a.is_active)
        .map(|a| ItemAddonView {
            addon_item_id: a.id.clone(),
            name: a.name.clone(),
            addon_type: a.addon_type.clone(),
            charged_price_minor: adjusted_addon_price(a, milk_base),
        })
        .collect()
}

// ── operations (store in, updated views out) ─────────────────────────────────

pub(crate) fn lines(store: &Store) -> CoreResult<Vec<CartLineView>> {
    Ok(view(&load(store)?))
}

/// Push a resolved line, merging into an identical existing line (same key).
pub(crate) fn add_resolved(store: &Store, line: StoredLine) -> CoreResult<Vec<CartLineView>> {
    let mut lines = load(store)?;
    let sig = signature(&line);
    match lines.iter_mut().find(|l| signature(l) == sig) {
        Some(l) => l.qty += line.qty,
        None => lines.push(line),
    }
    save(store, &lines)?;
    Ok(view(&lines))
}

/// Add one unit of an option-less item (the basic catalog tap).
pub(crate) fn add(
    store: &Store,
    item_id: &str,
    name: &str,
    unit_price_minor: i64,
) -> CoreResult<Vec<CartLineView>> {
    add_resolved(
        store,
        StoredLine {
            item_id: item_id.to_string(),
            name: name.to_string(),
            unit_price_minor,
            qty: 1,
            size_label: None,
            addons: vec![],
            optionals: vec![],
            notes: None,
        },
    )
}

/// Set the absolute quantity for a line (by its key); `qty <= 0` removes it.
pub(crate) fn set_qty(store: &Store, line_key: &str, qty: i64) -> CoreResult<Vec<CartLineView>> {
    let mut lines = load(store)?;
    if qty <= 0 {
        lines.retain(|l| signature(l) != line_key);
    } else if let Some(l) = lines.iter_mut().find(|l| signature(l) == line_key) {
        l.qty = qty;
    }
    save(store, &lines)?;
    Ok(view(&lines))
}

/// Remove a line entirely (by its key).
pub(crate) fn remove(store: &Store, line_key: &str) -> CoreResult<Vec<CartLineView>> {
    let mut lines = load(store)?;
    lines.retain(|l| signature(l) != line_key);
    save(store, &lines)?;
    Ok(view(&lines))
}

/// Empty the cart + its discount (e.g. after checkout or on sign-out).
pub(crate) fn clear(store: &Store) -> CoreResult<()> {
    clear_discount(store)?;
    save(store, &[])
}

// ── discount ─────────────────────────────────────────────────────────────────

pub(crate) fn set_discount(store: &Store, discount_id: &str) -> CoreResult<()> {
    store.kv_put(K_DISCOUNT, discount_id)
}
pub(crate) fn clear_discount(store: &Store) -> CoreResult<()> {
    store.kv_put(K_DISCOUNT, "")
}
/// The selected discount id, or `None`.
pub(crate) fn discount_id(store: &Store) -> CoreResult<Option<String>> {
    Ok(store.kv_get(K_DISCOUNT)?.filter(|s| !s.is_empty() && s != "null"))
}

/// Resolve the cart's selected discount → (kind, value) from the cached catalog.
/// Inactive / absent / unknown → no discount. The pricing engine then clamps it.
pub(crate) fn discount(store: &Store) -> CoreResult<(DiscountKind, i64)> {
    let id = match discount_id(store)? {
        Some(id) => id,
        None => return Ok((DiscountKind::None, 0)),
    };
    let raw: Vec<models::Discount> = match store.kv_get(menu::K_DISCOUNTS)? {
        Some(j) => serde_json::from_str(&j).unwrap_or_default(),
        None => Vec::new(),
    };
    match raw.iter().find(|d| d.id.to_string() == id && d.is_active) {
        Some(d) => Ok((kind_from_dtype(&d.dtype), d.value as i64)),
        None => Ok((DiscountKind::None, 0)),
    }
}

fn kind_from_dtype(dtype: &str) -> DiscountKind {
    match dtype {
        "percentage" => DiscountKind::Percentage,
        "fixed" => DiscountKind::Fixed,
        _ => DiscountKind::None,
    }
}

/// Map a stored line to the pricing engine's `CartLine` (the money input).
fn priced(l: &StoredLine) -> pricing::CartLine {
    pricing::CartLine {
        quantity: l.qty,
        unit_price: l.unit_price_minor,
        is_bundle: false,
        addons: l
            .addons
            .iter()
            .map(|a| pricing::AddonSel { price_modifier: a.price_modifier_minor, quantity: a.qty })
            .collect(),
        optionals: l.optionals.iter().map(|o| pricing::OptionalSel { price: o.price_minor }).collect(),
        bundle_components: vec![],
    }
}

/// Price the cart at `tax_rate` via the pricing engine, applying the selected
/// discount before tax.
pub(crate) fn totals(store: &Store, tax_rate: f64) -> CoreResult<CartTotals> {
    let lines = load(store)?;
    let item_count = lines.iter().map(|l| l.qty).sum();
    let (discount_kind, discount_value) = discount(store)?;
    let priced = pricing::price_cart(PriceCartInput {
        lines: lines.iter().map(priced).collect(),
        discount_kind,
        discount_value,
        tax_rate,
        amount_tendered: None,
        cash_tip: 0,
    });
    Ok(CartTotals {
        item_count,
        subtotal_minor: priced.subtotal_minor,
        discount_minor: priced.discount_minor,
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

    fn addon(id: &str, kind: &str, price: i64) -> menu::AddonItemView {
        menu::AddonItemView {
            id: id.into(),
            name: id.into(),
            addon_type: kind.into(),
            default_price_minor: price,
            is_active: true,
        }
    }

    fn item() -> menu::MenuItemView {
        menu::MenuItemView {
            id: "latte".into(),
            name: "Latte".into(),
            description: None,
            category_id: None,
            base_price_minor: 5000,
            image_url: None,
            is_active: true,
            default_milk_addon_id: Some("oat".into()), // base milk = oat @1500
            allowed_addon_ids: vec![],
            sizes: vec![menu::ItemSizeView { id: "lg".into(), label: "Large".into(), price_minor: 6000, is_active: true }],
            addon_slots: vec![],
            optional_fields: vec![menu::OptionalFieldView { id: "van".into(), name: "Vanilla".into(), price_minor: 300, is_active: true }],
            recipes: vec![],
        }
    }

    fn catalog() -> Vec<menu::AddonItemView> {
        vec![
            addon("oat", "milk_type", 1500),    // the default-milk base
            addon("almond", "milk_type", 2000), // swap → delta 500
            addon("whole", "milk_type", 0),     // downgrade → 0
            addon("shot", "extra", 800),        // additive → full
        ]
    }

    #[test]
    fn add_merges_and_appends() {
        let s = store();
        assert!(lines(&s).unwrap().is_empty());
        add(&s, "a", "Latte", 5000).unwrap();
        let v = add(&s, "a", "Latte", 5000).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].qty, 2);
        assert_eq!(v[0].line_total_minor, 10_000);
        assert_eq!(v[0].key, "a"); // option-less → key is item_id
        let v = add(&s, "b", "Tea", 3000).unwrap();
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn set_qty_and_remove_by_key() {
        let s = store();
        add(&s, "a", "Latte", 5000).unwrap();
        let v = set_qty(&s, "a", 4).unwrap();
        assert_eq!(v[0].qty, 4);
        assert!(set_qty(&s, "a", 0).unwrap().is_empty());
        add(&s, "a", "Latte", 5000).unwrap();
        assert!(remove(&s, "a").unwrap().is_empty());
    }

    #[test]
    fn resolve_line_prices_size_swap_additive_and_optionals() {
        // Large(6000) + almond(milk swap: 2000-1500=500) + 2×shot(additive 800)
        // + vanilla optional(300), qty 2.
        let line = resolve_line(
            &item(),
            &catalog(),
            Some("Large".into()),
            &[AddonSelection { addon_item_id: "almond".into(), qty: 1 },
              AddonSelection { addon_item_id: "shot".into(), qty: 2 }],
            &["van".into()],
            2,
            None,
        );
        assert_eq!(line.unit_price_minor, 6000);
        assert_eq!(line.addons.len(), 2);
        let almond = line.addons.iter().find(|a| a.addon_item_id == "almond").unwrap();
        assert_eq!(almond.price_modifier_minor, 500); // swap delta over oat base
        let shot = line.addons.iter().find(|a| a.addon_item_id == "shot").unwrap();
        assert_eq!(shot.price_modifier_minor, 800); // additive: full
        assert_eq!(shot.qty, 2);
        assert_eq!(line.optionals[0].price_minor, 300);
        // line unit = 6000 + (500*1 + 800*2) + 300 = 8400 ; ×2 = 16800
        assert_eq!(line_total(&line), 16_800);
    }

    #[test]
    fn milk_downgrade_is_free_not_negative() {
        let line = resolve_line(&item(), &catalog(), None,
            &[AddonSelection { addon_item_id: "whole".into(), qty: 1 }], &[], 1, None);
        // whole(0) - oat base(1500) = -1500 → clamped to 0.
        assert_eq!(line.addons[0].price_modifier_minor, 0);
        assert_eq!(line.unit_price_minor, 5000); // no size → base
    }

    #[test]
    fn no_milk_base_treats_swap_as_full() {
        let mut it = item();
        it.default_milk_addon_id = None; // no base → milk swap charges full
        let line = resolve_line(&it, &catalog(), None,
            &[AddonSelection { addon_item_id: "almond".into(), qty: 1 }], &[], 1, None);
        assert_eq!(line.addons[0].price_modifier_minor, 2000);
    }

    #[test]
    fn configured_lines_merge_only_when_identical() {
        let s = store();
        let mk = |milk: &str| resolve_line(&item(), &catalog(), Some("Large".into()),
            &[AddonSelection { addon_item_id: milk.into(), qty: 1 }], &[], 1, None);
        add_resolved(&s, mk("almond")).unwrap();
        let v = add_resolved(&s, mk("almond")).unwrap(); // same config → merge
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].qty, 2);
        let v = add_resolved(&s, mk("whole")).unwrap(); // different milk → new line
        assert_eq!(v.len(), 2);
        // qty + remove by the configured key.
        let key = v[0].key.clone();
        assert!(key.contains("latte|Large"));
        let v = set_qty(&s, &key, 5).unwrap();
        assert_eq!(v.iter().find(|l| l.key == key).unwrap().qty, 5);
    }

    #[test]
    fn totals_include_addon_and_optional_money() {
        let s = store();
        // base 5000 + almond(500) + vanilla(300) = 5800, qty 2 → 11600 subtotal.
        add_resolved(&s, resolve_line(&item(), &catalog(), None,
            &[AddonSelection { addon_item_id: "almond".into(), qty: 1 }], &["van".into()], 2, None)).unwrap();
        let t = totals(&s, 0.14).unwrap();
        assert_eq!(t.item_count, 2);
        assert_eq!(t.subtotal_minor, 11_600);
        assert_eq!(t.tax_minor, 1624); // round(11600 * 0.14)
        assert_eq!(t.total_minor, 13_224);
    }

    #[test]
    fn item_addons_resolve_charged_prices_for_display() {
        let v = item_addons(&item(), &catalog());
        let p = |id: &str| v.iter().find(|a| a.addon_item_id == id).unwrap().charged_price_minor;
        assert_eq!(p("almond"), 500); // milk swap delta over oat base
        assert_eq!(p("whole"), 0); // downgrade clamped
        assert_eq!(p("oat"), 0); // the default milk itself is free
        assert_eq!(p("shot"), 800); // additive full
    }

    #[test]
    fn empty_cart_totals_are_zero() {
        let s = store();
        let t = totals(&s, 0.14).unwrap();
        assert_eq!(t, CartTotals { item_count: 0, subtotal_minor: 0, discount_minor: 0, tax_minor: 0, total_minor: 0 });
    }

    fn seed_discounts(s: &Store) {
        s.kv_put(menu::K_DISCOUNTS, r#"[
          {"created_at":"2026-06-19T10:00:00Z","updated_at":"2026-06-19T10:00:00Z","dtype":"percentage","id":"00000000-0000-0000-0000-0000000000d1","is_active":true,"name":"10% off","name_translations":{},"org_id":"00000000-0000-0000-0000-0000000000ff","value":10},
          {"created_at":"2026-06-19T10:00:00Z","updated_at":"2026-06-19T10:00:00Z","dtype":"fixed","id":"00000000-0000-0000-0000-0000000000d2","is_active":true,"name":"250 off","name_translations":{},"org_id":"00000000-0000-0000-0000-0000000000ff","value":250}
        ]"#).unwrap();
    }

    #[test]
    fn percentage_discount_applies_before_tax() {
        let s = store();
        seed_discounts(&s);
        add(&s, "a", "Latte", 1000).unwrap();
        set_discount(&s, "00000000-0000-0000-0000-0000000000d1").unwrap();
        let t = totals(&s, 0.14).unwrap();
        assert_eq!(t.subtotal_minor, 1000);
        assert_eq!(t.discount_minor, 100); // 10%
        assert_eq!(t.tax_minor, 126); // round((1000-100) * 0.14)
        assert_eq!(t.total_minor, 1026);
    }

    #[test]
    fn fixed_discount_taxes_the_discounted_base() {
        let s = store();
        seed_discounts(&s);
        add(&s, "a", "Latte", 1000).unwrap();
        set_discount(&s, "00000000-0000-0000-0000-0000000000d2").unwrap();
        let t = totals(&s, 0.14).unwrap();
        assert_eq!(t.discount_minor, 250);
        assert_eq!(t.tax_minor, 105); // round(750 * 0.14)
        assert_eq!(t.total_minor, 855);
    }

    #[test]
    fn unknown_or_cleared_discount_is_none() {
        let s = store();
        seed_discounts(&s);
        add(&s, "a", "Latte", 1000).unwrap();
        set_discount(&s, "not-a-real-id").unwrap(); // not in the catalog → ignored
        assert_eq!(totals(&s, 0.0).unwrap().discount_minor, 0);
        set_discount(&s, "00000000-0000-0000-0000-0000000000d1").unwrap();
        assert_eq!(totals(&s, 0.0).unwrap().discount_minor, 100);
        clear_discount(&s).unwrap();
        assert_eq!(totals(&s, 0.0).unwrap().discount_minor, 0);
    }

    #[test]
    fn clearing_the_cart_resets_the_discount() {
        let s = store();
        seed_discounts(&s);
        add(&s, "a", "Latte", 1000).unwrap();
        set_discount(&s, "00000000-0000-0000-0000-0000000000d1").unwrap();
        clear(&s).unwrap();
        assert!(discount_id(&s).unwrap().is_none());
    }
}
