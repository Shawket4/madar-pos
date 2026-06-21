//! Thermal-receipt rendering (ESC/POS) — all of it in the core so both hosts
//! print byte-identical receipts.
//!
//! The work splits in two so it stays testable without hardware:
//!   1. `layout` — a `ReceiptView` + context → a vector of `Line`s (text +
//!      alignment + weight + size). Pure, human-readable, golden-tested.
//!   2. `encode` — `Line`s → ESC/POS bytes (init, justify, emphasize, size,
//!      feed, cut). The byte protocol; golden-tested against a known fixture.
//!
//! `escpos` is just `encode(layout(..))`. The actual TCP send to a network
//! printer lives in `lib.rs` (async) — unverifiable here without a printer, so
//! the contract this module pins down is the bytes, not the delivery.

use crate::checkout::{ReceiptModifierView, ReceiptView};
use crate::shift::ShiftReportView;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Size {
    Normal,
    Double,
}

/// One printed line: visible text plus how to render it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Line {
    pub text: String,
    pub align: Align,
    pub bold: bool,
    pub size: Size,
}

impl Line {
    fn plain(text: impl Into<String>) -> Self {
        Line { text: text.into(), align: Align::Left, bold: false, size: Size::Normal }
    }
    fn centered(text: impl Into<String>) -> Self {
        Line { text: text.into(), align: Align::Center, bold: false, size: Size::Normal }
    }
}

/// Localized strings + presentation knobs for one receipt. The FFI fills
/// `labels` from the i18n table so this module stays string-literal-free.
pub struct EscPosCtx {
    pub store_name: String,
    pub currency: String,
    /// Receipt width in monospace columns (58mm ≈ 32, 80mm ≈ 48).
    pub width: u32,
    pub labels: ReceiptLabels,
}

/// The localized labels the layout needs — resolved once by the caller. Mirrors
/// every label printed by Flutter's `printer_service.dart` receipt.
pub struct ReceiptLabels {
    pub order: String,
    pub reference: String,
    pub voided: String,
    pub delivery: String,
    pub channel_in_mall: String,
    pub channel_outside: String,
    pub customer: String,
    pub phone: String,
    pub address: String,
    pub zone: String,
    pub delivery_ref: String,
    pub payment_hint: String,
    pub notes: String,
    pub subtotal: String,
    pub discount: String,
    pub tax: String,
    pub delivery_fee: String,
    pub total: String,
    pub payment: String,
    pub teller: String,
    pub queued: String,
    pub thank_you: String,
}

/// Render a receipt to ESC/POS bytes ready to stream to a printer.
pub fn escpos(receipt: &ReceiptView, ctx: &EscPosCtx) -> Vec<u8> {
    encode(&layout(receipt, ctx))
}

