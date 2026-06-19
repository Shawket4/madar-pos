# Sufrix POS — Offline-First Audit & Rust-Core Extraction Map

> Generated from a cross-repo audit (Flutter POS + Actix backend). Raw per-area findings in `docs/audit/`.

# Sufrix POS — Rust-Core Rebuild & Backend Offline-First: Decision-Grade Synthesis

*Synthesis of seven audits (4 frontend, 3 backend) steering `~/Desktop/sufrix-rebuild` (shared Rust core via UniFFI + thin SwiftUI/Compose UIs) and a backend offline-first workstream. The user has authority to change the backend.*

---

## 1. Executive Summary

1. **The shift-logout bug is deliberate, not accidental — and the backend already disagrees with the client.** `close_shift_screen.dart:306-313` explicitly calls `authProvider.notifier.logout()` after a close, which physically deletes the JWT from the keychain (`auth_repository.logout()` → `setAuthToken(null)` + `_storage.clearAuth()`). But the backend `close_shift` (`SufrixRust/src/shifts/handlers.rs:835-930`) does **one** mutation — `UPDATE shifts SET status='closed'` — and **never touches the user, token, or any session**. The token stays cryptographically valid for its full 12h TTL after close. **The logout is a pure client-side policy choice layered on top of a backend that already decouples auth from shifts.** This is the single most important finding: the fix is mostly removing client behavior, not adding backend machinery.

2. **The offline trap is real and has two compounding root causes.** (a) Logout-on-close clears the only credential the device holds; (b) there is no refresh token and no offline credential to re-mint against — every token-issuing path (`/auth/login`, `/auth/resolve-branch`) is a server round-trip that verifies bcrypt hashes which **exist only in the DB** (`UserPublic` strips `pin_hash`/`password_hash` at `models/mod.rs:66-79`). After a close at a disconnected site, the teller is at `/login` with no token and structurally no way to mint one.

3. **The existing offline-unlock path is a degraded trap, not a solution.** `offlineUnlock` sets `isOfflineSession=true` and **parks the outbox** (`pauseForAuth`), so the entire next shift's orders + close pile up and never drain until a real online `/auth/login`. Worse, the offline-unlock secret is only written *during* an online login — a device never logged in online cannot offline-unlock at all.

4. **The backend is point-hardened, not offline-first.** Idempotency exists on exactly **two creation paths**: dine-in order create (`orders/handlers.rs:469-479`, `:1238-1248`) and public delivery create (`delivery/public.rs:929-939`). It is **missing** on every other mutation — critically `void_order`, `open/close/force_close_shift`, and `add_cash_movement`. There is no change-feed cursor, no tombstones, no batch replay, no temp-id reconciliation. A days-offline client can only full-refetch every domain except orders, and the orders "delta" (`updated_after`) is **not a safe cursor** (offset over a moving `updated_at` window → skips/dupes).

5. **The shift/cash backend is further along than the offline-support auditor's "all CRUD missing" framing implies — reconcile to the stronger evidence.** Per `be-shifts-cash`, `open_shift`/`close_shift` are *already* idempotent on the client `id`/status; client UUIDs and client timestamps (`opened_at`/`closed_at`/cash `created_at`) are *already* accepted with future-only rejection; a per-shift advisory lock already serializes close vs. inserts. The genuine gaps are narrower: **cash-movement idempotency** (no dedup key — `shift_cash_movements` has no `client_ref`), **`force_close` is non-idempotent** (400 on replay vs. `close`'s graceful return), and **no batch replay endpoint**.

6. **Cash movements are the sharpest money-correctness hole on both sides.** `add_cash_movement` (`shifts/handlers.rs:702`) has zero replay protection, so the client *correctly refuses to queue it offline* (`enqueueCashMovement` throws `OfflineCashMovementError`) — meaning tellers **cannot record petty-cash/paid-out offline at all**, and the history screen has no offline cache. Fixing this requires a backend `client_ref` column + unique index **before** the client can safely queue it.

7. **Printing must stay a hybrid: layout-as-data + Epson encoding move to Rust; rasterization + Star SDK stay native.** Everything prints as a **raster image, never text-mode ESC/POS** — this is why Arabic/Cairo works. A Rust port that switches to text mode would break Arabic entirely. The single biggest porting cost is PDF→bitmap rasterization with real Arabic shaping + bidi (`rustybuzz`/`harfbuzz`).

