//! Receipt → 1-bit raster bitmap.
//!
//! The TSP143III is raster-only and receipts carry an org logo + Arabic, so the
//! whole receipt is rendered to an image here (not text commands) and shipped via
//! the raster wrappers in [`crate::receipt`]. The layout mirrors the on-screen
//! `ReceiptPaper` preview (centered logo → uppercase store name → rules →
//! two-column money rows → indented modifiers → centered footer) so the teller
//! prints exactly what they saw. Text is shaped with the embedded Cairo font
//! (Arabic shaping + bidi via cosmic-text), the same font the app UI uses.
//!
//! Output is deterministic for a given input + font, so all three hosts print a
//! byte-identical receipt. Ink is rendered solid black: the preview's gray
//! "faint" text would threshold to white on a 1-bit head, so hierarchy is carried
//! by weight/size/indentation exactly as the preview does structurally.

use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Weight};

use crate::checkout::{ReceiptLineView, ReceiptModifierView, ReceiptView};
use crate::receipt::{fmt_dt, money, short_id, Bitmap, EscPosCtx, ShiftReportLabels};
use crate::shift::ShiftReportView;

/// Printable width in dots — 72 mm @ 203 dpi. Matches the Flutter `_printerWidth`
/// and the Star raster row width (576 / 8 = 72 bytes).
pub const PRINT_WIDTH: u32 = 576;

const MARGIN: i32 = 16; // left/right quiet zone, dots
const TOP_PAD: i32 = 16;
const BOTTOM_PAD: i32 = 28; // trailing whitespace before the cut

// Font sizes (dots). Tuned to the preview's sp ratios scaled to 576 px.
const SZ_STORE: f32 = 34.0;
const SZ_TOTAL: f32 = 28.0;
const SZ_BODY: f32 = 24.0;
const SZ_SMALL: f32 = 22.0;
const LINE: f32 = 1.30; // line-height multiple

// Org-logo bounding box (dots). The preview caps the logo at 220×60 dp on a
// ~360 dp paper; scaled to the 576-dot print (×1.6) that's ~352×96. Fit-inside,
// aspect-preserved — a wide wordmark or a square mark both stay in proportion.
const LOGO_MAX_W: u32 = 352;
const LOGO_MAX_H: u32 = 96;

// Embedded Cairo — the same family the Compose/Swift UI renders. Regular for
// body, SemiBold for the payment label, Bold for headers/totals.
const CAIRO_REGULAR: &[u8] = include_bytes!("../assets/fonts/Cairo-Regular.ttf");
const CAIRO_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Cairo-SemiBold.ttf");
const CAIRO_BOLD: &[u8] = include_bytes!("../assets/fonts/Cairo-Bold.ttf");

/// Render the receipt to a 1-bit bitmap. `logo` is the decoded org-logo image
/// bytes the core fetched + cached, or `None` to print without it.
pub fn render_receipt(receipt: &ReceiptView, ctx: &EscPosCtx, logo: Option<&[u8]>) -> Bitmap {
    let mut r = Renderer::new();
    r.build(receipt, ctx, logo);
    r.canvas.into_bitmap()
}

/// Render the shift report (Z-report) to a 1-bit bitmap — same engine/look as the
/// receipt, mirroring the on-screen `ShiftReportBreakdown` preview.
pub fn render_shift_report(report: &ShiftReportView, store: &str, currency: &str, labels: &ShiftReportLabels) -> Bitmap {
    let mut r = Renderer::new();
    r.build_shift(report, store, currency, labels);
    r.canvas.into_bitmap()
}

// ── grayscale canvas ─────────────────────────────────────────────────────────

/// A growable single-channel coverage canvas: one byte per dot, 0 = blank,
/// 255 = full ink. Height grows as content is appended.
struct Canvas {
    w: usize,
    px: Vec<u8>,
}

impl Canvas {
    fn new(w: usize) -> Self {
        Canvas { w, px: Vec::new() }
    }

    fn height(&self) -> usize {
        self.px.len() / self.w
    }

    /// Ensure the canvas is at least `rows` tall.
    fn grow_to(&mut self, rows: usize) {
        if rows > self.height() {
            self.px.resize(rows * self.w, 0);
        }
    }

