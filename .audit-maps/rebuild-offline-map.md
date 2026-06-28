I now have a complete picture of the entire end-to-end flow. The `fetch_order_or_404` confirms the void-404 path (order never synced). I have everything needed to write the report.

# Offline Sync System — End-to-End Audit Map (Rust core + SufrixRust backend)

This maps the complete offline → online replay system as implemented today. All file paths are absolute. Line references are to the versions read in this session.

---

## 1. Component map

| Layer | File | Responsibility |
|---|---|---|
| HTTP transport | `/Users/shawket/Desktop/madar-rebuild/rust-core/crates/madar-core/src/net.rs` | One pooled `reqwest` client; bearer injection; error → `CoreError` classification; `ping` (connectivity + clock skew). |
| Durable store | `/Users/shawket/Desktop/madar-rebuild/rust-core/crates/madar-core/src/store.rs` | SQLite `kv` mirror, `outbox`, `id_map`, `sync_cursors`; all outbox state transitions. |
| Orchestrator | `/Users/shawket/Desktop/madar-rebuild/rust-core/crates/madar-core/src/lib.rs` | `MadarCore`: enqueue from each op (open/close/order/void/cash), `drain_outbox`, `send_outbox_item`, backoff, connectivity reconcile, login/sign-in, shift adoption. |
| Session/auth | `/Users/shawket/Desktop/madar-rebuild/rust-core/crates/madar-core/src/session.rs` | Online login wire build; offline PIN unlock (argon2id) against cached org bundle; teller attribution identity. |
| Shift logic | `/Users/shawket/Desktop/madar-rebuild/rust-core/crates/madar-core/src/shift.rs` | Optimistic local shift, close-local, server-vs-local `reconcile` (bounce-proofing). |
| Backend replay | `/Users/shawket/Desktop/SufrixRust/src/sync/{mod.rs,handlers.rs,routes.rs}` | `POST /sync/replay`; `ActingContext` (live vs replay); per-op org/teller validation; dispatch to the shared `*_inner` handlers. |
| Backend login guard | `/Users/shawket/Desktop/SufrixRust/src/auth/handlers.rs` | Open-shift login rules + `X-Sufrix-Closing-Shifts` handover acknowledgment. |
| Inner handlers (idempotency) | `/Users/shawket/Desktop/SufrixRust/src/{shifts,orders}/handlers.rs` | Idempotency early-returns + unique-index backstops; replay-mode guard bypass. |
| Integration tests | `/Users/shawket/Desktop/madar-rebuild/rust-core/crates/madar-core/tests/offline_replay.rs` | End-to-end offline → replay against a live dev backend (`--ignored`). |

---

## 2. Outbox persistence (schema)

`store.rs` defines a single append-only command queue table (`SCHEMA`, lines 37–57):

```
outbox(
  seq             INTEGER PK AUTOINCREMENT,  -- global FIFO order
  id              TEXT UNIQUE,               -- client-minted uuid; dedups enqueue
  op_type         TEXT,                      -- create_order | void_order | open_shift | close_shift | cash_movement
  idempotency_key TEXT,                      -- in-body exactly-once token (server dedup)
  payload         TEXT,                      -- canonical request JSON (the typed command)
  event_at        TEXT,                      -- client real-event time (RFC3339)
  enqueued_at     TEXT,
  status          TEXT DEFAULT 'pending',    -- pending | inflight | acked | dead
  attempts        INTEGER DEFAULT 0,
  last_error      TEXT,
  server_id       TEXT,                      -- set on ack
  depends_on_seq  INTEGER,                   -- gate dependents
  next_attempt_at INTEGER DEFAULT 0,         -- epoch-ms backoff gate (0 = ready now)
  synced_at       INTEGER,                   -- epoch-ms ack time (retention)
  user_id         TEXT,                      -- enqueuing teller (attribution)
  clock_offset_ms INTEGER,                   -- device→server skew at enqueue
  shift_id        TEXT                       -- owning shift (close-last gating)
)
```

