I now have a complete and verified picture of the printing subsystem. The audit follows.

---

# Madar POS — Printing Subsystem Audit (for Rust core reimplementation)

## 1. File inventory (printing-related)

| File (absolute) | Role |
|---|---|
| `/Users/shawket/Desktop/sufrix_pos/lib/core/services/printer_service.dart` | **The whole engine.** Static `PrinterService` class: builds 3 PDF receipts (order, delivery, shift report), rasterizes them, owns transport for both printer brands, the ESC/POS bit-image encoder, cash-drawer kick, and logo download/cache. 936 lines — the one file to port. |
| `/Users/shawket/Desktop/sufrix_pos/lib/core/models/branch.dart` | Façade re-exporting generated `Branch` + `PrinterBrand`; defines `branch.hasPrinter` getter that gates every print entry point (IP present AND brand recognized). |
| `/Users/shawket/Desktop/sufrix_pos/packages/madar_api/lib/src/model/printer_brand.dart` | Generated enum: `star` / `epson` / `unknown_default_open_api`. Wire values come from backend. |
| `/Users/shawket/Desktop/sufrix_pos/lib/core/models/order.dart` | Façade: app `Order` = generated `OrderFull`; `OrderItem` = `OrderItemFull`. Adds `isBundleLine` (bundleId != null). Primary receipt data model. |
| `/Users/shawket/Desktop/sufrix_pos/lib/core/models/delivery_order.dart` | Client-side delivery order model (frozen cart snapshot) — feeds the delivery receipt PDF before finalize. |
| `/Users/shawket/Desktop/sufrix_pos/lib/core/models/shift_report.dart` | Façade: `ShiftReport` = `ShiftReportResponse`; `PaymentSummaryItem`, `CashMovementItem`. Feeds the Z-report ("Till Close Report") PDF. |
| `/Users/shawket/Desktop/sufrix_pos/lib/core/models/payment_method.dart` | Payment method list w/ `wireFormat` + `labelTranslations`; used to humanize payment labels on receipts. |
| `/Users/shawket/Desktop/sufrix_pos/lib/core/utils/formatting.dart` | `egp(piastres)` currency formatter (÷100, drop trailing `.00`) — money on every line. Must be replicated exactly. |
| `/Users/shawket/Desktop/sufrix_pos/lib/core/utils/app_tz.dart` | Branch-timezone clock (`AppTz.local`). Receipts/Z-reports print **branch** local time, not device time. |
| `/Users/shawket/Desktop/sufrix_pos/lib/features/order/widgets/receipt_sheet.dart` | **Trigger:** in-store order-complete sheet; auto-prints on open (`postFrameCallback`), kicks drawer when payment is cash, offers Reprint. |
| `/Users/shawket/Desktop/sufrix_pos/lib/features/order/widgets/receipt_preview_sheet.dart` | **Trigger (reprint):** order-history reprint; enriches delivery orders via detail fetch before reprinting. Renders an on-screen **widget** preview (not the PDF). |
| `/Users/shawket/Desktop/sufrix_pos/lib/features/shift/close_shift_screen.dart` | **Trigger:** `_autoPrintReport` — best-effort auto-print of the Z-report right after an online shift close; re-fetches the report from backend first. |
| `/Users/shawket/Desktop/sufrix_pos/lib/features/shift/shift_report_preview_sheet.dart` | **Trigger (reprint):** manual Z-report print/reprint from shift history. Widget preview. |
| `/Users/shawket/Desktop/sufrix_pos/lib/features/delivery/delivery_orders_screen.dart` | **Trigger:** `_maybePrintReceipt` — prints delivery receipt once at Confirm/accept (guards on `order.receiptPrinted`). |
| `/Users/shawket/Desktop/sufrix_pos/lib/shared/widgets/animated_icons.dart` | UI only: `PrinterPainter` printing spinner. Not logic. |
| `~/.pub-cache/.../starxpand_sdk_wrapper-1.0.2` | **Native plugin (iOS/Android only).** Pigeon bridge to Star's SDK: `startDiscovery`, `connect`, `printPdf` (rasters PDF → PNG → `printImage`), `openCashDrawer`, `getStatus`, `disconnect`. |
| `pubspec.yaml` deps | `pdf ^3.11.1` (build PDF), `printing ^5.14.2` (`Printing.raster` PDF→bitmap), `starxpand_sdk_wrapper ^1.0.2` (Star native), `flutter_cache_manager ^3.4.1` (logo cache). |

### How the three concerns actually work today