    /// Max-blend coverage at (x, y); out-of-bounds is dropped, height grows down.
    fn cover(&mut self, x: i32, y: i32, a: u8) {
        if a == 0 || x < 0 || y < 0 || x as usize >= self.w {
            return;
        }
        let (x, y) = (x as usize, y as usize);
        self.grow_to(y + 1);
        let i = y * self.w + x;
        if a > self.px[i] {
            self.px[i] = a;
        }
    }

    /// A solid horizontal rule from `x0`..`x1` at `y`, `thick` dots tall.
    fn rule(&mut self, x0: i32, x1: i32, y: i32, thick: i32) {
        for dy in 0..thick {
            for x in x0..x1 {
                self.cover(x, y + dy, 255);
            }
        }
    }

    /// Threshold the coverage canvas to a packed 1-bit MSB-first bitmap.
    fn into_bitmap(self) -> Bitmap {
        let rows = self.height() as u32;
        let wb = (self.w + 7) / 8;
        let mut bytes = vec![0u8; wb * self.height()];
        for y in 0..self.height() {
            for x in 0..self.w {
                if self.px[y * self.w + x] >= 128 {
                    bytes[y * wb + (x >> 3)] |= 0x80 >> (x & 7);
                }
            }
        }
        Bitmap { width: self.w as u32, rows, bytes }
    }
}

// ── renderer ─────────────────────────────────────────────────────────────────

struct Renderer {
    fonts: FontSystem,
    cache: SwashCache,
    canvas: Canvas,
    y: i32, // current baseline cursor (top of the next block)
}

impl Renderer {
    fn new() -> Self {
        let mut db = cosmic_text::fontdb::Database::new();
        db.load_font_data(CAIRO_REGULAR.to_vec());
        db.load_font_data(CAIRO_SEMIBOLD.to_vec());
        db.load_font_data(CAIRO_BOLD.to_vec());
        // Custom db only — never scan system fonts, so output is deterministic
        // and the Android/iOS sandboxes don't matter.
        let fonts = FontSystem::new_with_locale_and_db("en-US".to_string(), db);
        Renderer {
            fonts,
            cache: SwashCache::new(),
            canvas: Canvas::new(PRINT_WIDTH as usize),
            y: TOP_PAD,
        }
    }

    fn content_w(&self) -> i32 {
        PRINT_WIDTH as i32 - 2 * MARGIN
    }

    /// Shape one string into a laid-out buffer at the given size/weight, wrapping
    /// at `max_w` dots.
    fn shape(&mut self, text: &str, size: f32, weight: Weight, max_w: i32) -> Buffer {
        let mut buf = Buffer::new(&mut self.fonts, Metrics::new(size, size * LINE));
        buf.set_size(&mut self.fonts, Some(max_w as f32), None);
        let attrs = Attrs::new().family(Family::Name("Cairo")).weight(weight);
        buf.set_text(&mut self.fonts, text, attrs, Shaping::Advanced);
        buf.shape_until_scroll(&mut self.fonts, false);
        buf
    }

    /// Width (max run width) and height (bottom of last run) of a shaped buffer.
    fn measure(buf: &Buffer) -> (f32, f32) {
        let mut w = 0.0f32;
        let mut h = 0.0f32;
        for run in buf.layout_runs() {
            w = w.max(run.line_w);
            h = h.max(run.line_top + run.line_height);
        }
        (w, h)
    }

    /// Blit a shaped buffer with its left edge at `ox` and top at `oy`.
    fn blit(&mut self, buf: &Buffer, ox: i32, oy: i32) {
        let ink = Color::rgb(0, 0, 0);
        for run in buf.layout_runs() {
            let base_y = oy + run.line_y as i32;
            for glyph in run.glyphs.iter() {
                let phys = glyph.physical((0.0, 0.0), 1.0);
                let gx = ox + phys.x;
                let gy = base_y + phys.y;
                self.cache.with_pixels(&mut self.fonts, phys.cache_key, ink, |dx, dy, color| {
                    self.canvas.cover(gx + dx, gy + dy, color.a());
                });
            }
        }
    }

    /// Draw a centered text block at the current cursor, advancing past it.
    fn center(&mut self, s: &str, size: f32, weight: Weight) {
        if s.is_empty() {
            return;
        }
        let buf = self.shape(s, size, weight, self.content_w());
        let (w, h) = Self::measure(&buf);
        let ox = ((PRINT_WIDTH as f32 - w) / 2.0).round() as i32;
        let oy = self.y;
        self.blit(&buf, ox, oy);
        self.y += h.ceil() as i32;
    }

