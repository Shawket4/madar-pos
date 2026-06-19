This is a Flutter POS app with no Rust core yet — the task is to author the design for one as a section of PLAN.md. The generated API package confirms the wire models live in `packages/sufrix_api`. I have everything I need from the domain inventory. Let me write the design.

This is my return value, so I'll output the markdown directly.

---

# Local Store & Offline/Sync Engine — Sufrix POS Rust Core

> Scope: the `sufrix_core` Rust crate (compiled as a `cdylib`, bridged to Flutter over FFI). It owns a single embedded SQLite database (via `rusqlite` + `r2d2_sqlite` pool) that is **the source of truth the Dart UI reads from**, online or offline. The UI never talks to Dio for hot-path reads; it calls FFI read APIs that resolve entirely against SQLite. Writes on the selling hot path go into a durable **outbox** and are drained by a background sync worker.

## 0. Design tenets

| Tenet | Consequence |
|---|---|
| **Read = local-first** | Every `read-cache` GET is mirrored to a typed SQLite table. UI reads hit SQLite only. |
| **Write hot-path = outbox** | The 9 `outbox-mutation` ops (orders, void, delivery status/prep/finalize/cancel, shift open/close/cash-movement, waste) are enqueued locally, then replayed. |
| **Online-only = pass-through** | Back-office/admin ops bypass the store: FFI returns a typed "offline, not available" error; no local mirror, no queue. |
| **Client owns identity & time** | No server `Idempotency-Key` contract exists. The client generates UUID `client_temp_id`, an `idempotency_key`, and stamps real-event timestamps (`created_at`/`opened_at`/`closed_at`/`voided_at`). |
| **Money = integer minor-units** | `INTEGER` columns everywhere (piastres). `int32` vs `int64` width is a wire-deser concern only; SQLite `INTEGER` is 64-bit and absorbs both. **No `BigDecimal`-as-string in any teller-path domain.** |
| **Quantities = REAL** | `current_stock`, `quantity_used`, `quantity_ordered`, recipe qtys are doubles → SQLite `REAL`. `NULL` means *unknown*, never 0. |
| **Wire models are generated** | Mirror tables store the canonical wire JSON in a `payload` column + extract only the columns needed for indexing/filtering/merge. Deserialization uses the generated `sufrix_api` structs (Rust side), so untyped `*_translations`, free-form string "enums", and `serde(other)` fallbacks are handled once at the edge. |

---

## 1. SQLite schema strategy

### 1.1 Database-wide pragmas (set on every pooled connection)

```sql
PRAGMA journal_mode = WAL;        -- concurrent reader (UI) + writer (sync) 
PRAGMA synchronous  = NORMAL;     -- WAL-safe durability/perf tradeoff
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;       -- avoid SQLITE_BUSY under sync+UI contention
```

WAL is essential: the Dart UI reads while the background sync worker writes. The single writer is the sync worker; UI reads are snapshot-consistent.

### 1.2 Mirror tables (read-cache domains)

Two-layer pattern per mirrored entity:
- **Indexable columns** the UI filters/joins/merges on (id, branch_id, availability, soft-delete, sort keys, money/qty needed for merge math).
- **`payload BLOB`** — the full canonical wire JSON (the generated model round-trips through this), so we never lose fields and the FFI read API can hand the UI a complete object without a column per wire field.
- **Sync bookkeeping** — `server_seq`, `server_updated_at`, `deleted` (tombstone), `synced_at`.

**Generic mirror shape** (instantiated per entity):

