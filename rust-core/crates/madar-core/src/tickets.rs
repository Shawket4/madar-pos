//! Waiter open tickets — client side (PLAN §"waiter fire-now-pay-later").
//!
//! A waiter prices a dine-in cart with the SAME client-authoritative engine the
//! POS checkout uses (`checkout::lines_to_wire_items`) and FIRES it as an unpaid
//! open ticket; items are added in later ROUNDS; a cashier SETTLES it into a paid
//! order. Every write is offline-first: it rides the durable outbox →
//! `POST /sync/replay` (the backend ticket replay ops), deduped on a client-minted
//! key, so a fire/round/settle survives a dropped network exactly like a sale.
//!
//! This module holds the durable command shapes + the FFI view DTOs + pure
//! mappers. The exported `MadarCore` methods (fire/add_round/list/get/void/settle)
//! live in `lib.rs` alongside the other outbox entry points.

use serde::{Deserialize, Serialize};
use madar_api::models;

// ── Durable outbox commands (persisted as the op payload) ─────────────────────

/// Fire a new ticket (round 1). `ticket_id` is the client-minted ticket
/// idempotency key (also the outbox row id) — exactly-once across LAN + cloud.
#[derive(Serialize, Deserialize)]
pub(crate) struct FireTicketCommand {
    pub ticket_id: String,
    pub request: models::CreateOpenTicketRequest,
}

/// Add a round to an existing ticket. `round_id` is the per-round idempotency key.
#[derive(Serialize, Deserialize)]
pub(crate) struct AddRoundCommand {
    pub ticket_id: String,
    pub round_id: String,
    pub request: models::AddRoundRequest,
}

/// Settle a ticket into a paid order in the cashier's shift.
#[derive(Serialize, Deserialize)]
pub(crate) struct SettleTicketCommand {
    pub ticket_id: String,
    pub request: models::SettleOpenTicketRequest,
}

/// Void a ticket (and pull its kitchen tickets off the KDS).
#[derive(Serialize, Deserialize)]
pub(crate) struct VoidTicketCommand {
    pub ticket_id: String,
    pub request: models::VoidOpenTicketRequest,
}

// ── FFI view DTOs ─────────────────────────────────────────────────────────────

/// The slim "sent to kitchen" confirmation after a fire/round — deliberately NOT
/// a money-laden receipt (a fired ticket has no payment yet). `queued_offline` is
/// true when the fire is still in the outbox (no network) — the UI shows "queued".
#[derive(uniffi::Record, Clone, Debug)]
pub struct TicketFiredView {
    /// The client ticket id (idempotency key) — stable across the offline→online
    /// transition, so the UI can track the ticket before the server view arrives.
    pub ticket_id: String,
    /// The server-minted human ref (`T-…`), once known (None while queued offline).
    pub ticket_ref: Option<String>,
    pub queued_offline: bool,
}

/// An open ticket for the waiter list / detail screens.
#[derive(uniffi::Record, Clone, Debug)]
pub struct TicketView {
    pub id: String,
    pub ticket_ref: Option<String>,
    pub table_id: Option<String>,
    /// open | ready | settled | voided | queued (the last = still in the outbox).
    pub status: String,
    pub customer_name: Option<String>,
    /// The WAITER who opened this ticket (`open_tickets.opened_by` → user name),
    /// so the teller can see who took the table. `null` if the name is unknown.
    pub waiter_name: Option<String>,
    pub guest_count: Option<i32>,
    pub subtotal_minor: i64,
    pub order_id: Option<String>,
    pub opened_at: String,
    pub queued_offline: bool,
    pub lines: Vec<TicketLineView>,
}

/// One bill line (display projection of the frozen `StoredTicketLine`).
// PartialEq/Eq + serde so it can be embedded in `DeliveryOrderView` (which derives
// them) — tickets and delivery share this one line shape so both render identically.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TicketLineView {
    pub name: String,
    pub qty: i32,
    pub size_label: Option<String>,
    pub modifiers: Vec<String>,
    pub line_total_minor: i64,
    pub voided: bool,
}

// ── Request builders ──────────────────────────────────────────────────────────

/// Assemble the fire (round-1) request from priced cart items. Pricing is
/// client-authoritative (`items` already carry their charged `unit_price`); the
/// backend records them verbatim and settles them into a byte-identical order.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_fire_request(
    branch_id: uuid::Uuid,
    items: Vec<models::OrderItemInput>,
    ticket_id: uuid::Uuid,
    round_id: uuid::Uuid,
    table_id: Option<uuid::Uuid>,
    customer_name: Option<String>,
    notes: Option<String>,
    guest_count: Option<i32>,
) -> models::CreateOpenTicketRequest {
    let mut r = models::CreateOpenTicketRequest::new(branch_id, items);
    r.idempotency_key = Some(Some(ticket_id));
    r.round_idempotency_key = Some(Some(round_id));
    r.table_id = table_id.map(Some);
    r.customer_name = customer_name.filter(|s| !s.trim().is_empty()).map(Some);
    r.notes = notes.filter(|s| !s.trim().is_empty()).map(Some);
    r.guest_count = guest_count.map(Some);
    r
}