    /// A two-column row: `left` flush-left, `right` flush-right on one baseline.
    fn row(&mut self, left: &str, right: &str, size: f32, weight: Weight) {
        let right_buf = self.shape(right, size, weight, self.content_w());
        let (rw, rh) = Self::measure(&right_buf);
        // Keep the label clear of the value.
        let left_max = (self.content_w() as f32 - rw - 12.0).max(40.0) as i32;
        let left_buf = self.shape(left, size, weight, left_max);
        let (_, lh) = Self::measure(&left_buf);
        let oy = self.y;
        self.blit(&left_buf, MARGIN, oy);
        if !right.is_empty() {
            let rx = PRINT_WIDTH as i32 - MARGIN - rw.round() as i32;
            self.blit(&right_buf, rx, oy);
        }
        self.y += lh.max(rh).ceil() as i32;
    }

    /// A left-aligned line indented by `indent` dots (modifiers / address).
    fn indented(&mut self, s: &str, size: f32, indent: i32) {
        let buf = self.shape(s, size, Weight::NORMAL, self.content_w() - indent);
        let (_, h) = Self::measure(&buf);
        let oy = self.y;
        self.blit(&buf, MARGIN + indent, oy);
        self.y += h.ceil() as i32;
    }

    fn rule(&mut self) {
        self.y += 4;
        self.canvas.rule(MARGIN, PRINT_WIDTH as i32 - MARGIN, self.y, 2);
        self.y += 8;
    }

    fn gap(&mut self, dots: i32) {
        self.y += dots;
    }

    /// Decode, scale-to-fit and dither the org logo, then composite it centered.
    fn logo(&mut self, bytes: &[u8]) {
        let Some((w, h, ink)) = decode_logo(bytes, LOGO_MAX_W, LOGO_MAX_H) else {
            return;
        };
        let ox = ((PRINT_WIDTH as i32 - w as i32) / 2).max(0);
        let oy = self.y;
        for ly in 0..h {
            for lx in 0..w {
                if ink[ly * w + lx] != 0 {
                    self.canvas.cover(ox + lx as i32, oy + ly as i32, 255);
                }
            }
        }
        self.y += h as i32 + 8;
    }