```sql
CREATE TABLE menu_items (
  id              TEXT PRIMARY KEY,            -- server uuid
  org_id          TEXT NOT NULL,
  category_id     TEXT,                        -- nullable ([T,'null'])
  base_price      INTEGER NOT NULL,            -- minor-units (int32 wire)
  is_active       INTEGER NOT NULL DEFAULT 1,
  sort_order      INTEGER NOT NULL DEFAULT 0,
  payload         BLOB NOT NULL,               -- full MenuItemFull wire JSON
  server_seq      INTEGER NOT NULL,            -- monotonic change-feed cursor
  server_updated_at TEXT NOT NULL,             -- RFC3339, server-authoritative
  deleted         INTEGER NOT NULL DEFAULT 0,  -- tombstone (soft-delete via deleted_at)
  synced_at       TEXT NOT NULL
);
CREATE INDEX ix_menu_items_cat    ON menu_items(category_id) WHERE deleted = 0;
CREATE INDEX ix_menu_items_seq    ON menu_items(server_seq);
```

Entities that get a dedicated mirror table (one per `read-cache` op, same shape):

| Table | Source op(s) | Merge / filter notes |
|---|---|---|
| `me_session` (1 row) | `me`, `get_my_permissions` | tax_rate (REAL, real JSON number), currency_code, role; restores identity offline. |
| `permissions` | `get_my_permissions` | `(resource, action)` PK as **opaque strings**; `effective` bool gates UI. |
| `branches`, `branch_current` | `list_branches`, `get_branch` | printer_* nullable; `printer_brand` via `serde(other)`. |
| `orgs` | `get_org` | tax_rate REAL; receipt_footer/logo_url nullable. |
| `timezones` | `list_timezones` | static seed table; refreshed rarely. |
| `categories` | `list_categories` | filter `deleted=0` (soft-delete via `deleted_at`). |
| `menu_items`, `menu_item_sizes`, `addon_slots`, `optional_fields`, `addon_overrides` | menu reads | `price_override` INTEGER; size identity keyed by **label string** in override contexts → `(item_id,label)` composite key. |
| `addon_items`, `addon_catalog` | addon reads | `default_price` INTEGER. |
| `branch_menu_overrides`, `branch_addon_overrides` | override reads | **keyed by `(branch_id,item_id)` in body, no path id** — local PK must mirror that; `is_available` drives the layered sellable menu. |
| `bundles`, `bundle_components` | `list_bundles`, `available_bundles`, `get_bundle` | `status` enum `serde(other)`; `available_*_time` kept as TEXT (no time format). |
| `discounts` | `list_discounts` | `dtype` open string; `value` INTEGER. |
| `payment_methods` | `list_payment_methods` | `is_cash`/`is_active` gate tender UI. |
| `delivery_settings`, `delivery_zones`, `branch_tables` | delivery reads | zone `fee`/distances INTEGER; `name_translations` JSON in payload. |
| `orders`, `order_items`, `delivery_orders` | `list_orders`, `get_order`, `list_delivery_orders` | money INTEGER; `status`/`order_type`/`payment_method` opaque strings. **These rows can also originate locally — see §1.4 unified view.** |
| `shifts`, `cash_movements`, `shift_reports` | shift reads | `int32` (Shift) vs `int64` (Summary) reconciled by storing INTEGER; `revenue_by_method` lives in payload JSON. |
| `branch_stock`, `org_catalog`, `inventory_settings`, `drink_recipes`, `addon_ingredients` | inventory read-cache | quantities REAL; `cost_per_unit` REAL nullable (`null`=unknown). Recipes drive local stock-depletion math on each sale. |

**Layered sellable-menu view** (base catalog ⊕ branch overrides), computed in SQL so the UI gets one coherent list:

```sql
CREATE VIEW v_sellable_menu AS
SELECT m.id, m.category_id,
       COALESCE(o.price_override, m.base_price)      AS effective_price,
       COALESCE(o.is_available, 1)                   AS is_available,
       m.payload
FROM   menu_items m
LEFT JOIN branch_menu_overrides o
       ON o.item_id = m.id AND o.branch_id = (SELECT branch_id FROM me_session)
WHERE  m.deleted = 0
  AND  (o.is_available IS NULL OR o.is_available = 1);
```

### 1.3 The durable OUTBOX (command queue)

