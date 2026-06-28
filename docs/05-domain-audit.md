# Madar POS — Selling-Core & Domain Audit (Extraction Map, Part 2)

> Companion to `04-offline-audit.md`. Covers cart/pricing, checkout/payment/void, menu/catalog, delivery/realtime, inventory/recipe + backend pricing authority & stock depletion. Raw findings in `docs/audit/`.

# Madar POS Rebuild — Selling-Core, Inventory & Delivery Audit (Report 02)

> Companion to `docs/04-offline-audit.md` (printing, offline/cache, routing, shift/auth, backend offline support). This report covers the **selling core** (cart pricing, checkout/payment/void), **menu/catalog**, **inventory/recipe**, and **delivery + realtime**, and folds them into the locked rebuild architecture: **all logic in a shared Rust core (UniFFI), thin SwiftUI/Compose UIs; one drawer per branch; Epson + Star fully in Rust over TCP.**

---

## 1. Executive summary

1. **Pricing is effectively CLIENT-AUTHORITATIVE for every recorded money field — confirmed.** The backend's `create_order` records the POS-supplied breakdown **verbatim** (`body.subtotal.unwrap_or(...)`, `discount_amount.unwrap_or_else(...).clamp(0, subtotal)`, `tax_amount.unwrap_or_else(...)`, `total_amount.unwrap_or(...)` — `orders/handlers.rs:1034–1039`). It independently computes an *expected* total only to set an advisory `price_flagged` boolean + `price_expected_total`, and **nothing in production ever reads those columns** (no report, list filter, or aggregate; the `Order` response struct and `RETURNING` clause don't even expose them). **No order is ever rejected on a price/total/tax/discount mismatch.** The only money-related hard reject is split payments not summing to `total_amount`. **Consequence: the Rust pricing core IS the source of truth for all money. There is no server safety net.**

2. **The pricing algorithm must be ported byte-identically.** Integer piastres throughout, exactly two `round()` calls (ties-away-from-zero), discount-then-tax order, single org-wide exclusive tax rate, bundle base price fixed + component surcharge only. The POS getters and the server formula already agree to the piastre — the port must preserve that agreement so `price_flagged` stays clean (false deviations are the only failure mode the backend surfaces, and nobody watches it today).

3. **The dual idempotency-key scheme is the highest-severity money bug — fix it in the port.** Online create uses `state.id` (the cart tab id, **defaulting to the literal `"order_1"`**); the queued/offline path mints a fresh `Uuid().v4()` (`offline_queue.dart:502`). A lost-response after an online commit, then a network-error fallback to `placeQueued()`, sends a **different key** → the server sees a never-seen key → **second order, double charge / double drawer expectation**, with no 409 to catch it. The backend is provably idempotent *if* one stable key is reused (partial unique index on `orders.idempotency_key`); the client just isn't reusing it.

4. **Stock depletion is server-side and replay-safe — for orders.** Depletion is atomic with the order insert (single tx, `orders/handlers.rs:1093→1575`), pre-checked by idempotency key, and race-guarded by a partial unique index (23505 → rollback + replay). A queued offline order replayed N times deducts **exactly once**. The POS keeps **no local stock ledger**, so there is no client-side double-deduction risk to port. Inventory is already fully owned by Rust-backend.

5. **Delivery is online-only and the SSE stream has no replay cursor — two structural gaps.** Delivery mutations (accept/advance/finalize/cancel/prep-time) never touch the offline queue; they fail hard offline. The SSE stream is updates-only, in-process, single-instance, 128-event ring, **no `Last-Event-ID`/sequence** — any event during a disconnect is lost from the stream, recovered only by a full re-GET on reconnect or the 30s poll. **No push (FCM/APNs) anywhere**, so a backgrounded/killed teller never alerts.

6. **The POS has shed inventory-counting UI; only dead plumbing remains.** `InventoryApi.items`, `loadInventoryLocal`/`fetchInventoryFresh`, the `inventoryCounts` close-shift path (always `[]`), and zero-stock l10n strings have **zero callers**. The port should not re-implement these — they are dashboard-only concerns. No live stock counter exists in the POS to go stale offline (good).

