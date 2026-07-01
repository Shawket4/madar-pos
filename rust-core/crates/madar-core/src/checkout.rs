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
use madar_api::models;

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
// Branch metadata (code + IANA timezone) cached at login from `get_branch`, and
// this device's MANAGED code, so an OFFLINE checkout can MINT the exact order
// number/ref the server will store — from first boot, with no synced order needed.
pub(crate) const KEY_BRANCH_CODE: &str = "branch_code";
pub(crate) const KEY_BRANCH_TZ: &str = "branch_tz";
pub(crate) const KEY_DEVICE_CODE: &str = "device_code";
/// The org's logo URL for this branch — fetched + persisted alongside the branch
/// code/timezone (same `get_branch`, same kv store), so the receipt logo survives
/// app restarts + long offline stretches and refreshes on a manual data sync,
/// instead of living only in the host's volatile prefs from a one-time branch bind.
pub(crate) const KEY_ORG_LOGO_URL: &str = "org_logo_url";

/// Blob-cache key for the org logo's image BYTES (fetched from `KEY_ORG_LOGO_URL`
/// whenever online, in the same `get_branch` flow), so the receipt rasterizer can
/// composite the logo OFFLINE — the print path never touches the network.
pub(crate) const KEY_ORG_LOGO_PNG: &str = "org_logo_png";

/// Per-shift kv key holding the highest order_number this device has CONFIRMED is
/// on the server for the shift (the "synced base"). Seeded at login/adopt from the
/// shift's server orders and advanced on every create_order ack. The predicted
/// `order_number` is `base + (still-queued for the shift) + 1`, so it equals the
/// server's `MAX(order_number)+1` whether the order syncs immediately (online: base
/// already counts the prior orders, queued≈0) or sits in the outbox (offline: queued
/// grows). Without the base, an online receipt always predicted `#1` (queued drains
/// instantly) while the server stored the real number — the online-only mismatch.
pub(crate) fn order_base_key(shift_id: &str) -> String {
    format!("order_base:{shift_id}")
}

/// The synced base for a shift (0 when none recorded yet).
pub(crate) fn order_base(store: &Store, shift_id: &str) -> i64 {
    store
        .kv_get(&order_base_key(shift_id))
        .ok()
        .flatten()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0)
}

/// Raise the synced base to at least `n` (monotonic; a stale/out-of-order ack never
/// lowers it). Called when the server confirms an order's number on ack.
pub(crate) fn bump_order_base(store: &Store, shift_id: &str, n: i64) {
    if n > order_base(store, shift_id) {
        let _ = store.kv_put(&order_base_key(shift_id), &n.to_string());
    }
}

/// This device's short code (the `<DEVICE>` segment of every order_ref). A manager
/// sets it (T1/W2/K1) in Settings; until then a stable random 3-char code is
/// auto-assigned on first use so two devices never share a default. Persisted, so
/// every order this device ever mints carries the same tag.
pub(crate) fn device_code_or_default(store: &Store) -> String {
    if let Ok(Some(c)) = store.kv_get(KEY_DEVICE_CODE) {
        if !c.is_empty() {
            return c;
        }
    }
    let gen = uuid::Uuid::new_v4().simple().to_string()[..3].to_uppercase();
    let _ = store.kv_put(KEY_DEVICE_CODE, &gen);
    gen
}

/// Mint this order's display number + CLIENT-AUTHORITATIVE ref:
/// - `order_number` is PREDICTED per-shift (the order's position in its shift) to
///   match the server's `MAX(order_number)+1` exactly — one device numbers a shift,
///   so the offline receipt's `#N` equals the synced one. It is NOT sent (the
///   server owns the `UNIQUE(shift_id, order_number)` column).
/// - `order_ref` = `<BRANCH>-<YYMMDD>-<DEVICE>-<RRRR>`, where RRRR is this device's
///   monotonic per-business-day sequence (INDEPENDENT of order_number, so it stays
///   globally unique across shifts and devices with no shared counter). This IS
///   sent and stored verbatim, so the ref is byte-identical at ring-up and reprint.
///
/// Returns None only before the first ONLINE login has cached the branch code
/// (impossible offline — setup is always online once); the receipt then shows the
/// local id and the server mints its deterministic fallback.
pub(crate) fn mint_order_ref(store: &Store, shift_id: &str, now_rfc3339: &str) -> Option<(i64, String)> {
    let branch_code = store.kv_get(KEY_BRANCH_CODE).ok().flatten().filter(|s| !s.is_empty())?;
    let tz: chrono_tz::Tz = store.kv_get(KEY_BRANCH_TZ).ok().flatten()?.parse().ok()?;
    let device = device_code_or_default(store);
    let created = chrono::DateTime::parse_from_rfc3339(now_rfc3339).ok()?.with_timezone(&tz);
    let yymmdd = created.format("%y%m%d").to_string();
    // Per-shift display number = synced base + (still-queued for this shift) + 1,
    // which equals the server's MAX(order_number)+1 both online (base counts the
    // already-synced orders, queued≈0) and offline (base is frozen, queued grows).
    let order_number = order_base(store, shift_id) + crate::orders::queued(store, shift_id).ok()?.len() as i64 + 1;
    // Per-(device, business-day) ref sequence — its own counter, NOT order_number.
    let ref_key = format!("ref_seq:{yymmdd}");
    let ref_seq = store.kv_get(&ref_key).ok().flatten().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0) + 1;
    let _ = store.kv_put(&ref_key, &ref_seq.to_string());
    Some((order_number, format!("{branch_code}-{yymmdd}-{device}-{ref_seq:04}")))
}

