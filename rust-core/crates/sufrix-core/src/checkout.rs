//! Checkout — turn the cart into an order and place it through the durable
//! outbox (offline-safe), mirroring the `open_shift` pattern in `lib.rs`.
//!
//! Pricing is client-authoritative: `pricing::price_cart` is the money source of
//! truth and the server records our subtotal/tax/total **verbatim** (so the DB
//! equals the printed receipt even if the POS was offline or its menu cache was
//! stale). Like `open_shift`, the wire has no client idempotency key, so true
//! exactly-once isn't available: we key the OUTBOX row by a client order UUID
//! (the local queue won't double-enqueue) and ack on 2xx. A duplicate is only
//! possible if the server commits but its response is lost — the same known
//! limitation the shift command carries.
//!
//! Pure assembly lives in `prepare` (store reads only, no network) so it's
//! unit-testable; the FFI in `lib.rs` enqueues, clears the cart, and drains.

use serde::{Deserialize, Serialize};
use sufrix_api::models;

use crate::cart;
use crate::error::{CoreError, CoreResult};
use crate::menu;
use crate::pricing::{self, DiscountKind, PriceCartInput};
use crate::store::Store;

/// The outbox payload for a queued order (op_type `"create_order"`).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CheckoutCommand {
    pub request: models::CreateOrderRequest,
}

/// One line on the receipt the host shows after placing an order.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct ReceiptLineView {
    pub name: String,
    pub qty: i64,
    pub line_total_minor: i64,
}

/// The order confirmation / receipt summary.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct ReceiptView {
    /// Client-generated order id (the outbox idempotency key). The server id
    /// lands later via sync; this identifies the order locally meanwhile.
    pub local_order_id: String,
    pub lines: Vec<ReceiptLineView>,
    /// Localized payment-method label for display.
    pub payment_label: String,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub total_minor: i64,
    pub amount_tendered_minor: i64,
    pub change_minor: i64,
    pub is_cash: bool,
    /// `true` when the order is still queued (offline); `false` once it's been
    /// sent to the server. The host hints "saved — will sync" vs "sent".
    pub queued_offline: bool,
    pub created_at: String,
}

/// Everything the FFI needs to commit a checkout: the queued command + the
/// receipt. The FFI enqueues `command`, clears the cart, drains, then flips
/// `receipt.queued_offline` based on whether it actually went out.
#[derive(Debug)]
pub(crate) struct Prepared {
    pub order_id: uuid::Uuid,
    pub command: CheckoutCommand,
    pub receipt: ReceiptView,
    pub event_at: String,
}

