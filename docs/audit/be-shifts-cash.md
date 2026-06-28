I have everything needed. The audit is complete. Here is the full report.

---

# Backend Shift + Cash-Movement Audit (Actix-Web / Rust)

Source: `/Users/shawket/Desktop/MadarRust/src/shifts/` (`handlers.rs`, `routes.rs`, `mod.rs`, `tests.rs`), schema in `/Users/shawket/Desktop/MadarRust/migrations/20260531200000_full_schema.sql` and `…/20260613011000_one_open_shift_per_teller.sql`, clock guard in `src/clock.rs`.

Bottom line up front: **the offline groundwork is already substantially done.** Cash movements exist as a first-class endpoint; client-supplied UUIDs + clock-tolerant timestamps already exist on open/cash-movement/close; and open/close are idempotent. The main remaining gaps are (1) cash movements have **no idempotency/dedup key**, (2) there is **no batch replay endpoint**, and (3) one online-coupling check (`reject_if_future`) uses *server* `now()`.

## 1. Endpoint table

All routes are under `web::scope("/shifts")` wrapped by `JwtMiddleware` (`routes.rs`). Every handler calls `check_permission(... "shifts", <action>)` and `require_branch_access`.

| Method | Path | Handler | Perm | Request body | Response | Key requirements / side effects |
|---|---|---|---|---|---|---|
| GET | `/shifts/branches/{branch_id}/current` | `get_current_shift` | shifts:read | — | `ShiftPreFill { has_open_shift, open_shift, suggested_opening_cash }` | Read-only. `suggested_opening_cash` = previous declared closing (drawer carryover). |
| POST | `/shifts/branches/{branch_id}/open` | `open_shift` | shifts:create | `OpenShiftRequest { id?, opening_cash, opening_cash_edited?, edit_reason?, opened_at? }` | 201 `Shift` (200 on idempotent replay) | Inserts shift. **Idempotent** on client `id`. Enforces one-open-per-teller + one-open-per-branch (pre-checks + partial unique indexes). Derives `was_edited` server-side from carryover; requires `edit_reason` if opening ≠ prior declared closing. `opened_at` accepted from client; future rejected. |
| GET | `/shifts/branches/{branch_id}` | `list_shifts` | shifts:read | query `page?`, `per_page?` | `PaginatedShifts` | Read-only. `branch_id = nil UUID` → all branches in org. Pagination opt-in. |
| GET | `/shifts/{shift_id}` | `get_shift` | shifts:read | — | `Shift` | Read-only. |
| GET | `/shifts/{shift_id}/report` | `get_shift_report` | shifts:read | — | `ShiftReportResponse` | Read-only but **server-computed**: payment summary, cash-movement totals, and `expected_cash` (live `compute_system_cash` for open shifts, snapshot for closed). |
| POST | `/shifts/{shift_id}/cash-movements` | `add_cash_movement` | shifts:update | `CashMovementRequest { amount, note, created_at? }` | 201 `CashMovement` | **Cash-movement create exists.** Teller may only move cash in own shift. Rejects amount=0, empty note, future `created_at`. Takes per-shift advisory xact lock; re-checks shift `open`. **No idempotency key.** |
| GET | `/shifts/{shift_id}/cash-movements` | `list_cash_movements` | shifts:read | — | `Vec<CashMovement>` | Read-only. |
| POST | `/shifts/{shift_id}/close` | `close_shift` | shifts:update | `CloseShiftRequest { closing_cash_declared, cash_note?, closed_at? }` | 200 `CloseShiftResponse { shift }` | Teller may only close own shift (manager/admin any in scope). **Idempotent** (returns shift if already closed). Advisory lock + re-check open; snapshots `closing_cash_system` via `compute_system_cash`. `closed_at` accepted; future rejected; must be ≥ `opened_at`. |
| POST | `/shifts/{shift_id}/force-close` | `force_close_shift` | shifts:update | `ForceCloseRequest { reason? }` | 200 `Shift` | Tellers forbidden (managers/admins only). Errors 400 if not open (**not idempotent** — differs from `close`). Snapshots `closing_cash_system` under advisory lock; sets `force_closed_*`. **Uses server `NOW()` for `closed_at`/`force_closed_at` (no client timestamp accepted).** |
| DELETE | `/shifts/{shift_id}` | `delete_shift` | OrgAdmin/SuperAdmin only (hard role check, not permission) | — | 204 | Refuses open shifts (409) and shifts with non-voided orders (409). Deletes voided orders, then shift (cascades to `shift_cash_movements`). |