    /// Walk the receipt top-to-bottom, mirroring `ReceiptPaper.kt`.
    fn build(&mut self, r: &ReceiptView, ctx: &EscPosCtx, logo: Option<&[u8]>) {
        let cur = &ctx.currency;
        let lab = &ctx.labels;
        let m = |minor: i64| money(minor, cur);

        // ── header ──
        if let Some(bytes) = logo {
            self.logo(bytes);
        }
        if r.is_voided {
            self.center(&format!("*** {} ***", lab.voided), SZ_BODY, Weight::BOLD);
        }
        let store = if ctx.store_name.trim().is_empty() {
            "SUFRIX".to_string()
        } else {
            ctx.store_name.to_uppercase()
        };
        self.center(&store, SZ_STORE, Weight::BOLD);
        if r.is_delivery {
            if let Some(ch) = r.delivery_channel.as_deref() {
                let label = if ch == "in_mall" { &lab.channel_in_mall } else { &lab.delivery };
                self.center(&format!("— {} —", label.to_uppercase()), SZ_SMALL, Weight::NORMAL);
            }
        }
        self.rule();

        // ── order meta ──
        let title = match r.order_number {
            Some(n) => format!("{} #{}", lab.order, n),
            None => format!("{} {}", lab.order, short_id(&r.local_order_id)),
        };
        self.row(&title, &fmt_dt(&r.created_at), SZ_BODY, Weight::NORMAL);
        if let Some(rf) = &r.order_ref {
            self.row(&format!("{}: {}", lab.reference, rf), "", SZ_BODY, Weight::NORMAL);
        }
        self.rule();

        // ── delivery block ──
        if r.is_delivery {
            if let Some(v) = &r.customer_name {
                self.row(&lab.customer, v, SZ_SMALL, Weight::NORMAL);
            }
            if let Some(v) = &r.customer_phone {
                self.row(&lab.phone, v, SZ_SMALL, Weight::NORMAL);
            }
            if let Some(v) = &r.delivery_address {
                self.indented(&format!("{}: {}", lab.address, v), SZ_SMALL, 0);
            }
            if let Some(v) = &r.delivery_zone {
                self.row(&lab.zone, v, SZ_SMALL, Weight::NORMAL);
            }
            // Courier ref + COD/payment hint + customer instructions — the ESC/POS
            // text path printed these but the raster path (this) dropped them, so a
            // courier ticket lacked the ref/hint/notes the dispatcher needs.
            if let Some(v) = &r.delivery_ref {
                self.row(&lab.delivery_ref, v, SZ_SMALL, Weight::NORMAL);
            }
            if let Some(v) = &r.payment_hint {
                self.row(&lab.payment_hint, v, SZ_SMALL, Weight::NORMAL);
            }
            if let Some(v) = &r.delivery_notes {
                self.indented(&format!("{}: {}", lab.notes, v), SZ_SMALL, 0);
            }
            self.rule();
        }

        // ── items ──
        for line in &r.lines {
            self.item(line, cur);
        }
        self.rule();

        // ── totals ──
        self.row(&lab.subtotal, &m(r.subtotal_minor), SZ_BODY, Weight::NORMAL);
        if r.discount_minor > 0 {
            self.row(&lab.discount, &format!("−{}", m(r.discount_minor)), SZ_BODY, Weight::NORMAL);
        }
        if r.tax_minor > 0 {
            self.row(&lab.tax, &m(r.tax_minor), SZ_BODY, Weight::NORMAL);
        }
        if r.delivery_fee_minor > 0 {
            self.row(&lab.delivery_fee, &m(r.delivery_fee_minor), SZ_BODY, Weight::NORMAL);
        }
        self.row(&lab.total.to_uppercase(), &m(r.total_minor), SZ_TOTAL, Weight::BOLD);
        if r.tip_minor > 0 {
            self.row(&lab.tip, &m(r.tip_minor), SZ_BODY, Weight::NORMAL);
        }
        if r.is_cash {
            self.row(&lab.cash, &m(r.amount_tendered_minor), SZ_BODY, Weight::NORMAL);
            self.row(&lab.change, &m(r.change_minor), SZ_BODY, Weight::NORMAL);
        }
        self.rule();

        // ── footer ──
        self.center(&r.payment_label.to_uppercase(), SZ_SMALL, Weight::SEMIBOLD);
        if let Some(t) = &r.teller_name {
            self.center(&format!("{} {}", lab.served_by, t), SZ_SMALL, Weight::NORMAL);
        }
        if r.queued_offline {
            self.center(&lab.queued, SZ_SMALL, Weight::NORMAL);
        }
        self.center(&lab.thank_you, SZ_BODY, Weight::NORMAL);
        self.gap(BOTTOM_PAD);
    }

