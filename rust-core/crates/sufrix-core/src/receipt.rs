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

use crate::checkout::ReceiptView;

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

/// The localized labels the layout needs — resolved once by the caller.
pub struct ReceiptLabels {
    pub order: String,
    pub subtotal: String,
    pub discount: String,
    pub tax: String,
    pub total: String,
    pub paid: String,
    pub change: String,
    pub queued: String,
    pub thank_you: String,
}

/// Render a receipt to ESC/POS bytes ready to stream to a printer.
pub fn escpos(receipt: &ReceiptView, ctx: &EscPosCtx) -> Vec<u8> {
    encode(&layout(receipt, ctx))
}

/// Build the visible line list — pure, no bytes. Golden-tested.
pub fn layout(receipt: &ReceiptView, ctx: &EscPosCtx) -> Vec<Line> {
    let w = ctx.width.max(16) as usize;
    let cur = &ctx.currency;
    let mut out: Vec<Line> = Vec::new();

    // Header — store name big + centered, then the short order id.
    out.push(Line { text: ctx.store_name.clone(), align: Align::Center, bold: true, size: Size::Double });
    let short = short_id(&receipt.local_order_id);
    out.push(Line::centered(format!("{} {}", ctx.labels.order, short)));
    if !receipt.created_at.is_empty() {
        out.push(Line::centered(receipt.created_at.clone()));
    }
    out.push(Line::plain(divider(w)));

    // Line items — "qty× name" left, line total right-aligned.
    for l in &receipt.lines {
        let left = format!("{}x {}", l.qty, l.name);
        out.push(Line::plain(row(&left, &money(l.line_total_minor, cur), w)));
    }
    out.push(Line::plain(divider(w)));

    // Money summary.
    out.push(Line::plain(row(&ctx.labels.subtotal, &money(receipt.subtotal_minor, cur), w)));
    if receipt.discount_minor > 0 {
        out.push(Line::plain(row(&ctx.labels.discount, &money(-receipt.discount_minor, cur), w)));
    }
    out.push(Line::plain(row(&ctx.labels.tax, &money(receipt.tax_minor, cur), w)));
    out.push(Line {
        text: row(&ctx.labels.total, &money(receipt.total_minor, cur), w),
        align: Align::Left,
        bold: true,
        size: Size::Normal,
    });
    if receipt.is_cash {
        out.push(Line::plain(row(&ctx.labels.paid, &money(receipt.amount_tendered_minor, cur), w)));
        out.push(Line::plain(row(&ctx.labels.change, &money(receipt.change_minor, cur), w)));
    }
    out.push(Line::plain(divider(w)));

    // Payment method, offline hint, thanks.
    out.push(Line::centered(receipt.payment_label.clone()));
    if receipt.queued_offline {
        out.push(Line::centered(ctx.labels.queued.clone()));
    }
    out.push(Line::centered(ctx.labels.thank_you.clone()));
    out
}

// ── ESC/POS byte protocol ────────────────────────────────────────────────────
const ESC: u8 = 0x1B;
const GS: u8 = 0x1D;
const LF: u8 = 0x0A;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkout::{ReceiptLineView, ReceiptView};

    fn ctx() -> EscPosCtx {
        EscPosCtx {
            store_name: "SUFRIX CAFE".into(),
            currency: "EGP".into(),
            width: 32,
            labels: ReceiptLabels {
                order: "Order".into(),
                subtotal: "Subtotal".into(),
                discount: "Discount".into(),
                tax: "Tax".into(),
                total: "Total".into(),
                paid: "Cash received".into(),
                change: "Change".into(),
                queued: "Saved - will sync".into(),
                thank_you: "Thank you!".into(),
            },
        }
    }

    fn cash_receipt() -> ReceiptView {
        ReceiptView {
            local_order_id: "1a2b3c4d-0000-0000-0000-000000000000".into(),
            lines: vec![
                ReceiptLineView { name: "Latte".into(), qty: 2, line_total_minor: 9000 },
                ReceiptLineView { name: "Croissant".into(), qty: 1, line_total_minor: 3500 },
            ],
            payment_label: "Cash".into(),
            subtotal_minor: 12500,
            discount_minor: 0,
            tax_minor: 1750,
            total_minor: 14250,
            tip_minor: 0,
            amount_tendered_minor: 15000,
            change_minor: 750,
            is_cash: true,
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
    fn layout_cash_receipt_matches_expected_lines() {
        let lines = layout(&cash_receipt(), &ctx());
        let text: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(
            text,
            vec![
                "SUFRIX CAFE",
                "Order 1A2B3C4D",
                "2026-06-20T10:00:00Z",
                "--------------------------------",
                "2x Latte               90.00 EGP",
                "1x Croissant           35.00 EGP",
                "--------------------------------",
                "Subtotal              125.00 EGP",
                "Tax                    17.50 EGP",
                "Total                 142.50 EGP",
                "Cash received         150.00 EGP",
                "Change                  7.50 EGP",
                "--------------------------------",
                "Cash",
                "Thank you!",
            ]
        );
        // Header is big + bold; total is bold; the rest are normal.
        assert_eq!(lines[0].size, Size::Double);
        assert!(lines[0].bold);
        let total = lines.iter().find(|l| l.text.starts_with("Total")).unwrap();
        assert!(total.bold);
    }

    #[test]
    fn layout_shows_discount_and_offline_hint_when_present() {
        let mut r = cash_receipt();
        r.discount_minor = 1250;
        r.queued_offline = true;
        let lines = layout(&r, &ctx());
        let discount = lines.iter().find(|l| l.text.starts_with("Discount"));
        assert!(discount.is_some(), "discount line should render when discount > 0");
        assert!(discount.unwrap().text.ends_with("-12.50 EGP"));
        assert!(lines.iter().any(|l| l.text == "Saved - will sync"));
    }

    #[test]
    fn layout_omits_cash_lines_for_card_payments() {
        let mut r = cash_receipt();
        r.is_cash = false;
        let lines = layout(&r, &ctx());
        assert!(!lines.iter().any(|l| l.text.starts_with("Cash received")));
        assert!(!lines.iter().any(|l| l.text.starts_with("Change")));
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