/// Assemble an order from the current cart. Store reads only (no network).
/// Errors if the cart is empty, the payment method is unknown, or the
/// branch/shift ids are malformed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare(
    store: &Store,
    locale: &str,
    branch_id: &str,
    shift_id: &str,
    payment_method_id: &str,
    amount_tendered_minor: i64,
    tax_rate: f64,
    now_rfc3339: String,
) -> CoreResult<Prepared> {
    let lines = cart::lines(store)?;
    if lines.is_empty() {
        return Err(CoreError::Validation { field: "cart".into(), message: "cart is empty".into() });
    }

    let branch_uuid = parse_uuid(branch_id, "branch_id")?;
    let shift_uuid = parse_uuid(shift_id, "shift_id")?;

    // The wire wants the raw `name` column (the backend validates against it),
    // NOT the localized label — resolve from the cached payment-method catalog.
    let raw = raw_payment_method(store, payment_method_id)?.ok_or_else(|| CoreError::Validation {
        field: "payment_method".into(),
        message: "unknown payment method".into(),
    })?;
    let payment_method = raw.name.clone();
    let is_cash = raw.is_cash;
    let payment_label = display_label(store, locale, payment_method_id).unwrap_or_else(|| raw.name.clone());

    // Price through the engine (the money source of truth). Cash carries the
    // tender + change; non-cash records neither.
    let tendered = if is_cash { Some(amount_tendered_minor) } else { None };
    let priced = pricing::price_cart(PriceCartInput {
        lines: lines
            .iter()
            .map(|l| pricing::CartLine {
                quantity: l.qty,
                unit_price: l.unit_price_minor,
                is_bundle: false,
                addons: vec![],
                optionals: vec![],
                bundle_components: vec![],
            })
            .collect(),
        discount_kind: DiscountKind::None,
        discount_value: 0,
        tax_rate,
        amount_tendered: tendered,
        cash_tip: 0,
    });

    let order_id = uuid::Uuid::new_v4();

    let items: Vec<models::OrderItemInput> = lines
        .iter()
        .map(|l| {
            let mut item = models::OrderItemInput::new(vec![], vec![], l.qty as i32);
            // Cart item_ids are menu-item UUIDs; record the charged unit price so
            // the DB equals the receipt even on a stale/offline price.
            item.menu_item_id = uuid::Uuid::parse_str(&l.item_id).ok().map(Some);
            item.unit_price = Some(Some(l.unit_price_minor as i32));
            item
        })
        .collect();

    let mut request = models::CreateOrderRequest::new(branch_uuid, items, payment_method, shift_uuid);
    request.subtotal = Some(Some(priced.subtotal_minor as i32));
    request.tax_amount = Some(Some(priced.tax_minor as i32));
    request.total_amount = Some(Some(priced.total_minor as i32));
    request.created_at = chrono::DateTime::parse_from_rfc3339(&now_rfc3339).ok().map(Some);
    if is_cash {
        request.amount_tendered = Some(Some(amount_tendered_minor as i32));
        request.change_given = Some(Some(priced.change_given_minor as i32));
    }

    let receipt = ReceiptView {
        local_order_id: order_id.to_string(),
        lines: lines
            .iter()
            .map(|l| ReceiptLineView { name: l.name.clone(), qty: l.qty, line_total_minor: l.line_total_minor })
            .collect(),
        payment_label,
        subtotal_minor: priced.subtotal_minor,
        tax_minor: priced.tax_minor,
        total_minor: priced.total_minor,
        amount_tendered_minor: if is_cash { amount_tendered_minor } else { 0 },
        change_minor: priced.change_given_minor,
        is_cash,
        queued_offline: true, // the FFI flips this to false if the drain sends it now
        created_at: now_rfc3339.clone(),
    };

    Ok(Prepared { order_id, command: CheckoutCommand { request }, receipt, event_at: now_rfc3339 })
}

fn parse_uuid(s: &str, field: &str) -> CoreResult<uuid::Uuid> {
    uuid::Uuid::parse_str(s).map_err(|_| CoreError::Validation { field: field.into(), message: "bad uuid".into() })
}

/// The raw cached payment method (carries the untranslated `name` + `is_cash`).
fn raw_payment_method(store: &Store, id: &str) -> CoreResult<Option<models::OrgPaymentMethod>> {
    let list: Vec<models::OrgPaymentMethod> = match store.kv_get(menu::K_PAYMENT_METHODS)? {
        Some(j) => serde_json::from_str(&j).unwrap_or_default(),
        None => Vec::new(),
    };
    Ok(list.into_iter().find(|p| p.id.to_string() == id))
}