Indexes: `outbox_status_seq(status, seq)` and `outbox_due(status, next_attempt_at, seq)`. The latter is created only **after** migrations, because old DBs predate `next_attempt_at` (store.rs lines 128–139). Migrations are idempotent `ALTER TABLE … ADD COLUMN` with errors swallowed (lines 63–69, 131–133); a large block of tests locks every missing-column upgrade path (lines 611–840).

Companion tables: `kv` (read-through mirror of canonical wire JSON, keyed e.g. `current_shift`, `offline_auth_bundle`, `org_config`, `clock_skew_secs`); `id_map` (entity temp-id ↔ server-id); `sync_cursors` (per-stream high-water mark — present but not yet driven by a catch-up read in the code paths reviewed).

WAL + `synchronous=NORMAL` + 5s busy timeout; a single `Mutex<Connection>` serializes all writers; poisoned-lock recovery via `into_inner` (store.rs lines 143–147).

---

## 3. How an op is enqueued offline

Every mutating FFI method (`open_shift`, `close_shift`, `checkout`, `void_order`, `record_cash_movement` in lib.rs) follows the same offline-first shape:

1. **Write optimistic local state** to `kv` (e.g. `shift::save` for open; `shift::close_local` for close; `cart::clear` after checkout). The UI reads this immediately, regardless of connectivity.
2. **Mint a client UUID** that doubles as both the outbox `id` and the in-body idempotency token:
   - open_shift: `id = shift PK` (lib.rs 1399, 1432–1442).
   - close_shift: `id = "{shift_id}:close"` so open and close for one shift never collide in the idempotent outbox (lib.rs 1481–1493).
   - create_order: `id = order UUID`, also set as `idempotency_key` in the body (lib.rs 1686–1696).
   - cash_movement: `id = client_ref UUID`, set as `request.client_ref` (lib.rs 1554–1572).
   - void_order: `id = "{order_id}:void"` (in the unshown tail, consistent with the pattern).
3. **Stamp metadata** via `outbox_meta()` (lib.rs 373–382): `user_id` = the enqueuing teller's server id, `clock_offset_ms` = current server skew in ms. Timestamps are written with `corrected_now()` (lib.rs 626–629) = `Utc::now() + skew`, so a wrong-clock till does **not** future-date writes (which the backend's `reject_if_future` would 400).
4. **Wire the dependency** via `depends_on_seq = live_seq_of(shift_id)` (store.rs 283–289) — order/close/cash gate behind the shift's still-queued `open_shift`. `open_shift` itself has `depends_on_seq = None` (chain root).
5. **`store.enqueue`** (store.rs 200–211): `INSERT … ON CONFLICT(id) DO NOTHING`, then returns the existing `seq`. Re-enqueue with the same id is a no-op (a double-tap or replay never duplicates locally; first enqueue wins its field values — tests at store.rs 456–462, 1226–1235).
6. **Best-effort inline drain** (`let _ = self.drain_outbox().await`): online sends now; offline just leaves it queued.

`queued_offline` on the receipt is derived by re-reading `pending()` after the drain (lib.rs 1704–1707) — true iff the order is still queued.

---

## 4. Ordering & dependency enforcement

Three layers, all in `drain_outbox` (lib.rs 405–492) plus store predicates:

