I now have a complete picture. The void button is shown for any non-voided order, including `pending_sync` (offline-created) orders — the only guard is the queue-side `enqueueVoid` dependency logic and the `OfflineVoidBlockedError` for dead orders. Let me confirm one final detail: whether a `pending_sync` order whose creation is still queued can be voided **while online** (which would hit the server with a local UUID id that doesn't exist → 404). This is the offline gap. I have enough to write the audit.

Here is the complete audit.

---

# Sufrix POS — Checkout / Payment / Void Audit

Scope: `lib/features/order/checkout/**`, `void_order_sheet.dart`, `pending_orders_screen.dart`, `core/repositories/order_repository.dart`, `core/providers/order_history_notifier.dart`, `core/models/{order,cart,pending_action}.dart`, plus the load-bearing `core/services/offline_queue.dart`, `core/api/order_api.dart`, `core/providers/cart_notifier.dart`, `main.dart`.

All money is in **piastres** (int, EGP×100) throughout.

---

## 1. Checkout → Submit Flow

### Entry & validation (`checkout_sheet.dart` `_place`, lines 354–574)
1. Reads cart, shift, online flag, payment methods. Guards: cart non-empty, open shift exists, payment method selected (unless split).
2. **Tip**: parsed from `_tipCtrl` (`_parsedTip`, line 228); `tipMethod` only set if tip > 0.
3. **Cash tendered** (single-payment cash only): parsed from `_tenderedCtrl`. Validates `tendered != null/0`, `tendered >= cart.total`, and if the tip is cash, `tip <= (tendered - total)` (the change).
4. **Split**: validates ≥1 method selected, amounts entered, and `sum(splits) == cart.total - (cashTip)`. Note the split target subtracts a cash tip but **not** a card tip — consistent with the cash-tendered branch.
5. `paymentMethod` resolves to a single method, or `'mixed'` if >1 split, or the single split's own method if exactly one.

### Drawer cash computation (lines 458–469)
`cashAdded` = sum of cash splits, OR `cart.total` if single cash method, **plus** the tip if the tip method is cash. This is what's later handed to `shiftProvider.addLocalCash`.

### Change computation — three different formulas (inconsistency)
- **Cart model** `changeGiven` (cart.dart:407): `(amountTendered - total).clamp(0, 999999)` — **ignores cash tip**.
- **Receipt** (`_finalizeOrder`, line 722): `(tendered - total).clamp(0, 999999)` — **ignores cash tip**.
- **UI readout** (`cash_tendered_section.dart:87`): `tendered - cartTotal - _cashTip` — **subtracts the cash tip**.

So the live "change due" the teller sees during entry differs from the change printed on the receipt whenever there's a cash tip. The persisted `change_given` (sent on the queued path, pending_action.dart) is the cart's tip-ignoring value.

### Online path (lines 532–552)
`orderRepositoryProvider.create(... idempotencyKey: idempotencyKey ...)` → `OrderApi.create` POSTs `/orders` with header `Idempotency-Key`. On success → `_finalizeOrder`.

### Offline / queued path (`placeQueued`, lines 478–524)
Triggered when `offlineMode` (`!isOnline || authProvider.isOfflineSession`) **or** when the online attempt throws and `isNetworkError(e)` is true (line 554). It:
1. Mints `localId = Uuid().v4()`.
2. `queue.enqueueOrder(PendingOrder(... localId ...))` — persists to the SQLite outbox.
3. Builds an **optimistic Order** (`_buildOptimisticOrder`, 577–617): `id = localId`, `status = 'pending_sync'`, `orderNumber = -1`, items get fresh UUIDs, server-computed fields (name translations, costing, deductions) stubbed.
4. `_finalizeOrder` with the optimistic order.

### `_finalizeOrder` (shared tail, 693–724)
`Haptics.success()` → **await** `orderHistoryProvider.addOrder` (durable cache **before** the receipt) → clear cart → if `cashAdded>0` bump local drawer cash + `loadSystemCash()` → dismiss sheet (and promote oldest draft / pop cart sheet) → show `ReceiptSheet`. Errors anywhere release the button in `finally` (line 572).

### Charged-breakdown verbatim
Both `OrderApi.create` and `PendingOrder` carry `subtotal/discount_amount/tax_amount/total_amount/change_given` so the DB equals the printed receipt even if the order was priced from a stale/offline menu (backend records verbatim, only flags drift). Item-level `unit_price` is likewise sent verbatim.

---

## 2. Payment / Tender Model

| Concept | Field(s) | Notes |
|---|---|---|
| Single tender | `cart.payment` (wireFormat string) | `'cash'`, `'card'`, dashboard-configured methods |
| Split tender | `List<PaymentSplit>{method, amount}` | `paymentMethod` becomes `'mixed'` (or the lone method if exactly one) |
| Cash detection | `isCashMethod(methods, m)` (`payment_helpers.dart:45`) | Looks up the `PaymentMethod.isCash` flag; falls back to stubs for `cash`/`talabat_cash`/`card` |
| Cash tendered | `amountTendered` (int piastres) | Only for single-method cash; cleared/null otherwise |
| Change | `changeGiven` | See the three-formula inconsistency above |
| Tip | `tipAmount` + `tipPaymentMethod` | Independent of the order's payment method; can be cash or card |
| Tax | `cart.taxAmount` = `round(taxable * taxRate)` | `taxRate` injected from session, not persisted |

- `'mixed'` is a synthetic method (`paymentMethodMixed()`); it's filtered out of the selectable grids (`m.wireFormat != 'mixed'`).
- A cash tip is taken **out of the change / split target**; a card tip is additive.
- The queue has a normalization hotfix (`offline_queue.dart:506–512`): if the server echoes empty/`'mixed'` payment method on create but the action wasn't actually mixed, it overrides with the local method.

---

## 3. Order Lifecycle / State Machine

Status is a bare string on `OrderFull` (no enum). Observed values:

```
draft           — receipt preview only (never persisted); _previewReceipt builds it
pending_sync    — optimistic local order, queued offline, orderNumber = -1, id = localId(UUID)
pending/active  — server-confirmed (server-assigned status + real orderNumber)
voided          — order.isVoided == true; struck through, void reason shown
```

Transitions:
- **create (online)** → server order replaces nothing (added with server id).
- **create (offline)** → `pending_sync` optimistic order added to history with `id = localId`.
- **sync confirm** → `onOrderSynced(order, localId)` (main.dart:85) → `history.replaceOrder(localId, order)` swaps the optimistic row for the real one **by matching `id == localId`**, then refreshes `systemCash`.
- **void** → status `'voided'`.

### Void flow (`void_order_sheet.dart`)
1. Pick reason (`customer_request` / `wrong_order` / `quality_issue` / `other`+free text). `restore` (restock) toggle defaults **true**.
2. Always passes through `ConfirmSheet` (irreversible).
3. **Online**: `orderApiProvider.voidOrder(id, reason, note, restoreInventory, voidedAt)` → `POST /orders/{id}/void`. Void endpoint returns a **bare order without items**, so the API defaults `items: []` (order_api.dart:145). `onVoided(updated)` → `history.updateOrder`.
4. **Offline**: `enqueueVoid(PendingVoidOrder{...})` → optimistic `order.copyWith(status:'voided', voidReason:reason)` → `onVoided`.

### Restock
Carried end-to-end as `restore_inventory` (default true in the UI, but **false** in two defaulting code paths: `OrderRepository.voidOrder` default `false` line 100, and `PendingVoidOrder.fromJson` fallback `false` line 344). The actual restock decision is the backend's based on the flag sent; the UI always sends an explicit value, so the `false` defaults only bite if a payload is missing the key. Note `OrderRepository.voidOrder` is **dead code** for the POS void path — the sheet calls `orderApiProvider.voidOrder` directly, not the repository.

### Void ordering guarantees (queue)
- `enqueueVoid` sets `dependsOn` to the order's outbox `localId` if that order is still queued, so a void can never reach the server before its own create (offline_queue.dart:199–216).
- If the target order's create is already `dead`, the void is rejected up front with `OfflineVoidBlockedError`.
- Drain is FIFO by `created_at ASC` (`dueForSync`), with explicit prerequisite gating.
- Shift-close waits for all live orders/voids (`hasLiveOrdersOrVoids`).

---

## 4. Idempotency — End to End, and the Dual-Key Bug

### Two different keys for the same logical order
- **Online path** (`checkout_sheet.dart:456`): `idempotencyKey = cartProvider.notifier.idempotencyKey()` → `state.id ?? 'order_<ms>'` (`cart_notifier.dart:262`). The cart **tab id**, e.g. `order_1718800000000`, stable per tab until `startNewOrder`.
- **Offline / queued path** (`checkout_sheet.dart:480` + `offline_queue.dart:502`): `localId = Uuid().v4()`, and the drain sends `idempotencyKey: action.localId` — a **brand-new random UUID**, unrelated to the cart id.

### The bug
The fallback comment at lines 471–477 claims the server's idempotency handling prevents a double charge when an online attempt dies mid-flight and the queued retry reuses "the same code path". **It does not, because the two code paths use different idempotency keys.**

Concrete double-charge scenario:
1. Teller taps Place Order while online. `OrderApi.create` POSTs `/orders` with `Idempotency-Key: order_<cartId>`.
2. The request **reaches the server and the order is committed**, but the response is lost (timeout / dropped TCP) → `isNetworkError(e)` true.
3. `catch` → `placeQueued()` → mints a fresh `Uuid().v4()` and queues a `PendingOrder` whose sync sends `Idempotency-Key: <new-uuid>`.
4. On drain, the server sees a **never-seen key** → **creates a second order**. Customer is charged once but the shift gets two orders / double cash expectation.

The optimistic-order id is also the random UUID, so `replaceOrder(localId,…)` will correctly swap the duplicate — but the original (online-committed) order remains as a separate row, and drawer/system-cash math counts both.

Secondary idempotency weaknesses:
- The online key `order_<ms>` from the `??` fallback is **not** globally unique if `state.id` is ever null (millisecond collisions across tabs/devices are possible, though unlikely).
- Cash movements have **no idempotency key at all** — the queue refuses to enqueue them offline (`OfflineCashMovementError`) precisely because a retry after an ambiguous timeout would double-apply. This is the correct mitigation pattern that order-create should mirror.

**Fix-in-port**: use **one** idempotency key for the whole logical placement. The cleanest is to mint the `localId` UUID **before** the online attempt and pass it as the `Idempotency-Key` on both the direct `create` call and the queued `PendingOrder`. Then a lost-response retry replays the same key and the server dedupes (returns the existing order with 200, per the 409 comments at offline_queue.dart:360–386). Equivalently, store that UUID as the cart's idempotency key so both paths read it.

---

## 5. What Breaks Offline (gaps + fix-in-port)

1. **Dual idempotency key → double order on lost-response retry** (Section 4). Highest-severity money bug. Fix: unify the key across online + queued paths.

2. **Voiding a still-unsynced (`pending_sync`) order while online → 404 / dead void.**
   - The void button is shown for any non-voided order including `pending_sync` (order_history_screen.dart:1357). The dependency logic that links a void to its queued create (`enqueueVoid`) only runs on the **offline** branch. On the **online** branch (`void_order_sheet.dart:150`), it calls `voidOrder(widget.order.id, …)` directly — and for a `pending_sync` order `order.id` is the **local UUID**, which the server doesn't know → 404.
   - If instead it's queued (teller goes offline first), `_runDrain` treats a void 404 as **dead** with "Order not found on server" (offline_queue.dart:351–359) — correct, but the order's own create may still be queued behind it; the dependency guard saves the offline case but not the online-void-of-a-pending order case.
   - Fix: gate/disable the void action (or force-queue it with the create dependency) when `order.status == 'pending_sync'`, regardless of connectivity. Reuse the `OfflineVoidBlockedError` UX.

3. **Cash movements are online-only.** Correct by design (no idempotency key backend-side), but the UI must disable cash-in/out entry points when offline or it throws `OfflineCashMovementError`. Confirm those entry points are actually disabled in the port (the queue relies on it).

4. **Change-due UI vs persisted/printed change diverge with a cash tip** (Section 1). The teller sees `tendered - total - cashTip`; the receipt and DB record `tendered - total`. Fix: pick one definition (the cart model's `changeGiven` should subtract a cash tip to match the input UI, or the UI should match the model) and route all three through it.

5. **`restore_inventory` defaults to `false` in two parse/repo paths** (order_repository.dart:100, pending_action.dart:344) while the UI default is `true`. A payload missing the key silently won't restock. Low risk today (UI always sends it) but a latent footgun; default to `true` to match user intent, or make it non-nullable.

6. **Offline-priced totals trust the device.** Items/totals are sent verbatim and the backend records them, only flagging drift. An offline device with a stale menu can persist wrong prices that look authoritative on the receipt. Mitigated by design intent, but worth a reconciliation/flag surface in the port.

7. **Dead code to prune in the port**: `OrderRepository.voidOrder` (the sheet bypasses it for `orderApiProvider.voidOrder`); `OrderRepository.create`'s `idempotencyKey` param is wired but the *value* it forwards is the buggy one.

---

### Key file references
- Checkout flow + dual key: `/Users/shawket/Desktop/sufrix_pos/lib/features/order/checkout/checkout_sheet.dart` (`_place` 354–574, `placeQueued` 478–524, `_finalizeOrder` 693–724)
- Online key: `/Users/shawket/Desktop/sufrix_pos/lib/core/providers/cart_notifier.dart:262`
- Offline key: `/Users/shawket/Desktop/sufrix_pos/lib/core/services/offline_queue.dart:502`
- Sync confirm / replaceOrder: `/Users/shawket/Desktop/sufrix_pos/lib/main.dart:85`, `/Users/shawket/Desktop/sufrix_pos/lib/core/providers/order_history_notifier.dart:126`
- Change formulas: `cart.dart:407`, `checkout_sheet.dart:722`, `cash_tendered_section.dart:87`
- Void: `/Users/shawket/Desktop/sufrix_pos/lib/features/order/void_order_sheet.dart` (`_submit` 108–190), `order_api.dart:129`, `offline_queue.dart:199` & 343–390
- Queue/drain & error taxonomy: `offline_queue.dart:294–455`; DAO ordering `outbox_dao.dart:258`, `291`