    /// Walk the Z-report top-to-bottom — the full detailed layout, mirroring the
    /// Flutter `_buildShiftReportPdf`: header (store, business date, print/close
    /// time) → shift info (teller, opened, closed / interim) → PAYMENTS (per
    /// method + order count, total collected, transactions) → DRAWER OPERATIONS
    /// (pay-in/out + itemised movements with times) → CASH RECONCILIATION
    /// (opening, expected, actual, over/short) → voided → end.
    fn build_shift(&mut self, r: &ShiftReportView, store: &str, currency: &str, lab: &ShiftReportLabels) {
        let m = |minor: i64| money(minor, currency);
        let store_up = if store.trim().is_empty() { "SUFRIX".to_string() } else { store.to_uppercase() };

        // ── header ──
        self.center(&store_up, SZ_STORE, Weight::BOLD);
        self.center(&lab.title, SZ_BODY, Weight::SEMIBOLD);
        self.center(&format!("{}: {}", lab.business_date, date_only(&r.opened_at)), SZ_SMALL, Weight::NORMAL);
        match (r.is_open, &r.closed_at) {
            (false, Some(c)) => self.center(&format!("{}: {}", lab.closed, fmt_dt(c)), SZ_SMALL, Weight::NORMAL),
            _ => self.center(&format!("{}: {}", lab.printed_at, fmt_dt(&r.printed_at)), SZ_SMALL, Weight::NORMAL),
        }
        self.rule();

        // ── shift info ──
        self.row(&lab.teller, &r.teller_name, SZ_BODY, Weight::NORMAL);
        self.row(&lab.opened, &fmt_dt(&r.opened_at), SZ_SMALL, Weight::NORMAL);
        if r.is_open {
            self.center(&format!("— {} —", lab.interim), SZ_SMALL, Weight::NORMAL);
        } else if let Some(c) = &r.closed_at {
            self.row(&lab.closed, &fmt_dt(c), SZ_SMALL, Weight::NORMAL);
        }
        self.rule();

        // ── payments ──
        self.center(&lab.payments.to_uppercase(), SZ_SMALL, Weight::SEMIBOLD);
        let mut total_orders = 0i64;
        for pl in &r.payment_lines {
            total_orders += pl.order_count;
            self.row(&pl.method, &m(pl.total_minor), SZ_BODY, Weight::BOLD);
            self.indented(&format!("{} {}", pl.order_count, lab.orders), SZ_SMALL, 16);
        }
        self.row(&lab.total_collected, &m(r.total_payments_minor), SZ_BODY, Weight::BOLD);
        self.row(&lab.transactions, &total_orders.to_string(), SZ_SMALL, Weight::NORMAL);
        self.rule();

        // ── drawer operations ──
        self.center(&lab.drawer_ops.to_uppercase(), SZ_SMALL, Weight::SEMIBOLD);
        self.row(&lab.cash_in, &m(r.cash_in_minor), SZ_BODY, Weight::NORMAL);
        self.row(&lab.cash_out, &m(-r.cash_out_minor), SZ_BODY, Weight::NORMAL);
        for mv in &r.cash_movements {
            let label = if mv.note.trim().is_empty() { &mv.moved_by_name } else { &mv.note };
            self.indented(label, SZ_SMALL, 0);
            let sign = if mv.amount_minor < 0 { "−" } else { "+" };
            self.row(
                &format!("  {}", time_only(&mv.created_at)),
                &format!("{}{}", sign, m(mv.amount_minor.abs())),
                SZ_SMALL,
                Weight::NORMAL,
            );
        }
        self.rule();

        // ── cash reconciliation ──
        self.center(&lab.cash_recon.to_uppercase(), SZ_SMALL, Weight::SEMIBOLD);
        self.row(&lab.opening, &m(r.opening_cash_minor), SZ_BODY, Weight::NORMAL);
        // Opening mismatch: the teller counted a different opening float than the
        // suggested (last close) — show the signed difference + the reason.
        if r.opening_cash_was_edited {
            if let Some(orig) = r.opening_cash_original_minor {
                let diff = r.opening_cash_minor - orig;
                let sign = if diff < 0 { "−" } else { "+" };
                self.row(&lab.opening_mismatch, &format!("{}{}", sign, m(diff.abs())), SZ_SMALL, Weight::NORMAL);
            }
            if let Some(reason) = r.opening_cash_edit_reason.as_deref().filter(|s| !s.trim().is_empty()) {
                self.indented(&format!("{}: {}", lab.opening_reason, reason), SZ_SMALL, 16);
            }
        }
        self.row(&lab.expected, &m(r.expected_cash_minor), SZ_BODY, Weight::NORMAL);
        match r.closing_cash_declared_minor {
            Some(declared) => {
                self.row(&lab.actual, &m(declared), SZ_BODY, Weight::BOLD);
                // Difference = expected (system) − counted. Positive = short, negative = over.
                let diff = r.expected_cash_minor - declared;
                let (label, amount) = if diff == 0 {
                    (&lab.difference, 0)
                } else if diff > 0 {
                    (&lab.short_by, diff)
                } else {
                    (&lab.over_by, -diff)
                };
                self.row(label, &m(amount), SZ_BODY, Weight::BOLD);
            }
            None => self.center(&format!("({})", lab.not_closed), SZ_SMALL, Weight::NORMAL),
        }
        if r.voided_amount_minor > 0 {
            self.rule();
            self.row(&lab.voided, &m(r.voided_amount_minor), SZ_BODY, Weight::NORMAL);
        }
        self.rule();
        self.center(&format!("— {} —", lab.end_of_report), SZ_SMALL, Weight::SEMIBOLD);
        self.gap(BOTTOM_PAD);
    }

