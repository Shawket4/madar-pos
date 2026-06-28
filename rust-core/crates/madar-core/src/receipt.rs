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

/// Which thermal-printer command dialect to emit. Epson (ESC/POS) and Star
/// (Star Line Mode) are NOT byte-compatible — different alignment, character
/// size, cut and drawer-kick commands. The host picks this in Settings.
#[derive(uniffi::Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum PrinterBrand {
    Epson,
    Star,
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
    pub tip: String,
    pub cash: String,
    pub change: String,
    pub payment: String,
    pub teller: String,
    pub served_by: String,
    pub queued: String,
    pub thank_you: String,
}

/// Render a receipt to ESC/POS bytes ready to stream to a printer.
pub fn escpos(receipt: &ReceiptView, ctx: &EscPosCtx, brand: PrinterBrand) -> Vec<u8> {
    encode_for(brand, &layout(receipt, ctx))
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

/// Localized labels for the Z-report — resolved once by the FFI caller. Covers
/// the full detailed layout (shift header, payments, drawer ops, reconciliation)
/// matching the Flutter Z-report. (`cash_moves`/`by_method` are retained for the
/// legacy text `layout_shift_report`, now unused by the FFI.)
pub struct ShiftReportLabels {
    pub title: String,
    pub business_date: String,
    pub printed_at: String,
    pub teller: String,
    pub opened: String,
    pub closed: String,
    pub interim: String,
    pub payments: String,
    pub orders: String,
    pub total_collected: String,
    pub drawer_ops: String,
    pub cash_in: String,
    pub cash_out: String,
    pub cash_recon: String,
    pub opening: String,
    pub opening_mismatch: String,
    pub opening_reason: String,
    pub expected: String,
    pub actual: String,
    pub not_closed: String,
    pub difference: String,
    pub short_by: String,
    pub over_by: String,
    pub voided: String,
    pub transactions: String,
    pub end_of_report: String,
    pub cash_moves: String,
    pub by_method: String,
}

/// Render the shift report (Z-report) to ESC/POS bytes.
pub fn escpos_shift_report(
    report: &ShiftReportView,
    store: &str,
    currency: &str,
    width: u32,
    labels: &ShiftReportLabels,
    brand: PrinterBrand,
) -> Vec<u8> {
    encode_for(brand, &layout_shift_report(report, store, currency, width, labels))
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

/// Dispatch to the right printer dialect.
pub fn encode_for(brand: PrinterBrand, lines: &[Line]) -> Vec<u8> {
    match brand {
        PrinterBrand::Epson => encode(lines),
        PrinterBrand::Star => encode_star(lines),
    }
}

/// Cash-drawer kick for the chosen dialect.
pub fn drawer_kick_for(brand: PrinterBrand) -> Vec<u8> {
    match brand {
        PrinterBrand::Epson => drawer_kick_bytes(),
        PrinterBrand::Star => drawer_kick_star(),
    }
}

/// Encode visible lines to **Star Line Mode** — Star's command set is NOT
/// ESC/POS: alignment is `ESC GS a n` (not `ESC a`), character size is
/// `ESC i h w` (not `GS !`), bold is `ESC E`/`ESC F` (a separate cancel code,
/// not `ESC E n`), and the cut is `ESC d 3` (not `GS V`). `ESC @` init is shared.
/// Deterministic — golden-tested. (Per Star's "Star Line Mode Command
/// Specifications"; values match the StarPRNT / node-thermal-printer STAR set.)
pub fn encode_star(lines: &[Line]) -> Vec<u8> {
    let mut b: Vec<u8> = Vec::new();
    b.extend_from_slice(&[ESC, b'@']); // ESC @ — initialize
    for line in lines {
        let a = match line.align {
            Align::Left => 0,
            Align::Center => 1,
            Align::Right => 2,
        };
        b.extend_from_slice(&[ESC, GS, b'a', a]); // ESC GS a n — Star justification
        // Star uses two codes: ESC E enables emphasis, ESC F cancels it.
        b.extend_from_slice(&[ESC, if line.bold { b'E' } else { b'F' }]);
        let (h, w) = match line.size {
            Size::Normal => (0u8, 0u8),
            Size::Double => (1u8, 1u8),
        };
        b.extend_from_slice(&[ESC, b'i', h, w]); // ESC i h w — Star character expansion
        b.extend_from_slice(line.text.as_bytes());
        b.push(LF);
    }
    // Reset to a sane state, feed, partial cut.
    b.extend_from_slice(&[ESC, GS, b'a', 0]);
    b.extend_from_slice(&[ESC, b'F']);
    b.extend_from_slice(&[ESC, b'i', 0, 0]);
    b.extend_from_slice(&[LF, LF, LF]);
    b.extend_from_slice(&[ESC, b'd', 3]); // ESC d 3 — Star partial cut
    b
}

/// Star cash-drawer kick: a bare `BEL` (0x07) is the "external device 1 drive"
/// instruction that actually fires the drawer (per STAR Graphic Mode; `ESC BEL
/// n1 n2` only *sets* the pulse width and never drives). This matches the Flutter
/// app's confirmed-working kick (`socket.add([0x07])`) on the TSP143III.
pub fn drawer_kick_star() -> Vec<u8> {
    vec![0x07] // BEL — drive external device 1 (the drawer)
}

// ── raster (bitmap) protocol ─────────────────────────────────────────────────
//
// The TSP143III (and any logo/Arabic receipt) can't be driven by text commands —
// the whole receipt is rendered to a 1-bit bitmap and shipped as raster. These
// wrappers take the rendered bitmap and frame it for each brand. Both confirmed
// against real hardware; the framing is deterministic and golden-tested here,
// while the pixel rendering lives in the (font-dependent) layout layer.

/// A 1-bit raster image: `width` dots across, `rows` tall, rows packed MSB-first
/// (leftmost dot = 0x80), `1` = black. `bytes.len() == row_bytes() * rows`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bitmap {
    pub width: u32,
    pub rows: u32,
    pub bytes: Vec<u8>,
}

impl Bitmap {
    /// Bytes per packed row.
    pub fn row_bytes(&self) -> usize {
        ((self.width as usize) + 7) / 8
    }
}

/// Wrap a 1-bit bitmap in **Star Graphic (raster) Mode** for a TSP100/TSP143
/// printer. Sequence (per STAR Graphic Mode Command Specifications, confirmed on
/// a TSP143IIILAN): init, enter raster, continuous page, EOT = partial cut+feed,
/// one `b` (auto-line-feed) command per dot row, execute EOT (print + cut), quit.
/// Note the dual encoding: `ESC * r …` params are ASCII digits + NUL, but the
/// `b` length `n1 n2` is binary.
pub fn star_raster(bmp: &Bitmap) -> Vec<u8> {
    let wb = bmp.row_bytes();
    let mut b: Vec<u8> = Vec::with_capacity(bmp.bytes.len() + bmp.rows as usize * 3 + 24);
    b.extend_from_slice(&[ESC, b'@']); // ESC @ — initialize
    b.extend_from_slice(&[ESC, b'*', b'r', b'A']); // ESC * r A — enter raster mode
    b.extend_from_slice(&[ESC, b'*', b'r', b'P', b'0', 0x00]); // ESC * r P "0" NUL — continuous
    b.extend_from_slice(&[ESC, b'*', b'r', b'E', b'1', b'3', 0x00]); // ESC * r E "13" NUL — partial cut + feed
    for row in bmp.bytes.chunks(wb) {
        b.push(b'b'); // transfer raster data, auto line feed
        b.push((wb & 0xFF) as u8); // n1 (binary)
        b.push(((wb >> 8) & 0xFF) as u8); // n2 (binary)
        b.extend_from_slice(row);
    }
    b.extend_from_slice(&[ESC, 0x0C, 0x04]); // ESC FF EOT — print buffer + execute EOT (cut)
    b.extend_from_slice(&[ESC, b'*', b'r', b'B']); // ESC * r B — quit raster mode
    b
}

/// Wrap a 1-bit bitmap in **ESC/POS `GS v 0`** raster for an Epson printer (or a
/// Star unit in ESC/POS emulation). Init, the raster block (`m`=0, `xL xH` =
/// bytes/row, `yL yH` = rows), then feed + partial cut. Mirrors the Flutter
/// `_pngToEscPos` epilogue (`ESC d 5`, `GS V 65 5`).
pub fn escpos_raster(bmp: &Bitmap) -> Vec<u8> {
    let wb = bmp.row_bytes();
    let rows = bmp.rows as usize;
    let mut b: Vec<u8> = Vec::with_capacity(bmp.bytes.len() + 20);
    b.extend_from_slice(&[ESC, b'@']); // ESC @ — initialize
    b.extend_from_slice(&[
        GS,
        b'v',
        b'0',
        0x00, // m — normal
        (wb & 0xFF) as u8,
        ((wb >> 8) & 0xFF) as u8, // xL xH — bytes per row
        (rows & 0xFF) as u8,
        ((rows >> 8) & 0xFF) as u8, // yL yH — number of rows
    ]);
    b.extend_from_slice(&bmp.bytes);
    b.extend_from_slice(&[ESC, b'd', 0x05]); // ESC d 5 — feed 5 lines
    b.extend_from_slice(&[GS, b'V', 0x41, 0x05]); // GS V 65 5 — partial cut with feed
    b
}

/// Wrap a rendered bitmap for the chosen brand.
pub fn raster_for(brand: PrinterBrand, bmp: &Bitmap) -> Vec<u8> {
    match brand {
        PrinterBrand::Epson => escpos_raster(bmp),
        PrinterBrand::Star => star_raster(bmp),
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Minor units → "x.yy" with the currency code suffixed. Handles negatives.
pub(crate) fn money(minor: i64, currency: &str) -> String {
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
pub(crate) fn short_id(id: &str) -> String {
    id.split('-').next().unwrap_or(id).to_uppercase()
}

/// Format an RFC3339 timestamp the way Flutter's receipt does —
/// `dd/MM/yyyy hh:mm a`, in the timestamp's own offset. Falls back to the raw
/// string if it can't be parsed.
pub(crate) fn fmt_dt(rfc3339: &str) -> String {
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
            teller_name: "Mona".into(),
            opened_at: "t".into(),
            closed_at: None,
            printed_at: "t".into(),
            is_open: true,
            opening_cash_was_edited: false,
            opening_cash_original_minor: None,
            opening_cash_edit_reason: None,
            closing_cash_declared_minor: None,
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
        let labels = z_labels();
        let lines = layout_shift_report(&report, "Cafe Madar", "EGP", 32, &labels);
        let joined: String = lines.iter().map(|l| l.text.clone()).collect::<Vec<_>>().join("\n");
        assert!(joined.contains("Cafe Madar"));
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
            store_name: "MADAR CAFE".into(),
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
                tip: "Tip".into(),
                cash: "Cash".into(),
                change: "Change".into(),
                payment: "Payment".into(),
                teller: "Teller".into(),
                served_by: "Served by".into(),
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
        assert_eq!(text[0], "MADAR CAFE");
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

    // ── money() formatting: sign, no-currency, rounding boundaries ────────────

    #[test]
    fn money_handles_zero_thousands_and_exact_currency_suffix() {
        assert_eq!(money(0, "EGP"), "0.00 EGP");
        assert_eq!(money(100, "EGP"), "1.00 EGP"); // whole unit
        assert_eq!(money(99, "EGP"), "0.99 EGP"); // sub-unit, padded? 99 -> 0.99
        assert_eq!(money(1, "EGP"), "0.01 EGP"); // leading-zero on cents
        assert_eq!(money(123456, "USD"), "1234.56 USD"); // thousands, no grouping
    }

    #[test]
    fn money_no_currency_drops_the_suffix_and_keeps_sign() {
        assert_eq!(money(14250, ""), "142.50");
        assert_eq!(money(-14250, ""), "-142.50");
        assert_eq!(money(5, ""), "0.05");
        assert_eq!(money(0, ""), "0.00");
    }

    #[test]
    fn money_negative_one_cent_keeps_leading_zero() {
        // unsigned_abs path: -1 → magnitude 1 → "0.01" with a sign.
        assert_eq!(money(-1, "EGP"), "-0.01 EGP");
        assert_eq!(money(-100, "EGP"), "-1.00 EGP");
    }

    #[test]
    fn money_i64_min_does_not_panic_on_abs() {
        // unsigned_abs is used precisely so i64::MIN can't overflow .abs().
        let s = money(i64::MIN, "EGP");
        assert!(s.starts_with('-'));
        assert!(s.ends_with(" EGP"));
    }

    // ── row(): right-align, left-truncation, exact-width boundaries ───────────

    #[test]
    fn row_exact_width_value_returns_value_only() {
        // right value is exactly `width` wide → returns just the value, no left.
        let r = row("Total", "1234567890", 10);
        assert_eq!(r, "1234567890");
        assert_eq!(r.chars().count(), 10);
    }

    #[test]
    fn row_overlong_value_is_truncated_to_width() {
        // rc >= width: value is truncated from the front (take(width)).
        let r = row("L", "12345678901234", 8);
        assert_eq!(r, "12345678");
        assert_eq!(r.chars().count(), 8);
    }

    #[test]
    fn row_keeps_at_least_one_space_between_columns() {
        // max_left = width - rc - 1 guarantees a gap even when left would fill it.
        let r = row("ABCDEFGH", "XY", 10);
        assert_eq!(r.chars().count(), 10);
        assert!(r.ends_with("XY"));
        // left truncated to 7 chars (10 - 2 - 1), then a single pad space.
        assert_eq!(r, "ABCDEFG XY");
    }

    #[test]
    fn row_short_left_pads_with_spaces_to_full_width() {
        let r = row("A", "Z", 5);
        assert_eq!(r, "A   Z");
        assert_eq!(r.chars().count(), 5);
    }

    #[test]
    fn row_counts_unicode_by_chars_not_bytes() {
        // Right value is multi-byte; width accounting is by char count.
        let r = row("Total", "٥.٠٠", 12); // Arabic-Indic digits
        assert_eq!(r.chars().count(), 12);
        assert!(r.ends_with("٥.٠٠"));
    }

    // ── encode(): framing, ordering, per-line resets, empty input ─────────────

    #[test]
    fn encode_empty_lines_still_frames_init_feed_and_cut() {
        let bytes = encode(&[]);
        // ESC @ at the head.
        assert_eq!(&bytes[0..2], &[ESC, b'@']);
        // Tail reset block: ESC a 0, ESC E 0, GS ! 0, LF LF LF, GS V 66 0.
        let tail = &bytes[bytes.len() - 4..];
        assert_eq!(tail, &[GS, b'V', 66, 0]);
        assert!(bytes.windows(3).any(|w| w == [ESC, b'a', 0]));
        assert!(bytes.windows(3).any(|w| w == [ESC, b'E', 0]));
        assert!(bytes.windows(3).any(|w| w == [GS, b'!', 0]));
        assert!(bytes.windows(3).any(|w| w == [LF, LF, LF]));
    }

    #[test]
    fn encode_emits_per_line_control_sequence_in_order() {
        // For one left/plain line: ESC a 0, ESC E 0, GS ! 0, text, LF — in that
        // order, immediately after the ESC @ init.
        let bytes = encode(&[Line::plain("AB")]);
        let expected_head: Vec<u8> = vec![
            ESC, b'@', ESC, b'a', 0, ESC, b'E', 0, GS, b'!', 0, b'A', b'B', LF,
        ];
        assert_eq!(&bytes[..expected_head.len()], expected_head.as_slice());
    }

    #[test]
    fn encode_right_align_uses_code_two() {
        let bytes = encode(&[Line { text: "R".into(), align: Align::Right, bold: false, size: Size::Normal }]);
        assert!(bytes.windows(3).any(|w| w == [ESC, b'a', 2]));
    }

    #[test]
    fn encode_preserves_line_order_and_count() {
        let bytes = encode(&[Line::plain("FIRST"), Line::plain("SECOND")]);
        let i_first = bytes.windows(5).position(|w| w == b"FIRST").unwrap();
        let i_second = bytes.windows(6).position(|w| w == b"SECOND").unwrap();
        assert!(i_first < i_second);
    }

    #[test]
    fn escpos_equals_encode_of_layout() {
        // The convenience wrapper is exactly encode(layout(..)) for the brand.
        let r = cash_receipt();
        let c = ctx();
        assert_eq!(escpos(&r, &c, PrinterBrand::Epson), encode(&layout(&r, &c)));
        assert_eq!(escpos(&r, &c, PrinterBrand::Star), encode_star(&layout(&r, &c)));
    }

    // ── drawer kick exact bytes (decimal-form assertion) ─────────────────────

    #[test]
    fn drawer_kick_decimal_form_is_esc_p_0_25_250() {
        // ESC=27, 'p'=112, m=0, on=25, off=250.
        assert_eq!(drawer_kick_bytes(), vec![27, 112, 0, 25, 250]);
    }

    // ── Star Line Mode encoder (distinct command set from ESC/POS) ───────────

    #[test]
    fn star_encode_frames_with_init_and_star_partial_cut() {
        let bytes = encode_star(&[Line::centered("HI")]);
        // ESC @ init at the head; ESC d 3 (Star partial cut) at the tail — and
        // NOT the ESC/POS GS V cut.
        assert_eq!(&bytes[0..2], &[0x1B, b'@']);
        assert_eq!(&bytes[bytes.len() - 3..], &[0x1B, b'd', 3]);
        assert!(!bytes.windows(2).any(|w| w == [GS, b'V']));
    }

    #[test]
    fn star_uses_star_alignment_size_and_bold_codes() {
        let bytes = encode_star(&[Line { text: "BIG".into(), align: Align::Right, bold: true, size: Size::Double }]);
        // ESC GS a 2 (Star right-justify), ESC i 1 1 (double), ESC E (emphasize on).
        assert!(bytes.windows(4).any(|w| w == [ESC, GS, b'a', 2]));
        assert!(bytes.windows(4).any(|w| w == [ESC, b'i', 1, 1]));
        assert!(bytes.windows(2).any(|w| w == [ESC, b'E']));
        // It must NOT emit the ESC/POS alignment/size codes.
        assert!(!bytes.windows(3).any(|w| w == [ESC, b'a', 2]));
        assert!(!bytes.windows(2).any(|w| w == [GS, b'!']));
    }

    #[test]
    fn star_non_bold_emits_cancel_emphasis() {
        let bytes = encode_star(&[Line::plain("x")]);
        assert!(bytes.windows(2).any(|w| w == [ESC, b'F'])); // ESC F — cancel emphasis
    }

    #[test]
    fn star_drawer_kick_is_bare_bel() {
        // Bare BEL drives the drawer; ESC BEL n1 n2 would only set the pulse width.
        assert_eq!(drawer_kick_star(), vec![0x07]);
    }

    #[test]
    fn encode_for_dispatches_by_brand() {
        let l = [Line::centered("Z")];
        assert_eq!(encode_for(PrinterBrand::Epson, &l), encode(&l));
        assert_eq!(encode_for(PrinterBrand::Star, &l), encode_star(&l));
        assert_ne!(encode_for(PrinterBrand::Epson, &l), encode_for(PrinterBrand::Star, &l));
        assert_eq!(drawer_kick_for(PrinterBrand::Epson), drawer_kick_bytes());
        assert_eq!(drawer_kick_for(PrinterBrand::Star), drawer_kick_star());
    }

    // ── layout: conditional subtotal / tax / delivery-fee gating ──────────────

    #[test]
    fn layout_no_subtotal_when_no_discount_or_fee() {
        let mut r = cash_receipt();
        r.discount_minor = 0;
        r.delivery_fee_minor = 0;
        let lines = layout(&r, &ctx());
        assert!(!lines.iter().any(|l| l.text.starts_with("Subtotal")));
    }

    #[test]
    fn layout_subtotal_appears_for_delivery_fee_alone() {
        // Delivery fee with NO discount still forces the Subtotal row.
        let mut r = cash_receipt();
        r.discount_minor = 0;
        r.delivery_fee_minor = 1500;
        let lines = layout(&r, &ctx());
        assert!(lines.iter().any(|l| l.text.starts_with("Subtotal")));
        assert!(lines.iter().any(|l| l.text.starts_with("Delivery Fee") && l.text.ends_with("15.00 EGP")));
        // No discount line when discount is zero.
        assert!(!lines.iter().any(|l| l.text.starts_with("Discount")));
    }

    #[test]
    fn layout_omits_tax_row_when_zero() {
        let mut r = cash_receipt();
        r.tax_minor = 0;
        let lines = layout(&r, &ctx());
        assert!(!lines.iter().any(|l| l.text.starts_with("Tax")));
        // Total still printed.
        assert!(lines.iter().any(|l| l.text.starts_with("Total")));
    }

    #[test]
    fn layout_discount_row_is_negative_signed() {
        let mut r = cash_receipt();
        r.discount_minor = 500;
        let lines = layout(&r, &ctx());
        let d = lines.iter().find(|l| l.text.starts_with("Discount")).expect("discount row");
        assert!(d.text.ends_with("-5.00 EGP"));
    }

    // ── layout: delivery channel variants + minimal delivery block ────────────

    #[test]
    fn layout_delivery_in_mall_channel_flag() {
        let mut r = cash_receipt();
        r.is_delivery = true;
        r.delivery_channel = Some("in_mall".into());
        let lines = layout(&r, &ctx());
        assert!(lines.iter().any(|l| l.text == "*** DELIVERY — In-Mall ***" && l.bold));
    }

    #[test]
    fn layout_delivery_unknown_channel_falls_back_to_bare_flag() {
        let mut r = cash_receipt();
        r.is_delivery = true;
        r.delivery_channel = Some("drone".into()); // unrecognized
        let lines = layout(&r, &ctx());
        assert!(lines.iter().any(|l| l.text == "*** DELIVERY ***" && l.bold));
        assert!(!lines.iter().any(|l| l.text.contains("—")));
    }

    #[test]
    fn layout_delivery_none_channel_bare_flag() {
        let mut r = cash_receipt();
        r.is_delivery = true;
        r.delivery_channel = None;
        let lines = layout(&r, &ctx());
        assert!(lines.iter().any(|l| l.text == "*** DELIVERY ***"));
    }

    #[test]
    fn layout_delivery_optional_block_fields_are_each_conditional() {
        // Only zone + delivery_ref + payment_hint + notes set; others absent.
        let mut r = cash_receipt();
        r.is_delivery = true;
        r.delivery_zone = Some("Zone 3".into());
        r.delivery_ref = Some("DLV-9".into());
        r.payment_hint = Some("COD".into());
        r.delivery_notes = Some("ring bell".into());
        let lines = layout(&r, &ctx());
        let text: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert!(text.iter().any(|t| t.starts_with("Zone") && t.ends_with("Zone 3")));
        assert!(text.iter().any(|t| t.starts_with("Delivery Ref") && t.ends_with("DLV-9")));
        assert!(text.iter().any(|t| t.starts_with("Payment (hint)") && t.ends_with("COD")));
        // notes prints as a free-form "{label} {value}" line, not a right-aligned row.
        assert!(text.iter().any(|t| t.starts_with("Notes:") && t.contains("ring bell")));
        // Absent customer/phone/address are not printed.
        assert!(!text.iter().any(|t| t.starts_with("Customer")));
        assert!(!text.iter().any(|t| t.starts_with("Phone")));
    }

    #[test]
    fn layout_non_delivery_prints_customer_in_footer() {
        let mut r = cash_receipt();
        r.is_delivery = false;
        r.customer_name = Some("Walk-in".into());
        let lines = layout(&r, &ctx());
        // Non-delivery: customer appears (in the footer block), exactly once.
        assert_eq!(lines.iter().filter(|l| l.text.starts_with("Customer")).count(), 1);
        assert!(lines.iter().any(|l| l.text.starts_with("Customer") && l.text.ends_with("Walk-in")));
    }

    // ── layout: item / modifier / bundle indentation depth ────────────────────

    #[test]
    fn layout_addon_indent_is_two_spaces_optional_after_addon() {
        let mut r = cash_receipt();
        let mut latte = line("Latte", 1, 5000);
        latte.addons = vec![ReceiptModifierView { name: "Vanilla".into(), price_minor: 300 }];
        latte.optionals = vec![ReceiptModifierView { name: "Extra hot".into(), price_minor: 0 }];
        r.lines = vec![latte];
        let lines = layout(&r, &ctx());
        let text: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        // Non-bundle modifiers use a two-space prefix "  + ".
        assert!(text.iter().any(|t| t.starts_with("  + Vanilla") && t.contains("+3.00 EGP")));
        let free = text.iter().find(|t| t.contains("Extra hot")).unwrap();
        assert_eq!(*free, "  + Extra hot"); // free → name only, no price
        // Addon comes before the optional.
        let i_addon = text.iter().position(|t| t.contains("Vanilla")).unwrap();
        let i_opt = text.iter().position(|t| t.contains("Extra hot")).unwrap();
        assert!(i_addon < i_opt);
    }

    #[test]
    fn layout_bundle_component_and_addon_use_four_space_indent() {
        let mut r = cash_receipt();
        let mut combo = line("Combo", 1, 10000);
        combo.is_bundle = true;
        combo.components = vec![ReceiptComponentView {
            name: "Burger".into(),
            size_label: Some("Double".into()),
            addons: vec![ReceiptModifierView { name: "Bacon".into(), price_minor: 700 }],
            optionals: vec![ReceiptModifierView { name: "No onion".into(), price_minor: 0 }],
        }];
        r.lines = vec![combo];
        let lines = layout(&r, &ctx());
        let text: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        // Component line: "  - Burger (Double)" (size inlined).
        assert!(text.iter().any(|t| *t == "  - Burger (Double)"));
        // Bundle addon priced, four-space indent "    + ".
        assert!(text.iter().any(|t| t.starts_with("    + Bacon") && t.contains("+7.00 EGP")));
        // Bundle free optional: four-space indent, no price.
        assert!(text.iter().any(|t| *t == "    + No onion"));
    }

    #[test]
    fn layout_non_bundle_ignores_components() {
        // A line with components but is_bundle=false should NOT print them.
        let mut r = cash_receipt();
        let mut item = line("Solo", 1, 5000);
        item.is_bundle = false;
        item.components = vec![ReceiptComponentView {
            name: "ShouldNotShow".into(),
            size_label: None,
            addons: vec![],
            optionals: vec![],
        }];
        r.lines = vec![item];
        let lines = layout(&r, &ctx());
        assert!(!lines.iter().any(|l| l.text.contains("ShouldNotShow")));
    }

    #[test]
    fn layout_item_size_label_inlined_in_name() {
        let mut r = cash_receipt();
        let mut item = line("Tea", 3, 4500);
        item.size_label = Some("Large".into());
        r.lines = vec![item];
        let lines = layout(&r, &ctx());
        assert!(lines.iter().any(|l| l.text.starts_with("3x Tea (Large)") && l.text.ends_with("45.00 EGP")));
    }

    // ── layout: order id vs order number, ref row, datetime fallback ──────────

    #[test]
    fn layout_uses_short_uppercased_id_when_no_order_number() {
        let mut r = cash_receipt();
        r.order_number = None;
        r.local_order_id = "abc12345-9999-0000-0000-000000000000".into();
        let lines = layout(&r, &ctx());
        // short_id = first dash segment, uppercased ("abc12345" → "ABC12345").
        // At width 32 the left column is truncated to 12 cols (dt = 19 + 1 gap),
        // so the rendered prefix is the uppercased id clipped — still proves casing.
        assert!(lines.iter().any(|l| l.text.starts_with("Order ABC123")));
    }

    #[test]
    fn layout_order_ref_row_present_only_when_set() {
        let mut r = cash_receipt();
        r.order_ref = Some("POS-77".into());
        let lines = layout(&r, &ctx());
        assert!(lines.iter().any(|l| l.text.starts_with("Ref:") && l.text.contains("POS-77")));

        let mut r2 = cash_receipt();
        r2.order_ref = None;
        let lines2 = layout(&r2, &ctx());
        assert!(!lines2.iter().any(|l| l.text.starts_with("Ref:")));
    }

    #[test]
    fn layout_unparseable_created_at_falls_back_to_raw() {
        let mut r = cash_receipt();
        r.created_at = "not-a-date".into();
        let lines = layout(&r, &ctx());
        // fmt_dt returns the raw string; the order row carries it verbatim.
        assert!(lines.iter().any(|l| l.text.contains("not-a-date")));
    }

    // ── fmt_dt directly ───────────────────────────────────────────────────────

    #[test]
    fn fmt_dt_formats_in_source_offset_12h() {
        // 13:05 UTC → "01:05 PM"; date dd/MM/yyyy.
        assert_eq!(fmt_dt("2026-06-20T13:05:00Z"), "20/06/2026 01:05 PM");
        // Midnight → 12:00 AM.
        assert_eq!(fmt_dt("2026-01-09T00:00:00Z"), "09/01/2026 12:00 AM");
        // Non-UTC offset preserved (not converted to UTC).
        assert_eq!(fmt_dt("2026-06-20T13:05:00+02:00"), "20/06/2026 01:05 PM");
    }

    #[test]
    fn fmt_dt_returns_input_on_parse_failure() {
        assert_eq!(fmt_dt("garbage"), "garbage");
        assert_eq!(fmt_dt(""), "");
    }

    // ── short_id via layout indirection already covered; divider directly ─────

    #[test]
    fn divider_is_full_width_dashes() {
        assert_eq!(divider(5), "-----");
        assert_eq!(divider(0), "");
    }

    #[test]
    fn layout_clamps_width_to_minimum_sixteen() {
        // ctx.width below 16 is clamped to 16, so dividers are 16 wide.
        let mut c = ctx();
        c.width = 4;
        let lines = layout(&cash_receipt(), &c);
        assert!(lines.iter().any(|l| l.text == "-".repeat(16)));
    }

    // ── layout_shift_report: empty / conditional sections ─────────────────────

    fn z_labels() -> ShiftReportLabels {
        ShiftReportLabels {
            title: "Shift Report".into(),
            business_date: "Business Date".into(),
            printed_at: "Printed at".into(),
            teller: "Teller".into(),
            opened: "Opened".into(),
            closed: "Closed".into(),
            interim: "Interim".into(),
            payments: "Payments".into(),
            orders: "orders".into(),
            total_collected: "Total Collected".into(),
            drawer_ops: "Drawer Operations".into(),
            cash_in: "Cash in".into(),
            cash_out: "Cash out".into(),
            cash_recon: "Cash Reconciliation".into(),
            opening: "Opening".into(),
            opening_mismatch: "Opening mismatch".into(),
            opening_reason: "Reason".into(),
            expected: "Expected".into(),
            actual: "Actual".into(),
            not_closed: "Not closed".into(),
            difference: "Difference".into(),
            short_by: "Short by".into(),
            over_by: "Over by".into(),
            voided: "Voided".into(),
            transactions: "Transactions".into(),
            end_of_report: "End of Report".into(),
            cash_moves: "Cash moves".into(),
            by_method: "By method".into(),
        }
    }

    fn empty_report() -> ShiftReportView {
        ShiftReportView {
            teller_name: "Mona".into(),
            opened_at: "t".into(),
            closed_at: None,
            printed_at: "t".into(),
            is_open: true,
            opening_cash_was_edited: false,
            opening_cash_original_minor: None,
            opening_cash_edit_reason: None,
            closing_cash_declared_minor: None,
            expected_cash_minor: 10000,
            opening_cash_minor: 10000,
            total_payments_minor: 0,
            net_payments_minor: 0,
            voided_amount_minor: 0,
            cash_movements_net_minor: 0,
            cash_in_minor: 0,
            cash_out_minor: 0,
            payment_lines: vec![],
            cash_movements: vec![],
            from_server: true,
        }
    }

    #[test]
    fn layout_shift_report_minimal_has_core_rows_no_optional_sections() {
        let lines = layout_shift_report(&empty_report(), "Cafe", "EGP", 32, &z_labels());
        let text: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        // Title + opening + payments + expected always present.
        assert!(text.iter().any(|t| t.starts_with("Opening")));
        assert!(text.iter().any(|t| t.starts_with("Payments")));
        assert!(lines.iter().any(|l| l.bold && l.text.starts_with("Expected")));
        // No cash-in/out rows (both zero), no movements block, no voided block.
        assert!(!text.iter().any(|t| t.starts_with("Cash in")));
        assert!(!text.iter().any(|t| t.starts_with("Cash out")));
        assert!(!text.iter().any(|t| *t == "Cash moves"));
        assert!(!text.iter().any(|t| t.starts_with("Voided")));
        // The "By method" header still prints even with no payment lines.
        assert!(text.iter().any(|t| *t == "By method"));
    }

    #[test]
    fn layout_shift_report_cash_in_only_shows_in_not_out() {
        let mut rep = empty_report();
        rep.cash_in_minor = 2500;
        rep.cash_out_minor = 0;
        let lines = layout_shift_report(&rep, "Cafe", "EGP", 32, &z_labels());
        assert!(lines.iter().any(|l| l.text.starts_with("Cash in") && l.text.ends_with("25.00 EGP")));
        assert!(!lines.iter().any(|l| l.text.starts_with("Cash out")));
    }

    #[test]
    fn layout_shift_report_cash_out_prints_negative_signed() {
        let mut rep = empty_report();
        rep.cash_out_minor = 1800; // stored positive, printed negated
        let lines = layout_shift_report(&rep, "Cafe", "EGP", 32, &z_labels());
        assert!(lines.iter().any(|l| l.text.starts_with("Cash out") && l.text.ends_with("-18.00 EGP")));
    }

    #[test]
    fn layout_shift_report_blank_note_movement_labels_by_name() {
        let mut rep = empty_report();
        rep.cash_movements = vec![
            crate::shift::ShiftReportCashLine { amount_minor: 500, note: "   ".into(), moved_by_name: "Mona".into(), created_at: "t".into() },
        ];
        let lines = layout_shift_report(&rep, "Cafe", "EGP", 32, &z_labels());
        // A whitespace-only note falls back to the mover's name.
        assert!(lines.iter().any(|l| l.text.trim_start().starts_with("Mona") && l.text.ends_with("5.00 EGP")));
        // The "Cash moves" header is emitted once movements exist.
        assert!(lines.iter().any(|l| l.text == "Cash moves"));
    }

    #[test]
    fn layout_shift_report_voided_block_only_when_positive() {
        let mut rep = empty_report();
        rep.voided_amount_minor = 750;
        let lines = layout_shift_report(&rep, "Cafe", "EGP", 32, &z_labels());
        assert!(lines.iter().any(|l| l.text.starts_with("Voided") && l.text.ends_with("7.50 EGP")));

        let mut rep0 = empty_report();
        rep0.voided_amount_minor = 0;
        let lines0 = layout_shift_report(&rep0, "Cafe", "EGP", 32, &z_labels());
        assert!(!lines0.iter().any(|l| l.text.starts_with("Voided")));
    }

    #[test]
    fn layout_shift_report_header_is_double_size_bold() {
        let lines = layout_shift_report(&empty_report(), "Cafe Madar", "EGP", 32, &z_labels());
        assert_eq!(lines[0].text, "Cafe Madar");
        assert_eq!(lines[0].size, Size::Double);
        assert!(lines[0].bold);
        assert_eq!(lines[0].align, Align::Center);
    }

    #[test]
    fn layout_shift_report_clamps_width_to_sixteen() {
        let lines = layout_shift_report(&empty_report(), "Cafe", "EGP", 1, &z_labels());
        // Dividers reflect the clamped 16-col width.
        assert!(lines.iter().any(|l| l.text == "-".repeat(16)));
    }

    // ── raster wrappers ──────────────────────────────────────────────────────
    // A 16-wide × 2-row bitmap: row0 = AB CD, row1 = 12 34. wb = 2.
    fn tiny_bitmap() -> Bitmap {
        Bitmap { width: 16, rows: 2, bytes: vec![0xAB, 0xCD, 0x12, 0x34] }
    }

    #[test]
    fn star_raster_frames_bitmap_exactly() {
        #[rustfmt::skip]
        let want: Vec<u8> = vec![
            0x1B, 0x40,                         // ESC @
            0x1B, 0x2A, 0x72, 0x41,             // ESC * r A
            0x1B, 0x2A, 0x72, 0x50, 0x30, 0x00, // ESC * r P "0" NUL
            0x1B, 0x2A, 0x72, 0x45, 0x31, 0x33, 0x00, // ESC * r E "13" NUL
            b'b', 0x02, 0x00, 0xAB, 0xCD,       // row 0
            b'b', 0x02, 0x00, 0x12, 0x34,       // row 1
            0x1B, 0x0C, 0x04,                   // ESC FF EOT
            0x1B, 0x2A, 0x72, 0x42,             // ESC * r B
        ];
        assert_eq!(star_raster(&tiny_bitmap()), want);
    }

    #[test]
    fn escpos_raster_frames_bitmap_exactly() {
        #[rustfmt::skip]
        let want: Vec<u8> = vec![
            0x1B, 0x40,                                     // ESC @
            0x1D, 0x76, 0x30, 0x00, 0x02, 0x00, 0x02, 0x00, // GS v 0 m xL xH yL yH
            0xAB, 0xCD, 0x12, 0x34,                         // image data
            0x1B, 0x64, 0x05,                               // ESC d 5
            0x1D, 0x56, 0x41, 0x05,                         // GS V 65 5
        ];
        assert_eq!(escpos_raster(&tiny_bitmap()), want);
    }

    #[test]
    fn raster_for_dispatches_by_brand() {
        let b = tiny_bitmap();
        assert_eq!(raster_for(PrinterBrand::Star, &b), star_raster(&b));
        assert_eq!(raster_for(PrinterBrand::Epson, &b), escpos_raster(&b));
        assert_ne!(raster_for(PrinterBrand::Star, &b), raster_for(PrinterBrand::Epson, &b));
    }

    #[test]
    fn row_bytes_rounds_up_to_whole_bytes() {
        assert_eq!(Bitmap { width: 576, rows: 0, bytes: vec![] }.row_bytes(), 72);
        assert_eq!(Bitmap { width: 1, rows: 0, bytes: vec![] }.row_bytes(), 1);
        assert_eq!(Bitmap { width: 9, rows: 0, bytes: vec![] }.row_bytes(), 2);
    }
}
