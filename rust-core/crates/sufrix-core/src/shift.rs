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
/// kv key holding the suggested opening cash for the NEXT shift — the previous
/// shift's declared closing (cash continuity). Cached from the server prefill
/// when online and from a local close when offline, so the open-shift screen can
/// prefill it either way.
pub(crate) const SUGGESTED_OPEN_CASH_KEY: &str = "shift:suggested_open_cash";

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

/// Outbox payload for an offline cash movement — carries the path `shift_id`.
/// Idempotent on the request's `client_ref` (the backend dedups replays).
#[derive(Serialize, Deserialize)]
pub(crate) struct CashMovementCommand {
    pub shift_id: String,
    pub request: models::CashMovementRequest,
}

/// A cash-drawer movement (pay-in / pay-out). `amount_minor` is signed:
/// positive = cash in, negative = cash out.
#[derive(uniffi::Record, Clone, Debug)]
pub struct CashMovementView {
    pub id: String,
    pub amount_minor: i64,
    pub note: String,
    pub moved_by_name: String,
    pub created_at: String,
}

pub(crate) fn cash_movement_view(m: &models::CashMovement) -> CashMovementView {
    CashMovementView {
        id: m.id.to_string(),
        amount_minor: m.amount as i64,
        note: m.note.clone(),
        moved_by_name: m.moved_by_name.clone(),
        created_at: m.created_at.to_rfc3339(),
    }
}

/// A past shift, projected for the history list.
#[derive(uniffi::Record, Clone, Debug)]
pub struct ShiftSummaryView {
    pub id: String,
    pub branch_name: Option<String>,
    pub opened_at: String,
    pub closed_at: Option<String>,
    pub opening_cash_minor: i64,
    pub closing_declared_minor: Option<i64>,
    pub closing_system_minor: Option<i64>,
    pub discrepancy_minor: Option<i64>,
    pub status: String,
    pub is_open: bool,
}

pub(crate) fn shift_summary_view(s: &models::Shift) -> ShiftSummaryView {
    ShiftSummaryView {
        id: s.id.to_string(),
        branch_name: s.branch_name.clone().flatten(),
        opened_at: s.opened_at.to_rfc3339(),
        closed_at: s.closed_at.clone().flatten().map(|d| d.to_rfc3339()),
        opening_cash_minor: s.opening_cash as i64,
        closing_declared_minor: s.closing_cash_declared.flatten().map(|v| v as i64),
        closing_system_minor: s.closing_cash_system.flatten().map(|v| v as i64),
        discrepancy_minor: s.cash_discrepancy.flatten().map(|v| v as i64),
        status: s.status.clone(),
        is_open: s.status == "open",
    }
}

/// One payment-method line in the shift report.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct ShiftReportPaymentLine {
    pub method: String,
    pub is_cash: bool,
    pub order_count: i64,
    pub total_minor: i64,
}

/// The shift report shown on close (drives the system-cash + discrepancy) and in
/// a report preview. `expected_cash_minor` is the server's expected drawer cash
/// PLUS still-queued cash sales (offline: opening cash + queued cash).
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct ShiftReportView {
    pub expected_cash_minor: i64,
    pub opening_cash_minor: i64,
    pub total_payments_minor: i64,
    pub net_payments_minor: i64,
    pub voided_amount_minor: i64,
    pub cash_movements_net_minor: i64,
    pub payment_lines: Vec<ShiftReportPaymentLine>,
    /// `false` = offline fallback (no server figures, just opening + queued).
    pub from_server: bool,
}

/// Project the server report, adding still-queued cash sales to expected cash.
pub(crate) fn report_view(report: &models::ShiftReportResponse, queued_cash: i64) -> ShiftReportView {
    ShiftReportView {
        expected_cash_minor: report.expected_cash + queued_cash,
        opening_cash_minor: report.shift.opening_cash as i64,
        total_payments_minor: report.total_payments,
        net_payments_minor: report.net_payments,
        voided_amount_minor: report.voided_amount,
        cash_movements_net_minor: report.cash_movements_net,
        payment_lines: report
            .payment_summary
            .iter()
            .map(|p| ShiftReportPaymentLine {
                method: p.payment_method.clone(),
                is_cash: p.is_cash,
                order_count: p.order_count,
                total_minor: p.total,
            })
            .collect(),
        from_server: true,
    }
}

/// Offline fallback: expected = opening cash + still-queued cash sales.
pub(crate) fn offline_report_view(opening_cash_minor: i64, queued_cash: i64) -> ShiftReportView {
    ShiftReportView {
        expected_cash_minor: opening_cash_minor + queued_cash,
        opening_cash_minor,
        total_payments_minor: 0,
        net_payments_minor: 0,
        voided_amount_minor: 0,
        cash_movements_net_minor: 0,
        payment_lines: vec![],
        from_server: false,
    }
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

/// Cache the suggested opening cash (previous declared closing) for the next
/// shift. A non-positive value clears it (no carryover to suggest).
pub(crate) fn cache_suggested_opening_cash(store: &Store, minor: i64) -> CoreResult<()> {
    store.kv_put(SUGGESTED_OPEN_CASH_KEY, &minor.max(0).to_string())
}

/// The suggested opening cash for the next shift (0 when none is known).
pub(crate) fn suggested_opening_cash(store: &Store) -> CoreResult<i64> {
    Ok(store
        .kv_get(SUGGESTED_OPEN_CASH_KEY)?
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0))
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

    #[test]
    fn suggested_opening_cash_roundtrips_and_clamps() {
        let store = Store::open("").unwrap();
        assert_eq!(suggested_opening_cash(&store).unwrap(), 0); // none known yet
        cache_suggested_opening_cash(&store, 48000).unwrap();
        assert_eq!(suggested_opening_cash(&store).unwrap(), 48000);
        cache_suggested_opening_cash(&store, -5).unwrap(); // non-positive clears
        assert_eq!(suggested_opening_cash(&store).unwrap(), 0);
    }

    #[test]
    fn offline_report_view_is_opening_plus_queued() {
        let v = offline_report_view(50000, 2280);
        assert_eq!(v.expected_cash_minor, 52280);
        assert_eq!(v.opening_cash_minor, 50000);
        assert!(!v.from_server);
        assert!(v.payment_lines.is_empty());
    }

    #[test]
    fn report_view_adds_queued_cash_to_server_expected() {
        let mut report = models::ShiftReportResponse::default();
        report.expected_cash = 60000;
        report.shift = Box::new(models::Shift { opening_cash: 50000, ..Default::default() });
        report.total_payments = 15000;
        report.payment_summary = vec![models::PaymentSummaryRow::new(true, 3, "Cash".into(), 12000)];
        let v = report_view(&report, 2280);
        assert_eq!(v.expected_cash_minor, 62280); // 60000 + 2280 queued
        assert_eq!(v.opening_cash_minor, 50000);
        assert_eq!(v.total_payments_minor, 15000);
        assert_eq!(v.payment_lines.len(), 1);
        assert_eq!(v.payment_lines[0].total_minor, 12000);
        assert!(v.from_server);
    }
}
