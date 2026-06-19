# FE Menu / Catalog Layer — Audit & `rust-core::menu` Module Spec

> Focused re-audit (the first pass under-delivered). Scope: the Sufrix POS catalog that feeds the order screen and the **client-authoritative** pricing engine. Money is integer **piastres** (1 EGP = 100). Evidence cited as `path:line`. FE root: `/Users/shawket/Desktop/sufrix_pos/lib`; wire models `packages/sufrix_api/lib/src/model`; backend `/Users/shawket/Desktop/SufrixRust`.

---

## 0. How the catalog is fetched (current FE behavior)

| Endpoint | FE caller | Branch-aware? | Notes |
|---|---|---|---|
| `GET /categories?org_id` | `MenuApi.categories` (`core/api/menu_api.dart:10`) | No | Org-global, pre-sorted `name ASC`. |
| `GET /menu-items?org_id&full=true[&branch_id]` | `MenuApi.items` (`menu_api.dart:16`) | **Yes** | `List<MenuItemFull>`; with `branch_id`: branch-effective prices + branch-disabled excluded. |
| `GET /menu-items/{id}` | `MenuApi.item` (`menu_api.dart:28`) | **No** | Single full item; **always org-level pricing** — see Gotchas. |
| `GET /addon-items?org_id[&branch_id]` | `MenuApi.addonItems` (`menu_api.dart:34`) | **Yes** | Branch-effective addon prices + branch-disabled excluded. |
| `GET /bundles?org_id[&updated_since]` | `MenuApi.bundles` (`menu_api.dart:46`) | No (price); availability client-side | Tolerates bare array and `{data:[...]}`; keeps `status==active` (`menu_api.dart:62`). |
| `GET /payment-methods` | `PaymentMethodApi.list` (`payment_method_api.dart:9`) | No | Org from **JWT claims**, not a param. |

The teller is bound to one branch (`authProvider.user.branchId`), so the FE always fetches branch-effective data and caches under an `org:branch` key (`menu_notifier.dart:170-172`).

---

## 1. Catalog data model

`core/models/*.dart` are façades over OpenAPI-generated wire models; `*_translations` arrive as raw JSON maps `{"en":..,"ar":..}` (un-resolved server-side).

```
Category (org-global): id, org_id, name, name_translations, image_url?, is_active, deleted_at?

MenuItem (= MenuItemFull, core/models/menu.dart:19):
  id, org_id, category_id?, name, name_translations, description?, description_translations,
  image_url?, base_price:int (branch-effective when fetched w/ branch_id), is_active,
  default_milk_addon_id?, deleted_at?
  ├─ sizes:           List<ItemSize>
  ├─ addon_slots:     List<AddonSlot>
  ├─ optional_fields: List<OptionalField>
  └─ recipes:         List<MenuItemRecipe>  (offline recipe preview; out of catalog scope)

ItemSize: id, menu_item_id, label:String, price_override:int (ABSOLUTE, §2), is_active
AddonSlot: id, menu_item_id, addon_type:String, label?, label_translations,
           is_required, min_selections:int, max_selections:int?  (per-item group)
AddonItem (ORG-LEVEL, fetched separately): id, org_id, name, name_translations,
           addon_type:String, default_price:int (branch-effective w/ branch_id), is_active,
           ingredients?, primary_ingredient_id?
OptionalField: id, menu_item_id, name, name_translations, price:int, size_label? (gates visibility),
           ingredient_*?, org_ingredient_id?, quantity_used?, is_active

Bundle (= BundleWithComponents, core/models/bundle.dart:16):
  id, org_id, name, name_translations, description?, price:int (ORG-GLOBAL, not branch-overridable),
  status:BundleStatus, image_url?, available_from/until_date?, available_from/until_time?:"HH:MM[:SS]",
  branch_ids:List<String> (empty ⇒ all branches), computed_cost:int
  └─ components: List<BundleComponent{ id, bundle_id, item_id, item_name, item_price:int (org base),
                                       item_cost:int, position:int, quantity:int }>

PaymentMethod (= OrgPaymentMethod, core/models/payment_method.dart:9):
  id, org_id, name, label_translations?, color:hex, icon, is_cash, is_active
```

Relationships: `MenuItem.category_id?`→`Category.id` (nullable); `ItemSize/AddonSlot/OptionalField.menu_item_id`→`MenuItem.id`; `AddonSlot.addon_type` joins to org `AddonItem`s of the same `addon_type` (slot uniqueness `(menu_item_id, addon_type)`, backend `menu/handlers.rs:1541`); `BundleComponent.item_id`→`MenuItem.id`; `MenuItem.default_milk_addon_id?`→`AddonItem.id` (backend-computed default milk for swap pricing).