7. **Three change-due formulas disagree when there's a cash tip.** Cart model and receipt compute `tendered − total` (ignore tip); the live UI readout shows `tendered − total − cashTip`. The teller sees one number, the receipt/DB record another. Pick one definition in the port and route all three through it.

8. **Detection/decision logic is the natural Rust boundary; OS notification posting and the printer transport stay native/Rust-TCP respectively.** SSE parsing, reconnect/stall/backoff, new-order dedup, gate decisions → Rust core. The OS notification post (Android `NotificationManager`, iOS `UNUserNotificationCenter`) is the only piece that *must* stay native. Printing moves fully into Rust over TCP per the locked decision.

---

## 2. Pricing authority verdict + the exact algorithm to port

### 2.1 Verdict (reconciling `fe-cart-pricing` × `be-order-pricing-authority`)

**CLIENT-AUTHORITATIVE for recorded money. Server is a passive recorder + advisory deviation flagger, never an enforcer.**

The two audits agree completely. The POS computes the full breakdown (`cart.dart` getters), sends it on `POST /orders`, and the backend takes each field as-is:

| Server line | Behavior |
|---|---|
| `orders/handlers.rs:1034` | `subtotal = body.subtotal.unwrap_or(<Σ charged lines>)` |
| `:1035` | `discount_amount = body.discount_amount.unwrap_or_else(\|\| calc_discount(...)).clamp(0, subtotal)` |
| `:1037` | `tax_amount = body.tax_amount.unwrap_or_else(\|\| round(taxable * rate))` |
| `:1038` | `total_amount = body.total_amount.unwrap_or(taxable + tax_amount)` |
| `:1056` | `price_flagged = any line deviated OR subtotal != expected OR total != expected` → **advisory only** |

