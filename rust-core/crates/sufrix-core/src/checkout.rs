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

/// A priced modifier on a receipt line (an addon or a chosen optional). The
/// layout prints `+ name` and, when `price_minor > 0`, the charge. Mirrors the
/// Flutter receipt's addon/optional rows.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct ReceiptModifierView {
    pub name: String,
    pub price_minor: i64,
}

/// One component of a bundle line on the receipt, with its own modifiers —
/// printed indented under the bundle header (Flutter `bundleComponents`).
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct ReceiptComponentView {
    pub name: String,
    pub size_label: Option<String>,
    pub addons: Vec<ReceiptModifierView>,
    pub optionals: Vec<ReceiptModifierView>,
}

/// One line on the receipt the host shows after placing an order. Carries the
/// full modifier/bundle breakdown so the printed receipt matches Flutter's
/// `printer_service.dart` item block exactly.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct ReceiptLineView {
    pub name: String,
    pub qty: i64,
    /// Size variant ("(Large)"), printed inline after the name when present.
    pub size_label: Option<String>,
    pub line_total_minor: i64,
    /// A bundle/combo line — its breakdown is in `components`, not `addons`.
    pub is_bundle: bool,
    pub addons: Vec<ReceiptModifierView>,
    pub optionals: Vec<ReceiptModifierView>,
    pub components: Vec<ReceiptComponentView>,
}

/// The order confirmation / receipt summary.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct ReceiptView {
    /// Client-generated order id (the outbox idempotency key). The server id
    /// lands later via sync; this identifies the order locally meanwhile.
    pub local_order_id: String,
    /// Human order number (server-assigned). `None` for a freshly-queued sale
    /// that hasn't synced — the layout falls back to the short local id then.
    pub order_number: Option<i64>,
    /// Cross-channel order reference (e.g. delivery ticket id), printed when set.
    pub order_ref: Option<String>,
    /// `true` when the order is voided — prints a `*** VOIDED ***` stamp.
    pub is_voided: bool,
    pub lines: Vec<ReceiptLineView>,
    /// Localized payment-method label for display.
    pub payment_label: String,
    pub subtotal_minor: i64,
    /// Discount applied before tax (0 when none). Shown on the printed receipt.
    pub discount_minor: i64,
    pub tax_minor: i64,
    /// Delivery fee (0 for dine-in). Adds a line and forces a subtotal row.
    pub delivery_fee_minor: i64,
    pub total_minor: i64,
    /// Gratuity added on top of the total (0 when none).
    pub tip_minor: i64,
    pub amount_tendered_minor: i64,
    pub change_minor: i64,
    pub is_cash: bool,
    /// Customer name (dine-in pickup or delivery); printed when present.
    pub customer_name: Option<String>,
    /// Teller who rang the sale; printed in the footer when present.
    pub teller_name: Option<String>,
    /// Delivery block — populated only for delivery orders. When `is_delivery`,
    /// the header prints a `*** DELIVERY — {channel} ***` flag and the address
    /// block prints between the meta and item sections.
    pub is_delivery: bool,
    pub delivery_channel: Option<String>,
    pub customer_phone: Option<String>,
    pub delivery_address: Option<String>,
    pub delivery_zone: Option<String>,
    pub delivery_ref: Option<String>,
    pub payment_hint: Option<String>,
    pub delivery_notes: Option<String>,
    /// `true` when the order is still queued (offline); `false` once it's been
    /// sent to the server. The host hints "saved — will sync" vs "sent".
    pub queued_offline: bool,
    pub created_at: String,
}

/// One leg of a split payment (a method + the amount paid on it).
#[derive(uniffi::Record, Clone, Debug)]
pub struct CheckoutSplit {
    pub payment_method_id: String,
    pub amount_minor: i64,
}

