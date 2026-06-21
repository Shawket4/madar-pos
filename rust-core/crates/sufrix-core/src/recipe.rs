//! Local recipe preview — the effective ingredient list for a configured item.
//!
//! Ports the Flutter teller app's `computeRecipeLocally` (recipe_api.dart) into
//! the core so the customization sheet can show, live and offline, how the
//! selected size / addons / optionals change the drink's ingredients:
//!   1. base recipe rows for the chosen size (size-agnostic rows always apply),
//!   2. milk/coffee SWAPS — a `milk_type`/`coffee_type` addon replaces the base
//!      line of the matching category in place (inheriting the base quantity),
//!      unless it re-selects the default ingredient (same org-ingredient id),
//!   3. additive addons — every other addon adds its ingredients × selected qty,
//!   4. optional fields that carry an ingredient deduction add their line.
//!
//! Pure (item + catalog + selection in, view rows out) so it's unit-testable
//! without a store or network, and cheap enough to recompute on every toggle.
//! Lines are NOT merged by ingredient — the sheet groups them by source tag.

use crate::cart::AddonSelection;
use crate::menu::{AddonItemView, MenuItemView, RecipeLineView};

/// One effective ingredient line, tagged by origin so the sheet can chip it.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct ComputedRecipeLineView {
    pub ingredient_name: String,
    pub unit: String,
    pub quantity: f64,
    /// Display tag: `"base"`, `"addon"`, the swap addon's name, or the optional
    /// field's name — the sheet renders this (uppercased) as a chip.
    pub source_label: String,
    /// True for base drink-recipe lines (the sheet tones these as the accent).
    pub is_base: bool,
}

// Categories a swap-family addon targets in the base recipe.
const CAT_MILK: &str = "milk";
const CAT_COFFEE: &str = "coffee_bean";

// Internal working row — carries the matching keys (category / org-ingredient id)
// the swap step needs but the host view omits.
#[derive(Clone)]
struct Row {
    name: String,
    unit: String,
    quantity: f64,
    category: String,
    is_base: bool,
    /// `None` for base rows; `Some(label)` once an addon/optional sets the tag.
    source_label: Option<String>,
}

