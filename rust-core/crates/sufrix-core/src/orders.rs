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

/// One line of a fetched order (item + its chosen modifiers) — the expanded
/// history detail.
#[derive(uniffi::Record, Clone, Debug)]
pub struct OrderDetailLineView {
    pub name: String,
    pub qty: i64,
    pub size_label: Option<String>,
    pub line_total_minor: i64,
    /// Addon labels ("Oat milk ×2"), already qty-suffixed for display.
    pub addons: Vec<String>,
    /// Optional-field labels.
    pub optionals: Vec<String>,
}

/// A fetched order with its lines — drives the history detail + reprint.
#[derive(uniffi::Record, Clone, Debug)]
pub struct OrderDetailView {
    pub id: String,
    pub order_number: Option<i32>,
    pub status: String,
    pub payment_label: String,
    pub subtotal_minor: i64,
    pub discount_minor: i64,
    pub tax_minor: i64,
    pub total_minor: i64,
    pub created_at: String,
    pub lines: Vec<OrderDetailLineView>,
}

pub(crate) fn order_detail_view(o: &models::OrderFull) -> OrderDetailView {
    OrderDetailView {
        id: o.id.to_string(),
        order_number: Some(o.order_number),
        status: o.status.clone(),
        payment_label: o.payment_method.clone(),
        subtotal_minor: o.subtotal as i64,
        discount_minor: o.discount_amount as i64,
        tax_minor: o.tax_amount as i64,
        total_minor: o.total_amount as i64,
        created_at: o.created_at.to_rfc3339(),
        lines: o
            .items
            .iter()
            .map(|it| OrderDetailLineView {
                name: it.item_name.clone(),
                qty: it.quantity as i64,
                size_label: it.size_label.clone().filter(|s| !s.is_empty()),
                line_total_minor: it.line_total as i64,
                addons: it
                    .addons
                    .iter()
                    .map(|a| if a.quantity > 1 { format!("{} ×{}", a.addon_name, a.quantity) } else { a.addon_name.clone() })
                    .collect(),
                optionals: it.optionals.iter().map(|op| op.field_name.clone()).collect(),
            })
            .collect(),
    }
}