Cash data model:
- `Shift` (handlers.rs:21) — money fields are `i32` piastres; `cash_discrepancy` is a generated column (`closing_cash_declared - closing_cash_system`).
- `CashMovement` (handlers.rs:50) — `{ id, shift_id, amount, note, moved_by, moved_by_name, created_at }`. Signed `amount`: positive = cash in, negative = cash out. Table `shift_cash_movements` (schema:948) has **only** `id, shift_id, amount, note, moved_by, created_at` — no client ref, no idempotency key, FK `shift_id` `ON DELETE CASCADE`.

## 2. Cash-movement verdict

**A cash-movement endpoint EXISTS and is fully implemented** — it is *not* absent.
- Create: `POST /shifts/{shift_id}/cash-movements` → `add_cash_movement` (handlers.rs:702).
- List: `GET /shifts/{shift_id}/cash-movements` → `list_cash_movements` (handlers.rs:789).
- Rolled into reporting: `get_shift_report` returns `cash_movements`, `cash_movements_in/out/net`, and they feed `compute_system_cash` (handlers.rs:222) so movements affect `expected_cash` and the close snapshot.
- Model `CashMovement` + table `shift_cash_movements` are real. There is **no separate "deposit/withdrawal/payout/paid_in/paid_out" concept** — it's a single signed-amount movement (sign = direction). No `cash_movement_type` enum exists.

## 3. Shift state machine

```
                    ┌────────────────────────────────────────────┐
                    │                                            │
     open_shift     ▼          close_shift (own teller /        │
   ┌──────────► ┌────────┐     manager-admin in scope)   ┌──────────┐
   │            │  open  │ ───────────────────────────►  │  closed  │
   │            └────────┘                                └──────────┘
   │              │   ▲                                        │
   │   force_close│   │ idempotent re-open by same id          │ idempotent
   │   (manager/  │   │ returns existing                       │ re-close
   │    admin)    ▼                                            ▼ returns shift
   │         ┌──────────────┐                          (terminal; carryover
   │         │ force_closed │                           source for next open)
   │         └──────────────┘
   │              │
   └── both closed & force_closed are terminal; delete_shift may
       remove a terminal shift only if it has no non-voided orders
```

Enum `shift_status` (schema:182) = `'open' | 'closed' | 'force_closed'`. Transitions:
- `open` → `closed`: `close_shift`. Idempotent (re-close returns the shift).
- `open` → `force_closed`: `force_close_shift`. **Not** idempotent — returns 400 "Shift is not open" if already terminal (asymmetry worth fixing for offline replay).
- Terminal → deleted: only via `delete_shift`, only for empty (no non-voided orders) terminal shifts.
- No reopen transition exists. Carryover: next open derives `suggested_opening_cash`/`expected_opening` from the most recent `closed`/`force_closed` shift's `closing_cash_declared` (`previous_declared_closing`, handlers.rs:166).

**Uniqueness constraints (DB-enforced, race-proof):**
- `idx_shifts_one_open_per_branch` — `UNIQUE (branch_id) WHERE status='open'` (schema:1787): **one open shift per branch.**
- `idx_shifts_one_open_per_teller` — `UNIQUE (teller_id) WHERE status='open'` (migration 20260613011000): **one open shift per teller, globally** (across branches).
- There is **no register/drawer dimension** — uniqueness is per branch and per teller only. No `register_id` column exists. If you ever want multiple drawers per branch, this is a schema change.

## 4. Online-coupling analysis (what would force the client online)

Mostly **decoupled already** — this backend was clearly built with offline in mind. Findings:

- ✅ **Client-generated IDs.** `open_shift` accepts `body.id` and replays idempotently; cash movements and orders use client UUIDs too. POS can mint IDs offline.
- ✅ **Client timestamps accepted.** `opened_at`, `closed_at`, and cash-movement `created_at` are all optional client-supplied; the server stores them verbatim. Only **future** timestamps are rejected (`clock::reject_if_future`, ±5 min skew). Past/backdated values are honored (tests `test_shift_timestamp_guards`, `test_cash_movement_timestamp_contract`). Force-close is the exception (server `NOW()` only).
- ✅ **No server sequence number on shifts.** Shifts have no per-shift monotonic counter (orders do — `order_ref` via `order_ref_counters` — but shifts do not). Nothing blocks offline shift creation on a sequence.
- ⚠️ **`reject_if_future` keys on server `Utc::now()`.** A device that's been offline and is slightly fast relative to the server could have a *recently-stamped* `created_at`/`closed_at` land >5 min in the future at sync time and get a 400. Real but minor; 5-min tolerance covers normal skew. Worth widening tolerance or having the POS re-base timestamps to server offset at sync (the clock.rs doc comment already assumes the POS does this).
- ⚠️ **`expected_cash` / `closing_cash_system` is server-computed from prior server state** (`compute_system_cash`: opening + cash order payments + cash tips + net movements, handlers.rs:199). The *close snapshot* needs the orders/payments to already be on the server. This is fine for offline *capture* (close stores declared cash from the client and the server computes its own system figure at replay time) but means the **system/expected figure is only correct once all of that shift's orders + movements have also synced** — close must be replayed *after* its orders. This is an ordering dependency, not a hard online requirement.
- ⚠️ **Carryover continuity** (`previous_declared_closing` keyed on `closed_at DESC`) depends on the predecessor shift already being closed *on the server*. If two shifts on the same branch are opened/closed offline and synced out of order, the carryover validation (`edit_reason` requirement) could misfire. The `was_edited`/`edit_reason` rule is server-authoritative and not skippable, so offline opens that deviate must carry a reason or they'll 400 at replay.
- ✅ **Branch/teller open-uniqueness is the one genuinely online-ish constraint**, but it's correct to keep: it's what prevents two devices opening the same drawer. For offline, the partial unique index will surface as a 409 at sync — the POS needs to handle that (treat as "someone already opened it" / reconcile).

