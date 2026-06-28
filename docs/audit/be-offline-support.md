That confirms the final piece: `list_delivery_orders` does NOT support `updated_after`/`since` (only `status` + `limit`, ordered by `created_at DESC`). Delivery has an SSE live stream for online realtime updates, but it is "updates-only" with a seed-then-stream model and no replayable cursor — useless after a multi-day offline gap. I now have a complete, concrete picture across all six audit dimensions.

---

# Backend Offline-First Readiness Audit — Madar (Actix-Web Rust)

Scope: `/Users/shawket/Desktop/MadarRust/src` + `/migrations`. Target: a POS client that survives **days** offline and replays **in order**.

## Executive verdict

The backend is **point-hardened for duplicate order submission, but has no general sync substrate.** Idempotency exists on exactly **two creation paths** (dine-in order create, public delivery order create). There is **no replayable change-feed**, **no tombstones for hard-deletes**, **no batch replay**, **no optimistic-concurrency version/etag**, and **no client temp-id ↔ server-id reconciliation** anywhere. A client coming back after a long gap can only **full-refetch** every domain except orders, and even the orders "delta" (`updated_after`) is not a safe cursor.

---

## What EXISTS

| Capability | Where (endpoint / file:line) | Table / column | Notes |
|---|---|---|---|
| Idempotency on dine-in order create | `POST /orders` → `orders/handlers.rs:469-479` (pre-check) + `:1238-1248` (23505 race recovery) via `fetch_order_by_idempotency_key` `:2181` | `orders.idempotency_key uuid` (`full_schema.sql:852`); unique partial idx `orders_idempotency_key_idx` (`:1843`) | Reads `Idempotency-Key` header as UUID; replays existing order on hit. Both call sites are inside the **same** `create_order` handler. |
| Idempotency on public delivery order create | `POST .../delivery/orders` → `delivery/public.rs:929-939` via `staff::fetch_delivery_order_by_idem` (`delivery/staff.rs:97`) | `delivery_orders.idempotency_key uuid` (`delivery_core.sql:131`); unique idx `uq_delivery_orders_idem` (`:140`) | Same header pattern; returns existing delivery order on hit. |
| Partial delta pull for orders | `GET /orders?updated_after=...` → `orders/handlers.rs:331`, filter `:1676` (`o.updated_at >`) | `orders.updated_at` + trigger `orders_set_updated_at` (`:1850`) | **Only domain with any "changed-since" filter.** See "Missing" — it is not a safe cursor. |
| Server-authoritative timestamps (subset) | trigger fn `set_updated_at()` `full_schema.sql:217` | 17 triggers (incl. `set_updated_at` self): `orders, shifts, users, organizations, branches, categories, menu_items, menu_item_optional_fields, menu_item_recipes, bundles, addon_items, addon_item_ingredients, branch_inventory, org_ingredients, org_payment_methods` + `delivery_orders` (manual `updated_at = now()` in staff/public writes) | `created_at`/`updated_at` are server-set. Good foundation for cursoring — but most write paths set `updated_at` only via trigger on UPDATE; many child tables have none. |
| Soft-delete (tombstone-ish) on a few core tables | `branches/handlers.rs:445`, `menu/handlers.rs:595` (categories), `:1051` (menu_items), `users/handlers.rs:581`, `inventory/handlers.rs:650` (org_ingredients) | `deleted_at timestamptz` on `branches, categories, menu_items, users, org_ingredients, organizations, suppliers` | Soft-delete exists but is used **as a query filter only** (`WHERE deleted_at IS NULL`). No endpoint **surfaces** tombstones to a client, and `deleted_at` is not driven through any cursor. |
| Realtime delivery updates (online only) | `GET /delivery-orders/stream` (SSE) `delivery/staff.rs:171-177` | — | "Updates-only": seed via `GET /delivery-orders`, then stream. No replay/backfill — drops on disconnect, useless after an offline gap. |
| Per-(branch, business_date) server sequences | `order_ref.sql:19`, `delivery_core.sql:156` | `order_ref` / `delivery_ref` | Human-facing receipt numbers, **not** a global change sequence. Not usable for sync ordering. |

---

## What's MISSING

