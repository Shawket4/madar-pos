I now have the complete picture, including the dual-scheme idempotency bug. The online path mints `idempotencyKey = state.id` (e.g. `"order_1"`, stable per tab), while the queued path uses `idempotencyKey: action.localId` (a fresh `Uuid().v4()` minted in `placeQueued()`). Let me confirm the tip is excluded from `total` and verify what the backend trusts vs. recomputes by checking the `tip_amount` handling and that `total_amount` excludes tip.

The key observations are confirmed. Let me produce the final spec.

---

# Cart Pricing Engine — Exact Algorithm Spec (for byte-identical Rust re-implementation)

All money is **integer piastres** (1 EGP = 100 piastres). No floats are ever stored; floats appear only transiently in two `.round()` operations (discount %, tax). `round()` is Dart's `double.round()` = **round half away from zero** (banker's rounding is NOT used; `2.5 → 3`, `-2.5 → -3`). Rust must replicate this, not `f64::round`'s ties-to-even-free behavior… actually Dart `round()` is ties-away-from-zero, matching Rust `f64::round()`. Use `(x).round()` semantics = ties away from zero.

## 1. Line-item price

Source: `cart.dart` `CartItem.lineTotal` (lines 224-236), `menu.dart` `priceForSize`, `item_detail_sheet.dart` `_adjustedPrice`/`_addonsTotal`.

### 1a. Non-bundle line
```
addonsPrice    = Σ over addons   ( addon.priceModifier * addon.quantity )
optionalsPrice = Σ over optionals( optional.price )
lineTotal      = (unitPrice + addonsPrice + optionalsPrice) * quantity
```
- `unitPrice` = `priceForSize(sizeLabel)`:
  ```
  priceForSize(label):
    if label == null OR sizes is empty: return basePrice
    for s in sizes: if s.label == label: return s.priceOverride   # NOT basePrice + delta
    return basePrice                                              # label not found → basePrice
  ```
  **Size price is absolute (`price_override`), never an additive delta.**
- `optional.price` is the OptionalField's absolute price; `isFree ⇔ price == 0`.
- **Addon `priceModifier` is the CHARGED delta, computed client-side at selection time** via `_adjustedPrice` (item_detail_sheet.dart 394-401):
  ```
  adjustedPrice(addon):
    if addon.addonType in {'milk_type','coffee_type'}:
        base = baseSwapPrices[addonType]   # defaultPrice of the item's default milk/coffee addon, else 0
        diff = addon.defaultPrice - base
        return diff > 0 ? diff : 0         # swaps clamp negative to 0 (free downgrade, never a credit)
    else:
        return addon.defaultPrice          # extras charge full catalog price
  ```
  This means **for milk/coffee, the stored `priceModifier` is a swap delta vs. the item's default**, clamped at 0. For all other addon types it's the full `defaultPrice`. The backend receives this in `unit_price` per addon and **records it verbatim** (`cart.dart` 56-62: "absent → it re-prices from the catalog"). Rust must NOT recompute the swap delta — it trusts `unit_price`.

### 1b. Bundle line
```
componentExtras = Σ over bundleComponents( c.addonsPrice + c.optionalsPrice )
                  where c.addonsPrice    = Σ( addon.priceModifier * addon.quantity )
                        c.optionalsPrice = Σ( optional.price )
lineTotal       = (unitPrice + componentExtras) * quantity
```
- `unitPrice` for a bundle line = **`bundle.price` (fixed bundle price)**, set in `addBundle` (cart_notifier.dart 118-124). Component base prices are **NOT** summed into the charge.
- **Bundle component base/size price is NEVER charged.** Only component **addons + optionals** add on top of the fixed bundle price. A component's `sizeLabel` is recorded for the kitchen but contributes **0** to money (no `priceForSize` is applied to components — `BundleComponentSnapshot` has no `unitPrice` field).
- `savingsVsComponents` / `componentListPrice` (bundle.dart 55-67) are **display-only** (savings chip); they never enter the charged total.
- Wire (`cart.dart` 238-252): bundle line sends both `bundle_unit_price` and `unit_price` = `bundle.price`; `addons`/`optional_field_ids` at the line level are forced empty `[]`; extras live inside each `bundle_components[i]`.

## 2. Cart-level rollup (subtotal → discount → tax → total)

Source: `cart.dart` `CartState` getters (lines 386-410).

```
subtotal       = Σ over items( item.lineTotal )                      # integer

discountAmount =
    if discountType == null OR (discountValue ?? 0) == 0:  0
    elif discountType == percentage:  round( subtotal * discountValue / 100 )   # double mul, then round
    else (fixed):                     discountValue.clamp(0, subtotal)          # capped at subtotal, never negative

taxableAmount  = subtotal - discountAmount

taxAmount      = round( taxableAmount * taxRate )    # taxRate is a decimal e.g. 0.14; EXCLUSIVE (added on top)

total          = taxableAmount + taxAmount           # == subtotal - discountAmount + taxAmount
```