`price_flagged`/`price_expected_total` are **write-only audit residue**: written at `:1229` (and echoed by delivery snapshot replay at `snapshot.rs:538`), but no production read path exists. **The backend will not protect you. Whatever the Rust core computes is the money, the receipt, and the revenue report.** The single design implication: **match the server formula exactly** to keep flags clean, and **always send the full breakdown** (omitting any field silently hands authority back to the server's catalog math, which differs from a stale/offline device).

### 2.2 The exact algorithm the `rust-core::pricing` module must reproduce byte-identically

**Money type:** `i32` piastres (1 EGP = 100). No floats stored. Floats appear only inside the two `round()` operations. `round()` = **ties away from zero** (Dart `double.round()` == Rust `f64::round()`; backend uses `(x).round()` at `discounts/handlers.rs:272` and `orders/handlers.rs:1037`). Do **not** use banker's rounding.

**(1) Line price — non-bundle** (`cart.dart` `CartItem.lineTotal`):
```
unitPrice      = priceForSize(sizeLabel)      // ABSOLUTE price_override; falls back to basePrice if label null/unknown
addonsPrice    = Σ over addons   ( addon.priceModifier * addon.quantity )
optionalsPrice = Σ over optionals( optional.price )      // absolute; isFree ⇔ price == 0
lineTotal      = (unitPrice + addonsPrice + optionalsPrice) * quantity
```
- `priceModifier` is the **charged delta computed at selection time** (`item_detail_sheet.dart` `_adjustedPrice`): for `milk_type`/`coffee_type` it is `max(addon.defaultPrice − baseSwapPrice, 0)` (swaps clamp negative → 0, never a credit); for all other addon types it is the full `defaultPrice`. **The Rust core must TRUST the wire `unit_price` per addon and NOT recompute the swap delta** — the backend records it verbatim and only re-prices from catalog when absent.

**(1b) Line price — bundle** (`bundle_detail_sheet.dart`, `cart_notifier.addBundle`):
```
unitPrice       = bundle.price                 // FIXED bundle price (bundles.price, server-stored)
componentExtras = Σ over components( Σ(addon.priceModifier*qty) + Σ(optional.price) )
lineTotal       = (bundle.price + componentExtras) * quantity
```
- **Component base/size price is NEVER charged** (component `sizeLabel` is recorded for the kitchen, priced at 0). `savingsVsComponents`/`componentListPrice` are **display-only**. Server agrees: bundle base read verbatim from `bundles.price` (`orders/handlers.rs:832`), item-level addons/optionals forced to 0 for bundle lines (`:970–979`), only `component_surcharge` (catalog addon+optional) adds on (`:774`). Server **rejects** component swaps that don't match catalog quantity exactly (`:755–760`) — the Rust core must enforce exact component quantities too.

**(2) Cart rollup** (`cart.dart:388–405`, mirrored by `discounts/handlers.rs::calc_discount` + `orders/handlers.rs:1034–1038`):
```
1. subtotal       = Σ lineTotal                                    // i32
2. discountAmount =
       if type==null OR value==0:        0
       elif type==percentage:            round(subtotal * value / 100)     // f64 mul → round
       else (fixed):                     value.clamp(0, subtotal)          // capped, ≥0
3. taxableAmount  = subtotal − discountAmount
4. taxAmount      = round(taxableAmount * taxRate)                 // taxRate decimal e.g. 0.14, EXCLUSIVE
5. total          = taxableAmount + taxAmount
```
**Order is fixed: subtotal → discount → tax-on-discounted-base → total.** Exactly two `round()` points (percentage discount, tax). Everything else is integer add/sub. `taxRate` is a **single org-wide** decimal from `/auth/me` (org `tax_rate`, default `0.14` server-side / `0.0` in legacy carts), exclusive, applied to the whole discounted subtotal — no per-item/per-category tax, single currency (EGP).

**(3) Tip / service charge:** **Tip is NOT part of `total`.** `tipAmount` + `tipPaymentMethod` are sent separately and are drawer/accounting only. **There is no service-charge concept** — do not add one. A *cash* tip is subtracted from the change/split target; a *card* tip is additive and untouched by the order total.

**(4) Change & split:**
```
changeGiven = (amountTendered − total).clamp(0, 999999)   // 0 if amountTendered == null
split target = total − cashTip                            // validation invariant only; does not alter total
paymentMethod = single method | 'mixed' (>1 split) | the lone split's method (==1)
```

### 2.3 Field send-vs-derive table (the wire contract for the port)

| Field | Client sends? | Server behavior | Recorded value |
|---|---|---|---|
| `OrderItemInput.unit_price` (per line/addon) | Yes (`Option<i32>`) | Verbatim if present; else server expected (`:967`) | client (or expected) |
| `subtotal` | Yes | `unwrap_or(Σ charged lines)` `:1034` | **client verbatim** |
| `discount_amount` | Yes | `unwrap_or(calc_discount).clamp(0,subtotal)` `:1035` | **client, only clamped** |
| `tax_amount` | Yes | `unwrap_or(round(taxable*rate))` `:1037` | **client verbatim** |
| `total_amount` | Yes | `unwrap_or(taxable+tax)` `:1038` | **client verbatim** |
| `change_given` | Yes | client, else `tendered−total` clamp≥0 `:1039` | client (or derived) |
| `discount_type`/`discount_value` | Yes | used as-is unless `discount_id` sent | client (or DB-resolved) |
| `discount_id` | Yes | **server overrides** type/value from `discounts` table (must exist+active else reject) `:530` | server-resolved |
| `tip_amount`/`tip_payment_method` | Yes | recorded; **excluded from total** | client verbatim |
| `branch_id` | Sent | **server overrides** with shift's branch `:1207` | server-authoritative |
| bundle base price | No | server-derived from `bundles.price` `:832` | server-authoritative |
| `component_surcharge` | No | server-derived from catalog `:774` | server-authoritative |
| `line_cost`/`unit_cost` | No | server-derived point-in-time `:1066` | server-authoritative |
| `price_flagged`/`price_expected_total` | No | server-derived advisory; **write-only, unread** | server, residue |

**Rust-core `pricing` module contract:** pure function `price_cart(cart, tax_rate) -> PricedBreakdown { subtotal, discount_amount, taxable, tax_amount, total, change_given, line_unit_prices[] }`; `discount_id` resolution stays a store/sync concern that feeds `type`/`value` into the same function. Send the full breakdown on every order.

---

## 3. Extraction-map addendum (extends doc 04)

| Subsystem | Current Flutter files | Target rust-core module | Stays native | Migration risk |
|---|---|---|---|---|
| **Cart + pricing engine** | `core/models/cart.dart` (money getters), `core/models/menu.dart` (`priceForSize`), `core/models/bundle.dart`, `item_detail_sheet.dart` (`_adjustedPrice`), `bundle_detail_sheet.dart` (`_extrasTotal`) | `rust-core::pricing` (pure), `rust-core::cart` (state) | — | **Med.** Byte-identical rounding (ties-away-zero), swap-clamp, bundle-base-fixed rules. Must match server to keep `price_flagged` clean. |
| **Checkout / payment / tender / split / tip** | `checkout/checkout_sheet.dart` (`_place`, `placeQueued`, `_finalizeOrder`), `cash_tendered_section.dart`, `split_payment_section.dart`, `payment_helpers.dart` | `rust-core::checkout` (validation, split-sum invariant, change calc, idempotency-key mint) | Numeric keypad UI, haptics | **High.** Unify idempotency key here; resolve 3-way change formula; split-sum reject mirrors server's only money reject. |
| **Void** | `order/void_order_sheet.dart` (`_submit`), `order_api.voidOrder`, `offline_queue.dart` (`enqueueVoid`, dependsOn) | `rust-core::orders::void` (reason model, restock flag, queue dependency gating) | Confirm dialog UI | **Med.** Default `restore_inventory` to `true` (two parse paths default `false`); gate void of `pending_sync` order regardless of connectivity. Prune dead `OrderRepository.voidOrder`. |
| **Menu / catalog (translation + branch-override resolution)** | `core/models/menu.dart`, `bundle.dart`, `menu_notifier.dart`, recipe/addon embedded data | `rust-core::menu` (override + i18n resolution, `priceForSize`, availability) | — | **Med.** Resolution rules (branch override → catalog fallback, translation pick) move to Rust. **Bug:** branch-scoped `cachedAt` key mismatch in `menu_notifier.dart` (already flagged) — fix in port. |
| **Delivery lifecycle + settings** | `delivery_orders_screen.dart`, `delivery_order_repository.dart`, `delivery_api.dart`, `delivery_settings_notifier.dart`, `delivery_settings.dart`, `delivery_order.dart` | `rust-core::delivery` (state model, settings ownership split, action dispatch) | — | **Med.** Mutations are **online-only today**; decide whether to queue them (see §4/§6). Settings 409 (can't reopen dashboard-disabled channel) surfaces to UI. |
| **Realtime / SSE** | `delivery_realtime_service.dart` (`SseFrameParser`, `_onStall`, `_scheduleReconnect`), `new_order_detector.dart`, `delivery_realtime_host.dart`, `main.dart` lifecycle | `rust-core::realtime` (SSE parse, reconnect/stall/backoff state machine, new-order dedup, gate decision) | Background socket-keepalive shim, wakelock | **High.** Background execution is platform-specific; core decides "is new," native keeps connection + posts. |
| **Notifications** | `notification_service.dart` (`flutter_local_notifications`) | `rust-core` decides *what/when* | **Native: the OS post** (Android `NotificationManager` channel/importance/`notify`, iOS `UNUserNotificationCenter` auth/banner), permission prompts | **Med.** No Rust equivalent for OS post; thin shim only. |
| **Inventory / recipe** | `recipe_api.dart` (`computeRecipeLocally`), `recipe_sheet.dart`, `order_ingredients_sheet.dart`, `order.dart` (`InventoryDeduction` parse) | `rust-core::recipe` (preview compute only) | — | **Low. CONFIRMED server-side.** POS owns no stock state; depletion is server-authoritative. Only `computeRecipeLocally` (offline preview, mirrors backend `preview_recipe`) ports. **Delete** dead `InventoryApi.items`, `loadInventoryLocal`/`fetchInventoryFresh`, `inventoryCounts` path, zero-stock l10n. |

---

## 4. Order & delivery state machines

### 4.1 Order lifecycle

Status is a bare string on `OrderFull` (no enum — the port should make it a Rust enum).

```
draft         — receipt preview only, never persisted (_previewReceipt)
pending_sync  — optimistic offline order; id = localId (UUID), orderNumber = -1
pending/active— server-confirmed (server status + real orderNumber)
voided        — isVoided == true
```

| Transition | Mutating? | Offline / idempotency need |
|---|---|---|
| create (online) | Yes | `POST /orders` + `Idempotency-Key`. Backend pre-checks key → replays existing order (200, not 409) before any depletion. |
| create (offline) | Yes | `enqueueOrder` → SQLite outbox, optimistic `pending_sync` row added to history. On confirm, `onOrderSynced(order, localId)` → `replaceOrder(localId, order)` swaps by `id == localId`. |
| void (online) | Yes | `POST /orders/{id}/void`; guarded `WHERE status<>'voided'` → idempotent restock. **Gap:** voiding a `pending_sync` order online sends its local UUID → 404 (server doesn't know it). |
| void (offline) | Yes | `enqueueVoid` sets `dependsOn` = order's outbox `localId` so void never precedes its create; dead-create → `OfflineVoidBlockedError`. FIFO drain by `created_at ASC`. |
| cash movement | Yes | **Online-only by design** (no backend idempotency key); `OfflineCashMovementError`. UI must disable offline. |

### 4.2 Delivery lifecycle

```
received → confirmed → preparing → ready → out_for_delivery → delivered(terminal, via finalize only)
   └─(cancel)→ cancelled(terminal)    └─(cancel,isReject)→ rejected(terminal)
```
`nextForward` is the single plain step; `delivered` only via `POST /finalize`; `cancelled`/`rejected` only via `POST /cancel`. A `delivery_orders` row exists from intake; **no `orders` row until finalize**.

| Transition | Endpoint / fn | Mutating? | Idempotent today? | Offline need |
|---|---|---|---|---|
| intake → received | `public.rs create_delivery_order` | Yes (creates row) | **Yes** — `Idempotency-Key` + partial unique index `uq_delivery_orders_idem` | n/a (customer-facing) |
| accept (→confirmed) | `staff.rs set_status` + POS prints receipt once (`receiptPrintedAt`) | Yes — bare `UPDATE`, no tx/CAS | Data-idempotent (absolute set) but **WhatsApp side effect** on forward jump | Online-only today. Client must suppress re-issuing same-target transitions. |
| advance (preparing/ready/out) | `staff.rs set_status` | Yes — bare `UPDATE` | Same as above | Online-only. |
| prep-time (±5) | `staff.rs set_prep_time` | Yes — bare `UPDATE` | **Idempotent** (absolute set) | Online-only; safe to replay. |
| cancel / reject (+waste) | `staff.rs cancel_delivery_order` | Yes — tx + CAS `WHERE status NOT IN (terminal)` | **Yes** — winner deducts waste once; retry → 409 | Treat **409 as success** on replay. |
| finalize | `staff.rs finalize_delivery_order` | Yes — tx + `FOR UPDATE` CAS on `order_id` + per-shift advisory lock | **Yes** — replay → **409 (not a replayed result)** | Treat **409 as success**; reuse same `shift_id`; requires open shift. |

**Key state-machine gap:** `set_status`/`set_prep_time` have **no tx, no CAS, no idempotency key** — naturally data-idempotent (absolute writes, no ledger) but `set_status` carries a **non-idempotent WhatsApp send** and a **lost-update risk** (two managers / offline replay racing a live edit, last-write-wins). All delivery mutations are **online-only** in the POS today (zero entries in `offline_queue.dart`).

---

## 5. Stock-depletion replay safety

**Verdict: SAFE for order-create; SAFE for delivery finalize; SAFE for void/cancel. No client-side ledger to corrupt.**

- **Order create depletes server-side, atomically.** Single tx (`orders/handlers.rs:1093`→commit `:1575`); `UPDATE branch_inventory SET current_stock = current_stock - $1` (`:1517`) + `inventory_movements` (`sale`, `source_type='order'`) run on the same `&mut *tx`. Negative stock is allowed-but-flagged (`below_zero` on the movement); untracked ingredients soft-fail.
- **Idempotent for replayed offline orders ✅** — provided a stable key. Pre-check: existing key → replay order, return **before** depletion (`:475`). Race fallback: `orders.idempotency_key` partial unique index → 23505 caught → tx rolled back → committed order replayed (`:1238`). The client passes the stable `action.localId` on the queued path (`offline_queue.dart:502`), so **a queued order replayed N times deducts exactly once.**
- **Delivery finalize** is replay-safe by a different mechanism — `FOR UPDATE` CAS on `order_id` returns **409** (not a replay) → client must treat 409 as already-applied.
- **Void restock / delivery-cancel waste** are idempotent via guarded CAS (`WHERE status<>'voided'` / `WHERE status NOT IN (terminal)`).
- **POS keeps no local stock.** No provisional decrement offline; the inventory-fetch path is dead code. So there is **no double-deduction risk to port** and no stale-stock counter in the UI.

**The one residual double-charge risk is NOT depletion — it's the order-create idempotency key itself** (§1.3, §6). Because depletion is keyed off that same key, the dual-key bug means a lost-response online commit + queued-UUID retry creates **two orders and deducts stock twice**. Fixing the key (§6) closes both the double-charge and the double-deduction.

---

## 6. Fix-in-port list (concrete correctness bugs)

| # | Bug | Evidence | Fix in port |
|---|---|---|---|
| **F1 (P0)** | **Dual order-idempotency-key → double order + double depletion** on lost-response retry. Online uses `state.id` (defaults to literal `"order_1"`); queued mints fresh `Uuid().v4()`. | `cart_notifier.dart:262`, `checkout_sheet.dart:456/480`, `offline_queue.dart:502` | **Mint one UUID per logical placement BEFORE the online attempt**; use it as `Idempotency-Key` on both the direct `create` and the queued `PendingOrder`. Lost-response retry then replays the same key → server dedupes (200). |
| **F2 (P0)** | **Non-unique default cart id `"order_1"`** can dedupe two genuinely-distinct orders into one if `startNewOrder` wasn't called between them. | `cart.dart` `_defaultCart`, `cart_notifier.dart:262` | Per-order UUID from F1 eliminates this; never derive the key from a reusable tab id. |
| **F3 (P1)** | **Void of a `pending_sync` order while online → 404** (sends local UUID the server doesn't know). | `void_order_sheet.dart:150` (online branch bypasses queue dependency logic) | Gate/force-queue void when `status == 'pending_sync'` **regardless of connectivity**; reuse `OfflineVoidBlockedError` UX + the create-dependency. |
| **F4 (P1)** | **Three change-due formulas diverge with a cash tip.** UI shows `tendered−total−cashTip`; receipt + DB record `tendered−total`. | `cart.dart:407`, `checkout_sheet.dart:722`, `cash_tendered_section.dart:87` | One definition in `rust-core::checkout`; all three read it. (Recommend the cart model subtract cash tip to match what the teller sees.) |
| **F5 (P1)** | **`restore_inventory` defaults to `false`** in two parse/repo paths while UI intent is `true`. | `order_repository.dart:100`, `pending_action.dart:344` | Default `true` (or make non-nullable) in the Rust model. |
| **F6 (P1)** | **SSE has no replay cursor** — events during any disconnect window are lost from the stream; recovered only by full re-GET (≤30s alert delay) or poll. | `delivery_api.dart` (only `branch_id`/`status` params); backend `hub.rs` 128-ring, no event id | Client side: keep the re-GET-on-reconnect reconcile. Real fix is backend (§7-B1). |
| **F7 (P2)** | **No background alerting** — socket dropped on `pause`, no push; backgrounded teller never alerts. | `main.dart:137`, no FCM/APNs in codebase | Rust-core detection + native background service, or add push (§7-B2). |
| **F8 (P2)** | **Percentage discount is uncapped** — value > 100 yields `discountAmount > subtotal` → **negative total** (only *fixed* discounts clamp). Confirm intended; backend `calc_discount` clamps `[0, subtotal]`, so server would *flag* (not reject) the mismatch. | `cart.dart:392–396` vs `discounts/handlers.rs:276` | Clamp percentage discount to subtotal in the Rust core to match server `calc_discount` and avoid negative totals / false flags. **(Note: zero-tax `rate==0 ⇒ total==taxable` and 100%-discount `⇒ total==0` are already correct — preserve them.)** |
| **F9 (P2)** | **Recipe-preview logic duplicated** between `computeRecipeLocally` and backend `preview_recipe`; drift → preview disagrees with actual deduction. | `recipe_api.dart:73–218` | Single source in `rust-core::recipe`; keep offline preview but one implementation. |
| **F10 (P2)** | **Dead code** to drop: `OrderRepository.voidOrder` (sheet bypasses it), `InventoryApi.items`, `loadInventoryLocal`/`fetchInventoryFresh`, `inventoryCounts` path, zero-stock l10n, `menu_notifier.dart` branch-scoped `cachedAt` key mismatch. | inventory/menu audits | Don't port; fix the `cachedAt` key. |

---

## 7. Backend additions (NEW beyond doc 04)

> Doc 04 already lists offline-support backend work. The items below are **new or refined** from the selling-core/inventory/delivery audits.

**B1 (P1) — SSE replay cursor + durable, multi-instance hub.** Add a monotonic event sequence id to `DeliveryEvent`, honor `Last-Event-ID` on `GET /delivery-orders/stream`, and add an `updated_since` / `after_seq` query param to the list endpoint so reconnects fetch only the delta (today: `created_at DESC`, no `updated_at` cursor, default 200-row window can theoretically drop a changed old order). Back the in-process `tokio::broadcast` hub (128-ring, single-instance) with Postgres `LISTEN/NOTIFY` for durability across restarts and horizontal scaling. *Refines doc 04's offline-support — this is the realtime catch-up gap.*

**B2 (P2) — Real push (FCM/APNs) for backgrounded tellers.** There is no push token registration anywhere. Without it (or a Rust background service), no order can alert while the app is backgrounded/killed. Add device-token registration + server-initiated push on new delivery order.

**B3 (P1) — Idempotency on `set_status` / `set_prep_time`, and de-dup the WhatsApp side effect.** These two delivery transitions have no tx/CAS/key and a non-idempotent WhatsApp send + lost-update risk. *Required before delivery mutations can ever be queued offline.* Add a client-token + row lock; gate the notification on an actual step crossing recorded server-side, not on the client not re-issuing.

**B4 (P2) — Idempotency keys on `create_waste`, `create_transfer`, `cash_movement`, `finalize_stocktake`.** These are concurrency-safe (FOR UPDATE) but **not replay-safe** — each retry double-applies the stock/cash delta. Latent today (POS doesn't queue them) but a hard prerequisite if the offline-first mandate ever extends to them. Mirror the `orders.idempotency_key` partial-unique-index pattern. *Refines the cash-movement note in doc 04.*

**B5 (P2) — Make finalize/cancel 409 a documented "already-applied" success contract.** Delivery finalize/cancel return 409 on replay (not a replayed result). Document this so the offline client treats 409-on-finalize/cancel as success, not error. (Order-create correctly replays 200 — keep that asymmetry documented.)

**B6 (P3, optional) — Surface `price_flagged` somewhere.** Today it's write-only audit residue with no reader. If the rebuild wants client-authoritative pricing to be *auditable*, add a reconciliation report/filter on `price_flagged` / `price_expected_total`. Otherwise, formally drop the columns. (Not required for correctness — the POS is the source of truth by design.)

---

## 8. Impact on PLAN.md

**Module sequencing (rust-core):**

1. **`store` / `sync` / `outbox` first** (doc 04 foundation) — the idempotency key, SQLite outbox, and replace-by-localId all live here. **F1/F2 (unified per-order idempotency key) must land in this layer**, because both pricing and depletion safety hinge on it.
2. **`pricing` immediately after `store`, before `checkout`.** Pricing is a **pure function** with no I/O — it's the lowest-risk, highest-leverage module and the de-facto source of truth for all money. Build and exhaustively unit-test it (rounding ties-away-zero, swap-clamp, bundle-base-fixed, F8 percentage clamp, zero-tax, 100%-discount) against the backend `calc_discount` + create formula as golden vectors. **This is the spec in §2.2.**
3. **`cart` + `menu` (override/i18n resolution) alongside `pricing`** — `pricing` consumes resolved menu prices, so `menu::priceForSize`/override resolution must exist first or in parallel. Fix the `cachedAt` key (F10) here.
4. **`checkout` after `pricing` + `cart` + `outbox`** — validation, split-sum invariant (the server's only money reject), 3-way change unification (F4), idempotency mint (F1). Pairs with the **printer-over-TCP (Epson + Star in Rust)** module since checkout's tail prints the receipt.
5. **`orders::void` after `checkout`** — restock-default fix (F5), pending_sync gating (F3), queue dependency.
6. **`realtime` + `delivery` last in the selling arc** — highest platform-coupling (background socket, OS notification post). Build the pure SSE-parse/reconnect/dedup state machine in Rust; defer background execution + push (B1/B2/B7) to a later phase. Delivery mutations can stay **online-only** initially (matches today's behavior); only queue them after B3 lands.
7. **`recipe` (preview only)** — small, late, optional; consolidates `computeRecipeLocally` (F9). **No inventory-authority module needed** — confirmed server-side.

**UI module breakdown — confirmed.** Thin SwiftUI/Compose surfaces over the Rust core, with these platform-only pieces staying native (everything else is a thin shim calling the core):
- **OS notification post** (Android `NotificationManager`, iOS `UNUserNotificationCenter`) + permission prompts.
- **Printer hardware discovery/pairing UI** (transport itself is Rust-over-TCP).
- **Background socket-keepalive + wakelock** shim.
- **Numeric keypad / haptics / receipt rendering surface.**

**One-drawer-per-branch** simplifies the checkout/cash module (single `addLocalCash` target, `loadSystemCash` reconcile) — no per-teller drawer reconciliation logic to port. **No service-charge** concept to model (§2.3). **Single currency, single tax rate** — keep the `pricing` module's tax as one org-wide exclusive rate; do not over-generalize to per-item tax.

---

### Load-bearing file references
- Pricing (client): `cart.dart:388–410`, `cart_notifier.dart:262`, `menu.dart:22–28`, `item_detail_sheet.dart:394–401`, `bundle_detail_sheet.dart:140–147`
- Pricing (server): `orders/handlers.rs:1034–1058`, `discounts/handlers.rs:270–277`, `bundles/handlers.rs:355–387`, migration `20260614120000_branch_menu_overrides.sql:31–36`
- Checkout/void: `checkout_sheet.dart` (`_place` 354–574, `placeQueued` 478–524, `_finalizeOrder` 693–724), `void_order_sheet.dart:108–190`, `order_api.dart:52–60`
- Idempotency/queue: `offline_queue.dart:502` (key), `:199–216` (void dependsOn), `:351–386` (404/409), `main.dart:85` (onOrderSynced)
- Depletion (server): `orders/handlers.rs:1093/1517/1546/1575`, `:475/1238` (idempotency), `delivery/staff.rs:475/554`, partial unique indexes `20260531200000_full_schema.sql:1843`, `20260614150000_delivery_core.sql:140`
- Delivery/realtime: `delivery_realtime_service.dart`, `new_order_detector.dart`, `notification_service.dart`, `delivery_api.dart:128`, `delivery_order.dart:88`, `hub.rs:38/62`, `staff.rs:188/291/377/475/631`
- Inventory/recipe: `recipe_api.dart:73–218`, `order.dart:54`, dead `inventory_api.dart` / `shift_repository.dart`