Single append-only command log for **all** `outbox-mutation` ops. This is the heart of the engine.

```sql
CREATE TABLE outbox (
  seq             INTEGER PRIMARY KEY AUTOINCREMENT,  -- global FIFO ordering
  id              TEXT NOT NULL UNIQUE,               -- this command's own uuid
  client_temp_id  TEXT,                               -- temp id of the entity this op creates (NULL for pure updates)
  op_type         TEXT NOT NULL,                      -- 'create_order' | 'void_order' | 'set_status' |
                                                      -- 'set_prep_time' | 'finalize_delivery_order' |
                                                      -- 'cancel_delivery_order' | 'open_shift' |
                                                      -- 'close_shift' | 'add_cash_movement' | 'create_waste'
  idempotency_key TEXT NOT NULL UNIQUE,               -- client-generated; replay-safe (X-Idempotency-Key header)
  payload         BLOB NOT NULL,                      -- wire request JSON (with client temp ids embedded)

  -- timestamps: distinguish the real event time from bookkeeping
  event_at        TEXT NOT NULL,                      -- client-stamped real event time (created_at/opened_at/voided_at...)
  enqueued_at     TEXT NOT NULL,                      -- when row was written locally (client clock)
  server_acked_at TEXT,                               -- server-authoritative ack time (NULL until acked)

  status          TEXT NOT NULL DEFAULT 'pending',    -- pending|inflight|acked|failed|dead|superseded
  attempts        INTEGER NOT NULL DEFAULT 0,
  next_attempt_at TEXT,                               -- backoff schedule
  last_error      TEXT,                               -- last failure (for diagnostics / dead-letter)
  last_http_status INTEGER,                           -- distinguish 4xx (terminal) vs 5xx/timeout (retry)

  server_id       TEXT,                               -- real server id after ack (reconciliation target)
  server_number   TEXT,                               -- e.g. order_number assigned by server

  depends_on_seq  INTEGER,                            -- FK to another outbox.seq this op must wait for
  FOREIGN KEY (depends_on_seq) REFERENCES outbox(seq)
);
CREATE INDEX ix_outbox_drain   ON outbox(status, seq) WHERE status IN ('pending','failed');
CREATE INDEX ix_outbox_temp    ON outbox(client_temp_id);
```

**`id_map`** — the durable bridge between client temp ids and server ids. Survives across restarts so late-arriving dependent ops and UI references resolve correctly:

```sql
CREATE TABLE id_map (
  entity_type   TEXT NOT NULL,        -- 'order' | 'shift' | 'delivery_order' | 'cash_movement' | 'waste'
  client_temp_id TEXT NOT NULL,
  server_id     TEXT,                 -- NULL until acked
  server_number TEXT,
  resolved_at   TEXT,
  PRIMARY KEY (entity_type, client_temp_id)
);
CREATE UNIQUE INDEX ix_id_map_server ON id_map(entity_type, server_id) WHERE server_id IS NOT NULL;
```

### 1.4 Unified read model for locally-created entities

A teller who creates an order offline must immediately see it on the shift screen. So the mirror tables (`orders`, `shifts`, `cash_movements`, `delivery_orders`) carry an **optimistic local row** the moment the outbox command is enqueued, in the same transaction:

```sql
ALTER TABLE orders ADD COLUMN origin     TEXT NOT NULL DEFAULT 'server';  -- 'server' | 'local'
ALTER TABLE orders ADD COLUMN local_state TEXT;       -- 'pending'|'synced'|'rejected' (mirrors outbox status)
ALTER TABLE orders ADD COLUMN client_temp_id TEXT;    -- links to id_map / outbox
```

The UI reads orders with a stable predicate; local-pending rows show a "syncing" chip. On ack, the local row's `id` is rewritten to `server_id`, `origin` stays for audit, `local_state='synced'`.

### 1.5 Sync cursor / checkpoint table

