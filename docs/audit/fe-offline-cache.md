I now have a complete picture. Critical reconciliation detail confirmed: for queued orders the `localId` is a fresh UUID used as BOTH the optimistic order id and the idempotency key, while the online path uses `cart.idempotencyKey()` (cart id or timestamp string) — two different key schemes. The optimistic order is inserted with `id == localId` and `replaceOrder(localId, synced)` swaps it for the server order.

Here is the audit.

# Madar POS — Offline + Caching Engine Audit

Single local DB file `madar-pos.db` (sqflite; FFI on desktop/web, native on mobile), schema **version 2**. Two cooperating subsystems: a **durable write outbox** (`outbox` table + `OfflineQueueNotifier`) and a **read-through cache** (KV blobs in the `kv` table + `sync_meta` TTL timestamps, fronted by per-domain repositories). Menu images cache separately on disk via `flutter_cache_manager`.

## 1. Local SQLite schema (`core/db/app_database.dart`)

| Table | Columns | Purpose |
|---|---|---|
| `kv` | `k` TEXT PK, `v` TEXT NOT NULL, `ts` INTEGER NOT NULL | General key→JSON store (replaces SharedPreferences). `ts` = last-write epoch-ms, doubles as the cache-write timestamp (`menuCachedAt` reads it back). Fully hydrated into memory at boot by `KvStore.init()`; **reads are synchronous from the in-memory map**, writes go to both map and table. |
| `outbox` | `local_id` TEXT PK, `type` TEXT, `payload` TEXT (JSON of `PendingAction.toJson`), `status` TEXT, `depends_on` TEXT (nullable), `retry_count` INT DEFAULT 0, `last_error` TEXT, `created_at` INT, `next_attempt_at` INT (0 = ready now), `user_id` TEXT (v2), `synced_at` INT (v2) | Durable write-ahead queue for offline mutations. Index `idx_outbox_status(status, next_attempt_at)`. |
| `sync_meta` | `entity` TEXT PK, `last_synced_at` INTEGER | Per-domain freshness timestamps for TTL staleness checks. Key format `"<domain>:<scope>"` (e.g. `menu:org:branch`, `orders:<shiftId>`, `inventory:<branchId>`). |

**Migration (v1→v2):** `ALTER TABLE outbox ADD COLUMN user_id`, `ADD COLUMN synced_at`; resets any `in_flight` rows to `pending`. There is no v1→v2 migration that touches `kv`/`sync_meta` (they were unchanged). `createSchema` test helper hardcodes version 2.

**Not in SQLite (relevant boundaries):**
- JWT lives in the platform keychain (`SecureTokenStore`), never in `kv`.
- All cached *domain data* (menu, orders, shifts, inventory, drafts, carts, discounts, payment methods, offline-unlock PIN hashes) are JSON blobs inside the `kv` table — there are **no relational tables for business entities**. Everything is opaque JSON keyed by string.
- Menu images: separate on-disk store `madar_menu_images` with its own `JsonCacheInfoRepository` sqlite db (managed by `flutter_cache_manager`, not `AppDatabase`).

## 2. Outbox model & lifecycle

**Op types queued** (`PendingActionType`): `shiftOpen`, `order`, `shiftClose`, `voidOrder`, `cashMovement`. Note `cashMovement` is **never actually queued** — `enqueueCashMovement` throws `OfflineCashMovementError` when offline and fires immediately when online (no server idempotency key → refuses to risk a double-apply). So the durable queue is effectively 4 op types.

**Ordering:** strictly `created_at ASC` (`dueForSync`, `loadAll`). Causal ordering enforced on top of FIFO by two mechanisms:
- `depends_on` (a prerequisite `local_id`): orders/closes/voids attach to a live `shiftOpen` for the same `shift_id`; a void attaches to the still-queued order it targets. Gating in `_runDrain`: if prereq is `pending`/`in_flight` → skip this pass; if prereq is `dead` → mark this entry `dead` too; if prereq is `synced`/discarded(null) → proceed.
- Shift-close barrier: a `shiftClose` is skipped every pass until `hasLiveOrdersOrVoids()` returns false (no pending/in_flight order or void remains).

**Idempotency key:** for queued orders the **order's own `local_id` (a fresh UUID) is passed as `idempotencyKey`** to `orderApi.create`. shiftOpen reuses a **client-generated `shift_id` UUID** via `openWithId` (replaying the same id returns 200). void/shiftClose rely on **server-side idempotency** of those endpoints (replays treated as success). cashMovement has **no idempotency key at all** (the reason it's online-only).