**(1) Discovery / connection / transport.** No discovery is used in production — printers are configured by IP, not scanned (the wrapper *has* `startDiscovery` but the app never calls it). Transport is **LAN/TCP only**:
- **Star** (`_printStar`): connects via `StarXpand` native SDK over `StarInterfaceType.lan` to the device IP; drawer kick is a raw `Socket` to **port 9100** sending Star BEL `0x07`; PDF goes through the native SDK's `printPdf` (which itself rasters via `Printing.raster` at 203 dpi → PNG → `printImage`, width 576).
- **Epson** (`_printEpson`): pure Dart — opens a raw `Socket` to `ip:port` (default 9100), `tcpNoDelay`, sends ESC/POS init + drawer-kick (`ESC p 0 0x19 0xFA`) + a **raster bit-image** (`GS v 0` block) built in-app by `_pngToEscPos`, then feed+cut (`ESC d 5`, `GS V A 5`). The PDF is rasterized to PNG via `Printing.raster(dpi:203)` first.
- `_send` strips a CIDR suffix off the IP (`ip.split('/').first`) and dispatches on brand. Bluetooth/USB/serial are **not** implemented anywhere.

**(2) Content rendering.** All three documents are rendered as **PDF** (`pdf` package, `pw.Page`, page width `72mm`, infinite height) by `_buildReceiptPdf`, `_buildDeliveryReceiptPdf`, `_buildShiftReportPdf`. The PDF is then **rasterized to a 203-dpi bitmap** and pushed to the printer as an image — there is **no text-mode ESC/POS** path; everything prints as a graphic. Helpers `_row` (Row+Expanded for flush-right numbers), `_divider`, `_thinDivider`, `_fmtDt`, `_fmtPayment`, `_channelLabel`. The on-screen preview sheets render **Flutter widgets**, a separate visual layer — not the PDF.

**(3) Triggers.** Order-complete auto-print (receipt_sheet, drawer kick if cash); manual reprint (receipt_preview_sheet, shift_report_preview_sheet); delivery accept (once, guarded by `receiptPrinted`); shift-close auto-print of backend-refetched Z-report (close_shift_screen). No kitchen tickets exist — every `kitchen`/`ticket` grep hit was a false positive (icon names, "Avg ticket" stat label).

**(4) Config storage.** Per-branch, **server-sourced**: `Branch.printerIp`, `Branch.printerPort` (nullable, default 9100), `Branch.printerBrand` (enum). Wire keys `printer_ip` / `printer_port` / `printer_brand`. Read at print time from `authProvider.branch`. No local override / settings screen for printer config. `branch.hasPrinter` is the universal gate; unknown brand → treated as "no printer."

---

## 2. Moves to Rust / Stays native

### Moves to Rust (portable, pure-logic core)
| Concern | Rationale |
|---|---|
| **Document model → layout** | The receipt/delivery/Z-report structure (header, items+bundles+addons+optionals, totals, footer, drawer ops, cash reconciliation) is pure data transformation. Owns business rules: subtotal-shows-only-when-needed, shortage/over math (`system − declared`), VOIDED/DELIVERY banners, bundle-component indentation. |
| **Epson ESC/POS encoder** (`_pngToEscPos`) | Pure byte math: alpha-over-white compositing, luminance threshold `<128`, `GS v 0` raster framing, feed/cut. Trivially portable and the obvious thing Rust does better. Drawer-kick byte sequences for both brands are constants. |
| **TCP transport for Epson + the Star drawer-kick socket** | Raw socket to `ip:9100` is plain TCP — Rust `std::net::TcpStream` / async. The Epson path needs zero native code once rasterization is solved. |
| **Currency formatting** (`egp`) | ÷100, drop `.00`, `EGP ` prefix — must match Dart byte-for-byte. |
| **Branch-timezone formatting** (`AppTz` + `_fmtDt`) | IANA tz conversion + `dd/MM/yyyy hh:mm a` / `hh:mm a`. Rust `chrono-tz`. Critical: branch zone, never device zone. |
| **Payment-label humanization** (`_fmtPayment`, `_channelLabel`) | Pure string/lookup logic over the payment-method list. |
| **`hasPrinter` gating + brand dispatch** | Pure config validation. |

