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
  op_type         TEXT NOT NULL,                      -- submit_order | void_order | open_shift | ...
  idempotency_key TEXT NOT NULL,                      -- sent as X-Idempotency-Key on replay
  payload         TEXT NOT NULL,                      -- canonical request JSON
  event_at        TEXT NOT NULL,                      -- client real-event time (RFC3339)
  enqueued_at     TEXT NOT NULL,
  status          TEXT NOT NULL DEFAULT 'pending',    -- pending|inflight|acked|dead|superseded
  attempts        INTEGER NOT NULL DEFAULT 0,
  last_error      TEXT,
  server_id       TEXT,                               -- set on ack
  depends_on_seq  INTEGER                             -- gate dependents (e.g. void after its order)
);
CREATE INDEX IF NOT EXISTS outbox_status_seq ON outbox(status, seq);
"#;

/// An op to enqueue. `id` is the client uuid (re-enqueue with the same `id` is a
/// no-op, so retries/replays don't duplicate).
#[derive(Debug, Clone)]
pub struct NewOutboxOp {
    pub id: String,
    pub op_type: String,
    pub idempotency_key: String,
    pub payload: String,
    pub event_at: String,
    pub depends_on_seq: Option<i64>,
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
    /// and returns the existing `seq`.
    pub fn enqueue(&self, op: &NewOutboxOp) -> CoreResult<i64> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO outbox(id, op_type, idempotency_key, payload, event_at, enqueued_at, depends_on_seq)
             VALUES(?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(id) DO NOTHING",
            params![op.id, op.op_type, op.idempotency_key, op.payload, op.event_at, now_iso(), op.depends_on_seq],
        )?;
        Ok(conn.query_row("SELECT seq FROM outbox WHERE id=?1", [&op.id], |r| r.get(0))?)
    }

    /// Drainable items in FIFO order (pending + inflight).
    pub fn pending(&self) -> CoreResult<Vec<OutboxItem>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT seq,id,op_type,idempotency_key,payload,event_at,status,attempts,last_error,server_id,depends_on_seq
             FROM outbox WHERE status IN ('pending','inflight') ORDER BY seq ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(OutboxItem {
                    seq: r.get(0)?, id: r.get(1)?, op_type: r.get(2)?, idempotency_key: r.get(3)?,
                    payload: r.get(4)?, event_at: r.get(5)?, status: r.get(6)?, attempts: r.get(7)?,
                    last_error: r.get(8)?, server_id: r.get(9)?, depends_on_seq: r.get(10)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
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
    /// sync-center read. Acked + superseded rows are hidden (nothing to act on).
    pub fn list_active(&self) -> CoreResult<Vec<OutboxItem>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT seq,id,op_type,idempotency_key,payload,event_at,status,attempts,last_error,server_id,depends_on_seq
             FROM outbox WHERE status IN ('pending','inflight','dead') ORDER BY seq ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(OutboxItem {
                    seq: r.get(0)?, id: r.get(1)?, op_type: r.get(2)?, idempotency_key: r.get(3)?,
                    payload: r.get(4)?, event_at: r.get(5)?, status: r.get(6)?, attempts: r.get(7)?,
                    last_error: r.get(8)?, server_id: r.get(9)?, depends_on_seq: r.get(10)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Reset every dead command back to `pending` (clearing its error) so the
    /// next drain retries it. Returns how many were requeued.
    pub fn requeue_dead(&self) -> CoreResult<u32> {
        let n = self.lock().execute(
            "UPDATE outbox SET status='pending', last_error=NULL WHERE status='dead'", [])?;
        Ok(n as u32)
    }

    /// Discard a single DEAD command by client id (the teller gives up on it).
    /// Only dead rows can be discarded — a pending/inflight op might still land.
    pub fn discard_dead(&self, id: &str) -> CoreResult<bool> {
        let n = self.lock().execute(
            "DELETE FROM outbox WHERE id=?1 AND status='dead'", params![id])?;
        Ok(n > 0)
    }

    pub fn mark_acked(&self, seq: i64, server_id: Option<&str>) -> CoreResult<()> {
        self.lock().execute(
            "UPDATE outbox SET status='acked', server_id=?2 WHERE seq=?1",
            params![seq, server_id])?;
        Ok(())
    }

    pub fn mark_dead(&self, seq: i64, error: &str) -> CoreResult<()> {
        self.lock().execute(
            "UPDATE outbox SET status='dead', last_error=?2, attempts=attempts+1 WHERE seq=?1",
            params![seq, error])?;
        Ok(())
    }

    /// Drop every queued command. Only for an explicit destructive sign-out —
    /// offline shifts are real sales, so the default logout preserves them.
    pub fn wipe_outbox(&self) -> CoreResult<()> {
        self.lock().execute("DELETE FROM outbox", [])?;
        Ok(())
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(id: &str) -> NewOutboxOp {
        NewOutboxOp {
            id: id.into(),
            op_type: "submit_order".into(),
            idempotency_key: id.into(),
            payload: r#"{"total":2280}"#.into(),
            event_at: "2026-06-19T10:00:00Z".into(),
            depends_on_seq: None,
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
}