8. **Routing is three layers, not one — and "force-closed" is invisible to the cache.** The central `redirect()` is only ~60% of the logic; the rest is force-logout interceptors + imperative per-screen `goNamed` reconciliation. `Shift.isOpen == (status=='open')` with no `force_closed` state means an admin force-close from the dashboard leaves the local cache saying "open" → **a teller sells against a dead shift indefinitely offline.**

---

## 2. THE Shift-Reopen Fix (the priority)

### 2.1 Root cause, stated precisely

Two independent facts compose into the trap:

- **Client side (`fe-shift-auth`):** A normal close *always* takes the logout branch. `close_shift_screen.dart:306-313`:
  ```dart
  final canNowLogout = await ref.read(authProvider.notifier).canLogout();
  if (canNowLogout) {
    await ref.read(authProvider.notifier).logout();   // ← deletes JWT from keychain
    if (mounted && !_navigatedAway) context.goNamed('login');
  } else {
    context.goNamed('open-shift');                      // ← effectively dead branch
  }
  ```
  `canLogout()` (`auth_notifier.dart:338-352`, verified) returns `true` whenever **no open shift remains** — which is always true right after a successful close (online: `/shifts/current` says closed; offline: throws → cache fallback to the just-removed shift → `true`). So the `else → open-shift` branch is effectively dead, and **every** close → `logout()` → `/login`.

- **Backend side (`be-auth`):** The JWT is a stateless HS256 token binding `user + org + role + branch_id` only — **no `shift_id`, no session id, no shift state** (`auth/jwt.rs:10-18`). TTL is **teller = 12h**, others = 24h (`handlers.rs:290`). **There is no refresh token anywhere in `src/`.** Middleware (`middleware.rs:58-99`) does signature + `exp` only — no DB lookup, no denylist. The device never holds the JWT secret, so it can't re-validate or re-mint offline. PIN/password verification runs against bcrypt hashes that live **only in the DB** and are never provisioned to the device (`UserPublic` strips them, `models/mod.rs:66-79`).

**Net:** logout deletes the one credential the device has; the device cannot mint a new one without the server; `open_shift` is an authenticated endpoint. At a disconnected remote site, the next shift cannot be opened. *The backend is innocent of the logout — it's purely a POS flow + token-lifetime artifact.* The login handler's own comment (`handlers.rs:234-241`) confirms re-login is meant to *resume* a still-open shift; the backend treats auth and shift as decoupled already.

### 2.2 RECOMMENDED solution

**"Close never logs out (auth decoupled from shift lifecycle) + a securely-provisioned per-branch offline-PIN bundle for true offline re-auth + shift open/close as idempotent outbox mutations."** This is the right answer, and the evidence sharpens it as follows.

Three layers, applied in order of cost:

