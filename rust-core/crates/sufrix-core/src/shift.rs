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
    /// Pay-in / pay-out drawer totals (separate, not just the net) — Z-report depth.
    pub cash_in_minor: i64,
    pub cash_out_minor: i64,
    pub payment_lines: Vec<ShiftReportPaymentLine>,
    /// Each individual cash movement (newest-first), for the itemised drawer block.
    pub cash_movements: Vec<ShiftReportCashLine>,
    /// `false` = offline fallback (no server figures, just opening + queued).
    pub from_server: bool,
}

/// One itemised cash-drawer movement on the report. `amount_minor` is signed
/// (positive = pay-in, negative = pay-out).
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct ShiftReportCashLine {
    pub amount_minor: i64,
    pub note: String,
    pub moved_by_name: String,
    pub created_at: String,
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
        cash_in_minor: report.cash_movements_in,
        cash_out_minor: report.cash_movements_out,
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
        cash_movements: report
            .cash_movements
            .iter()
            .map(|m| ShiftReportCashLine {
                amount_minor: m.amount as i64,
                note: m.note.clone(),
                moved_by_name: m.moved_by_name.clone(),
                created_at: m.created_at.to_rfc3339(),
            })
            .collect(),
        from_server: true,
    }
}

/// Offline fallback: expected = opening cash + still-queued cash sales; the
/// drawer block is reconstructed from the still-queued cash movements.
pub(crate) fn offline_report_view(
    opening_cash_minor: i64,
    queued_cash: i64,
    movements: Vec<ShiftReportCashLine>,
) -> ShiftReportView {
    let cash_in: i64 = movements.iter().filter(|m| m.amount_minor > 0).map(|m| m.amount_minor).sum();
    let cash_out: i64 = movements.iter().filter(|m| m.amount_minor < 0).map(|m| -m.amount_minor).sum();
    ShiftReportView {
        expected_cash_minor: opening_cash_minor + queued_cash,
        opening_cash_minor,
        total_payments_minor: 0,
        net_payments_minor: 0,
        voided_amount_minor: 0,
        cash_movements_net_minor: cash_in - cash_out,
        cash_in_minor: cash_in,
        cash_out_minor: cash_out,
        payment_lines: vec![],
        cash_movements: movements,
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
        let moves = vec![
            ShiftReportCashLine { amount_minor: 5000, note: "float".into(), moved_by_name: "Mona".into(), created_at: "t".into() },
            ShiftReportCashLine { amount_minor: -1500, note: "supplier".into(), moved_by_name: "Mona".into(), created_at: "t".into() },
        ];
        let v = offline_report_view(50000, 2280, moves);
        assert_eq!(v.expected_cash_minor, 52280);
        assert_eq!(v.opening_cash_minor, 50000);
        assert!(!v.from_server);
        assert!(v.payment_lines.is_empty());
        // Pay-in / pay-out split derived from the queued movements.
        assert_eq!(v.cash_in_minor, 5000);
        assert_eq!(v.cash_out_minor, 1500);
        assert_eq!(v.cash_movements_net_minor, 3500);
        assert_eq!(v.cash_movements.len(), 2);
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

    // ── report_view: full field projection + movements + ordering ─────────────

    #[test]
    fn report_view_projects_every_field_and_preserves_movement_order() {
        let mut report = models::ShiftReportResponse::default();
        report.expected_cash = 30000;
        report.shift = Box::new(models::Shift { opening_cash: 20000, ..Default::default() });
        report.total_payments = 9000;
        report.net_payments = 8500; // distinct from total (a void)
        report.voided_amount = 500;
        report.cash_movements_net = 1200;
        report.cash_movements_in = 3000;
        report.cash_movements_out = 1800;
        report.payment_summary = vec![
            models::PaymentSummaryRow::new(true, 2, "Cash".into(), 5000),
            models::PaymentSummaryRow::new(false, 1, "Card".into(), 4000),
        ];
        report.cash_movements = vec![
            models::CashMovementSummaryRow { amount: 3000, note: "float".into(), moved_by_name: "Mona".into(), ..Default::default() },
            models::CashMovementSummaryRow { amount: -1800, note: "".into(), moved_by_name: "Ali".into(), ..Default::default() },
        ];
        let v = report_view(&report, 0);
        // Every server figure mapped through verbatim (queued = 0 here).
        assert_eq!(v.expected_cash_minor, 30000);
        assert_eq!(v.net_payments_minor, 8500);
        assert_eq!(v.voided_amount_minor, 500);
        assert_eq!(v.cash_movements_net_minor, 1200);
        assert_eq!(v.cash_in_minor, 3000);
        assert_eq!(v.cash_out_minor, 1800);
        // Payment lines keep order and per-row fields.
        assert_eq!(v.payment_lines.len(), 2);
        assert_eq!(v.payment_lines[0].method, "Cash");
        assert!(v.payment_lines[0].is_cash);
        assert_eq!(v.payment_lines[0].order_count, 2);
        assert_eq!(v.payment_lines[1].method, "Card");
        assert!(!v.payment_lines[1].is_cash);
        // Movement order preserved; note + signed amount mapped.
        assert_eq!(v.cash_movements.len(), 2);
        assert_eq!(v.cash_movements[0].amount_minor, 3000);
        assert_eq!(v.cash_movements[0].note, "float");
        assert_eq!(v.cash_movements[0].moved_by_name, "Mona");
        assert_eq!(v.cash_movements[1].amount_minor, -1800);
        assert_eq!(v.cash_movements[1].moved_by_name, "Ali");
    }

    #[test]
    fn report_view_default_response_is_all_zero_and_empty() {
        // A defaulted server response (no sales, no movements) projects cleanly.
        let report = models::ShiftReportResponse::default();
        let v = report_view(&report, 0);
        assert_eq!(v.expected_cash_minor, 0);
        assert_eq!(v.opening_cash_minor, 0);
        assert_eq!(v.total_payments_minor, 0);
        assert_eq!(v.net_payments_minor, 0);
        assert_eq!(v.voided_amount_minor, 0);
        assert_eq!(v.cash_in_minor, 0);
        assert_eq!(v.cash_out_minor, 0);
        assert!(v.payment_lines.is_empty());
        assert!(v.cash_movements.is_empty());
        assert!(v.from_server);
    }

    #[test]
    fn report_view_negative_queued_cash_lowers_expected() {
        // queued_cash is just added — a negative (net cash refund queued) lowers it.
        let mut report = models::ShiftReportResponse::default();
        report.expected_cash = 60000;
        let v = report_view(&report, -1500);
        assert_eq!(v.expected_cash_minor, 58500);
    }

    // ── offline_report_view: cash split / net / empties / boundaries ──────────

    #[test]
    fn offline_report_view_empty_movements_is_pure_opening_plus_queued() {
        let v = offline_report_view(50000, 2280, vec![]);
        assert_eq!(v.expected_cash_minor, 52280);
        assert_eq!(v.opening_cash_minor, 50000);
        assert_eq!(v.cash_in_minor, 0);
        assert_eq!(v.cash_out_minor, 0);
        assert_eq!(v.cash_movements_net_minor, 0);
        assert!(v.cash_movements.is_empty());
        assert!(v.payment_lines.is_empty());
        assert!(!v.from_server);
        // Sales figures are always zero in the offline fallback.
        assert_eq!(v.total_payments_minor, 0);
        assert_eq!(v.net_payments_minor, 0);
        assert_eq!(v.voided_amount_minor, 0);
    }

    #[test]
    fn offline_report_view_zero_amount_movement_counts_as_neither_in_nor_out() {
        // amount == 0 is excluded from both the >0 and <0 filters (boundary).
        let moves = vec![ShiftReportCashLine {
            amount_minor: 0,
            note: "noop".into(),
            moved_by_name: "Mona".into(),
            created_at: "t".into(),
        }];
        let v = offline_report_view(10000, 0, moves);
        assert_eq!(v.cash_in_minor, 0);
        assert_eq!(v.cash_out_minor, 0);
        assert_eq!(v.cash_movements_net_minor, 0);
        assert_eq!(v.cash_movements.len(), 1); // still itemised
    }

    #[test]
    fn offline_report_view_only_pay_outs_net_is_negative() {
        let moves = vec![
            ShiftReportCashLine { amount_minor: -2000, note: "supplier".into(), moved_by_name: "Ali".into(), created_at: "t".into() },
            ShiftReportCashLine { amount_minor: -500, note: "tips".into(), moved_by_name: "Ali".into(), created_at: "t".into() },
        ];
        let v = offline_report_view(30000, 0, moves);
        assert_eq!(v.cash_in_minor, 0);
        assert_eq!(v.cash_out_minor, 2500); // stored as a positive magnitude
        assert_eq!(v.cash_movements_net_minor, -2500);
    }

    #[test]
    fn offline_report_view_preserves_given_movement_order() {
        // The fallback itemises the movements exactly as handed in (newest-first
        // is the caller's responsibility) — no reordering.
        let moves = vec![
            ShiftReportCashLine { amount_minor: 100, note: "a".into(), moved_by_name: "X".into(), created_at: "3".into() },
            ShiftReportCashLine { amount_minor: 200, note: "b".into(), moved_by_name: "X".into(), created_at: "2".into() },
            ShiftReportCashLine { amount_minor: 300, note: "c".into(), moved_by_name: "X".into(), created_at: "1".into() },
        ];
        let v = offline_report_view(0, 0, moves);
        assert_eq!(v.cash_movements[0].note, "a");
        assert_eq!(v.cash_movements[1].note, "b");
        assert_eq!(v.cash_movements[2].note, "c");
        assert_eq!(v.cash_in_minor, 600);
    }

    // ── cash_movement_view ────────────────────────────────────────────────────

    #[test]
    fn cash_movement_view_maps_fields_and_widens_amount() {
        let m = models::CashMovement {
            amount: -1500, // i32 → i64
            note: "supplier".into(),
            moved_by_name: "Mona".into(),
            created_at: chrono::DateTime::parse_from_rfc3339("2026-06-20T09:30:00+02:00").unwrap(),
            ..Default::default()
        };
        let v = cash_movement_view(&m);
        assert_eq!(v.amount_minor, -1500_i64);
        assert_eq!(v.note, "supplier");
        assert_eq!(v.moved_by_name, "Mona");
        // created_at is rendered as an RFC3339 string in the source offset.
        assert!(v.created_at.starts_with("2026-06-20T09:30:00"));
        // id comes from the model's uuid (defaulted → all-zero uuid).
        assert_eq!(v.id, "00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn cash_movement_view_positive_amount_kept() {
        let m = models::CashMovement { amount: 4200, ..Default::default() };
        let v = cash_movement_view(&m);
        assert_eq!(v.amount_minor, 4200);
    }

    // ── shift_summary_view: Option<Option<T>> flatten ────────────────────────

    #[test]
    fn shift_summary_view_flattens_present_double_options() {
        let s = models::Shift {
            status: "closed".into(),
            opening_cash: 50000,
            branch_name: Some(Some("Maadi".into())),
            closed_at: Some(Some(
                chrono::DateTime::parse_from_rfc3339("2026-06-20T18:00:00Z").unwrap(),
            )),
            closing_cash_declared: Some(Some(60000)),
            closing_cash_system: Some(Some(60500)),
            cash_discrepancy: Some(Some(-500)),
            ..Default::default()
        };
        let v = shift_summary_view(&s);
        assert_eq!(v.branch_name.as_deref(), Some("Maadi"));
        assert!(v.closed_at.unwrap().starts_with("2026-06-20T18:00:00"));
        assert_eq!(v.opening_cash_minor, 50000);
        assert_eq!(v.closing_declared_minor, Some(60000));
        assert_eq!(v.closing_system_minor, Some(60500));
        assert_eq!(v.discrepancy_minor, Some(-500));
        assert_eq!(v.status, "closed");
        assert!(!v.is_open);
    }

    #[test]
    fn shift_summary_view_flattens_absent_and_inner_none_to_none() {
        // Outer-None (field absent) and inner-None (explicit JSON null) both
        // collapse to None after `.flatten()`.
        let s = models::Shift {
            status: "open".into(),
            opening_cash: 10000,
            branch_name: None,             // outer none
            closed_at: Some(None),         // inner none (explicit null)
            closing_cash_declared: Some(None),
            closing_cash_system: None,
            cash_discrepancy: Some(None),
            ..Default::default()
        };
        let v = shift_summary_view(&s);
        assert_eq!(v.branch_name, None);
        assert_eq!(v.closed_at, None);
        assert_eq!(v.closing_declared_minor, None);
        assert_eq!(v.closing_system_minor, None);
        assert_eq!(v.discrepancy_minor, None);
        assert!(v.is_open); // status == "open"
    }

    #[test]
    fn shift_summary_view_is_open_only_for_exact_open_status() {
        let mk = |status: &str| models::Shift { status: status.into(), ..Default::default() };
        assert!(shift_summary_view(&mk("open")).is_open);
        assert!(!shift_summary_view(&mk("closed")).is_open);
        assert!(!shift_summary_view(&mk("force_closed")).is_open);
        assert!(!shift_summary_view(&mk("Open")).is_open); // case-sensitive
    }

    // ── view_from / current / save / clear / close_local ─────────────────────

    #[test]
    fn view_from_maps_core_fields_and_is_open_flag() {
        let s = models::Shift {
            status: "open".into(),
            opening_cash: 25000,
            teller_name: "Sara".into(),
            ..Default::default()
        };
        let v = view_from(&s);
        assert_eq!(v.teller_name, "Sara");
        assert_eq!(v.opening_cash_minor, 25000);
        assert!(v.is_open);
        // ids stringify from the (defaulted) uuids.
        assert_eq!(v.branch_id, "00000000-0000-0000-0000-000000000000");
        assert_eq!(v.teller_id, "00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn save_then_current_roundtrips_a_model() {
        let store = Store::open("").unwrap();
        let s = models::Shift { status: "open".into(), opening_cash: 33000, teller_name: "Omar".into(), ..Default::default() };
        save(&store, &s).unwrap();
        let v = current(&store).unwrap().unwrap();
        assert_eq!(v.opening_cash_minor, 33000);
        assert_eq!(v.teller_name, "Omar");
        assert!(v.is_open);
    }

    #[test]
    fn clear_makes_current_none() {
        let store = Store::open("").unwrap();
        save(&store, &models::Shift { status: "open".into(), ..Default::default() }).unwrap();
        assert!(current(&store).unwrap().is_some());
        clear(&store).unwrap();
        assert!(current(&store).unwrap().is_none());
        // The literal "null" is what's stored, read back as None.
        assert_eq!(store.kv_get(CURRENT_SHIFT_KEY).unwrap().as_deref(), Some("null"));
    }

    #[test]
    fn current_treats_literal_null_as_none() {
        let store = Store::open("").unwrap();
        store.kv_put(CURRENT_SHIFT_KEY, "null").unwrap();
        assert!(current(&store).unwrap().is_none());
    }

    #[test]
    fn close_local_flips_status_to_closed() {
        let store = Store::open("").unwrap();
        save(&store, &models::Shift { status: "open".into(), opening_cash: 12000, ..Default::default() }).unwrap();
        close_local(&store).unwrap();
        let v = current(&store).unwrap().unwrap();
        assert_eq!(v.status, "closed");
        assert!(!v.is_open);
        assert_eq!(v.opening_cash_minor, 12000); // other fields untouched
    }

    #[test]
    fn close_local_is_noop_without_a_cached_shift() {
        let store = Store::open("").unwrap();
        // No shift saved at all.
        assert!(close_local(&store).is_ok());
        assert!(current(&store).unwrap().is_none());
        // And a no-op on an explicitly-cleared ("null") cache.
        clear(&store).unwrap();
        assert!(close_local(&store).is_ok());
        assert!(current(&store).unwrap().is_none());
    }

    #[test]
    fn close_local_is_idempotent() {
        let store = Store::open("").unwrap();
        save(&store, &models::Shift { status: "open".into(), ..Default::default() }).unwrap();
        close_local(&store).unwrap();
        close_local(&store).unwrap(); // second call stays closed
        assert_eq!(current(&store).unwrap().unwrap().status, "closed");
    }

    // ── suggested opening cash: clamp boundary ───────────────────────────────

    #[test]
    fn cache_suggested_opening_cash_clamps_negative_to_zero_exactly() {
        let store = Store::open("").unwrap();
        cache_suggested_opening_cash(&store, 0).unwrap(); // boundary: 0 stays 0
        assert_eq!(suggested_opening_cash(&store).unwrap(), 0);
        cache_suggested_opening_cash(&store, -1).unwrap(); // just below clamps
        assert_eq!(suggested_opening_cash(&store).unwrap(), 0);
        cache_suggested_opening_cash(&store, 1).unwrap(); // just above kept
        assert_eq!(suggested_opening_cash(&store).unwrap(), 1);
    }

    #[test]
    fn suggested_opening_cash_defaults_to_zero_on_garbage() {
        let store = Store::open("").unwrap();
        store.kv_put(SUGGESTED_OPEN_CASH_KEY, "not-a-number").unwrap();
        // Unparseable cached value falls back to 0, not an error.
        assert_eq!(suggested_opening_cash(&store).unwrap(), 0);
    }

    // ── reconcile: remaining matrix corners ──────────────────────────────────

    #[test]
    fn reconcile_adopts_carries_the_server_shift_payload() {
        // Adopt actually hands back the server's shift (not a placeholder).
        let mut pf = models::ShiftPreFill::new(true, 0);
        let mut srv = open_shift_model();
        srv.teller_name = "ServerTeller".into();
        pf.open_shift = Some(Some(Box::new(srv)));
        match reconcile(&pf, false, false) {
            ShiftReconcile::Adopt(s) => assert_eq!(s.teller_name, "ServerTeller"),
            other => panic!("expected Adopt, got {other:?}"),
        }
    }

    #[test]
    fn reconcile_flag_true_inner_none_is_treated_as_no_payload() {
        // has_open_shift=true but open_shift = Some(None) (explicit null payload):
        // falls through to the pending/clear branch like a missing payload.
        let mut pf = models::ShiftPreFill::new(true, 0);
        pf.open_shift = Some(None);
        assert!(matches!(reconcile(&pf, true, false), ShiftReconcile::KeepLocal));
        assert!(matches!(reconcile(&pf, false, false), ShiftReconcile::Clear));
    }

    #[test]
    fn reconcile_close_pending_irrelevant_when_server_has_no_shift() {
        // close_pending only matters on the server-open branch; with no server
        // shift, the open_pending logic governs regardless of close_pending.
        let pf = models::ShiftPreFill::new(false, 0);
        assert!(matches!(reconcile(&pf, true, true), ShiftReconcile::KeepLocal));
        assert!(matches!(reconcile(&pf, false, true), ShiftReconcile::Clear));
    }

    #[test]
    fn reconcile_both_pending_with_server_open_keeps_local() {
        // Server open + both commands pending: close_pending short-circuits to
        // KeepLocal (the reverse bounce wins over a re-adopt).
        let mut pf = models::ShiftPreFill::new(true, 0);
        pf.open_shift = Some(Some(Box::new(open_shift_model())));
        assert!(matches!(reconcile(&pf, true, true), ShiftReconcile::KeepLocal));
    }
}
