//! Delivery-order management for the teller — the staff side of the delivery
//! stack (the customer-facing guest/OTP create flow lives elsewhere). The teller
//! *receives* and *works* a branch's delivery queue: list, advance status
//! (received → confirmed → preparing → ready → out_for_delivery → delivered),
//! set prep time, cancel, and finalize (replay the frozen snapshot into a real
//! sale on the open shift). Mirrors Flutter's `delivery_order_repository.dart` /
//! `delivery_api.dart`.
//!
//! These are inherently ONLINE operations (you're working a live queue), so the
//! FFIs hit the network directly via the generated client rather than the
//! offline outbox. Projection here is pure + unit-testable.

use sufrix_api::models;

/// One delivery order, projected for the queue list + detail. Money is minor
/// units; `channel`/`status` are wire strings the host localizes.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct DeliveryOrderView {
    pub id: String,
    pub order_ref: Option<String>,
    /// "in_mall" | "outside".
    pub channel: String,
    /// received | confirmed | preparing | ready | out_for_delivery | delivered |
    /// cancelled | rejected.
    pub status: String,
    pub customer_name: String,
    pub customer_phone: String,
    /// One-line composed address (place, line, unit, floor, landmark).
    pub address: Option<String>,
    pub delivery_notes: Option<String>,
    pub payment_hint: Option<String>,
    pub subtotal_minor: i64,
    pub discount_minor: i64,
    pub delivery_fee_minor: i64,
    pub total_minor: i64,
    pub item_count: i64,
    pub created_at: String,
    /// `true` once the order reached a terminal state (delivered/cancelled/rejected).
    pub is_terminal: bool,
}

/// The branch's delivery configuration + the POS-owned accepting overrides.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct DeliverySettingsView {
    pub in_mall_enabled: bool,
    /// "auto" | "open" | "closed".
    pub in_mall_override: String,
    pub in_mall_fee_minor: i64,
    pub outside_enabled: bool,
    pub outside_override: String,
    pub prep_time_minutes: i64,
}

/// Result of finalizing a delivery order into a real sale.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct DeliveryFinalizeView {
    pub order_id: String,
    pub order_ref: Option<String>,
    pub warnings: Vec<String>,
}

/// Project a wire `DeliveryOrder` into the view. `locale` localizes the address
/// "Unit"/"Floor" prefixes.
pub(crate) fn order_view(o: &models::DeliveryOrder, locale: &str) -> DeliveryOrderView {
    let item_count = o
        .cart
        .get("items")
        .and_then(|v| v.as_array())
        .map(|a| a.len() as i64)
        .unwrap_or(0);
    DeliveryOrderView {
        id: o.id.to_string(),
        order_ref: o.delivery_ref.clone().flatten().filter(|s| !s.is_empty()),
        channel: o.channel.clone(),
        status: o.status.clone(),
        customer_name: o.customer_name.clone(),
        customer_phone: o.customer_phone.clone(),
        address: compose_address(o, locale),
        delivery_notes: o.delivery_notes.clone().flatten().filter(|s| !s.is_empty()),
        payment_hint: None,
        subtotal_minor: o.subtotal as i64,
        discount_minor: o.discount_amount.unwrap_or(0) as i64,
        delivery_fee_minor: o.delivery_fee as i64,
        total_minor: o.total as i64,
        item_count,
        created_at: o.created_at.to_rfc3339(),
        is_terminal: matches!(o.status.as_str(), "delivered" | "cancelled" | "rejected"),
    }
}

/// Compose a one-line address from a delivery order's parts (Flutter's order:
/// place, line, unit, floor, landmark), comma-joined, skipping blanks.
fn compose_address(o: &models::DeliveryOrder, locale: &str) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let push = |parts: &mut Vec<String>, v: &Option<Option<String>>| {
        if let Some(s) = v.clone().flatten() {
            if !s.trim().is_empty() {
                parts.push(s);
            }
        }
    };
    push(&mut parts, &o.place_name);
    push(&mut parts, &o.address_line);
    if let Some(u) = o.unit_number.clone().flatten().filter(|s| !s.trim().is_empty()) {
        parts.push(format!("{} {}", crate::i18n::tr(locale, "delivery.unit"), u));
    }
    if let Some(f) = o.floor.clone().flatten().filter(|s| !s.trim().is_empty()) {
        parts.push(format!("{} {}", crate::i18n::tr(locale, "delivery.floor"), f));
    }
    push(&mut parts, &o.landmark);
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

/// Project the branch delivery settings.
pub(crate) fn settings_view(s: &models::BranchDeliverySettings) -> DeliverySettingsView {
    DeliverySettingsView {
        in_mall_enabled: s.in_mall_enabled,
        in_mall_override: s.in_mall_override.clone(),
        in_mall_fee_minor: s.in_mall_fee as i64,
        outside_enabled: s.outside_enabled,
        outside_override: s.outside_override.clone(),
        prep_time_minutes: s.prep_time_minutes as i64,
    }
}

/// The forward status step after `current` (Flutter's single-step advance).
/// `None` at a terminal/last-workable state.
pub fn next_status(current: &str) -> Option<&'static str> {
    match current {
        "received" => Some("confirmed"),
        "confirmed" => Some("preparing"),
        "preparing" => Some("ready"),
        "ready" => Some("out_for_delivery"),
        "out_for_delivery" => Some("delivered"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_advances_through_the_lifecycle_then_stops() {
        assert_eq!(next_status("received"), Some("confirmed"));
        assert_eq!(next_status("preparing"), Some("ready"));
        assert_eq!(next_status("out_for_delivery"), Some("delivered"));
        assert_eq!(next_status("delivered"), None);
        assert_eq!(next_status("cancelled"), None);
    }
}
