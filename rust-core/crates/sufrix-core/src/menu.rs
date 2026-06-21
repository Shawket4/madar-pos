//! Menu / catalog reads (PLAN §R9). The POS consumes the server's already
//! branch-effective catalog and mirrors the canonical JSON into `kv`; the UI
//! reads always succeed offline. This module owns the read projection: it parses
//! the mirrored wire models and hands the host curated view DTOs with
//!   - money as `i64` minor-units (the wire is already integer minor-units),
//!   - `*_translations` pre-resolved to the device locale (fallback locale→en→base),
//!   - soft-deletes dropped.
//!
//! It does NOT re-implement the §3 branch-override merge — the server's
//! `?branch_id=` snapshot is already merged (R9). The fetch/orchestration lives
//! in `lib.rs`; this module is pure (store + locale in, view DTOs out) so it's
//! unit-testable without a network.

use serde::Deserialize;
use serde_json::Value;
use sufrix_api::models;

use crate::error::CoreResult;
use crate::store::Store;

// kv keys — one canonical JSON array per catalog stream.
pub(crate) const K_MENU_ITEMS: &str = "catalog:menu_items"; // Vec<MenuItemFull>
pub(crate) const K_CATEGORIES: &str = "catalog:categories"; // Vec<Category>
pub(crate) const K_ADDONS: &str = "catalog:addons"; // Vec<AddonItem>
pub(crate) const K_BUNDLES: &str = "catalog:bundles"; // Vec<Bundle>
pub(crate) const K_PAYMENT_METHODS: &str = "catalog:payment_methods"; // Vec<OrgPaymentMethod>
pub(crate) const K_DISCOUNTS: &str = "catalog:discounts"; // Vec<Discount>

// ── view DTOs (host-facing) ─────────────────────────────────────────────────