**Order of operations (fixed, must preserve):**
1. Sum line totals → `subtotal`.
2. Apply discount to `subtotal` → `discountAmount`, then `taxableAmount = subtotal − discountAmount`.
3. Apply tax to the **discounted** base: `taxAmount = round(taxableAmount * taxRate)`.
4. `total = taxableAmount + taxAmount`.

**Rounding points (exactly two):**
- `round(subtotal * discountValue / 100)` for percentage discounts.
- `round(taxableAmount * taxRate)` for tax.
- Everything else is integer addition/subtraction. No intermediate rounding inside line items (multiplication by integer quantity only).

**Tax model:** `taxRate` is a **decimal fraction** (0.14 = 14%), tax is **EXCLUSIVE** (added on top of the discounted subtotal). `taxRate` defaults to `0.0` ⇒ tax-free total (legacy). It is **session config from `/auth/me`**, never persisted with the cart, re-injected on every load (`cart_notifier.dart` 51-60, 233, 241, 251). Backend computes `round((subtotal - discount) * tax_rate)` at order time — comment at `cart.dart` 402-403 states this is the contract to match.

**Tip / service charge:** Tip is **NOT** part of `total`. `tipAmount` is sent separately (`tip_amount`) and is purely drawer/accounting (`changeGiven` and split math subtract a *cash* tip but the order total never includes it). There is **no service charge** concept in the cart engine.

```
changeGiven =
    if amountTendered == null:  0
    else: (amountTendered - total).clamp(0, 999999)   # never negative, capped at 999999 piastres
```

**Split payment** (split_payment_section.dart): the entered split amounts must sum to `cartTotal - cashTip` (a cash tip is taken out of the split target). This is a **validation invariant only** — it does not alter `total`. `payment_method` becomes the single method if one split, else `'mixed'`.

**Charged breakdown sent to backend** (`order_api.dart` 52-56, optional fields): `subtotal`, `discount_amount`, `tax_amount`, `total_amount`, `change_given`. These mirror the getters above and are **recorded verbatim**; the catalog is used only to flag deviations, never to reject. **Whatever these getters compute IS the money.**

## 3. Idempotency key derivation (and the dual-scheme bug)

`cart_notifier.dart` 261-262:
```dart
String idempotencyKey() => state.id ?? 'order_${TimeUtils.now().millisecondsSinceEpoch}';
```
- **Minted from `state.id`**, which is the cart/tab id, e.g. `"order_1"` (default, `_defaultCart` line 16) or `"order_<epochMs>"` (`startNewOrder` line 248). It is **stable for the lifetime of the tab** until a new order is started/cleared (`clear()` keeps the same `id`; `startNewOrder` mints a fresh one).
- Sent as HTTP header `Idempotency-Key` (`order_api.dart` 60).

**Dual-scheme bug (confirmed):**
- **Online path** (`checkout_sheet.dart` 456 → 537): uses `cartProvider.notifier.idempotencyKey()` = `state.id` (e.g. `"order_1"`).
- **Queued/offline path** (`checkout_sheet.dart` 478-504): `placeQueued()` mints `final localId = const Uuid().v4();` and the offline drain (`offline_queue.dart` 502) sends `idempotencyKey: action.localId` — **a fresh random UUID, NOT `state.id`**.

Consequence: the same logical order can carry **two different idempotency keys** depending on whether it went out online vs. got queued. The fallback at `checkout_sheet.dart` 553-555 catches a mid-flight network error after an online attempt and re-routes to `placeQueued()` — at that point the online attempt may have already reached the server under key `"order_1"`, but the queued retry uses the UUID. **The server cannot dedupe these two as the same order via the idempotency key.** (The 409 handling in `offline_queue.dart` 360-382 is the only backstop, and it explicitly notes an idempotency replay returns 200 not 409 — so a successful-but-unacknowledged online create followed by a queued UUID retry produces a **double order / double charge** with no 409 to catch it.)

Additional collision risk on the online side: `state.id` defaults to the literal `"order_1"` for the default cart, so two separate orders placed from a fresh tab without `startNewOrder` in between could **reuse `"order_1"`** and be deduped into one by the server.

## 4. Edge cases (must preserve exactly)

