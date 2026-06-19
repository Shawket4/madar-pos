I now have everything needed for the audit. Here is my report.

---

# POS Inventory / Recipe Audit

## Verdict: server-side responsibility; POS is display + a read-only recipe preview

The Flutter POS does **not** own inventory state. It never computes, persists, or mutates stock levels. The Rust backend is the single source of truth: it deducts stock atomically when an order is created (`POST /orders`), records the exact deductions into the order item's `deductions_snapshot` JSON column, and restores/wastes on void/cancel. The POS only **reads** that snapshot back to display it, and additionally computes a **non-authoritative recipe preview** client-side purely so the teller can see "what goes into this drink" instantly (and offline) before checkout.

There is exactly **one** piece of real client-side inventory *logic*: `computeRecipeLocally` in `core/api/recipe_api.dart`. It is a preview/UX optimization that mirrors the backend's `preview_recipe` handler — it never affects what actually gets deducted. The deduction of record is whatever the server writes into `deductions_snapshot` at order-create time.

---

## How depletion actually works (server, not POS)

- **Order create** (`core/api/order_api.dart` → `POST /orders`): POS sends only cart line items (item ids, sizes, addons, optionals, quantities). The server computes the recipe, deducts stock, and returns the order with each item carrying a `deductions_snapshot`. The POS sends **no** quantities-on-hand and performs **no** stock arithmetic.
- **Offline orders** (`core/services/offline_queue.dart`): orders created offline are queued and replayed to `POST /orders` on reconnect — i.e. depletion is deferred to the server at sync time. The POS does not provisionally decrement any local stock while offline. (Confirms there is no client-side ledger to keep consistent.)
- **Void** (`order_api.voidOrder` → `POST /orders/{id}/void`) carries a `restore_inventory` bool; the POS just passes the teller's choice. l10n string `orderVoidRestoreBody` = "Ingredients go back into stock" — wording only; the restore happens server-side.
- **Delivery cancel** (`core/api/delivery_api.dart` `cancel`, `restore_inventory` flag) and **delivery finalize** (returns server-produced `warnings` incl. oversold) — again the POS only relays the flag and renders server warnings.

`InventoryDeduction` (`core/models/order.dart:54`) is explicitly a hand-written parse of the **opaque server-written** `deductions_snapshot` blob, with a comment saying it is "not a wire schema." It is read-only display data.

---

## POS inventory/recipe touchpoints (the complete list)

| Touchpoint | File | What it does | Authority |
|---|---|---|---|
| **Recipe preview sheet** | `features/order/widgets/recipe_sheet.dart` + `item_detail_sheet.dart` (`_fetchRecipe`/`_showRecipeSheet`) | Shows ingredient composition for a configured drink before adding to cart. Teller-facing. | Display + local preview compute |
| **Local recipe compute** | `core/api/recipe_api.dart` → `computeRecipeLocally` | Replicates backend recipe math (size filter, addon swaps for milk/coffee, additive addons, optional-field ingredients) so the preview works offline. **The only real client logic.** | Non-authoritative preview |
| **Recipe network fallback** | `recipe_api.dart` → `RecipeApi.preview` → `POST /orders/preview-recipe` (cached) | Used when embedded recipe data is insufficient. | Server compute |
| **Order ingredients sheet** | `features/order/widgets/order_ingredients_sheet.dart` (opened from `order_history_screen.dart:1552`) | Renders the server's `deductions_snapshot` for a placed order line (groups by base/combo/addon/optional). | Pure display of server data |
| **Inventory fetch** | `core/api/inventory_api.dart` (`GET /inventory/branches/{id}/stock`) + `shift_repository.dart` `loadInventoryLocal`/`fetchInventoryFresh` | Fetch/cache `BranchInventoryItem` (current_stock, reorder_threshold, below_reorder, last_counted_at). | Read-only fetch |
| **Void / delivery restore-vs-waste toggle** | `order_api.voidOrder`, `delivery_api.cancel`, `delivery_orders_screen.dart:1535+` | Relays a `restore_inventory` boolean. | Flag relay only |
| **`inventory_counts` on shift close** | `shift_api.close`, `shift_notifier.closeShift`, `pending_action.dart` | Plumbing for a stocktake-at-close, but **always passed as `const []`** from the UI. | Vestigial plumbing |