**Retry / backoff** (`offline_queue.dart`): max 8 counted retries (`_kMaxRetries`). Backoff `2000ms * 2^(retry-1)`, capped at 300000ms (5min), plus `rand(0..1000ms)` jitter. Network blips use a **fixed 15s reschedule that does NOT consume retry budget** (`markRetryNoCount`). A periodic 15s timer plus connectivity-online events plus enqueue calls all trigger `_drain`.

**Status lifecycle:**

| Status | Set by | Meaning / transition |
|---|---|---|
| `pending` | `insert` (initial), `markRetry`, `markRetryNoCount`, `resetRetry`, `recoverInFlight` | Eligible for `dueForSync` once `next_attempt_at ≤ now`. |
| `in_flight` | `markInFlight` (just before API call) | Active attempt. Recovered → `pending` on `init()` and in the drain `finally` (crash-safety net). |
| `synced` | `markSynced` (+ `synced_at`) | Success. Row **kept as a recovery log**, dropped from in-memory view; purged after 48h (`_kSyncedRetentionMs`). |
| `dead` | `markDead` | Permanent failure: retries exhausted, or terminal HTTP (400/403/422; 409 for order/shiftOpen; 404 for void). Stays in the table & "stuck" list until user discards/resets. |
| (deleted) | `discard` | User-initiated permanent delete. |

**HTTP → outcome mapping in `_runDrain`:**

| Response | order | shiftOpen | void | shiftClose |
|---|---|---|---|---|
| 2xx | synced | synced | synced | synced |
| 401 | park whole queue (`authPaused`), `markRetryNoCount`, **no budget burned** | same | same | same |
| 404 | treat as applied → synced | applied → synced | **dead** ("order not found") | applied → synced |
| 409 | **dead** (shift was closed; cash unrecorded) | **dead** + `onShiftOpenRejected` (phantom shift) | applied → synced | applied → synced |
| 400/403/422 | dead | dead | dead | dead |
| network err | `markRetryNoCount` +15s | same | same | same |
| other 5xx/unknown | counted retry w/ backoff | same | same | same |

**Temp-id ⇄ server-id reconciliation:**
- **Order:** optimistic order is built with `id == localId` and inserted into history. On sync, `onOrderSynced(serverOrder, localId)` → `OrderHistoryNotifier.replaceOrder(localId, serverOrder)` swaps the row where `o.id == localId`, re-persists the order cache, and refreshes `systemCash`. Server idempotency keyed on that same `localId` prevents double-charge on retry.
- **Shift:** there is **no temp-id swap** — the client generates the canonical `shift_id` UUID up front and the server adopts it via `openWithId`. `onShiftOpenSynced` only calls `updateShiftSynced` (clears the `isLocalShift` phantom flag). Rejected (409) → `handleOpenRejected` clears the phantom shift.
- **Correct-at-sync timestamps:** each payload stamps `clock_offset_ms` (the device-clock offset at creation); at drain, every timestamp is re-based to the fresh server offset (`rebase` in `_processEntry`), so a constant clock skew is corrected at sync time.

## 3. Read-through cached domains + refresh strategy

All caches are **full-snapshot replace** (overwrite the whole blob); the only delta-shaped param in the codebase (`bundles updated_since`) is defined in the API but **never passed** by `MenuRepository.fetchBundlesFresh`. Pattern everywhere: synchronous local paint → background `fetchXFresh` → overwrite KV + bump `sync_meta`. No invalidation beyond TTL + forced refresh; nothing is ever deleted on a delta.

| Domain | KV key | `sync_meta` key | TTL | Refresh strategy |
|---|---|---|---|---|
| Menu (categories + items) | `menu_v2_<org[:branch]>` | `menu:<scope>` | 10 min | Full refetch (`categories` + `items` in parallel), branch-scoped. Stale OR `force` OR no-cache → fetch. |
| Bundles | `bundles_v1_<org>` | `bundles:<org>` | 10 min | Full refetch. `updated_since` param exists but unused → always full. |
| Addons | `addons_<scope>` | `addons:<scope>` | 10 min | Full refetch. |
| Single menu item | `menu_item_<id>` | — | — | On-demand fetch; falls back to cache on network error. |
| Current shift | `shift_<branchId>` | `shift:<branchId>` | 5 min default | Full refetch; cache removed when no open shift. |
| Shifts list (history) | `shifts_list_<branchId>` | `shifts:<branchId>` | 5 min | Paginated (page size 20). `fetchShiftsPage` bumps meta but does **not** write cache; caller accumulates and calls `cacheShifts`. |
| Inventory | `inventory_<branchId>` | `inventory:<branchId>` | 5 min | Full refetch. |
| Orders (per shift) | `orders_<shiftId>` | `orders:<shiftId>` | 5 min | Full refetch; also written optimistically on order place/replace. |
| Delivery orders | `delivery_orders_<branchId>` | `delivery_orders:<branchId>` | 5 min | Full refetch; plus a live SSE stream (`openStream`) for realtime updates. |
| Shift report | `shift_report_<shiftId>` | — | — | On-demand; cache fallback on error. |
| Recipe preview | `recipe_preview_<key>` | — | — | On-demand. |
| Discounts / Payment methods | `discounts_<org>` / `payment_methods_<org>` | — | — | Cached blobs, no TTL meta. |
| Device config, cached user, branch, offline-unlock PIN hashes/user | `device_*`, `cached_user`, `branch_<id>`, `offline_unlock_*`, `offline_user_*` | — | Config/identity; offline-unlock keys deliberately survive logout. |
| Draft carts / active cart | `draft_carts_v1_<scope>` / `active_cart_v1_<scope>` | — | Client-only state, scope = `<branchId>_<shiftId>` (`cartStorageScope`). Never synced — purely local. |
| **Menu images** | on-disk (`flutter_cache_manager`) | — | stalePeriod 365 days, max 500 objects | Warmed (`warmUp`) after each fresh menu load; only evicted on `invalidate()` during a forced refresh. |