/// Compute the effective recipe for `item` given the chosen `size_label`,
/// `addons` (id + qty) and `optional_ids`. `addon_catalog` supplies the embedded
/// ingredient data (a milk/coffee addon's first ingredient drives the swap).
pub(crate) fn compute_recipe(
    item: &MenuItemView,
    addon_catalog: &[AddonItemView],
    size_label: Option<&str>,
    addons: &[AddonSelection],
    optional_ids: &[String],
) -> Vec<ComputedRecipeLineView> {
    // 1. Base rows for the selected size. A row with no size_label applies to
    //    every size, so it's always included; size-specific rows match the
    //    selection (or, with no size chosen, the first concrete size present).
    let target_size: Option<&str> = size_label.or_else(|| {
        item.recipes.iter().find_map(|r| r.size_label.as_deref())
    });
    let base_rows: Vec<&RecipeLineView> = item
        .recipes
        .iter()
        .filter(|r| match (r.size_label.as_deref(), target_size) {
            (None, _) => true,            // size-agnostic → always
            (Some(rs), Some(ts)) => rs == ts,
            (Some(_), None) => false,
        })
        .collect();

    let mut rows: Vec<Row> = base_rows
        .iter()
        .map(|r| Row {
            name: r.ingredient_name.clone(),
            unit: r.unit.clone(),
            quantity: r.quantity,
            category: r.category.clone(),
            is_base: true,
            source_label: None,
        })
        .collect();

    // 2 + 3. Walk the selected addons: milk/coffee types swap the base line of
    //        the matching category; everything else is additive (× qty).
    for sel in addons {
        let Some(addon) = addon_catalog.iter().find(|a| a.id == sel.addon_item_id) else {
            continue; // unknown addon — skip (Flutter falls back to the API here)
        };
        let addon_qty = sel.qty.max(1) as f64;

        let target_category = match addon.addon_type.as_str() {
            "milk_type" => Some(CAT_MILK),
            "coffee_type" => Some(CAT_COFFEE),
            _ => None,
        };

        if let Some(cat) = target_category {
            // Need the addon's ingredient to know what to swap in. With none, we
            // can't tell a swap from the default → leave the base line untouched.
            let Some(repl) = addon.ingredients.first() else { continue };

            // Only swap when there's a base line of this category to replace.
            let base_ing_id = base_rows
                .iter()
                .find(|b| b.category == cat)
                .and_then(|b| b.org_ingredient_id.clone());
            let has_base = rows.iter().any(|r| r.is_base && r.category == cat);

            // Re-selecting the default ingredient (same org-ingredient id as the
            // base line) is NOT a swap — leave the base line as-is.
            let is_default = match (&base_ing_id, &repl.org_ingredient_id) {
                (Some(b), Some(a)) => b == a,
                _ => false,
            };

            if has_base && !is_default {
                // Replace every base line of this category in place; a swapped
                // line inherits the base quantity (swaps never scale) and is
                // re-tagged with the addon's name (no longer the plain base).
                for r in rows.iter_mut().filter(|r| r.is_base && r.category == cat) {
                    r.name = repl.ingredient_name.clone();
                    r.unit = repl.unit.clone();
                    r.source_label = Some(addon.name.clone());
                    r.is_base = false;
                }
            }
            continue; // swap families never add a separate line
        }

        // Additive addon: append each ingredient, scaled by the selected qty.
        for ing in &addon.ingredients {
            rows.push(Row {
                name: ing.ingredient_name.clone(),
                unit: ing.unit.clone(),
                quantity: ing.quantity * addon_qty,
                category: "general".into(),
                is_base: false,
                source_label: Some("addon".into()),
            });
        }
    }

    // 4. Optional fields that carry an ingredient deduction.
    for oid in optional_ids {
        let Some(f) = item.optional_fields.iter().find(|f| &f.id == oid) else { continue };
        if let (Some(name), Some(unit), Some(qty)) =
            (f.ingredient_name.as_ref(), f.ingredient_unit.as_ref(), f.quantity_used)
        {
            rows.push(Row {
                name: name.clone(),
                unit: unit.clone(),
                quantity: qty,
                category: "general".into(),
                is_base: false,
                source_label: Some(f.name.clone()),
            });
        }
    }

    rows.into_iter()
        .map(|r| ComputedRecipeLineView {
            ingredient_name: r.name,
            unit: r.unit,
            quantity: r.quantity,
            source_label: r.source_label.unwrap_or_else(|| "base".into()),
            is_base: r.is_base,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::{AddonIngredientView, ItemSizeView, OptionalFieldView};

    fn addon(id: &str, name: &str, atype: &str, ings: Vec<AddonIngredientView>) -> AddonItemView {
        AddonItemView {
            id: id.into(),
            name: name.into(),
            addon_type: atype.into(),
            default_price_minor: 0,
            is_active: true,
            ingredients: ings,
        }
    }

    fn ing(name: &str, unit: &str, qty: f64, org: Option<&str>) -> AddonIngredientView {
        AddonIngredientView {
            ingredient_name: name.into(),
            unit: unit.into(),
            quantity: qty,
            org_ingredient_id: org.map(String::from),
        }
    }

    fn recipe(name: &str, unit: &str, qty: f64, size: Option<&str>, cat: &str, org: Option<&str>) -> RecipeLineView {
        RecipeLineView {
            ingredient_name: name.into(),
            quantity: qty,
            unit: unit.into(),
            size_label: size.map(String::from),
            category: cat.into(),
            org_ingredient_id: org.map(String::from),
        }
    }

    fn item(recipes: Vec<RecipeLineView>, optionals: Vec<OptionalFieldView>) -> MenuItemView {
        MenuItemView {
            id: "item1".into(),
            name: "Latte".into(),
            description: None,
            category_id: None,
            base_price_minor: 5000,
            image_url: None,
            is_active: true,
            default_milk_addon_id: Some("milk_default".into()),
            allowed_addon_ids: vec![],
            sizes: vec![ItemSizeView { id: "s1".into(), label: "M".into(), price_minor: 5000, is_active: true }],
            addon_slots: vec![],
            optional_fields: optionals,
            recipes,
        }
    }

    fn sel(id: &str, qty: i64) -> AddonSelection {
        AddonSelection { addon_item_id: id.into(), qty }
    }

    #[test]
    fn base_recipe_filters_by_size_and_keeps_agnostic_rows() {
        let it = item(
            vec![
                recipe("Beans", "g", 18.0, Some("M"), "coffee_bean", Some("o-beans")),
                recipe("Beans", "g", 24.0, Some("L"), "coffee_bean", Some("o-beans")),
                recipe("Water", "ml", 30.0, None, "general", Some("o-water")),
            ],
            vec![],
        );
        let out = compute_recipe(&it, &[], Some("M"), &[], &[]);
        // M coffee row + size-agnostic water; the L row is excluded.
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].ingredient_name, "Beans");
        assert_eq!(out[0].quantity, 18.0);
        assert!(out[0].is_base);
        assert_eq!(out[1].ingredient_name, "Water");
    }

    #[test]
    fn milk_swap_replaces_base_line_inheriting_quantity() {
        let it = item(vec![recipe("Whole milk", "ml", 200.0, Some("M"), "milk", Some("o-whole"))], vec![]);
        let oat = addon("a-oat", "Oat Milk", "milk_type", vec![ing("Oat milk", "ml", 999.0, Some("o-oat"))]);
        let out = compute_recipe(&it, &[oat], Some("M"), &[sel("a-oat", 1)], &[]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].ingredient_name, "Oat milk");
        assert_eq!(out[0].quantity, 200.0, "swap inherits the base quantity, not the addon's");
        assert_eq!(out[0].source_label, "Oat Milk");
        assert!(!out[0].is_base);
    }

    #[test]
    fn reselecting_default_milk_is_not_a_swap() {
        let it = item(vec![recipe("Whole milk", "ml", 200.0, Some("M"), "milk", Some("o-whole"))], vec![]);
        // Same org-ingredient id as the base line → not a swap.
        let same = addon("a-whole", "Whole Milk", "milk_type", vec![ing("Whole milk", "ml", 200.0, Some("o-whole"))]);
        let out = compute_recipe(&it, &[same], Some("M"), &[sel("a-whole", 1)], &[]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].ingredient_name, "Whole milk");
        assert!(out[0].is_base, "default re-selection stays a base line");
        assert_eq!(out[0].source_label, "base");
    }

    #[test]
    fn additive_addon_scales_by_quantity() {
        let it = item(vec![recipe("Beans", "g", 18.0, Some("M"), "coffee_bean", Some("o-beans"))], vec![]);
        let syrup = addon("a-syrup", "Caramel", "extra", vec![ing("Caramel syrup", "ml", 10.0, Some("o-car"))]);
        let out = compute_recipe(&it, &[syrup], Some("M"), &[sel("a-syrup", 2)], &[]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].ingredient_name, "Caramel syrup");
        assert_eq!(out[1].quantity, 20.0, "10ml × 2");
        assert_eq!(out[1].source_label, "addon");
        assert!(!out[1].is_base);
    }

    #[test]
    fn optional_with_ingredient_adds_line_cosmetic_does_not() {
        let with_ing = OptionalFieldView {
            id: "opt-shot".into(),
            name: "Extra shot".into(),
            price_minor: 1500,
            is_active: true,
            ingredient_name: Some("Espresso".into()),
            ingredient_unit: Some("shot".into()),
            quantity_used: Some(1.0),
            org_ingredient_id: Some("o-esp".into()),
        };
        let cosmetic = OptionalFieldView {
            id: "opt-deco".into(),
            name: "Latte art".into(),
            price_minor: 0,
            is_active: true,
            ingredient_name: None,
            ingredient_unit: None,
            quantity_used: None,
            org_ingredient_id: None,
        };
        let it = item(vec![recipe("Beans", "g", 18.0, Some("M"), "coffee_bean", None)], vec![with_ing, cosmetic]);
        let out = compute_recipe(&it, &[], Some("M"), &[], &["opt-shot".into(), "opt-deco".into()]);
        assert_eq!(out.len(), 2, "cosmetic optional contributes no line");
        assert_eq!(out[1].ingredient_name, "Espresso");
        assert_eq!(out[1].quantity, 1.0);
        assert_eq!(out[1].source_label, "Extra shot");
    }

    #[test]
    fn swap_with_no_addon_ingredients_leaves_base_untouched() {
        let it = item(vec![recipe("Whole milk", "ml", 200.0, Some("M"), "milk", Some("o-whole"))], vec![]);
        let empty = addon("a-x", "Mystery Milk", "milk_type", vec![]);
        let out = compute_recipe(&it, &[empty], Some("M"), &[sel("a-x", 1)], &[]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].ingredient_name, "Whole milk");
        assert!(out[0].is_base);
    }

    #[test]
    fn lines_are_not_merged_by_ingredient() {
        let it = item(vec![recipe("Beans", "g", 18.0, Some("M"), "coffee_bean", Some("o-beans"))], vec![]);
        // Two additive shots of the same ingredient stay as two rows.
        let shot = addon("a-shot", "Shot", "extra", vec![ing("Espresso", "ml", 30.0, Some("o-esp"))]);
        let out = compute_recipe(&it, &[shot.clone(), shot], Some("M"), &[sel("a-shot", 1), sel("a-shot", 1)], &[]);
        assert_eq!(out.len(), 3, "base + two separate addon rows, not merged");
    }
}