### Dead / vestigial inventory code (worth flagging)
The teller app appears to have **shed** its inventory-counting and low-stock UI; the plumbing is orphaned:
- `ShiftRepository.loadInventoryLocal` / `fetchInventoryFresh` and `InventoryApi.items` — **zero callers** anywhere in `lib/` (the only reference to `InventoryApi` is the DI wiring in `shift_repository`/its provider). The whole `inventory_api.dart` fetch path is dead.
- `inventoryCounts` flows through `closeShift` / `PendingShiftClose` / `shift_api.close` but the close-shift screen (`features/shift/close_shift_screen.dart:288`) only counts **cash** and never supplies counts → always `[]`. No stocktake UI exists.
- l10n strings `shiftZeroStockTitle`/`shiftZeroStockBody`/`shiftSystemStock`/`orderItemOutOfStock`/`orderOutOfStock` exist but have **no feature-side usages** — leftovers from a removed low-stock/zero-stock-warning feature.

So: **recipe view = yes** (teller-facing), **deductions view = yes** (read-only), **ingredient/low-stock/stocktake screens = no** (removed; only dead plumbing remains). These are dashboard-only concerns now.

---

## Offline implications

- **Recipe preview is offline-capable** via `computeRecipeLocally` (embedded `MenuItemRecipe`/`AddonItemIngredient` data on the cached menu) with a cached `preview-recipe` fallback. Good.
- **No stale-stock risk in the POS UI**, because the POS does not display live stock anywhere (the inventory fetch is dead code). There is no "12 left" counter that could go stale offline. Out-of-stock gating shown to tellers is driven by menu-item/bundle **availability** flags (`bundle.dart` `isBundleAvailableNow`, `formatting.dart` `bundleOutOfStockHint`), not by ingredient stock levels.
- **Depletion is correct-by-construction offline**: since the server deducts at create/sync time and the POS keeps no local ledger, queued offline orders cannot drift a local stock count out of sync — there isn't one. The only offline edge is that an order queued offline could oversell (server may reject/warn at sync), which is surfaced via server `warnings`, not pre-checked client-side.

---

## What (if anything) moves to Rust

**Minimal — confirmed.** Inventory authority is already entirely in Rust. The single candidate is `computeRecipeLocally` (`recipe_api.dart:73-218`), which is a deliberate Dart re-implementation of the Rust `preview_recipe` handler. It is **not** a depletion path, so nothing inventory-authoritative needs extraction. Considerations:

- **Keep it as-is** if offline recipe preview matters — pushing it to Rust would mean a network call and lose the offline path, defeating its purpose.
- The only real cost is **logic duplication**: the addon-swap/category-mapping rules (`milk_type→milk`, `coffee_type→coffee_bean`, optional-field ingredient handling) are maintained in two places and must stay in lockstep with the backend (the code comments already acknowledge "mirrors Rust exactly"). Any drift produces a preview that disagrees with the actual server deduction. If the goal is a single source of truth, that's the thing to consolidate — but it's a preview-accuracy concern, not an inventory-correctness one.

**Cleanup opportunity (separate from extraction):** the dead inventory-fetch/stocktake/low-stock plumbing (`InventoryApi.items`, `loadInventoryLocal`/`fetchInventoryFresh`, unused `inventoryCounts` path, unused zero-stock l10n) can be deleted from the POS without behavior change, since those responsibilities now live only in the dashboard/backend.

---

Key files (absolute paths):
- `/Users/shawket/Desktop/sufrix_pos/lib/core/api/recipe_api.dart` (the only client inventory logic: `computeRecipeLocally`)
- `/Users/shawket/Desktop/sufrix_pos/lib/core/api/inventory_api.dart` (dead read-only fetch)
- `/Users/shawket/Desktop/sufrix_pos/lib/core/models/order.dart` (`InventoryDeduction`, parses server `deductions_snapshot`)
- `/Users/shawket/Desktop/sufrix_pos/lib/core/models/inventory.dart` (façade over generated `BranchInventoryItem`)
- `/Users/shawket/Desktop/sufrix_pos/lib/features/order/widgets/recipe_sheet.dart`, `order_ingredients_sheet.dart`, `item_detail_sheet.dart`
- `/Users/shawket/Desktop/sufrix_pos/lib/core/repositories/shift_repository.dart` (dead `loadInventoryLocal`/`fetchInventoryFresh`)
- `/Users/shawket/Desktop/sufrix_pos/lib/core/api/order_api.dart`, `delivery_api.dart` (server-side deplete/void/restore relays)