| Case | Behavior | Source |
|---|---|---|
| **Zero tax** (`taxRate == 0.0`) | `taxAmount = 0`, `total = taxableAmount`. Default for carts built without session config. | cart.dart 383, 403 |
| **100% / over discount (percentage)** | `round(subtotal * value/100)`; at 100% → `discountAmount == subtotal`, `taxableAmount == 0`, `taxAmount == 0`, `total == 0`. Percentage is **not clamped** — a value > 100 yields `discountAmount > subtotal`, making `taxableAmount` (and `total`) **negative**. Only *fixed* discounts clamp. | cart.dart 392-396 |
| **Fixed discount > subtotal** | `discountValue.clamp(0, subtotal)` → capped at subtotal; `total` floors at 0 (tax of 0). Negative fixed values clamp to 0. | cart.dart 395 |
| **Discount with value 0 or type null** | `discountAmount = 0` (guard at 391). `setDiscount(null,_)` and `applyCheckoutFields` clear both type+value+id. | cart.dart 390-391; cart_notifier 173-176, 221 |
| **Free bundle component / free addon / free optional** | `priceModifier`/`price == 0` contribute 0; bundle base is `bundle.price` regardless. A free milk swap (downgrade) clamps to `0` (no credit). | item_detail_sheet 398; cart.dart 97-100 |
| **Milk/coffee swap as a credit** | Never. `_adjustedPrice` clamps negative diff to 0 — swapping to a cheaper milk does not reduce the charge. | item_detail_sheet 398-399 |
| **Unknown size label** | `priceForSize` falls back to `basePrice`, not an error. | menu.dart 22-28 |
| **Bundle component size** | Recorded for kitchen, **priced at 0** (never `priceForSize`). | cart.dart 88-100 (no unitPrice field) |
| **`discount_id` (catalog) vs ad-hoc** | `setDiscountById` sets `discountId` + type + value; ad-hoc `setDiscount` clears `discountId`. The *amount math is identical either way* — `discountId` is just provenance. The selected catalog `Discount.value`/`dtype` override cart values at checkout (`checkout_sheet.dart` 429-439). | cart_notifier 173-180; checkout_sheet 429-439 |
| **`amountTendered == null`** | `changeGiven = 0` (e.g. card/split). | cart.dart 407-409 |
| **Tip never in total** | `total` excludes tip; split target is `total − cashTip`. | cart.dart 405; split_payment_section 115 |
| **Per-currency** | Single currency (EGP); no multi-currency logic anywhere. Formatting only divides by 100. | formatting.dart 5-8 |
| **Per-tax-rate** | Single org-wide `taxRate`; no per-item/per-category tax. Tax always on the whole discounted subtotal. | cart.dart 367, 403 |

## Invariant checklist for the Rust port
1. `lineTotal = (unitPrice + Σ addonModifier·qty + Σ optionalPrice) · quantity` for items; `(bundle.price + Σ component(addons+optionals)) · quantity` for bundles.
2. Size price is absolute `price_override`; bundle component base price is never charged.
3. Addon swap deltas (milk/coffee) clamp negative→0; other addons charge full `default_price`. **Trust the wire `unit_price`; do not recompute.**
4. `subtotal` (int) → `discountAmount` → `taxableAmount = subtotal − discount` → `taxAmount = round(taxable · rate)` → `total = taxable + tax`. Exactly two `round()` calls (ties away from zero).
5. Percentage discount is uncapped (can exceed subtotal → negative total); fixed discount clamps `[0, subtotal]`.
6. Tax is exclusive, rate is a decimal, single rate, applied to discounted base; `rate==0` ⇒ tax-free.
7. Tip and service charge are excluded from `total`.
8. `changeGiven = clamp(tendered − total, 0, 999999)`; `0` when tendered is null.
9. Idempotency: online uses `state.id` (stable per tab, **defaults to non-unique `"order_1"`**); queued uses a fresh `Uuid().v4()`. **These two schemes do not match — a network-error fallback can double-charge.** This must be unified to one stable per-order key before the backend can rely on idempotency dedupe.

### Relevant file paths
- `/Users/shawket/Desktop/sufrix_pos/lib/core/models/cart.dart` — line-item, subtotal/discount/tax/total getters (the canonical money math)
- `/Users/shawket/Desktop/sufrix_pos/lib/core/providers/cart_notifier.dart` — `idempotencyKey()` (line 262), discount/tip/split setters
- `/Users/shawket/Desktop/sufrix_pos/lib/core/models/menu.dart` — `priceForSize` (lines 22-28), `isFree`
- `/Users/shawket/Desktop/sufrix_pos/lib/core/models/bundle.dart` — fixed `bundle.price`, display-only savings
- `/Users/shawket/Desktop/sufrix_pos/lib/features/order/widgets/item_detail_sheet.dart` — `_adjustedPrice` swap-clamp (394-401), addon/optional totals
- `/Users/shawket/Desktop/sufrix_pos/lib/features/order/widgets/bundle_detail_sheet.dart` — `_extrasTotal` (140-147), bundle line assembly
- `/Users/shawket/Desktop/sufrix_pos/lib/features/order/checkout/checkout_sheet.dart` — online idempotency mint (456), queued UUID mint (478-504)
- `/Users/shawket/Desktop/sufrix_pos/lib/core/services/offline_queue.dart` — queued create uses `action.localId` (502); 409 dedupe gap (360-382)
- `/Users/shawket/Desktop/sufrix_pos/lib/core/api/order_api.dart` — charged breakdown fields + `Idempotency-Key` header (52-60)