I have everything needed for the audit. Both `orders` and `delivery_orders` have partial unique indexes on `idempotency_key`, confirming the replay-safety mechanism. Here is the complete audit.

---

# Backend Audit: Recipe-Driven Stock Depletion & Delivery Lifecycle (Offline-First)

Root audited: `/Users/shawket/Desktop/MadarRust/src`

## 1. Depletion Model & Replay-Safety Verdict

### Where depletion actually happens
Recipes are **not** depleted by the `recipes/` module — `recipes/handlers.rs` is pure CRUD on `menu_item_recipes` / `addon_item_ingredients`. The depletion engine lives in **order creation**. The recipe is resolved to a flat list of `InventoryDeduction` entries (base-unit, yield-grossed quantities — see `recipes/handlers.rs:346 normalize_recipe_unit`) by the shared resolver, then applied.

Two depletion sites, both **server-side at sale time**, never at delivery-order intake:

| Path | Entry point | Stock mutation | Ledger |
|---|---|---|---|
| POS / direct sale | `orders/handlers.rs:460 create_order` → loop at `:1506` | `UPDATE branch_inventory SET current_stock = current_stock - $1` (`:1517`) | `inventory_movements` type `sale`, `source_type='order'` (`:1546`) |
| Delivery finalize | `delivery/staff.rs:475 finalize_delivery_order` → `snapshot::apply_snapshot` (`snapshot.rs:494`, depletes at `:686`) | same `UPDATE … - $1` | type `sale` |

Negative stock is **allowed but flagged**: the `RETURNING current_stock` is checked, `below_zero` is stamped on the movement, and a warning is surfaced (`:1538-1544`). Untracked ingredients soft-fail (warn + skip, `:1528`).

### Replay-safety — **SAFE for order-create** ✅

Depletion is **atomic with the order insert** (single `tx` opened at `handlers.rs:1093`, committed at `:1575`; the `branch_inventory` UPDATE and the `record_movement` both run on `&mut *tx`). It is guarded by an idempotency key:

- Client sends `Idempotency-Key: <uuid>` header (`:469`).
- **Pre-check**: if the key already maps to an order, the existing order is replayed and the function returns *before* any depletion (`:475-479`).
- **Race fallback**: the `orders.idempotency_key` column has a **partial unique index** (`migrations/20260531200000_full_schema.sql:1843`). A concurrent duplicate that passes the pre-check hits a `23505`; the handler catches it, drops the tx (rolling back the would-be second depletion), and replays the committed order (`:1238-1248`).

**Verdict: a queued offline order replayed N times deducts stock exactly once**, *provided the POS reuses a stable `Idempotency-Key`*. Confirmed on the client side: `offline_queue.dart:502` passes `idempotencyKey: action.localId` (stable local UUID), so retries of a queued order are idempotent end-to-end. Offline `created_at` is honored (`:1060`) with only future-clock rejection (`:1064`).

### Replay-safety — **SAFE for delivery finalize** ✅ (but by a different mechanism)
`finalize_delivery_order` has **no idempotency key**. It is instead protected by a `SELECT order_id … FOR UPDATE` CAS at `staff.rs:554`: if `order_id` is already set, it returns `409 Conflict`. The finalize-driven depletion is in the same tx as the `delivery_orders.order_id` link (`:585-594`). A replay finds `order_id` populated → conflicts → no second depletion. Note this returns **409, not a replay of the result**, so an offline client must treat 409-on-finalize as "already applied" (not an error).

### Void / restock reversal — **idempotent** ✅
`void_order` (`:1844`) flips status with a guarded `WHERE id=$1 AND status <> 'voided'` + `fetch_optional`; a 0-row match returns the already-voided order without re-restocking (`:1877`). This is logged in `AUDIT_REPORT.md` as V6 (double-restock fixed).

---

## 2. Delivery Order State Machine + Per-Transition Offline Needs

States: `received → confirmed → preparing → ready → out_for_delivery → delivered`, plus terminal `cancelled` / `rejected`. A `delivery_orders` row exists from intake; **no `orders` row until finalize** (`delivery/mod.rs:1-12`).

| Transition | Endpoint / fn | Mutating? | Idempotent today? | Offline need |
|---|---|---|---|---|
| intake → `received` | `public.rs create_delivery_order` (`:929`) | yes (creates row) | **Yes** — `Idempotency-Key` header + partial unique index `uq_delivery_orders_idem` (`migrations/20260614150000_delivery_core.sql:140`); replays existing (`public.rs:936`) | n/a (customer-facing) |
| line-step jump (`confirmed`/`preparing`/`ready`/`out_for_delivery`) | `staff.rs:291 set_status` | yes — bare `UPDATE` (`:326`), **no tx, no CAS** | **Naturally idempotent on stock** (sets absolute status, clears other stamps; no inventory touched) but **WhatsApp is a side effect**: `jump_whatsapp_message` fires on any forward jump (`:342`). A retried "confirm" where the order is *already* confirmed re-sends nothing (no new step crossed), but an out-of-order retry that re-crosses a step *could* re-notify. | Safe to retry for stock; client should suppress duplicate sends by not re-issuing same-target transitions. No server idempotency key. |
| `prep-time` | `staff.rs:631 set_prep_time` | yes — bare `UPDATE` (`:652`) | **Idempotent** (absolute set of `extra_prep_minutes`) | Safe to replay. |
| cancel / reject (+ optional waste) | `staff.rs:377 cancel_delivery_order` | yes — **tx + guarded CAS** (`:404` `WHERE status NOT IN (delivered,cancelled,rejected)`) | **Yes** — CAS winner deducts waste once; loser/retry gets `409` (`:422`) and waste is never double-deducted | Client must treat `409 "already cancelled"` as success on replay. |
| finalize | `staff.rs:475` | yes — tx + `FOR UPDATE` CAS on `order_id` (`:554`); also per-shift advisory lock (`:540`) | **Yes** via CAS, returns `409` on replay | Treat `409 "already finalized"` as success; reuse same `shift_id`. |