One row per sync stream so each domain catches up independently after days offline.

```sql
CREATE TABLE sync_cursors (
  stream          TEXT PRIMARY KEY,    -- 'menu' | 'catalog' | 'orders' | 'shifts' | 'inventory' | 'identity' | 'delivery'
  last_server_seq INTEGER NOT NULL DEFAULT 0,  -- monotonic change-feed position acked by server
  last_pulled_at  TEXT,                -- server-authoritative high-water timestamp
  full_resync_needed INTEGER NOT NULL DEFAULT 1, -- 1 → do a bootstrap snapshot first
  etag            TEXT                 -- optional, if server supports conditional GET
);

CREATE TABLE sync_meta (               -- singleton diagnostics row
  k TEXT PRIMARY KEY, v TEXT
);  -- 'server_time_skew_ms', 'last_drain_at', 'last_pull_at', 'connectivity'
```

---

## 2. Migration approach (Rust)

**Tool: `refinery` with embedded, versioned, forward-only SQL migrations** (`migrations/V001__init.sql`, `V002__add_outbox_dlq.sql`, …), embedded via `refinery::embed_migrations!`. Run inside a transaction on core init, **before** the connection pool is exposed.

```rust
mod embedded { refinery::embed_migrations!("migrations"); }

pub fn open_store(path: &Path) -> Result<Store> {
    let mut conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    embedded::migrations::runner().run(&mut conn)?;   // transactional, idempotent
    Ok(Store::from(conn))
}
```

Why refinery over hand-rolled:
- Tracks applied versions in its own `refinery_schema_history`; refuses to re-run or run out of order.
- Migrations are plain `.sql`, reviewable in PRs alongside the matching backend migration.
- Works offline at app start with zero network.

**Staying in sync with backend schema changes** — the local schema is *not* the backend schema; it's a cache shaped by the **generated wire models**. The contract:

1. Backend OpenAPI changes → run `./tool/generate_api.sh` (regenerates `packages/sufrix_api`, and the Rust `sufrix_api` model crate).
2. **Mirror tables store full wire JSON in `payload`**, so additive wire fields (new optional field, new translation key) need **no migration** — they flow through transparently. A migration is only needed when a *new column* must be indexed/filtered/merged on (e.g. a new `is_featured` flag the grid sorts by).
3. CI guard: a test deserializes a captured server sample through the generated model **and** re-serializes from `payload` to assert round-trip fidelity. If the backend renames/removes a field used as an extracted column, this test fails and forces a `Vxxx` migration.
4. **Defensive deser is mandatory** so a newer backend never bricks an un-updated POS:
   - closed enums (`BundleStatus`, `RunStatus`, `UserRole`, `PrinterBrand`, `Decision`, `SuggestionKind`) → `#[serde(other)] Unknown` fallback.
   - open strings (`order_type`, `payment_method`, `status`, `dtype`, `addon_type`, `channel`, `movement_type`, `variance_reason`) → plain `String`, never Rust enums.
   - PATCH update bodies → `#[serde(skip_serializing_if = "Option::is_none")]` so we never null out `printer_ip`/`role`/`logo` (absent-vs-null trap). On the POS this matters for any future local→server PATCH; the teller path has none, but the rule is enforced crate-wide.
   - `*_translations`, `revenue_by_method` → `serde_json::Value`.

A schema-version mismatch the migration runner can't resolve (DB newer than binary) triggers a **safe degrade**: refuse outbox drains, force `full_resync_needed`, surface a `NeedsUpgrade` status event over FFI rather than corrupting data.

---

## 3. Read-through caching flow

The UI's read APIs are served **entirely from SQLite**. There is no synchronous network read on any hot path.

```
Dart UI  ──FFI──►  core::read_*(query)  ──►  SQLite (mirror tables / views)  ──►  typed JSON back to Dart
                                                  ▲
                          background sync worker  │  (writes mirrors; UI never blocks on it)
                          pull loop ──HTTP──► server delta feed
```