/// Build the visible line list — pure, no bytes. Golden-tested. Reproduces the
/// Flutter `printer_service.dart` receipt structure top-to-bottom: header +
/// delivery flag, order meta, delivery block, item lines with their full
/// modifier/bundle breakdown, totals, payment/teller footer, thank-you.
pub fn layout(receipt: &ReceiptView, ctx: &EscPosCtx) -> Vec<Line> {
    let w = ctx.width.max(16) as usize;
    let cur = &ctx.currency;
    let lab = &ctx.labels;
    let mut out: Vec<Line> = Vec::new();
    let bold_left = |text: String| Line { text, align: Align::Left, bold: true, size: Size::Normal };
    let bold_center = |text: String| Line { text, align: Align::Center, bold: true, size: Size::Normal };

    // ── header ──────────────────────────────────────────────────────────────
    // Branch name as the brand (big + bold), then a delivery flag for delivery.
    out.push(Line { text: ctx.store_name.clone(), align: Align::Center, bold: true, size: Size::Double });
    if receipt.is_delivery {
        let chan = match receipt.delivery_channel.as_deref() {
            Some("in_mall") => Some(lab.channel_in_mall.as_str()),
            Some("outside") => Some(lab.channel_outside.as_str()),
            _ => None,
        };
        let flag = match chan {
            Some(c) => format!("*** {} — {} ***", lab.delivery, c),
            None => format!("*** {} ***", lab.delivery),
        };
        out.push(bold_center(flag));
    }
    out.push(Line::plain(divider(w)));

    // ── order meta ──────────────────────────────────────────────────────────
    if receipt.is_voided {
        out.push(bold_center(format!("*** {} ***", lab.voided)));
    }
    let dt = fmt_dt(&receipt.created_at);
    let id_label = match receipt.order_number {
        Some(n) => format!("{} #{}", lab.order, n),
        None => format!("{} {}", lab.order, short_id(&receipt.local_order_id)),
    };
    out.push(bold_left(row(&id_label, &dt, w)));
    if let Some(r) = &receipt.order_ref {
        out.push(bold_left(row(&format!("{} {}", lab.reference, r), &dt, w)));
    }
    out.push(Line::plain(divider(w)));

    // ── delivery block ──────────────────────────────────────────────────────
    if receipt.is_delivery {
        if let Some(v) = &receipt.customer_name {
            out.push(Line::plain(row(&lab.customer, v, w)));
        }
        if let Some(v) = &receipt.customer_phone {
            out.push(Line::plain(row(&lab.phone, v, w)));
        }
        if let Some(v) = &receipt.delivery_address {
            out.push(Line::plain(format!("{} {}", lab.address, v)));
        }
        if let Some(v) = &receipt.delivery_zone {
            out.push(Line::plain(row(&lab.zone, v, w)));
        }
        if let Some(v) = &receipt.delivery_ref {
            out.push(Line::plain(row(&lab.delivery_ref, v, w)));
        }
        if let Some(v) = &receipt.payment_hint {
            out.push(Line::plain(row(&lab.payment_hint, v, w)));
        }
        if let Some(v) = &receipt.delivery_notes {
            out.push(Line::plain(format!("{} {}", lab.notes, v)));
        }
        out.push(Line::plain(divider(w)));
    }

    // ── items ───────────────────────────────────────────────────────────────
    for l in &receipt.lines {
        let name = match &l.size_label {
            Some(s) => format!("{} ({})", l.name, s),
            None => l.name.clone(),
        };
        out.push(bold_left(row(&format!("{}x {}", l.qty, name), &money(l.line_total_minor, cur), w)));
        if l.is_bundle {
            for c in &l.components {
                let cname = match &c.size_label {
                    Some(s) => format!("{} ({})", c.name, s),
                    None => c.name.clone(),
                };
                out.push(Line::plain(format!("  - {}", cname)));
                for m in &c.addons {
                    push_modifier(&mut out, "    + ", m, cur, w);
                }
                for m in &c.optionals {
                    push_modifier(&mut out, "    + ", m, cur, w);
                }
            }
        } else {
            for m in &l.addons {
                push_modifier(&mut out, "  + ", m, cur, w);
            }
            for m in &l.optionals {
                push_modifier(&mut out, "  + ", m, cur, w);
            }
        }
    }
    out.push(Line::plain(divider(w)));

    // ── totals ──────────────────────────────────────────────────────────────
    if receipt.discount_minor > 0 || receipt.delivery_fee_minor > 0 {
        out.push(Line::plain(row(&lab.subtotal, &money(receipt.subtotal_minor, cur), w)));
    }
    if receipt.discount_minor > 0 {
        out.push(Line::plain(row(&lab.discount, &money(-receipt.discount_minor, cur), w)));
    }
    if receipt.tax_minor > 0 {
        out.push(Line::plain(row(&lab.tax, &money(receipt.tax_minor, cur), w)));
    }
    if receipt.delivery_fee_minor > 0 {
        out.push(Line::plain(row(&lab.delivery_fee, &money(receipt.delivery_fee_minor, cur), w)));
    }
    out.push(bold_left(row(&lab.total, &money(receipt.total_minor, cur), w)));
    out.push(Line::plain(divider(w)));

    // ── footer ──────────────────────────────────────────────────────────────
    out.push(Line::plain(row(&lab.payment, &receipt.payment_label, w)));
    if !receipt.is_delivery {
        if let Some(v) = &receipt.customer_name {
            out.push(Line::plain(row(&lab.customer, v, w)));
        }
    }
    if let Some(v) = &receipt.teller_name {
        out.push(Line::plain(row(&lab.teller, v, w)));
    }
    if receipt.queued_offline {
        out.push(Line::centered(lab.queued.clone()));
    }
    out.push(Line::centered(lab.thank_you.clone()));
    out
}

