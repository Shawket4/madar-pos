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
use madar_api::models;

use crate::error::CoreResult;
use crate::menu;
use crate::pricing::{self, DiscountKind, PriceCartInput};
use crate::store::Store;

/// kv key — the whole cart is one JSON array.
pub(crate) const K_CART: &str = "cart:lines";
/// kv key — the selected discount id (empty = none).
pub(crate) const K_DISCOUNT: &str = "cart:discount";
/// kv key — parked/held carts (drafts) as a JSON array.
pub(crate) const K_DRAFTS: &str = "cart:drafts";

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

/// One configured component inside a bundle line. The component's base/size price
/// is NEVER charged (the bundle's fixed price covers it); only its addons +
/// optionals add money. Mirrors Flutter's `BundleComponentSnapshot`.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct StoredBundleComponent {
    item_id: String,
    name: String,
    qty: i64,
    size_label: Option<String>,
    addons: Vec<StoredAddon>,
    optionals: Vec<StoredOptional>,
}

/// The persisted cart line. New modifier fields default in (forward-compatible).
/// Opaque to callers — only `resolve_line`/`resolve_bundle_line`/`add_resolved`
/// construct/consume it. A bundle line sets `bundle_id` + `bundle_components`,
/// `unit_price_minor` = the fixed bundle price, and leaves its own
/// `addons`/`optionals` empty (component extras carry the up-charges).
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
    #[serde(default)]
    bundle_id: Option<String>,
    #[serde(default)]
    bundle_components: Vec<StoredBundleComponent>,
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

/// A configured component of a bundle cart line, for the bundle row breakdown.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct CartBundleComponentView {
    pub item_id: String,
    pub name: String,
    pub qty: i64,
    pub size_label: Option<String>,
    pub addons: Vec<CartAddonView>,
    pub optionals: Vec<CartOptionalView>,
}

/// A cart line as the host renders it (with the derived line total). When
/// `bundle_id` is set the line is a bundle: `name` is the bundle name,
/// `unit_price_minor` the fixed bundle price, and `bundle_components` the
/// configured items (the row renders their breakdown).
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
    pub bundle_id: Option<String>,
    pub bundle_components: Vec<CartBundleComponentView>,
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

fn addon_optional_extras(addons: &[StoredAddon], optionals: &[StoredOptional]) -> i64 {
    addons.iter().map(|a| a.price_modifier_minor * a.qty).sum::<i64>()
        + optionals.iter().map(|o| o.price_minor).sum::<i64>()
}

fn line_extras(l: &StoredLine) -> i64 {
    // A normal line's extras are its own addons/optionals; a bundle line's are
    // the sum across its components (the fixed base already covers the items).
    addon_optional_extras(&l.addons, &l.optionals)
        + l.bundle_components.iter().map(|c| addon_optional_extras(&c.addons, &c.optionals)).sum::<i64>()
}

fn line_total(l: &StoredLine) -> i64 {
    (l.unit_price_minor + line_extras(l)) * l.qty
}

