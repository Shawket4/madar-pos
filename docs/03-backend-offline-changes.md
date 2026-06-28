I now have a complete and accurate picture of the existing backend. I have everything I need to write a concrete, implementation-grade spec grounded in the real code. Let me produce the PLAN.md section.

Key facts I've confirmed:
- `orders` already has `idempotency_key uuid` + `orders_idempotency_key_idx` unique partial index, read via `Idempotency-Key` header in `create_order`.
- `delivery_orders` already has the same pattern (`uq_delivery_orders_idem`).
- Shifts use client-supplied PK `id` as the de-facto idempotency token (replay-by-PK).
- `clock.rs` already enforces `reject_if_future` with 5-min skew on client timestamps.
- `void_order`, `set_status`, `set_prep_time`, `finalize_delivery_order`, `cancel_delivery_order`, `add_cash_movement`, `close_shift`, `create_waste` have NO idempotency.
- `AppError` maps to 401/403/404/400/409; no 412/422 yet.
- `list_orders` already accepts `updated_after` cursor + `include_items`.
- Migrations are timestamp-named, applied on boot via `sqlx::migrate!`.

Here is the spec.

---

# Offline-Sync Backend Support (MadarRust)

> Scope: make the POS survive days offline and replay its write backlog safely. This section specifies the **backend** changes. Money is integer minor-units (piastres) everywhere relevant; nothing here introduces BigDecimal-as-string. All new endpoints are additive — see §6 for the no-break rollout.

## 0. What already exists (do not re-build)

Audit of `MadarRust` as of this plan:

| Capability | Status | Location |
|---|---|---|
| Per-table idempotency on `create_order` via `Idempotency-Key` header | **Exists** | `src/orders/handlers.rs:469-479`; column `orders.idempotency_key uuid` + partial unique `orders_idempotency_key_idx` |
| Per-table idempotency on `create_delivery_order` (public) | **Exists** | `src/delivery/public.rs:929`; `delivery_orders.idempotency_key` + `uq_delivery_orders_idem` |
| Client-supplied PK as replay token (shifts) | **Exists** | `open_shift` replays by `body.id` (`src/shifts/handlers.rs:313-339`) |
| Client-supplied event timestamps (`created_at`/`opened_at`/`closed_at`/`voided_at`) | **Exists** | `OpenShiftRequest`, `CloseShiftRequest`, `CashMovementRequest`, `CreateOrderRequest`, `VoidOrderRequest` |
| Future-timestamp guard (5-min skew) | **Exists** | `src/clock.rs::reject_if_future` |
| `updated_after` cursor + `include_items` bulk read on orders | **Partial** | `ListOrdersQuery` (`src/orders/handlers.rs:328-348`) — time-based, not monotonic, no tombstones |
| Generic idempotency store, batch replay, change-feed with tombstones, version/ETag | **Missing** | — this plan |

The strategy is to **generalize** the ad-hoc order/delivery idempotency into one mechanism and apply it to every outbox-mutation, then add a change-feed and a batch-replay envelope on top.

## 1. Idempotency keys

### 1.1 Header contract

Every outbox-mutation request carries:

```
Idempotency-Key: <uuidv4>          # required for the endpoints in §1.4
```

- Client owns the key (the server mints none). The key is **stable per logical mutation** — generated once when the action is enqueued in the POS outbox, reused on every retry.
- Key must be a v4 UUID; reject malformed with `400`. (Matches existing `Uuid::parse_str(...).ok()` parse in `create_order`.)
- Keys are scoped per `(org_id, endpoint)` — see fingerprint in §1.2. A teller and a different branch can never collide.

### 1.2 Storage

Replace the scattered per-table `idempotency_key` columns with a **central store**, while keeping the existing per-table columns during migration (§6) so in-flight clients don't break.

New migration `20260620000000_idempotency_store.sql`:

```sql
CREATE TABLE idempotency_keys (
    org_id         uuid        NOT NULL,
    key            uuid        NOT NULL,
    endpoint       text        NOT NULL,          -- opId, e.g. 'create_order'
    request_hash   bytea       NOT NULL,          -- sha256 of canonical body (mismatch detection)
    -- Outcome, written only after the mutation commits:
    status_code    smallint,                      -- NULL until committed
    response_body  jsonb,                         -- the original 2xx payload, replayed verbatim
    target_id      uuid,                          -- server PK of the row created (orders.id, shifts.id, …)
    created_at     timestamptz NOT NULL DEFAULT now(),
    completed_at   timestamptz,
    PRIMARY KEY (org_id, key, endpoint)
);
-- TTL sweep index
CREATE INDEX idempotency_keys_created_idx ON idempotency_keys (created_at);
```

- **TTL: 30 days.** Offline windows are "days", and `created_at` on outbox items can be old; a 7-day TTL would prune a key the client still wants to replay. A background sweep (or `DELETE ... WHERE created_at < now() - interval '30 days'` on a cron / `tokio` interval in `main.rs`) reclaims rows. The selling-day uniqueness invariants (e.g. one open shift) provide a second-line defense after TTL expiry.
- `response_body` stores the **exact** serialized 2xx response (e.g. the `OrderFull` JSON). On replay we return it byte-for-byte with the original status — see §1.3.

### 1.3 Behavior on replay (middleware)

Implement as an Actix wrapper used by the outbox routes (mirrors the manual check already in `create_order:475`, but centralized):

1. Extract `Idempotency-Key`. Absent on a required endpoint → `400 IdempotencyKeyRequired`.
2. `SELECT` from `idempotency_keys` by `(org_id, key, endpoint)`:
   - **Hit, `completed_at` set, `request_hash` matches** → short-circuit: return stored `status_code` + `response_body` verbatim, plus header `Idempotency-Replayed: true`. The handler never runs. (This is what `create_order` does today via `fetch_order_by_idempotency_key`.)
   - **Hit, `request_hash` differs** → `422 IdempotencyKeyReuse` ("key reused with a different body"). Protects against client bugs replaying a mutated payload under an old key.
   - **Hit, `completed_at` NULL** (in-flight / crashed mid-write) → `409 IdempotencyInFlight`, `Retry-After: 2`. Client retries; the unique PK insert serializes concurrent duplicates (same pattern as the `23505` race handling at `orders/handlers.rs:1235`).
   - **Miss** → `INSERT ... (org_id, key, endpoint, request_hash)` (claims the slot), run the handler **in the same transaction**, then `UPDATE` the row with `status_code/response_body/target_id/completed_at` before commit. On handler error, the claim row is rolled back so the client can retry cleanly.

`AppError` gains two variants → `IdempotencyKeyReuse => 422`, `IdempotencyInFlight => 409` (extend the match in `errors.rs:97-102`; `422` is new, add `UnprocessableEntity` mapping).

### 1.4 Endpoints that MUST honor it (all outbox-mutations)

| opId | Method · Path | Today | Action |
|---|---|---|---|
| `create_order` | `POST /orders` | header-idem on `orders` table | migrate to central store |
| `void_order` | `POST /orders/{order_id}/void` | **none** | **add** |
| `set_status` | `POST /delivery-orders/{id}/status` | **none** | **add** (+ suppress-late side effects, §1.5) |
| `set_prep_time` | `POST /delivery-orders/{id}/prep-time` | **none** | **add** |
| `finalize_delivery_order` | `POST /delivery-orders/{id}/finalize` | **none** | **add** (creates an order row; reconcile per §2) |
| `cancel_delivery_order` | `POST /delivery-orders/{id}/cancel` | **none** | **add** |
| `open_shift` | `POST /shifts/branches/{branch_id}/open` | PK-replay | **also** accept header; keep PK-replay |
| `close_shift` | `POST /shifts/{shift_id}/close` | **none** | **add** |
| `add_cash_movement` | `POST /shifts/{shift_id}/cash-movements` | **none** | **add** |
| `create_waste` | `POST /inventory/branches/{branch_id}/waste` | **none** | **add** (only outbox-mutation in inventory) |

