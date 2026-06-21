//! Order history reads. The shift's orders = the server's synced orders (online)
//! UNIONed with the locally-queued `create_order` commands still in the outbox
//! (shown as `queued`), so a teller always sees the sale they just rang even
//! before it syncs, and the whole list degrades to just the queued ones offline.
//! Projection is pure (store/outbox in, view DTOs out) so it's unit-testable.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sufrix_api::models;

use crate::error::CoreResult;
use crate::store::Store;

/// Outbox payload for a void-order command — carries the path `order_id`.
#[derive(Serialize, Deserialize)]
pub(crate) struct VoidOrderCommand {
    pub order_id: String,
    pub request: models::VoidOrderRequest,
}

/// Server order ids that have a queued/failed void command — used to overlay an
/// optimistic "voided" status on the synced orders before the void syncs.
pub(crate) fn pending_void_ids(store: &Store) -> CoreResult<HashSet<String>> {
    let mut ids = HashSet::new();
    for item in store.list_active()? {
        if item.op_type != "void_order" {
            continue;
        }
        if let Ok(cmd) = serde_json::from_str::<VoidOrderCommand>(&item.payload) {
            ids.insert(cmd.order_id);
        }
    }
    Ok(ids)
}

/// One order row for the history list (+ a totals detail).
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct OrderSummaryView {
    /// Server id, or the client order uuid while queued.
    pub id: String,
    /// `None` until the server assigns it (queued orders have no number yet).
    pub order_number: Option<i32>,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub total_minor: i64,
    /// Raw payment-method name as recorded on the order.
    pub payment_label: String,
    /// Server status (`completed`/`voided`/…), or `queued`/`failed` for unsynced.
    pub status: String,
    pub created_at: String,
    /// `true` while the order is still in the outbox (not yet on the server).
    pub queued: bool,
}

/// Live shift totals for the action-bar stats pill: sales total + order count,
/// voided orders excluded (a voided sale is no revenue and not a fulfilled
/// order). Mirrors the Flutter pill, which sums `orderHistoryProvider` the same way.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct ShiftStatsView {
    pub sales_minor: i64,
    pub order_count: i64,
}

/// Derive the stats pill from the orders the host already holds (synced +
/// queued, from `list_shift_orders`). Pure — no store/network — so the host can
/// recompute it cheaply whenever the order list changes.
pub fn shift_stats(orders: &[OrderSummaryView]) -> ShiftStatsView {
    let mut sales_minor = 0i64;
    let mut order_count = 0i64;
    for o in orders {
        if o.status == "voided" {
            continue;
        }
        order_count += 1;
        sales_minor += o.total_minor;
    }
    ShiftStatsView { sales_minor, order_count }
}

/// Project a synced server order.
pub(crate) fn from_server(o: &models::Order) -> OrderSummaryView {
    OrderSummaryView {
        id: o.id.to_string(),
        order_number: Some(o.order_number),
        subtotal_minor: o.subtotal as i64,
        tax_minor: o.tax_amount as i64,
        total_minor: o.total_amount as i64,
        payment_label: o.payment_method.clone(),
        status: o.status.clone(),
        created_at: o.created_at.to_rfc3339(),
        queued: false,
    }
}

/// The shift's still-queued orders, newest first — parsed from the outbox's
/// `create_order` commands for `shift_id`. A dead command shows as `failed`.
pub(crate) fn queued(store: &Store, shift_id: &str) -> CoreResult<Vec<OrderSummaryView>> {
    let mut out = Vec::new();
    for item in store.list_active()? {
        if item.op_type != "create_order" {
            continue;
        }
        let cmd: crate::checkout::CheckoutCommand = match serde_json::from_str(&item.payload) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let r = &cmd.request;
        if r.shift_id.to_string() != shift_id {
            continue;
        }
        out.push(OrderSummaryView {
            id: item.id.clone(),
            order_number: None,
            subtotal_minor: flat_i32(&r.subtotal),
            tax_minor: flat_i32(&r.tax_amount),
            total_minor: flat_i32(&r.total_amount),
            payment_label: r.payment_method.clone(),
            status: if item.status == "dead" { "failed".into() } else { "queued".into() },
            created_at: flat(&r.created_at).map(|d| d.to_rfc3339()).unwrap_or_default(),
            queued: true,
        });
    }
    // Outbox is oldest-first; show the latest-rung sale on top.
    out.reverse();
    Ok(out)
}

