//! Shift lifecycle (PLAN §7.4). Opening a shift is the first OUTBOX WRITE: it
//! writes an optimistic local shift + queues an idempotent `open_shift` command
//! (client UUID = the shift PK, so replay is safe), then drains if online. The
//! UI reads `current` regardless of connectivity.

use serde::{Deserialize, Serialize};
use sufrix_api::models;

use crate::error::CoreResult;
use crate::store::Store;

/// kv key holding the device's current shift (canonical `Shift` JSON).
pub(crate) const CURRENT_SHIFT_KEY: &str = "current_shift";

#[derive(uniffi::Record, Clone, Debug)]
pub struct ShiftView {
    pub id: String,
    pub branch_id: String,
    pub teller_id: String,
    pub teller_name: String,
    pub opening_cash_minor: i64,
    pub opened_at: String,
    pub status: String,
    pub is_open: bool,
}

/// Outbox payload for an open-shift command — carries the path `branch_id`
/// alongside the wire request.
#[derive(Serialize, Deserialize)]
pub(crate) struct OpenShiftCommand {
    pub branch_id: String,
    pub request: models::OpenShiftRequest,
}

/// Outbox payload for a close-shift command — carries the path `shift_id`.
#[derive(Serialize, Deserialize)]
pub(crate) struct CloseShiftCommand {
    pub shift_id: String,
    pub request: models::CloseShiftRequest,
}

pub(crate) fn view_from(shift: &models::Shift) -> ShiftView {
    ShiftView {
        id: shift.id.to_string(),
        branch_id: shift.branch_id.to_string(),
        teller_id: shift.teller_id.to_string(),
        teller_name: shift.teller_name.clone(),
        opening_cash_minor: shift.opening_cash as i64,
        opened_at: shift.opened_at.to_rfc3339(),
        status: shift.status.clone(),
        is_open: shift.status == "open",
    }
}

pub(crate) fn current(store: &Store) -> CoreResult<Option<ShiftView>> {
    match store.kv_get(CURRENT_SHIFT_KEY)? {
        Some(json) if json != "null" => {
            let shift: models::Shift = serde_json::from_str(&json)?;
            Ok(Some(view_from(&shift)))
        }
        _ => Ok(None),
    }
}

pub(crate) fn save(store: &Store, shift: &models::Shift) -> CoreResult<()> {
    store.kv_put(CURRENT_SHIFT_KEY, &serde_json::to_string(shift)?)
}

/// Drop the cached shift (closed/none on the server, or on sign-out). `current`
/// reads "null" back as `None`.
pub(crate) fn clear(store: &Store) -> CoreResult<()> {
    store.kv_put(CURRENT_SHIFT_KEY, "null")
}

/// Mark the cached shift closed optimistically (status → "closed") so routing
/// flips to open-shift the instant the teller closes; the close command syncs
/// via the outbox. No-op if there's no cached shift.
pub(crate) fn close_local(store: &Store) -> CoreResult<()> {
    match store.kv_get(CURRENT_SHIFT_KEY)? {
        Some(json) if json != "null" => {
            let mut shift: models::Shift = serde_json::from_str(&json)?;
            shift.status = "closed".into();
            save(store, &shift)
        }
        _ => Ok(()),
    }
}

/// What to do with the local shift after the server's prefill comes back.
#[derive(Debug)]
pub(crate) enum ShiftReconcile {
    /// The server has an open shift — adopt it as the local truth.
    Adopt(Box<models::Shift>),
    /// The server reports none, but our `open_shift` command is still queued, so
    /// the server simply hasn't seen it yet — keep the optimistic local shift.
    KeepLocal,
    /// The server authoritatively has no open shift (and nothing is pending) —
    /// clear the local cache (e.g. a dashboard force-close).
    Clear,
}