Flow per read:
1. UI calls e.g. `core_list_sellable_menu(branch_id)` over FFI.
2. Core queries `v_sellable_menu` (base ⊕ overrides, tombstones filtered), returns rows as a JSON array. **Always succeeds offline**; freshness comes from whatever the last pull wrote.
3. Independently, the sync worker's pull loop refreshes mirrors via delta sync (§5) and emits a `DataChanged{stream}` event (§7) so the UI re-queries. This is **stale-while-revalidate**: the screen renders instantly from cache, then updates when fresh data lands.

For `online-only` reads (reports, costing, admin lists): the FFI read returns `Err(Offline)` when disconnected; the UI shows "requires connection." These are never cached and never block the selling path.

**Cache invalidation** is driven entirely by the change feed: a mirror row is replaced when a delta with a higher `server_seq` arrives; deletes arrive as tombstones (set `deleted=1`, keep the row so dependent local reads resolve, GC after a retention window).

---

## 4. Outbox lifecycle

### 4.1 State machine

```
            enqueue (UI, in same txn as optimistic mirror row)
                │
                ▼
            pending ──drain picks up──► inflight ──HTTP 2xx──► acked ──reconcile──► (terminal, GC after retention)
                ▲                          │
                │ 5xx / timeout (backoff)  │ 4xx validation (terminal)
                └──────── failed ◄─────────┤
                                           ▼
                                         dead  (dead-letter; surfaced to manager UI)
            superseded ◄── compaction (e.g. void cancels a not-yet-synced create)
```

### 4.2 Enqueue → persist (atomic)

A single SQLite transaction writes **three** things so the UI is instantly consistent and the queue is durable:

```rust
let tx = conn.transaction()?;
// 1. optimistic mirror row (so the order shows on the shift screen immediately)
tx.execute("INSERT INTO orders(id, origin, local_state, client_temp_id, ...) VALUES(?,'local','pending',?, ...)", ..)?;
// 2. id_map placeholder
tx.execute("INSERT INTO id_map(entity_type, client_temp_id) VALUES('order', ?)", ..)?;
// 3. the durable command
tx.execute("INSERT INTO outbox(id, client_temp_id, op_type, idempotency_key, payload, event_at, enqueued_at, depends_on_seq, status)
            VALUES(?,?,?,?,?,?,?,?, 'pending')", ..)?;
tx.commit()?;
```

The `payload` already contains the client-stamped `created_at`/`opened_at`/`voided_at` so offline replay preserves true event time (the spec explicitly supports client-supplied timestamps on order/shift/cash-movement creates).

### 4.3 Drain on connectivity

`connectivity_plus` (Dart) → FFI `set_online(true)` → wakes the drain loop (also runs on a timer + after each successful pull). Drain algorithm:

```
loop:
  rows = SELECT * FROM outbox
         WHERE status IN ('pending','failed')
           AND (next_attempt_at IS NULL OR next_attempt_at <= now)
           AND (depends_on_seq IS NULL OR depends_on_seq resolved in id_map)
         ORDER BY seq ASC            -- strict FIFO; never reorder
  for row in rows:
     resolve temp-ids in row.payload from id_map (§4.5)  -- a dependent op's parent is now real
     mark inflight
     POST with header X-Idempotency-Key: row.idempotency_key
     match response:
        2xx → ack(row, body)            -- §4.4
        409/200-duplicate → treat as ack (idempotent replay hit)  -- §4.6
        4xx (validation) → status=dead, last_error, emit OutboxRejected  -- terminal, do NOT block queue head forever
        5xx/timeout/offline → status=failed, attempts++, next_attempt_at = backoff(attempts)
        break loop on offline   -- stop draining, wait for reconnect
```