> Online-only non-idempotent admin POSTs flagged `needsIdempotency:true` in the inventory (`create_org`, `create_user`, `create_branch`, `assign_branch`, `complete_onboarding`, `create_*` catalog/menu, `create_zone`, `create_table`, etc.) **should also accept** the header — they run online but a flaky reconnect can double-submit. They are **not** part of the batch-replay outbox (§4); they go through the normal single-request idempotency middleware. Lower priority than the outbox set above.

### 1.5 Side-effecting replays (WhatsApp / customer tracking)

`set_status` and `finalize_delivery_order` fire WhatsApp messages and update customer tracking (`src/delivery/staff.rs:291+`). On a **late offline replay** these would send stale notifications. Rule:

- The idempotency short-circuit (§1.3) already prevents a *duplicate* of the *same* key from re-firing.
- For genuinely stale-but-new transitions, gate the side effect on freshness: if `created_at` of the action (client-supplied) is older than a threshold (e.g. `now() - 6h`) **or** the order has already advanced past the target step server-side, persist the state change but **skip the outbound WhatsApp send**. Add a `suppress_notifications` decision inside the handler, not a new field.

## 2. Client temp-id ↔ server-id reconciliation

### 2.1 The two existing patterns, unified

The backend already supports **client-supplied primary keys** for shifts (`OpenShiftRequest.id`). Standardize on this as the primary reconciliation mechanism and add an explicit `client_ref` echo for endpoints where the server, not the client, mints the PK (orders).

**Request shape** — every outbox-mutation accepts an optional:

```jsonc
{ "client_ref": "a3f1...-uuid", ...domain fields }
```

- For shift open: `client_ref` may equal the existing `id` field (keep `id` for back-compat; treat `client_ref` as an alias that also gets stored).
- The POS generates `client_ref` as the local row's UUID PK in SQLite. It is the join key the client uses to patch its local row once the server responds.

**Response shape** — every outbox-mutation returns the canonical entity **plus** an echo:

```jsonc
{
  "id": "<server uuid>",          // authoritative server PK
  "client_ref": "<echoed>",       // exactly what the client sent (null if none)
  "order_number": 42,             // server-minted human refs (orders)
  "order_ref": "DT-260614-0042",  // server-minted (orders/handlers.rs:1170)
  ... rest of entity
}
```

Add `client_ref` to the persisted row (nullable `uuid` column on `orders`, `shifts`, `shift_cash_movements`, `delivery_orders`, `inventory_movements`/waste). It is stored so the change-feed (§3) can echo it back too, letting a client that lost its outbox still re-key.

### 2.2 Dependent records created offline (the hard case)

A teller offline opens a shift (`client_ref = S`), places orders against it (`shift_id = S`), records a cash movement (`shift_id = S`). All three reference a shift that **does not yet have a server id** because the client used its own UUID.

**Resolution: client-PK passthrough (preferred).** Because `open_shift` accepts a client-supplied `id`, the server PK **equals** the client UUID `S`. So child rows referencing `shift_id = S` are already valid server-side after the parent replays — **no rewrite needed**. This is why the batch (§4) must be **ordered**: parent (`open_shift`) before children (`create_order`, `add_cash_movement`).

**Resolution: ref-rewrite (fallback, for server-minted PKs).** `finalize_delivery_order` creates an `orders` row whose PK the **server** mints. If a later offline action referenced that order by the client's temp id, the batch envelope (§4) carries a `client_ref` on the producer and lets consumers reference `{"ref": "<client_ref>"}` instead of a literal id. The batch executor maintains an in-transaction map `client_ref -> server_id` built from each step's result and substitutes before executing dependent steps. Document which fields are ref-resolvable per endpoint:
  - `create_order.shift_id` — resolvable (but normally already a client-PK shift, so literal).
  - `add_cash_movement` path `{shift_id}` — resolvable.
  - `void_order` path `{order_id}` — resolvable to a `create_order` produced earlier in the same batch.

