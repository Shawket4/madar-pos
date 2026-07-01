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

use madar_api::models;

/// One delivery order, projected for the queue list + detail. Money is minor
/// units; `channel`/`status` are wire strings the host localizes.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    /// The order's actual priced lines, projected from the frozen `cart.lines`
    /// snapshot into the SAME shape tickets use — so both render identically.
    pub lines: Vec<crate::tickets::TicketLineView>,
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
    // The frozen snapshot stores its priced lines under `cart.lines` (a
    // `CartSnapshot`), NOT `cart.items` — reading the wrong key left every delivery
    // order showing "0 items" with no line detail.
    let lines = cart_lines(&o.cart);
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
        item_count: lines.len() as i64,
        lines,
        created_at: o.created_at.to_rfc3339(),
        is_terminal: matches!(o.status.as_str(), "delivered" | "cancelled" | "rejected"),
    }
}

/// Project the frozen cart's priced `lines` into display lines, reusing the ticket
/// line shape so the tickets + delivery details render through one component.
/// Tolerant of field-name variants so it works with the wire snapshot and tests.
fn cart_lines(cart: &serde_json::Value) -> Vec<crate::tickets::TicketLineView> {
    cart.get("lines")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().map(project_line).collect())
        .unwrap_or_default()
}

fn project_line(l: &serde_json::Value) -> crate::tickets::TicketLineView {
    let str_of = |k: &str| {
        l.get(k)
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .filter(|s| !s.is_empty())
    };
    let name = str_of("item_name").or_else(|| str_of("name")).unwrap_or_default();
    let qty = l
        .get("quantity")
        .or_else(|| l.get("qty"))
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(1) as i32;
    let line_total = l
        .get("line_total")
        .or_else(|| l.get("line_total_minor"))
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    // Modifiers: addon names (× qty when > 1), then chosen optional/field names.
    let mut modifiers: Vec<String> = Vec::new();
    if let Some(addons) = l.get("addons").and_then(|v| v.as_array()) {
        for a in addons {
            if let Some(n) = a.get("addon_name").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                let q = a.get("quantity").and_then(serde_json::Value::as_i64).unwrap_or(1);
                modifiers.push(if q > 1 { format!("{n} ×{q}") } else { n.to_string() });
            }
        }
    }
    if let Some(opts) = l.get("optionals").and_then(|v| v.as_array()) {
        for opt in opts {
            if let Some(n) = opt.get("field_name").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                modifiers.push(n.to_string());
            }
        }
    }
    crate::tickets::TicketLineView {
        name,
        qty,
        size_label: str_of("size_label"),
        modifiers,
        line_total_minor: line_total,
        voided: false,
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

/// The forward status step after `current` (single-step advance). `None` at a
/// terminal/last-workable state. NOTE: `out_for_delivery` is the LAST step the
/// `/status` endpoint accepts — it only validates the line steps received→
/// out_for_delivery. `delivered` is reached ONLY by `finalize` (which records the
/// sale), so advancing into it via `/status` 400s. The board therefore stops the
/// Advance action at out_for_delivery and offers Finalize for the terminal step.
pub fn next_status(current: &str) -> Option<&'static str> {
    match current {
        "received" => Some("confirmed"),
        "confirmed" => Some("preparing"),
        "preparing" => Some("ready"),
        "ready" => Some("out_for_delivery"),
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
        // out_for_delivery is the last /status step — `delivered` is finalize-only.
        assert_eq!(next_status("out_for_delivery"), None);
        assert_eq!(next_status("delivered"), None);
        assert_eq!(next_status("cancelled"), None);
    }

    // ---- builders -------------------------------------------------------

    fn uid(b: u8) -> uuid::Uuid {
        let mut bytes = [0u8; 16];
        bytes[15] = b;
        uuid::Uuid::from_bytes(bytes)
    }

    fn ts() -> chrono::DateTime<chrono::FixedOffset> {
        chrono::DateTime::parse_from_rfc3339("2026-06-21T10:00:00+00:00").unwrap()
    }

    fn cart_with_lines(n: usize) -> serde_json::Value {
        let lines: Vec<serde_json::Value> = (0..n)
            .map(|i| serde_json::json!({ "item_name": format!("Item {i}"), "quantity": 1, "line_total": 1000 }))
            .collect();
        serde_json::json!({ "lines": lines })
    }

    fn order(status: &str, channel: &str, cart: serde_json::Value) -> models::DeliveryOrder {
        models::DeliveryOrder::new(
            uid(1),      // branch_id
            cart,
            channel.into(),
            ts(),        // created_at
            "Carol".into(),
            "01000000000".into(),
            1500,        // delivery_fee
            0,           // extra_prep_minutes
            uid(2),      // id
            uid(3),      // org_id
            true,        // otp_verified
            status.into(),
            10000,       // subtotal
            11500,       // total
            ts(),        // updated_at
        )
    }

    fn settings() -> models::BranchDeliverySettings {
        models::BranchDeliverySettings::new(
            uid(9),
            true,          // in_mall_enabled
            500,           // in_mall_fee
            "auto".into(), // in_mall_override
            true,          // in_mall_require_location
            true,          // otp_required
            false,         // outside_enabled
            "closed".into(),
            25,            // prep_time_minutes
        )
    }

    // ---- order_view: projection ----------------------------------------

    #[test]
    fn order_view_maps_core_fields() {
        let o = order("received", "outside", cart_with_lines(3));
        let v = order_view(&o, "en");
        assert_eq!(v.id, o.id.to_string());
        assert_eq!(v.channel, "outside");
        assert_eq!(v.status, "received");
        assert_eq!(v.customer_name, "Carol");
        assert_eq!(v.customer_phone, "01000000000");
        assert_eq!(v.subtotal_minor, 10000);
        assert_eq!(v.delivery_fee_minor, 1500);
        assert_eq!(v.total_minor, 11500);
        assert_eq!(v.discount_minor, 0); // discount_amount None → 0
        assert_eq!(v.item_count, 3);
        assert_eq!(v.created_at, ts().to_rfc3339());
        assert_eq!(v.payment_hint, None); // always None in the projection
        assert!(!v.is_terminal);
    }

    #[test]
    fn order_view_discount_amount_passed_through() {
        let mut o = order("received", "outside", cart_with_lines(1));
        o.discount_amount = Some(750);
        let v = order_view(&o, "en");
        assert_eq!(v.discount_minor, 750);
    }

    #[test]
    fn order_view_item_count_zero_when_no_lines_key() {
        let mut o = order("received", "in_mall", serde_json::json!({}));
        let v = order_view(&o, "en");
        assert_eq!(v.item_count, 0);
        assert!(v.lines.is_empty());
        // also when lines is not an array
        o.cart = serde_json::json!({ "lines": "nope" });
        assert_eq!(order_view(&o, "en").item_count, 0);
        // and an empty lines array
        o.cart = serde_json::json!({ "lines": [] });
        assert_eq!(order_view(&o, "en").item_count, 0);
    }

    #[test]
    fn order_view_projects_cart_lines_from_snapshot() {
        // The real frozen shape: cart.lines[] of SnapshotLine (item_name/quantity/
        // line_total/size_label/addons/optionals).
        let cart = serde_json::json!({ "lines": [
            {
                "item_name": "Burger", "size_label": "Large", "quantity": 2, "line_total": 4500,
                "addons": [{ "addon_name": "Extra cheese", "quantity": 2 }],
                "optionals": [{ "field_name": "No onion" }]
            },
            { "item_name": "Fries", "quantity": 1, "line_total": 1500 }
        ]});
        let o = order("received", "outside", cart);
        let v = order_view(&o, "en");
        assert_eq!(v.item_count, 2, "count is the number of priced lines");
        assert_eq!(v.lines.len(), 2);
        let l = &v.lines[0];
        assert_eq!(l.name, "Burger");
        assert_eq!(l.qty, 2);
        assert_eq!(l.size_label.as_deref(), Some("Large"));
        assert_eq!(l.line_total_minor, 4500);
        assert_eq!(l.modifiers, vec!["Extra cheese ×2".to_string(), "No onion".to_string()]);
        assert!(!l.voided);
        assert_eq!(v.lines[1].name, "Fries");
        assert!(v.lines[1].modifiers.is_empty());
    }

    #[test]
    fn order_view_order_ref_blank_filtered() {
        let mut o = order("received", "outside", cart_with_lines(1));
        o.delivery_ref = Some(Some(String::new()));
        assert_eq!(order_view(&o, "en").order_ref, None);
        o.delivery_ref = Some(Some("D-DT-0042".into()));
        assert_eq!(order_view(&o, "en").order_ref.as_deref(), Some("D-DT-0042"));
        o.delivery_ref = None; // absent
        assert_eq!(order_view(&o, "en").order_ref, None);
    }

    #[test]
    fn order_view_delivery_notes_blank_filtered() {
        let mut o = order("received", "outside", cart_with_lines(1));
        o.delivery_notes = Some(Some(String::new()));
        assert_eq!(order_view(&o, "en").delivery_notes, None);
        o.delivery_notes = Some(Some("Leave at door".into()));
        assert_eq!(order_view(&o, "en").delivery_notes.as_deref(), Some("Leave at door"));
    }

    // ---- order_view: is_terminal ---------------------------------------

    #[test]
    fn order_view_terminal_states() {
        for s in ["delivered", "cancelled", "rejected"] {
            let o = order(s, "outside", cart_with_lines(1));
            assert!(order_view(&o, "en").is_terminal, "{s} should be terminal");
        }
    }

    #[test]
    fn order_view_non_terminal_states() {
        for s in ["received", "confirmed", "preparing", "ready", "out_for_delivery"] {
            let o = order(s, "outside", cart_with_lines(1));
            assert!(!order_view(&o, "en").is_terminal, "{s} should not be terminal");
        }
    }

    // ---- order_view: address composition -------------------------------

    #[test]
    fn order_view_address_full_order_and_prefixes() {
        let mut o = order("received", "outside", cart_with_lines(1));
        o.place_name = Some(Some("Tower A".into()));
        o.address_line = Some(Some("12 Main St".into()));
        o.unit_number = Some(Some("4B".into()));
        o.floor = Some(Some("3".into()));
        o.landmark = Some(Some("Near park".into()));
        let v = order_view(&o, "en");
        assert_eq!(v.address.as_deref(), Some("Tower A, 12 Main St, Unit 4B, Floor 3, Near park"));
    }

    #[test]
    fn order_view_address_arabic_prefixes() {
        let mut o = order("received", "outside", cart_with_lines(1));
        o.unit_number = Some(Some("4B".into()));
        o.floor = Some(Some("3".into()));
        let v = order_view(&o, "ar");
        assert_eq!(v.address.as_deref(), Some("وحدة 4B, طابق 3"));
    }

    #[test]
    fn order_view_address_skips_blanks_and_whitespace() {
        let mut o = order("received", "outside", cart_with_lines(1));
        o.place_name = Some(Some("Tower A".into()));
        o.address_line = Some(Some(String::new())); // blank skipped
        o.unit_number = Some(Some("   ".into()));    // whitespace skipped
        o.landmark = Some(Some("Gate 2".into()));
        let v = order_view(&o, "en");
        // place, then landmark — line/unit/floor all blank/absent.
        assert_eq!(v.address.as_deref(), Some("Tower A, Gate 2"));
    }

    #[test]
    fn order_view_address_none_when_all_absent() {
        let o = order("received", "outside", cart_with_lines(1));
        assert_eq!(order_view(&o, "en").address, None);
    }

    #[test]
    fn order_view_address_none_when_all_blank() {
        let mut o = order("received", "outside", cart_with_lines(1));
        o.place_name = Some(Some("  ".into()));
        o.address_line = Some(Some(String::new()));
        o.floor = Some(Some(" ".into()));
        assert_eq!(order_view(&o, "en").address, None);
    }

    #[test]
    fn order_view_address_unit_only() {
        let mut o = order("received", "outside", cart_with_lines(1));
        o.unit_number = Some(Some("9".into()));
        let v = order_view(&o, "en");
        assert_eq!(v.address.as_deref(), Some("Unit 9"));
    }

    // ---- settings_view --------------------------------------------------

    #[test]
    fn settings_view_projects_fields() {
        let v = settings_view(&settings());
        assert!(v.in_mall_enabled);
        assert_eq!(v.in_mall_override, "auto");
        assert_eq!(v.in_mall_fee_minor, 500);
        assert!(!v.outside_enabled);
        assert_eq!(v.outside_override, "closed");
        assert_eq!(v.prep_time_minutes, 25);
    }

    #[test]
    fn settings_view_zero_prep_and_fee() {
        let mut s = settings();
        s.in_mall_fee = 0;
        s.prep_time_minutes = 0;
        let v = settings_view(&s);
        assert_eq!(v.in_mall_fee_minor, 0);
        assert_eq!(v.prep_time_minutes, 0);
    }

    // ---- next_status: full lifecycle -----------------------------------

    #[test]
    fn next_status_each_forward_step() {
        assert_eq!(next_status("confirmed"), Some("preparing"));
        assert_eq!(next_status("ready"), Some("out_for_delivery"));
    }

    #[test]
    fn next_status_terminal_and_unknown_yield_none() {
        assert_eq!(next_status("rejected"), None);
        assert_eq!(next_status(""), None);
        assert_eq!(next_status("bogus"), None);
        assert_eq!(next_status("DELIVERED"), None); // case-sensitive
    }

    #[test]
    fn next_status_walks_the_whole_chain() {
        let mut cur = "received".to_string();
        let mut seen = vec![cur.clone()];
        while let Some(n) = next_status(&cur) {
            cur = n.to_string();
            seen.push(cur.clone());
        }
        assert_eq!(
            seen,
            // Stops at out_for_delivery — `delivered` is reached via finalize, not /status.
            vec!["received", "confirmed", "preparing", "ready", "out_for_delivery"]
        );
    }
}