**Ordering guarantees:**
- Single drain worker, `ORDER BY seq ASC` → **global FIFO**. A shift must open before its orders; an order must be created before it's voided; this falls out of insertion order naturally.
- `depends_on_seq` makes cross-entity ordering explicit and lets independent chains proceed when one stalls. Head-of-line blocking is bounded: a 4xx-`dead` command is removed from the active set (its dependents are also marked `dead`/needs-attention) so one poisoned command can't freeze the whole queue. 5xx just backs off and retries in order.

### 4.4 Server ack → reconcile temp-id ↔ server_id

On 2xx the server returns the real entity (`OrderFull` with `id`/`order_number`, or `Shift` with `id`). In **one transaction**:

```sql
UPDATE outbox SET status='acked', server_id=?, server_number=?, server_acked_at=?, last_error=NULL WHERE seq=?;
UPDATE id_map SET server_id=?, server_number=?, resolved_at=? WHERE entity_type=? AND client_temp_id=?;
-- promote the optimistic row to the real id
UPDATE orders SET id=?, local_state='synced', server_seq=?, server_updated_at=? WHERE client_temp_id=?;
-- rewrite FKs in already-mirrored children
UPDATE order_items SET order_id=? WHERE order_id=<temp>;
```

Also fold in server `warnings[]` (oversell flags) onto the local row for display. The server's `created_at`/timestamps are authoritative and overwrite the optimistic ones on the synced row (clock-skew correction, §5).

### 4.5 Rewrite dependent queued ops

The hard case: a command enqueued offline references an entity that **only existed as a temp id**. Examples on this hot path:
- `create_order` carries `shift_id`. If the shift was opened offline, `shift_id` is a `client_temp_id` until the `open_shift` command acks.
- `void_order` / delivery `set_status` / `finalize` reference an `order_id` that may still be a temp id.
- `add_cash_movement` references the shift.

Two mechanisms keep this correct:

1. **`depends_on_seq`** — `create_order` is enqueued with `depends_on_seq` = the `open_shift` command's seq. The drain won't attempt it until that dependency resolves.
2. **Late temp-id substitution at send time** — just before POST, every command's `payload` is passed through `resolve_temp_ids(payload, id_map)`: any field whose value matches an unresolved-then-resolved `client_temp_id` is swapped for the real `server_id`. Because resolution happens at send time (not enqueue time), a parent acked moments earlier is already in `id_map`.

If a dependency went `dead` (parent rejected), its dependents are cascaded to `needs_attention` rather than sent with a dangling reference.

### 4.6 Exactly-once via idempotency keys

The spec notes **no server-side `Idempotency-Key` contract exists** on any create endpoint — the client owns it. Strategy:

- Each command generates a stable `idempotency_key` at enqueue (UUIDv4), stored uniquely. It is sent as `X-Idempotency-Key` on every retry of that command.
- **At-least-once delivery + idempotent server handling = effectively-once.** The dangerous window is "server committed, ack lost, client retries." Mitigations:
  - The `client_temp_id` / client-supplied `id` (orders, shifts, cash-movements accept a client uuid) is the natural dedup key: a retry with the same client id should be a no-op upsert server-side returning the existing entity. The drain treats a `409 Conflict` or a duplicate-200 as a successful ack and reconciles from the returned entity.
  - Until the backend honors `X-Idempotency-Key`, this client-id-as-dedup is the primary guard; the header is sent forward-compatibly so it activates for free once the server contract lands.
- **Suppress stale side-effects on late replay:** `set_status`/`finalize` fire WhatsApp / customer tracking. A command replayed hours later could fire a stale "your order is ready" notification. The command payload carries `event_at`; the server (or a client `X-Suppress-Notifications-If-Older-Than` hint) can skip the notification when the event is stale. Locally we also mark such acks so the UI doesn't re-toast.

---

## 5. Delta / cursor pull (catch-up after days offline)

Each `read-cache` stream has a `sync_cursors` row with a monotonic `last_server_seq`. The pull is a **change feed**, not a full GET, so a teller offline for days catches up incrementally.