/// Assemble an add-round request (its own per-round idempotency key).
pub(crate) fn build_round_request(
    items: Vec<models::OrderItemInput>,
    round_id: uuid::Uuid,
) -> models::AddRoundRequest {
    let mut r = models::AddRoundRequest::new(items);
    r.idempotency_key = Some(Some(round_id));
    r
}

// ── Mappers (generated model → FFI view) ──────────────────────────────────────

/// Flatten a double-`Option` (the generated nullable shape) to a single `Option`.
fn flat<T: Clone>(o: &Option<Option<T>>) -> Option<T> {
    o.as_ref().and_then(|x| x.clone())
}

/// Project a server `OpenTicketView` to the FFI `TicketView`. `queued_offline` is
/// set by the caller (true for a still-outboxed fire that has no server view).
pub(crate) fn to_view(v: &models::OpenTicketView, queued_offline: bool) -> TicketView {
    TicketView {
        id: v.id.to_string(),
        ticket_ref: flat(&v.ticket_ref),
        table_id: flat(&v.table_id).map(|u| u.to_string()),
        status: v.status.clone(),
        customer_name: flat(&v.customer_name),
        waiter_name: flat(&v.opened_by_name).filter(|s| !s.is_empty()),
        guest_count: flat(&v.guest_count),
        subtotal_minor: v.subtotal as i64,
        order_id: flat(&v.order_id).map(|u| u.to_string()),
        opened_at: v.opened_at.to_rfc3339(),
        queued_offline,
        lines: v.items.iter().map(line_view).collect(),
    }
}

/// Project one bill item, reading the display fields out of the frozen `line`
/// JSON (the `StoredTicketLine` projection: name / size_label / modifiers / qty).
fn line_view(it: &models::OpenTicketItemView) -> TicketLineView {
    let line = it.line.as_ref();
    let s = |k: &str| line.and_then(|l| l.get(k)).and_then(|v| v.as_str()).map(|s| s.to_string());
    let modifiers = line
        .and_then(|l| l.get("modifiers"))
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|m| m.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let qty = line
        .and_then(|l| l.get("qty"))
        .and_then(|v| v.as_i64())
        .unwrap_or(1) as i32;
    TicketLineView {
        name: s("name").unwrap_or_else(|| "Item".to_string()),
        qty,
        size_label: s("size_label"),
        modifiers,
        line_total_minor: it.line_total as i64,
        voided: it.voided,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fire_request_carries_idempotency_and_optionals() {
        let tid = uuid::Uuid::new_v4();
        let rid = uuid::Uuid::new_v4();
        let bid = uuid::Uuid::new_v4();
        let r = build_fire_request(
            bid,
            vec![models::OrderItemInput::new(2)],
            tid,
            rid,
            None,
            Some("  ".into()), // whitespace-only customer → dropped
            Some("extra hot".into()),
            Some(4),
        );
        assert_eq!(r.branch_id, bid);
        assert_eq!(r.idempotency_key, Some(Some(tid)));
        assert_eq!(r.round_idempotency_key, Some(Some(rid)));
        assert_eq!(r.customer_name, None, "blank customer name dropped");
        assert_eq!(r.notes, Some(Some("extra hot".into())));
        assert_eq!(r.guest_count, Some(Some(4)));
        assert_eq!(r.items.len(), 1);
    }

    #[test]
    fn line_view_reads_frozen_json() {
        let it = models::OpenTicketItemView {
            id: uuid::Uuid::new_v4(),
            line: Some(serde_json::json!({
                "name": "Burger", "size_label": "Large", "qty": 3,
                "modifiers": ["No onion", "Extra cheese"]
            })),
            line_total: 4500,
            menu_item_id: None,
            round_number: 1,
            voided: false,
        };
        let lv = line_view(&it);
        assert_eq!(lv.name, "Burger");
        assert_eq!(lv.qty, 3);
        assert_eq!(lv.size_label.as_deref(), Some("Large"));
        assert_eq!(lv.modifiers, vec!["No onion", "Extra cheese"]);
        assert_eq!(lv.line_total_minor, 4500);
        assert!(!lv.voided);
    }

    #[test]
    fn to_view_flattens_double_options() {
        let v = models::OpenTicketView {
            id: uuid::Uuid::new_v4(),
            branch_id: uuid::Uuid::new_v4(),
            table_id: None,
            ticket_ref: Some(Some("T-BR-260625-0001".into())),
            status: "open".into(),
            opened_by: uuid::Uuid::new_v4(),
            opened_by_name: Some(Some("Sara".into())),
            customer_name: None,
            notes: None,
            guest_count: Some(Some(2)),
            subtotal: 2000,
            order_id: None,
            opened_at: chrono::Utc::now().fixed_offset(),
            ready_at: None,
            settled_at: None,
            items: vec![],
        };
        let tv = to_view(&v, false);
        assert_eq!(tv.ticket_ref.as_deref(), Some("T-BR-260625-0001"));
        assert_eq!(tv.guest_count, Some(2));
        assert_eq!(tv.subtotal_minor, 2000);
        assert_eq!(tv.status, "open");
        assert!(!tv.queued_offline);
        assert_eq!(tv.waiter_name.as_deref(), Some("Sara"), "the ticket's opener is exposed as the waiter");
    }
}