/// Map priced cart lines to the wire `OrderItemInput`s the backend records
/// VERBATIM (client-authoritative pricing). Shared by the POS checkout AND the
/// waiter ticket-fire path (a fired round is the same cart, minus payment), so a
/// ticket settles into a byte-identical order. A bundle line carries its config in
/// `bundle_components` with empty top-level addons/optionals (Flutter parity); a
/// plain line records its menu-item id, size, charged unit price, addons + optionals.
pub(crate) fn lines_to_wire_items(lines: &[cart::CartLineView]) -> Vec<models::OrderItemInput> {
    lines
        .iter()
        .map(|l| {
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
                let mut item = models::OrderItemInput::new(l.qty as i32);
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
            let mut item = models::OrderItemInput::new(l.qty as i32);
            item.addons = Some(addons);
            item.optional_field_ids = Some(optional_ids);
            item.menu_item_id = uuid::Uuid::parse_str(&l.item_id).ok().map(Some);
            item.size_label = Some(l.size_label.clone());
            item.unit_price = Some(Some(l.unit_price_minor as i32));
            item
        })
        .collect()
}

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

    let items = lines_to_wire_items(&lines);

    let mut request = models::CreateOrderRequest::new(branch_uuid, items, payment_method.clone(), shift_uuid);
    // Exactly-once: the in-body idempotency key IS the client order id. It rides
    // inside the persisted outbox payload, so a replay after a lost response —
    // even months later — dedups against `orders.idempotency_key` server-side.
    request.idempotency_key = Some(Some(order_id));
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

    // Mint the per-shift display number (predicted, NOT sent) + the client-
    // authoritative order_ref (SENT, stored verbatim) so the offline ring-up receipt
    // is byte-identical to the synced reprint.
    let (mint_number, mint_ref) = match mint_order_ref(store, shift_id, &now_rfc3339) {
        Some((n, r)) => (Some(n), Some(r)),
        None => (None, None),
    };
    request.order_ref = mint_ref.clone().map(Some);
    let receipt = ReceiptView {
        local_order_id: order_id.to_string(),
        order_number: mint_number,
        order_ref: mint_ref,
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

/// Cash physically in the drawer for ONE queued order: the cash leg(s) of the
/// sale PLUS a cash tip. A SPLIT order contributes only its cash legs (not the
/// whole total — the card/wallet legs aren't drawer cash); a single-method order
/// contributes its total only when that method is cash. The backend prices the
/// tip separately from `total_amount`, so a CASH tip (its own method, or the
/// order's method when none was chosen) is added on top.
fn order_cash_in_drawer(
    req: &models::CreateOrderRequest,
    cash_names: &std::collections::HashSet<String>,
) -> i64 {
    let sale_cash: i64 = match req.payment_splits.as_ref().and_then(|s| s.as_ref()) {
        Some(legs) if !legs.is_empty() => legs
            .iter()
            .filter(|l| cash_names.contains(&l.method))
            .map(|l| l.amount as i64)
            .sum(),
        _ if cash_names.contains(&req.payment_method) => req.total_amount.flatten().unwrap_or(0) as i64,
        _ => 0,
    };
    let tip = req.tip_amount.flatten().unwrap_or(0) as i64;
    let tip_cash = if tip > 0 {
        let tip_method = req
            .tip_payment_method
            .as_ref()
            .and_then(|m| m.as_ref())
            .map(|s| s.as_str())
            .unwrap_or(req.payment_method.as_str());
        if cash_names.contains(tip_method) { tip } else { 0 }
    } else {
        0
    };
    sale_cash + tip_cash
}

/// All-shifts aggregate of still-queued cash (cash legs + cash tips + movements).
/// Production scopes per-shift via [`queued_cash_total_for`] (a prior shift's
/// undrained cash must not inflate the current drawer); this unscoped roll-up is
/// retained to exercise the shared [`order_cash_in_drawer`] aggregation.
#[cfg(test)]
pub(crate) fn queued_cash_total(store: &Store) -> CoreResult<i64> {
    let raw: Vec<models::OrgPaymentMethod> = match store.kv_get(menu::K_PAYMENT_METHODS)? {
        Some(j) => serde_json::from_str(&j).unwrap_or_default(),
        None => Vec::new(),
    };
    let cash_names: std::collections::HashSet<String> =
        raw.iter().filter(|p| p.is_cash).map(|p| p.name.clone()).collect();
    let mut total = 0i64;
    for item in store.list_active()? {
        // `inflight` = already sent to the server, so a freshly-fetched shift report
        // already reflects it. Counting it here too would DOUBLE it in the drawer
        // during the lost-response window. `pending`/`dead` cash is in the drawer
        // but NOT on the server, so it's still added.
        if item.status == "inflight" {
            continue;
        }
        match item.op_type.as_str() {
            // A still-queued cash sale: the cash leg(s) + any cash tip are in the drawer.
            "create_order" => {
                if let Ok(cmd) = serde_json::from_str::<CheckoutCommand>(&item.payload) {
                    total += order_cash_in_drawer(&cmd.request, &cash_names);
                }
            }
            // A queued pay-in/pay-out (signed): the drawer already moved.
            "cash_movement" => {
                if let Ok(cmd) = serde_json::from_str::<crate::shift::CashMovementCommand>(&item.payload) {
                    total += cmd.request.amount as i64;
                }
            }
            _ => {}
        }
    }
    Ok(total)
}

/// Like [`queued_cash_total`] but scoped to ONE shift — for reconstructing a past
/// OFFLINE shift's Z-report (the drawer holds that shift's queued cash sales +
/// movements). Matches the outbox row's `shift_id`.
pub(crate) fn queued_cash_total_for(store: &Store, shift_id: &str) -> CoreResult<i64> {
    let raw: Vec<models::OrgPaymentMethod> = match store.kv_get(menu::K_PAYMENT_METHODS)? {
        Some(j) => serde_json::from_str(&j).unwrap_or_default(),
        None => Vec::new(),
    };
    let cash_names: std::collections::HashSet<String> =
        raw.iter().filter(|p| p.is_cash).map(|p| p.name.clone()).collect();
    let mut total = 0i64;
    for item in store.list_active()? {
        if item.shift_id.as_deref() != Some(shift_id) {
            continue;
        }
        // Inflight = already sent → the synced report counts it; skip to avoid a
        // double in the lost-response window (see queued_cash_total).
        if item.status == "inflight" {
            continue;
        }
        match item.op_type.as_str() {
            "create_order" => {
                if let Ok(cmd) = serde_json::from_str::<CheckoutCommand>(&item.payload) {
                    total += order_cash_in_drawer(&cmd.request, &cash_names);
                }
            }
            "cash_movement" => {
                if let Ok(cmd) = serde_json::from_str::<crate::shift::CashMovementCommand>(&item.payload) {
                    total += cmd.request.amount as i64;
                }
            }
            _ => {}
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
                    ..Default::default()
                })
                .unwrap();
        };
        assert_eq!(queued_cash_total(&store).unwrap(), 0);
        queue("o1", "Cash", 2280);
        queue("o2", "Card", 1500); // not cash → excluded
        queue("o3", "Cash", 1000);
        assert_eq!(queued_cash_total(&store).unwrap(), 3280);
    }

    #[test]
    fn queued_cash_total_counts_only_cash_legs_and_cash_tips() {
        let store = Store::open("").unwrap();
        seed_methods(&store); // "Cash" is_cash=true, "Card" is_cash=false
        let push = |id: &str, req: models::CreateOrderRequest| {
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
        };
        let mk = || models::CreateOrderRequest::new(uuid::Uuid::new_v4(), vec![], "Cash".into(), uuid::Uuid::new_v4());

        // Split: total 1000 = 600 cash + 400 card → only the 600 cash leg is drawer cash.
        let mut split = mk();
        split.total_amount = Some(Some(1000));
        split.payment_splits = Some(Some(vec![
            models::PaymentSplitInput { amount: 600, method: "Cash".into(), reference: None },
            models::PaymentSplitInput { amount: 400, method: "Card".into(), reference: None },
        ]));
        push("split", split);
        assert_eq!(queued_cash_total(&store).unwrap(), 600, "split counts only the cash leg, not the whole total");

        // Cash order 1000 + CASH tip 150 → both in the drawer.
        let mut cash_tip = mk();
        cash_tip.total_amount = Some(Some(1000));
        cash_tip.tip_amount = Some(Some(150));
        cash_tip.tip_payment_method = Some(Some("Cash".into()));
        push("cashtip", cash_tip);

        // Card order 2000 + CASH tip 200 → only the tip is drawer cash.
        let mut card_with_cash_tip = mk();
        card_with_cash_tip.payment_method = "Card".into();
        card_with_cash_tip.total_amount = Some(Some(2000));
        card_with_cash_tip.tip_amount = Some(Some(200));
        card_with_cash_tip.tip_payment_method = Some(Some("Cash".into()));
        push("cardtip", card_with_cash_tip);

        // Cash order 500 + CARD tip 99 → the tip is NOT drawer cash.
        let mut cash_with_card_tip = mk();
        cash_with_card_tip.total_amount = Some(Some(500));
        cash_with_card_tip.tip_amount = Some(Some(99));
        cash_with_card_tip.tip_payment_method = Some(Some("Card".into()));
        push("cashcardtip", cash_with_card_tip);

        // 600 + (1000+150) + 200 + 500 = 2450 (no card sale, no card tip).
        assert_eq!(queued_cash_total(&store).unwrap(), 600 + 1150 + 200 + 500);
    }

    // ── idempotency key ───────────────────────────────────────────────────────

    #[test]
    fn idempotency_key_is_the_local_order_id() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        cart::add(&store, ITEM, "Latte", 1000).unwrap();
        let p = prep(&store, CASH, 2000).unwrap();
        // The in-body idempotency key IS the client order id, and the receipt's
        // local_order_id mirrors it (the outbox keys the row by the same UUID).
        assert_eq!(p.command.request.idempotency_key, Some(Some(p.order_id)));
        assert_eq!(p.receipt.local_order_id, p.order_id.to_string());
    }

    // ── order numbering (synced base + queued prediction) ─────────────────────

    fn seed_numbering_ctx(store: &Store) {
        store.kv_put(KEY_BRANCH_CODE, "B1").unwrap();
        store.kv_put(KEY_BRANCH_TZ, "UTC").unwrap();
        store.kv_put(KEY_DEVICE_CODE, "T1").unwrap();
    }

    fn queue_order_for_shift(store: &Store, id: &str, shift: &str) {
        let req = models::CreateOrderRequest::new(
            uuid::Uuid::new_v4(), vec![], "Cash".into(), uuid::Uuid::parse_str(shift).unwrap());
        let cmd = CheckoutCommand { request: req };
        store
            .enqueue(&crate::store::NewOutboxOp {
                id: id.into(),
                op_type: "create_order".into(),
                idempotency_key: id.into(),
                payload: serde_json::to_string(&cmd).unwrap(),
                event_at: "2026-06-20T12:00:00+00:00".into(),
                shift_id: Some(shift.into()),
                ..Default::default()
            })
            .unwrap();
    }

    #[test]
    fn order_base_defaults_zero_is_monotonic_and_per_shift() {
        let store = Store::open("").unwrap();
        assert_eq!(order_base(&store, SHIFT), 0, "no base recorded yet → 0");
        bump_order_base(&store, SHIFT, 5);
        assert_eq!(order_base(&store, SHIFT), 5);
        bump_order_base(&store, SHIFT, 3); // a stale/out-of-order ack never lowers it
        assert_eq!(order_base(&store, SHIFT), 5, "monotonic — a lower number is ignored");
        bump_order_base(&store, SHIFT, 9);
        assert_eq!(order_base(&store, SHIFT), 9);
        const OTHER: &str = "00000000-0000-0000-0000-0000000000c9";
        assert_eq!(order_base(&store, OTHER), 0, "per-shift isolation — OTHER keeps its own base");
    }

    #[test]
    fn mint_predicts_max_plus_one_online_counting_the_synced_base() {
        // ONLINE: orders ack immediately so `queued` is ~empty; WITHOUT the base the
        // prediction would always be #1 (the online-only mismatch). WITH the base =
        // the server's MAX(order_number), it's the correct MAX+1.
        let store = Store::open("").unwrap();
        seed_numbering_ctx(&store);
        bump_order_base(&store, SHIFT, 7); // the shift already has 7 synced orders
        let (n, r) = mint_order_ref(&store, SHIFT, "2026-06-20T12:00:00+00:00").expect("mint");
        assert_eq!(n, 8, "predicts MAX(order_number)+1, not #1");
        assert_eq!(r, "B1-260620-T1-0001", "ref = BRANCH-YYMMDD-DEVICE-RRRR");
    }

    #[test]
    fn mint_counts_queued_orders_offline() {
        // OFFLINE: the synced base is frozen and the queue grows — the prediction is
        // base + (still-queued for this shift) + 1, matching the server's MAX+1 when
        // these replay in order.
        let store = Store::open("").unwrap();
        seed_numbering_ctx(&store);
        bump_order_base(&store, SHIFT, 3); // 3 synced before going offline
        queue_order_for_shift(&store, "q1", SHIFT);
        queue_order_for_shift(&store, "q2", SHIFT);
        // Another shift's queued order must NOT bleed into this shift's count.
        queue_order_for_shift(&store, "x1", "00000000-0000-0000-0000-0000000000c9");
        let (n, _) = mint_order_ref(&store, SHIFT, "2026-06-20T12:00:00+00:00").expect("mint");
        assert_eq!(n, 6, "3 synced + 2 queued + 1 = the 6th order (other shift excluded)");
    }

    #[test]
    fn ref_seq_is_monotonic_per_day_and_independent_of_order_number() {
        // The order_ref's RRRR is this device's own per-business-day counter, NOT the
        // order_number — so it stays globally unique across shifts/devices with no
        // shared counter.
        let store = Store::open("").unwrap();
        seed_numbering_ctx(&store);
        bump_order_base(&store, SHIFT, 50); // order_number ~51, ref still starts at 0001
        let (n1, r1) = mint_order_ref(&store, SHIFT, "2026-06-20T12:00:00+00:00").expect("mint1");
        let (_n2, r2) = mint_order_ref(&store, SHIFT, "2026-06-20T18:00:00+00:00").expect("mint2");
        assert_eq!(n1, 51);
        assert!(r1.ends_with("-0001"), "first ref of the day = 0001: {r1}");
        assert!(r2.ends_with("-0002"), "second ref = 0002 regardless of order_number: {r2}");
        let (_n3, r3) = mint_order_ref(&store, SHIFT, "2026-06-21T09:00:00+00:00").expect("mint3");
        assert!(r3.ends_with("-0001"), "ref sequence resets per business day: {r3}");
    }

    #[test]
    fn mint_needs_the_branch_code_cached() {
        // Before the first online login caches the branch code, mint returns None
        // (the receipt then shows the local id; the server mints its fallback).
        let store = Store::open("").unwrap();
        store.kv_put(KEY_BRANCH_TZ, "UTC").unwrap(); // tz present, branch code absent
        assert!(mint_order_ref(&store, SHIFT, "2026-06-20T12:00:00+00:00").is_none());
    }

    #[test]
    fn device_code_autogenerates_once_and_persists() {
        // No managed code set → a stable 3-char code is auto-assigned on first use
        // and reused forever after (so every ref this device mints carries it).
        let store = Store::open("").unwrap();
        let first = device_code_or_default(&store);
        assert_eq!(first.len(), 3, "auto code is 3 chars: {first}");
        assert_eq!(device_code_or_default(&store), first, "stable across calls");
        // A manager-set code takes over.
        store.kv_put(KEY_DEVICE_CODE, "W2").unwrap();
        assert_eq!(device_code_or_default(&store), "W2");
    }

    // ── split payments ────────────────────────────────────────────────────────

    #[test]
    fn split_payments_resolve_each_legs_raw_method_name() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        cart::add(&store, ITEM, "Latte", 2000).unwrap();
        let mut input = mk_input(CASH, 0);
        input.splits = vec![
            CheckoutSplit { payment_method_id: CASH.into(), amount_minor: 1000 },
            CheckoutSplit { payment_method_id: CARD.into(), amount_minor: 1280 },
        ];
        let p = prepare(&store, "en", BRANCH, SHIFT, &input, 0.14, "2026-06-20T12:00:00+00:00".into()).unwrap();
        let legs = p.command.request.payment_splits.flatten().expect("splits set");
        assert_eq!(legs.len(), 2);
        assert_eq!(legs[0].method, "Cash"); // raw wire name, not id/label
        assert_eq!(legs[0].amount, 1000);
        assert_eq!(legs[1].method, "Card");
        assert_eq!(legs[1].amount, 1280);
    }

    #[test]
    fn split_legs_with_unknown_method_are_dropped() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        cart::add(&store, ITEM, "Latte", 2000).unwrap();
        let mut input = mk_input(CASH, 0);
        input.splits = vec![
            CheckoutSplit { payment_method_id: CASH.into(), amount_minor: 1000 },
            CheckoutSplit { payment_method_id: "00000000-0000-0000-0000-0000000000ee".into(), amount_minor: 500 },
        ];
        let p = prepare(&store, "en", BRANCH, SHIFT, &input, 0.0, "2026-06-20T12:00:00+00:00".into()).unwrap();
        let legs = p.command.request.payment_splits.flatten().expect("at least one good leg");
        assert_eq!(legs.len(), 1); // the ghost leg is filtered out
        assert_eq!(legs[0].method, "Cash");
    }

    #[test]
    fn no_splits_leaves_payment_splits_unset() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        cart::add(&store, ITEM, "Latte", 1000).unwrap();
        let p = prep(&store, CASH, 2000).unwrap();
        assert_eq!(p.command.request.payment_splits, None);
    }

    // ── tip on card leaves change untouched ───────────────────────────────────

    #[test]
    fn card_tip_does_not_reduce_cash_change() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        cart::add(&store, ITEM, "Latte", 1000).unwrap();
        // Cash order (total 1140 @14%), tendered 1500, but the tip is paid on CARD
        // → change stays 360 (1500 - 1140), not reduced by the tip.
        let mut input = mk_input(CASH, 1500);
        input.tip_minor = 200;
        input.tip_payment_method_id = Some(CARD.into());
        let p = prepare(&store, "en", BRANCH, SHIFT, &input, 0.14, "2026-06-20T12:00:00+00:00".into()).unwrap();
        assert_eq!(p.receipt.tip_minor, 200);
        assert_eq!(p.receipt.change_minor, 360); // unaffected by the card tip
        assert_eq!(p.command.request.tip_amount, Some(Some(200)));
        assert_eq!(p.command.request.tip_payment_method, Some(Some("Card".into())));
    }

    #[test]
    fn negative_tip_is_clamped_to_zero_and_not_recorded() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        cart::add(&store, ITEM, "Latte", 1000).unwrap();
        let mut input = mk_input(CASH, 2000);
        input.tip_minor = -500;
        let p = prepare(&store, "en", BRANCH, SHIFT, &input, 0.0, "2026-06-20T12:00:00+00:00".into()).unwrap();
        assert_eq!(p.receipt.tip_minor, 0);
        assert_eq!(p.command.request.tip_amount, None); // tip <= 0 → not set on the wire
    }

    // ── discount kinds carried verbatim onto the wire ─────────────────────────

    fn seed_discounts(store: &Store) {
        store
            .kv_put(
                menu::K_DISCOUNTS,
                r#"[
                  {"created_at":"2026-06-19T10:00:00Z","updated_at":"2026-06-19T10:00:00Z","dtype":"percentage","id":"00000000-0000-0000-0000-0000000000d1","is_active":true,"name":"10% off","name_translations":{},"org_id":"00000000-0000-0000-0000-0000000000ff","value":10},
                  {"created_at":"2026-06-19T10:00:00Z","updated_at":"2026-06-19T10:00:00Z","dtype":"fixed","id":"00000000-0000-0000-0000-0000000000d2","is_active":true,"name":"250 off","name_translations":{},"org_id":"00000000-0000-0000-0000-0000000000ff","value":250}
                ]"#,
            )
            .unwrap();
    }

    #[test]
    fn percentage_discount_is_recorded_on_the_wire_and_receipt() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        seed_discounts(&store);
        cart::add(&store, ITEM, "Latte", 1000).unwrap();
        cart::set_discount(&store, "00000000-0000-0000-0000-0000000000d1").unwrap();
        let p = prepare(&store, "en", BRANCH, SHIFT, &mk_input(CASH, 2000), 0.14, "2026-06-20T12:00:00+00:00".into()).unwrap();
        let r = &p.command.request;
        assert_eq!(r.discount_type, Some(Some("percentage".into())));
        assert_eq!(r.discount_value, Some(Some(10)));
        assert_eq!(r.discount_amount, Some(Some(100))); // 10% of 1000
        assert_eq!(r.discount_id, Some(Some(uuid::Uuid::parse_str("00000000-0000-0000-0000-0000000000d1").unwrap())));
        // subtotal 1000, discount 100, taxable 900, tax round(126), total 1026.
        assert_eq!(r.subtotal, Some(Some(1000)));
        assert_eq!(r.tax_amount, Some(Some(126)));
        assert_eq!(r.total_amount, Some(Some(1026)));
        assert_eq!(p.receipt.discount_minor, 100);
        assert_eq!(p.receipt.total_minor, 1026);
    }

    #[test]
    fn fixed_discount_is_recorded_on_the_wire() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        seed_discounts(&store);
        cart::add(&store, ITEM, "Latte", 1000).unwrap();
        cart::set_discount(&store, "00000000-0000-0000-0000-0000000000d2").unwrap();
        let p = prepare(&store, "en", BRANCH, SHIFT, &mk_input(CASH, 2000), 0.0, "2026-06-20T12:00:00+00:00".into()).unwrap();
        let r = &p.command.request;
        assert_eq!(r.discount_type, Some(Some("fixed".into())));
        assert_eq!(r.discount_value, Some(Some(250)));
        assert_eq!(r.discount_amount, Some(Some(250)));
        assert_eq!(r.total_amount, Some(Some(750))); // 1000 - 250, no tax
    }

    #[test]
    fn no_discount_leaves_discount_fields_unset() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        cart::add(&store, ITEM, "Latte", 1000).unwrap();
        let p = prep(&store, CASH, 2000).unwrap();
        let r = &p.command.request;
        assert_eq!(r.discount_type, None);
        assert_eq!(r.discount_value, None);
        assert_eq!(r.discount_amount, None);
        assert_eq!(r.discount_id, None);
    }

    // ── queued_cash_total: signed cash_movement ops ───────────────────────────

    #[test]
    fn queued_cash_total_adds_signed_cash_movements() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        // One queued cash order (2000) plus a +500 pay-in and a -300 pay-out.
        let mut req = models::CreateOrderRequest::new(
            uuid::Uuid::new_v4(), vec![], "Cash".into(), uuid::Uuid::new_v4());
        req.total_amount = Some(Some(2000));
        store
            .enqueue(&crate::store::NewOutboxOp {
                id: "o1".into(),
                op_type: "create_order".into(),
                idempotency_key: "o1".into(),
                payload: serde_json::to_string(&CheckoutCommand { request: req }).unwrap(),
                event_at: "2026-06-20T12:00:00+00:00".into(),
                ..Default::default()
            })
            .unwrap();

        let mv = |id: &str, amount: i32| {
            let mut r = models::CashMovementRequest::new(amount, "drawer".into());
            r.client_ref = Some(Some(uuid::Uuid::new_v4()));
            let cmd = crate::shift::CashMovementCommand { shift_id: "s1".into(), request: r };
            store
                .enqueue(&crate::store::NewOutboxOp {
                    id: id.into(),
                    op_type: "cash_movement".into(),
                    idempotency_key: id.into(),
                    payload: serde_json::to_string(&cmd).unwrap(),
                    event_at: "2026-06-20T12:00:00+00:00".into(),
                    ..Default::default()
                })
                .unwrap();
        };
        mv("m1", 500); // pay-in
        mv("m2", -300); // pay-out
        // 2000 (cash order) + 500 - 300 = 2200.
        assert_eq!(queued_cash_total(&store).unwrap(), 2200);
    }

    #[test]
    fn queued_cash_total_for_is_scoped_to_one_shift() {
        // The OFFLINE Z-report drawer figure for a PAST shift: sum only THAT shift's
        // still-queued cash sales + cash movements (scoped by the outbox row's
        // shift_id), never another shift's or non-cash sales.
        let store = Store::open("").unwrap();
        seed_methods(&store);
        const A: &str = "00000000-0000-0000-0000-0000000000aa";
        const B: &str = "00000000-0000-0000-0000-0000000000bb";
        let order = |id: &str, shift: &str, method: &str, total: i32| {
            let mut req = models::CreateOrderRequest::new(
                uuid::Uuid::new_v4(), vec![], method.into(), uuid::Uuid::new_v4());
            req.total_amount = Some(Some(total));
            store
                .enqueue(&crate::store::NewOutboxOp {
                    id: id.into(),
                    op_type: "create_order".into(),
                    idempotency_key: id.into(),
                    payload: serde_json::to_string(&CheckoutCommand { request: req }).unwrap(),
                    event_at: "2026-06-20T12:00:00+00:00".into(),
                    shift_id: Some(shift.into()),
                    ..Default::default()
                })
                .unwrap();
        };
        let movement = |id: &str, shift: &str, amount: i32| {
            let mut r = models::CashMovementRequest::new(amount, "drawer".into());
            r.client_ref = Some(Some(uuid::Uuid::new_v4()));
            let cmd = crate::shift::CashMovementCommand { shift_id: shift.into(), request: r };
            store
                .enqueue(&crate::store::NewOutboxOp {
                    id: id.into(),
                    op_type: "cash_movement".into(),
                    idempotency_key: id.into(),
                    payload: serde_json::to_string(&cmd).unwrap(),
                    event_at: "2026-06-20T12:00:00+00:00".into(),
                    shift_id: Some(shift.into()),
                    ..Default::default()
                })
                .unwrap();
        };
        order("a1", A, "Cash", 2000);
        order("a2", A, "Card", 999); // not cash → excluded
        order("b1", B, "Cash", 5000); // other shift → excluded
        movement("ma", A, 300); // A pay-in
        movement("mb", B, 100); // other shift → excluded
        assert_eq!(queued_cash_total_for(&store, A).unwrap(), 2300, "A: cash 2000 + movement 300");
        assert_eq!(queued_cash_total_for(&store, B).unwrap(), 5100, "B: cash 5000 + its own movement 100");
    }

    #[test]
    fn queued_cash_total_excludes_inflight_orders() {
        // An inflight order has been SENT → a freshly-fetched shift report already
        // counts it, so queued_cash must NOT add it again (the lost-response
        // double-count the report layer was hitting). pending → still counted.
        let store = Store::open("").unwrap();
        seed_methods(&store);
        let mut req = models::CreateOrderRequest::new(
            uuid::Uuid::new_v4(), vec![], "Cash".into(), uuid::Uuid::new_v4());
        req.total_amount = Some(Some(2000));
        let seq = store
            .enqueue(&crate::store::NewOutboxOp {
                id: "o1".into(),
                op_type: "create_order".into(),
                idempotency_key: "o1".into(),
                payload: serde_json::to_string(&CheckoutCommand { request: req }).unwrap(),
                event_at: "2026-06-20T12:00:00+00:00".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(queued_cash_total(&store).unwrap(), 2000, "pending cash counts");
        store.mark_inflight(seq).unwrap();
        assert_eq!(queued_cash_total(&store).unwrap(), 0, "inflight excluded (the synced report has it)");
    }

    #[test]
    fn queued_cash_total_ignores_unrelated_op_types() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        // An open_shift op (unrelated) must contribute nothing.
        store
            .enqueue(&crate::store::NewOutboxOp {
                id: "open".into(),
                op_type: "open_shift".into(),
                idempotency_key: "open".into(),
                payload: "{}".into(),
                event_at: "2026-06-20T12:00:00+00:00".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(queued_cash_total(&store).unwrap(), 0);
    }

    // ── receipt projection: size / modifiers / bundle components ──────────────

    fn cfg_addon(id: &str, kind: &str, price: i64) -> menu::AddonItemView {
        menu::AddonItemView {
            id: id.into(),
            name: id.into(),
            addon_type: kind.into(),
            default_price_minor: price,
            is_active: true,
            ingredients: vec![],
        }
    }

    fn cfg_item() -> menu::MenuItemView {
        menu::MenuItemView {
            id: "latte".into(),
            name: "Latte".into(),
            description: None,
            category_id: None,
            base_price_minor: 5000,
            image_url: None,
            is_active: true,
            default_milk_addon_id: Some("oat".into()),
            allowed_addon_ids: vec![],
            sizes: vec![menu::ItemSizeView { id: "lg".into(), label: "Large".into(), price_minor: 6000, is_active: true }],
            addon_slots: vec![],
            optional_fields: vec![menu::OptionalFieldView {
                id: "van".into(), name: "Vanilla".into(), price_minor: 300, is_active: true,
                ingredient_name: None, ingredient_unit: None, quantity_used: None, org_ingredient_id: None,
            }],
            recipes: vec![],
        }
    }

    fn cfg_catalog() -> Vec<menu::AddonItemView> {
        vec![
            cfg_addon("oat", "milk_type", 1500),    // default-milk base
            cfg_addon("almond", "milk_type", 2000), // swap → +500
            cfg_addon("shot", "extra", 800),        // additive → full
        ]
    }

    #[test]
    fn receipt_line_carries_size_addons_and_optionals() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        // Large + almond(+500 swap) + 2×shot(+800) + vanilla(+300).
        let line = cart::resolve_line(
            &cfg_item(),
            &cfg_catalog(),
            Some("Large".into()),
            &[cart::AddonSelection { addon_item_id: "almond".into(), qty: 1 },
              cart::AddonSelection { addon_item_id: "shot".into(), qty: 2 }],
            &["van".into()],
            1,
            None,
        );
        cart::add_resolved(&store, line).unwrap();
        let p = prep(&store, CASH, 20000).unwrap();
        assert_eq!(p.receipt.lines.len(), 1);
        let rl = &p.receipt.lines[0];
        assert_eq!(rl.name, "Latte");
        assert_eq!(rl.size_label.as_deref(), Some("Large"));
        assert!(!rl.is_bundle);
        // Two addons; the multi-qty one prints "name ×N".
        assert_eq!(rl.addons.len(), 2);
        let almond = rl.addons.iter().find(|a| a.name == "almond").unwrap();
        assert_eq!(almond.price_minor, 500);
        let shot = rl.addons.iter().find(|a| a.name.starts_with("shot")).unwrap();
        assert_eq!(shot.name, "shot ×2");
        assert_eq!(shot.price_minor, 800);
        assert_eq!(rl.optionals.len(), 1);
        assert_eq!(rl.optionals[0].name, "Vanilla");
        assert_eq!(rl.optionals[0].price_minor, 300);
        // line total = 6000 + 500 + 800*2 + 300 = 8400.
        assert_eq!(rl.line_total_minor, 8400);
        assert!(rl.components.is_empty());
    }

    fn cfg_bundle() -> menu::BundleView {
        menu::BundleView {
            id: "b1".into(),
            name: "Morning Combo".into(),
            description: None,
            price_minor: 10000,
            image_url: None,
            is_available: true,
            available_from_date: None,
            available_until_date: None,
            available_from_time: None,
            available_until_time: None,
            components: vec![],
        }
    }

    #[test]
    fn receipt_bundle_line_carries_components_with_their_modifiers() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        let comp = cart::BundleComponentSelection {
            item_id: "latte".into(),
            size_label: Some("Large".into()),
            qty: 1,
            addons: vec![cart::AddonSelection { addon_item_id: "almond".into(), qty: 1 }],
            optional_field_ids: vec!["van".into()],
        };
        let line = cart::resolve_bundle_line(&cfg_bundle(), &[cfg_item()], &cfg_catalog(), &[comp], 1);
        cart::add_resolved(&store, line).unwrap();
        let p = prep(&store, CASH, 20000).unwrap();
        let rl = &p.receipt.lines[0];
        assert!(rl.is_bundle);
        assert_eq!(rl.name, "Morning Combo");
        // Bundle lines carry no top-level addons/optionals — only components.
        assert!(rl.addons.is_empty());
        assert!(rl.optionals.is_empty());
        assert_eq!(rl.components.len(), 1);
        let c = &rl.components[0];
        assert_eq!(c.name, "Latte");
        assert_eq!(c.size_label.as_deref(), Some("Large"));
        assert_eq!(c.addons.len(), 1);
        assert_eq!(c.addons[0].name, "almond");
        assert_eq!(c.addons[0].price_minor, 500);
        assert_eq!(c.optionals.len(), 1);
        assert_eq!(c.optionals[0].name, "Vanilla");
        // line total = 10000 fixed + 500 almond delta + 300 vanilla = 10800.
        assert_eq!(rl.line_total_minor, 10800);
    }

    // The wire (CreateOrderRequest) parses cart ids as UUIDs, so the wire-shape
    // test needs UUID ids (the receipt projection above only copies strings).
    const BUNDLE_UUID: &str = "00000000-0000-0000-0000-0000000000b1";
    const ITEM_UUID: &str = "00000000-0000-0000-0000-0000000000a1";
    const ALMOND_UUID: &str = "00000000-0000-0000-0000-0000000000a2";
    const VAN_UUID: &str = "00000000-0000-0000-0000-0000000000a3";

    fn uuid_item() -> menu::MenuItemView {
        let mut it = cfg_item();
        it.id = ITEM_UUID.into();
        it.default_milk_addon_id = None; // no milk base → almond charges full (simpler)
        it.optional_fields = vec![menu::OptionalFieldView {
            id: VAN_UUID.into(), name: "Vanilla".into(), price_minor: 300, is_active: true,
            ingredient_name: None, ingredient_unit: None, quantity_used: None, org_ingredient_id: None,
        }];
        it
    }

    fn uuid_catalog() -> Vec<menu::AddonItemView> {
        vec![cfg_addon(ALMOND_UUID, "extra", 800)] // additive → full 800
    }

    fn uuid_bundle() -> menu::BundleView {
        let mut b = cfg_bundle();
        b.id = BUNDLE_UUID.into();
        b
    }

    #[test]
    fn bundle_wire_item_carries_components_and_clears_top_level_modifiers() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        let comp = cart::BundleComponentSelection {
            item_id: ITEM_UUID.into(),
            size_label: Some("Large".into()),
            qty: 2,
            addons: vec![cart::AddonSelection { addon_item_id: ALMOND_UUID.into(), qty: 1 }],
            optional_field_ids: vec![VAN_UUID.into()],
        };
        let line = cart::resolve_bundle_line(&uuid_bundle(), &[uuid_item()], &uuid_catalog(), &[comp], 1);
        cart::add_resolved(&store, line).unwrap();
        let p = prep(&store, CASH, 20000).unwrap();
        let item = &p.command.request.items[0];
        // Bundle id set, top-level addons/optionals empty (absent or empty vec),
        // components present.
        assert_eq!(item.bundle_id, Some(Some(uuid::Uuid::parse_str(BUNDLE_UUID).unwrap())));
        assert!(item.addons.as_ref().is_none_or(|v| v.is_empty()));
        assert!(item.optional_field_ids.as_ref().is_none_or(|v| v.is_empty()));
        let comps = item.bundle_components.as_ref().expect("bundle_components set");
        assert_eq!(comps.len(), 1);
        assert_eq!(comps[0].quantity, 2);
        assert_eq!(comps[0].size_label, Some(Some("Large".into())));
        // The component's chosen addon rode through with its charged price.
        let cadd = comps[0].addons.as_ref().expect("component addons");
        assert_eq!(cadd.len(), 1);
        assert_eq!(cadd[0].unit_price, Some(Some(800)));
        let copt = comps[0].optional_field_ids.as_ref().expect("component optional ids");
        assert_eq!(copt.len(), 1);
    }

    #[test]
    fn normal_wire_item_carries_size_and_unit_price() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        let line = cart::resolve_line(
            &uuid_item(),
            &uuid_catalog(),
            Some("Large".into()),
            &[],
            &[],
            1,
            None,
        );
        cart::add_resolved(&store, line).unwrap();
        let p = prep(&store, CASH, 20000).unwrap();
        let item = &p.command.request.items[0];
        assert_eq!(item.menu_item_id, Some(Some(uuid::Uuid::parse_str(ITEM_UUID).unwrap())));
        assert_eq!(item.size_label, Some(Some("Large".into())));
        assert_eq!(item.unit_price, Some(Some(6000))); // Large size price recorded verbatim
        assert_eq!(item.bundle_id, None);
    }

    // ── Receipt projection edge cases (closed test gaps found by cargo-mutants) ──

    /// A whitespace-only customer name must be filtered to None on the receipt, not
    /// printed as blank. (Kills the `!` deletion in the prepare receipt filter.)
    #[test]
    fn receipt_filters_blank_customer_name() {
        let store = Store::open("").unwrap();
        seed_methods(&store);
        cart::add(&store, ITEM, "Latte", 1000).unwrap();
        let mut input = mk_input(CASH, 2000);
        input.customer_name = Some("   ".into()); // whitespace only
        let p = prepare(&store, "en", BRANCH, SHIFT, &input, 0.14, "2026-06-20T12:00:00+00:00".into())
            .unwrap();
        assert_eq!(p.receipt.customer_name, None, "blank name must not reach the receipt");

        // A real name survives the filter.
        let mut named = mk_input(CASH, 2000);
        named.customer_name = Some("Mona".into());
        let p2 = prepare(&store, "en", BRANCH, SHIFT, &named, 0.14, "2026-06-20T12:00:00+00:00".into())
            .unwrap();
        assert_eq!(p2.receipt.customer_name, Some("Mona".into()));
    }

    /// A BUNDLE-COMPONENT addon with qty>1 must render "name ×qty" on the receipt
    /// (the top-level addon path was tested; the nested bundle one was not — this
    /// kills the `>`→`<` mutant in receipt_line_from_cart's component branch).
    #[test]
    fn receipt_bundle_component_addon_shows_multiplier() {
        let line = cart::CartLineView {
            key: "k".into(),
            item_id: "i".into(),
            name: "Combo".into(),
            size_label: None,
            addons: vec![],
            optionals: vec![],
            notes: None,
            unit_price_minor: 0,
            qty: 1,
            line_total_minor: 0,
            bundle_id: Some("b".into()),
            bundle_components: vec![cart::CartBundleComponentView {
                item_id: "c".into(),
                name: "Espresso".into(),
                qty: 1,
                size_label: None,
                addons: vec![
                    cart::CartAddonView {
                        addon_item_id: "a1".into(),
                        name: "Extra Shot".into(),
                        qty: 2, // >1 → must show "×2"
                        price_modifier_minor: 500,
                    },
                    cart::CartAddonView {
                        addon_item_id: "a2".into(),
                        name: "Oat Milk".into(),
                        qty: 1, // ==1 → no multiplier
                        price_modifier_minor: 300,
                    },
                ],
                optionals: vec![],
            }],
        };
        let r = receipt_line_from_cart(&line);
        let comp_addons = &r.components[0].addons;
        assert_eq!(comp_addons[0].name, "Extra Shot ×2", "qty>1 must show the multiplier");
        assert_eq!(comp_addons[1].name, "Oat Milk", "qty==1 must NOT show a multiplier");
    }
}