/// The localized label for the receipt (falls back to the raw name).
fn display_label(store: &Store, locale: &str, id: &str) -> Option<String> {
    menu::payment_methods(store, locale)
        .ok()?
        .into_iter()
        .find(|p| p.id.as_str() == id)
        .map(|p| p.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BRANCH: &str = "00000000-0000-0000-0000-0000000000b0";
    const SHIFT: &str = "00000000-0000-0000-0000-0000000000c0";
    const ITEM: &str = "00000000-0000-0000-0000-0000000000a1";
    const CASH: &str = "00000000-0000-0000-0000-0000000000e1";
    const CARD: &str = "00000000-0000-0000-0000-0000000000e2";

    fn seed_methods(store: &Store) {
        store
            .kv_put(
                menu::K_PAYMENT_METHODS,
                r##"[
                  {"color":"#000","created_at":"2026-06-19T10:00:00Z","updated_at":"2026-06-19T10:00:00Z","icon":"cash","id":"00000000-0000-0000-0000-0000000000e1","is_active":true,"is_cash":true,"name":"Cash","org_id":"00000000-0000-0000-0000-0000000000ff","label_translations":{"ar":"نقدي"}},
                  {"color":"#111","created_at":"2026-06-19T10:00:00Z","updated_at":"2026-06-19T10:00:00Z","icon":"card","id":"00000000-0000-0000-0000-0000000000e2","is_active":true,"is_cash":false,"name":"Card","org_id":"00000000-0000-0000-0000-0000000000ff","label_translations":null}
                ]"##,
            )
            .unwrap();
    }

    fn prep(store: &Store, method: &str, tendered: i64) -> CoreResult<Prepared> {
        prepare(store, "en", BRANCH, SHIFT, method, tendered, 0.14, "2026-06-20T12:00:00+00:00".into())
    }

    #[test]
    fn cash_order_assembles_priced_with_change() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        cart::add(&store, ITEM, "Latte", 1000).unwrap();
        cart::set_qty(&store, ITEM, 2).unwrap(); // 2 × 1000 = 2000

        let p = prep(&store, CASH, 5000).unwrap();
        let r = &p.command.request;
        assert_eq!(r.payment_method, "Cash");
        assert_eq!(r.branch_id.to_string(), BRANCH);
        assert_eq!(r.shift_id.to_string(), SHIFT);
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].quantity, 2);
        assert_eq!(r.items[0].menu_item_id, Some(Some(uuid::Uuid::parse_str(ITEM).unwrap())));
        assert_eq!(r.items[0].unit_price, Some(Some(1000)));
        assert_eq!(r.subtotal, Some(Some(2000)));
        assert_eq!(r.tax_amount, Some(Some(280))); // round(2000 * 0.14)
        assert_eq!(r.total_amount, Some(Some(2280)));
        assert_eq!(r.amount_tendered, Some(Some(5000)));
        assert_eq!(r.change_given, Some(Some(2720))); // 5000 - 2280

        let rc = &p.receipt;
        assert_eq!(rc.payment_label, "Cash");
        assert_eq!(rc.total_minor, 2280);
        assert_eq!(rc.change_minor, 2720);
        assert!(rc.is_cash);
        assert_eq!(rc.lines.len(), 1);
        assert_eq!(rc.lines[0].qty, 2);
        assert_eq!(rc.lines[0].line_total_minor, 2000);
    }

    #[test]
    fn card_order_records_no_tender_or_change() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        cart::add(&store, ITEM, "Latte", 1500).unwrap();

        let p = prep(&store, CARD, 9999).unwrap(); // tendered ignored for non-cash
        let r = &p.command.request;
        assert_eq!(r.payment_method, "Card");
        assert_eq!(r.amount_tendered, None);
        assert_eq!(r.change_given, None);
        assert!(!p.receipt.is_cash);
        assert_eq!(p.receipt.amount_tendered_minor, 0);
        assert_eq!(p.receipt.change_minor, 0);
    }

    #[test]
    fn localized_label_resolves_but_wire_uses_raw_name() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        cart::add(&store, ITEM, "Latte", 1000).unwrap();

        let p = prepare(&store, "ar", BRANCH, SHIFT, CASH, 2000, 0.0, "2026-06-20T12:00:00+00:00".into()).unwrap();
        assert_eq!(p.command.request.payment_method, "Cash"); // raw name on the wire
        assert_eq!(p.receipt.payment_label, "نقدي"); // localized on the receipt
    }

    #[test]
    fn empty_cart_is_rejected() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        let err = prep(&store, CASH, 1000).unwrap_err();
        assert!(matches!(err, CoreError::Validation { .. }));
    }

    #[test]
    fn unknown_payment_method_is_rejected() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        cart::add(&store, ITEM, "Latte", 1000).unwrap();
        let err = prep(&store, "00000000-0000-0000-0000-0000000000ee", 1000).unwrap_err();
        assert!(matches!(err, CoreError::Validation { .. }));
    }
}