    /// One item line + its modifier/bundle breakdown (mirrors `lineBlock`).
    fn item(&mut self, line: &ReceiptLineView, cur: &str) {
        let name = name_with_size(&line.name, &line.size_label);
        self.row(&format!("{}× {}", line.qty, name), &money(line.line_total_minor, cur), SZ_BODY, Weight::BOLD);
        if line.is_bundle {
            for c in &line.components {
                self.indented(&format!("– {}", name_with_size(&c.name, &c.size_label)), SZ_SMALL, 16);
                for mo in c.addons.iter().chain(c.optionals.iter()) {
                    self.modifier(mo, cur, 32);
                }
            }
        } else {
            for mo in line.addons.iter().chain(line.optionals.iter()) {
                self.modifier(mo, cur, 16);
            }
        }
    }

    fn modifier(&mut self, m: &ReceiptModifierView, cur: &str, indent: i32) {
        if m.price_minor > 0 {
            // Indented label on the left, charge flush-right.
            let buf = self.shape(&format!("+ {}", m.name), SZ_SMALL, Weight::NORMAL, self.content_w() - indent);
            let (_, lh) = Self::measure(&buf);
            let right = self.shape(&format!("+{}", money(m.price_minor, cur)), SZ_SMALL, Weight::NORMAL, self.content_w());
            let (rw, rh) = Self::measure(&right);
            let oy = self.y;
            self.blit(&buf, MARGIN + indent, oy);
            self.blit(&right, PRINT_WIDTH as i32 - MARGIN - rw.round() as i32, oy);
            self.y += lh.max(rh).ceil() as i32;
        } else {
            self.indented(&format!("+ {}", m.name), SZ_SMALL, indent);
        }
    }
}

fn name_with_size(base: &str, size: &Option<String>) -> String {
    match size {
        Some(s) if !s.is_empty() => format!("{} ({})", base, s),
        _ => base.to_string(),
    }
}

/// `dd/MM/yyyy` from an RFC3339 timestamp (in its own offset), raw on parse error.
fn date_only(rfc3339: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(rfc3339)
        .map(|d| d.format("%d/%m/%Y").to_string())
        .unwrap_or_else(|_| rfc3339.to_string())
}

/// `hh:mm AM/PM` from an RFC3339 timestamp (in its own offset).
fn time_only(rfc3339: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(rfc3339)
        .map(|d| d.format("%I:%M %p").to_string())
        .unwrap_or_default()
}

// ── logo decode + dither ─────────────────────────────────────────────────────