Freshness is surfaced to UI as `DataFreshness.live / stale / offline`; the top bar shows `menuCachedAt` (the KV `ts`).

## 4. Connectivity transitions & drain triggers (`connectivity_service.dart`)

- **Oracle** = HTTP `GET /health` (own Dio, 5s timeouts) every 10s + on every interface change, debounced. Goes **offline** after **2 consecutive failures** (`reportNetworkFailure` from Dio interceptors or failed pings); goes **online immediately** on the first success (`reportSuccess`). Hardware "no interface" event forces offline immediately (sets failure count to threshold).
- Emits only on actual edge (`_emit` dedupes); broadcast stream consumed via `connectivityStreamProvider` / `isOnlineProvider`. Default state is optimistic (`_isOnline = true`).
- **What triggers a drain (`OfflineQueueNotifier`):**
  1. Connectivity stream → `online == true` ⇒ `_drain()`.
  2. Always-on `Timer.periodic(15s)` ⇒ `_drain()`.
  3. Immediately at `init()` if already online.
  4. Every `enqueueX` call.
  5. `resumeAfterAuth()` (post-login) ⇒ `syncAll()` (two passes).
  6. `resetRetry` / manual "Sync Now" (`syncAll` = drain twice).
- **Single-flight:** `_drain` returns the in-flight future if one exists; bails if `authPaused` or offline. `syncAll` runs two sequential drains so changes made mid-loop get picked up. `isSyncing` only toggles when there's actual work (avoids rebuilding watchers on idle ticks).

## What moves to Rust

- **The whole outbox engine** — schema, `OutboxDao` CRUD, `OfflineQueueNotifier` drain loop, dependency/barrier gating, backoff, the full HTTP-status → lifecycle decision table, crash recovery (`recoverInFlight`), 48h synced-log purge, per-user scoping. This is the highest-value, most-tested, hardest-to-reimplement piece; the status machine and the HTTP-code semantics (esp. the 409/404 money-loss distinctions) must move **verbatim**.
- **Idempotency-key generation** (order `local_id` UUID = idempotency key; client-minted `shift_id` via `openWithId`) and **correct-at-sync timestamp rebasing** (`clock_offset_ms`).
- **Connectivity oracle** (`/health` ping cadence + 2-failure debounce + interface watch) — Rust can own this and expose an online/offline signal + a drain trigger.
- **`sync_meta` TTL/freshness logic** and the read-through repository pattern (local paint + background fetch + overwrite + bump). The KV blob store can move too, but see gaps.
- **Reconciliation hooks** must be re-exposed to the Flutter UI as events: order temp-id→server-id swap, shift-open-synced, shift-open-rejected (phantom clear), shift-close-synced, void-synced. These currently live as `Function?` callbacks wired in `main.dart`; in Rust they become emitted events/streams the Dart layer subscribes to.

**Stays in Dart (do not move):** draft/active carts (`cartStorageScope`, `DraftCartsNotifier`) are pure local UI state never synced; menu image disk cache (`flutter_cache_manager`) is platform-bound; `MenuImage` widget logic.

## Gaps / risks found

1. **No deltas / cursors anywhere.** Every cache refresh is a full refetch-and-replace. `bundles updated_since` is the only delta-shaped param and it is **defined but never passed** (`fetchBundlesFresh` ignores it). There is no `since`/cursor column, no tombstone handling — a server-side *delete* is only reflected when the full list is refetched and overwrites the blob. Rust port is the chance to add real incremental sync, but be aware the backend contract for it is currently unused/untested.

