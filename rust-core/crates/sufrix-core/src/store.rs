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

    /// True while any order/void/cash for `shift_id` is still pending or inflight
    /// (excluding `exclude_seq`, the close itself). A shift close must be the
    /// LAST thing that syncs for its shift — shift-scoped so a later shift's
    /// orders never block an earlier shift's close.
    pub fn has_live_shift_writes(&self, shift_id: &str, exclude_seq: i64) -> CoreResult<bool> {
        let n: i64 = self.lock().query_row(
            "SELECT COUNT(*) FROM outbox \
             WHERE status IN ('pending','inflight') AND shift_id=?1 AND seq<>?2 \
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
    fn open_upgrades_an_old_schema_db_without_erroring() {
        // Reproduces the startup crash: an app updated in place has a DB whose
        // `outbox` predates the offline-orchestration columns. Opening it MUST
        // migrate (not fail on the backoff index that references a new column).
        let path = std::env::temp_dir().join("sufrix_old_schema_upgrade_test.sqlite");
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
}