/// The line key: a deterministic signature over the full selection. Option-less
/// lines key by `item_id` so the basic add/qty/remove path stays stable.
fn signature(l: &StoredLine) -> String {
    // A bundle keys by its id + each component's full selection, so identical
    // configurations merge (qty++) and differently-configured ones stay distinct.
    if let Some(bid) = &l.bundle_id {
        let comps: Vec<String> = l
            .bundle_components
            .iter()
            .map(|c| {
                let mut a: Vec<String> =
                    c.addons.iter().map(|x| format!("{}:{}", x.addon_item_id, x.qty)).collect();
                a.sort();
                let mut o: Vec<String> = c.optionals.iter().map(|x| x.optional_field_id.clone()).collect();
                o.sort();
                format!("{}@{}#{}#{}", c.item_id, c.size_label.as_deref().unwrap_or(""), a.join(","), o.join(","))
            })
            .collect();
        return format!("bundle:{}|{}", bid, comps.join(";"));
    }
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
            bundle_id: l.bundle_id.clone(),
            bundle_components: l
                .bundle_components
                .iter()
                .map(|c| CartBundleComponentView {
                    item_id: c.item_id.clone(),
                    name: c.name.clone(),
                    qty: c.qty,
                    size_label: c.size_label.clone(),
                    addons: c
                        .addons
                        .iter()
                        .map(|a| CartAddonView {
                            addon_item_id: a.addon_item_id.clone(),
                            name: a.name.clone(),
                            qty: a.qty,
                            price_modifier_minor: a.price_modifier_minor,
                        })
                        .collect(),
                    optionals: c
                        .optionals
                        .iter()
                        .map(|o| CartOptionalView {
                            optional_field_id: o.optional_field_id.clone(),
                            name: o.name.clone(),
                            price_minor: o.price_minor,
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect()
}

/// Charged addon price: SWAP families (milk_type, coffee_type) pay only the delta
/// over the item's default base for that family (clamped ≥0) — re-selecting the
/// default costs 0; everything else (additive) pays the full default. The backend
/// (component_resolve) charges a coffee swap as a delta too; the POS used to charge
/// the FULL coffee price, overstating the order total vs the recorded sale.
fn adjusted_addon_price(a: &menu::AddonItemView, milk_base: i64, coffee_base: i64) -> i64 {
    match a.addon_type.as_str() {
        "milk_type" => (a.default_price_minor - milk_base).max(0),
        "coffee_type" => (a.default_price_minor - coffee_base).max(0),
        _ => a.default_price_minor,
    }
}

/// The base price a coffee swap is charged ABOVE: find the item's recipe line in
/// the `coffee_bean` category, then the coffee_type addon whose embedded ingredient
/// matches that line's org-ingredient — its default price is the base. Recipe-driven
/// (mirrors the backend's component_resolve; no precomputed default-coffee id).
fn coffee_swap_base(item: &menu::MenuItemView, addon_catalog: &[menu::AddonItemView]) -> i64 {
    let Some(base_ing) = item
        .recipes
        .iter()
        .find(|r| r.category == "coffee_bean")
        .and_then(|r| r.org_ingredient_id.as_deref())
    else {
        return 0;
    };
    addon_catalog
        .iter()
        .filter(|a| a.addon_type == "coffee_type")
        .find(|a| a.ingredients.iter().any(|ing| ing.org_ingredient_id.as_deref() == Some(base_ing)))
        .map(|a| a.default_price_minor)
        .unwrap_or(0)
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
    StoredLine {
        item_id: item.id.clone(),
        name: item.name.clone(),
        unit_price_minor: unit_price,
        qty: qty.max(1),
        size_label,
        addons: resolve_addons(item, addon_catalog, addon_sels),
        optionals: resolve_optionals(item, optional_ids),
        notes,
        bundle_id: None,
        bundle_components: vec![],
    }
}

/// Resolve a selection's charged addon prices against `item` + the catalog
/// (swap-delta vs additive). Unknown ids are dropped. Shared by normal lines and
/// bundle components.
fn resolve_addons(
    item: &menu::MenuItemView,
    addon_catalog: &[menu::AddonItemView],
    addon_sels: &[AddonSelection],
) -> Vec<StoredAddon> {
    let milk_base = item
        .default_milk_addon_id
        .as_ref()
        .and_then(|id| addon_catalog.iter().find(|a| &a.id == id))
        .map(|a| a.default_price_minor)
        .unwrap_or(0);
    let coffee_base = coffee_swap_base(item, addon_catalog);
    addon_sels
        .iter()
        .filter_map(|sel| {
            let a = addon_catalog.iter().find(|x| x.id == sel.addon_item_id)?;
            Some(StoredAddon {
                addon_item_id: a.id.clone(),
                name: a.name.clone(),
                price_modifier_minor: adjusted_addon_price(a, milk_base, coffee_base),
                qty: sel.qty.max(1),
            })
        })
        .collect()
}

/// Resolve selected optional-field ids to stored optionals (price + name).
fn resolve_optionals(item: &menu::MenuItemView, optional_ids: &[String]) -> Vec<StoredOptional> {
    optional_ids
        .iter()
        .filter_map(|oid| {
            let o = item.optional_fields.iter().find(|f| &f.id == oid)?;
            Some(StoredOptional {
                optional_field_id: o.id.clone(),
                name: o.name.clone(),
                price_minor: o.price_minor,
            })
        })
        .collect()
}

/// A host-supplied configured component of a bundle (which item, its size, and
/// the chosen addons/optionals). The CORE resolves the charged extra prices.
#[derive(uniffi::Record, Clone, Debug)]
pub struct BundleComponentSelection {
    pub item_id: String,
    pub size_label: Option<String>,
    pub qty: i64,
    pub addons: Vec<AddonSelection>,
    pub optional_field_ids: Vec<String>,
}

/// Build a bundle cart line: the fixed bundle price as the unit price, plus each
/// component with its addon/optional up-charges resolved from the catalog. The
/// component base/size price is never charged (Flutter parity).
pub(crate) fn resolve_bundle_line(
    bundle: &menu::BundleView,
    items: &[menu::MenuItemView],
    addon_catalog: &[menu::AddonItemView],
    components: &[BundleComponentSelection],
    qty: i64,
) -> StoredLine {
    let bundle_components = components
        .iter()
        .filter_map(|sel| {
            let item = items.iter().find(|i| i.id == sel.item_id)?;
            Some(StoredBundleComponent {
                item_id: item.id.clone(),
                name: item.name.clone(),
                qty: sel.qty.max(1),
                size_label: sel.size_label.clone(),
                addons: resolve_addons(item, addon_catalog, &sel.addons),
                optionals: resolve_optionals(item, &sel.optional_field_ids),
            })
        })
        .collect();
    StoredLine {
        item_id: bundle.id.clone(),
        name: bundle.name.clone(),
        unit_price_minor: bundle.price_minor,
        qty: qty.max(1),
        size_label: None,
        addons: vec![],
        optionals: vec![],
        notes: None,
        bundle_id: Some(bundle.id.clone()),
        bundle_components,
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
    let coffee_base = coffee_swap_base(item, addon_catalog);
    addon_catalog
        .iter()
        .filter(|a| a.is_active)
        .map(|a| ItemAddonView {
            addon_item_id: a.id.clone(),
            name: a.name.clone(),
            addon_type: a.addon_type.clone(),
            charged_price_minor: adjusted_addon_price(a, milk_base, coffee_base),
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
            bundle_id: None,
            bundle_components: vec![],
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

/// Where a swiped-away line is stashed so `restore_last_removed` can undo it.
const K_LAST_REMOVED: &str = "cart:last_removed";

/// Remove a line entirely (by its key), stashing it so the host can offer an
/// "Undo" (see `restore_last_removed`).
pub(crate) fn remove(store: &Store, line_key: &str) -> CoreResult<Vec<CartLineView>> {
    let mut lines = load(store)?;
    let removed: Vec<StoredLine> = lines.iter().filter(|l| signature(l) == line_key).cloned().collect();
    lines.retain(|l| signature(l) != line_key);
    if !removed.is_empty() {
        store.kv_put(K_LAST_REMOVED, &serde_json::to_string(&removed)?)?;
    }
    save(store, &lines)?;
    Ok(view(&lines))
}

/// Re-insert the most recently `remove`d line(s) — the Undo for a swipe-delete.
/// Merges back into an identical line (same signature) if one exists, else
/// re-appends. Clears the stash; a no-op when nothing was stashed.
pub(crate) fn restore_last_removed(store: &Store) -> CoreResult<Vec<CartLineView>> {
    let stash: Vec<StoredLine> = match store.kv_get(K_LAST_REMOVED)? {
        Some(j) => serde_json::from_str(&j).unwrap_or_default(),
        None => Vec::new(),
    };
    let mut lines = load(store)?;
    for r in stash {
        match lines.iter_mut().find(|l| signature(l) == signature(&r)) {
            Some(l) => l.qty += r.qty,
            None => lines.push(r),
        }
    }
    store.kv_put(K_LAST_REMOVED, "[]")?; // consume the stash (no double-undo)
    save(store, &lines)?;
    Ok(view(&lines))
}

/// Empty the cart + its discount (e.g. after checkout or on sign-out).
pub(crate) fn clear(store: &Store) -> CoreResult<()> {
    clear_discount(store)?;
    store.kv_put(K_LAST_REMOVED, "[]")?; // a stale undo must not resurrect a sold line
    save(store, &[])
}

// ── drafts (parked / held carts) ─────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
struct StoredDraft {
    id: String,
    name: String,
    created_at: String,
    lines: Vec<StoredLine>,
}

/// A parked cart, summarized for the drafts list.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct DraftView {
    pub id: String,
    pub name: String,
    pub item_count: i64,
    pub total_minor: i64,
    pub created_at: String,
}

fn load_drafts(store: &Store) -> CoreResult<Vec<StoredDraft>> {
    match store.kv_get(K_DRAFTS)? {
        Some(json) => Ok(serde_json::from_str(&json).unwrap_or_default()),
        None => Ok(Vec::new()),
    }
}
fn save_drafts(store: &Store, drafts: &[StoredDraft]) -> CoreResult<()> {
    store.kv_put(K_DRAFTS, &serde_json::to_string(drafts)?)
}

/// Park the current cart as a named draft and empty the cart. `id`/`now` are
/// host-supplied (the core stays free of clock/uuid). Errors if the cart is empty.
pub(crate) fn hold(store: &Store, id: String, name: String, now: String) -> CoreResult<()> {
    let lines = load(store)?;
    if lines.is_empty() {
        return Err(crate::error::CoreError::Validation { field: "cart".into(), detail: "cart is empty".into() });
    }
    let mut drafts = load_drafts(store)?;
    drafts.push(StoredDraft { id, name, created_at: now, lines });
    save_drafts(store, &drafts)?;
    clear(store)
}

/// The parked drafts, newest first.
pub(crate) fn drafts(store: &Store) -> CoreResult<Vec<DraftView>> {
    let mut out: Vec<DraftView> = load_drafts(store)?
        .iter()
        .map(|d| DraftView {
            id: d.id.clone(),
            name: d.name.clone(),
            item_count: d.lines.iter().map(|l| l.qty).sum(),
            total_minor: d.lines.iter().map(line_total).sum(),
            created_at: d.created_at.clone(),
        })
        .collect();
    out.reverse();
    Ok(out)
}

/// Restore a draft into the cart (replacing any current lines) and drop it from
/// the drafts list. Returns the new cart view.
pub(crate) fn restore_draft(store: &Store, id: &str) -> CoreResult<Vec<CartLineView>> {
    let mut drafts = load_drafts(store)?;
    let Some(pos) = drafts.iter().position(|d| d.id == id) else {
        return Ok(view(&load(store)?));
    };
    let draft = drafts.remove(pos);
    save_drafts(store, &drafts)?;
    clear_discount(store)?;
    save(store, &draft.lines)?;
    Ok(view(&draft.lines))
}

/// Discard a parked draft without restoring it.
pub(crate) fn discard_draft(store: &Store, id: &str) -> CoreResult<()> {
    let mut drafts = load_drafts(store)?;
    drafts.retain(|d| d.id != id);
    save_drafts(store, &drafts)
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
        is_bundle: l.bundle_id.is_some(),
        addons: l
            .addons
            .iter()
            .map(|a| pricing::AddonSel { price_modifier: a.price_modifier_minor, quantity: a.qty })
            .collect(),
        optionals: l.optionals.iter().map(|o| pricing::OptionalSel { price: o.price_minor }).collect(),
        bundle_components: l
            .bundle_components
            .iter()
            .map(|c| pricing::BundleComponentSel {
                addons: c
                    .addons
                    .iter()
                    .map(|a| pricing::AddonSel { price_modifier: a.price_modifier_minor, quantity: a.qty })
                    .collect(),
                optionals: c.optionals.iter().map(|o| pricing::OptionalSel { price: o.price_minor }).collect(),
            })
            .collect(),
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
            ingredients: vec![],
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
            optional_fields: vec![menu::OptionalFieldView { id: "van".into(), name: "Vanilla".into(), price_minor: 300, is_active: true, ingredient_name: None, ingredient_unit: None, quantity_used: None, org_ingredient_id: None }],
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

    fn coffee(id: &str, price: i64, ing: &str) -> menu::AddonItemView {
        menu::AddonItemView {
            id: id.into(), name: id.into(), addon_type: "coffee_type".into(),
            default_price_minor: price, is_active: true,
            ingredients: vec![menu::AddonIngredientView {
                ingredient_name: ing.into(), unit: "g".into(), quantity: 18.0, org_ingredient_id: Some(ing.into()),
            }],
        }
    }

    #[test]
    fn coffee_swap_charges_delta_over_recipe_base_not_full() {
        // The item's recipe uses the house bean; its coffee_type addon (1200) is the
        // DEFAULT. Swapping to a single-origin bean (1800) costs the delta 600 — not
        // the full 1800 (the POS bug) — and re-selecting the house bean costs 0.
        let mut it = item();
        it.recipes = vec![menu::RecipeLineView {
            ingredient_name: "House Bean".into(), quantity: 18.0, unit: "g".into(),
            size_label: None, category: "coffee_bean".into(), org_ingredient_id: Some("bean-house".into()),
        }];
        let catalog = vec![coffee("house", 1200, "bean-house"), coffee("single", 1800, "bean-single")];

        let line = resolve_line(&it, &catalog, None,
            &[AddonSelection { addon_item_id: "single".into(), qty: 1 }], &[], 1, None);
        assert_eq!(line.addons[0].price_modifier_minor, 600, "coffee swap = 1800 - 1200 base");

        let line = resolve_line(&it, &catalog, None,
            &[AddonSelection { addon_item_id: "house".into(), qty: 1 }], &[], 1, None);
        assert_eq!(line.addons[0].price_modifier_minor, 0, "re-selecting the default coffee is free");
    }

    #[test]
    fn coffee_swap_without_recipe_base_falls_back_to_full() {
        // No coffee_bean recipe line → no base derivable → full price (best-effort).
        let line = resolve_line(&item(), &[coffee("single", 1800, "bean-single")], None,
            &[AddonSelection { addon_item_id: "single".into(), qty: 1 }], &[], 1, None);
        assert_eq!(line.addons[0].price_modifier_minor, 1800);
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
    fn hold_then_restore_roundtrips_the_cart() {
        let s = store();
        add(&s, "latte", "Latte", 5000).unwrap();
        add(&s, "bun", "Bun", 2000).unwrap();
        hold(&s, "d1".into(), "Table 4".into(), "2026-06-21T10:00:00Z".into()).unwrap();
        // Held → cart empty, one draft summarizing the two lines.
        assert!(lines(&s).unwrap().is_empty());
        let ds = drafts(&s).unwrap();
        assert_eq!(ds.len(), 1);
        assert_eq!(ds[0].name, "Table 4");
        assert_eq!(ds[0].item_count, 2);
        assert_eq!(ds[0].total_minor, 7000);
        // Restore → cart back, draft gone.
        let restored = restore_draft(&s, "d1").unwrap();
        assert_eq!(restored.len(), 2);
        assert!(drafts(&s).unwrap().is_empty());
        // Holding an empty cart is rejected.
        clear(&s).unwrap();
        assert!(hold(&s, "d2".into(), "x".into(), "t".into()).is_err());
    }

    #[test]
    fn remove_then_restore_brings_the_line_back() {
        let s = store();
        add(&s, "latte", "Latte", 5000).unwrap();
        set_qty(&s, &lines(&s).unwrap()[0].key, 3).unwrap(); // a 3× line
        add(&s, "bun", "Bun", 2000).unwrap();
        let key = lines(&s).unwrap().iter().find(|l| l.name == "Latte").unwrap().key.clone();
        // Swipe-remove the latte → one line left.
        let after = remove(&s, &key).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].name, "Bun");
        // Undo → the 3× latte is back (qty preserved).
        let restored = restore_last_removed(&s).unwrap();
        assert_eq!(restored.len(), 2);
        let latte = restored.iter().find(|l| l.name == "Latte").unwrap();
        assert_eq!(latte.qty, 3);
        // The stash is consumed — a second undo is a no-op.
        let again = restore_last_removed(&s).unwrap();
        assert_eq!(again.iter().find(|l| l.name == "Latte").unwrap().qty, 3);
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

    fn bundle() -> menu::BundleView {
        menu::BundleView {
            id: "b1".into(),
            name: "Morning Combo".into(),
            description: None,
            price_minor: 10000,
            image_url: None,
            is_available: true,
            available_from_date: None,
            available_until_date: None,
            available_from_time: None,
            available_until_time: None,
            components: vec![],
        }
    }

    fn combo_component() -> BundleComponentSelection {
        // Latte, Large, + almond milk (milk_type 2000 − oat base 1500 = +500 swap
        // delta) + vanilla optional (+300). Component base/size price NOT charged.
        BundleComponentSelection {
            item_id: "latte".into(),
            size_label: Some("Large".into()),
            qty: 1,
            addons: vec![AddonSelection { addon_item_id: "almond".into(), qty: 1 }],
            optional_field_ids: vec!["van".into()],
        }
    }

    #[test]
    fn bundle_line_charges_fixed_price_plus_component_extras() {
        let s = store();
        let line = resolve_bundle_line(&bundle(), &[item()], &catalog(), &[combo_component()], 1);
        add_resolved(&s, line).unwrap();
        let lines = lines(&s).unwrap();
        assert_eq!(lines.len(), 1);
        let l = &lines[0];
        assert_eq!(l.bundle_id.as_deref(), Some("b1"));
        assert_eq!(l.unit_price_minor, 10000, "fixed bundle price");
        // (10000 base + 500 almond delta + 300 vanilla) × 1
        assert_eq!(l.line_total_minor, 10800);
        assert_eq!(l.bundle_components.len(), 1);
        assert_eq!(l.bundle_components[0].name, "Latte");
        assert_eq!(l.bundle_components[0].size_label.as_deref(), Some("Large"));
    }

    #[test]
    fn identical_bundle_configs_merge_distinct_ones_dont() {
        let s = store();
        let a = resolve_bundle_line(&bundle(), &[item()], &catalog(), &[combo_component()], 1);
        add_resolved(&s, a).unwrap();
        // Same config again → merges (qty 2, one line).
        let b = resolve_bundle_line(&bundle(), &[item()], &catalog(), &[combo_component()], 1);
        add_resolved(&s, b).unwrap();
        assert_eq!(lines(&s).unwrap().len(), 1);
        assert_eq!(lines(&s).unwrap()[0].qty, 2);
        // A different component config → a separate line.
        let mut plain = combo_component();
        plain.addons = vec![];
        plain.optional_field_ids = vec![];
        let c = resolve_bundle_line(&bundle(), &[item()], &catalog(), &[plain], 1);
        add_resolved(&s, c).unwrap();
        assert_eq!(lines(&s).unwrap().len(), 2);
    }

    // ── add / merge edge cases ────────────────────────────────────────────────

    #[test]
    fn add_to_empty_creates_single_line() {
        let s = store();
        let v = add(&s, "a", "Latte", 5000).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].qty, 1);
        assert_eq!(v[0].line_total_minor, 5000);
        assert_eq!(v[0].key, "a");
    }

    #[test]
    fn add_resolved_merges_on_matching_signature() {
        let s = store();
        // Two option-less lines for the same item id merge regardless of name/price
        // because the signature for an option-less line is just the item_id.
        add_resolved(&s, resolve_line(&item(), &catalog(), None, &[], &[], 1, None)).unwrap();
        let v = add_resolved(&s, resolve_line(&item(), &catalog(), None, &[], &[], 1, None)).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].qty, 2);
        assert_eq!(v[0].key, "latte"); // option-less → key is item_id
    }

    // ── set_qty boundaries ────────────────────────────────────────────────────

    #[test]
    fn set_qty_to_one_keeps_line() {
        let s = store();
        add(&s, "a", "Latte", 5000).unwrap();
        let v = set_qty(&s, "a", 1).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].qty, 1);
    }

    #[test]
    fn set_qty_negative_removes_line() {
        let s = store();
        add(&s, "a", "Latte", 5000).unwrap();
        assert!(set_qty(&s, "a", -5).unwrap().is_empty());
    }

    #[test]
    fn set_qty_on_missing_key_is_noop() {
        let s = store();
        add(&s, "a", "Latte", 5000).unwrap();
        // A positive qty on a non-existent key changes nothing.
        let v = set_qty(&s, "nope", 9).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].qty, 1);
    }

    #[test]
    fn set_qty_zero_on_missing_key_leaves_others() {
        let s = store();
        add(&s, "a", "Latte", 5000).unwrap();
        add(&s, "b", "Tea", 3000).unwrap();
        // qty<=0 only retains lines whose signature differs from the key; an
        // unknown key removes nothing.
        let v = set_qty(&s, "nope", 0).unwrap();
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn set_qty_on_empty_cart_is_noop() {
        let s = store();
        assert!(set_qty(&s, "a", 3).unwrap().is_empty());
        assert!(set_qty(&s, "a", 0).unwrap().is_empty());
    }

    // ── remove edge cases ─────────────────────────────────────────────────────

    #[test]
    fn remove_missing_key_does_not_stash() {
        let s = store();
        add(&s, "a", "Latte", 5000).unwrap();
        let v = remove(&s, "nope").unwrap();
        assert_eq!(v.len(), 1); // nothing removed
        // Nothing was stashed, so undo is a no-op (cart unchanged).
        let after = restore_last_removed(&s).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].qty, 1);
    }

    #[test]
    fn remove_from_empty_cart_is_noop() {
        let s = store();
        assert!(remove(&s, "a").unwrap().is_empty());
    }

    // ── undo (restore_last_removed) edge cases ───────────────────────────────

    #[test]
    fn restore_with_no_stash_is_noop() {
        let s = store();
        add(&s, "a", "Latte", 5000).unwrap();
        let v = restore_last_removed(&s).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].qty, 1);
    }

    #[test]
    fn restore_merges_back_into_matching_line() {
        let s = store();
        add(&s, "a", "Latte", 5000).unwrap();
        set_qty(&s, "a", 2).unwrap(); // 2× in cart
        // Remove it (stash = 2×), then re-add one fresh, then undo: the stashed 2
        // merges into the existing 1× for qty 3 in a single line.
        remove(&s, "a").unwrap();
        add(&s, "a", "Latte", 5000).unwrap();
        let restored = restore_last_removed(&s).unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].qty, 3);
    }

    #[test]
    fn clear_drops_the_undo_stash() {
        let s = store();
        add(&s, "a", "Latte", 5000).unwrap();
        remove(&s, "a").unwrap(); // stash now holds the latte
        clear(&s).unwrap(); // a stale undo must not resurrect a sold line
        let after = restore_last_removed(&s).unwrap();
        assert!(after.is_empty());
    }

    // ── signature determinism ─────────────────────────────────────────────────

    #[test]
    fn signature_is_order_independent_for_addons_and_optionals() {
        // Same selection, different option ORDER → identical signature → merge.
        let mut it = item();
        it.optional_fields.push(menu::OptionalFieldView {
            id: "cin".into(), name: "Cinnamon".into(), price_minor: 100, is_active: true,
            ingredient_name: None, ingredient_unit: None, quantity_used: None, org_ingredient_id: None,
        });
        let a = resolve_line(&it, &catalog(), Some("Large".into()),
            &[AddonSelection { addon_item_id: "almond".into(), qty: 1 },
              AddonSelection { addon_item_id: "shot".into(), qty: 1 }],
            &["van".into(), "cin".into()], 1, None);
        let b = resolve_line(&it, &catalog(), Some("Large".into()),
            &[AddonSelection { addon_item_id: "shot".into(), qty: 1 },
              AddonSelection { addon_item_id: "almond".into(), qty: 1 }],
            &["cin".into(), "van".into()], 1, None);
        let s = store();
        add_resolved(&s, a).unwrap();
        let v = add_resolved(&s, b).unwrap();
        assert_eq!(v.len(), 1, "reordered options must merge");
        assert_eq!(v[0].qty, 2);
    }

    #[test]
    fn signature_distinguishes_notes() {
        // Same item, different notes → distinct lines.
        let s = store();
        add_resolved(&s, resolve_line(&item(), &catalog(), None, &[], &[], 1, Some("no sugar".into()))).unwrap();
        add_resolved(&s, resolve_line(&item(), &catalog(), None, &[], &[], 1, Some("extra hot".into()))).unwrap();
        assert_eq!(lines(&s).unwrap().len(), 2);
    }

    #[test]
    fn signature_distinguishes_addon_qty() {
        // Same addon, different qty → distinct lines (qty is in the signature).
        let s = store();
        let mk = |q: i64| resolve_line(&item(), &catalog(), None,
            &[AddonSelection { addon_item_id: "shot".into(), qty: q }], &[], 1, None);
        add_resolved(&s, mk(1)).unwrap();
        let v = add_resolved(&s, mk(2)).unwrap();
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn signature_distinguishes_size() {
        let s = store();
        add_resolved(&s, resolve_line(&item(), &catalog(), Some("Large".into()), &[], &[], 1, None)).unwrap();
        // No size → falls back to the option-less item_id signature, distinct from
        // the sized line.
        add_resolved(&s, resolve_line(&item(), &catalog(), None, &[], &[], 1, None)).unwrap();
        assert_eq!(lines(&s).unwrap().len(), 2);
    }

    // ── resolve_line boundaries / malformed input ─────────────────────────────

    #[test]
    fn resolve_line_clamps_qty_floor_to_one() {
        let line = resolve_line(&item(), &catalog(), None, &[], &[], 0, None);
        assert_eq!(line.qty, 1);
        let line = resolve_line(&item(), &catalog(), None, &[], &[], -3, None);
        assert_eq!(line.qty, 1);
    }

    #[test]
    fn resolve_line_clamps_addon_qty_floor_to_one() {
        let line = resolve_line(&item(), &catalog(), None,
            &[AddonSelection { addon_item_id: "shot".into(), qty: 0 }], &[], 1, None);
        assert_eq!(line.addons.len(), 1);
        assert_eq!(line.addons[0].qty, 1);
    }

    #[test]
    fn resolve_line_unknown_size_falls_back_to_base() {
        let line = resolve_line(&item(), &catalog(), Some("Gigantic".into()), &[], &[], 1, None);
        assert_eq!(line.unit_price_minor, 5000); // base, unknown size label ignored
        // The bogus size label is still recorded (and so part of the signature).
        assert_eq!(line.size_label.as_deref(), Some("Gigantic"));
    }

    #[test]
    fn resolve_line_drops_unknown_addon_and_optional_ids() {
        let line = resolve_line(&item(), &catalog(), None,
            &[AddonSelection { addon_item_id: "ghost".into(), qty: 1 },
              AddonSelection { addon_item_id: "shot".into(), qty: 1 }],
            &["nope".into(), "van".into()], 1, None);
        assert_eq!(line.addons.len(), 1); // only "shot" survives
        assert_eq!(line.addons[0].addon_item_id, "shot");
        assert_eq!(line.optionals.len(), 1); // only "van" survives
        assert_eq!(line.optionals[0].optional_field_id, "van");
    }

    #[test]
    fn resolve_line_with_no_options_keys_by_item_id() {
        let line = resolve_line(&item(), &catalog(), None, &[], &[], 1, None);
        assert_eq!(signature(&line), "latte");
    }

    // ── item_addons filtering ─────────────────────────────────────────────────

    #[test]
    fn item_addons_drops_inactive_entries() {
        let mut cat = catalog();
        cat.push(menu::AddonItemView {
            id: "retired".into(), name: "Retired".into(), addon_type: "extra".into(),
            default_price_minor: 999, is_active: false, ingredients: vec![],
        });
        let v = item_addons(&item(), &cat);
        assert!(v.iter().all(|a| a.addon_item_id != "retired"));
    }

    // ── bundle pricing / component resolution ─────────────────────────────────

    #[test]
    fn resolve_bundle_line_uses_fixed_price_and_clamps_qty() {
        let line = resolve_bundle_line(&bundle(), &[item()], &catalog(), &[combo_component()], 0);
        assert_eq!(line.unit_price_minor, 10000); // fixed bundle price
        assert_eq!(line.qty, 1); // qty clamped up from 0
        assert!(line.addons.is_empty()); // bundle's own addons stay empty
        assert!(line.optionals.is_empty());
        assert_eq!(line.bundle_id.as_deref(), Some("b1"));
    }

    #[test]
    fn resolve_bundle_line_drops_components_with_unknown_item() {
        let mut ghost = combo_component();
        ghost.item_id = "not-in-catalog".into();
        // One good + one ghost component → only the resolvable one survives.
        let line = resolve_bundle_line(&bundle(), &[item()], &catalog(),
            &[combo_component(), ghost], 1);
        assert_eq!(line.bundle_components.len(), 1);
        assert_eq!(line.bundle_components[0].item_id, "latte");
    }

    #[test]
    fn resolve_bundle_line_with_no_components_charges_only_base() {
        let line = resolve_bundle_line(&bundle(), &[item()], &catalog(), &[], 2);
        assert!(line.bundle_components.is_empty());
        // (10000 base + 0 extras) × 2 = 20000.
        assert_eq!(line_total(&line), 20000);
    }

    #[test]
    fn bundle_component_qty_does_not_scale_extras() {
        // A component qty of 5 must NOT multiply the addon/optional up-charge; the
        // bundle's own line qty is the only multiplier (Flutter parity — extras are
        // per-bundle, base covers the component count).
        let mut comp = combo_component();
        comp.qty = 5;
        let line = resolve_bundle_line(&bundle(), &[item()], &catalog(), &[comp], 1);
        assert_eq!(line.bundle_components[0].qty, 5);
        // 10000 base + 500 almond delta + 300 vanilla = 10800 (extras counted once).
        assert_eq!(line_total(&line), 10800);
    }

    #[test]
    fn bundle_totals_flow_through_pricing_engine() {
        let s = store();
        add_resolved(&s, resolve_bundle_line(&bundle(), &[item()], &catalog(), &[combo_component()], 2)).unwrap();
        let t = totals(&s, 0.0).unwrap();
        assert_eq!(t.item_count, 2);
        // (10000 + 500 + 300) × 2 = 21600.
        assert_eq!(t.subtotal_minor, 21600);
    }

    // ── drafts ────────────────────────────────────────────────────────────────

    #[test]
    fn hold_empty_cart_errors_with_validation() {
        let s = store();
        let err = hold(&s, "d1".into(), "x".into(), "now".into()).unwrap_err();
        match err {
            crate::error::CoreError::Validation { field, detail } => {
                assert_eq!(field, "cart");
                assert_eq!(detail, "cart is empty");
            }
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn drafts_are_listed_newest_first() {
        let s = store();
        add(&s, "a", "Latte", 5000).unwrap();
        hold(&s, "d1".into(), "First".into(), "2026-06-21T10:00:00Z".into()).unwrap();
        add(&s, "b", "Tea", 3000).unwrap();
        hold(&s, "d2".into(), "Second".into(), "2026-06-21T11:00:00Z".into()).unwrap();
        let ds = drafts(&s).unwrap();
        assert_eq!(ds.len(), 2);
        assert_eq!(ds[0].name, "Second"); // newest first (reversed)
        assert_eq!(ds[1].name, "First");
    }

    #[test]
    fn drafts_empty_when_none_held() {
        let s = store();
        assert!(drafts(&s).unwrap().is_empty());
    }

    #[test]
    fn restore_draft_replaces_current_cart_lines() {
        let s = store();
        add(&s, "a", "Latte", 5000).unwrap();
        hold(&s, "d1".into(), "Held".into(), "t".into()).unwrap();
        // Build a new, different cart, then restore — the draft REPLACES it.
        add(&s, "b", "Tea", 3000).unwrap();
        let restored = restore_draft(&s, "d1").unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].name, "Latte"); // the held line, not the Tea
        assert!(drafts(&s).unwrap().is_empty()); // draft consumed
    }

    #[test]
    fn restore_draft_clears_any_selected_discount() {
        let s = store();
        seed_discounts(&s);
        add(&s, "a", "Latte", 1000).unwrap();
        hold(&s, "d1".into(), "Held".into(), "t".into()).unwrap();
        // Pick a discount on the (now empty) cart, then restore the draft.
        set_discount(&s, "00000000-0000-0000-0000-0000000000d1").unwrap();
        restore_draft(&s, "d1").unwrap();
        assert!(discount_id(&s).unwrap().is_none());
    }

    #[test]
    fn restore_unknown_draft_is_noop_returning_current_cart() {
        let s = store();
        add(&s, "a", "Latte", 5000).unwrap();
        let v = restore_draft(&s, "ghost").unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].name, "Latte"); // current cart unchanged
    }

    #[test]
    fn discard_draft_removes_only_the_target() {
        let s = store();
        add(&s, "a", "Latte", 5000).unwrap();
        hold(&s, "d1".into(), "One".into(), "t".into()).unwrap();
        add(&s, "b", "Tea", 3000).unwrap();
        hold(&s, "d2".into(), "Two".into(), "t".into()).unwrap();
        discard_draft(&s, "d1").unwrap();
        let ds = drafts(&s).unwrap();
        assert_eq!(ds.len(), 1);
        assert_eq!(ds[0].id, "d2");
    }

    #[test]
    fn discard_unknown_draft_is_noop() {
        let s = store();
        add(&s, "a", "Latte", 5000).unwrap();
        hold(&s, "d1".into(), "One".into(), "t".into()).unwrap();
        discard_draft(&s, "ghost").unwrap();
        assert_eq!(drafts(&s).unwrap().len(), 1);
    }

    #[test]
    fn hold_clears_the_cart_and_its_discount() {
        let s = store();
        seed_discounts(&s);
        add(&s, "a", "Latte", 1000).unwrap();
        set_discount(&s, "00000000-0000-0000-0000-0000000000d1").unwrap();
        hold(&s, "d1".into(), "Held".into(), "t".into()).unwrap();
        assert!(lines(&s).unwrap().is_empty());
        assert!(discount_id(&s).unwrap().is_none()); // hold → clear → clear_discount
    }

    #[test]
    fn draft_summary_counts_quantities_and_totals() {
        let s = store();
        add(&s, "a", "Latte", 5000).unwrap();
        set_qty(&s, "a", 3).unwrap();
        add(&s, "b", "Tea", 2000).unwrap();
        hold(&s, "d1".into(), "Table".into(), "t".into()).unwrap();
        let ds = drafts(&s).unwrap();
        assert_eq!(ds[0].item_count, 4); // 3 + 1
        assert_eq!(ds[0].total_minor, 17000); // 5000*3 + 2000
    }

    // ── discount set / clear / resolve ────────────────────────────────────────

    #[test]
    fn discount_id_none_when_unset() {
        let s = store();
        assert!(discount_id(&s).unwrap().is_none());
    }

    #[test]
    fn set_then_clear_discount_id() {
        let s = store();
        set_discount(&s, "abc").unwrap();
        assert_eq!(discount_id(&s).unwrap().as_deref(), Some("abc"));
        clear_discount(&s).unwrap();
        assert!(discount_id(&s).unwrap().is_none());
    }

    #[test]
    fn discount_id_treats_literal_null_as_none() {
        let s = store();
        set_discount(&s, "null").unwrap(); // the string "null" is filtered out
        assert!(discount_id(&s).unwrap().is_none());
    }

    #[test]
    fn discount_resolves_kind_and_value_from_catalog() {
        let s = store();
        seed_discounts(&s);
        set_discount(&s, "00000000-0000-0000-0000-0000000000d1").unwrap();
        let (kind, value) = discount(&s).unwrap();
        assert_eq!(kind, DiscountKind::Percentage);
        assert_eq!(value, 10);
        set_discount(&s, "00000000-0000-0000-0000-0000000000d2").unwrap();
        let (kind, value) = discount(&s).unwrap();
        assert_eq!(kind, DiscountKind::Fixed);
        assert_eq!(value, 250);
    }

    #[test]
    fn discount_none_when_nothing_selected() {
        let s = store();
        seed_discounts(&s);
        let (kind, value) = discount(&s).unwrap();
        assert_eq!(kind, DiscountKind::None);
        assert_eq!(value, 0);
    }

    #[test]
    fn discount_none_when_catalog_missing() {
        // A selected id but no discounts catalog seeded → resolves to none.
        let s = store();
        set_discount(&s, "00000000-0000-0000-0000-0000000000d1").unwrap();
        let (kind, value) = discount(&s).unwrap();
        assert_eq!(kind, DiscountKind::None);
        assert_eq!(value, 0);
    }

    // ── clear ─────────────────────────────────────────────────────────────────

    #[test]
    fn clear_empties_lines_and_keeps_drafts() {
        let s = store();
        add(&s, "a", "Latte", 5000).unwrap();
        hold(&s, "d1".into(), "Held".into(), "t".into()).unwrap(); // parks + clears
        add(&s, "b", "Tea", 3000).unwrap();
        clear(&s).unwrap();
        assert!(lines(&s).unwrap().is_empty());
        // clear() empties the live cart but does NOT touch the drafts stash.
        assert_eq!(drafts(&s).unwrap().len(), 1);
    }
}