/// Decide how to reconcile the local shift with the server's prefill. PURE so
/// the shift "bounce" bugs stay covered by tests:
/// - the server's "no open shift" is authoritative only once our own open_shift
///   command has reached it — until then (`open_pending`) the optimistic shift
///   stands (forward bounce);
/// - the server's "still open" is stale while our close_shift command is queued
///   (`close_pending`) — keep the locally-closed shift so routing stays on
///   open-shift (reverse bounce).
pub(crate) fn reconcile(
    prefill: &models::ShiftPreFill,
    open_pending: bool,
    close_pending: bool,
) -> ShiftReconcile {
    if prefill.has_open_shift {
        if let Some(Some(server_shift)) = &prefill.open_shift {
            if close_pending {
                return ShiftReconcile::KeepLocal;
            }
            return ShiftReconcile::Adopt(server_shift.clone());
        }
    }
    if open_pending {
        ShiftReconcile::KeepLocal
    } else {
        ShiftReconcile::Clear
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_roundtrips_and_handles_empty() {
        let store = Store::open("").unwrap();
        assert!(current(&store).unwrap().is_none());
        let json = r#"{
          "branch_id":"00000000-0000-0000-0000-0000000000b1",
          "id":"00000000-0000-0000-0000-0000000000a1",
          "opened_at":"2026-06-20T09:00:00Z",
          "opening_cash":50000,
          "opening_cash_was_edited":false,
          "status":"open",
          "teller_id":"00000000-0000-0000-0000-0000000000c1",
          "teller_name":"Sara"
        }"#;
        store.kv_put(CURRENT_SHIFT_KEY, json).unwrap();
        let v = current(&store).unwrap().unwrap();
        assert_eq!(v.teller_name, "Sara");
        assert_eq!(v.opening_cash_minor, 50000);
        assert!(v.is_open);
    }

    fn open_shift_model() -> models::Shift {
        models::Shift { status: "open".into(), ..Default::default() }
    }

    #[test]
    fn reconcile_adopts_server_open_shift() {
        let mut pf = models::ShiftPreFill::new(true, 0);
        pf.open_shift = Some(Some(Box::new(open_shift_model())));
        // The server's open shift wins regardless of any pending open command.
        assert!(matches!(reconcile(&pf, false, false), ShiftReconcile::Adopt(_)));
        assert!(matches!(reconcile(&pf, true, false), ShiftReconcile::Adopt(_)));
    }

    #[test]
    fn reconcile_keeps_local_while_open_command_pending() {
        // Server says no open shift, but our open_shift is still queued: the
        // server just hasn't seen it yet → keep the optimistic local shift.
        // (The forward open-shift "bounce" regression.)
        let pf = models::ShiftPreFill::new(false, 0);
        assert!(matches!(reconcile(&pf, true, false), ShiftReconcile::KeepLocal));
    }

    #[test]
    fn reconcile_clears_when_server_authoritatively_has_none() {
        // Server says none AND nothing is pending → a real force-close: clear.
        let pf = models::ShiftPreFill::new(false, 0);
        assert!(matches!(reconcile(&pf, false, false), ShiftReconcile::Clear));
    }

    #[test]
    fn reconcile_handles_flag_set_but_payload_missing() {
        // has_open_shift true but no payload: treat like "none" — keep local
        // while pending, clear only when authoritative.
        let pf = models::ShiftPreFill::new(true, 0);
        assert!(matches!(reconcile(&pf, true, false), ShiftReconcile::KeepLocal));
        assert!(matches!(reconcile(&pf, false, false), ShiftReconcile::Clear));
    }

    #[test]
    fn reconcile_keeps_local_when_close_is_pending_despite_server_open() {
        // The REVERSE bounce: we closed locally, the close is queued, but the
        // server still reports the shift open. Don't re-adopt it — keep the
        // locally-closed shift so routing stays on open-shift.
        let mut pf = models::ShiftPreFill::new(true, 0);
        pf.open_shift = Some(Some(Box::new(open_shift_model())));
        assert!(matches!(reconcile(&pf, false, true), ShiftReconcile::KeepLocal));
    }

    #[test]
    fn reconcile_adopts_again_once_close_has_synced() {
        // Close acked (no longer pending) but the server somehow still open →
        // adopt the server truth (e.g. the close was rejected server-side).
        let mut pf = models::ShiftPreFill::new(true, 0);
        pf.open_shift = Some(Some(Box::new(open_shift_model())));
        assert!(matches!(reconcile(&pf, false, false), ShiftReconcile::Adopt(_)));
    }
}