/// Project a fetched order into a printable receipt (reprint from history) —
/// the full breakdown (modifiers, bundle components, delivery block) so a
/// reprint is byte-identical to the original. `locale` localizes the address
/// "Unit"/"Floor" prefixes.
pub(crate) fn order_to_receipt(o: &models::OrderFull, locale: &str) -> crate::checkout::ReceiptView {
    use crate::checkout::{ReceiptComponentView, ReceiptLineView, ReceiptModifierView};

    let lines = o
        .items
        .iter()
        .map(|it| {
            let addons = it
                .addons
                .iter()
                .map(|a| ReceiptModifierView {
                    name: if a.quantity > 1 { format!("{} ×{}", a.addon_name, a.quantity) } else { a.addon_name.clone() },
                    price_minor: a.unit_price as i64,
                })
                .collect();
            let optionals = it
                .optionals
                .iter()
                .map(|op| ReceiptModifierView { name: op.field_name.clone(), price_minor: op.price as i64 })
                .collect();
            let components = it
                .bundle_components
                .as_ref()
                .map(|cs| {
                    cs.iter()
                        .map(|c| ReceiptComponentView {
                            name: c.item_name.clone(),
                            size_label: c.size_label.clone().flatten().filter(|s| !s.is_empty()),
                            addons: c
                                .addons
                                .iter()
                                .map(|a| ReceiptModifierView {
                                    name: if a.quantity > 1 {
                                        format!("{} ×{}", a.addon_name, a.quantity)
                                    } else {
                                        a.addon_name.clone()
                                    },
                                    price_minor: a.unit_price as i64,
                                })
                                .collect(),
                            optionals: c
                                .optionals
                                .iter()
                                .map(|op| ReceiptModifierView { name: op.field_name.clone(), price_minor: op.price as i64 })
                                .collect(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            ReceiptLineView {
                name: it.item_name.clone(),
                qty: it.quantity as i64,
                size_label: it.size_label.clone().filter(|s| !s.is_empty()),
                line_total_minor: it.line_total as i64,
                is_bundle: it.bundle_id.is_some(),
                addons,
                optionals,
                components,
            }
        })
        .collect();

    let is_delivery = o.order_type == "delivery";
    let dinfo = o.delivery.clone().flatten();
    let delivery_address = dinfo.as_ref().and_then(|d| compose_address(d, locale));

    crate::checkout::ReceiptView {
        local_order_id: o.id.to_string(),
        order_number: Some(o.order_number as i64),
        order_ref: o.order_ref.clone().filter(|s| !s.is_empty()),
        is_voided: o.status == "voided",
        lines,
        payment_label: o.payment_method.clone(),
        subtotal_minor: o.subtotal as i64,
        discount_minor: o.discount_amount as i64,
        tax_minor: o.tax_amount as i64,
        delivery_fee_minor: o.delivery_fee as i64,
        total_minor: o.total_amount as i64,
        tip_minor: o.tip_amount.unwrap_or(0) as i64,
        amount_tendered_minor: o.amount_tendered.unwrap_or(0) as i64,
        change_minor: o.change_given.unwrap_or(0) as i64,
        is_cash: o.amount_tendered.is_some(),
        customer_name: o.customer_name.clone().filter(|s| !s.is_empty()),
        teller_name: Some(o.teller_name.clone()).filter(|s| !s.is_empty()),
        is_delivery,
        delivery_channel: if is_delivery {
            o.delivery_channel.clone().filter(|s| !s.is_empty())
        } else {
            None
        },
        customer_phone: dinfo.as_ref().map(|d| d.customer_phone.clone()).filter(|s| !s.is_empty()),
        delivery_address,
        delivery_zone: dinfo.as_ref().and_then(|d| d.zone_name.clone().flatten()).filter(|s| !s.is_empty()),
        delivery_ref: dinfo.as_ref().and_then(|d| d.delivery_ref.clone().flatten()).filter(|s| !s.is_empty()),
        payment_hint: dinfo.as_ref().and_then(|d| d.payment_method_hint.clone().flatten()).filter(|s| !s.is_empty()),
        delivery_notes: dinfo.as_ref().and_then(|d| d.delivery_notes.clone().flatten()).filter(|s| !s.is_empty()),
        queued_offline: false,
        created_at: o.created_at.to_rfc3339(),
    }
}

/// Compose a one-line delivery address from its parts, in Flutter's order:
/// place name, address line, unit, floor, landmark — comma-joined, skipping
/// blanks. `None` when nothing is set.
fn compose_address(d: &models::OrderDeliveryInfo, locale: &str) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let push = |parts: &mut Vec<String>, v: &Option<Option<String>>| {
        if let Some(s) = v.clone().flatten() {
            if !s.trim().is_empty() {
                parts.push(s);
            }
        }
    };
    push(&mut parts, &d.place_name);
    push(&mut parts, &d.address_line);
    if let Some(u) = d.unit_number.clone().flatten().filter(|s| !s.trim().is_empty()) {
        parts.push(format!("{} {}", crate::i18n::tr(locale, "delivery.unit"), u));
    }
    if let Some(f) = d.floor.clone().flatten().filter(|s| !s.trim().is_empty()) {
        parts.push(format!("{} {}", crate::i18n::tr(locale, "delivery.floor"), f));
    }
    push(&mut parts, &d.landmark);
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
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
                ..Default::default()
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
                ..Default::default()
            })
            .unwrap();

        let ids = pending_void_ids(&store).unwrap();
        assert!(ids.contains("srv-order-1"));
        assert_eq!(ids.len(), 1);
    }

    // ---- builders for the wire models ----------------------------------

    fn uid(b: u8) -> uuid::Uuid {
        let mut bytes = [0u8; 16];
        bytes[15] = b;
        uuid::Uuid::from_bytes(bytes)
    }

    fn ts() -> chrono::DateTime<chrono::FixedOffset> {
        chrono::DateTime::parse_from_rfc3339("2026-06-21T10:00:00+00:00").unwrap()
    }

    fn addon(name: &str, qty: i32, unit_price: i32) -> models::OrderItemAddon {
        let mut a = models::OrderItemAddon::new(
            uid(1),
            name.into(),
            uid(2),
            unit_price * qty,
            serde_json::json!({}),
            uid(3),
            qty,
            unit_price,
        );
        a.line_total = unit_price * qty;
        a
    }

    fn optional(field: &str, price: i32) -> models::OrderItemOptional {
        models::OrderItemOptional::new(field.into(), uid(4), serde_json::json!({}), uid(5), price)
    }

    fn comp_addon(name: &str, qty: i32, unit_price: i32) -> models::OrderBundleComponentAddon {
        models::OrderBundleComponentAddon::new(
            uid(6),
            name.into(),
            uid(7),
            uid(8),
            unit_price * qty,
            serde_json::json!({}),
            uid(9),
            qty,
            unit_price,
        )
    }

    fn comp_optional(field: &str, price: i32) -> models::OrderBundleComponentOptional {
        models::OrderBundleComponentOptional::new(uid(10), field.into(), uid(11), serde_json::json!({}), uid(12), price)
    }

    fn bundle_component(name: &str, size: Option<&str>) -> models::OrderBundleComponentFull {
        let mut c = models::OrderBundleComponentFull::new(vec![], uid(13), name.into(), serde_json::json!({}), vec![], 1);
        c.size_label = size.map(|s| Some(s.to_string()));
        c
    }

    fn item(name: &str, qty: i32, line_total: i32) -> models::OrderItemFull {
        models::OrderItemFull::new(
            false,
            None,
            uid(20),
            name.into(),
            line_total,
            serde_json::json!({}),
            uid(21),
            qty,
            line_total / qty.max(1),
            vec![],
            vec![],
        )
    }

    fn order_full(items: Vec<models::OrderItemFull>) -> models::OrderFull {
        let mut o = models::OrderFull::new(
            uid(30),
            ts(),
            0,    // delivery_fee
            0,    // discount_amount
            0,    // discount_value
            uid(31),
            42,   // order_number
            "dine_in".into(),
            "Cash".into(),
            uid(32),
            "completed".into(),
            10000, // subtotal
            1400,  // tax_amount
            uid(33),
            "Tara".into(), // teller_name
            11400, // total_amount
            items,
        );
        o.customer_name = Some("Alice".into());
        o
    }

    fn server_order(total: i32, status: &str) -> models::Order {
        let mut o = models::Order::new(
            uid(40),
            ts(),
            0,
            0,
            0,
            uid(41),
            7,
            "dine_in".into(),
            "Card".into(),
            uid(42),
            status.into(),
            (total as f64 / 1.14).round() as i32,
            total - (total as f64 / 1.14).round() as i32,
            uid(43),
            "Bob".into(),
            total,
        );
        o.status = status.into();
        o
    }

    fn delivery_info() -> models::OrderDeliveryInfo {
        models::OrderDeliveryInfo::new("outside".into(), "01000000000".into())
    }

    // ---- order_detail_view ---------------------------------------------

    #[test]
    fn order_detail_view_maps_top_level_and_money() {
        let o = order_full(vec![item("Latte", 2, 6000)]);
        let v = order_detail_view(&o);
        assert_eq!(v.id, o.id.to_string());
        assert_eq!(v.order_number, Some(42));
        assert_eq!(v.status, "completed");
        assert_eq!(v.payment_label, "Cash");
        assert_eq!(v.subtotal_minor, 10000);
        assert_eq!(v.discount_minor, 0);
        assert_eq!(v.tax_minor, 1400);
        assert_eq!(v.total_minor, 11400);
        assert_eq!(v.created_at, ts().to_rfc3339());
        assert_eq!(v.lines.len(), 1);
        assert_eq!(v.lines[0].name, "Latte");
        assert_eq!(v.lines[0].qty, 2);
        assert_eq!(v.lines[0].line_total_minor, 6000);
    }

    #[test]
    fn order_detail_view_empty_size_label_becomes_none() {
        let mut it = item("Espresso", 1, 3000);
        it.size_label = Some(String::new()); // blank → filtered out
        let o = order_full(vec![it]);
        let v = order_detail_view(&o);
        assert_eq!(v.lines[0].size_label, None);
    }

    #[test]
    fn order_detail_view_keeps_nonempty_size_label() {
        let mut it = item("Espresso", 1, 3000);
        it.size_label = Some("Large".into());
        let o = order_full(vec![it]);
        let v = order_detail_view(&o);
        assert_eq!(v.lines[0].size_label.as_deref(), Some("Large"));
    }

    #[test]
    fn order_detail_view_addon_qty_suffix() {
        let mut it = item("Latte", 1, 6000);
        it.addons = vec![addon("Oat milk", 2, 500), addon("Shot", 1, 700)];
        it.optionals = vec![optional("Extra hot", 0)];
        let o = order_full(vec![it]);
        let v = order_detail_view(&o);
        // qty>1 gets the " ×N" suffix; qty==1 stays bare.
        assert_eq!(v.lines[0].addons, vec!["Oat milk ×2".to_string(), "Shot".to_string()]);
        assert_eq!(v.lines[0].optionals, vec!["Extra hot".to_string()]);
    }

    #[test]
    fn order_detail_view_empty_order_has_no_lines() {
        let o = order_full(vec![]);
        let v = order_detail_view(&o);
        assert!(v.lines.is_empty());
    }

    // ---- order_to_receipt ----------------------------------------------

    #[test]
    fn receipt_basic_fields_and_cash_flag() {
        let mut o = order_full(vec![item("Latte", 1, 6000)]);
        o.order_ref = Some("DT-260614-0042".into());
        o.amount_tendered = Some(12000); // cash → is_cash true, tendered carried
        o.change_given = Some(600);
        o.tip_amount = Some(300);
        let r = order_to_receipt(&o, "en");
        assert_eq!(r.local_order_id, o.id.to_string());
        assert_eq!(r.order_number, Some(42));
        assert_eq!(r.order_ref.as_deref(), Some("DT-260614-0042"));
        assert!(!r.is_voided);
        assert_eq!(r.payment_label, "Cash");
        assert_eq!(r.subtotal_minor, 10000);
        assert_eq!(r.tax_minor, 1400);
        assert_eq!(r.total_minor, 11400);
        assert_eq!(r.tip_minor, 300);
        assert_eq!(r.amount_tendered_minor, 12000);
        assert_eq!(r.change_minor, 600);
        assert!(r.is_cash);
        assert_eq!(r.customer_name.as_deref(), Some("Alice"));
        assert_eq!(r.teller_name.as_deref(), Some("Tara"));
        assert!(!r.queued_offline);
        assert_eq!(r.created_at, ts().to_rfc3339());
    }

    #[test]
    fn receipt_non_cash_when_no_tender() {
        let o = order_full(vec![item("Latte", 1, 6000)]);
        // amount_tendered None → not cash, tendered/change default to 0.
        let r = order_to_receipt(&o, "en");
        assert!(!r.is_cash);
        assert_eq!(r.amount_tendered_minor, 0);
        assert_eq!(r.change_minor, 0);
        assert_eq!(r.tip_minor, 0);
    }

    #[test]
    fn receipt_is_cash_true_even_for_zero_tender() {
        let mut o = order_full(vec![item("Latte", 1, 6000)]);
        o.amount_tendered = Some(0); // present (Some) → cash, boundary at 0
        let r = order_to_receipt(&o, "en");
        assert!(r.is_cash);
        assert_eq!(r.amount_tendered_minor, 0);
    }

    #[test]
    fn receipt_voided_status_sets_flag() {
        let mut o = order_full(vec![item("Latte", 1, 6000)]);
        o.status = "voided".into();
        let r = order_to_receipt(&o, "en");
        assert!(r.is_voided);
    }

    #[test]
    fn receipt_blank_order_ref_and_customer_filtered() {
        let mut o = order_full(vec![item("Latte", 1, 6000)]);
        o.order_ref = Some(String::new());
        o.customer_name = Some(String::new());
        o.teller_name = String::new();
        let r = order_to_receipt(&o, "en");
        assert_eq!(r.order_ref, None);
        assert_eq!(r.customer_name, None);
        assert_eq!(r.teller_name, None);
    }

    #[test]
    fn receipt_line_modifiers_with_qty_suffix() {
        let mut it = item("Latte", 1, 6000);
        it.addons = vec![addon("Oat milk", 2, 500), addon("Caramel", 1, 400)];
        it.optionals = vec![optional("No sugar", 0), optional("Extra shot", 700)];
        it.size_label = Some("Large".into());
        let o = order_full(vec![it]);
        let r = order_to_receipt(&o, "en");
        let line = &r.lines[0];
        assert_eq!(line.size_label.as_deref(), Some("Large"));
        assert!(!line.is_bundle);
        assert_eq!(line.addons[0].name, "Oat milk ×2");
        assert_eq!(line.addons[0].price_minor, 500); // unit_price, not line_total
        assert_eq!(line.addons[1].name, "Caramel");
        assert_eq!(line.optionals[0].name, "No sugar");
        assert_eq!(line.optionals[1].price_minor, 700);
        assert!(line.components.is_empty());
    }

    #[test]
    fn receipt_bundle_line_components_composed() {
        let mut it = item("Combo", 1, 9000);
        it.bundle_id = Some(uid(50)); // → is_bundle
        let mut c1 = bundle_component("Burger", Some("Large"));
        c1.addons = vec![comp_addon("Cheese", 2, 300)];
        c1.optionals = vec![comp_optional("No onion", 0)];
        let mut c2 = bundle_component("Fries", Some(""));
        c2.size_label = Some(Some(String::new())); // blank component size → None
        it.bundle_components = Some(vec![c1, c2]);
        let o = order_full(vec![it]);
        let r = order_to_receipt(&o, "en");
        let line = &r.lines[0];
        assert!(line.is_bundle);
        assert_eq!(line.components.len(), 2);
        assert_eq!(line.components[0].name, "Burger");
        assert_eq!(line.components[0].size_label.as_deref(), Some("Large"));
        assert_eq!(line.components[0].addons[0].name, "Cheese ×2");
        assert_eq!(line.components[0].addons[0].price_minor, 300);
        assert_eq!(line.components[0].optionals[0].name, "No onion");
        assert_eq!(line.components[1].name, "Fries");
        assert_eq!(line.components[1].size_label, None); // blank filtered
    }

    #[test]
    fn receipt_bundle_id_without_components_yields_empty_vec() {
        let mut it = item("Combo", 1, 9000);
        it.bundle_id = Some(uid(50));
        // bundle_components None → components default to empty.
        let o = order_full(vec![it]);
        let r = order_to_receipt(&o, "en");
        assert!(r.lines[0].is_bundle);
        assert!(r.lines[0].components.is_empty());
    }

    #[test]
    fn receipt_non_delivery_has_no_delivery_block() {
        let o = order_full(vec![item("Latte", 1, 6000)]); // order_type dine_in
        let r = order_to_receipt(&o, "en");
        assert!(!r.is_delivery);
        assert_eq!(r.delivery_channel, None);
        assert_eq!(r.delivery_address, None);
        assert_eq!(r.customer_phone, None);
        assert_eq!(r.delivery_fee_minor, 0);
    }

    #[test]
    fn receipt_delivery_block_composed() {
        let mut o = order_full(vec![item("Latte", 1, 6000)]);
        o.order_type = "delivery".into();
        o.delivery_channel = Some("outside".into());
        o.delivery_fee = 1500;
        let mut d = delivery_info();
        d.place_name = Some(Some("Tower A".into()));
        d.address_line = Some(Some("12 Main St".into()));
        d.unit_number = Some(Some("4B".into()));
        d.floor = Some(Some("3".into()));
        d.landmark = Some(Some("Near park".into()));
        d.zone_name = Some(Some("Zone 5".into()));
        d.delivery_ref = Some(Some("D-DT-0042".into()));
        d.payment_method_hint = Some(Some("cash".into()));
        d.delivery_notes = Some(Some("Ring bell".into()));
        o.delivery = Some(Some(Box::new(d)));
        let r = order_to_receipt(&o, "en");
        assert!(r.is_delivery);
        assert_eq!(r.delivery_channel.as_deref(), Some("outside"));
        assert_eq!(r.delivery_fee_minor, 1500);
        assert_eq!(r.customer_phone.as_deref(), Some("01000000000"));
        assert_eq!(
            r.delivery_address.as_deref(),
            Some("Tower A, 12 Main St, Unit 4B, Floor 3, Near park")
        );
        assert_eq!(r.delivery_zone.as_deref(), Some("Zone 5"));
        assert_eq!(r.delivery_ref.as_deref(), Some("D-DT-0042"));
        assert_eq!(r.payment_hint.as_deref(), Some("cash"));
        assert_eq!(r.delivery_notes.as_deref(), Some("Ring bell"));
    }

    #[test]
    fn receipt_delivery_channel_blank_filtered() {
        let mut o = order_full(vec![item("Latte", 1, 6000)]);
        o.order_type = "delivery".into();
        o.delivery_channel = Some(String::new()); // blank → None even when delivery
        o.delivery = Some(Some(Box::new(delivery_info())));
        let r = order_to_receipt(&o, "en");
        assert!(r.is_delivery);
        assert_eq!(r.delivery_channel, None);
    }

    #[test]
    fn receipt_delivery_address_arabic_prefixes() {
        let mut o = order_full(vec![item("Latte", 1, 6000)]);
        o.order_type = "delivery".into();
        let mut d = delivery_info();
        d.unit_number = Some(Some("4B".into()));
        d.floor = Some(Some("3".into()));
        o.delivery = Some(Some(Box::new(d)));
        let r = order_to_receipt(&o, "ar");
        assert_eq!(r.delivery_address.as_deref(), Some("وحدة 4B, طابق 3"));
    }

    #[test]
    fn receipt_delivery_address_none_when_all_blank() {
        let mut o = order_full(vec![item("Latte", 1, 6000)]);
        o.order_type = "delivery".into();
        let mut d = delivery_info();
        // explicit blanks / whitespace are skipped
        d.place_name = Some(Some("   ".into()));
        d.address_line = Some(Some(String::new()));
        d.unit_number = Some(Some(" ".into()));
        o.delivery = Some(Some(Box::new(d)));
        let r = order_to_receipt(&o, "en");
        assert_eq!(r.delivery_address, None);
    }

    #[test]
    fn receipt_delivery_address_skips_missing_middle_parts() {
        let mut o = order_full(vec![item("Latte", 1, 6000)]);
        o.order_type = "delivery".into();
        let mut d = delivery_info();
        d.place_name = Some(Some("Tower A".into()));
        // no address_line, no unit
        d.floor = Some(Some("2".into()));
        o.delivery = Some(Some(Box::new(d)));
        let r = order_to_receipt(&o, "en");
        assert_eq!(r.delivery_address.as_deref(), Some("Tower A, Floor 2"));
    }

    // ---- from_server ----------------------------------------------------

    #[test]
    fn from_server_projects_synced_order() {
        let o = server_order(11400, "completed");
        let v = from_server(&o);
        assert_eq!(v.id, o.id.to_string());
        assert_eq!(v.order_number, Some(7));
        assert_eq!(v.total_minor, 11400);
        assert_eq!(v.tax_minor, o.tax_amount as i64);
        assert_eq!(v.subtotal_minor, o.subtotal as i64);
        assert_eq!(v.payment_label, "Card");
        assert_eq!(v.status, "completed");
        assert_eq!(v.created_at, ts().to_rfc3339());
        assert!(!v.queued);
    }

    // ---- shift_stats edge cases ----------------------------------------

    #[test]
    fn shift_stats_all_voided_is_zero() {
        let orders = vec![summary(5000, "voided"), summary(3000, "voided")];
        assert_eq!(shift_stats(&orders), ShiftStatsView { sales_minor: 0, order_count: 0 });
    }

    #[test]
    fn shift_stats_single_completed() {
        let s = shift_stats(&[summary(4200, "completed")]);
        assert_eq!(s.order_count, 1);
        assert_eq!(s.sales_minor, 4200);
    }

    // ---- queued additional coverage ------------------------------------

    #[test]
    fn queued_ignores_non_create_order_ops() {
        let store = Store::open("").unwrap();
        let cmd = VoidOrderCommand {
            order_id: "srv-1".into(),
            request: models::VoidOrderRequest::new("oops".into()),
        };
        store
            .enqueue(&crate::store::NewOutboxOp {
                id: "v1".into(),
                op_type: "void_order".into(),
                idempotency_key: "v1".into(),
                payload: serde_json::to_string(&cmd).unwrap(),
                event_at: "2026-06-20T12:00:00+00:00".into(),
                ..Default::default()
            })
            .unwrap();
        assert!(queued(&store, SHIFT).unwrap().is_empty());
    }

    #[test]
    fn queued_skips_malformed_create_order_payload() {
        let store = Store::open("").unwrap();
        store
            .enqueue(&crate::store::NewOutboxOp {
                id: "bad".into(),
                op_type: "create_order".into(),
                idempotency_key: "bad".into(),
                payload: "{not json".into(),
                event_at: "2026-06-20T12:00:00+00:00".into(),
                ..Default::default()
            })
            .unwrap();
        queue_order(&store, "good", SHIFT, 1000);
        let q = queued(&store, SHIFT).unwrap();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].id, "good");
    }

    #[test]
    fn queued_single_order_money_split() {
        let store = Store::open("").unwrap();
        queue_order(&store, "o1", SHIFT, 1140);
        let q = queued(&store, SHIFT).unwrap();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].total_minor, 1140);
        assert_eq!(q[0].subtotal_minor + q[0].tax_minor, 1140);
    }

    #[test]
    fn queued_created_at_is_rfc3339_from_request() {
        let store = Store::open("").unwrap();
        let mut req = models::CreateOrderRequest::new(uid(60), vec![], "Cash".into(), uid(61));
        req.shift_id = uuid::Uuid::parse_str(SHIFT).unwrap();
        req.created_at = Some(Some(ts()));
        let cmd = CheckoutCommand { request: req };
        store
            .enqueue(&crate::store::NewOutboxOp {
                id: "o1".into(),
                op_type: "create_order".into(),
                idempotency_key: "o1".into(),
                payload: serde_json::to_string(&cmd).unwrap(),
                event_at: "2026-06-20T12:00:00+00:00".into(),
                ..Default::default()
            })
            .unwrap();
        let q = queued(&store, SHIFT).unwrap();
        assert_eq!(q[0].created_at, ts().to_rfc3339());
    }

    #[test]
    fn queued_created_at_empty_when_request_has_none() {
        let store = Store::open("").unwrap();
        // queue_order leaves created_at unset → empty string fallback.
        queue_order(&store, "o1", SHIFT, 1000);
        let q = queued(&store, SHIFT).unwrap();
        assert_eq!(q[0].created_at, "");
    }

    #[test]
    fn pending_void_ids_skips_malformed_and_other_ops() {
        let store = Store::open("").unwrap();
        // malformed void payload
        store
            .enqueue(&crate::store::NewOutboxOp {
                id: "vbad".into(),
                op_type: "void_order".into(),
                idempotency_key: "vbad".into(),
                payload: "garbage".into(),
                event_at: "2026-06-20T12:00:00+00:00".into(),
                ..Default::default()
            })
            .unwrap();
        // a create_order op is ignored entirely
        queue_order(&store, "co1", SHIFT, 1000);
        assert!(pending_void_ids(&store).unwrap().is_empty());
    }
}