**Key offline gap in the state machine:** `set_status` and `set_prep_time` are the only two mutating delivery transitions with **no transaction and no idempotency key**. They happen to be naturally idempotent (absolute writes, no ledger), so replay-safe for *data*, but `set_status` carries a **non-idempotent WhatsApp side effect** that is only de-duplicated by the "no new step crossed" check — fragile under offline reorder/replay. There is also a **lost-update risk**: two managers (or an offline replay racing a live edit) both `UPDATE` with no row lock; last write wins silently.

---

## 3. Realtime Catch-Up Gap (SSE)

The stream is `GET /delivery-orders/stream` (`staff.rs:188`), fanned out from an **in-process per-branch `tokio::broadcast`** hub (`hub.rs`).

**There is no replay, no cursor, no event id, no Last-Event-ID support.** Concretely:

- The stream is explicitly **"updates-only"** (`staff.rs:173`): the documented contract is *"GET /delivery-orders first to seed, then connect; on ANY error/disconnect, re-GET and reconnect."*
- Broadcast capacity is a fixed ring of **128** events per branch (`hub.rs:38`). A slow/backgrounded POS that falls >128 behind gets a `Lagged` error; the handler converts it to a `500` body error to **force-drop the connection** (`staff.rs:203-212`), relying on the client to reconnect and full-refetch.
- Events carry no monotonic id or sequence — `DeliveryEvent` is just `{event_type, order}` (`hub.rs:29`). The POS upserts the full `DeliveryOrder` by id; there is **no way to know an event was missed** except by noticing the disconnect.
- `publish` is **fire-and-forget with no persistence** (`hub.rs:62`): if no subscriber exists at publish time, or the event is dropped due to lag, it is gone forever. Nothing is written to a durable outbox.
- **Single-instance only** (documented limitation, `hub.rs:9-13`): horizontal scaling silently breaks fan-out — a publish on instance 1 never reaches a subscriber on instance 2. No Redis/`LISTEN-NOTIFY` backing yet.

**What a client needs to catch up after a gap:** there is no incremental catch-up. The only recovery is a **full `GET /delivery-orders`** (optionally `?status=` filtered, default limit 200, `staff.rs:130`) on every reconnect/lag. For a POS that was offline/backgrounded, this means: reconnect → re-GET the whole active queue → reconcile by id. Cost grows with queue size; there is no `updated_since` / cursor parameter on the list endpoint to fetch only deltas (the list orders by `created_at DESC` and has no `updated_at` cursor filter). **Recommended fix for offline-first:** add an event sequence id + an `updated_since`/`after_seq` query param so reconnects fetch only the delta, and back the hub with Postgres `LISTEN/NOTIFY` for durability across restarts/instances.

---

## 4. Mutations Lacking Idempotency (cross-cut with offline-support)

These inventory mutations are protected against **concurrent races** (FOR UPDATE row locks) but have **no `Idempotency-Key` / `client_token`** — so an offline-queued retry would **double-apply** the stock delta:

| Mutation | fn | Guard present | Replay-safe? |
|---|---|---|---|
| Waste | `inventory/handlers.rs:1043 create_waste` | `FOR UPDATE OF bi` lock (`:1066`), stock-floor check | **NO** — each retry deducts again. No idempotency key. |
| Transfer | `inventory/handlers.rs:1192 create_transfer` | source `FOR UPDATE` lock (`:1250`) | **NO** — each retry moves stock again. |
| Stocktake finalize | `stocktakes/handlers.rs:403 finalize_stocktake` | sets stock absolutely + variance movements | Partial — re-finalize would re-apply variance movements; no terminal-state guard verified here. |
| Cash movement | (POS-side note) `offline_queue.dart:220` | — | Flagged by the client itself: *"The backend has no idempotency key for cash movements"* |

The delivery **cancel-with-waste** path is the **only** waste-producing endpoint that is replay-safe (CAS-guarded, `staff.rs:404`). Direct `create_waste` and `create_transfer` are the concrete double-deduction risks if the POS ever queues them offline. The POS today only queues `order / shiftOpen / shiftClose / voidOrder / cashMovement` (`offline_queue.dart:56-71`) — waste/transfer/stocktake are **not** in the offline outbox, so the gap is latent, but any future move to queue them requires adding idempotency keys server-side first (mirror the `orders.idempotency_key` partial-unique-index pattern).

### Summary of verdicts
- **Recipe depletion on order-create: replay-safe** (idempotency key + atomic tx + partial unique index). ✅
- **Delivery finalize depletion: replay-safe** via `order_id` CAS (returns 409, not a replay). ✅
- **Void restock & delivery cancel waste: idempotent** (guarded CAS). ✅
- **Delivery `set_status` / `set_prep_time`: data-idempotent but unguarded** (no tx/lock; WhatsApp side effect + lost-update risk). ⚠️
- **SSE: no cursor/replay** — full re-GET on every reconnect; in-process, single-instance, 128-event ring. ⚠️
- **`create_waste` / `create_transfer` / cash movements: NOT idempotent** — concurrency-safe only, would double-apply on offline replay. ❌