/// Decode (PNG/JPEG), composite over white, scale to fit `max_w`×`max_h`
/// preserving aspect, then Floyd–Steinberg dither to 1-bit. Returns
/// `(width, height, ink)` where `ink[y*w+x]` is `255` (black) or `0` (white).
/// `None` if the bytes aren't a decodable image.
fn decode_logo(bytes: &[u8], max_w: u32, max_h: u32) -> Option<(usize, usize, Vec<u8>)> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w0, h0) = (rgba.width().max(1), rgba.height().max(1));
    let scale = (max_w as f32 / w0 as f32).min(max_h as f32 / h0 as f32).min(1.0);
    let tw = ((w0 as f32 * scale).round() as u32).max(1);
    let th = ((h0 as f32 * scale).round() as u32).max(1);
    let scaled = image::imageops::resize(&rgba, tw, th, image::imageops::FilterType::Lanczos3);

    let (w, h) = (scaled.width() as usize, scaled.height() as usize);
    // ink amount, 0.0 = white .. 1.0 = black (composited over white paper).
    let mut g = vec![0f32; w * h];
    for (i, p) in scaled.pixels().enumerate() {
        let [r, gc, b, a] = p.0;
        let af = a as f32 / 255.0;
        let over = |c: u8| c as f32 * af + 255.0 * (1.0 - af);
        let lum = 0.299 * over(r) + 0.587 * over(gc) + 0.114 * over(b);
        g[i] = 1.0 - lum / 255.0;
    }
    // Floyd–Steinberg.
    let mut out = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let old = g[i];
            let newp = if old >= 0.5 { 1.0 } else { 0.0 };
            out[i] = if newp == 1.0 { 255 } else { 0 };
            let err = old - newp;
            if x + 1 < w {
                g[i + 1] += err * 7.0 / 16.0;
            }
            if y + 1 < h {
                if x > 0 {
                    g[i + w - 1] += err * 3.0 / 16.0;
                }
                g[i + w] += err * 5.0 / 16.0;
                if x + 1 < w {
                    g[i + w + 1] += err * 1.0 / 16.0;
                }
            }
        }
    }
    Some((w, h, out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkout::ReceiptView;
    use crate::receipt::ReceiptLabels;

    fn ctx() -> EscPosCtx {
        EscPosCtx {
            store_name: "Cafe Sufrix".into(),
            currency: "EGP".into(),
            width: 48,
            labels: ReceiptLabels {
                order: "Order".into(),
                reference: "Ref".into(),
                voided: "VOIDED".into(),
                delivery: "Delivery".into(),
                channel_in_mall: "In-Mall".into(),
                channel_outside: "Outside".into(),
                customer: "Customer".into(),
                phone: "Phone".into(),
                address: "Addr".into(),
                zone: "Zone".into(),
                delivery_ref: "Delivery Ref".into(),
                payment_hint: "Payment".into(),
                notes: "Notes".into(),
                subtotal: "Subtotal".into(),
                discount: "Discount".into(),
                tax: "Tax".into(),
                delivery_fee: "Delivery".into(),
                total: "Total".into(),
                tip: "Tip".into(),
                cash: "Cash".into(),
                change: "Change".into(),
                payment: "Payment".into(),
                teller: "Teller".into(),
                served_by: "Served by".into(),
                queued: "Saved — will sync".into(),
                thank_you: "Thank you!".into(),
            },
        }
    }

    fn receipt() -> ReceiptView {
        ReceiptView {
            local_order_id: "abcdef12-0000-0000-0000-000000000000".into(),
            order_number: Some(42),
            order_ref: None,
            is_voided: false,
            lines: vec![ReceiptLineView {
                name: "شاورما".into(), // Arabic — exercises shaping + bidi
                qty: 2,
                size_label: Some("Large".into()),
                line_total_minor: 12000,
                is_bundle: false,
                addons: vec![ReceiptModifierView { name: "Extra cheese".into(), price_minor: 500 }],
                optionals: vec![],
                components: vec![],
            }],
            payment_label: "Cash".into(),
            subtotal_minor: 12500,
            discount_minor: 0,
            tax_minor: 0,
            delivery_fee_minor: 0,
            total_minor: 12500,
            tip_minor: 0,
            amount_tendered_minor: 15000,
            change_minor: 2500,
            is_cash: true,
            customer_name: None,
            teller_name: Some("Mona".into()),
            is_delivery: false,
            delivery_channel: None,
            customer_phone: None,
            delivery_address: None,
            delivery_zone: None,
            delivery_ref: None,
            payment_hint: None,
            delivery_notes: None,
            queued_offline: false,
            created_at: "2026-06-24T18:30:00+03:00".into(),
        }
    }

    #[test]
    fn renders_full_width_nonblank_bitmap() {
        let bmp = render_receipt(&receipt(), &ctx(), None);
        assert_eq!(bmp.width, PRINT_WIDTH);
        assert!(bmp.rows > 200, "receipt should be a few hundred dots tall, got {}", bmp.rows);
        assert_eq!(bmp.bytes.len(), bmp.row_bytes() * bmp.rows as usize);
        // Not blank: shaping actually put ink down.
        assert!(bmp.bytes.iter().any(|&b| b != 0), "bitmap is entirely blank");
    }

    #[test]
    fn render_is_deterministic() {
        let (a, b) = (render_receipt(&receipt(), &ctx(), None), render_receipt(&receipt(), &ctx(), None));
        assert_eq!(a, b, "same input must produce a byte-identical bitmap");
    }

    #[test]
    fn ignores_undecodable_logo() {
        // A bogus "logo" must not panic or abort the render — just no logo.
        let bmp = render_receipt(&receipt(), &ctx(), Some(b"not an image"));
        assert!(bmp.rows > 200);
    }

    fn shift_labels() -> ShiftReportLabels {
        ShiftReportLabels {
            title: "Till Close Report".into(),
            business_date: "Business Date".into(),
            printed_at: "Printed at".into(),
            teller: "Teller".into(),
            opened: "Opened".into(),
            closed: "Closed".into(),
            interim: "Interim Report (Shift Still Open)".into(),
            payments: "Payments".into(),
            orders: "orders".into(),
            total_collected: "Total Collected".into(),
            drawer_ops: "Drawer Operations".into(),
            cash_in: "Cash in".into(),
            cash_out: "Cash out".into(),
            cash_recon: "Cash Reconciliation".into(),
            opening: "Opening cash".into(),
            opening_mismatch: "Opening mismatch".into(),
            opening_reason: "Reason".into(),
            expected: "Expected in Drawer".into(),
            actual: "Actual in Drawer".into(),
            not_closed: "Shift not yet closed".into(),
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

    fn shift_report() -> ShiftReportView {
        ShiftReportView {
            teller_name: "Mona".into(),
            opened_at: "2026-06-24T09:00:00+03:00".into(),
            closed_at: Some("2026-06-24T21:30:00+03:00".into()),
            printed_at: "2026-06-24T21:31:00+03:00".into(),
            is_open: false,
            closing_cash_declared_minor: Some(27500),
            opening_cash_was_edited: true,
            opening_cash_original_minor: Some(8000),
            opening_cash_edit_reason: Some("Float top-up".into()),
            expected_cash_minor: 28000,
            opening_cash_minor: 10000,
            total_payments_minor: 20000,
            net_payments_minor: 20000,
            voided_amount_minor: 750,
            cash_movements_net_minor: -2000,
            cash_in_minor: 0,
            cash_out_minor: 2000,
            payment_lines: vec![
                crate::shift::ShiftReportPaymentLine { method: "Cash".into(), is_cash: true, order_count: 5, total_minor: 12000 },
                crate::shift::ShiftReportPaymentLine { method: "Card".into(), is_cash: false, order_count: 3, total_minor: 8000 },
            ],
            cash_movements: vec![crate::shift::ShiftReportCashLine {
                amount_minor: -2000,
                note: "مصروف".into(), // Arabic note — exercises shaping in the report too
                moved_by_name: "Mona".into(),
                created_at: "2026-06-24T19:00:00+03:00".into(),
            }],
            from_server: true,
        }
    }

    #[test]
    fn renders_shift_report_bitmap() {
        let bmp = render_shift_report(&shift_report(), "Cafe Sufrix", "EGP", &shift_labels());
        assert_eq!(bmp.width, PRINT_WIDTH);
        assert!(bmp.rows > 150);
        assert!(bmp.bytes.iter().any(|&b| b != 0), "shift report bitmap is blank");
    }

    fn save_png(bmp: &Bitmap, path: &str) {
        let wb = bmp.row_bytes();
        let mut img = image::GrayImage::new(bmp.width, bmp.rows);
        for y in 0..bmp.rows {
            for x in 0..bmp.width {
                let bit = bmp.bytes[y as usize * wb + (x >> 3) as usize] & (0x80 >> (x & 7));
                img.put_pixel(x, y, image::Luma([if bit != 0 { 0 } else { 255 }]));
            }
        }
        img.save(path).unwrap();
        eprintln!("wrote {} ({}x{})", path, bmp.width, bmp.rows);
    }

    // Visual check: `cargo test -p sufrix-core dump_receipt_png -- --ignored`
    // writes the rendered receipt + shift report to /tmp to be eyeballed.
    #[test]
    #[ignore]
    fn dump_receipt_png() {
        save_png(&render_receipt(&receipt(), &ctx(), None), "/tmp/receipt.png");
        save_png(&render_shift_report(&shift_report(), "Cafe Sufrix", "EGP", &shift_labels()), "/tmp/shift_report.png");
        // Synthetic 4:1 wordmark (border + diagonal) to check logo sizing.
        let mut logo = image::RgbaImage::new(800, 200);
        for (x, y, px) in logo.enumerate_pixels_mut() {
            let border = x < 10 || x >= 790 || y < 10 || y >= 190;
            let diag = (x as i64 / 4) - y as i64;
            *px = if border || diag.abs() < 8 { image::Rgba([0, 0, 0, 255]) } else { image::Rgba([0, 0, 0, 0]) };
        }
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(logo).write_to(&mut png, image::ImageFormat::Png).unwrap();
        save_png(&render_receipt(&receipt(), &ctx(), Some(png.get_ref())), "/tmp/receipt_logo.png");
    }
}