### Stays native (platform-bound)
| Concern | Rationale |
|---|---|
| **Star printing transport** | Goes through Star's proprietary **iOS/Android SDK** via `starxpand_sdk_wrapper` (Pigeon). The wrapper's `pubspec` declares **android + ios only** — there is no desktop/Rust binding. Star's `connect`/`printPdf`/`openCashDrawer`/`getStatus` must remain a native FFI/platform-channel call unless Star is dropped or re-spoken at the raw socket protocol level. |
| **PDF → bitmap rasterization** | Both brands ultimately print a **raster image**. Today that's `pdf` (build) + `printing`'s `Printing.raster` (Skia/PDFium-backed, platform graphics). Rust would need its own PDF/PDFium or a vector-text→bitmap renderer with embedded fonts — feasible but the single biggest porting cost. **Decision point:** either (a) keep building PDF + raster natively and have Rust only own layout-as-data + Epson encoding, or (b) move to a Rust raster pipeline (e.g. render layout straight to a 1-bpp bitmap with a font shaper), which removes the `pdf`/`printing` deps entirely and is the cleaner long-term target. |
| **Font asset loading** | `rootBundle.load('assets/fonts/Cairo-*.ttf')` and `IconForeground.png` — bundled Flutter assets. Rust core needs these fonts/images shipped alongside it (embed via `include_bytes!` or load from a known path). |
| **Logo download + cache** (`_downloadImage` via `flutter_cache_manager`) | Currently uses Flutter's cache manager + `HttpClient` fallback. The fetch logic ports to Rust (`reqwest`), but the *cache backing* is platform/Flutter-specific; replace with a Rust cache or pass pre-fetched bytes in. |
| **Trigger orchestration & UI state** | The "when to print" wiring (Riverpod notifiers, postFrame auto-print, snackbars, reprint buttons, `receiptPrinted` guard) is Flutter/Riverpod. Rust can expose a `print(doc, config) -> Result` API; the *invocation* and *progress UI* stay in Flutter. |

---

## 3. Gotchas

- **Everything prints as an image, not text.** No ESC/POS text mode. This is *why* Arabic works at all — there is no codepage dependency. A Rust port that switches to text-mode ESC/POS would break Arabic/Cairo entirely. Keep the raster approach.
- **Arabic / Cairo / RTL.** Receipts embed `Cairo-Regular.ttf` + `Cairo-SemiBold.ttf` and rely on the PDF engine's text shaping (ligatures, RTL bidi) for Arabic branch names, item names, customer names, cash-movement notes. A Rust raster pipeline must do real **Unicode bidi + Arabic shaping/ligatures** (e.g. `rustybuzz`/`harfbuzz` + a bidi pass), not naive left-to-right glyph placement. The current `_row` deliberately uses Row+Expanded so numbers stay flush-right "regardless of Cairo glyph widths" — preserve that two-column flush-right behavior.
- **Mixed-direction lines.** Lines like `2x سندويتش (Large)   EGP 50` mix Arabic item text with LTR price/qty — the trickiest shaping case. Latin and Arabic coexist in one document.
- **Paper width is hard-coded to 80mm / 576 dots.** `_printerWidth = 576` (Star), `width ?? 576` in the wrapper, PDF page `72mm`, raster at **203 dpi**. 58mm printers are not supported. Make width a parameter in the Rust core rather than re-baking the constant.
- **DPI must stay 203.** Bit-image math (`(w+7)/8` bytes/row, the `GS v 0` height/width little-endian fields) assumes the 203-dpi raster. Changing dpi without re-deriving the framing produces garbage.
- **Image binarization is alpha-aware.** `_pngToEscPos` composites RGBA over white before luminance-thresholding at `<128`. A logo PNG with transparency will smear if the port skips the alpha-over-white step.
- **Logo fallback.** If the org logo URL is missing/unreachable, it falls back to bundled `assets/IconForeground.png`. The download has a 3–4s timeout and silently swallows errors — never block/fail a print on the logo.
- **Drawer-kick is brand-specific and best-effort.** Star = separate raw socket on **9100** sending `0x07` (fails silently, never aborts the print). Epson = inline `ESC p 0 0x19 0xFA` before the image. Only fired when payment method is cash (`isCashMethod`).
- **IP may carry a CIDR suffix** — `_send` does `ip.split('/').first`. Replicate this sanitization.
- **Timezone trap.** All timestamps use `AppTz.local` (branch IANA zone), explicitly *not* the till's OS clock, because tills are often mis-set. The Rust core must be told the branch timezone, not infer system local time.
- **Two-pass connect for Star.** `connect` → `printPdf` → `disconnect` in a `finally`; failure to connect returns a user-facing string. Star errors are returned as strings, not thrown — the whole API returns `String?` (null = success). Preserve this error contract if the Flutter UI keeps consuming it.
- **Money is integer piastres end-to-end** (`int`, ÷100 only at format time). Do not introduce float money in the port.