2. **Drain ordering is not transactional & not crash-atomic across the network boundary.** Sequence per entry is: `markInFlight` → API call → on success `onSynced` callback (mutates Riverpod + writes the order cache blob) → `markSynced`. A crash **between server ack and `markSynced`** leaves the row `in_flight`; recovery re-marks it `pending` and it **re-sends**. That's safe for order/shiftOpen (idempotency key/shift_id replay → 200) and for void/shiftClose (server idempotent), but the **`onOrderSynced` side effects (history swap, cache write, systemCash) can run twice**, and worse, the optimistic-replace assumes `o.id == localId` — after the first swap the row id is the server id, so a re-run's `replaceOrder(localId,…)` misses and falls through to `addOrder` ⇒ **possible duplicate order in the local history/cache** on crash-replay.

3. **Cash-movement double-apply hole is real, just deferred.** cashMovement has no idempotency key, so it's online-only and fires directly. If an online `addCashMovement` dies on an **ambiguous timeout** (server applied, client saw network error), nothing retries — but the *call site* could; there's no protection. Backend needs an idempotency key here before this can ever be queued.

4. **`menuCachedAt` scope bug in the refresh path.** In `menu_notifier.dart` phase 2, the post-fetch read is `menuCachedAt(orgId)` (org only) while everywhere else uses `menuCachedAt(scope)` (org:branch). On a branch-bound device the "last synced" timestamp shown after a refresh reads the wrong key and will typically be null/stale. Minor (display only) but carry the correct scope into Rust.

5. **Two different idempotency schemes for the same order.** Online path uses `cart.idempotencyKey()` = `cart.id ?? "order_<ms>"`; the queued path uses a fresh `Uuid().v4()` as the key. If an online attempt partially succeeds and the code later falls back to `placeQueued()`, the two attempts carry **different idempotency keys** → the server cannot dedupe them ⇒ **double order risk** on the online-fail→queue fallback. The Rust port should unify on one stable key generated once at checkout.

6. **`isStale` is best-effort, not gated by success.** `_bumpSyncMeta` is only called on successful fetch, good — but `dueForSync`/repository reads never check connectivity transactionally; a fetch that throws after partial KV writes can leave the blob updated without the meta bumped (or vice-versa) since the two writes aren't in one transaction.

7. **Per-user queue scoping leaks legacy rows.** `dueForSync` includes `user_id IS NULL` rows for *every* user (legacy compat). On a shared till, a pre-v2 queued action would sync under whoever happens to be logged in → **mis-attribution**. Fine going forward, but the Rust migration should decide whether to backfill or drop NULL-user rows.

8. **`hasLiveOrdersOrVoids` ignores `depends_on` chains and dead-prereq deadlock subtlety.** A `shiftClose` waits on any live order/void globally for the table, not just its own shift — multiple concurrent shifts on one device (rare but possible across users) could delay an unrelated close. Also a void whose target order went `dead` is itself force-killed (`dead`), correct, but the **cash for that order is already in the drawer with no server record** — surfaced only via the stuck list, not reconciled.

9. **Connectivity oracle uses a separate unauthenticated Dio with no base-path validation** and a fixed `/health`; a captive portal returning 200 to `/health` would report online and then burn 401/real failures in the drain. Acceptable, but worth hardening in Rust (e.g. check a known body).

10. **In-memory KV is loaded eagerly and fully at boot** (`KvStore.init` reads the entire `kv` table into a map). As cached blobs grow (orders/menu history across many shifts/branches accumulate; nothing prunes `kv` except cart-scope clears), startup memory and load time grow unbounded — there is **no eviction/TTL on the `kv` table itself**, only on `sync_meta` freshness flags and the 48h outbox purge. The Rust port should add KV pruning.

Key files: `/Users/shawket/Desktop/madar-pos/lib/core/services/offline_queue.dart`, `/Users/shawket/Desktop/madar-pos/lib/core/db/outbox_dao.dart`, `/Users/shawket/Desktop/madar-pos/lib/core/db/app_database.dart`, `/Users/shawket/Desktop/madar-pos/lib/core/db/kv_store.dart`, `/Users/shawket/Desktop/madar-pos/lib/core/storage/storage_service.dart`, `/Users/shawket/Desktop/madar-pos/lib/core/services/connectivity_service.dart`, `/Users/shawket/Desktop/madar-pos/lib/core/repositories/{menu,shift,order,delivery_order}_repository.dart`, `/Users/shawket/Desktop/madar-pos/lib/core/models/pending_action.dart`, `/Users/shawket/Desktop/madar-pos/lib/core/providers/{menu_notifier,shift_notifier,order_history_notifier,draft_carts_notifier,cart_storage}.dart`, `/Users/shawket/Desktop/madar-pos/lib/features/order/checkout/checkout_sheet.dart`, `/Users/shawket/Desktop/madar-pos/lib/main.dart`.