## 5. Backend changes needed for offline shift open/close + cash movements

Ordered by impact. Several items are *already done* — flagged so you don't redo them.

**Already in place (no work):**
- Idempotent `open_shift` and `close_shift` keyed on client `id` / current status.
- Client UUIDs for shifts and cash movements.
- Client-supplied `opened_at` / `closed_at` / cash-movement `created_at`, with future-only rejection.
- Per-shift advisory lock serializing close vs. order/movement inserts (no cash lost to races).

**Gaps to close:**

1. **Idempotency / dedup key on cash movements (highest priority).**
   `add_cash_movement` has *no* replay protection — an offline POS that retries a sync will insert duplicate movements, corrupting `expected_cash` and the close snapshot. Add a `client_ref UUID` (or reuse an `Idempotency-Key` header like orders) column on `shift_cash_movements` with a partial unique index, plus a replay branch that returns the existing row. Schema: add `client_ref uuid` + `CREATE UNIQUE INDEX … (client_ref) WHERE client_ref IS NOT NULL`. Handler: accept `client_ref` in `CashMovementRequest`, check-before-insert and handle the 23505 race like `create_order` does (orders/handlers.rs:1235).

2. **Make `force_close` idempotent like `close`.**
   Currently returns 400 if already terminal (handlers.rs:961). On offline replay a re-sent force-close should return the existing shift (200), not error. Mirror the `close_shift` early-return pattern.

3. **Accept client timestamp on `force_close`.**
   `force_close_shift` hard-codes `NOW()` for `closed_at`/`force_closed_at` (handlers.rs:996–999). Add an optional `closed_at`/`force_closed_at` to `ForceCloseRequest` and apply the same `reject_if_future` guard, so an offline-recorded force-close keeps its real time.

4. **Batch replay endpoint.**
   There is **no** batch/sync endpoint anywhere in the backend (grep confirms only `create_order`, `delivery`, and shift open/close do per-call idempotency). For offline-first sync you want one transactional `POST /shifts/sync` (or `/sync/replay`) that accepts an ordered envelope — `[shift opens, orders, cash movements, closes]` — each carrying its client ref, and replays them idempotently in dependency order in a single pass, returning per-item results (applied / replayed / conflict). Without it the POS must fan out N individual calls and manage ordering + partial-failure itself. At minimum, document/enforce the required replay order: **open → orders → cash movements → close/force-close** (because `compute_system_cash` reads orders + movements at close time).

5. **client_ref / temp-id surfaced on shifts for reconciliation (optional but recommended).**
   Open already round-trips the client `id`, which is enough for shifts. But consider returning the client ref explicitly in responses and storing it, so the POS can map its local temp record to the server row deterministically even if it lost the `id` mapping.

6. **Widen / clarify the future-skew contract for replay.**
   `clock_skew_tolerance()` is 5 min (clock.rs). Confirm the POS re-bases offline timestamps to the server offset at sync (clock.rs's doc comment assumes this). If it can't guarantee that, widen tolerance or special-case sync requests, so a slightly-fast device's recent `created_at`/`closed_at` isn't rejected as "future."

7. **Conflict semantics for the open-uniqueness indexes during sync.**
   The one-open-per-branch / one-open-per-teller partial unique indexes will throw 409 when an offline open collides with an already-synced open. Define the contract (the handler already maps 23505 to a friendly 409 with branch-vs-teller distinction at handlers.rs:426) and make sure the batch/replay path returns it per-item so the POS can reconcile rather than abort the whole batch.

**No DB migration needed for:** accepting client timestamps (columns already `timestamptz`), shift idempotency (uses existing `id` PK). **DB migration needed for:** cash-movement `client_ref` + unique index (item 1).