```
for stream in active_streams:
   cur = sync_cursors[stream]
   if cur.full_resync_needed:
        snapshot = GET /<stream>?limit=N        -- paginated bootstrap
        upsert all rows; set full_resync_needed=0
   loop:
        page = GET /<stream>/changes?since_seq=cur.last_server_seq&limit=500
        for change in page.items:
            if change.deleted: UPSERT row SET deleted=1 (tombstone), server_seq, server_updated_at
            else:              UPSERT mirror row (payload + extracted cols), server_seq, server_updated_at
        cur.last_server_seq = page.max_seq
        cur.last_pulled_at  = page.server_time     -- server-authoritative
        if page.items < 500: break
   emit DataChanged{stream}
```

Key properties:
- **Monotonic sequence** = the catch-up cursor. The client only ever asks "what changed since seq X," so reconnect cost is proportional to *changes missed*, not catalog size.
- **Tombstones for deletes** — deletes arrive as `deleted=true` change records (covers soft-delete `deleted_at` on categories/menu items and hard deletes of overrides/zones). We set `deleted=1` and keep the row briefly so any locally-queued op referencing it can still resolve, then GC past a retention horizon.
- **Server-authoritative timestamps for clock skew** — on every ack/pull the server's time is read from a header/field; `sync_meta.server_time_skew_ms = server_time − client_time` is recorded. All *display* and *ordering* uses server time when available; client time is used only to stamp the *real event time* of offline actions (and is corrected to the server's recorded value once the create acks). This prevents a teller's wrong device clock from mis-sequencing the shift report.
- The pull and the drain are coordinated: **drain outbox first, then pull** on reconnect, so the change feed already reflects the teller's own just-synced writes (no flicker of a locally-created order vanishing then reappearing).

If the backend exposes no `/changes` feed yet, the interim is `GET ?updated_after=<last_pulled_at>` using server `updated_at` ordering + a tombstones endpoint; the cursor table abstracts which mechanism is used so the swap is internal.

---

## 6. Conflict strategy per domain