| Gap | Affected endpoints / tables | Impact for days-offline replay |
|---|---|---|
| **Idempotency on all other mutating endpoints** | `POST /orders/{id}/void`; shifts: `open_shift`, `close_shift`, `force_close_shift`, `add_cash_movement`, `delete_shift` (`shifts/handlers.rs` — zero idempotency hits); inventory: `add_to_branch_stock`, `create_waste`, `create_transfer` + updates; stocktakes: `create_stocktake`, `upsert_items`, `finalize`, `cancel`; purchasing: `create_order`, `create_return`, `submit/receive/cancel`; delivery staff: `set_status`, `set_prep_time`, `cancel`, `finalize`; all menu/discount/user/payment-method CRUD | A queued void/close-shift/waste/receive replayed after reconnect (or auto-retried after a flaky 5xx) **double-applies**: double inventory deduction, duplicate cash movements, double-finalized stocktakes. Only order/delivery **creates** are safe. |
| **A real change-feed / cursor per mirrored domain** | Everything except orders. `list_delivery_orders` (`delivery/staff.rs:132`) has **no** `updated_after`; menu, categories, addons, branches, discounts, inventory, shifts, payment methods, users — all **full-refetch only** | After a multi-day gap the client must re-pull entire catalogs. No `?since=`/`?cursor=` → no incremental sync, heavy payloads, races. |
| **Orders "delta" is not a safe cursor** | `GET /orders?updated_after` (`orders/handlers.rs:1676-1705`) | Filters on `updated_at >` but **orders by `created_at DESC` with page/offset** → offset over a moving `updated_at` window skips/duplicates rows. Strict `>` can drop ties on equal timestamps. Cannot convey deletes/tombstones. Not a monotonic, resumable cursor. |
| **Tombstones surfaced to clients for deletes** | Hard deletes (rows vanish with no trace): `discounts` (`discounts/handlers.rs:213`), `item_sizes`, `addon_items`, `menu_item_addon_slots`, addon/menu overrides, `branch_inventory`, `branch_inventory_transfers`, `user_branch_assignments`, QR tables | An offline client **never learns a row was deleted**. Even soft-deleted tables don't expose `deleted_at` through any since-cursor, so deletes can't be replayed to the client. |
| **Batch / replay endpoint** | None anywhere (no `/sync`, `/batch`, `/replay`, bulk handler found) | Client with N queued mutations does N sequential round-trips, each independently retried, no atomic/ordered server-side replay, no per-item result map. |
| **Optimistic-concurrency version / ETag** | No `version`/`row_version` column anywhere; no `ETag`/`If-Match`/`If-None-Match` handling anywhere | No conflict detection. Last-write-wins blindly: a stale offline edit silently clobbers a newer server change. No 412 path. |
| **Client temp-id ↔ server-id reconciliation** | No `client_ref`/`temp_id`/`client_order_id` column or field anywhere | Offline-created entities get client temp UUIDs; server returns its own id only via the idempotency replay body. There is **no first-class echo of the client ref**, and for everything except orders/delivery there's no mechanism at all — the client can't map locally-referenced children (e.g. an offline order's payment splits) to server ids on reconnect. |

---

## Prioritized backend work-list to reach full offline-first

**P0 — stop double-apply on replay (correctness/money).** Extend the existing `Idempotency-Key` header pattern (copy `orders/handlers.rs:469-479` + 23505 race-recovery) to every mutating endpoint, prioritizing money/stock paths: `void_order`, `open/close/force_close_shift`, `add_cash_movement`, inventory `create_waste`/`create_transfer`/`add_to_branch_stock`, stocktake `finalize`, purchasing `receive_order`/`create_return`, delivery `set_status`/`finalize`/`cancel`. Cheapest scalable approach: a single `idempotency_keys(key uuid pk, org_id, endpoint, request_hash, response_json, status_code, created_at)` table checked by middleware, instead of one nullable column per table.

**P1 — temp-id reconciliation.** Add a `client_ref` (uuid/text) column to client-created entities (`orders`, `delivery_orders`, `shifts`, `stocktakes`, inventory movements, purchasing orders) with a unique `(branch_id, client_ref)` index, and **echo it back** in every create/replay response so the client maps temp→server ids deterministically. This makes idempotency replay also serve reconciliation.

**P2 — change-feed cursor per mirrored domain.** Add a monotonic, gap-safe cursor. Recommended: a global `change_log(seq bigserial pk, org_id, branch_id, entity, entity_id, op, version, updated_at)` populated by triggers, exposed as `GET /changes?since=<seq>&entities=...` returning ordered rows incl. tombstones. Minimum viable alternative per table: add `?updated_after=` ordered by `(updated_at, id)` keyset (not offset) — but fix the orders endpoint first, since its current offset-over-`updated_at` is unsafe. Cover at least: menu/categories/addons/overrides, discounts, inventory, branches, payment methods, delivery_orders, orders.

**P3 — tombstones in the feed.** Convert remaining hard-deletes on synced tables (`discounts`, overrides, `item_sizes`, addon slots, transfers, branch_inventory) to soft-delete `deleted_at`, and emit a `delete` op into the change_log so the cursor pull carries deletions. Without this, deletes never reach an offline client.

**P4 — batch replay endpoint.** `POST /sync/batch` accepting an ordered array of `{client_ref, idempotency_key, method, path, body}`, applied **in submission order**, returning a per-item result map (`{client_ref → {server_id, status}}`). Reuses P0 idempotency + P1 client_ref. Lets a days-offline client drain its queue in one ordered, resumable call.

**P5 — optimistic concurrency.** Add `version integer` (bump in `set_updated_at` trigger) to mutable synced entities; accept `If-Match`/expected-version on update endpoints and return `409/412` on mismatch so stale offline edits surface as conflicts instead of silent clobbers.

### Key file references
- Idempotency pattern (template to replicate): `/Users/shawket/Desktop/MadarRust/src/orders/handlers.rs:469-479`, `:1238-1248`, `:2181`
- Delivery idempotency: `/Users/shawket/Desktop/MadarRust/src/delivery/public.rs:929-939`, `/Users/shawket/Desktop/MadarRust/src/delivery/staff.rs:97`
- Orders unsafe "delta": `/Users/shawket/Desktop/MadarRust/src/orders/handlers.rs:331`, `:1676-1705`
- Delivery list (no since-cursor): `/Users/shawket/Desktop/MadarRust/src/delivery/staff.rs:118-143`; SSE-only stream `:171-177`
- `updated_at` trigger fn + coverage: `/Users/shawket/Desktop/MadarRust/migrations/20260531200000_full_schema.sql:217`, triggers `:1847+`
- Idempotency columns/indexes: `full_schema.sql:852`/`:1843` (orders), `20260614150000_delivery_core.sql:131`/`:140` (delivery)
- Hard-delete sites to convert: `discounts/handlers.rs:213`, `menu/handlers.rs:1189/1474/1660/1901/1950/2278/2470/2519/2631`, `inventory/handlers.rs:953/1719`, `users/handlers.rs:725`