/// Everything the tender screen collects for a checkout.
#[derive(uniffi::Record, Clone, Debug)]
pub struct CheckoutInput {
    /// The (primary) payment method id.
    pub payment_method_id: String,
    /// Cash handed over (for change); 0 / ignored for non-cash.
    pub amount_tendered_minor: i64,
    /// Gratuity on top of the total (0 = none).
    pub tip_minor: i64,
    /// Which method the tip is paid on (defaults to the order method).
    pub tip_payment_method_id: Option<String>,
    pub customer_name: Option<String>,
    pub notes: Option<String>,
    /// Per-method split legs (empty = single payment).
    pub splits: Vec<CheckoutSplit>,
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
pub(crate) fn prepare(
    store: &Store,
    locale: &str,
    branch_id: &str,
    shift_id: &str,
    input: &CheckoutInput,
    tax_rate: f64,
    now_rfc3339: String,
) -> CoreResult<Prepared> {
    let payment_method_id = &input.payment_method_id;
    let amount_tendered_minor = input.amount_tendered_minor;
    let lines = cart::lines(store)?;
    if lines.is_empty() {
        return Err(CoreError::Validation { field: "cart".into(), detail: "cart is empty".into() });
    }

    let branch_uuid = parse_uuid(branch_id, "branch_id")?;
    let shift_uuid = parse_uuid(shift_id, "shift_id")?;

    // The wire wants the raw `name` column (the backend validates against it),
    // NOT the localized label — resolve from the cached payment-method catalog.
    let raw = raw_payment_method(store, payment_method_id)?.ok_or_else(|| CoreError::Validation {
        field: "payment_method".into(),
        detail: "unknown payment method".into(),
    })?;
    let payment_method = raw.name.clone();
    let is_cash = raw.is_cash;
    let payment_label = display_label(store, locale, payment_method_id).unwrap_or_else(|| raw.name.clone());

    // A tip reduces change only when it's paid IN CASH (default = the order
    // method). A card tip leaves the cash drawer untouched.
    let tip_minor = input.tip_minor.max(0);
    let tip_is_cash = match &input.tip_payment_method_id {
        Some(id) => raw_payment_method(store, id)?.map(|p| p.is_cash).unwrap_or(is_cash),
        None => is_cash,
    };
    let cash_tip = if tip_is_cash { tip_minor } else { 0 };

    // Price through the engine (the money source of truth). Cash carries the
    // tender + change; non-cash records neither. The cart's discount applies
    // before tax (the engine clamps it).
    let (discount_kind, discount_value) = cart::discount(store)?;
    let tendered = if is_cash { Some(amount_tendered_minor) } else { None };
    let priced = pricing::price_cart(PriceCartInput {
        lines: lines
            .iter()
            .map(|l| pricing::CartLine {
                quantity: l.qty,
                unit_price: l.unit_price_minor,
                is_bundle: l.bundle_id.is_some(),
                addons: l
                    .addons
                    .iter()
                    .map(|a| pricing::AddonSel { price_modifier: a.price_modifier_minor, quantity: a.qty })
                    .collect(),
                optionals: l.optionals.iter().map(|o| pricing::OptionalSel { price: o.price_minor }).collect(),
                bundle_components: l
                    .bundle_components
                    .iter()
                    .map(|c| pricing::BundleComponentSel {
                        addons: c
                            .addons
                            .iter()
                            .map(|a| pricing::AddonSel { price_modifier: a.price_modifier_minor, quantity: a.qty })
                            .collect(),
                        optionals: c.optionals.iter().map(|o| pricing::OptionalSel { price: o.price_minor }).collect(),
                    })
                    .collect(),
            })
            .collect(),
        discount_kind,
        discount_value,
        tax_rate,
        amount_tendered: tendered,
        cash_tip,
    });

    let order_id = uuid::Uuid::new_v4();

    let items: Vec<models::OrderItemInput> = lines
        .iter()
        .map(|l| {
            // A bundle line carries its config in `bundle_components`; its own
            // top-level addons/optionals stay empty (Flutter parity).
            if let Some(bid) = &l.bundle_id {
                let comps: Vec<models::BundleComponentInput> = l
                    .bundle_components
                    .iter()
                    .filter_map(|c| {
                        let item_id = uuid::Uuid::parse_str(&c.item_id).ok()?;
                        let mut ci = models::BundleComponentInput::new(item_id, c.qty as i32);
                        let addons = component_addons(&c.addons);
                        if !addons.is_empty() {
                            ci.addons = Some(addons);
                        }
                        let opt_ids: Vec<uuid::Uuid> = c
                            .optionals
                            .iter()
                            .filter_map(|o| uuid::Uuid::parse_str(&o.optional_field_id).ok())
                            .collect();
                        if !opt_ids.is_empty() {
                            ci.optional_field_ids = Some(opt_ids);
                        }
                        ci.size_label = Some(c.size_label.clone());
                        Some(ci)
                    })
                    .collect();
                let mut item = models::OrderItemInput::new(vec![], vec![], l.qty as i32);
                item.bundle_id = uuid::Uuid::parse_str(bid).ok().map(Some);
                item.bundle_components = Some(comps);
                item.unit_price = Some(Some(l.unit_price_minor as i32));
                return item;
            }
            // Addons carry their CHARGED unit price (swap delta / extra) verbatim.
            let addons = component_addons(&l.addons);
            let optional_ids: Vec<uuid::Uuid> = l
                .optionals
                .iter()
                .filter_map(|o| uuid::Uuid::parse_str(&o.optional_field_id).ok())
                .collect();
            let mut item = models::OrderItemInput::new(addons, optional_ids, l.qty as i32);
            // Cart item_ids are menu-item UUIDs; record the size + charged unit
            // price so the DB equals the receipt even on a stale/offline price.
            item.menu_item_id = uuid::Uuid::parse_str(&l.item_id).ok().map(Some);
            item.size_label = Some(l.size_label.clone());
            item.unit_price = Some(Some(l.unit_price_minor as i32));
            item
        })
        .collect();

    let mut request = models::CreateOrderRequest::new(branch_uuid, items, payment_method.clone(), shift_uuid);
    request.subtotal = Some(Some(priced.subtotal_minor as i32));
    request.tax_amount = Some(Some(priced.tax_minor as i32));
    request.total_amount = Some(Some(priced.total_minor as i32));
    request.created_at = chrono::DateTime::parse_from_rfc3339(&now_rfc3339).ok().map(Some);
    if is_cash {
        request.amount_tendered = Some(Some(amount_tendered_minor as i32));
        request.change_given = Some(Some(priced.change_given_minor as i32));
    }
    // Tip, customer, notes (the backend prices the tip separately from the total).
    if tip_minor > 0 {
        request.tip_amount = Some(Some(tip_minor as i32));
        let tip_method = input
            .tip_payment_method_id
            .as_ref()
            .and_then(|id| raw_payment_method(store, id).ok().flatten())
            .map(|p| p.name)
            .unwrap_or_else(|| payment_method.clone());
        request.tip_payment_method = Some(Some(tip_method));
    }
    request.customer_name = input.customer_name.clone().filter(|s| !s.trim().is_empty()).map(Some);
    request.notes = input.notes.clone().filter(|s| !s.trim().is_empty()).map(Some);
    // Split payments: resolve each leg's method to its raw name.
    if !input.splits.is_empty() {
        let legs: Vec<models::PaymentSplitInput> = input
            .splits
            .iter()
            .filter_map(|s| {
                let name = raw_payment_method(store, &s.payment_method_id).ok().flatten()?.name;
                Some(models::PaymentSplitInput { amount: s.amount_minor as i32, method: name, reference: None })
            })
            .collect();
        if !legs.is_empty() {
            request.payment_splits = Some(Some(legs));
        }
    }
    // Record the applied discount verbatim (the engine already clamped it).
    if discount_kind != DiscountKind::None {
        let dtype = match discount_kind {
            DiscountKind::Percentage => "percentage",
            DiscountKind::Fixed => "fixed",
            DiscountKind::None => "",
        };
        request.discount_id = cart::discount_id(store)?
            .and_then(|id| uuid::Uuid::parse_str(&id).ok())
            .map(Some);
        request.discount_type = Some(Some(dtype.into()));
        request.discount_value = Some(Some(discount_value as i32));
        request.discount_amount = Some(Some(priced.discount_minor as i32));
    }

    let receipt = ReceiptView {
        local_order_id: order_id.to_string(),
        order_number: None, // server-assigned on sync
        order_ref: None,
        is_voided: false,
        lines: lines.iter().map(receipt_line_from_cart).collect(),
        payment_label,
        subtotal_minor: priced.subtotal_minor,
        discount_minor: priced.discount_minor,
        tax_minor: priced.tax_minor,
        delivery_fee_minor: 0,
        total_minor: priced.total_minor,
        tip_minor,
        amount_tendered_minor: if is_cash { amount_tendered_minor } else { 0 },
        change_minor: priced.change_given_minor,
        is_cash,
        customer_name: input.customer_name.clone().filter(|s| !s.trim().is_empty()),
        teller_name: None, // the FFI fills this from the session
        is_delivery: false,
        delivery_channel: None,
        customer_phone: None,
        delivery_address: None,
        delivery_zone: None,
        delivery_ref: None,
        payment_hint: None,
        delivery_notes: None,
        queued_offline: true, // the FFI flips this to false if the drain sends it now
        created_at: now_rfc3339.clone(),
    };

    Ok(Prepared { order_id, command: CheckoutCommand { request }, receipt, event_at: now_rfc3339 })
}

/// Project a cart line into its printable receipt line — bundle-aware, carrying
/// the full modifier breakdown so the receipt matches the order.
fn receipt_line_from_cart(l: &cart::CartLineView) -> ReceiptLineView {
    let addons = l
        .addons
        .iter()
        .map(|a| ReceiptModifierView {
            name: if a.qty > 1 { format!("{} ×{}", a.name, a.qty) } else { a.name.clone() },
            price_minor: a.price_modifier_minor,
        })
        .collect();
    let optionals = l
        .optionals
        .iter()
        .map(|o| ReceiptModifierView { name: o.name.clone(), price_minor: o.price_minor })
        .collect();
    let components = l
        .bundle_components
        .iter()
        .map(|c| ReceiptComponentView {
            name: c.name.clone(),
            size_label: c.size_label.clone().filter(|s| !s.is_empty()),
            addons: c
                .addons
                .iter()
                .map(|a| ReceiptModifierView {
                    name: if a.qty > 1 { format!("{} ×{}", a.name, a.qty) } else { a.name.clone() },
                    price_minor: a.price_modifier_minor,
                })
                .collect(),
            optionals: c
                .optionals
                .iter()
                .map(|o| ReceiptModifierView { name: o.name.clone(), price_minor: o.price_minor })
                .collect(),
        })
        .collect();
    ReceiptLineView {
        name: l.name.clone(),
        qty: l.qty,
        size_label: l.size_label.clone().filter(|s| !s.is_empty()),
        line_total_minor: l.line_total_minor,
        is_bundle: l.bundle_id.is_some(),
        addons,
        optionals,
        components,
    }
}

/// Wire `AddonInput`s from cart addons — the CHARGED unit price recorded
/// verbatim (so an offline order equals the receipt). Shared by normal lines
/// and bundle components.
fn component_addons(addons: &[cart::CartAddonView]) -> Vec<models::AddonInput> {
    addons
        .iter()
        .filter_map(|a| {
            let id = uuid::Uuid::parse_str(&a.addon_item_id).ok()?;
            let mut ai = models::AddonInput::new(id);
            ai.quantity = Some(a.qty as i32);
            ai.unit_price = Some(Some(a.price_modifier_minor as i32));
            Some(ai)
        })
        .collect()
}

fn parse_uuid(s: &str, field: &str) -> CoreResult<uuid::Uuid> {
    uuid::Uuid::parse_str(s).map_err(|_| CoreError::Validation { field: field.into(), detail: "bad uuid".into() })
}

/// Total of still-queued cash sales (outbox `create_order` whose payment method
/// is cash) — added to the shift report's expected cash so the close-shift
/// guidance stays right before those orders sync.
pub(crate) fn queued_cash_total(store: &Store) -> CoreResult<i64> {
    let raw: Vec<models::OrgPaymentMethod> = match store.kv_get(menu::K_PAYMENT_METHODS)? {
        Some(j) => serde_json::from_str(&j).unwrap_or_default(),
        None => Vec::new(),
    };
    let cash_names: std::collections::HashSet<String> =
        raw.iter().filter(|p| p.is_cash).map(|p| p.name.clone()).collect();
    let mut total = 0i64;
    for item in store.list_active()? {
        if item.op_type != "create_order" {
            continue;
        }
        if let Ok(cmd) = serde_json::from_str::<CheckoutCommand>(&item.payload) {
            if cash_names.contains(&cmd.request.payment_method) {
                total += cmd.request.total_amount.flatten().unwrap_or(0) as i64;
            }
        }
    }
    Ok(total)
}

/// The raw cached payment method (carries the untranslated `name` + `is_cash`).
pub(crate) fn raw_payment_method(store: &Store, id: &str) -> CoreResult<Option<models::OrgPaymentMethod>> {
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

    fn mk_input(method: &str, tendered: i64) -> CheckoutInput {
        CheckoutInput {
            payment_method_id: method.into(),
            amount_tendered_minor: tendered,
            tip_minor: 0,
            tip_payment_method_id: None,
            customer_name: None,
            notes: None,
            splits: vec![],
        }
    }

    fn prep(store: &Store, method: &str, tendered: i64) -> CoreResult<Prepared> {
        prepare(store, "en", BRANCH, SHIFT, &mk_input(method, tendered), 0.14, "2026-06-20T12:00:00+00:00".into())
    }

    #[test]
    fn cash_tip_reduces_change_and_records_tip_customer_notes() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        cart::add(&store, ITEM, "Latte", 1000).unwrap();
        // total = 1000 + 14% tax = 1140; tendered 1500; cash tip 200 → change 160.
        let mut input = mk_input(CASH, 1500);
        input.tip_minor = 200;
        input.customer_name = Some("Sara".into());
        input.notes = Some("no sugar".into());
        let p = prepare(&store, "en", BRANCH, SHIFT, &input, 0.14, "2026-06-20T12:00:00+00:00".into()).unwrap();
        assert_eq!(p.receipt.tip_minor, 200);
        assert_eq!(p.receipt.change_minor, 160); // 1500 - 1140 - 200
        let r = &p.command.request;
        assert_eq!(r.tip_amount, Some(Some(200)));
        assert_eq!(r.tip_payment_method, Some(Some("Cash".into()))); // defaults to order method
        assert_eq!(r.customer_name, Some(Some("Sara".into())));
        assert_eq!(r.notes, Some(Some("no sugar".into())));
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

        let p = prepare(&store, "ar", BRANCH, SHIFT, &mk_input(CASH, 2000), 0.0, "2026-06-20T12:00:00+00:00".into()).unwrap();
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

    #[test]
    fn queued_cash_total_sums_only_cash_orders() {
        let store = Store::open("").unwrap();
        seed_methods(&store); // "Cash" is_cash=true, "Card" is_cash=false
        let queue = |id: &str, method: &str, total: i32| {
            let mut req = models::CreateOrderRequest::new(
                uuid::Uuid::new_v4(), vec![], method.into(), uuid::Uuid::new_v4());
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
        };
        assert_eq!(queued_cash_total(&store).unwrap(), 0);
        queue("o1", "Cash", 2280);
        queue("o2", "Card", 1500); // not cash → excluded
        queue("o3", "Cash", 1000);
        assert_eq!(queued_cash_total(&store).unwrap(), 3280);
    }
}