/// Push one modifier (addon/optional) line: `{prefix}{name}` with the charge
/// right-aligned when it's priced, else just the name.
fn push_modifier(out: &mut Vec<Line>, prefix: &str, m: &ReceiptModifierView, cur: &str, w: usize) {
    let left = format!("{}{}", prefix, m.name);
    if m.price_minor > 0 {
        out.push(Line::plain(row(&left, &format!("+{}", money(m.price_minor, cur)), w)));
    } else {
        out.push(Line::plain(left));
    }
}

// ── shift report (Z-report) ──────────────────────────────────────────────────

/// Localized labels for the Z-report — resolved once by the FFI caller.
pub struct ShiftReportLabels {
    pub title: String,
    pub opening: String,
    pub payments: String,
    pub cash_moves: String,
    pub cash_in: String,
    pub cash_out: String,
    pub expected: String,
    pub voided: String,
    pub by_method: String,
}

/// Render the shift report (Z-report) to ESC/POS bytes.
pub fn escpos_shift_report(
    report: &ShiftReportView,
    store: &str,
    currency: &str,
    width: u32,
    labels: &ShiftReportLabels,
) -> Vec<u8> {
    encode(&layout_shift_report(report, store, currency, width, labels))
}

/// Build the Z-report's visible lines — pure, no bytes. Golden-tested.
pub fn layout_shift_report(
    report: &ShiftReportView,
    store: &str,
    currency: &str,
    width: u32,
    labels: &ShiftReportLabels,
) -> Vec<Line> {
    let w = (width.max(16)) as usize;
    let cur = currency;
    let mut out: Vec<Line> = Vec::new();

    out.push(Line { text: store.to_string(), align: Align::Center, bold: true, size: Size::Double });
    out.push(Line::centered(labels.title.clone()));
    out.push(Line::plain(divider(w)));

    // Drawer reconciliation — opening, sales, then pay-in / pay-out separately.
    out.push(Line::plain(row(&labels.opening, &money(report.opening_cash_minor, cur), w)));
    out.push(Line::plain(row(&labels.payments, &money(report.net_payments_minor, cur), w)));
    if report.cash_in_minor > 0 {
        out.push(Line::plain(row(&labels.cash_in, &money(report.cash_in_minor, cur), w)));
    }
    if report.cash_out_minor > 0 {
        out.push(Line::plain(row(&labels.cash_out, &money(-report.cash_out_minor, cur), w)));
    }
    out.push(Line {
        text: row(&labels.expected, &money(report.expected_cash_minor, cur), w),
        align: Align::Left,
        bold: true,
        size: Size::Normal,
    });

    // Itemised cash movements (each pay-in/out with its note).
    if !report.cash_movements.is_empty() {
        out.push(Line::plain(divider(w)));
        out.push(Line::plain(labels.cash_moves.clone()));
        for m in &report.cash_movements {
            let label = if m.note.trim().is_empty() { m.moved_by_name.clone() } else { m.note.clone() };
            out.push(Line::plain(row(&format!("  {label}"), &money(m.amount_minor, cur), w)));
        }
    }
    out.push(Line::plain(divider(w)));

    // Payment breakdown — "method (n orders)" left, total right.
    out.push(Line::plain(labels.by_method.clone()));
    for pl in &report.payment_lines {
        let left = format!("{} ({})", pl.method, pl.order_count);
        out.push(Line::plain(row(&left, &money(pl.total_minor, cur), w)));
    }
    if report.voided_amount_minor > 0 {
        out.push(Line::plain(divider(w)));
        out.push(Line::plain(row(&labels.voided, &money(report.voided_amount_minor, cur), w)));
    }
    out
}

// ── ESC/POS byte protocol ────────────────────────────────────────────────────
const ESC: u8 = 0x1B;
const GS: u8 = 0x1D;
const LF: u8 = 0x0A;