**`is_active` filtering is client-side** — backend `/menu-items` & `/addon-items` filter only `deleted_at IS NULL` + branch availability (`menu/handlers.rs:648,1230`); the FE drops `!isActive` (`menu_notifier.dart:86,107-114`, `item_detail_sheet.dart:280`).

**OptionalField size-gating** — shown only when `sizeLabel==null || sizeLabel==selectedSize` (`optional_fields_card.dart:32-33`); the view must preserve `size_label`.

---

## 2. `priceForSize` + price resolution

**A size's `price_override` is an ABSOLUTE price (full replacement), not a delta; falls back to `base_price` when label null/unknown.** (`core/models/menu.dart:21-28`)

```
priceForSize(label):
  if label==null OR sizes empty: return base_price
  for s in sizes: if s.label==label: return s.price_override   # ABSOLUTE
  return base_price                                            # label not found → base
```

Backend confirms absolute: `upsert_size` stores `price_override` verbatim (`menu/handlers.rs:1096-1108`); branch overlay does `s.price_override = *p` (`:2339`); DB `price_override >= 0` (migration `...130000`).

Line price (`item_detail_sheet.dart:387-388,648-649`):
```
unitPrice  = priceForSize(selectedSize)
lineTotal  = (unitPrice + addonsTotal + optionalsTotal) * qty
```
- Addon swap families (`milk_type`,`coffee_type`): charge only the **positive delta** over the item's default-milk base (`default_milk_addon_id`); non-swap addons charge full `default_price` (`item_detail_sheet.dart:394-401,486-500`).
- Bundle line: `bundle.price + Σ component extras` (base NOT re-derived from components).

---

## 3. Branch override merge (the heart of the spec)

Runs **server-side today**; the FE consumes resolved prices. Three tables (a 4th `in_mall`/`outside` channel layer exists only in `src/delivery/`, not the POS path):

| Table (migration) | Price | Availability | Granularity |
|---|---|---|---|
| `branch_menu_overrides` (`...120000`) | yes (null→inherit) | yes | per item |
| `branch_menu_size_overrides` (`...130000`) | yes (NOT NULL, absolute) | **no** | per (item, size) |
| `branch_addon_overrides` (`...140000`) | yes (null→inherit) | yes | per addon |

Precedence (reimplement exactly):
```
effective_item_price = branch_menu_overrides[B,item].price_override ?? base_price
item_available       = branch_menu_overrides[B,item].is_available  ?? true   // drop if false (EXCLUDED, not flagged)
effective_size_price(s) = branch_menu_size_overrides[B,item,s.label].price_override ?? item_sizes[s].price_override
        // NOTE: item-level price override does NOT feed size prices — sizes change ONLY via the size-override table
effective_addon_price = branch_addon_overrides[B,addon].price_override ?? default_price
addon_available       = branch_addon_overrides[B,addon].is_available ?? true // drop if false
```
All `price_override` values at every layer are **absolute piastres, never deltas**. Backend: item merge `menu/handlers.rs:615-677` (COALESCE price `:631`, availability exclusion `:648`), size overlay in Rust `:2322-2343`, addon merge `:1209-1245`. **Bundles are NOT branch-priced** (`bundles.price` org-global; branches toggle only availability via `branch_ids` + date/time windows). Bundle availability: `isBundleAvailableNow` requires `status==active`, branch in `branch_ids` (or empty), now inside date AND time-of-day window; **unknown status ⇒ unavailable** (`core/models/bundle.dart:127-162`).

---

## 4. Translation resolution

Server returns `*_translations` as raw maps; **all locale resolution is client-side.** Device locale: `localeProvider` (`locale_notifier.dart:7-29`, supported `['ar','en']`, falls back to platform→`en`); widgets read `Localizations.localeOf(context).languageCode`.

Lookup + fallback (`core/models/menu.dart:73-82`):
```
lang = locale[0..2]
translations[lang] (non-empty String) → use it          # 1. requested locale
else translations['en'] (non-empty)   → use it          # 2. English
else fallback (the base scalar field, e.g. `name`)        # 3. wire base
```

Translated today: Category.name, MenuItem.name, AddonItem.name, PaymentMethod.label, Bundle.name. **Dropped today (resolve in port):** AddonSlot.label (`label_translations` ignored → uses raw `label`/title-cased `addon_type`), OptionalField.name (`name_translations` ignored), MenuItem.description. `normaliseName(...)` is cosmetic cleanup, separate from translation.

---

## 5. Caching & freshness

KV store `kv(k,v,ts)` records a write timestamp per key (`kv_store.dart:46-56`). Caches:

| Cache | KV key | Scope | sync_meta entity |
|---|---|---|---|
| Menu (cats+items) | `menu_v2_<scope>` | `org:branch` | `menu:<scope>` |
| Bundles | `bundles_v1_<orgId>` | `org` | `bundles:<orgId>` |
| Addons | `addons_<scope>` | `org:branch` | `addons:<scope>` |
| Payment methods | `payment_methods_<orgId>` | `org` | — (no TTL) |