**Layer 1 — Stop forcing logout on close (the 80% fix, near-zero new surface).**
- **Rust-core:** `close_shift()` returns the next `AppDestination` directly (per `fe-routing`'s proposed API). The decision becomes: *if the same teller will keep the till* → `OpenShift`; *only an explicit "switch teller" action* → `Login{None}`. Never auto-logout as a side effect of close. Keep the cached identity **and the token if still valid**.
- **Backend:** Bump teller access-token TTL from 12h to a working-week-plus window (e.g. **7 days**), so brief/medium outages never expire the token. This is `be-auth` Option B + a TTL bump, and it's the cheapest correctness fix because the backend already permits `open_shift` on any valid token after a close.
- **UI:** Remove the implicit `logout()` from the close flow. Add an explicit "End shift & sign out" vs "End shift, same teller continues" choice (or default to same-teller-continues and put sign-out in the action drawer).
- **Solves:** Same teller opening a new shift after a close, for the entire token TTL, fully offline. This alone removes most real-world pain.
- **Security tradeoff:** A 7-day stateless JWT can't be revoked (no denylist today). Pair with Layer 2's refresh + revocation table to regain killability.

**Layer 2 — Refresh token + revocation (hygiene, widens the online window).**
- **Backend:** Issue a long-lived refresh token at login (30–90 days) in a new hashed, revocable `refresh_tokens(id, user_id, device_id, token_hash, expires_at, revoked_at)` table; add `POST /auth/refresh`. Keeps verification server-side, makes the longer-lived access token killable per device.
- **Rust-core:** Silent re-mint when online; expose `auth_status()` = `SignedOut | Online | Offline`.
- **Security tradeoff:** Refresh still needs the server — **does not solve hard-offline**, only widens the window. Necessary but insufficient alone.

**Layer 3 — Per-branch offline-PIN bundle for true offline re-auth + teller takeover (the real answer for long outages).**
- **Backend:** New authenticated endpoint `GET /branches/{id}/offline-auth-bundle` returning, for each teller assigned to that branch: `user_id, name, role, is_active`, and a **dedicated offline-PIN hash** — *not* the login `pin_hash`. Use a separate bcrypt/argon2 credential so a leaked bundle ≠ full login credential. Bundle is fetched while online, refreshed periodically, scoped strictly to the branch's own tellers, with a freshness window and rotation capability.
- **Rust-core (`store` + `domain`):** Cache the bundle encrypted at rest (OS keystore key). `offline_unlock(name, pin)` verifies bcrypt locally → unlocks the app and **enqueues `open_shift` as an idempotent outbox mutation** (client-minted `shift_id`). **Critically: do NOT park the outbox the way Flutter's `isOfflineSession` does today** — the whole next shift must keep queuing *and remain drainable* the instant connectivity returns, even before a fresh `/auth/login`. This is the key correction to the current design.
- **UI:** Login screen routes a 6-digit PIN to `offline_unlock` when offline; **a different teller takes over by entering their own name + PIN** against the cached bundle — no network, no shared device lockout.
- **Security tradeoffs:** Ships password-equivalent hashes to devices. Mitigations (all from `be-auth` Option C, adopted): dedicated offline-PIN credential, encrypt-at-rest bound to the device keystore, branch-scoped, short freshness window, rotatable. Accept that a deactivated/PIN-changed teller stays valid offline until the next bundle refresh — inherent to offline; sync reconciles on reconnect.

**Why this combination and not just Layer 1:** Layer 1 is bounded by `exp`. A device offline *past* the (extended) TTL is back to square one. Layer 3 is the only path that delivers genuine offline re-auth across multi-day outages and clean teller-takeover on a shared till. Layers compose: 1 ships first and removes most pain; 3 is the Phase 3/4 cornerstone.

### 2.3 Alternatives

**Alt A — Device-bound long-lived token (provisioned-device model, `be-auth` Option D).**
- **Backend:** Register device keypair at first online login (`devices(device_id, public_key, branch_id, revoked_at)`); issue a long-lived token bound to the device public key. PIN locally gates access to the device private key (secure enclave/keystore).
- **Rust-core:** Custody the private key; prove possession on each unlock; device key authorizes offline shift ops that sync later.
- **Tradeoff:** **Best security posture** — a stolen token is useless without the enclave-held private key; cleanly revocable by `device_id`; no hash export. But it's the **heaviest build** (device registration, key custody, attestation, revocation sync), and the server *still* can't verify the PIN offline — PIN-change enforcement defers to reconnect anyway. **Verdict:** strong long-term direction, best layered *over* Layer 3 rather than instead of it.

**Alt B — Minimal: Layer 1 only + much longer teller TTL (e.g. 30 days), no bundle.**
- Cheapest possible; ships in days. Removes the logout, leans entirely on a long TTL so the token rarely expires.
- **Tradeoff:** No true offline re-auth — a new/wiped device, an expired token, or a genuinely-needed teller switch on a never-online device still fails. No revocation on a 30-day stateless token (theft window). **Verdict:** acceptable stopgap *only* if remote sites reliably reconnect within the TTL; not a durable answer for "survives days offline."

---

## 3. Frontend → Rust-Core Extraction Map

Target modules: **net** (transport/HTTP/SSE/sockets), **store** (sqlite/KV/secure), **domain** (business logic/state machines), **print** (layout+encode), **ffi** (UniFFI surface).

| Subsystem | Current Flutter files | Target rust-core module | Stays native | Migration risk |
|---|---|---|---|---|
| **Printing — layout & encoding** | `core/services/printer_service.dart` (936 lines), `models/{order,delivery_order,shift_report,payment_method,branch}.dart`, `utils/{formatting,app_tz}.dart` | **print + domain**: 3 document models→layout, Epson `_pngToEscPos` (alpha-over-white, luminance `<128`, `GS v 0` framing, feed/cut), drawer-kick byte constants, `egp` formatter, branch-tz `_fmtDt`, `_fmtPayment`/`_channelLabel`, `hasPrinter` gate. **net**: Epson raw `TcpStream` to `ip:9100` + Star drawer-kick socket (port 9100, BEL `0x07`) | **PDF→bitmap rasterization** (PDFium/Skia today), **Star iOS/Android SDK** (`starxpand_sdk_wrapper`, no desktop binding), **Cairo font assets** (embed via `include_bytes!`), **logo disk cache** (`flutter_cache_manager`), trigger orchestration (postFrame auto-print, snackbars, `receiptPrinted` guard) | **HIGH** — rasterization + Arabic shaping/bidi is the biggest single cost. Must keep raster (not text ESC/POS) or Arabic breaks. DPI locked at 203; width 576/80mm hard-coded → parameterize. Preserve Row+Expanded flush-right numbers and Star's `String?` error contract |
| **Offline / outbox** | `core/services/offline_queue.dart`, `db/{outbox_dao,app_database,kv_store}.dart`, `models/pending_action.dart`, `main.dart` (callback wiring) | **domain + store + net**: full status machine (`pending/in_flight/synced/dead`), `depends_on` + shift-close barrier gating, backoff (`2000ms·2^(n-1)`, cap 300s, jitter, 15s no-count network reschedule), the **full HTTP-code→outcome table verbatim** (esp. 409/404 money-loss semantics), `recoverInFlight` crash recovery, 48h purge, per-user scoping, idempotency-key generation, `clock_offset_ms` rebasing | Draft/active carts (`DraftCartsNotifier`, `cartStorageScope`) — pure local UI state, never synced | **HIGH** — highest-value, hardest-to-reproduce. The 409/404 distinctions and the crash-replay duplicate-order bug (Gap #2) must be fixed *in* the port, not carried over |
| **Caching (read-through)** | `core/repositories/{menu,shift,order,delivery_order}_repository.dart`, `providers/{menu,shift,order_history}_notifier.dart`, `db/kv_store.dart`, `sync_meta` | **store + domain + net**: KV blob store, `sync_meta` TTL/freshness logic, read-through pattern (local paint→bg fetch→overwrite→bump), `DataFreshness` derivation | **Menu image disk cache** (`flutter_cache_manager`), `MenuImage` widget | **MEDIUM** — straightforward port, but fix: full-refetch-only (no deltas), unbounded `kv` growth (no eviction), `menuCachedAt` scope bug, two idempotency schemes for the same order (online `cart.idempotencyKey()` vs queued fresh UUID → unify to one key minted at checkout) |
| **Cache-based routing** | `core/router/router.dart` (`redirect` rows 1-11, `_AuthListenable`), `api/client.dart` (401/403 interceptors), `providers/{auth,shift}_notifier.dart`, imperative `goNamed` in `order/open-shift/close-shift/action_drawer/device_setup` screens | **domain**: pure `app_route(ctx) -> AppDestination` (mirrors `redirect()`), side-effecting transitions (`login/offline_unlock/open_shift/close_shift/revalidate_session/force_logout`) each **return the next destination** — folds the scattered imperative `goNamed` (rows 17-23) into the core. **net**: own the 401/403 interceptor | UI navigates **only** on (a) a core transition result or (b) a "route invalidated" event → `app_route()` | **MEDIUM-HIGH** — must add first-class `ShiftCacheState::ForceClosed` (today invisible → teller sells against dead shift offline). Eliminate the dual `hasShift` sources and the screen-vs-redirect split-brain. `isOfflineSession` is a single point of truth gating the whole force-logout machine |
| **Shift / cash** | `features/shift/{open_shift,close_shift,cash_movement_sheet,cash_movements}_screen.dart`, `providers/shift_notifier.dart`, `repositories/shift_repository.dart`, `models/shift.dart` | **domain + net**: open/close as idempotent outbox mutations (client `shift_id` via `openWithId`), `_queuedCashForShift` drawer math, `compute`-mirror of system cash, the close→destination decision (decoupled from logout — see §2) | — | **HIGH** — touches money + the headline bug. Cash movements **blocked on backend `client_ref`** before they can be queued. `force_closed` state must become first-class |
| **Auth / session** | `providers/auth_notifier.dart` (`logout`, `_forceLogout`, `offlineUnlock`, `canLogout`, `revalidateSession`), `repositories/auth_repository.dart`, `storage/secure_token_store.dart` | **domain + store + net**: token lifecycle, offline-unlock against cached bundle, `auth_status()`, refresh-on-online; **do not park the outbox on offline sessions** | JWT in **platform keychain/keystore** (Keychain/Keystore/DPAPI), offline-bundle encryption key | **HIGH** — security-sensitive; the offline-PIN bundle custody is new code with no Flutter precedent. Keep secure storage native, logic in Rust |

---

## 4. Backend Offline-First Audit (reconciled)

**Reconciliation note:** `be-offline-support` lists shift open/close/cash as having "zero idempotency hits" and all CRUD as full-refetch-only. `be-shifts-cash` (deeper, shift-specific) shows open/close *are* idempotent and client timestamps/UUIDs *are* accepted. **The shift-specific audit is the stronger evidence** (cites exact handler line ranges and migration `20260613011000_one_open_shift_per_teller.sql`). The tables below reflect that: shifts are partially done; the real gaps are cash idempotency, `force_close` idempotency, cursors, tombstones, and batch replay.

### What EXISTS

| Capability | Endpoint / file | Column / index |
|---|---|---|
| Idempotent dine-in order create | `POST /orders` → `orders/handlers.rs:469-479`, `:1238-1248`, `:2181` | `orders.idempotency_key` (`full_schema.sql:852`), partial uniq `orders_idempotency_key_idx` (`:1843`) |
| Idempotent public delivery create | `POST .../delivery/orders` → `delivery/public.rs:929-939`, `delivery/staff.rs:97` | `delivery_orders.idempotency_key` (`delivery_core.sql:131`), uniq `uq_delivery_orders_idem` (`:140`) |
| **Idempotent `open_shift`** (on client `id`; 200 on replay) | `POST /shifts/branches/{id}/open` → `shifts/handlers.rs` | client `id` PK; partial uniq one-open-per-branch (`full_schema.sql:1787`) + one-open-per-teller (`migration 20260613011000`) |
| **Idempotent `close_shift`** (returns shift if already closed) | `POST /shifts/{id}/close` → `shifts/handlers.rs:835-930` | status re-check under advisory lock |
| **Cash-movement create + list EXIST** (signed amount = direction) | `POST/GET /shifts/{id}/cash-movements` → `:702`, `:789` | table `shift_cash_movements` (`full_schema.sql:948`) — but **no `client_ref`** |
| Client-supplied UUIDs + timestamps (open/close/cash) | `opened_at`/`closed_at`/`created_at`, future-only rejection ±5min (`clock.rs`) | `timestamptz` columns; `clock::reject_if_future` |
| Per-shift advisory lock (serializes close vs. inserts) | `close_shift`, `add_cash_movement`, `force_close` | — |
| Server-authoritative `updated_at` triggers (17 tables) | `set_updated_at()` `full_schema.sql:217`, triggers `:1847+` | covers orders/shifts/menu/inventory/etc. |
| Orders partial delta filter | `GET /orders?updated_after` → `orders/handlers.rs:331`, `:1676` | `orders.updated_at` — **not a safe cursor (see below)** |
| Soft-delete on a few tables | `branches`, `categories`, `menu_items`, `users`, `org_ingredients`, `organizations`, `suppliers` | `deleted_at` — **used as query filter only, never surfaced** |
| Realtime delivery SSE (online only) | `GET /delivery-orders/stream` → `delivery/staff.rs:171-177` | no replay cursor — useless after a gap |

### What's MISSING

| Gap | Affected endpoints / tables | Impact for days-offline replay |
|---|---|---|
| **Idempotency on cash movements** | `add_cash_movement` (`:702`) — no `client_ref` on `shift_cash_movements` | Offline retry double-applies cash → corrupts `expected_cash` + close snapshot. *This is why the client refuses to queue cash offline at all.* |
| **`force_close` idempotency** | `force_close_shift` (`:961`) returns 400 if already terminal (asymmetric vs `close`) | Re-sent force-close on replay errors instead of returning the shift |
| **Idempotency on all other mutations** | `void_order`; inventory `add_to_branch_stock`/`create_waste`/`create_transfer`; stocktake `finalize`; purchasing `receive`/`create_return`; delivery `set_status`/`finalize`/`cancel`; all menu/discount/user CRUD | Queued/retried replay double-applies: double stock deduction, double-finalized stocktakes |
| **Safe change-feed cursor** | Everything except orders. `list_delivery_orders` (`delivery/staff.rs:132`) has **no** `updated_after`; menu/categories/addons/branches/discounts/inventory/shifts/payment-methods are full-refetch only | Multi-day gap → re-pull entire catalogs; heavy payloads; races |
| **Orders "delta" is not a safe cursor** | `GET /orders?updated_after` (`:1676-1705`): filters `updated_at >` but orders by `created_at DESC` + offset | Offset over a moving window skips/dupes; strict `>` drops ties; conveys no deletes |
| **Tombstones surfaced to clients** | Hard deletes: `discounts` (`discounts/handlers.rs:213`), `item_sizes`, `addon_items`, `menu_item_addon_slots`, overrides, `branch_inventory`, transfers, `user_branch_assignments` | Offline client never learns a row was deleted; even soft-deletes aren't exposed via any cursor |
| **Batch / replay endpoint** | None (`/sync`, `/batch`, `/replay` all absent) | N queued mutations = N round-trips, no ordered atomic replay, no per-item result map |
| **Optimistic concurrency (version/ETag)** | No `version`/`row_version` anywhere; no `If-Match` handling | Stale offline edit silently clobbers newer server change; no 409/412 path |
| **Client temp-id ↔ server-id reconciliation** | No `client_ref`/`temp_id` column anywhere except orders/delivery's implicit echo | Client can't deterministically map temp→server ids for shifts/movements/children |
| **No offline-auth provisioning** | `UserPublic` strips `pin_hash` (`models/mod.rs:66-79`); no refresh token anywhere in `src/` | Device structurally cannot re-auth offline (the §2 root cause) |

**Minor online-coupling traps to note** (from `be-shifts-cash`): `reject_if_future` keys on server `Utc::now()` (a slightly-fast device's recent timestamp can 400 at sync — widen tolerance or have the POS re-base to server offset, which `clock.rs`'s doc comment already assumes); `force_close` hard-codes `NOW()` (no client timestamp); carryover `previous_declared_closing` depends on the predecessor being closed *on the server* → enforce replay order **open → orders → cash → close**.

---

## 5. Backend Change List (prioritized, per-endpoint)

Each marked P0/P1/P2 with risk to the **live Flutter app** + **dashboard**. The user has authority to change the backend; all additive changes below are backward-compatible (nullable columns, new optional fields/headers, new endpoints) unless noted.

### P0 — Money/correctness; unblocks offline shift+cash

1. **Cash-movement idempotency.** Add `shift_cash_movements.client_ref uuid` + `CREATE UNIQUE INDEX … (client_ref) WHERE client_ref IS NOT NULL`. Accept `client_ref` in `CashMovementRequest`; check-before-insert + handle 23505 race like `create_order` (`orders/handlers.rs:1238`). **Unblocks the client queuing cash offline** (removes `OfflineCashMovementError`). *Risk: low — additive nullable column + optional field; live app ignores it until updated.*

2. **Make `force_close` idempotent.** Mirror `close_shift`'s early-return: if already terminal, return existing shift (200) instead of 400 (`shifts/handlers.rs:961`). *Risk: low — only changes an error into a success on replay; dashboard force-close UX unaffected.*

3. **Idempotency on the remaining money/stock mutations.** Extend the `Idempotency-Key` header pattern to `void_order`, inventory `create_waste`/`create_transfer`/`add_to_branch_stock`, stocktake `finalize`, purchasing `receive`/`create_return`. **Cheapest scalable approach: one central `idempotency_keys(key uuid pk, org_id, endpoint, request_hash, response_json, status_code, created_at)` table checked by middleware** rather than a column per table. *Risk: medium — touches many handlers; roll out behind the middleware, default no-op when header absent so the live app is unaffected.*

4. **Auth/shift decoupling + TTL bump.** Make it explicit policy that a valid token survives close and `open_shift` needs only a valid token (already true). Bump teller access-token TTL 12h → 7d (`auth/handlers.rs:290`). *Risk: low on backend; **the live Flutter app still force-logs-out on close** until the client ships §2 Layer 1 — so this is necessary-but-not-sufficient until the rebuild/UI change lands. No dashboard impact.*

### P1 — Reconciliation + true offline auth

5. **`client_ref` + echo-back on client-created entities.** Add `client_ref` (uniq `(branch_id, client_ref)`) to `shifts` (already round-trips `id`, but surface explicitly), inventory movements, stocktakes, purchasing orders; **echo it in every create/replay response.** Makes idempotency replay double as reconciliation. *Risk: low — additive; live app/dashboard unaffected.*

6. **Offline-auth provisioning endpoint.** `GET /branches/{id}/offline-auth-bundle` → per-teller `{user_id, name, role, is_active, offline_pin_hash}` using a **dedicated offline-PIN credential** (new `users.offline_pin_hash` or a side table), *not* the login `pin_hash`. Add a `POST /auth/refresh` + hashed revocable `refresh_tokens` table. *Risk: medium — new security surface; ships password-equivalent hashes (mitigations in §2.3). No impact on live app until it consumes the bundle; dashboard may need a "rotate offline PIN" affordance.*

7. **Accept client timestamp on `force_close`.** Add optional `closed_at`/`force_closed_at` to `ForceCloseRequest` with `reject_if_future` (`shifts/handlers.rs:996-999`). *Risk: low. Dashboard force-close keeps working unchanged.*

8. **Widen/clarify future-skew contract for sync.** Either widen `clock_skew_tolerance()` beyond 5min for sync requests or document that the POS re-bases timestamps to server offset (`clock.rs`). *Risk: low.*

### P2 — General sync substrate (heavier, scoped to the offline-first workstream)

9. **Change-feed cursor per mirrored domain.** Preferred: global `change_log(seq bigserial pk, org_id, branch_id, entity, entity_id, op, version, updated_at)` via triggers, exposed as `GET /changes?since=<seq>&entities=...` (ordered, includes tombstones). Minimum viable: per-table `?updated_after=` with **keyset ordering on `(updated_at, id)`, not offset**. **Fix the orders endpoint first** — its current offset-over-`updated_at` is unsafe. Cover menu/categories/addons/overrides, discounts, inventory, branches, payment-methods, delivery_orders, orders. *Risk: medium; new read paths, live app keeps full-refetch until it adopts cursors.*

10. **Tombstones in the feed.** Convert remaining hard-deletes on synced tables (`discounts/handlers.rs:213`, overrides, `item_sizes`, addon slots, transfers, `branch_inventory`) to soft-delete `deleted_at` + emit a `delete` op into `change_log`. *Risk: medium — changes delete semantics; verify dashboard delete flows + every `WHERE deleted_at IS NULL` filter is in place first.*

11. **Batch replay endpoint.** `POST /sync/batch` accepting an ordered `[{client_ref, idempotency_key, method, path, body}]`, applied in submission order, returning `{client_ref → {server_id, status}}`. Enforce replay order **open → orders → cash → close/force-close** (because `compute_system_cash` reads orders+movements at close). Reuses P0 #3 + P1 #5. *Risk: medium-high — new transactional path; build after idempotency + client_ref land.*

12. **Optimistic concurrency.** Add `version integer` (bumped in `set_updated_at` trigger) to mutable synced entities; accept `If-Match`; return 409/412 on mismatch. *Risk: medium — additive but changes update contracts; phase last.*

---

## 6. Impact on PLAN.md

The audits reshape the roadmap in four concrete ways:

1. **Add a parallel BACKEND OFFLINE-FIRST workstream, starting now.** It was implicitly assumed the rebuild was front-loaded on the Rust core. The evidence shows several Rust-core capabilities are **blocked on backend changes** — cash-movement queuing needs `client_ref` (P0 #1); true offline re-auth needs the provisioning endpoint (P1 #6); safe incremental sync needs cursors (P2 #9). Sequence the backend P0 items *ahead of or concurrent with* the corresponding Rust-core modules so the core isn't built against a contract that's about to change. The good news (from `be-shifts-cash`): shift open/close idempotency + client timestamps are **already done**, so the shift outbox can be built against the current backend immediately.

2. **Pull printing, caching, and routing extraction earlier than a "polish" phase.** All three are largely pure logic with well-understood Flutter references and **no backend dependency**, so they're the safest early Rust-core wins that de-risk the UniFFI surface. Sequence within: **caching/outbox first** (highest value, the status machine + HTTP-code table are the spine everything else hangs on), **routing second** (the `app_route()` API consumes auth+shift+cache state), **printing third** (isolatable, but carries the high-risk rasterization/Arabic-shaping cost — start a spike early even if the full port lands later).

3. **Promote the offline-auth design to a Phase 3/4 cornerstone, not an afterthought.** It is the headline user-facing bug, it spans backend + core + UI + security, and it has a clean staging: **Layer 1 (decouple + TTL) can ship in an early phase with near-zero new surface**; **Layer 3 (offline-PIN bundle) is the cornerstone deliverable** for "survives days offline." Make Layer 1 an explicit early milestone so remote sites get relief before the full bundle work completes.

4. **Add explicit "fix-in-port" line items rather than 1:1 ports** for the known correctness bugs the audits surfaced: the crash-replay duplicate-order hole (offline-cache Gap #2 — `replaceOrder(localId)` misses after the first swap), the two-idempotency-scheme order bug (Gap #5 — unify to one key minted at checkout), the `menuCachedAt` scope bug (Gap #4), unbounded `kv` growth (Gap #10), and first-class `ShiftCacheState::ForceClosed` (routing Edge #1). These must be tracked as port-time fixes, not carried forward verbatim.

---

## 7. Open Questions / Decisions for the User

1. **Teller token TTL target.** 7 days (Layer 1) vs 30 days (Alt B stopgap)? Longer = fewer offline lockouts but a longer theft window on a non-revocable stateless token until refresh+denylist (P1 #6) lands. Recommendation: 7d + ship refresh/revocation soon after.

2. **Offline-PIN credential: reuse `pin_hash` or mint a dedicated one?** Strong recommendation is **dedicated** (`be-auth` Option C) so a leaked bundle ≠ full login credential — but it means a separate enrollment step (teller sets/confirms an offline PIN, or it's derived at first online login). Acceptable to derive silently from the same PIN at first online login into a *separate* hash with different cost params?

3. **Device-bound tokens (Alt A / Option D) now or later?** It's the strongest security posture and the cleanest revocation story, but the heaviest build. Layer it over the bundle in a later phase, or skip the bundle and go straight to device-binding? (Recommendation: bundle first for time-to-relief, device-binding as a hardening follow-on.)

4. **Multiple drawers per branch?** The backend enforces one-open-shift **per branch** and **per teller** — there is **no `register_id`/drawer dimension**. If any site runs multiple tills/drawers under one branch, this is a schema change (`be-shifts-cash`). Confirm one-drawer-per-branch is the intended model.

5. **Change-feed: global `change_log` vs per-table keyset cursors?** Global log is cleaner for tombstones + ordering but is a bigger build and a hot write path; per-table `?updated_after` keyset is incremental but won't carry deletes without also converting hard-deletes to soft-deletes (P2 #10). Which to commit the offline-first workstream to?

6. **Printing rasterization strategy.** Keep building PDF + rasterize natively (Rust owns only layout-as-data + Epson encoding) — lower risk, keeps the `pdf`/`printing` deps — **or** move to a full Rust raster pipeline (render layout → 1-bpp bitmap with `rustybuzz`/harfbuzz shaping, drop the deps)? The latter is the cleaner long-term target but the single biggest porting cost and the Arabic-shaping correctness risk. Which for v1?

7. **Star printers: keep the native SDK or re-speak the raw protocol?** Star printing is locked to the iOS/Android-only `starxpand_sdk_wrapper` (no desktop binding). Keeping it means a permanent native FFI seam in the otherwise-shared core. Is dropping Star (Epson-only, fully in Rust) or implementing Star's raw socket protocol on the table?

8. **Legacy NULL-user outbox rows.** The v2 migration left pre-v2 queued actions with `user_id IS NULL`, which `dueForSync` syncs under whoever is logged in (mis-attribution risk on a shared till). At Rust migration: backfill, drop, or leave them? (Recommendation: drop on migration — they're stale by now.)

9. **Cash-movement history offline.** Should the Rust core cache cash-movement history (currently online-only, no fallback) so the screen works offline read-only, even before write-queuing lands? Low effort, removes a visible offline gap.