/// ESC/POS cash-drawer kick: `ESC p m t1 t2` pulses connector pin 2 (the
/// standard drawer line). Sent right after a CASH sale prints so the till pops.
/// Star and Epson both honour this; m=0 = pin 2, t1/t2 = on/off pulse widths.
pub fn drawer_kick_bytes() -> Vec<u8> {
    vec![ESC, b'p', 0x00, 0x19, 0xFA]
}

/// Encode visible lines to ESC/POS: init, then per-line justify/emphasize/size,
/// then feed + a partial cut. Deterministic — golden-tested.
pub fn encode(lines: &[Line]) -> Vec<u8> {
    let mut b: Vec<u8> = Vec::new();
    b.extend_from_slice(&[ESC, b'@']); // ESC @  — initialize
    for line in lines {
        let a = match line.align {
            Align::Left => 0,
            Align::Center => 1,
            Align::Right => 2,
        };
        b.extend_from_slice(&[ESC, b'a', a]); // ESC a n — justification
        b.extend_from_slice(&[ESC, b'E', line.bold as u8]); // ESC E n — emphasize
        let sz = match line.size {
            Size::Normal => 0x00,
            Size::Double => 0x11, // double width | double height
        };
        b.extend_from_slice(&[GS, b'!', sz]); // GS ! n — character size
        b.extend_from_slice(line.text.as_bytes());
        b.push(LF);
    }
    // Reset to a sane state, feed, partial cut.
    b.extend_from_slice(&[ESC, b'a', 0]);
    b.extend_from_slice(&[ESC, b'E', 0]);
    b.extend_from_slice(&[GS, b'!', 0]);
    b.extend_from_slice(&[LF, LF, LF]);
    b.extend_from_slice(&[GS, b'V', 66, 0]); // GS V 66 n — partial cut, feed n
    b
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Minor units → "x.yy" with the currency code suffixed. Handles negatives.
fn money(minor: i64, currency: &str) -> String {
    let neg = minor < 0;
    let abs = minor.unsigned_abs();
    let amount = format!("{}.{:02}", abs / 100, abs % 100);
    if currency.is_empty() {
        if neg {
            format!("-{amount}")
        } else {
            amount
        }
    } else if neg {
        format!("-{amount} {currency}")
    } else {
        format!("{amount} {currency}")
    }
}

/// A full-width rule.
fn divider(width: usize) -> String {
    "-".repeat(width)
}

/// Left text + right text on one `width`-column line, right value flush-right.
/// The left is truncated (never the value) so columns always line up.
fn row(left: &str, right: &str, width: usize) -> String {
    let rc = right.chars().count();
    if rc >= width {
        return right.chars().take(width).collect();
    }
    let max_left = width - rc - 1; // keep ≥1 space between columns
    let l: String = left.chars().take(max_left).collect();
    let pad = width - l.chars().count() - rc;
    format!("{}{}{}", l, " ".repeat(pad), right)
}

/// First segment of the order UUID — enough to identify a ticket by eye.
fn short_id(id: &str) -> String {
    id.split('-').next().unwrap_or(id).to_uppercase()
}

/// Format an RFC3339 timestamp the way Flutter's receipt does —
/// `dd/MM/yyyy hh:mm a`, in the timestamp's own offset. Falls back to the raw
/// string if it can't be parsed.
fn fmt_dt(rfc3339: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(rfc3339) {
        Ok(dt) => dt.format("%d/%m/%Y %I:%M %p").to_string(),
        Err(_) => rfc3339.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkout::{ReceiptComponentView, ReceiptLineView, ReceiptView};

    #[test]
    fn shift_report_layout_has_drawer_lines_and_methods() {
        let report = ShiftReportView {
            expected_cash_minor: 15000,
            opening_cash_minor: 10000,
            total_payments_minor: 8000,
            net_payments_minor: 8000,
            voided_amount_minor: 500,
            cash_movements_net_minor: 1000,
            cash_in_minor: 3000,
            cash_out_minor: 2000,
            payment_lines: vec![crate::shift::ShiftReportPaymentLine {
                method: "Cash".into(),
                is_cash: true,
                order_count: 3,
                total_minor: 5000,
            }],
            cash_movements: vec![
                crate::shift::ShiftReportCashLine { amount_minor: 3000, note: "float".into(), moved_by_name: "Mona".into(), created_at: "t".into() },
                crate::shift::ShiftReportCashLine { amount_minor: -2000, note: "".into(), moved_by_name: "Mona".into(), created_at: "t".into() },
            ],
            from_server: true,
        };
        let labels = ShiftReportLabels {
            title: "Shift Report".into(),
            opening: "Opening".into(),
            payments: "Payments".into(),
            cash_moves: "Cash moves".into(),
            cash_in: "Cash in".into(),
            cash_out: "Cash out".into(),
            expected: "Expected".into(),
            voided: "Voided".into(),
            by_method: "By method".into(),
        };
        let lines = layout_shift_report(&report, "Cafe Sufrix", "EGP", 32, &labels);
        let joined: String = lines.iter().map(|l| l.text.clone()).collect::<Vec<_>>().join("\n");
        assert!(joined.contains("Cafe Sufrix"));
        assert!(joined.contains("Opening"));
        assert!(joined.contains("Cash (3)")); // method + order count
        // The expected-cash line is emphasized (bold).
        assert!(lines.iter().any(|l| l.bold && l.text.contains("Expected")));
        // Separate pay-in / pay-out totals.
        assert!(lines.iter().any(|l| l.text.starts_with("Cash in") && l.text.ends_with("30.00 EGP")));
        assert!(lines.iter().any(|l| l.text.starts_with("Cash out") && l.text.ends_with("-20.00 EGP")));
        // Itemised movements: the noted one shows its note, the blank one the name.
        assert!(joined.contains("float") && joined.contains("Mona"));
    }

    fn ctx() -> EscPosCtx {
        EscPosCtx {
            store_name: "SUFRIX CAFE".into(),
            currency: "EGP".into(),
            width: 32,
            labels: ReceiptLabels {
                order: "Order".into(),
                reference: "Ref:".into(),
                voided: "VOIDED".into(),
                delivery: "DELIVERY".into(),
                channel_in_mall: "In-Mall".into(),
                channel_outside: "Outside".into(),
                customer: "Customer".into(),
                phone: "Phone".into(),
                address: "Address:".into(),
                zone: "Zone".into(),
                delivery_ref: "Delivery Ref".into(),
                payment_hint: "Payment (hint)".into(),
                notes: "Notes:".into(),
                subtotal: "Subtotal".into(),
                discount: "Discount".into(),
                tax: "Tax".into(),
                delivery_fee: "Delivery Fee".into(),
                total: "Total".into(),
                payment: "Payment".into(),
                teller: "Teller".into(),
                queued: "Saved - will sync".into(),
                thank_you: "Thank you!".into(),
            },
        }
    }

    fn line(name: &str, qty: i64, total: i64) -> ReceiptLineView {
        ReceiptLineView {
            name: name.into(),
            qty,
            size_label: None,
            line_total_minor: total,
            is_bundle: false,
            addons: vec![],
            optionals: vec![],
            components: vec![],
        }
    }

    fn cash_receipt() -> ReceiptView {
        ReceiptView {
            local_order_id: "1a2b3c4d-0000-0000-0000-000000000000".into(),
            order_number: None,
            order_ref: None,
            is_voided: false,
            lines: vec![line("Latte", 2, 9000), line("Croissant", 1, 3500)],
            payment_label: "Cash".into(),
            subtotal_minor: 12500,
            discount_minor: 0,
            tax_minor: 1750,
            delivery_fee_minor: 0,
            total_minor: 14250,
            tip_minor: 0,
            amount_tendered_minor: 15000,
            change_minor: 750,
            is_cash: true,
            customer_name: None,
            teller_name: None,
            is_delivery: false,
            delivery_channel: None,
            customer_phone: None,
            delivery_address: None,
            delivery_zone: None,
            delivery_ref: None,
            payment_hint: None,
            delivery_notes: None,
            queued_offline: false,
            created_at: "2026-06-20T10:00:00Z".into(),
        }
    }

    #[test]
    fn money_formats_minor_units_with_currency_and_sign() {
        assert_eq!(money(14250, "EGP"), "142.50 EGP");
        assert_eq!(money(5, "EGP"), "0.05 EGP");
        assert_eq!(money(-1250, "EGP"), "-12.50 EGP");
        assert_eq!(money(0, ""), "0.00");
    }

    #[test]
    fn row_right_aligns_value_to_full_width() {
        let r = row("Total", "142.50 EGP", 32);
        assert_eq!(r.chars().count(), 32);
        assert!(r.starts_with("Total"));
        assert!(r.ends_with("142.50 EGP"));
    }

    #[test]
    fn row_truncates_long_left_never_the_value() {
        let r = row("A really really long item name here", "9.00 EGP", 24);
        assert_eq!(r.chars().count(), 24);
        assert!(r.ends_with("9.00 EGP"));
        assert!(r.contains(' ')); // at least one column gap survives
    }

    #[test]
    fn layout_cash_receipt_matches_flutter_structure() {
        let lines = layout(&cash_receipt(), &ctx());
        let text: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        // Header (brand), then the order# + formatted datetime row, divider,
        // the two item rows, divider, Tax + Total (no Subtotal — no discount or
        // delivery fee), divider, the Payment footer row, then thanks.
        assert_eq!(text[0], "SUFRIX CAFE");
        let order_row = text.iter().find(|t| t.contains("20/06/2026")).expect("order row");
        assert!(order_row.starts_with("Order 1A2B3C")); // order id (right side may truncate at 32 cols)
        assert!(order_row.contains("20/06/2026")); // dd/MM/yyyy, not raw RFC3339
        assert!(text.iter().any(|t| t.starts_with("2x Latte") && t.ends_with("90.00 EGP")));
        assert!(text.iter().any(|t| t.starts_with("1x Croissant") && t.ends_with("35.00 EGP")));
        // No discount/delivery fee → no Subtotal row; Tax + Total present.
        assert!(!text.iter().any(|t| t.starts_with("Subtotal")));
        assert!(text.iter().any(|t| t.starts_with("Tax") && t.ends_with("17.50 EGP")));
        assert!(text.iter().any(|t| t.starts_with("Total") && t.ends_with("142.50 EGP")));
        // Flutter prints the payment method as a labelled row, not tendered/change.
        assert!(text.iter().any(|t| t.starts_with("Payment") && t.ends_with("Cash")));
        assert!(!text.iter().any(|t| t.contains("150.00"))); // no amount-tendered line
        assert_eq!(*text.last().unwrap(), "Thank you!");
        // Header big+bold; the order row and total are bold.
        assert_eq!(lines[0].size, Size::Double);
        assert!(lines[0].bold);
        assert!(lines.iter().find(|l| l.text.contains("20/06/2026")).unwrap().bold);
        assert!(lines.iter().find(|l| l.text.starts_with("Total")).unwrap().bold);
    }

    #[test]
    fn layout_shows_subtotal_discount_and_offline_hint_when_present() {
        let mut r = cash_receipt();
        r.discount_minor = 1250;
        r.queued_offline = true;
        let lines = layout(&r, &ctx());
        // A discount forces the Subtotal row to appear.
        assert!(lines.iter().any(|l| l.text.starts_with("Subtotal")));
        let discount = lines.iter().find(|l| l.text.starts_with("Discount")).expect("discount row");
        assert!(discount.text.ends_with("-12.50 EGP"));
        assert!(lines.iter().any(|l| l.text == "Saved - will sync"));
    }

    #[test]
    fn layout_voided_stamp_and_teller_footer() {
        let mut r = cash_receipt();
        r.is_voided = true;
        r.teller_name = Some("Mona".into());
        r.order_number = Some(412);
        let lines = layout(&r, &ctx());
        assert!(lines.iter().any(|l| l.text == "*** VOIDED ***" && l.bold));
        assert!(lines.iter().any(|l| l.text.starts_with("Order #412")));
        assert!(lines.iter().any(|l| l.text.starts_with("Teller") && l.text.ends_with("Mona")));
    }

    #[test]
    fn layout_delivery_with_modifiers_and_bundle() {
        let mut r = cash_receipt();
        r.is_delivery = true;
        r.delivery_channel = Some("outside".into());
        r.customer_name = Some("Sara".into());
        r.customer_phone = Some("0100".into());
        r.delivery_address = Some("Flat 2, Nile St".into());
        r.delivery_fee_minor = 2000;
        // A regular line with a priced addon + a free optional.
        let mut latte = line("Latte", 1, 5000);
        latte.size_label = Some("Large".into());
        latte.addons = vec![ReceiptModifierView { name: "Oat milk".into(), price_minor: 800 }];
        latte.optionals = vec![ReceiptModifierView { name: "No sugar".into(), price_minor: 0 }];
        // A bundle line with one configured component.
        let mut combo = line("Breakfast Combo", 1, 12000);
        combo.is_bundle = true;
        combo.components = vec![ReceiptComponentView {
            name: "Eggs".into(),
            size_label: None,
            addons: vec![ReceiptModifierView { name: "Cheese".into(), price_minor: 500 }],
            optionals: vec![],
        }];
        r.lines = vec![latte, combo];
        let lines = layout(&r, &ctx());
        let text: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        // Delivery flag with the channel label.
        assert!(text.iter().any(|t| *t == "*** DELIVERY — Outside ***"));
        // Delivery block: customer / phone / address.
        assert!(text.iter().any(|t| t.starts_with("Customer") && t.ends_with("Sara")));
        assert!(text.iter().any(|t| t.starts_with("Address:") && t.contains("Nile St")));
        // Size inline on the item; a priced addon line ("+8.00") and a free optional.
        assert!(text.iter().any(|t| t.starts_with("1x Latte (Large)")));
        assert!(text.iter().any(|t| t.trim_start().starts_with("+ Oat milk") && t.contains("+8.00 EGP")));
        assert!(text.iter().any(|t| t.trim() == "+ No sugar")); // free → no price
        // Bundle breakdown: a "- component" line and an indented "+ addon".
        assert!(text.iter().any(|t| t.trim() == "- Eggs"));
        assert!(text.iter().any(|t| t.trim_start().starts_with("+ Cheese") && t.contains("+5.00 EGP")));
        // Delivery fee forces a Subtotal row and prints the fee.
        assert!(text.iter().any(|t| t.starts_with("Subtotal")));
        assert!(text.iter().any(|t| t.starts_with("Delivery Fee") && t.ends_with("20.00 EGP")));
        // Delivery orders print customer in the block, not the footer (no dup).
        assert_eq!(text.iter().filter(|t| t.starts_with("Customer")).count(), 1);
    }

    #[test]
    fn drawer_kick_is_the_escpos_pulse_command() {
        // ESC p 0 25 250 — the standard pin-2 drawer pulse.
        assert_eq!(drawer_kick_bytes(), vec![0x1B, b'p', 0x00, 0x19, 0xFA]);
    }

    #[test]
    fn encode_frames_with_init_and_cut() {
        let bytes = encode(&[Line::centered("HI")]);
        // Starts with ESC @ (initialize).
        assert_eq!(&bytes[0..2], &[0x1B, 0x40]);
        // Ends with GS V 66 0 (partial cut).
        let n = bytes.len();
        assert_eq!(&bytes[n - 4..], &[0x1D, 0x56, 66, 0]);
        // The visible text survives verbatim.
        assert!(bytes.windows(2).any(|w| w == b"HI"));
    }

    #[test]
    fn encode_emits_size_and_emphasis_control_codes() {
        let bytes = encode(&[Line { text: "BIG".into(), align: Align::Center, bold: true, size: Size::Double }]);
        // GS ! 0x11 (double) and ESC E 1 (bold) both present.
        assert!(bytes.windows(3).any(|w| w == [0x1D, b'!', 0x11]));
        assert!(bytes.windows(3).any(|w| w == [0x1B, b'E', 0x01]));
        // ESC a 1 (center).
        assert!(bytes.windows(3).any(|w| w == [0x1B, b'a', 0x01]));
    }
}