fn flat<T: Clone>(o: &Option<Option<T>>) -> Option<T> {
    o.clone().flatten()
}
fn flat_i32(o: &Option<Option<i32>>) -> i64 {
    flat(o).unwrap_or(0) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkout::CheckoutCommand;

    const SHIFT: &str = "00000000-0000-0000-0000-0000000000c0";
    const OTHER: &str = "00000000-0000-0000-0000-0000000000c9";

    fn summary(total: i64, status: &str) -> OrderSummaryView {
        OrderSummaryView {
            id: "o".into(),
            order_number: None,
            subtotal_minor: total,
            tax_minor: 0,
            total_minor: total,
            payment_label: "Cash".into(),
            status: status.into(),
            created_at: "2026-06-21T10:00:00Z".into(),
            queued: status == "queued",
        }
    }

    #[test]
    fn shift_stats_sums_nonvoided_and_excludes_voided() {
        let orders = vec![
            summary(5000, "completed"),
            summary(3000, "queued"),
            summary(9999, "voided"), // excluded from both count + total
            summary(2000, "failed"), // an unsynced sale still counts
        ];
        let s = shift_stats(&orders);
        assert_eq!(s.order_count, 3);
        assert_eq!(s.sales_minor, 10000);
        // Empty list → zeros (no open-shift activity yet).
        assert_eq!(shift_stats(&[]), ShiftStatsView { sales_minor: 0, order_count: 0 });
    }

    fn queue_order(store: &Store, id: &str, shift: &str, total: i32) {
        let mut req = models::CreateOrderRequest::new(
            uuid::Uuid::parse_str("00000000-0000-0000-0000-0000000000b0").unwrap(),
            vec![],
            "Cash".into(),
            uuid::Uuid::parse_str(shift).unwrap(),
        );
        req.subtotal = Some(Some((total as f64 / 1.14).round() as i32));
        req.tax_amount = Some(Some(total - (total as f64 / 1.14).round() as i32));
        req.total_amount = Some(Some(total));
        let cmd = CheckoutCommand { request: req };
        store
            .enqueue(&crate::store::NewOutboxOp {
                id: id.into(),
                op_type: "create_order".into(),
                idempotency_key: id.into(),
                payload: serde_json::to_string(&cmd).unwrap(),
                event_at: "2026-06-20T12:00:00+00:00".into(),
                depends_on_seq: None,
            })
            .unwrap();
    }

    #[test]
    fn queued_lists_this_shifts_orders_newest_first() {
        let store = Store::open("").unwrap();
        queue_order(&store, "o1", SHIFT, 1000);
        queue_order(&store, "o2", SHIFT, 2280);
        queue_order(&store, "x1", OTHER, 999); // a different shift → excluded

        let q = queued(&store, SHIFT).unwrap();
        assert_eq!(q.len(), 2);
        assert_eq!(q[0].id, "o2"); // newest first
        assert_eq!(q[0].total_minor, 2280);
        assert!(q[0].queued);
        assert_eq!(q[0].order_number, None);
        assert_eq!(q[0].payment_label, "Cash");
        assert_eq!(q[0].status, "queued");
    }

    #[test]
    fn queued_marks_dead_commands_failed() {
        let store = Store::open("").unwrap();
        queue_order(&store, "o1", SHIFT, 1000);
        // Find its seq and kill it.
        let seq = store.list_active().unwrap()[0].seq;
        store.mark_dead(seq, "rejected").unwrap();
        let q = queued(&store, SHIFT).unwrap();
        assert_eq!(q[0].status, "failed");
    }

    #[test]
    fn empty_when_no_queued_orders() {
        let store = Store::open("").unwrap();
        assert!(queued(&store, SHIFT).unwrap().is_empty());
    }

    #[test]
    fn pending_void_ids_collects_queued_voids() {
        let store = Store::open("").unwrap();
        assert!(pending_void_ids(&store).unwrap().is_empty());

        let cmd = VoidOrderCommand {
            order_id: "srv-order-1".into(),
            request: models::VoidOrderRequest::new("mistake".into()),
        };
        store
            .enqueue(&crate::store::NewOutboxOp {
                id: "srv-order-1:void".into(),
                op_type: "void_order".into(),
                idempotency_key: "srv-order-1:void".into(),
                payload: serde_json::to_string(&cmd).unwrap(),
                event_at: "2026-06-20T12:00:00+00:00".into(),
                depends_on_seq: None,
            })
            .unwrap();

        let ids = pending_void_ids(&store).unwrap();
        assert!(ids.contains("srv-order-1"));
        assert_eq!(ids.len(), 1);
    }
}