- **Global FIFO**: `due_for_sync` returns `status='pending' AND next_attempt_at<=now ORDER BY seq ASC` (store.rs 217–237). Seq is the autoincrement insertion order.
- **Prerequisite gating** (`depends_on_seq`): before sending, the drain inspects the dependency's status (lib.rs 432–444): `pending`/`inflight` → skip this pass; `dead` → **cascade-fail** (mark this op dead with "a required earlier action failed to sync", log a diag); `acked`/discarded → proceed. This prevents an order from ever hitting the server before its shift's open.
- **Close-last gating** (`has_live_shift_writes`, store.rs 295–302): a `close_shift` is held while any `create_order` / `void_order` / `cash_movement` for the **same** `shift_id` is still `pending`/`inflight` (excluding the close's own seq). Shift-scoped, so a later shift's orders never block an earlier shift's close. This guarantees the close is the last op to replay for its shift even though FIFO alone wouldn't.

The combination is what makes the "offline day" safe: `open → orders/cash → close`, then `open next shift` replays in dependency order. The sequential-only invariant (one open shift per device, even offline) is enforced at enqueue by `device_has_open_shift()` (lib.rs 342–354, 1378–1383).

---

## 5. The drain → `/sync/replay`

`send_outbox_item` (lib.rs 499–605) is the single network path for **all** queued ops:

1. **Rebase timestamps** by `rebase_delta_ms = now_skew_ms − enqueue_skew_ms` (lib.rs 610–616) applied via `rebase_dopt` to the op's `*_at` field — corrects only for skew that changed between enqueue and send.
2. **Resolve attribution**: pull `item.user_id` as `teller_id`; a legacy/un-attributed op is `Dead("queued op has no teller attribution")` (lib.rs 506–510) — it can never be safely replayed.
3. **Build the replay envelope**: a hand-built JSON wrapper `{ "op": <type>, "teller_id": …, <path ids>, "request": <generated request type> }` (lib.rs 517–574). The `request` is the **generated** wire type, so the body is byte-identical to the live endpoint's. Each op also carries an `Idem` profile (No / Yes / VoidIdem) controlling how 409/404 are read.
4. **POST** `/sync/replay` via `api.post_json` (net.rs 110–124).
5. **Classify the result** into `SendOutcome` (lib.rs 632–675):
   - 2xx → `Acked`; for open_shift it caches the server's authoritative `Shift`; for create_order it extracts `order.id`. A 2xx it can't decode still acks (the write landed).
   - `Offline` → `mark_retry_no_count` + **stop the pass** (network is down for everything).
   - `AuthExpired` (401) → `mark_retry_no_count`, set `auth_paused = true`, **park the whole queue**, return.
   - `Dead` (validation/forbidden/non-idempotent 409/404) → `mark_dead`, diag log; a dead `open_shift` whose id is the cached shift clears the phantom local shift (lib.rs 458–462).
   - `Retry` (5xx / undecodable 2xx / transport `Transient`) → counted exponential backoff; dead-letter at `K_MAX_RETRIES = 8`.

**Backend side** (`sync/handlers.rs`): `ReplayOp` is a `#[serde(tag="op")]` enum (lines 19–27). `replay()` (59–111):
- extracts the bearer's org (`token_org`);
- verifies the embedded `teller_id` is an **active Teller of that same org** (lines 75–88) — the attribution-safety boundary; a write can't be attributed cross-org or to a manager/admin;
- `op_branch_must_be_in_org` (116–157) resolves the op's effective branch/shift/order to its org and rejects a target in a different org (a not-yet-present target is left to the inner handler);
- builds `ActingContext::replay(teller_id, token_org)` (mod.rs 63–66) and dispatches to the same `*_inner` handler the live route uses.

`ActingContext.replay = true` (mod.rs 31–46) **bypasses** ownership / drawer-owner / one-open-per-branch precheck / cash-continuity / teller-match guards (it's recorded history), while **keeping** FKs, unique indexes, idempotency early-returns, org scoping, and the shift-must-be-open guard for orders.

---

## 6. Idempotency & exactly-once

Exactly-once lives **server-side**, keyed on the in-body token; the client just retries safely:

- **create_order**: `create_order_inner` early-returns the existing order if `idempotency_key` already exists (orders/handlers.rs 523–527); a concurrent insert that loses the `orders.idempotency_key` unique race re-fetches and returns the original (1284–1293).
- **cash_movement**: `add_cash_movement_inner` early-returns by `client_ref` (shifts/handlers.rs 816–819); the `client_ref` unique-constraint race re-fetches the original (867–876).
- **open_shift**: `open_shift_inner` early-returns the existing shift by `(shift_id, branch_id)` (shifts/handlers.rs 352–376); unique partial indexes (one open per branch / per teller) are the backstop.
- **close_shift**: closing an already-closed shift just returns it (shifts/handlers.rs 982–989; idempotent at 963).
- **void_order**: `fetch_order_or_404`; if already `voided`, return it (orders/handlers.rs 1868–1869); the conditional `UPDATE … WHERE status <> 'voided'` matching 0 rows re-fetches and returns the already-voided order without double-restocking (1937–1944).

Client `Idem` profiles match this (lib.rs 658–675): `Yes` (close/cash) and `VoidIdem` treat 409 as already-applied → `Acked`; `VoidIdem` 404 → `Dead("order not found")` (the order never synced, so don't silently swallow the void); `No` (open/order) treats 409/404 as genuine → `Dead`, since the open/order endpoints already 2xx-return on the idempotent path.

The net guarantee: a lost-response retry (the same idempotency key replayed any number of times) lands the effect exactly once. The integration tests drain 2–3× to prove it.

---

## 7. Attribution

- Each op stores `user_id` (the teller who rang it) at enqueue (lib.rs 373–382).
- The drain is **device-global**, NOT teller-scoped: `due_for_sync(now, None)` (lib.rs 420) flushes everyone's backlog. `/sync/replay` re-attributes each op to its **embedded** `teller_id`, so any signed-in teller (A or B) can flush a shared till's mixed backlog and each write lands under its true author (this is the fix to the "must be the same teller to sync" bug). `due_for_sync` still supports a `user_id` scope for legacy/other callers (store.rs 217–237), but the live drain passes `None`.
- The backend re-validates the embedded teller per op (handlers.rs 75–88) and bypasses live ownership guards in replay mode, so B flushing A's reopened shift records it under A. Test `another_teller_flushes_the_backlog_attributed_to_the_original` (offline_replay.rs 460–552) locks this end-to-end, including the post-fix invariant that B is **not** dropped into a shift B doesn't own (shift state is teller-scoped on read).

---

## 8. Connectivity transitions → reconcile

- **Classification** (net.rs `classify_reqwest` 217–236): any connect/timeout/request/connectivity-io failure → `Offline` (uncounted retry — a queued sale must not dead-letter on a flaky link); only a non-transport failure stays `Transient` (counted). 5xx → `Transient`; 401 → `Unauthenticated`; 400/422 → `Validation`; 403 → `Forbidden`.
- **Heartbeat** `refresh_connectivity` (lib.rs 1732–1756): `ping` (any HTTP response = online; reads server `Date` for skew). On success: store skew (+ persist to `kv` for cold offline boot), set `online=true`, `clear_network_backoff()` to un-gate the offline backlog **now**, then `drain_outbox`. On failure: `online=false`.
- **`clear_network_backoff`** (store.rs 370–376): zeroes `next_attempt_at` only for `attempts=0` rows with a positive gate (the no-count connectivity reschedules); counted server-error backoffs (`attempts>0`) keep their exponential gate. `sync_now` also calls it (lib.rs 1713–1718) so an explicit sync flushes immediately rather than waiting the 15s network window.
- **Crash recovery**: every drain first runs `recover_inflight()` (inflight → pending) and `purge_acked_older_than(now − 48h)` (lib.rs 408–409). Idempotency makes the retry of a recovered inflight op safe.
- **401 un-park**: a successful `login` clears `auth_paused` and re-drains (lib.rs 1160–1161, 1201).
- **Login-time reconcile & handover**: `login` drains the backlog **before** adopting the shift (lib.rs 1196–1206), so the just-signed-in teller sees current server state. `closing_shift_ids_csv()` (lib.rs 360–369) is sent as `X-Sufrix-Closing-Shifts`; the backend's open-shift login guard (auth/handlers.rs 249–314) permits signing in over another teller's open shift **only** when that shift's id is in the acknowledged CSV (a legitimate offline handover whose close is queued), else 409. `sign_in` (lib.rs 1215–1273) adds an up-front ownership gate (a device's own open shift may only be resumed by its owner), a 7s online timeout, and the captive-portal/transport fallback to offline unlock (`is_connectivity_failure`, net.rs 266–275 — covers Offline/Transient, decode-failures, and 511/407/408, but never 401/403/400/422).
- **Shift bounce-proofing** (`shift::reconcile`, shift.rs 287–305): server "no open shift" is authoritative only once our own `open_shift` has acked (`open_pending` → KeepLocal); server "still open" is stale while our `close_shift` is queued (`close_pending` → KeepLocal). This prevents the optimistic shift from flickering on/off as commands sync.

---

## 9. Backoff / retry constants

`lib.rs` 678–698: `K_MAX_RETRIES=8`, `K_BASE_BACKOFF_MS=2000`, `K_MAX_BACKOFF_MS=300_000` (5min), `K_NETWORK_RETRY_MS=15_000`, `K_ACKED_RETENTION_MS=48h`. `compute_backoff_ms` = `BASE·2^(attempts-1)` capped at MAX, plus deterministic per-seq jitter `0–999ms` (no RNG dep) to avoid a thundering herd. Counted retries use `mark_retry` (attempts++, error, gate); connectivity/401 use `mark_retry_no_count` (gate only) — store.rs 349–363.

---

## 10. Invariants the tests currently lock

**Unit (store.rs `tests`)** — enqueue idempotency on id (and first-enqueue-wins); FIFO by seq; ack/dead/acked-hidden; `due_for_sync` backoff gate inclusive boundary, status filtering, user-scoping with NULL-legacy inclusion; `live_seq_of`/`status_of_seq` state transitions; `has_live_shift_writes` op-type filtering, shift-scoping, self-exclusion, ignores acked/dead; counted vs no-count retry; purge only touches acked; requeue/discard only dead; inflight recovery; full old-schema migration matrix (every missing column + index + re-open idempotency).

**Unit (net.rs)** — status→variant table; connectivity io kinds → Offline (uncounted); non-transport → Transient; captive-portal fallback covers 511/407/408 + decode errors but never credential rejections; HTTP `Date` parsing.

**Unit (session.rs / shift.rs)** — PIN/email wire validation; offline argon2id unlock (right/wrong PIN, inactive teller, null hash, case-insensitive name, multi-teller); permissions optimistic-while-unloaded; `reconcile` full matrix (forward/reverse shift bounce); corrected/offline shift-report projection.

**Integration (offline_replay.rs, `--ignored`, live backend)** — these lock the headline guarantees:
- `offline_login_unlocks_after_an_online_login` / `captive_portal_login_falls_back_to_offline` — offline unlock + portal fallback.
- `online_checkout_syncs_immediately` — online sale leaves nothing queued.
- `offline_then_replay_lands_exactly_once` / `offline_backlog_replays_exactly_once_across_a_reconnect` — **exactly-once across repeated drains** (lost-ack replay), nothing dead-lettered, no duplicate history.
- `full_offline_day_open_sell_close_replays_in_dependency_order` — open → order (gated) → close-last dependency chain replays correctly.
- `another_teller_flushes_the_backlog_attributed_to_the_original` / `offline_teller_switch_close_open_syncs_on_reconnect` — cross-teller flush with correct per-op attribution; offline close+open syncs and server shift state is authoritative.
- `login_rejected_taking_over_another_tellers_open_shift` — the rejection half of the handover guard (no ack ⇒ 409).
- `cash_movement_is_offline_first_and_idempotent` — drawer op queues, drains, and `client_ref` dedups across replays.

**Not locked / worth auditor attention:** `sync_cursors` exist but no days-offline catch-up *read* path was observed using them; `id_map` is written but reconciliation consumers weren't in the files reviewed; the integration suite is `--ignored` (requires a live dev backend + seeded fixture org, `--test-threads=1` for the shift-mutating tests) so it does not run in CI; a legacy NULL-`user_id` outbox row dead-letters on drain (by design) rather than replaying.