`scope = orgId` or `orgId:branchId` (`menu_repository.dart:15-16`). TTL **10 min** (`menu_repository.dart:10`). Two-phase load (`menu_notifier.dart:157-279`): paint local instantly (`freshness=offline` if no net, else `stale`), then per-entity refresh only if `isStale||force||local==null` → `freshness=live`; network failure demotes to `offline`, keeps local. **Full-refetch per entity (no deltas)** — bundles API accepts `updated_since` but the notifier never passes it.

### The `cachedAt` bug — two compounding defects
- **(A) stale `cachedAt` despite `live`:** `:236` re-reads `menuCachedAt(scope)` = the **menu blob's** write ts (`storage_service.dart:139-143`), but the menu blob is re-saved only when the *menu* entity was stale. A partial refresh (only addons/bundles stale) shows `live` with an **old** timestamp.
- **(B) `copyWith` null-coalesces `cachedAt` with no `clearCachedAt`** (`:144`) → a timestamp can bleed across org/branch.
- Root flaw: a **1-entity timestamp decorating a 3-entity freshness state.** **Fix:** stamp `cachedAt=now()` on transition to `live` (or take max of the 3 `sync_meta` rows); add `clearCachedAt`; clear on org/branch change.

---

## 6. The `rust-core::menu` module contract

Move **all price/availability/translation resolution into Rust**; native keeps only image widgets + chrome.

```rust
fn list_sellable_menu(branch_id, locale) -> Vec<MenuItemView>;   // overrides applied, is_active+availability filtered, translated, sizes/slots/optionals attached
fn list_categories(branch_id, locale) -> Vec<CategoryView>;       // only categories with >=1 sellable item
fn price_for_size(item, size_label: Option<&str>) -> i64;         // mirrors priceForSize (absolute size price; base on null/unknown)
fn addons_by_type(branch_id, locale) -> Map<String, Vec<AddonItemView>>;  // branch-effective, is_active+availability applied
fn available_bundles(branch_id, locale, now) -> Vec<BundleView>;  // status active + branch + date/time window; hydrated
fn list_payment_methods(locale) -> Vec<PaymentMethodView>;        // active+inactive (caller filters); labels resolved
fn price_line(item, sel) -> LinePrice;                            // priceForSize + sum adjusted_addon*qty + sum optional.price, x qty
```
Views carry resolution baked in (translated names, branch-effective absolute prices, `size_label` gate preserved, `display_name` for slots). Rust owns: fetch+cache, TTL/freshness, override merge, price_for_size/line pricing, swap-delta, translation (incl. slot/optional), availability/time-window filtering, bundle savings. Native owns: image loading/placeholders, icons, theme, animations.

> **Architecture note (decided post-audit, see PLAN R9):** because the POS is bound to ONE branch, the module consumes the server's **branch-effective** catalog and caches it (read-cache) rather than fetching raw base + override tables and merging client-side. Rust still owns translation/availability/`priceForSize`/line-pricing; the §3 merge stays server-side. The §3 algorithm is documented for correctness/parity and in case multi-branch-on-device is ever needed.

---

## 7. Gotchas / fix-in-port
1. **`cachedAt` bug (§5).** Stamp from the refresh event / max sync_meta; add `clearCachedAt`; clear on scope change. (`menu_notifier.dart:144,236`)
2. **`GET /menu-items/{id}` is NOT branch-aware** (`menu/handlers.rs:799-817`) → returns org base prices, diverging from the branch list. Port: never refetch single items for pricing (resolve from the cached branch list) **or** make the endpoint branch-aware.
3. **AddonSlot/OptionalField translations dropped** (§4) — resolve in port.
4. **Branch-disabled items/addons are removed, never flagged** — a cart line / bundle component referencing a now-removed item must degrade gracefully (FE already does for bundles, `bundle_detail_sheet.dart:175-205`).
5. **No per-size availability** — item is wholly available or wholly hidden.
6. **Item-level branch price override does NOT affect size prices** — don't apply it to sized SKUs.
7. **`max_selections==null` = multi-select, no cap;** single-select only when `==1`; required satisfaction uses `min_selections.clamp(1,999)` (`item_detail_sheet.dart:313,352,438-444,765`).
8. **Swap-family addon pricing** (positive delta over default milk) lives in `price_line`, not the UI.
9. **Unknown enum safety** — `BundleStatus` unknown ⇒ unavailable; keep that stance.
10. **Bundle prices are org-global** — no branch resolution for bundle price (only availability/time).
11. **Payment-method org from JWT**, returns active+inactive ordered by `created_at` (seeded `display_order` ignored); `wireFormat==name` is the literal order-payload value — keep byte-stable.
