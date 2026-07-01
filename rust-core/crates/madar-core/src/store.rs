//! Local store — embedded SQLite (PLAN §8). The source of truth the UI reads
//! from, online or offline:
//!   - `kv`          : read-through mirror (canonical wire JSON per key),
//!   - `outbox`      : the durable, append-only command queue (global FIFO `seq`),
//!   - `id_map`      : the client-temp-id ↔ server-id bridge for reconciliation,
//!   - `sync_cursors`: per-stream high-water mark for days-offline catch-up.
//!
//! A single writer behind a `Mutex` (FFI calls serialize here); WAL gives
//! snapshot-consistent reads. `db_path == ""` opens in-memory (tests / first boot).

use std::sync::Mutex;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::CoreResult;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS kv (
  k          TEXT PRIMARY KEY,
  v          TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

-- Binary cache (e.g. the org logo PNG for receipt printing). Separate from `kv`
-- because that column is TEXT; this one is a real BLOB.
CREATE TABLE IF NOT EXISTS blob (
  k          TEXT PRIMARY KEY,
  v          BLOB NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS id_map (
  entity_type    TEXT NOT NULL,
  client_temp_id TEXT NOT NULL,
  server_id      TEXT NOT NULL,
  PRIMARY KEY (entity_type, client_temp_id)
);

CREATE TABLE IF NOT EXISTS sync_cursors (
  stream          TEXT PRIMARY KEY,
  last_server_seq INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS outbox (
  seq             INTEGER PRIMARY KEY AUTOINCREMENT,  -- global FIFO order
  id              TEXT NOT NULL UNIQUE,               -- client-minted uuid (dedups enqueue)
  op_type         TEXT NOT NULL,                      -- create_order | void_order | open_shift | ...
  idempotency_key TEXT NOT NULL,                      -- in-body exactly-once token (dedups on the server)
  payload         TEXT NOT NULL,                      -- canonical request JSON
  event_at        TEXT NOT NULL,                      -- client real-event time (RFC3339)
  enqueued_at     TEXT NOT NULL,
  status          TEXT NOT NULL DEFAULT 'pending',    -- pending|inflight|acked|dead
  attempts        INTEGER NOT NULL DEFAULT 0,
  last_error      TEXT,
  server_id       TEXT,                               -- set on ack
  depends_on_seq  INTEGER,                            -- gate dependents (e.g. order after its open_shift)
  next_attempt_at INTEGER NOT NULL DEFAULT 0,         -- epoch ms backoff gate (0 = ready now)
  synced_at       INTEGER,                            -- epoch ms when acked (recovery-log retention)
  user_id         TEXT,                               -- teller who enqueued (drain scopes to JWT holder)
  clock_offset_ms INTEGER,                            -- device→server skew at enqueue (correct-at-sync)
  shift_id        TEXT                                -- the shift this op belongs to (close-last gating)
);
CREATE INDEX IF NOT EXISTS outbox_status_seq ON outbox(status, seq);
"#;

/// Idempotent column adds for stores created before the offline-orchestration
/// columns existed. `CREATE TABLE IF NOT EXISTS` won't alter an existing table,
/// so older DBs get the new columns here (errors on already-present columns are
/// expected and ignored).
const MIGRATIONS: &[&str] = &[
    "ALTER TABLE outbox ADD COLUMN next_attempt_at INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE outbox ADD COLUMN synced_at INTEGER",
    "ALTER TABLE outbox ADD COLUMN user_id TEXT",
    "ALTER TABLE outbox ADD COLUMN clock_offset_ms INTEGER",
    "ALTER TABLE outbox ADD COLUMN shift_id TEXT",
];

/// An op to enqueue. `id` is the client uuid (re-enqueue with the same `id` is a
/// no-op, so retries/replays don't duplicate).
#[derive(Debug, Clone, Default)]
pub struct NewOutboxOp {
    pub id: String,
    pub op_type: String,
    pub idempotency_key: String,
    pub payload: String,
    pub event_at: String,
    /// Gate: this op won't send until the op at this seq is acked.
    pub depends_on_seq: Option<i64>,
    /// The teller who enqueued it (the drain only sends the JWT holder's ops).
    pub user_id: Option<String>,
    /// Device→server clock skew (ms) captured at enqueue, for correct-at-sync.
    pub clock_offset_ms: Option<i64>,
    /// The shift this op belongs to (close-last gating; None for shift-less ops).
    pub shift_id: Option<String>,
}

/// A queued outbox row.
#[derive(Debug, Clone)]
pub struct OutboxItem {
    pub seq: i64,
    pub id: String,
    pub op_type: String,
    pub idempotency_key: String,
    pub payload: String,
    pub event_at: String,
    pub status: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub server_id: Option<String>,
    pub depends_on_seq: Option<i64>,
    pub next_attempt_at: i64,
    pub user_id: Option<String>,
    pub clock_offset_ms: Option<i64>,
    pub shift_id: Option<String>,
}

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    /// Open (or create) the store and run migrations. Empty path → in-memory.
    pub fn open(db_path: &str) -> CoreResult<Store> {
        let conn = if db_path.is_empty() {
            Connection::open_in_memory()?
        } else {
            Connection::open(db_path)?
        };
        // Best-effort pragmas (in-memory ignores WAL).
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "synchronous", "NORMAL");
        let _ = conn.pragma_update(None, "foreign_keys", "ON");
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch(SCHEMA)?;
        // Bring older stores up to the current outbox shape (no-op on fresh DBs).
        // MUST run before any index that references the new columns — on an
        // upgraded DB the columns don't exist until these ALTERs add them.
        for stmt in MIGRATIONS {
            let _ = conn.execute(stmt, []); // "duplicate column" is expected + fine
        }
        // The backoff-gate index references `next_attempt_at`, so it's created
        // only AFTER the migrations guarantee that column exists.
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS outbox_due ON outbox(status, next_attempt_at, seq)",
            [],
        );
        Ok(Store { conn: Mutex::new(conn) })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        // Poisoning only happens if a holder panicked mid-write; recover the
        // guard rather than cascading the panic across the FFI.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ── read-through mirror ─────────────────────────────────────
    pub fn kv_put(&self, key: &str, json: &str) -> CoreResult<()> {
        self.lock().execute(
            "INSERT INTO kv(k, v, updated_at) VALUES(?1, ?2, ?3)
             ON CONFLICT(k) DO UPDATE SET v=excluded.v, updated_at=excluded.updated_at",
            params![key, json, now_iso()],
        )?;
        Ok(())
    }
    pub fn kv_get(&self, key: &str) -> CoreResult<Option<String>> {
        Ok(self.lock()
            .query_row("SELECT v FROM kv WHERE k=?1", [key], |r| r.get::<_, String>(0))
            .optional()?)
    }

    /// Upsert raw bytes (e.g. the org logo PNG) into the binary cache.
    pub fn blob_put(&self, key: &str, bytes: &[u8]) -> CoreResult<()> {
        self.lock().execute(
            "INSERT INTO blob(k, v, updated_at) VALUES(?1, ?2, ?3)
             ON CONFLICT(k) DO UPDATE SET v=excluded.v, updated_at=excluded.updated_at",
            params![key, bytes, now_iso()],
        )?;
        Ok(())
    }
    pub fn blob_get(&self, key: &str) -> CoreResult<Option<Vec<u8>>> {
        Ok(self.lock()
            .query_row("SELECT v FROM blob WHERE k=?1", [key], |r| r.get::<_, Vec<u8>>(0))
            .optional()?)
    }

    // ── id_map (temp-id ↔ server-id) ────────────────────────────
    pub fn id_map_put(&self, entity: &str, client_temp_id: &str, server_id: &str) -> CoreResult<()> {
        self.lock().execute(
            "INSERT INTO id_map(entity_type, client_temp_id, server_id) VALUES(?1,?2,?3)
             ON CONFLICT(entity_type, client_temp_id) DO UPDATE SET server_id=excluded.server_id",
            params![entity, client_temp_id, server_id],
        )?;
        Ok(())
    }
    pub fn id_map_get(&self, entity: &str, client_temp_id: &str) -> CoreResult<Option<String>> {
        Ok(self.lock()
            .query_row(
                "SELECT server_id FROM id_map WHERE entity_type=?1 AND client_temp_id=?2",
                params![entity, client_temp_id], |r| r.get(0))
            .optional()?)
    }

    // ── per-stream sync cursors ─────────────────────────────────
    pub fn cursor_get(&self, stream: &str) -> CoreResult<i64> {
        Ok(self.lock()
            .query_row("SELECT last_server_seq FROM sync_cursors WHERE stream=?1", [stream], |r| r.get(0))
            .optional()?
            .unwrap_or(0))
    }
    pub fn cursor_set(&self, stream: &str, seq: i64) -> CoreResult<()> {
        self.lock().execute(
            "INSERT INTO sync_cursors(stream, last_server_seq) VALUES(?1,?2)
             ON CONFLICT(stream) DO UPDATE SET last_server_seq=excluded.last_server_seq",
            params![stream, seq],
        )?;
        Ok(())
    }

    // ── durable outbox ──────────────────────────────────────────
    /// Enqueue an op. Idempotent on `id`: re-enqueuing the same id is a no-op
    /// and returns the existing `seq` (so a double-tap or a re-run never dups).
    pub fn enqueue(&self, op: &NewOutboxOp) -> CoreResult<i64> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO outbox(id, op_type, idempotency_key, payload, event_at, enqueued_at,
                                depends_on_seq, user_id, clock_offset_ms, shift_id)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
             ON CONFLICT(id) DO NOTHING",
            params![op.id, op.op_type, op.idempotency_key, op.payload, op.event_at, now_iso(),
                    op.depends_on_seq, op.user_id, op.clock_offset_ms, op.shift_id],
        )?;
        Ok(conn.query_row("SELECT seq FROM outbox WHERE id=?1", [&op.id], |r| r.get(0))?)
    }

    /// Upsert a state-TOGGLING LAN-mirror backup (kitchen bump/unbump on one line),
    /// keyed line-scoped so the LATEST tap wins. Unlike [`Self::enqueue`] (keep-first),
    /// a repeated tap after an opposite one must OVERWRITE the queued backup — else
    /// the mirror replays a STALE direction if the originating device dies before its
    /// own primary op reaches the cloud. Only a still-re-sendable (pending/dead) row
    /// is replaced + re-armed; an in-flight send is left to finish (bump/unbump is
    /// idempotent server-side, so a momentary stale-in-flight is harmless).
    pub fn upsert_mirror(&self, op: &NewOutboxOp) -> CoreResult<()> {
        self.lock().execute(
            "INSERT INTO outbox(id, op_type, idempotency_key, payload, event_at, enqueued_at,
                                depends_on_seq, user_id, clock_offset_ms, shift_id)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
             ON CONFLICT(id) DO UPDATE SET
                payload=excluded.payload, op_type=excluded.op_type,
                idempotency_key=excluded.idempotency_key, event_at=excluded.event_at,
                status='pending', attempts=0, next_attempt_at=0, last_error=NULL
             WHERE outbox.status IN ('pending','dead')",
            params![op.id, op.op_type, op.idempotency_key, op.payload, op.event_at, now_iso(),
                    op.depends_on_seq, op.user_id, op.clock_offset_ms, op.shift_id],
        )?;
        Ok(())
    }

    /// Items ready to send NOW for the drain — `pending` AND past their backoff
    /// gate (`next_attempt_at <= now_ms`), in FIFO order. Scoped to `user_id`
    /// when given (a different teller's queued ops must sync under THEIR token,
    /// not the current holder's — legacy NULL-user rows are always included).
    pub fn due_for_sync(&self, now_ms: i64, user_id: Option<&str>) -> CoreResult<Vec<OutboxItem>> {
        let conn = self.lock();
        let (sql, mapper): (String, _) = if let Some(uid) = user_id {
            (
                format!("SELECT {COLS} FROM outbox WHERE status='pending' AND next_attempt_at<=?1 \
                         AND (user_id IS NULL OR user_id=?2) ORDER BY seq ASC"),
                Some(uid),
            )
        } else {
            (
                format!("SELECT {COLS} FROM outbox WHERE status='pending' AND next_attempt_at<=?1 ORDER BY seq ASC"),
                None,
            )
        };
        let mut stmt = conn.prepare(&sql)?;
        let rows: Vec<OutboxItem> = match mapper {
            Some(uid) => stmt.query_map(params![now_ms, uid], map_item)?.collect::<Result<Vec<_>, _>>()?,
            None => stmt.query_map(params![now_ms], map_item)?.collect::<Result<Vec<_>, _>>()?,
        };
        Ok(rows)
    }

    /// Drainable items (pending/inflight) in FIFO order — counts + the
    /// queued-orders projection. (Not backoff-gated; that's `due_for_sync`.)
    pub fn pending(&self) -> CoreResult<Vec<OutboxItem>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            &format!("SELECT {COLS} FROM outbox WHERE status IN ('pending','inflight') ORDER BY seq ASC"))?;
        let rows: Vec<OutboxItem> = stmt.query_map([], map_item)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn pending_count(&self) -> CoreResult<u32> {
        let n: i64 = self.lock().query_row(
            "SELECT COUNT(*) FROM outbox WHERE status IN ('pending','inflight')", [], |r| r.get(0))?;
        Ok(n as u32)
    }

    /// Count of dead (exhausted/rejected) outbox rows — the "needs attention"
    /// signal for the sync chip. Acked rows are gone, so this is only the stuck set.
    pub fn dead_count(&self) -> CoreResult<u32> {
        let n: i64 = self.lock().query_row(
            "SELECT COUNT(*) FROM outbox WHERE status = 'dead'", [], |r| r.get(0))?;
        Ok(n as u32)
    }

    /// Every un-acked outbox row (pending/inflight/dead) in FIFO order — the
    /// sync-center read. Acked rows are hidden (nothing to act on).
    pub fn list_active(&self) -> CoreResult<Vec<OutboxItem>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            &format!("SELECT {COLS} FROM outbox WHERE status IN ('pending','inflight','dead') ORDER BY seq ASC"))?;
        let rows: Vec<OutboxItem> = stmt.query_map([], map_item)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// The status of one row by seq (`None` if discarded) — prerequisite gating.
    pub fn status_of_seq(&self, seq: i64) -> CoreResult<Option<String>> {
        Ok(self.lock()
            .query_row("SELECT status FROM outbox WHERE seq=?1", [seq], |r| r.get(0))
            .optional()?)
    }

    /// The seq of the live (non-acked) op with this client `id`, for wiring a
    /// dependency at enqueue (e.g. an order onto its still-queued open_shift).
    /// `None` when no such row exists or it's already acked (no gate needed).
    pub fn live_seq_of(&self, id: &str) -> CoreResult<Option<i64>> {
        Ok(self.lock()
            .query_row(
                "SELECT seq FROM outbox WHERE id=?1 AND status IN ('pending','inflight','dead')",
                [id], |r| r.get(0))
            .optional()?)
    }

    /// The seq of the most recent NOT-YET-ACKED `close_shift` (pending/inflight or
    /// dead), so a freshly-opened shift can DEPEND on it: the branch must be
    /// confirmed free — the prior shift's close fully drained — before the next
    /// `open_shift` replays, or the open races the still-open prior shift and 409s
    /// ("a shift is already open for this branch"). `None` when no close is queued
    /// (the branch is already free, e.g. the prior shift closed online). A dead
    /// close cascades the dependent open dead, surfacing the stuck branch instead
    /// of dead-lettering the open with a misleading conflict.
    pub fn latest_unsynced_close_seq(&self) -> CoreResult<Option<i64>> {
        Ok(self.lock().query_row(
            "SELECT MAX(seq) FROM outbox \
             WHERE op_type='close_shift' AND status IN ('pending','inflight','dead')",
            [], |r| r.get::<_, Option<i64>>(0))?)
    }

    /// True while any order/void/cash for `shift_id` is still un-acked — pending,
    /// inflight, OR **dead** — (excluding `exclude_seq`, the close itself). A shift
    /// close must be the LAST thing that syncs for its shift; counting `dead` too
    /// means a close NEVER overtakes a failed order (which would land the close with
    /// an undercounted Z-report and strand the order). The dead write surfaces in
    /// the stuck list and the close waits until it's retried-and-acked or discarded
    /// — mirroring how the dependency gate (`drain_outbox`) and
    /// `latest_unsynced_close_seq` already treat `dead` as still-blocking. Waiting
    /// burns no retry budget, so this never deadlocks; the dead ROOT surfaces the
    /// jam. Shift-scoped, so a later shift's orders never block an earlier close.
    pub fn has_live_shift_writes(&self, shift_id: &str, exclude_seq: i64) -> CoreResult<bool> {
        let n: i64 = self.lock().query_row(
            "SELECT COUNT(*) FROM outbox \
             WHERE status IN ('pending','inflight','dead') AND shift_id=?1 AND seq<>?2 \
               AND op_type IN ('create_order','void_order','cash_movement')",
            params![shift_id, exclude_seq], |r| r.get(0))?;
        Ok(n > 0)
    }

    /// Reset every dead command back to `pending` (clearing its error + backoff)
    /// so the next drain retries it. Returns how many were requeued.
    pub fn requeue_dead(&self) -> CoreResult<u32> {
        let n = self.lock().execute(
            "UPDATE outbox SET status='pending', last_error=NULL, attempts=0, next_attempt_at=0 WHERE status='dead'", [])?;
        Ok(n as u32)
    }

    /// Discard a single DEAD command by client id (the teller gives up on it).
    /// Only dead rows can be discarded — a pending/inflight op might still land.
    pub fn discard_dead(&self, id: &str) -> CoreResult<bool> {
        let n = self.lock().execute(
            "DELETE FROM outbox WHERE id=?1 AND status='dead'", params![id])?;
        Ok(n > 0)
    }

    /// Distinct shift ids of THIS teller's never-synced (queued/dead) `open_shift`
    /// commands, excluding `keep` — i.e. shifts the device optimistically opened
    /// offline that never became real server-side. Used to recover orphaned sales:
    /// when the teller's real open shift is `keep`, every op on one of these dead
    /// shifts belongs on `keep`. Scoped to the teller so a shared-till device never
    /// re-points another teller's work.
    pub fn orphan_open_shift_ids(&self, teller_id: &str, keep: &str) -> CoreResult<Vec<String>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT shift_id FROM outbox \
             WHERE op_type='open_shift' AND status IN ('pending','inflight','dead') \
               AND user_id=?1 AND shift_id IS NOT NULL AND shift_id<>?2",
        )?;
        let rows: Vec<String> = stmt
            .query_map(params![teller_id, keep], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Shift ids whose `open_shift` op has DEAD-lettered for this teller — the roots
    /// that strand their dependent orders. Narrower than [`orphan_open_shift_ids`]
    /// (DEAD only, not pending): the drain auto-heal re-points just these onto the
    /// live shift, so a legitimately-pending sequential shift is never merged.
    pub fn dead_open_shift_ids(&self, teller_id: &str) -> CoreResult<Vec<String>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT shift_id FROM outbox \
             WHERE op_type='open_shift' AND status='dead' AND user_id=?1 AND shift_id IS NOT NULL",
        )?;
        let rows: Vec<String> = stmt
            .query_map([teller_id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Count create_order ops blocked because their `open_shift` dependency
    /// DEAD-lettered — the "stuck sales" the sync center surfaces (and the auto-heal
    /// clears). They are neither sent (dependency dead) nor lost (still queued).
    pub fn count_orders_blocked_by_dead_dep(&self) -> CoreResult<u32> {
        let n: i64 = self.lock().query_row(
            "SELECT COUNT(*) FROM outbox o \
             WHERE o.op_type='create_order' AND o.status IN ('pending','inflight') \
               AND o.depends_on_seq IS NOT NULL \
               AND EXISTS (SELECT 1 FROM outbox d WHERE d.seq=o.depends_on_seq AND d.status='dead')",
            [],
            |r| r.get(0),
        )?;
        Ok(n as u32)
    }

    /// Re-point every non-acked (pending/inflight/dead) op tied to shift `old` onto
    /// shift `new`: rewrites the `shift_id` column AND any occurrence of the id inside
    /// the JSON payload (UUIDs are unique, so a plain string replace is safe).
    ///
    /// The `open_shift` op replays idempotently against `new` because the backend
    /// dedups the open on its in-PAYLOAD `request.id` (rewritten above), NOT on the
    /// outbox row's `id`/`idempotency_key` columns (which are local bookkeeping and
    /// never sent — `/sync/replay` posts the payload envelope). Those columns are
    /// therefore left UNTOUCHED: rewriting them to `new` made every orphan open's row
    /// `id` collide on `outbox.id UNIQUE` when two orphans were re-pointed onto the
    /// same target, aborting the second remap and stranding its paid offline sales
    /// permanently. Returns rows touched.
    pub fn remap_shift(&self, old: &str, new: &str) -> CoreResult<u32> {
        let n = self.lock().execute(
            "UPDATE outbox SET \
                payload = replace(payload, ?1, ?2), \
                shift_id = ?2 \
             WHERE shift_id = ?1 AND status IN ('pending','inflight','dead')",
            params![old, new])?;
        Ok(n as u32)
    }

    /// Requeue (dead → pending, clearing error + backoff) every dead op for one
    /// shift — the recovery counterpart of [`remap_shift`], so re-pointed sales
    /// replay on the next drain. Returns rows requeued.
    pub fn requeue_dead_for_shift(&self, shift_id: &str) -> CoreResult<u32> {
        let n = self.lock().execute(
            "UPDATE outbox SET status='pending', last_error=NULL, attempts=0, next_attempt_at=0 \
             WHERE status='dead' AND shift_id=?1", params![shift_id])?;
        Ok(n as u32)
    }

    /// Mark an op inflight (about to hit the network). Crash recovery
    /// (`recover_inflight`) returns it to pending if we die before the ack.
    pub fn mark_inflight(&self, seq: i64) -> CoreResult<()> {
        self.lock().execute("UPDATE outbox SET status='inflight' WHERE seq=?1", params![seq])?;
        Ok(())
    }

    /// Crash recovery: any row stranded `inflight` (killed mid-request) goes back
    /// to `pending` so the drain retries it (idempotency makes the retry safe).
    pub fn recover_inflight(&self) -> CoreResult<u32> {
        let n = self.lock().execute("UPDATE outbox SET status='pending' WHERE status='inflight'", [])?;
        Ok(n as u32)
    }

    pub fn mark_acked(&self, seq: i64, server_id: Option<&str>) -> CoreResult<()> {
        self.lock().execute(
            "UPDATE outbox SET status='acked', server_id=?2, synced_at=?3 WHERE seq=?1",
            params![seq, server_id, now_ms()])?;
        Ok(())
    }

    pub fn mark_dead(&self, seq: i64, error: &str) -> CoreResult<()> {
        self.lock().execute(
            "UPDATE outbox SET status='dead', last_error=?2 WHERE seq=?1",
            params![seq, error])?;
        Ok(())
    }

    /// Counted retry: bump attempts, set the next backoff gate, record the error.
    pub fn mark_retry(&self, seq: i64, error: &str, next_attempt_at: i64) -> CoreResult<()> {
        self.lock().execute(
            "UPDATE outbox SET status='pending', attempts=attempts+1, last_error=?2, next_attempt_at=?3 WHERE seq=?1",
            params![seq, error, next_attempt_at])?;
        Ok(())
    }

    /// Uncounted reschedule: a connectivity blip or a 401-park must never push an
    /// op toward `dead`, so it sets the next gate WITHOUT bumping attempts.
    pub fn mark_retry_no_count(&self, seq: i64, next_attempt_at: i64) -> CoreResult<()> {
        self.lock().execute(
            "UPDATE outbox SET status='pending', next_attempt_at=?2 WHERE seq=?1",
            params![seq, next_attempt_at])?;
        Ok(())
    }

    /// Clear the connectivity (no-count) backoff gate so a freshly-confirmed
    /// reconnect drains its backlog NOW instead of waiting out the ~15s network
    /// retry window. Only touches items rescheduled purely for connectivity
    /// (`attempts=0`, a positive gate) — a counted server-error backoff
    /// (`attempts>0`) keeps its exponential gate. Returns rows un-gated.
    pub fn clear_network_backoff(&self) -> CoreResult<u32> {
        let n = self.lock().execute(
            "UPDATE outbox SET next_attempt_at=0 \
             WHERE status='pending' AND attempts=0 AND next_attempt_at>0",
            [])?;
        Ok(n as u32)
    }

    /// Drop acked recovery-log rows older than `cutoff_ms` (kept ~48h so a crash
    /// between server ack and local writes never loses the record).
    pub fn purge_acked_older_than(&self, cutoff_ms: i64) -> CoreResult<u32> {
        let n = self.lock().execute(
            "DELETE FROM outbox WHERE status='acked' AND synced_at IS NOT NULL AND synced_at < ?1",
            params![cutoff_ms])?;
        Ok(n as u32)
    }

    /// Drop every queued command. Only for an explicit destructive sign-out —
    /// offline shifts are real sales, so the default logout preserves them.
    pub fn wipe_outbox(&self) -> CoreResult<()> {
        self.lock().execute("DELETE FROM outbox", [])?;
        Ok(())
    }
}

/// The column list every `OutboxItem` SELECT shares (kept in sync with `map_item`).
const COLS: &str = "seq,id,op_type,idempotency_key,payload,event_at,status,attempts,last_error,\
                    server_id,depends_on_seq,next_attempt_at,user_id,clock_offset_ms,shift_id";

/// Map a row selected with `COLS` into an `OutboxItem`.
fn map_item(r: &rusqlite::Row<'_>) -> rusqlite::Result<OutboxItem> {
    Ok(OutboxItem {
        seq: r.get(0)?, id: r.get(1)?, op_type: r.get(2)?, idempotency_key: r.get(3)?,
        payload: r.get(4)?, event_at: r.get(5)?, status: r.get(6)?, attempts: r.get(7)?,
        last_error: r.get(8)?, server_id: r.get(9)?, depends_on_seq: r.get(10)?,
        next_attempt_at: r.get(11)?, user_id: r.get(12)?, clock_offset_ms: r.get(13)?, shift_id: r.get(14)?,
    })
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Epoch milliseconds — the unit of `next_attempt_at` / `synced_at` (matches the
/// Flutter outbox so backoff/retention windows are identical).
fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(id: &str) -> NewOutboxOp {
        NewOutboxOp {
            id: id.into(),
            op_type: "create_order".into(),
            idempotency_key: id.into(),
            payload: r#"{"total":2280}"#.into(),
            event_at: "2026-06-19T10:00:00Z".into(),
            ..Default::default()
        }
    }

    #[test]
    fn kv_roundtrip_and_overwrite() {
        let s = Store::open("").unwrap();
        assert_eq!(s.kv_get("menu").unwrap(), None);
        s.kv_put("menu", "[1,2]").unwrap();
        assert_eq!(s.kv_get("menu").unwrap().as_deref(), Some("[1,2]"));
        s.kv_put("menu", "[3]").unwrap();
        assert_eq!(s.kv_get("menu").unwrap().as_deref(), Some("[3]"));
    }

    #[test]
    fn id_map_and_cursor_roundtrip() {
        let s = Store::open("").unwrap();
        s.id_map_put("order", "local-1", "srv-99").unwrap();
        assert_eq!(s.id_map_get("order", "local-1").unwrap().as_deref(), Some("srv-99"));
        assert_eq!(s.id_map_get("order", "missing").unwrap(), None);
        assert_eq!(s.cursor_get("orders").unwrap(), 0);
        s.cursor_set("orders", 42).unwrap();
        assert_eq!(s.cursor_get("orders").unwrap(), 42);
    }

    #[test]
    fn enqueue_is_idempotent_on_id() {
        let s = Store::open("").unwrap();
        let seq1 = s.enqueue(&op("o1")).unwrap();
        let seq2 = s.enqueue(&op("o1")).unwrap(); // same id → no dup
        assert_eq!(seq1, seq2);
        assert_eq!(s.pending_count().unwrap(), 1);
    }

    #[test]
    fn outbox_fifo_and_ack_dead() {
        let s = Store::open("").unwrap();
        let a = s.enqueue(&op("a")).unwrap();
        let b = s.enqueue(&op("b")).unwrap();
        assert!(a < b);
        let pend = s.pending().unwrap();
        assert_eq!(pend.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);

        s.mark_acked(a, Some("server-a")).unwrap();
        assert_eq!(s.pending_count().unwrap(), 1);
        assert_eq!(s.pending().unwrap()[0].id, "b");

        s.mark_dead(b, "4xx rejected").unwrap();
        assert_eq!(s.pending_count().unwrap(), 0);
    }

    #[test]
    fn list_active_requeue_and_discard() {
        let s = Store::open("").unwrap();
        let a = s.enqueue(&op("a")).unwrap();
        s.enqueue(&op("b")).unwrap();
        s.mark_acked(a, Some("srv")).unwrap(); // acked → hidden from list_active

        // b still pending → shown; a (acked) hidden.
        let active = s.list_active().unwrap();
        assert_eq!(active.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), vec!["b"]);

        // Kill b, then it shows as dead and is requeue/discard-able.
        let b_seq = active[0].seq;
        s.mark_dead(b_seq, "boom").unwrap();
        assert_eq!(s.list_active().unwrap()[0].status, "dead");
        assert!(!s.discard_dead("a").unwrap()); // a is acked, not dead → no-op
        assert_eq!(s.requeue_dead().unwrap(), 1);
        assert_eq!(s.list_active().unwrap()[0].status, "pending");
        assert_eq!(s.pending_count().unwrap(), 1);

        // Kill again, then discard it.
        s.mark_dead(b_seq, "boom2").unwrap();
        assert!(s.discard_dead("b").unwrap());
        assert!(s.list_active().unwrap().is_empty());
    }

    fn op_with(id: &str, op_type: &str, shift_id: Option<&str>, depends_on_seq: Option<i64>) -> NewOutboxOp {
        NewOutboxOp {
            id: id.into(),
            op_type: op_type.into(),
            idempotency_key: id.into(),
            payload: "{}".into(),
            event_at: "2026-06-19T10:00:00Z".into(),
            depends_on_seq,
            shift_id: shift_id.map(|s| s.into()),
            ..Default::default()
        }
    }

    #[test]
    fn remap_shift_recovers_orphaned_ops_onto_the_real_shift() {
        // The offline data-loss scenario: the device optimistically opened shift B
        // offline and rang orders on it, but B could never be created server-side
        // (the branch already had the teller's real shift A open), so the open +
        // orders dead-lettered. Recovery = re-point B's ops onto A and requeue.
        let s = Store::open("").unwrap();
        const B: &str = "00000000-0000-0000-0000-0000000000bb"; // orphan offline shift
        const A: &str = "00000000-0000-0000-0000-0000000000aa"; // teller's REAL shift
        const T: &str = "00000000-0000-0000-0000-0000000000a1"; // the teller

        let mut open = op_with(B, "open_shift", Some(B), None);
        open.payload = format!("{{\"request\":{{\"id\":\"{B}\"}}}}");
        open.user_id = Some(T.into());
        let open_seq = s.enqueue(&open).unwrap();

        let order = |id: &str| {
            let mut o = op_with(id, "create_order", Some(B), Some(open_seq));
            o.payload = format!("{{\"request\":{{\"shift_id\":\"{B}\"}}}}");
            o.user_id = Some(T.into());
            o
        };
        s.enqueue(&order("order-1")).unwrap();
        let o2_seq = s.enqueue(&order("order-2")).unwrap();

        // The open + one order dead-lettered (the cascade); one order is still pending.
        s.mark_dead(open_seq, "Conflict: A shift is already open for this branch").unwrap();
        s.mark_dead(o2_seq, "a required earlier action failed to sync").unwrap();

        // 1) The teller's orphan open shift is discoverable (excluding the real one).
        assert_eq!(s.orphan_open_shift_ids(T, A).unwrap(), vec![B.to_string()]);
        // …and scoped to the teller — another teller's work is never re-pointed.
        assert!(s.orphan_open_shift_ids("00000000-0000-0000-0000-0000000000a2", A).unwrap().is_empty());
        // …and never re-points onto itself.
        assert!(s.orphan_open_shift_ids(T, B).unwrap().is_empty());

        // 2) Remap B → A: rewrites shift_id + the payload (the open op's local row
        // id is deliberately left as B — see remap_shift; idempotency rides the payload).
        assert_eq!(s.remap_shift(B, A).unwrap(), 3, "open + 2 orders re-pointed");
        for it in s.list_active().unwrap() {
            assert_eq!(it.shift_id.as_deref(), Some(A), "shift_id column re-pointed");
            assert!(!it.payload.contains(B), "stale orphan id gone: {}", it.payload);
            assert!(it.payload.contains(A), "payload carries the real shift: {}", it.payload);
        }
        let open_now = s.list_active().unwrap().into_iter().find(|i| i.op_type == "open_shift").unwrap();
        assert_eq!(open_now.id, B, "open op KEEPS its own row id (only the payload request.id → A)");
        assert!(open_now.payload.contains(A) && !open_now.payload.contains(B), "open payload request.id → A");

        // 3) Requeue the dead ones so the recovered sales replay on the next drain.
        assert_eq!(s.requeue_dead_for_shift(A).unwrap(), 2);
        assert_eq!(s.dead_count().unwrap(), 0);
        assert_eq!(s.pending_count().unwrap(), 3);
    }

    #[test]
    fn remap_shift_handles_two_orphans_onto_one_target_without_unique_collision() {
        // Critical regression (audit #2): a device optimistically opened TWO shifts
        // offline (e.g. an open, an app restart, a second open attempt) and rang sales
        // on each. Both dead-letter on reconnect (the branch already has the teller's
        // real shift T open). Recovery re-points BOTH onto T. Rewriting each open op's
        // row id to T used to collide on outbox.id UNIQUE, aborting the second remap
        // and stranding its paid offline sales forever.
        let s = Store::open("").unwrap();
        const A: &str = "00000000-0000-0000-0000-0000000000aa"; // orphan 1
        const B: &str = "00000000-0000-0000-0000-0000000000bb"; // orphan 2
        const T: &str = "00000000-0000-0000-0000-0000000000a1"; // the teller's REAL shift
        const TELLER: &str = "00000000-0000-0000-0000-0000000000c1";

        for shift in [A, B] {
            let mut open = op_with(shift, "open_shift", Some(shift), None);
            open.payload = format!("{{\"request\":{{\"id\":\"{shift}\"}}}}");
            open.user_id = Some(TELLER.into());
            let seq = s.enqueue(&open).unwrap();
            let mut order = op_with(&format!("order-{shift}"), "create_order", Some(shift), Some(seq));
            order.payload = format!("{{\"request\":{{\"shift_id\":\"{shift}\"}}}}");
            order.user_id = Some(TELLER.into());
            s.enqueue(&order).unwrap();
            s.mark_dead(seq, "Conflict: a shift is already open for this branch").unwrap();
        }

        // BOTH remaps must succeed — the second no longer collides on outbox.id.
        assert_eq!(s.remap_shift(A, T).unwrap(), 2, "orphan A: open + order re-pointed");
        assert_eq!(s.remap_shift(B, T).unwrap(), 2, "orphan B: open + order re-pointed (NO UNIQUE collision)");

        // Every op now targets T, and BOTH orphan opens survive with their own row ids.
        let active = s.list_active().unwrap();
        assert!(active.iter().all(|it| it.shift_id.as_deref() == Some(T)), "all re-pointed to T");
        assert!(
            active.iter().all(|it| it.payload.contains(T) && !it.payload.contains(A) && !it.payload.contains(B)),
            "every payload carries T, none the orphan ids",
        );
        let opens: Vec<String> = active.iter().filter(|i| i.op_type == "open_shift").map(|i| i.id.clone()).collect();
        assert_eq!(opens.len(), 2, "both orphan opens preserved");
        assert!(opens.contains(&A.to_string()) && opens.contains(&B.to_string()), "each keeps its unique row id");

        // None stranded: requeue brings both shifts' dead opens back to pending.
        assert_eq!(s.requeue_dead_for_shift(T).unwrap(), 2, "both orphan opens requeued, nothing lost");
    }

    #[test]
    fn latest_unsynced_close_seq_gates_the_next_open() {
        // The sequential-handover gate: a freshly-opened shift DEPENDS on the prior
        // shift's still-queued close, so the open never races the un-closed branch.
        let s = Store::open("").unwrap();

        // Nothing queued → no gate (the branch is free / prior shift closed online).
        assert_eq!(s.latest_unsynced_close_seq().unwrap(), None);

        // An order + an open are NOT closes — they never gate a later open.
        s.enqueue(&op_with("ord", "create_order", Some("S0"), None)).unwrap();
        s.enqueue(&op_with("openS0", "open_shift", Some("S0"), None)).unwrap();
        assert_eq!(s.latest_unsynced_close_seq().unwrap(), None, "only close_shift gates");

        // A pending close → the next open must depend on it.
        let close_a = s.enqueue(&op_with("closeA", "close_shift", Some("A"), None)).unwrap();
        assert_eq!(s.latest_unsynced_close_seq().unwrap(), Some(close_a));

        // The MOST RECENT un-synced close wins (sequential handover, latest branch state).
        let close_b = s.enqueue(&op_with("closeB", "close_shift", Some("B"), None)).unwrap();
        assert!(close_b > close_a);
        assert_eq!(s.latest_unsynced_close_seq().unwrap(), Some(close_b));

        // An INFLIGHT close still gates (it hasn't landed yet).
        s.mark_inflight(close_b).unwrap();
        assert_eq!(s.latest_unsynced_close_seq().unwrap(), Some(close_b));

        // A DEAD close still gates — the open then cascades dead (branch stuck),
        // surfacing the jam instead of dead-lettering the open on a 409.
        s.mark_dead(close_b, "boom").unwrap();
        assert_eq!(s.latest_unsynced_close_seq().unwrap(), Some(close_b));

        // Once the latest close ACKS, the gate falls back to the earlier un-synced
        // close; when ALL closes ack, the branch is free → no gate.
        s.mark_acked(close_b, None).unwrap();
        assert_eq!(s.latest_unsynced_close_seq().unwrap(), Some(close_a), "falls back to the earlier open close");
        s.mark_acked(close_a, None).unwrap();
        assert_eq!(s.latest_unsynced_close_seq().unwrap(), None, "all closes landed → branch free");
    }

    #[test]
    fn open_upgrades_an_old_schema_db_without_erroring() {
        // Reproduces the startup crash: an app updated in place has a DB whose
        // `outbox` predates the offline-orchestration columns. Opening it MUST
        // migrate (not fail on the backoff index that references a new column).
        let path = std::env::temp_dir().join("madar_old_schema_upgrade_test.sqlite");
        let _ = std::fs::remove_file(&path);
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE outbox (
                   seq INTEGER PRIMARY KEY AUTOINCREMENT,
                   id TEXT NOT NULL UNIQUE, op_type TEXT NOT NULL,
                   idempotency_key TEXT NOT NULL, payload TEXT NOT NULL,
                   event_at TEXT NOT NULL, enqueued_at TEXT NOT NULL,
                   status TEXT NOT NULL DEFAULT 'pending', attempts INTEGER NOT NULL DEFAULT 0,
                   last_error TEXT, server_id TEXT, depends_on_seq INTEGER);
                 INSERT INTO outbox(id,op_type,idempotency_key,payload,event_at,enqueued_at)
                   VALUES('old-1','create_order','old-1','{}','t','t');",
            )
            .unwrap();
        }
        // Opening with the CURRENT schema must NOT error (the bug threw here).
        let s = Store::open(path.to_str().unwrap()).expect("open must migrate, not crash");
        // The pre-existing row survives and the new columns defaulted sanely.
        let due = s.due_for_sync(now_ms() + 1, None).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, "old-1");
        assert_eq!(due[0].next_attempt_at, 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn due_for_sync_respects_backoff_gate() {
        let s = Store::open("").unwrap();
        let a = s.enqueue(&op("a")).unwrap();
        s.enqueue(&op("b")).unwrap();
        // Back 'a' off into the future; only 'b' is due now.
        s.mark_retry(a, "transient", 9_000_000_000_000).unwrap();
        let due: Vec<_> = s.due_for_sync(1_000, None).unwrap().into_iter().map(|i| i.id).collect();
        assert_eq!(due, vec!["b"]);
        // Past 'a's gate, both are due, FIFO.
        let due2: Vec<_> = s.due_for_sync(9_999_999_999_999, None).unwrap().into_iter().map(|i| i.id).collect();
        assert_eq!(due2, vec!["a", "b"]);
        // mark_retry bumped attempts; mark_retry_no_count must not.
        assert_eq!(s.due_for_sync(9_999_999_999_999, None).unwrap()[0].attempts, 1);
    }

    #[test]
    fn user_scoping_excludes_other_tellers() {
        let s = Store::open("").unwrap();
        s.enqueue(&NewOutboxOp { user_id: Some("alice".into()), ..op("a") }).unwrap();
        s.enqueue(&NewOutboxOp { user_id: Some("bob".into()), ..op("b") }).unwrap();
        s.enqueue(&op("legacy")).unwrap(); // NULL user → always included
        let due: Vec<_> = s.due_for_sync(now_ms() + 1, Some("alice")).unwrap().into_iter().map(|i| i.id).collect();
        assert_eq!(due, vec!["a", "legacy"]);
    }

    #[test]
    fn gating_and_dependency_lookups() {
        let s = Store::open("").unwrap();
        let open = s.enqueue(&op_with("shiftX", "open_shift", Some("shiftX"), None)).unwrap();
        let ord = s.enqueue(&op_with("o1", "create_order", Some("shiftX"), Some(open))).unwrap();
        s.enqueue(&op_with("shiftX:close", "close_shift", Some("shiftX"), Some(open))).unwrap();

        assert_eq!(s.status_of_seq(open).unwrap().as_deref(), Some("pending"));
        assert_eq!(s.live_seq_of("shiftX").unwrap(), Some(open));
        // The close must wait — an order for the shift is still live.
        assert!(s.has_live_shift_writes("shiftX", s.live_seq_of("shiftX:close").unwrap().unwrap()).unwrap());
        // Order acked → no live shift writes left → close may proceed.
        s.mark_acked(ord, Some("srv-o1")).unwrap();
        assert!(!s.has_live_shift_writes("shiftX", s.live_seq_of("shiftX:close").unwrap().unwrap()).unwrap());
        // Acked op is no longer a live dependency target.
        assert_eq!(s.live_seq_of("o1").unwrap(), None);
    }

    #[test]
    fn inflight_recovery_and_purge() {
        let s = Store::open("").unwrap();
        let a = s.enqueue(&op("a")).unwrap();
        s.mark_inflight(a).unwrap();
        assert!(s.due_for_sync(now_ms() + 1, None).unwrap().is_empty()); // inflight isn't due
        assert_eq!(s.recover_inflight().unwrap(), 1);
        assert_eq!(s.due_for_sync(now_ms() + 1, None).unwrap()[0].id, "a"); // back to pending
        // Acked + purge.
        let seq = s.due_for_sync(now_ms() + 1, None).unwrap()[0].seq;
        s.mark_acked(seq, Some("srv")).unwrap();
        assert_eq!(s.purge_acked_older_than(now_ms() - 1000).unwrap(), 0); // too fresh
        assert_eq!(s.purge_acked_older_than(now_ms() + 1000).unwrap(), 1); // now purged
    }

    // ════════════════════════════════════════════════════════════════════════
    // UPGRADE / MIGRATION PATH
    //
    // An app updated in place inherits a DB whose `outbox` predates the
    // offline-orchestration columns. `Store::open` MUST migrate it (adding the
    // missing columns + the backoff-gate index) rather than crash on the index
    // that references a column that doesn't exist yet. One test per missing
    // column, a fully-old-schema variant, and a re-open idempotency check.
    // ════════════════════════════════════════════════════════════════════════

    /// Unique on-disk path per test (no shared-fixture collisions when the suite
    /// runs in parallel). Pre-removed so a leftover from a prior run can't taint.
    fn temp_db(tag: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("madar_store_mig_{tag}.sqlite"));
        let _ = std::fs::remove_file(&path);
        // WAL/SHM siblings can linger and confuse a re-create; clear them too.
        let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
        path
    }

    /// Build an `outbox` table whose column set is the modern shape MINUS the
    /// columns named in `omit`, then insert one pending row. The omitted columns
    /// are exactly the ones the migrations are responsible for adding back. The
    /// `outbox_due` index is intentionally NOT created (old DBs lacked it).
    fn build_old_outbox(path: &std::path::Path, omit: &[&str]) {
        // The full post-migration column set, in DDL order, with type+constraints.
        let all: &[(&str, &str)] = &[
            ("next_attempt_at", "INTEGER NOT NULL DEFAULT 0"),
            ("synced_at", "INTEGER"),
            ("user_id", "TEXT"),
            ("clock_offset_ms", "INTEGER"),
            ("shift_id", "TEXT"),
        ];
        // Columns that always existed pre-orchestration (never omitted).
        let mut cols = vec![
            "seq INTEGER PRIMARY KEY AUTOINCREMENT".to_string(),
            "id TEXT NOT NULL UNIQUE".to_string(),
            "op_type TEXT NOT NULL".to_string(),
            "idempotency_key TEXT NOT NULL".to_string(),
            "payload TEXT NOT NULL".to_string(),
            "event_at TEXT NOT NULL".to_string(),
            "enqueued_at TEXT NOT NULL".to_string(),
            "status TEXT NOT NULL DEFAULT 'pending'".to_string(),
            "attempts INTEGER NOT NULL DEFAULT 0".to_string(),
            "last_error TEXT".to_string(),
            "server_id TEXT".to_string(),
            "depends_on_seq INTEGER".to_string(),
        ];
        for (name, decl) in all {
            if !omit.contains(name) {
                cols.push(format!("{name} {decl}"));
            }
        }
        let ddl = format!("CREATE TABLE outbox (\n  {}\n);", cols.join(",\n  "));
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(&ddl).unwrap();
        // The other tables exist in old DBs too; create them so a real open is faithful.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS kv (k TEXT PRIMARY KEY, v TEXT NOT NULL, updated_at TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS id_map (entity_type TEXT NOT NULL, client_temp_id TEXT NOT NULL, server_id TEXT NOT NULL, PRIMARY KEY(entity_type, client_temp_id));
             CREATE TABLE IF NOT EXISTS sync_cursors (stream TEXT PRIMARY KEY, last_server_seq INTEGER NOT NULL DEFAULT 0);
             CREATE INDEX IF NOT EXISTS outbox_status_seq ON outbox(status, seq);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO outbox(id,op_type,idempotency_key,payload,event_at,enqueued_at)
             VALUES('old-1','create_order','old-1','{}','2026-06-19T10:00:00Z','2026-06-19T10:00:00Z')",
            [],
        )
        .unwrap();
        // Drop the connection (and its lock) before the Store re-opens the file.
        drop(conn);
    }

    /// After a migrating open, the single pre-existing 'old-1' row must survive,
    /// be readable through the `COLS` mapper (proving every new column exists and
    /// maps), and have sane defaults: ready-now backoff, NULL orchestration fields.
    fn assert_old_row_survives_with_defaults(s: &Store) {
        // due_for_sync exercises next_attempt_at + user scoping over the migrated row.
        let due = s.due_for_sync(now_ms() + 10_000, None).unwrap();
        assert_eq!(due.len(), 1, "the migrated row must be due");
        let row = &due[0];
        assert_eq!(row.id, "old-1");
        assert_eq!(row.status, "pending");
        assert_eq!(row.next_attempt_at, 0, "missing next_attempt_at defaults to 0 (ready now)");
        assert_eq!(row.user_id, None, "legacy row has NULL user_id");
        assert_eq!(row.clock_offset_ms, None);
        assert_eq!(row.shift_id, None);
        // pending() round-trips the same row through the full mapper independently.
        assert_eq!(s.pending().unwrap().len(), 1);
        // The backoff-gate index must now exist (open creates it post-migration).
        let cnt: i64 = s
            .lock()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='outbox_due'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cnt, 1, "outbox_due index must exist after open");
    }

    #[test]
    fn migrate_missing_next_attempt_at() {
        let path = temp_db("missing_naa");
        build_old_outbox(&path, &["next_attempt_at"]);
        let s = Store::open(path.to_str().unwrap()).expect("open must add next_attempt_at + index");
        assert_old_row_survives_with_defaults(&s);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn migrate_missing_synced_at() {
        let path = temp_db("missing_synced");
        build_old_outbox(&path, &["synced_at"]);
        let s = Store::open(path.to_str().unwrap()).expect("open must add synced_at");
        assert_old_row_survives_with_defaults(&s);
        // purge keys off synced_at; with the column freshly added the row is NULL,
        // so it must never be purged (and the query must not error on the new col).
        s.enqueue(&op("fresh")).unwrap();
        assert_eq!(s.purge_acked_older_than(now_ms() + 10_000).unwrap(), 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn migrate_missing_user_id() {
        let path = temp_db("missing_user");
        build_old_outbox(&path, &["user_id"]);
        let s = Store::open(path.to_str().unwrap()).expect("open must add user_id");
        assert_old_row_survives_with_defaults(&s);
        // The NULL-user legacy row is included under any teller's scope.
        let scoped: Vec<_> = s
            .due_for_sync(now_ms() + 10_000, Some("alice"))
            .unwrap()
            .into_iter()
            .map(|i| i.id)
            .collect();
        assert_eq!(scoped, vec!["old-1"]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn migrate_missing_clock_offset_ms() {
        let path = temp_db("missing_clock");
        build_old_outbox(&path, &["clock_offset_ms"]);
        let s = Store::open(path.to_str().unwrap()).expect("open must add clock_offset_ms");
        assert_old_row_survives_with_defaults(&s);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn migrate_missing_shift_id() {
        let path = temp_db("missing_shift");
        build_old_outbox(&path, &["shift_id"]);
        let s = Store::open(path.to_str().unwrap()).expect("open must add shift_id");
        assert_old_row_survives_with_defaults(&s);
        // shift gating queries the freshly-added column without erroring.
        assert!(!s.has_live_shift_writes("any-shift", -1).unwrap());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn migrate_fully_old_schema_all_columns_missing() {
        // The realistic upgrade: NONE of the orchestration columns nor the
        // outbox_due index exist. All five ALTERs + the index must run.
        let path = temp_db("all_missing");
        build_old_outbox(
            &path,
            &["next_attempt_at", "synced_at", "user_id", "clock_offset_ms", "shift_id"],
        );
        let s = Store::open(path.to_str().unwrap()).expect("open must fully migrate, not crash");
        assert_old_row_survives_with_defaults(&s);
        // The migrated store is fully functional end-to-end: enqueue + drain + ack.
        let seq = s.due_for_sync(now_ms() + 10_000, None).unwrap()[0].seq;
        s.mark_inflight(seq).unwrap();
        s.mark_acked(seq, Some("srv")).unwrap();
        assert_eq!(s.pending_count().unwrap(), 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reopen_twice_is_idempotent() {
        // Re-running migrations on an already-current DB must be a no-op (the
        // ALTERs raise "duplicate column" which open swallows). Open three times.
        let path = temp_db("reopen_idem");
        build_old_outbox(&path, &["next_attempt_at", "synced_at", "user_id", "clock_offset_ms", "shift_id"]);
        {
            let s = Store::open(path.to_str().unwrap()).expect("first open migrates");
            s.enqueue(&op("after-migrate")).unwrap();
            assert_eq!(s.pending_count().unwrap(), 2); // old-1 + after-migrate
        }
        {
            let s = Store::open(path.to_str().unwrap()).expect("second open is a no-op migrate");
            assert_eq!(s.pending_count().unwrap(), 2);
            // old-1 still carries its migrated defaults (don't assert total count
            // here — there are now 2 pending rows).
            let old = s
                .due_for_sync(now_ms() + 10_000, None)
                .unwrap()
                .into_iter()
                .find(|i| i.id == "old-1")
                .expect("old-1 still present after re-open");
            assert_eq!(old.status, "pending");
            assert_eq!(old.next_attempt_at, 0);
            assert_eq!(old.user_id, None);
            assert_eq!(old.clock_offset_ms, None);
            assert_eq!(old.shift_id, None);
        }
        {
            // Third open on the now-modern DB must also succeed unchanged.
            let s = Store::open(path.to_str().unwrap()).expect("third open still fine");
            assert_eq!(s.pending_count().unwrap(), 2);
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn open_on_fresh_db_runs_migrations_as_noops() {
        // A brand-new DB already has every column via SCHEMA; the ALTERs all hit
        // "duplicate column" and are swallowed, and open still succeeds.
        let path = temp_db("fresh_noop");
        {
            let s = Store::open(path.to_str().unwrap()).expect("fresh open");
            s.enqueue(&op("x")).unwrap();
        }
        let s = Store::open(path.to_str().unwrap()).expect("re-open fresh DB");
        assert_eq!(s.pending_count().unwrap(), 1);
        let _ = std::fs::remove_file(&path);
    }

    // ════════════════════════════════════════════════════════════════════════
    // KV / id_map / cursors — boundary + overwrite coverage
    // ════════════════════════════════════════════════════════════════════════

    #[test]
    fn kv_empty_value_and_missing_key() {
        let s = Store::open("").unwrap();
        // Missing key → None.
        assert_eq!(s.kv_get("nope").unwrap(), None);
        // Empty-string value is a real stored value, distinct from absent.
        s.kv_put("blank", "").unwrap();
        assert_eq!(s.kv_get("blank").unwrap().as_deref(), Some(""));
        // Overwrite back to non-empty.
        s.kv_put("blank", "x").unwrap();
        assert_eq!(s.kv_get("blank").unwrap().as_deref(), Some("x"));
    }

    #[test]
    fn id_map_overwrite_updates_server_id() {
        let s = Store::open("").unwrap();
        s.id_map_put("order", "local-1", "srv-1").unwrap();
        // Re-put same (entity, temp) updates the server id in place.
        s.id_map_put("order", "local-1", "srv-2").unwrap();
        assert_eq!(s.id_map_get("order", "local-1").unwrap().as_deref(), Some("srv-2"));
        // Same temp id under a DIFFERENT entity type is a distinct row.
        s.id_map_put("shift", "local-1", "srv-shift").unwrap();
        assert_eq!(s.id_map_get("shift", "local-1").unwrap().as_deref(), Some("srv-shift"));
        assert_eq!(s.id_map_get("order", "local-1").unwrap().as_deref(), Some("srv-2"));
    }

    #[test]
    fn cursor_defaults_zero_and_overwrites() {
        let s = Store::open("").unwrap();
        assert_eq!(s.cursor_get("unknown").unwrap(), 0); // default for absent stream
        s.cursor_set("orders", 10).unwrap();
        s.cursor_set("orders", 5).unwrap(); // set is an unconditional overwrite (not max)
        assert_eq!(s.cursor_get("orders").unwrap(), 5);
        // Streams are independent.
        s.cursor_set("shifts", 99).unwrap();
        assert_eq!(s.cursor_get("orders").unwrap(), 5);
        assert_eq!(s.cursor_get("shifts").unwrap(), 99);
    }

    // ════════════════════════════════════════════════════════════════════════
    // due_for_sync — backoff boundary + FIFO + status filtering
    // ════════════════════════════════════════════════════════════════════════

    #[test]
    fn due_for_sync_gate_is_inclusive_boundary() {
        let s = Store::open("").unwrap();
        let a = s.enqueue(&op("a")).unwrap();
        // Gate the row exactly at t=1000.
        s.mark_retry(a, "x", 1000).unwrap();
        // now < gate → not due.
        assert!(s.due_for_sync(999, None).unwrap().is_empty());
        // now == gate → due (predicate is `next_attempt_at <= now_ms`).
        assert_eq!(s.due_for_sync(1000, None).unwrap().len(), 1);
        // now > gate → due.
        assert_eq!(s.due_for_sync(1001, None).unwrap().len(), 1);
    }

    #[test]
    fn due_for_sync_excludes_non_pending_statuses() {
        let s = Store::open("").unwrap();
        let a = s.enqueue(&op("a")).unwrap();
        let b = s.enqueue(&op("b")).unwrap();
        let c = s.enqueue(&op("c")).unwrap();
        s.mark_inflight(a).unwrap(); // inflight: excluded
        s.mark_acked(b, Some("srv")).unwrap(); // acked: excluded
        // dead c: excluded too.
        s.mark_dead(c, "boom").unwrap();
        assert!(s.due_for_sync(now_ms() + 10_000, None).unwrap().is_empty());
    }

    #[test]
    fn due_for_sync_is_fifo_by_seq() {
        let s = Store::open("").unwrap();
        // Enqueue out of "alphabetical" order to prove ordering is by seq, not id.
        s.enqueue(&op("zeta")).unwrap();
        s.enqueue(&op("alpha")).unwrap();
        s.enqueue(&op("mid")).unwrap();
        let ids: Vec<_> = s.due_for_sync(now_ms() + 10_000, None).unwrap().into_iter().map(|i| i.id).collect();
        assert_eq!(ids, vec!["zeta", "alpha", "mid"]);
    }

    #[test]
    fn due_for_sync_empty_queue_returns_empty() {
        let s = Store::open("").unwrap();
        assert!(s.due_for_sync(now_ms(), None).unwrap().is_empty());
        assert!(s.due_for_sync(now_ms(), Some("alice")).unwrap().is_empty());
    }

    #[test]
    fn clear_network_backoff_ungates_only_no_count_pending() {
        let s = Store::open("").unwrap();
        let net = s.enqueue(&op("net")).unwrap();
        let srv = s.enqueue(&op("srv")).unwrap();
        s.enqueue(&op("fresh")).unwrap(); // never attempted; gate already 0
        // A connectivity blip reschedules WITHOUT counting (attempts stays 0).
        s.mark_retry_no_count(net, now_ms() + 60_000).unwrap();
        // A server error backs off WITH a count bump (attempts=1).
        s.mark_retry(srv, "5xx", now_ms() + 60_000).unwrap();
        // Before: only "fresh" is due (net + srv gated into the future).
        let before: Vec<_> = s.due_for_sync(now_ms(), None).unwrap().into_iter().map(|i| i.id).collect();
        assert_eq!(before, vec!["fresh"]);
        // The reconnect path un-gates ONLY the no-count item.
        assert_eq!(s.clear_network_backoff().unwrap(), 1);
        let after: Vec<_> = s.due_for_sync(now_ms(), None).unwrap().into_iter().map(|i| i.id).collect();
        assert!(after.contains(&"net".to_string()), "the connectivity-gated item is now due");
        assert!(!after.contains(&"srv".to_string()), "the counted server backoff stays gated");
        assert!(after.contains(&"fresh".to_string()));
    }

    #[test]
    fn due_for_sync_scoped_excludes_pure_other_teller() {
        let s = Store::open("").unwrap();
        s.enqueue(&NewOutboxOp { user_id: Some("bob".into()), ..op("b") }).unwrap();
        // Alice's scope sees nothing (bob's op + no legacy NULL rows).
        assert!(s.due_for_sync(now_ms() + 10_000, Some("alice")).unwrap().is_empty());
        // Bob's scope sees his own op.
        let ids: Vec<_> = s.due_for_sync(now_ms() + 10_000, Some("bob")).unwrap().into_iter().map(|i| i.id).collect();
        assert_eq!(ids, vec!["b"]);
    }

    // ════════════════════════════════════════════════════════════════════════
    // status_of_seq / live_seq_of
    // ════════════════════════════════════════════════════════════════════════

    #[test]
    fn status_of_seq_tracks_transitions_and_unknown() {
        let s = Store::open("").unwrap();
        assert_eq!(s.status_of_seq(99999).unwrap(), None); // no such seq
        let a = s.enqueue(&op("a")).unwrap();
        assert_eq!(s.status_of_seq(a).unwrap().as_deref(), Some("pending"));
        s.mark_inflight(a).unwrap();
        assert_eq!(s.status_of_seq(a).unwrap().as_deref(), Some("inflight"));
        s.mark_dead(a, "boom").unwrap();
        assert_eq!(s.status_of_seq(a).unwrap().as_deref(), Some("dead"));
        s.requeue_dead().unwrap();
        assert_eq!(s.status_of_seq(a).unwrap().as_deref(), Some("pending"));
        s.mark_acked(a, None).unwrap();
        assert_eq!(s.status_of_seq(a).unwrap().as_deref(), Some("acked"));
    }

    #[test]
    fn live_seq_of_covers_live_states_and_acked_and_missing() {
        let s = Store::open("").unwrap();
        assert_eq!(s.live_seq_of("ghost").unwrap(), None); // never enqueued
        let a = s.enqueue(&op("a")).unwrap();
        assert_eq!(s.live_seq_of("a").unwrap(), Some(a)); // pending counts as live
        s.mark_inflight(a).unwrap();
        assert_eq!(s.live_seq_of("a").unwrap(), Some(a)); // inflight counts
        s.mark_dead(a, "x").unwrap();
        assert_eq!(s.live_seq_of("a").unwrap(), Some(a)); // dead counts (still a real row)
        s.mark_acked(a, Some("srv")).unwrap();
        assert_eq!(s.live_seq_of("a").unwrap(), None); // acked is NOT a live dep target
    }

    // ════════════════════════════════════════════════════════════════════════
    // has_live_shift_writes — close-last gating
    // ════════════════════════════════════════════════════════════════════════

    #[test]
    fn has_live_shift_writes_excludes_self_seq() {
        let s = Store::open("").unwrap();
        // Only the close itself is live for the shift.
        let close = s.enqueue(&op_with("close", "close_shift", Some("sh1"), None)).unwrap();
        // Excluding the close's own seq → no OTHER live writes → false.
        assert!(!s.has_live_shift_writes("sh1", close).unwrap());
        // Without excluding it... close_shift isn't a counted op_type anyway → still false.
        assert!(!s.has_live_shift_writes("sh1", -1).unwrap());
    }

    #[test]
    fn has_live_shift_writes_counts_only_relevant_op_types() {
        let s = Store::open("").unwrap();
        // A non-write op for the shift (e.g. open_shift) must NOT gate the close.
        s.enqueue(&op_with("open", "open_shift", Some("sh1"), None)).unwrap();
        assert!(!s.has_live_shift_writes("sh1", -1).unwrap());
        // A real write (create_order) DOES gate it.
        s.enqueue(&op_with("o1", "create_order", Some("sh1"), None)).unwrap();
        assert!(s.has_live_shift_writes("sh1", -1).unwrap());
        // void_order and cash_movement gate too.
        let s2 = Store::open("").unwrap();
        s2.enqueue(&op_with("v", "void_order", Some("sh2"), None)).unwrap();
        assert!(s2.has_live_shift_writes("sh2", -1).unwrap());
        let s3 = Store::open("").unwrap();
        s3.enqueue(&op_with("c", "cash_movement", Some("sh3"), None)).unwrap();
        assert!(s3.has_live_shift_writes("sh3", -1).unwrap());
    }

    #[test]
    fn has_live_shift_writes_is_shift_scoped() {
        let s = Store::open("").unwrap();
        // A live order belongs to a DIFFERENT shift; sh1's close is unblocked.
        s.enqueue(&op_with("o-other", "create_order", Some("sh2"), None)).unwrap();
        assert!(!s.has_live_shift_writes("sh1", -1).unwrap());
        assert!(s.has_live_shift_writes("sh2", -1).unwrap());
    }

    #[test]
    fn has_live_shift_writes_gates_on_unacked_writes_including_dead() {
        let s = Store::open("").unwrap();
        let o = s.enqueue(&op_with("o1", "create_order", Some("sh1"), None)).unwrap();
        assert!(s.has_live_shift_writes("sh1", -1).unwrap());
        // An ACKED write no longer gates (the close may proceed past a landed order).
        s.mark_acked(o, Some("srv")).unwrap();
        assert!(!s.has_live_shift_writes("sh1", -1).unwrap());
        // A DEAD write STILL gates — the close must not overtake a failed order
        // (that would land an undercounted Z-report and strand the sale). It clears
        // only when the dead write is retried-and-acked or explicitly discarded.
        let o2 = s.enqueue(&op_with("o2", "create_order", Some("sh1"), None)).unwrap();
        s.mark_dead(o2, "boom").unwrap();
        assert!(s.has_live_shift_writes("sh1", -1).unwrap());
        // Discarding the dead row releases the gate.
        assert!(s.discard_dead("o2").unwrap());
        assert!(!s.has_live_shift_writes("sh1", -1).unwrap());
        // Inflight write DOES gate.
        let o3 = s.enqueue(&op_with("o3", "create_order", Some("sh1"), None)).unwrap();
        s.mark_inflight(o3).unwrap();
        assert!(s.has_live_shift_writes("sh1", -1).unwrap());
    }

    // ════════════════════════════════════════════════════════════════════════
    // retry semantics — attempts bump vs no-count
    // ════════════════════════════════════════════════════════════════════════

    #[test]
    fn mark_retry_bumps_attempts_and_sets_gate_and_error() {
        let s = Store::open("").unwrap();
        let a = s.enqueue(&op("a")).unwrap();
        s.mark_inflight(a).unwrap();
        s.mark_retry(a, "503 transient", 5000).unwrap();
        let row = &s.due_for_sync(5000, None).unwrap()[0]; // back to pending, gate hit at 5000
        assert_eq!(row.status, "pending");
        assert_eq!(row.attempts, 1);
        assert_eq!(row.next_attempt_at, 5000);
        assert_eq!(row.last_error.as_deref(), Some("503 transient"));
        // A second counted retry bumps again.
        s.mark_retry(a, "503 again", 6000).unwrap();
        assert_eq!(s.due_for_sync(6000, None).unwrap()[0].attempts, 2);
    }

    #[test]
    fn mark_retry_no_count_reschedules_without_bumping_attempts() {
        let s = Store::open("").unwrap();
        let a = s.enqueue(&op("a")).unwrap();
        // First, a counted retry to push attempts to 1 and set an error.
        s.mark_retry(a, "real failure", 1000).unwrap();
        assert_eq!(s.due_for_sync(1000, None).unwrap()[0].attempts, 1);
        // A no-count reschedule moves the gate but leaves attempts AND last_error.
        s.mark_inflight(a).unwrap();
        s.mark_retry_no_count(a, 8000).unwrap();
        let row = &s.due_for_sync(8000, None).unwrap()[0];
        assert_eq!(row.status, "pending");
        assert_eq!(row.attempts, 1, "no-count must not bump attempts");
        assert_eq!(row.next_attempt_at, 8000);
        assert_eq!(row.last_error.as_deref(), Some("real failure"), "error preserved");
        // It is gated until 8000.
        assert!(s.due_for_sync(7999, None).unwrap().is_empty());
    }

    // ════════════════════════════════════════════════════════════════════════
    // ack / dead / purge / requeue / discard — extra edges
    // ════════════════════════════════════════════════════════════════════════

    #[test]
    fn mark_acked_with_none_server_id_still_sets_synced_at() {
        let s = Store::open("").unwrap();
        let a = s.enqueue(&op("a")).unwrap();
        s.mark_acked(a, None).unwrap();
        assert_eq!(s.status_of_seq(a).unwrap().as_deref(), Some("acked"));
        // synced_at was set (now_ms), so a future-cutoff purge collects it.
        assert_eq!(s.purge_acked_older_than(now_ms() + 60_000).unwrap(), 1);
    }

    #[test]
    fn purge_only_touches_acked_rows() {
        let s = Store::open("").unwrap();
        let a = s.enqueue(&op("a")).unwrap();
        s.enqueue(&op("b")).unwrap(); // stays pending
        s.mark_acked(a, Some("srv")).unwrap();
        // Generous cutoff: only the acked 'a' is removed; pending 'b' survives.
        assert_eq!(s.purge_acked_older_than(now_ms() + 60_000).unwrap(), 1);
        assert_eq!(s.pending_count().unwrap(), 1);
        assert_eq!(s.pending().unwrap()[0].id, "b");
    }

    #[test]
    fn requeue_dead_resets_attempts_error_and_gate() {
        let s = Store::open("").unwrap();
        let a = s.enqueue(&op("a")).unwrap();
        let b = s.enqueue(&op("b")).unwrap();
        // Drive 'a' to dead with a bumped attempt + future gate + error.
        s.mark_retry(a, "boom", 9_000_000_000_000).unwrap();
        s.mark_dead(a, "exhausted").unwrap();
        s.mark_dead(b, "rejected").unwrap();
        assert_eq!(s.dead_count().unwrap(), 2);
        // Requeue clears status→pending, attempts→0, error→NULL, gate→0.
        assert_eq!(s.requeue_dead().unwrap(), 2);
        assert_eq!(s.dead_count().unwrap(), 0);
        let row = s.due_for_sync(0, None).unwrap().into_iter().find(|i| i.id == "a").unwrap();
        assert_eq!(row.attempts, 0);
        assert_eq!(row.next_attempt_at, 0);
        assert_eq!(row.last_error, None);
    }

    #[test]
    fn requeue_dead_on_empty_returns_zero() {
        let s = Store::open("").unwrap();
        assert_eq!(s.requeue_dead().unwrap(), 0);
        s.enqueue(&op("a")).unwrap(); // pending, not dead
        assert_eq!(s.requeue_dead().unwrap(), 0);
    }

    #[test]
    fn discard_dead_only_removes_dead_rows() {
        let s = Store::open("").unwrap();
        let a = s.enqueue(&op("a")).unwrap();
        // Pending → cannot discard.
        assert!(!s.discard_dead("a").unwrap());
        s.mark_inflight(a).unwrap();
        assert!(!s.discard_dead("a").unwrap()); // inflight → cannot discard
        s.mark_dead(a, "x").unwrap();
        assert!(s.discard_dead("a").unwrap()); // dead → removed
        assert!(!s.discard_dead("a").unwrap()); // gone → no-op
        // Unknown id → no-op.
        assert!(!s.discard_dead("ghost").unwrap());
    }

    #[test]
    fn recover_inflight_only_touches_inflight() {
        let s = Store::open("").unwrap();
        let a = s.enqueue(&op("a")).unwrap();
        let b = s.enqueue(&op("b")).unwrap();
        s.mark_inflight(a).unwrap(); // a inflight, b pending
        assert_eq!(s.recover_inflight().unwrap(), 1);
        assert_eq!(s.status_of_seq(a).unwrap().as_deref(), Some("pending"));
        assert_eq!(s.status_of_seq(b).unwrap().as_deref(), Some("pending"));
        // Nothing inflight now → no-op.
        assert_eq!(s.recover_inflight().unwrap(), 0);
    }

    #[test]
    fn wipe_outbox_clears_everything() {
        let s = Store::open("").unwrap();
        let a = s.enqueue(&op("a")).unwrap();
        s.enqueue(&op("b")).unwrap();
        s.mark_acked(a, Some("srv")).unwrap(); // mix of acked + pending
        s.wipe_outbox().unwrap();
        assert_eq!(s.pending_count().unwrap(), 0);
        assert_eq!(s.dead_count().unwrap(), 0);
        assert!(s.list_active().unwrap().is_empty());
        // The id is free again (the unique row is gone).
        let re = s.enqueue(&op("a")).unwrap();
        assert_eq!(s.status_of_seq(re).unwrap().as_deref(), Some("pending"));
    }

    #[test]
    fn enqueue_preserves_all_orchestration_fields() {
        let s = Store::open("").unwrap();
        let full = NewOutboxOp {
            id: "f1".into(),
            op_type: "create_order".into(),
            idempotency_key: "idem-f1".into(),
            payload: r#"{"total":1}"#.into(),
            event_at: "2026-06-19T10:00:00Z".into(),
            depends_on_seq: Some(7),
            user_id: Some("alice".into()),
            clock_offset_ms: Some(-250),
            shift_id: Some("shift-7".into()),
        };
        s.enqueue(&full).unwrap();
        let row = s.pending().unwrap().into_iter().find(|i| i.id == "f1").unwrap();
        assert_eq!(row.op_type, "create_order");
        assert_eq!(row.idempotency_key, "idem-f1");
        assert_eq!(row.payload, r#"{"total":1}"#);
        assert_eq!(row.event_at, "2026-06-19T10:00:00Z");
        assert_eq!(row.depends_on_seq, Some(7));
        assert_eq!(row.user_id.as_deref(), Some("alice"));
        assert_eq!(row.clock_offset_ms, Some(-250));
        assert_eq!(row.shift_id.as_deref(), Some("shift-7"));
        assert_eq!(row.attempts, 0);
        assert_eq!(row.next_attempt_at, 0);
        assert_eq!(row.last_error, None);
        assert_eq!(row.server_id, None);
    }

    #[test]
    fn enqueue_idempotent_keeps_original_fields() {
        let s = Store::open("").unwrap();
        let seq1 = s.enqueue(&NewOutboxOp { user_id: Some("alice".into()), ..op("dup") }).unwrap();
        // Re-enqueue same id with DIFFERENT fields → ignored, original kept.
        let seq2 = s.enqueue(&NewOutboxOp { user_id: Some("bob".into()), ..op("dup") }).unwrap();
        assert_eq!(seq1, seq2);
        let row = s.pending().unwrap().into_iter().find(|i| i.id == "dup").unwrap();
        assert_eq!(row.user_id.as_deref(), Some("alice"), "first enqueue wins (DO NOTHING)");
        assert_eq!(s.pending_count().unwrap(), 1);
    }

    /// The durable outbox must survive a process restart — a queued order rung up
    /// offline cannot evaporate when the app is killed and reopened.
    #[test]
    fn outbox_survives_reopen() {
        let path = std::env::temp_dir().join(format!("madar_outbox_{}.db", std::process::id()));
        let p = path.to_str().unwrap();
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{p}{ext}"));
        }
        {
            let s = Store::open(p).unwrap();
            s.enqueue(&op("a")).unwrap();
            s.enqueue(&op("b")).unwrap();
            assert_eq!(s.pending_count().unwrap(), 2);
        } // dropped → connection closed
        let s2 = Store::open(p).unwrap();
        assert_eq!(s2.pending_count().unwrap(), 2, "queue must persist across reopen");
        assert_eq!(
            s2.pending().unwrap().iter().map(|i| i.id.clone()).collect::<Vec<_>>(),
            vec!["a", "b"],
            "FIFO order must persist too"
        );
        drop(s2);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{p}{ext}"));
        }
    }

    /// `due_for_sync` only surfaces ops past their backoff gate, scoped to the
    /// teller whose token will send them (a 503 backoff must hide an op until its
    /// gate; a different teller's ops must not ride the current holder's drain).
    #[test]
    fn due_for_sync_respects_backoff_and_user() {
        let s = Store::open("").unwrap();
        let mut a = op("a");
        a.user_id = Some("alice".into());
        let mut b = op("b");
        b.user_id = Some("bob".into());
        let sa = s.enqueue(&a).unwrap();
        s.enqueue(&b).unwrap();

        // Both ready now (next_attempt_at defaults to 0); alice-scoped → only a.
        let due_alice: Vec<String> =
            s.due_for_sync(1, Some("alice")).unwrap().iter().map(|i| i.id.clone()).collect();
        assert_eq!(due_alice, vec!["a"]);

        // Back a off into the future → not due before its gate, due after.
        s.mark_retry(sa, "503", 10_000).unwrap();
        assert!(s.due_for_sync(5_000, Some("alice")).unwrap().is_empty(), "backed-off op leaks");
        assert_eq!(s.due_for_sync(20_000, Some("alice")).unwrap().len(), 1);
        // An unscoped drain sees both tellers' due ops.
        assert_eq!(s.due_for_sync(20_000, None).unwrap().len(), 2);
    }

    /// A crash mid-send leaves ops `inflight`; recovery must return them to
    /// `pending` so the next drain re-sends (the server dedups on idempotency_key).
    #[test]
    fn recover_inflight_returns_ops_to_pending() {
        let s = Store::open("").unwrap();
        let a = s.enqueue(&op("a")).unwrap();
        s.mark_inflight(a).unwrap();
        assert_eq!(s.pending_count().unwrap(), 1, "inflight still counts as un-synced work");
        assert_eq!(s.recover_inflight().unwrap(), 1);
        assert_eq!(s.pending().unwrap()[0].status, "pending");
    }

    /// An order stranded by a DEAD open_shift is surfaced (blocked count) and HEALS
    /// when its ops are re-pointed onto a live shift and the dead open is revived —
    /// the auto-heal / recover_orphaned_orders path. No sale is ever lost.
    #[test]
    fn dead_open_shift_blocks_order_then_heals_on_remap() {
        let s = Store::open("").unwrap();
        const A: &str = "00000000-0000-0000-0000-0000000000aa"; // failed offline shift
        const B: &str = "00000000-0000-0000-0000-0000000000bb"; // the teller's new shift
        const T: &str = "00000000-0000-0000-0000-0000000000a1"; // the teller

        // Teller opened A offline, rang an order on it, then A's open DIED.
        let mut open = op("open-A");
        open.op_type = "open_shift".into();
        open.shift_id = Some(A.into());
        open.user_id = Some(T.into());
        let open_seq = s.enqueue(&open).unwrap();
        let mut order = op("order-1");
        order.op_type = "create_order".into();
        order.shift_id = Some(A.into());
        order.depends_on_seq = Some(open_seq);
        s.enqueue(&order).unwrap();
        s.mark_dead(open_seq, "open rejected").unwrap();

        // The order is now BLOCKED by the dead open (surfaced to the sync center),
        // and the dead-open lookup is teller-scoped.
        assert_eq!(s.count_orders_blocked_by_dead_dep().unwrap(), 1);
        assert_eq!(s.dead_open_shift_ids(T).unwrap(), vec![A.to_string()]);
        assert!(s.dead_open_shift_ids("00000000-0000-0000-0000-0000000000a2").unwrap().is_empty());

        // Heal: re-point A's ops onto the live shift B and revive the dead open.
        assert!(s.remap_shift(A, B).unwrap() >= 2, "open + order re-pointed");
        s.requeue_dead_for_shift(B).unwrap();

        // No longer blocked — the order rides B, whose open is pending again.
        assert_eq!(s.count_orders_blocked_by_dead_dep().unwrap(), 0);
        assert!(s.dead_open_shift_ids(T).unwrap().is_empty());
    }

    // ── Model-based stateful testing of the durable outbox ──────────────────────
    // The offline sync engine's correctness lives here: as queued work moves
    // pending→inflight→{acked,dead}→pending, nothing may be lost, double-counted,
    // or reordered. We drive RANDOM transition sequences against the real store and
    // an independent reference model, asserting counts, FIFO and conservation after
    // EVERY step — coverage no fixed example sequence can match.
    mod outbox_model {
        use super::*;
        use proptest::prelude::*;
        use std::collections::BTreeMap;

        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum St {
            Pending,
            Inflight,
            Dead,
        }

        #[derive(Clone, Debug)]
        enum Cmd {
            Enqueue,
            Inflight(usize),
            Ack(usize),
            Dead(usize),
            Retry(usize),
            RequeueDead,
            DiscardDead(usize),
            RecoverInflight,
        }

        fn arb_cmd() -> impl Strategy<Value = Cmd> {
            prop_oneof![
                3 => Just(Cmd::Enqueue),
                2 => (0usize..100).prop_map(Cmd::Ack),
                1 => (0usize..100).prop_map(Cmd::Inflight),
                1 => (0usize..100).prop_map(Cmd::Dead),
                1 => (0usize..100).prop_map(Cmd::Retry),
                1 => Just(Cmd::RequeueDead),
                1 => (0usize..100).prop_map(Cmd::DiscardDead),
                1 => Just(Cmd::RecoverInflight),
            ]
        }

        // The k-th seq (mod count) whose state is in `want`.
        fn pick(model: &BTreeMap<i64, (String, St)>, k: usize, want: &[St]) -> Option<i64> {
            let v: Vec<i64> = model
                .iter()
                .filter(|(_, (_, st))| want.contains(st))
                .map(|(&seq, _)| seq)
                .collect();
            (!v.is_empty()).then(|| v[k % v.len()])
        }

        proptest! {
            #[test]
            fn outbox_invariants(cmds in prop::collection::vec(arb_cmd(), 0..80)) {
                let s = Store::open("").unwrap();
                let mut model: BTreeMap<i64, (String, St)> = BTreeMap::new();
                let mut n = 0u32;

                for cmd in cmds {
                    match cmd {
                        Cmd::Enqueue => {
                            let id = format!("op{n}");
                            n += 1;
                            let seq = s.enqueue(&op(&id)).unwrap();
                            model.insert(seq, (id, St::Pending));
                        }
                        Cmd::Inflight(k) => {
                            if let Some(seq) = pick(&model, k, &[St::Pending]) {
                                s.mark_inflight(seq).unwrap();
                                model.get_mut(&seq).unwrap().1 = St::Inflight;
                            }
                        }
                        Cmd::Ack(k) => {
                            if let Some(seq) = pick(&model, k, &[St::Pending, St::Inflight]) {
                                s.mark_acked(seq, Some("srv")).unwrap();
                                model.remove(&seq);
                            }
                        }
                        Cmd::Dead(k) => {
                            if let Some(seq) = pick(&model, k, &[St::Pending, St::Inflight]) {
                                s.mark_dead(seq, "err").unwrap();
                                model.get_mut(&seq).unwrap().1 = St::Dead;
                            }
                        }
                        Cmd::Retry(k) => {
                            if let Some(seq) = pick(&model, k, &[St::Pending, St::Inflight]) {
                                s.mark_retry(seq, "err", 0).unwrap();
                                model.get_mut(&seq).unwrap().1 = St::Pending;
                            }
                        }
                        Cmd::RequeueDead => {
                            s.requeue_dead().unwrap();
                            for (_, st) in model.values_mut() {
                                if *st == St::Dead {
                                    *st = St::Pending;
                                }
                            }
                        }
                        Cmd::DiscardDead(k) => {
                            if let Some(seq) = pick(&model, k, &[St::Dead]) {
                                let id = model[&seq].0.clone();
                                prop_assert!(s.discard_dead(&id).unwrap(), "dead op should discard");
                                model.remove(&seq);
                            }
                        }
                        Cmd::RecoverInflight => {
                            s.recover_inflight().unwrap();
                            for (_, st) in model.values_mut() {
                                if *st == St::Inflight {
                                    *st = St::Pending;
                                }
                            }
                        }
                    }

                    // ── invariants after EVERY command ──
                    let exp_pending = model
                        .values()
                        .filter(|(_, st)| *st == St::Pending || *st == St::Inflight)
                        .count();
                    let exp_dead = model.values().filter(|(_, st)| *st == St::Dead).count();
                    prop_assert_eq!(s.pending_count().unwrap() as usize, exp_pending, "pending count");
                    prop_assert_eq!(s.dead_count().unwrap() as usize, exp_dead, "dead count");
                    prop_assert_eq!(s.list_active().unwrap().len(), exp_pending + exp_dead, "active count");

                    // FIFO: pending() seqs strictly ascending.
                    let pseqs: Vec<i64> = s.pending().unwrap().iter().map(|i| i.seq).collect();
                    prop_assert!(pseqs.windows(2).all(|w| w[0] < w[1]), "pending not FIFO");

                    // Conservation: the store's active seq-set equals the model's.
                    let mut store_seqs: Vec<i64> =
                        s.list_active().unwrap().iter().map(|i| i.seq).collect();
                    store_seqs.sort_unstable();
                    let mut model_seqs: Vec<i64> = model.keys().copied().collect();
                    model_seqs.sort_unstable();
                    prop_assert_eq!(store_seqs, model_seqs, "store/model seq-sets diverged");
                }
            }
        }
    }
}