| Domain | Ownership | Conflict risk | Strategy |
|---|---|---|---|
| **Orders / void / delivery status** | Teller-owned, append-only | **Low** — each order is created once by one teller; status advances forward only | **No merge.** Outbox replay + idempotency. The local order is authoritative until acked, then server copy wins (server assigns id/number/totals). `set_status` is forward-monotonic; a replay that arrives after the order already advanced is a no-op (server idempotent on step). |
| **Shifts / cash-movements** | Teller-owned, single open shift per branch | **Low**, except concurrent open | Client-supplied shift `id` + idempotency. If two devices open a shift for the same branch offline, the server enforces one-open-shift; the loser's `open_shift` returns a conflict → mark `dead`, surface to manager, re-home its orders under the winning shift via `id_map` remap. |
| **Waste** | Teller-owned event | **Very low** — append-only event log | Outbox + idempotency, like orders. No merge. |
| **Menu / catalog / pricing / bundles / discounts / payment methods / branch overrides** | **Dashboard-owned** (admin), POS is read-only mirror | Conflict only between admin edits; POS never writes | **Last-write-wins by `server_seq`.** POS unconditionally accepts the higher-seq server version on pull. No local edits → no merge needed. Price drift between an offline-captured order and current menu is acceptable: the order froze its `unit_price` at sale time (sent in the outbox payload), so a later price change doesn't retroactively alter past sales. |
| **Inventory stock levels (`branch_stock`)** | Server-derived (depletes on sale) + admin edits | **Medium** — POS applies *local optimistic depletion* per offline sale via recipes; server recomputes authoritatively | **Server-authoritative reconcile, not LWW-overwrite-blind.** Local applies provisional `current_stock` decrements (recipe math) so the low-stock UI is roughly right offline. On reconnect: the server's post-replay stock value (after it processes the queued orders) is the truth and replaces the local provisional value. We never push local stock numbers; we only push the *orders/waste* that cause depletion and let the server be the ledger. |
| **Stock counts / stocktakes** | Manager-owned, `online-only` | N/A on hot path | Not in outbox; online-only. If ever moved offline, stock counts need **explicit merge** (variance reconciliation), never silent LWW — a count is an assertion about reality at time T, so it must carry `counted_at` and be reconciled against intervening movements server-side. Flagged here as the one place LWW would be wrong. |
| **Identity / permissions / org / branch config** | Server-owned | Low | LWW by seq; POS read-only. Session JWT cached so an already-logged-in teller survives days offline; `login`/`resolve-branch` stay online-only (can't start a new session offline). |

Rule of thumb encoded in the engine: **teller-path entities are append-only and conflict-free by construction** (unique client ids, forward-only status, frozen prices); **everything the dashboard owns is pulled LWW**; **the single genuine merge case (stock counts) is deliberately kept online-only.**

---

## 7. Sync status events across FFI

The core pushes status to Dart so the UI can render sync chips, banners, and dead-letter alerts without polling.

**Transport:** an `allo-isolate` (or flutter_rust_bridge `StreamSink`) channel: the core holds a `Sender<SyncEvent>`; Dart subscribes once at startup and gets a broadcast `Stream<SyncEvent>` exposed through a Riverpod `StreamProvider`.

**Event enum (serialized as tagged JSON):**

```rust
#[serde(tag = "type")]
pub enum SyncEvent {
    Connectivity   { online: bool },
    DrainStarted   { pending: u32 },
    DrainProgress  { op_type: String, remaining: u32 },
    OutboxAcked    { op_type: String, client_temp_id: String, server_id: String, server_number: Option<String> },
    OutboxRejected { op_type: String, client_temp_id: String, http_status: u16, message: String }, // dead-letter → manager alert
    DataChanged    { stream: String },          // UI re-queries affected mirror tables
    PullProgress   { stream: String, applied: u32, behind: u32 },
    IdReconciled   { entity_type: String, client_temp_id: String, server_id: String }, // UI swaps the temp id it was showing
    ClockSkew      { skew_ms: i64 },
    NeedsUpgrade   { reason: String },          // DB newer than binary / unresolvable migration
    Error          { stream: Option<String>, message: String },
}
```

Usage:
- `DataChanged{stream}` → the matching Riverpod provider invalidates and re-reads from SQLite (drives stale-while-revalidate).
- `OutboxAcked` / `IdReconciled` → the optimistic order row flips from "syncing" to "synced," and any open detail screen swaps the temp id for the real `order_number`.
- `OutboxRejected` → a persistent banner / manager dead-letter screen reads the `dead` outbox rows; nothing is silently dropped.
- `Connectivity` + `DrainProgress` → the global sync indicator.

FFI also exposes **pull, non-streaming, query functions** for the dead-letter / sync-health screen: `core_outbox_pending_count()`, `core_outbox_dead_list()`, `core_sync_cursors()`, `core_retry_dead(seq)` (manager-initiated re-queue after fixing the cause).

---

### Notes for the rest of PLAN.md

- No existing Rust core is present in the repo yet; the current Flutter app has hand-written wire facades in `lib/core/models/` and a generated Dart client in `packages/sufrix_api`. This engine adds a Rust `sufrix_core` crate (new `rust/` workspace) that the Flutter app loads over FFI, replacing direct Dio reads on the hot path while Dio remains the transport the core's own sync worker uses internally.
- `lib/core/models/pending_action.dart` already hints at an outbox concept on the Dart side; it should be retired in favor of the durable Rust `outbox` table so the queue survives process death and is drained off the UI isolate.
- The four teller-path money widths (`int32` Shift vs `int64` ShiftSummary, etc.) are a *wire-deser* concern only; all SQLite money columns are 64-bit `INTEGER`, so no local truncation risk.