> Invariant: a child whose parent ref can't be resolved (parent step failed, see §4) is **skipped, not orphaned** — its outbox item stays pending on the client.

## 3. Delta / cursor sync (change feed)

### 3.1 Monotonic sequence (not wall-clock)

The existing `updated_after` cursor (`ListOrdersQuery`) is wall-clock and **misses concurrent writes with equal timestamps and can't represent deletes**. Replace with a per-tenant monotonic sequence.

New migration `20260620010000_change_log.sql`:

```sql
CREATE SEQUENCE change_seq;     -- global; monotonic, gap-tolerant

CREATE TABLE change_log (
    seq        bigint      NOT NULL DEFAULT nextval('change_seq') PRIMARY KEY,
    org_id     uuid        NOT NULL,
    branch_id  uuid,                          -- NULL for org-scoped rows (catalog)
    domain     text        NOT NULL,          -- 'menu_item','category','order','shift', …
    entity_id  uuid        NOT NULL,
    op         text        NOT NULL,          -- 'upsert' | 'delete'
    version    bigint      NOT NULL,          -- entity row version (§5)
    changed_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX change_log_feed_idx ON change_log (org_id, branch_id, domain, seq);
```

Populate via triggers (or a thin write-path helper called from each mutating handler — preferred over triggers to keep ordering with the idempotency txn). A `delete` (incl. soft-delete via `deleted_at`, used by `categories`/`menu_items`) emits `op='delete'` so the client can tombstone its mirror.

### 3.2 Change-feed endpoint per mirrored domain

```
GET /sync/changes?since=<seq>&domains=menu_item,category,addon_item,bundle,discount,
                   payment_method,branch_menu_override,branch_addon_override,
                   order,shift,cash_movement,branch_stock,catalog,recipe
                   &branch_id=<uuid>&limit=500
```

Response:

```jsonc
{
  "changes": [
    { "seq": 90412, "domain": "menu_item", "entity_id": "...", "op": "upsert",
      "version": 7, "entity": { ...full row... } },
    { "seq": 90413, "domain": "menu_item", "entity_id": "...", "op": "delete",
      "version": 8, "entity": null }
  ],
  "next_since": 90413,      // pass back as ?since=
  "has_more": true          // page until false
}
```

- **Scoping:** filtered by the caller's `org_id` (from JWT) and `branch_id`. Catalog/identity rows are org-scoped (`branch_id IS NULL` returned to every branch); orders/shifts/stock are branch-scoped.
- **Pagination:** strictly by `seq` (`WHERE seq > $since ORDER BY seq LIMIT $limit`). Cursor is opaque-but-monotonic; client loops until `has_more=false`. No offset pagination.
- **Tombstones:** `op='delete'` rows carry `entity:null` + `entity_id`; the client deletes the local mirror row. Tombstone rows are retained in `change_log` for the TTL window (e.g. 60 days) so a client offline for weeks still sees the delete; older deletes are reconciled by a full re-pull (`?since=0`).
- **Which domains map to the feed** = exactly the `read-cache` entries in the inventory: all of menu/catalog/pricing, payment methods, branch overrides (layered client-side over base catalog), `orders`/`get_order`, `shifts`/cash-movements/shift-report, `list_branch_stock`, `list_catalog`, `get_inventory_settings`, drink/addon recipes, `list_tables`, `list_zones`, `get_branch_settings`, `timezones` (static — `?since=0` once). **Excluded** (online-only, too large/rare): movement ledger, all `/reports/*`, menu-advisor, costing, PO/stocktake detail.

> Back-compat: keep `GET /orders?updated_after=` working through the deprecation window (§6). New clients prefer `/sync/changes`.

## 4. Batch replay endpoint

A teller back online after days has a large **ordered** backlog. One round trip per item is too slow and loses ordering guarantees.

```
POST /sync/replay
Idempotency-Key: <batch-uuid>        # idempotent at the batch level too
```

Request envelope:

```jsonc
{
  "branch_id": "<uuid>",
  "items": [                          // STRICT order = client outbox order
    { "seq": 1, "op": "open_shift",
      "idempotency_key": "k1", "client_ref": "S",
      "path": { "branch_id": "..." },
      "body": { "id": "S", "opening_cash": 50000, "opened_at": "2026-06-15T07:00:00Z" } },
    { "seq": 2, "op": "create_order",
      "idempotency_key": "k2", "client_ref": "O1",
      "body": { "shift_id": "S", "items": [...], "total_amount": 8500,
                "created_at": "2026-06-15T07:14:00Z" } },
    { "seq": 3, "op": "add_cash_movement",
      "idempotency_key": "k3",
      "path": { "shift_id": "S" },
      "body": { "amount": -2000, "created_at": "2026-06-15T09:00:00Z" } }
  ]
}
```

- `op` is the opId; the executor dispatches to the same handler logic as the single-request route, reusing the §1 idempotency check **per item** (so a re-sent batch with overlapping keys no-ops the already-applied items).
- `client_ref` map (§2.2) is built as items execute; `{"ref":"O1"}` placeholders in later items' bodies/paths are substituted.
- `created_at`/`opened_at`/etc. are honored (client's real offline time), still subject to `reject_if_future` (`clock.rs`).

**Partial-failure semantics — stop-on-dependency, continue-on-independent:**

- Each item runs in **its own transaction** (so item N+1's failure doesn't roll back N's committed order).
- Items are applied in `seq` order. If an item **fails**, every later item that **depends on it** (references its `client_ref`) is marked `skipped`; independent later items still apply.
- Response is per-item, never a blanket 4xx/5xx:

```jsonc
{
  "results": [
    { "seq": 1, "status": "applied",  "id": "S",  "client_ref": "S",  "http_status": 201 },
    { "seq": 2, "status": "applied",  "id": "<server>", "client_ref": "O1", "http_status": 201,
      "order_number": 42, "order_ref": "DT-260615-0042" },
    { "seq": 3, "status": "failed",   "client_ref": null, "http_status": 409,
      "error": { "code": "ShiftClosed", "message": "..." } }
  ],
  "applied": 2, "failed": 1, "skipped": 0,
  "max_seq": 90999          // change_log high-water after this batch
}
```

- The client marks `applied` items done (patching local PK from `id`), retries `failed` items (or surfaces to the user for `409`/`422` business conflicts like "shift already closed"), and leaves `skipped` items pending.
- **Batch size cap:** `items.length <= 500`; larger backlogs paginate into multiple `/sync/replay` calls (the per-item idempotency keys make re-sends safe).
- HTTP status of the envelope is `200` whenever the batch was *processed* (even with per-item failures); `400` only for a malformed envelope; `401/403` for auth.

## 5. Server-authoritative timestamps & versioning

### 5.1 Timestamps

- Wall-clock event time is **client-supplied where it models a real offline event** (`created_at`/`opened_at`/`closed_at`/`voided_at` — already supported), guarded by `reject_if_future` (`clock.rs`). Business-day bucketing and `order_ref` minting stay **server-derived** from the corrected instant `AT TIME ZONE` the branch zone (already done at `orders/handlers.rs:1145`).
- **Sync ordering time is server-authoritative**: `change_log.changed_at` + `seq` are stamped by the server, never the client. Clients must never sort the mirror by client `created_at` for sync purposes.

### 5.2 Version column / ETag

Add a monotonic `version bigint NOT NULL DEFAULT 1` to every mirrored, server-mutable entity (`orders`, `shifts`, `delivery_orders`, branch stock, catalog rows, …). Bump on every update (`version = version + 1`) in the same statement that writes the row; the trigger/helper that appends to `change_log` copies it.

Conflict checks (for the few client-editable shared rows — primarily delivery-order status, branch settings):

```
If-Match: <version>          # optimistic concurrency on PUT/POST status edits
```

- Match → apply, bump version. Mismatch → `412 PreconditionFailed` with the current entity in the body so the client can rebase. (Add `AppError::PreconditionFailed => 412` to `errors.rs`.)
- Pure outbox creates (`create_order`, `open_shift`, `create_waste`) need **no** `If-Match` — idempotency key + client-PK already make them safe; versioning there is only for the change-feed.
- Responses expose the version as both a JSON `version` field and an `ETag` header for HTTP-cache-friendly reads.

> Most POS conflicts don't need `If-Match`: a teller owns their shift and orders exclusively (enforced by `teller_id`/branch guards already in the handlers), so concurrent edits are rare. Reserve `If-Match` for genuinely shared mutable rows (delivery tickets touched by multiple stations; branch delivery settings).

## 6. Migration & rollout plan (no break to current Flutter POS or dashboard)

**Principle: everything additive; no field removed, no status code changed for existing requests.**

1. **DB migrations (backward-compatible):** add `idempotency_keys`, `change_log`, `change_seq`, the `client_ref` and `version` columns (nullable / defaulted). Keep the existing `orders.idempotency_key` / `delivery_orders.idempotency_key` columns and indexes **in place** — the dual-write phase below depends on them. All run through the existing boot-time `sqlx::migrate!` (`main.rs`), which already applies pending migrations on startup.

2. **Phase A — dual-write idempotency.** New middleware writes the central `idempotency_keys` row; `create_order`/`create_delivery_order` **also** keep writing their existing per-table column and the existing replay check stays as a fallback. Old clients (no/legacy header) keep working unchanged; new outbox endpoints (`void_order`, `close_shift`, …) use only the central store.

3. **Phase B — change-feed shadow.** Populate `change_log` on all mutating paths but keep `GET /orders?updated_after=` serving. Ship `GET /sync/changes` and `POST /sync/replay` as **new** routes (configured in `main.rs` alongside the existing `.configure(...)` calls). Existing clients ignore them.

4. **Phase C — client cutover.** New POS build uses `Idempotency-Key` on all outbox writes, `/sync/changes` for mirrors, `/sync/replay` for backlog. Dashboard (React/Tauri, Orval-generated) is **unaffected**: it consumes the same untouched read/write endpoints; the new sync endpoints are simply not in its generated client until the OpenAPI spec is regenerated.

5. **OpenAPI / generator safety.** Regenerate `openapi.json` (utoipa) **after** the new schemas are annotated. Honor the inventory's generator quirks: new envelope enums (`op`, item `status`) must be **open strings with `#[serde(other)]` unknown-default** (same rule already noted for `status`/`order_type`/`payment_method`), money stays `int32`/`int64` minor-units (no BigDecimal-as-string), all new optionals are `Option<T>` (`["T","null"]`). Run `./tool/generate_api.sh` (POS) / `npm run generate:api` (dashboard) only once the spec is stable so generated clients don't churn.

6. **Phase D — deprecate.** After the new POS is in the field, mark `?updated_after=` deprecated in the spec and, in a later release, drop the per-table `idempotency_key` fallback in favor of the central store. No code path is removed until telemetry shows zero legacy-client traffic.

**Rollback:** every new table is write-additive; disabling the new routes and middleware reverts behavior to today with no data loss. The dual-write phase means even the idempotency store can be turned off without stranding orders (the per-table column still works).

---

### Files a backend engineer will touch
- New: `src/sync/mod.rs` (middleware + `/sync/changes` + `/sync/replay`), `migrations/20260620000000_idempotency_store.sql`, `migrations/20260620010000_change_log.sql`, `migrations/2026062002xxxx_client_ref_and_version.sql`.
- Edit: `src/errors.rs` (add `IdempotencyKeyReuse`→422, `IdempotencyInFlight`→409, `PreconditionFailed`→412), `src/main.rs` (register sync routes + TTL sweep task), `src/orders/handlers.rs` (route `create_order`/`void_order` through central store; echo `client_ref`), `src/shifts/handlers.rs` (`close_shift`/`add_cash_movement` idempotency + `client_ref`), `src/delivery/staff.rs` (status/prep/finalize/cancel idempotency + late-replay suppression), `src/inventory/*` (`create_waste`), each mutating handler to append to `change_log`.