#[derive(uniffi::Record, Clone, Debug)]
pub struct MenuItemView {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub category_id: Option<String>,
    pub base_price_minor: i64,
    pub image_url: Option<String>,
    pub is_active: bool,
    /// The item's default-milk addon (swap families charge only the delta over it).
    pub default_milk_addon_id: Option<String>,
    /// Per-item addon allowlist (ids). Non-empty ⇒ the sheet shows only these by
    /// default, with a "show all" escape hatch (mirrors the dashboard). Empty =
    /// no restriction (show the type's full set).
    pub allowed_addon_ids: Vec<String>,
    pub sizes: Vec<ItemSizeView>,
    pub addon_slots: Vec<AddonSlotView>,
    pub optional_fields: Vec<OptionalFieldView>,
    /// The item's recipe lines (per size) — shown in the customization sheet.
    pub recipes: Vec<RecipeLineView>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct ItemSizeView {
    pub id: String,
    pub label: String,
    /// Absolute price for this size (NOT a delta) — R9.
    pub price_minor: i64,
    pub is_active: bool,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct AddonSlotView {
    pub id: String,
    pub label: Option<String>,
    pub addon_type: String,
    pub is_required: bool,
    pub min_selections: i32,
    /// `None` ⇒ multi-select with no cap (R9).
    pub max_selections: Option<i32>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct OptionalFieldView {
    pub id: String,
    pub name: String,
    pub price_minor: i64,
    pub is_active: bool,
    /// Optional ingredient deduction: an optional that maps to stock carries a
    /// full `(name, unit, quantity)` triplet; cosmetic fields leave these `None`
    /// and contribute no recipe line. Mirrors Flutter's `OptionalField`.
    pub ingredient_name: Option<String>,
    pub ingredient_unit: Option<String>,
    pub quantity_used: Option<f64>,
    pub org_ingredient_id: Option<String>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct RecipeLineView {
    pub ingredient_name: String,
    /// Quantity used (major units of the ingredient's unit, e.g. 18.0 g).
    pub quantity: f64,
    pub unit: String,
    /// `None` = applies to all sizes; otherwise the size this line is for.
    pub size_label: Option<String>,
    /// Ingredient category (e.g. `milk`, `coffee_bean`) — the swap engine matches
    /// a milk/coffee addon against the base line of the same category.
    pub category: String,
    /// The org-ingredient identity — used to tell a real swap from re-selecting
    /// the default (same id ⇒ no swap). May be absent on older rows.
    pub org_ingredient_id: Option<String>,
}

/// One ingredient embedded in an addon item (`/addon-items` wire). Drives the
/// recipe preview: a milk/coffee addon's first ingredient replaces the base
/// line; other addons add their ingredients (scaled by qty).
#[derive(uniffi::Record, Clone, Debug)]
pub struct AddonIngredientView {
    pub ingredient_name: String,
    pub unit: String,
    pub quantity: f64,
    pub org_ingredient_id: Option<String>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct CategoryView {
    pub id: String,
    pub name: String,
    pub image_url: Option<String>,
    pub is_active: bool,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct AddonItemView {
    pub id: String,
    pub name: String,
    pub addon_type: String,
    pub default_price_minor: i64,
    pub is_active: bool,
    /// Embedded ingredient rows (recipe preview input). Empty when the addon has
    /// no stock impact (e.g. a flavour shot) or the wire omitted them.
    pub ingredients: Vec<AddonIngredientView>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct BundleView {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub price_minor: i64,
    pub image_url: Option<String>,
    /// `status == active`. The date/time availability window (below) is gated in
    /// the branch timezone by the cart/order context, not in this static read.
    pub is_available: bool,
    pub available_from_date: Option<String>,
    pub available_until_date: Option<String>,
    pub available_from_time: Option<String>,
    pub available_until_time: Option<String>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct PaymentMethodView {
    pub id: String,
    pub name: String,
    pub is_cash: bool,
    pub icon: String,
    pub color: String,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct DiscountView {
    pub id: String,
    pub name: String,
    /// Open string: `percentage` | `fixed` | … — host interprets `value`.
    pub dtype: String,
    /// Percent points for `percentage`, minor-units for `fixed`.
    pub value: i64,
    pub is_active: bool,
}

// ── projections (kv → views) ────────────────────────────────────────────────

// Local, deserialization-TOLERANT shapes for the `?full=true` menu-item wire.
//
// We deliberately DO NOT reuse `models::MenuItemFull` here: that generated struct
// embeds `recipes: Vec<MenuItemRecipe>` and `optional_fields[].quantity_used`,
// both typed `f64` by the generator — but the backend serializes those Postgres
// `numeric` columns via `BigDecimal`, i.e. as JSON *strings* ("0.500"). serde
// then fails the WHOLE `Vec<MenuItemFull>` parse, which blanked the menu (the
// host swallows the error to an empty list). These local structs capture only
// the fields the POS projection actually needs and omit every decimal field, so
// the wire's string-vs-number encoding can't break the read. Unknown JSON fields
// are ignored by serde, so this stays forward-compatible.
#[derive(Deserialize)]
struct FullItem {
    id: uuid::Uuid,
    name: String,
    #[serde(default)]
    name_translations: Value,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    description_translations: Value,
    #[serde(default)]
    category_id: Option<uuid::Uuid>,
    base_price: i32,
    #[serde(default)]
    image_url: Option<String>,
    is_active: bool,
    #[serde(default)]
    deleted_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    #[serde(default)]
    default_milk_addon_id: Option<String>,
    #[serde(default)]
    allowed_addon_ids: Vec<String>,
    #[serde(default)]
    sizes: Vec<FullSize>,
    #[serde(default)]
    addon_slots: Vec<FullSlot>,
    #[serde(default)]
    optional_fields: Vec<FullOptional>,
    #[serde(default)]
    recipes: Vec<FullRecipe>,
}

#[derive(Deserialize)]
struct FullRecipe {
    #[serde(default)]
    ingredient_name: String,
    #[serde(default)]
    ingredient_unit: String,
    #[serde(default)]
    size_label: Option<String>,
    #[serde(default)]
    category: String,
    #[serde(default)]
    org_ingredient_id: Option<String>,
    // numeric(12,3) → BigDecimal → JSON STRING ("18.000"). Captured as a Value so
    // the string-vs-number encoding can't break the parse; projected to f64 below.
    #[serde(default)]
    quantity_used: Value,
}

#[derive(Deserialize)]
struct FullSize {
    id: uuid::Uuid,
    label: String,
    price_override: i32,
    is_active: bool,
}

#[derive(Deserialize)]
struct FullSlot {
    id: uuid::Uuid,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    label_translations: Value,
    addon_type: String,
    is_required: bool,
    min_selections: i32,
    #[serde(default)]
    max_selections: Option<i32>,
}

#[derive(Deserialize)]
struct FullOptional {
    id: uuid::Uuid,
    name: String,
    #[serde(default)]
    name_translations: Value,
    price: i32,
    is_active: bool,
    #[serde(default)]
    ingredient_name: Option<String>,
    #[serde(default)]
    ingredient_unit: Option<String>,
    #[serde(default)]
    org_ingredient_id: Option<String>,
    // numeric → BigDecimal → JSON STRING; tolerant Value (projected below).
    #[serde(default)]
    quantity_used: Value,
}

// Tolerant local shape for the `/addon-items` wire. Like the menu, the embedded
// ingredient `quantity_used` is a Postgres `numeric` → BigDecimal → JSON STRING,
// so the generated `models::AddonItem` (which types it `f64`) can't parse it. We
// capture only what the catalog + recipe preview need, decimals as `Value`.
#[derive(Deserialize)]
struct FullAddon {
    id: uuid::Uuid,
    name: String,
    #[serde(default)]
    name_translations: Value,
    addon_type: String,
    default_price: i32,
    is_active: bool,
    #[serde(default)]
    ingredients: Vec<FullAddonIngredient>,
}

#[derive(Deserialize)]
struct FullAddonIngredient {
    #[serde(default)]
    ingredient_name: String,
    #[serde(default)]
    ingredient_unit: String,
    #[serde(default)]
    org_ingredient_id: Option<String>,
    #[serde(default)]
    quantity_used: Value,
}

pub(crate) fn menu_items(store: &Store, locale: &str) -> CoreResult<Vec<MenuItemView>> {
    let items: Vec<FullItem> = parse_kv_lenient(store, K_MENU_ITEMS)?;
    Ok(items
        .into_iter()
        .filter(|i| i.deleted_at.is_none())
        .map(|i| MenuItemView {
            id: i.id.to_string(),
            name: resolve(&i.name_translations, &i.name, locale),
            description: i.description.clone().map(|d| resolve(&i.description_translations, &d, locale)),
            category_id: i.category_id.map(|c| c.to_string()),
            base_price_minor: i.base_price as i64,
            image_url: i.image_url.clone(),
            is_active: i.is_active,
            default_milk_addon_id: i.default_milk_addon_id.clone(),
            allowed_addon_ids: i.allowed_addon_ids.clone(),
            recipes: i
                .recipes
                .iter()
                .map(|r| RecipeLineView {
                    ingredient_name: r.ingredient_name.clone(),
                    quantity: value_to_f64(&r.quantity_used),
                    unit: r.ingredient_unit.clone(),
                    size_label: r.size_label.clone().filter(|s| !s.is_empty()),
                    category: r.category.clone(),
                    org_ingredient_id: r.org_ingredient_id.clone().filter(|s| !s.is_empty()),
                })
                .collect(),
            sizes: i
                .sizes
                .iter()
                .map(|s| ItemSizeView {
                    id: s.id.to_string(),
                    label: s.label.clone(),
                    price_minor: s.price_override as i64,
                    is_active: s.is_active,
                })
                .collect(),
            addon_slots: i
                .addon_slots
                .iter()
                .map(|sl| AddonSlotView {
                    id: sl.id.to_string(),
                    label: sl.label.clone().map(|l| resolve(&sl.label_translations, &l, locale)),
                    addon_type: sl.addon_type.clone(),
                    is_required: sl.is_required,
                    min_selections: sl.min_selections,
                    max_selections: sl.max_selections,
                })
                .collect(),
            optional_fields: i
                .optional_fields
                .iter()
                .map(|o| OptionalFieldView {
                    id: o.id.to_string(),
                    name: resolve(&o.name_translations, &o.name, locale),
                    price_minor: o.price as i64,
                    is_active: o.is_active,
                    ingredient_name: o.ingredient_name.clone().filter(|s| !s.is_empty()),
                    ingredient_unit: o.ingredient_unit.clone().filter(|s| !s.is_empty()),
                    quantity_used: value_to_opt_f64(&o.quantity_used),
                    org_ingredient_id: o.org_ingredient_id.clone().filter(|s| !s.is_empty()),
                })
                .collect(),
        })
        .collect())
}

pub(crate) fn categories(store: &Store, locale: &str) -> CoreResult<Vec<CategoryView>> {
    let cats: Vec<models::Category> = parse_kv(store, K_CATEGORIES)?;
    Ok(cats
        .into_iter()
        .filter(|c| flat(&c.deleted_at).is_none())
        .map(|c| CategoryView {
            id: c.id.to_string(),
            name: resolve(&c.name_translations, &c.name, locale),
            image_url: flat(&c.image_url),
            is_active: c.is_active,
        })
        .collect())
}

pub(crate) fn addons(store: &Store, locale: &str) -> CoreResult<Vec<AddonItemView>> {
    // Lenient, decimal-tolerant parse: the `/addon-items` wire embeds ingredient
    // `quantity_used` as a BigDecimal string, which the generated `AddonItem`
    // (f64) can't decode — and a single bad row must never blank the addon list.
    let items: Vec<FullAddon> = parse_kv_lenient(store, K_ADDONS)?;
    Ok(items
        .into_iter()
        .map(|a| AddonItemView {
            id: a.id.to_string(),
            name: resolve(&a.name_translations, &a.name, locale),
            addon_type: a.addon_type.clone(),
            default_price_minor: a.default_price as i64,
            is_active: a.is_active,
            ingredients: a
                .ingredients
                .iter()
                .map(|ing| AddonIngredientView {
                    ingredient_name: ing.ingredient_name.clone(),
                    unit: ing.ingredient_unit.clone(),
                    quantity: value_to_f64(&ing.quantity_used),
                    org_ingredient_id: ing.org_ingredient_id.clone().filter(|s| !s.is_empty()),
                })
                .collect(),
        })
        .collect())
}

pub(crate) fn bundles(store: &Store, locale: &str) -> CoreResult<Vec<BundleView>> {
    let items: Vec<models::Bundle> = parse_kv(store, K_BUNDLES)?;
    Ok(items
        .into_iter()
        .map(|b| BundleView {
            id: b.id.to_string(),
            name: resolve(b.name_translations.as_ref().unwrap_or(&Value::Null), &b.name, locale),
            description: flat(&b.description).map(|d| {
                resolve(b.description_translations.as_ref().unwrap_or(&Value::Null), &d, locale)
            }),
            price_minor: b.price as i64,
            image_url: flat(&b.image_url),
            is_available: matches!(b.status, models::BundleStatus::Active),
            available_from_date: flat(&b.available_from_date).map(|d| d.to_string()),
            available_until_date: flat(&b.available_until_date).map(|d| d.to_string()),
            available_from_time: flat(&b.available_from_time),
            available_until_time: flat(&b.available_until_time),
        })
        .collect())
}

/// True when `b` is orderable at `now` (branch-local wall-clock): status active
/// AND within its optional date and time windows (inclusive). Branch filtering
/// is already applied server-side. Mirrors Flutter's `isBundleAvailableNow`.
pub(crate) fn bundle_available(b: &BundleView, now: chrono::DateTime<chrono::FixedOffset>) -> bool {
    use chrono::Timelike;
    if !b.is_available {
        return false;
    }
    let date = now.date_naive();
    if let Some(from) = b.available_from_date.as_deref().and_then(parse_ymd) {
        if date < from {
            return false;
        }
    }
    if let Some(until) = b.available_until_date.as_deref().and_then(parse_ymd) {
        if date > until {
            return false;
        }
    }
    let mins = now.hour() as i32 * 60 + now.minute() as i32;
    if let Some(from) = b.available_from_time.as_deref().and_then(parse_hm) {
        if mins < from {
            return false;
        }
    }
    if let Some(until) = b.available_until_time.as_deref().and_then(parse_hm) {
        if mins > until {
            return false;
        }
    }
    true
}

/// "YYYY-MM-DD" (or the date prefix of an RFC3339 string) → NaiveDate.
fn parse_ymd(s: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(&s[..s.len().min(10)], "%Y-%m-%d").ok()
}

/// "HH:MM[:SS]" → minutes since midnight.
fn parse_hm(s: &str) -> Option<i32> {
    let mut it = s.split(':');
    let h: i32 = it.next()?.parse().ok()?;
    let m: i32 = it.next()?.parse().ok()?;
    Some(h * 60 + m)
}

pub(crate) fn payment_methods(store: &Store, locale: &str) -> CoreResult<Vec<PaymentMethodView>> {
    let items: Vec<models::OrgPaymentMethod> = parse_kv(store, K_PAYMENT_METHODS)?;
    Ok(items
        .into_iter()
        .filter(|p| p.is_active)
        .map(|p| PaymentMethodView {
            id: p.id.to_string(),
            name: resolve(p.label_translations.as_ref().unwrap_or(&Value::Null), &p.name, locale),
            is_cash: p.is_cash,
            icon: p.icon.clone(),
            color: p.color.clone(),
        })
        .collect())
}

pub(crate) fn discounts(store: &Store, locale: &str) -> CoreResult<Vec<DiscountView>> {
    let items: Vec<models::Discount> = parse_kv(store, K_DISCOUNTS)?;
    Ok(items
        .into_iter()
        .map(|d| DiscountView {
            id: d.id.to_string(),
            name: resolve(&d.name_translations, &d.name, locale),
            dtype: d.dtype.clone(),
            value: d.value as i64,
            is_active: d.is_active,
        })
        .collect())
}

// ── helpers ─────────────────────────────────────────────────────────────────

/// Parse a kv catalog stream into a typed vec; an absent key = empty list (the
/// host shows nothing until the first sync, never an error).
fn parse_kv<T: serde::de::DeserializeOwned>(store: &Store, key: &str) -> CoreResult<Vec<T>> {
    match store.kv_get(key)? {
        Some(json) => Ok(serde_json::from_str(&json)?),
        None => Ok(Vec::new()),
    }
}

/// Like `parse_kv`, but parses the array element-by-element and SKIPS any row
/// that fails to deserialize, instead of failing the whole stream. A single
/// malformed item must never blank an entire catalog screen — better to show the
/// rows that parse. Used for the menu items, whose `?full=true` payload is the
/// widest (and historically the most fragile) shape on the wire.
fn parse_kv_lenient<T: serde::de::DeserializeOwned>(store: &Store, key: &str) -> CoreResult<Vec<T>> {
    let rows: Vec<Value> = match store.kv_get(key)? {
        Some(json) => serde_json::from_str(&json)?,
        None => return Ok(Vec::new()),
    };
    Ok(rows.into_iter().filter_map(|v| serde_json::from_value(v).ok()).collect())
}

/// Resolve a `*_translations` object to the device locale, falling back
/// locale → its language subtag → `en` → the base field (R9).
fn resolve(translations: &Value, base: &str, locale: &str) -> String {
    let lang = locale.split(['-', '_']).next().unwrap_or(locale);
    for key in [locale, lang, "en"] {
        if let Some(s) = translations.get(key).and_then(Value::as_str) {
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    base.to_string()
}

/// Collapse the generator's `Option<Option<T>>` (the absent-vs-null double
/// option) to a plain `Option<T>`.
fn flat<T: Clone>(opt: &Option<Option<T>>) -> Option<T> {
    opt.clone().flatten()
}

/// A JSON number OR a BigDecimal-as-string ("18.000") → f64 (0.0 on failure).
fn value_to_f64(v: &Value) -> f64 {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0.0)
}

/// Like `value_to_f64` but preserves absence: JSON `null`/missing → `None`, so a
/// cosmetic optional (no ingredient deduction) is distinguishable from a 0 qty.
fn value_to_opt_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Null => None,
        Value::Number(_) => v.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(store: &Store, key: &str, json: &str) {
        store.kv_put(key, json).unwrap();
    }

    #[test]
    fn menu_items_project_and_resolve_locale() {
        let store = Store::open("").unwrap();
        seed(
            &store,
            K_MENU_ITEMS,
            r#"[{
              "base_price": 5000,
              "created_at": "2026-06-19T10:00:00Z",
              "updated_at": "2026-06-19T10:00:00Z",
              "id": "00000000-0000-0000-0000-0000000000a1",
              "org_id": "00000000-0000-0000-0000-0000000000ff",
              "is_active": true,
              "name": "Latte",
              "name_translations": {"ar": "لاتيه", "en": "Latte"},
              "description_translations": {},
              "addon_slots": [{
                  "addon_type": "milk_type", "id": "00000000-0000-0000-0000-0000000000b1",
                  "created_at": "2026-06-19T10:00:00Z",
                  "is_required": false, "label": null, "label_translations": {},
                  "max_selections": null, "menu_item_id": "00000000-0000-0000-0000-0000000000a1",
                  "min_selections": 0
              }],
              "allowed_addon_ids": [],
              "optional_fields": [],
              "recipes": [],
              "sizes": [{
                  "id": "00000000-0000-0000-0000-0000000000c1", "is_active": true,
                  "label": "Large", "menu_item_id": "00000000-0000-0000-0000-0000000000a1",
                  "price_override": 6000
              }]
            }]"#,
        );

        let ar = menu_items(&store, "ar-EG").unwrap();
        assert_eq!(ar.len(), 1);
        assert_eq!(ar[0].name, "لاتيه"); // ar-EG → ar
        assert_eq!(ar[0].base_price_minor, 5000);
        assert_eq!(ar[0].sizes[0].price_minor, 6000);
        assert_eq!(ar[0].addon_slots[0].max_selections, None); // null = no cap

        let en = menu_items(&store, "en").unwrap();
        assert_eq!(en[0].name, "Latte");

        // Unknown locale with no match → base field.
        let fr = menu_items(&store, "fr").unwrap();
        assert_eq!(fr[0].name, "Latte");
    }

    #[test]
    fn menu_items_tolerate_bigdecimal_string_quantity_used() {
        // Regression: the backend serializes `quantity_used` (Postgres numeric)
        // via BigDecimal, i.e. as a JSON STRING. The generated MenuItemFull types
        // it `f64`, so the full-payload parse used to fail and blank the menu.
        // Recipes + optional_fields here carry string quantity_used — the read
        // must still surface the item.
        let store = Store::open("").unwrap();
        seed(
            &store,
            K_MENU_ITEMS,
            r#"[{
              "base_price": 5000,
              "created_at": "2026-06-19T10:00:00Z",
              "updated_at": "2026-06-19T10:00:00Z",
              "id": "00000000-0000-0000-0000-0000000000a1",
              "org_id": "00000000-0000-0000-0000-0000000000ff",
              "is_active": true,
              "name": "Latte",
              "name_translations": {"en": "Latte"},
              "description_translations": {},
              "addon_slots": [],
              "allowed_addon_ids": ["00000000-0000-0000-0000-0000000000d1"],
              "optional_fields": [{
                  "id": "00000000-0000-0000-0000-0000000000f1",
                  "created_at": "2026-06-19T10:00:00Z",
                  "updated_at": "2026-06-19T10:00:00Z",
                  "menu_item_id": "00000000-0000-0000-0000-0000000000a1",
                  "name": "Extra shot",
                  "name_translations": {"en": "Extra shot"},
                  "price": 1500,
                  "is_active": true,
                  "quantity_used": "0.500",
                  "ingredient_unit": "shot"
              }],
              "recipes": [{
                  "category": "coffee", "ingredient_name": "Beans",
                  "ingredient_unit": "g", "quantity_used": "18.000", "size_label": "Large"
              }],
              "sizes": []
            }]"#,
        );

        let items = menu_items(&store, "en").unwrap();
        assert_eq!(items.len(), 1, "string quantity_used must not blank the menu");
        assert_eq!(items[0].name, "Latte");
        assert_eq!(items[0].base_price_minor, 5000);
        assert_eq!(items[0].optional_fields.len(), 1);
        assert_eq!(items[0].optional_fields[0].name, "Extra shot");
        assert_eq!(items[0].optional_fields[0].price_minor, 1500);
        // Optional ingredient mapping projects (BigDecimal-string qty tolerated).
        assert_eq!(items[0].optional_fields[0].quantity_used, Some(0.5));
        assert_eq!(items[0].optional_fields[0].ingredient_unit.as_deref(), Some("shot"));
        assert_eq!(items[0].optional_fields[0].ingredient_name, None); // absent → None
        // Recipes now parse despite the BigDecimal-as-string quantity_used.
        assert_eq!(items[0].recipes.len(), 1);
        assert_eq!(items[0].recipes[0].ingredient_name, "Beans");
        assert_eq!(items[0].recipes[0].quantity, 18.0);
        assert_eq!(items[0].recipes[0].unit, "g");
        assert_eq!(items[0].recipes[0].size_label.as_deref(), Some("Large"));
        assert_eq!(items[0].recipes[0].category, "coffee");
        // Per-item addon allowlist surfaces for the "show item's options" default.
        assert_eq!(items[0].allowed_addon_ids, vec!["00000000-0000-0000-0000-0000000000d1"]);
    }

    #[test]
    fn menu_items_skip_malformed_rows_keep_good_ones() {
        // One broken row (missing required base_price) must not nuke the rest.
        let store = Store::open("").unwrap();
        seed(
            &store,
            K_MENU_ITEMS,
            r#"[
              {"id":"00000000-0000-0000-0000-0000000000a1","name":"Broken","is_active":true},
              {"base_price":4200,"created_at":"2026-06-19T10:00:00Z","updated_at":"2026-06-19T10:00:00Z",
               "id":"00000000-0000-0000-0000-0000000000a2","org_id":"00000000-0000-0000-0000-0000000000ff",
               "is_active":true,"name":"Espresso","name_translations":{"en":"Espresso"},
               "description_translations":{},"addon_slots":[],"allowed_addon_ids":[],
               "optional_fields":[],"recipes":[],"sizes":[]}
            ]"#,
        );
        let items = menu_items(&store, "en").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Espresso");
    }

    #[test]
    fn absent_catalog_is_empty_not_error() {
        let store = Store::open("").unwrap();
        assert!(menu_items(&store, "en").unwrap().is_empty());
        assert!(categories(&store, "en").unwrap().is_empty());
        assert!(addons(&store, "en").unwrap().is_empty());
        assert!(bundles(&store, "en").unwrap().is_empty());
        assert!(payment_methods(&store, "en").unwrap().is_empty());
        assert!(discounts(&store, "en").unwrap().is_empty());
    }

    #[test]
    fn addons_and_payment_methods_project() {
        let store = Store::open("").unwrap();
        seed(
            &store,
            K_ADDONS,
            r#"[{"addon_type":"milk_type","created_at":"2026-06-19T10:00:00Z","updated_at":"2026-06-19T10:00:00Z","default_price":1500,"id":"00000000-0000-0000-0000-0000000000d1","is_active":true,"name":"Oat Milk","name_translations":{"ar":"حليب شوفان"},"org_id":"00000000-0000-0000-0000-0000000000ff","ingredients":[{"ingredient_name":"Oat milk","ingredient_unit":"ml","org_ingredient_id":"00000000-0000-0000-0000-0000000000aa","quantity_used":"200.000"}]}]"#,
        );
        let a = addons(&store, "ar").unwrap();
        assert_eq!(a[0].name, "حليب شوفان");
        assert_eq!(a[0].default_price_minor, 1500);
        assert_eq!(a[0].addon_type, "milk_type");
        // Embedded ingredients project despite the BigDecimal-string quantity.
        assert_eq!(a[0].ingredients.len(), 1);
        assert_eq!(a[0].ingredients[0].ingredient_name, "Oat milk");
        assert_eq!(a[0].ingredients[0].quantity, 200.0);
        assert_eq!(a[0].ingredients[0].org_ingredient_id.as_deref(), Some("00000000-0000-0000-0000-0000000000aa"));

        seed(
            &store,
            K_PAYMENT_METHODS,
            r##"[{"color":"#000","created_at":"2026-06-19T10:00:00Z","updated_at":"2026-06-19T10:00:00Z","icon":"cash","id":"00000000-0000-0000-0000-0000000000e1","is_active":true,"is_cash":true,"name":"Cash","org_id":"00000000-0000-0000-0000-0000000000ff","label_translations":null},
              {"color":"#111","created_at":"2026-06-19T10:00:00Z","updated_at":"2026-06-19T10:00:00Z","icon":"card","id":"00000000-0000-0000-0000-0000000000e2","is_active":false,"is_cash":false,"name":"Card","org_id":"00000000-0000-0000-0000-0000000000ff","label_translations":null}]"##,
        );
        let pm = payment_methods(&store, "en").unwrap();
        assert_eq!(pm.len(), 1); // inactive filtered
        assert_eq!(pm[0].name, "Cash");
        assert!(pm[0].is_cash);
    }

    #[test]
    fn bundle_availability_gates_status_and_time_window() {
        fn bv(active: bool, from_t: Option<&str>, until_t: Option<&str>) -> BundleView {
            BundleView {
                id: "b".into(),
                name: "Combo".into(),
                description: None,
                price_minor: 1000,
                image_url: None,
                is_available: active,
                available_from_date: None,
                available_until_date: None,
                available_from_time: from_t.map(String::from),
                available_until_time: until_t.map(String::from),
            }
        }
        let now = chrono::DateTime::parse_from_rfc3339("2026-06-21T10:30:00+00:00").unwrap();
        assert!(bundle_available(&bv(true, None, None), now)); // active, no window
        assert!(!bundle_available(&bv(false, None, None), now)); // inactive (draft/archived)
        assert!(bundle_available(&bv(true, Some("08:00"), Some("12:00")), now)); // inside window
        assert!(!bundle_available(&bv(true, Some("11:00"), Some("12:00")), now)); // before it opens
        assert!(!bundle_available(&bv(true, Some("08:00"), Some("10:00")), now)); // after it closes
    }
}
