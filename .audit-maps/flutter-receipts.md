I now have the complete picture. Here is the exhaustive spec.

---

# Sufrix POS — RECEIPT SYSTEM MAP (spec to mirror 1:1)

## 0. Critical architectural fact: ON-SCREEN ≠ PRINTED

There are **two entirely separate renderers** that are NOT the same widget:

1. **On-screen preview** — `ThermalReceiptCard` (a Flutter `Widget` tree) in `receipt_preview_sheet.dart`. Pure UI, white-paper styled, scrollable in a sheet.
2. **Printed output** — `_buildReceiptPdf` (a `pdf` package `pw.Document`) in `printer_service.dart`. Rasterised and sent over TCP.

They are visually similar but hand-maintained independently, with different fonts (UI font tokens vs bundled Cairo TTF), different sizes, and the PDF carries extra fields (delivery block, delivery fee, ref styling) the on-screen card does not. To mirror "the preview," port `ThermalReceiptCard`. To mirror "what prints," port the PDF builder. **They drift — document both.**

There is **no Share/Export action anywhere** — only Print / Reprint / Close. No barcode, no QR, no social, no tax-id, no address/phone on the in-store receipt (those wire fields exist on `Branch` but are unused). The header is just **logo + branch name**.

---

## 1. RECEIPT VISUAL STYLE (top-to-bottom)

### 1A. On-screen preview card — `ThermalReceiptCard` (`receipt_preview_sheet.dart` L296–512)

**Theme invariance (important):** the card uses a hardcoded `static const AppTokens _ink = AppTokens.light` for ALL ink colors, and `color: Colors.white` for the paper — it ignores the app's dark/light theme so the preview always looks like physical white thermal paper with dark ink. Mirror this: receipt is always light, regardless of app theme.

**Card container:** white fill; `borderRadius: AppRadius.md` (16); 1px border `t.border` (uses live theme border only for the card frame); `boxShadow: AppShadows.of(t)`. Children `crossAxisAlignment: stretch`.

Paper width on screen: the card is wrapped in `ConstrainedBox(maxWidth: 420)` and `Center`-ed inside the scroll body (preview-sheet L142–147). So it reads as a wide ~420pt receipt, not a literal 58/80mm strip.

Vertical structure (exact, top→bottom):
1. **Logo + branch block** (`_buildLogoAndBranch`)
2. `SizedBox(AppSpace.md=12)`
3. `_DottedLine`
4. `SizedBox(md=12)`
5. **Order details** (`_buildOrderDetails`)
6. `SizedBox(md=12)` → `_DottedLine` → `SizedBox(md=12)`
7. **Items list** (`_buildItemsList`)
8. `SizedBox(AppSpace.sm=8)` → `_DottedLine` → `SizedBox(md=12)`
9. **Summary / totals** (`_buildSummary`)
10. `SizedBox(AppSpace.lg=16)` → `_DottedLine` → `SizedBox(14)`
11. **Thank-you** centered, `ui(size:11, color:_ink.textMuted)`, text = `s.receiptThankYou`
12. `SizedBox(AppSpace.xl=24)`

**LOGO (`_buildLogoAndBranch`, L351–384):**
- `hasLogo = branch?.orgLogoUrl != null && isNotEmpty`.
- Source: **network URL** `branch.orgLogoUrl` (the org's logo, resolved per-branch). Field is `Branch.orgLogoUrl` (wire `org_logo_url`, `String?`) — there is no separate org-name field; the org's logo is the only org branding.
- Rendered via `Image(image: CachedNetworkImageProvider(branch.orgLogoUrl!), width:72, height:72)`.
- `errorBuilder` → placeholder.
- **Placeholder (`_buildPlaceholderLogo`):** bundled asset `Image.asset('assets/IconForeground.png', width:72, height:72)`. Used when no logo URL OR network image fails.
- Padding around logo: `EdgeInsets.only(top: AppSpace.xl=24, bottom: AppSpace.sm=8)`.
- **Branch name** below logo, centered: `branch?.name ?? 'Sufrix POS'`, `ui(size:15, weight:w700, color:_ink.textPrimary)`. (No org name, no address, no phone, no tax id.)

**ORDER DETAILS (`_buildOrderDetails`, L386–438):** left-aligned `_ReceiptInfoRow`s, horizontal padding `AppSpace.lg=16`, each separated by `SizedBox(AppSpace.xs=4)`.
- **Voided stamp** (only if `order.isVoided`): full-width centered box, `_ink.dangerBg` fill, `_ink.danger@0.3` border, radius 4, `vertical: AppSpace.xs` padding, `margin bottom sm`; text `s.receiptVoidedStamp` = `"*** VOIDED ***"`, `ui(size:14, weight:w800, color:_ink.danger, letterSpacing:1.5)`.
- Row "Order #": label `s.receiptOrderLabel` (`"Order #"`), value = `order.orderNumber == 0 ? s.receiptDraft ("DRAFT") : '#${order.orderNumber}'`.
- Row "Ref" (only if `order.orderRef != null`): hardcoded label literal `'Ref'`, value `order.orderRef!`.
- Row "Date": label `s.receiptDate` (`"Date"`), value `dateTime(order.createdAt)` → format `'MMM d, hh:mm a'` in **branch timezone** (AppTz).
- Row "Teller" (only if `order.tellerName.isNotEmpty`): label `s.commonTeller` (`"Teller"`), value `order.tellerName`.
- Row "Customer" (only if `order.customerName != null`): label `s.orderReceiptCustomer` (`"Customer"`).
- Row "Payment": label `s.orderPaymentMethod` (`"Payment"`), value `methodLabel(methods, locale, order.paymentMethod)` (localized payment label).
- **Not shown on screen:** order type, table, address, phone, tax id, tip, change, delivery block, delivery fee.

`_ReceiptInfoRow` layout (L654–685): `Row`, fixed-width label column `SizedBox(width:80)` `ui(size:12, w500, _ink.textSecondary)` + `Expanded` value `ui(size:12, w700, _ink.textPrimary)`.

**ITEMS LIST (`_buildItemsList`, L440–469):** horizontal padding `lg=16`.
- Section header text `s.receiptItems` = `"ITEMS"` (uppercase in arb), `ui(size:10, weight:w800, color:_ink.textMuted, letterSpacing:1.0)`. Then `SizedBox(sm=8)`.
- Empty: `s.receiptNoItems` = `"No items in cart"`, `ui(12, textSecondary)`.
- Else map each `order.items` → `ReceiptItemRow`.

**`ReceiptItemRow` (L515–650):** each item is a `Column`, bottom margin `AppSpace.md=12`.
- **Main line `Row`** (crossAxis start):
  - qty prefix `'${item.quantity}x '`, `ui(size:13, w700, color:_ink.navy)`.
  - `Expanded` name = `item.itemName` + (if `sizeLabel != null` → ` · ${normaliseName(sizeLabel)}`), `ui(13, w600, _ink.textPrimary)`. `normaliseName` = Title Case.
  - `SizedBox(sm=8)`.
  - line total `egp(item.lineTotal)`, `money(13, w700, _ink.textPrimary)`.
- **If bundle line** (`item.isBundleLine` && components non-empty): indented block `start:20, top:4`. Each component:
  - `'– ${normaliseName(c.itemName)}$sizePart × $qty'` where `qty = c.quantity * item.quantity`, `ui(11, w600, _ink.textSecondary)`. (sizePart = ` · Title`.)
  - component **addons** (indent `start:12, top:1`): `'+ ${normaliseName(a.name)}${qty>1?" ×N":""}${linePrice>0?" (egp)":""}'`, `ui(size:10, color:_ink.navy)`. `linePrice = a.priceModifier * a.quantity`.
  - component **optionals** (indent `start:12, top:1`): `'• ${normaliseName(o.name)}${o.price>0?" (egp)":""}'`, `ui(size:10, color:_ink.warning)`.
- **Else (à-la-carte)** addons/optionals indented `start:20, top:2`:
  - addons: `'+ ${normaliseName(a.addonName)} (${egp(a.lineTotal)})'`, `ui(11, _ink.textMuted)`.
  - optionals: `'• ${normaliseName(o.fieldName)} (${egp(o.price)})'`, `ui(11, _ink.textMuted)`.

Note the color coding: qty + bundle-addon = **navy**; bundle-optional = **warning/amber**; à-la-carte modifiers = **muted grey**. Bullet glyphs: `+` addon, `•` optional, `–` bundle component, `×` multiplier.

**SUMMARY (`_buildSummary`, L471–511):** horizontal padding `lg=16`.
- `_ReceiptAmountRow` Subtotal: `s.orderSubtotal` / `egp(order.subtotal)`.
- Discount (if `order.discountAmount > 0`): `s.orderDiscount`, value `'- ${egp(discountAmount)}'`, `valueColor: _ink.success` (green). Preceded by `SizedBox(xs)`.
- Tax (if `order.taxAmount > 0`): `s.orderTax14` = `"Tax (14%)"`, `egp(order.taxAmount)`. Preceded by `SizedBox(xs)`.
- `SizedBox(sm=8)`, then **TOTAL row** (not an AmountRow — bespoke `Row`): label `s.orderTotal.toUpperCase()` (`"TOTAL"`), `ui(15, w800, _ink.textPrimary)`; value `egp(order.totalAmount)`, `money(17, w800, color:_ink.navy)`.
- **On screen the summary has no tip, no delivery fee, no change, no payment-tendered line.** (Tip/change appear only in the post-checkout `ReceiptSheet`, see §2B.)

`_ReceiptAmountRow` (L687–722): `Row`, `Expanded` label `ui(12, w500, textSecondary)` + value `money(13, w600, valueColor ?? textPrimary)`.

**Dividers — `_DottedLine` (L724–755):** horizontal padding `lg=16`. A `LayoutBuilder` that lays `dashWidth=4, dashSpace=4` 1px tall dashes via `Row(spaceBetween)`, color `AppTokens.light.border` (theme-invariant). Count = `floor(width / 8)`. (Mirror: dotted/dashed hairline, 4-on/4-off, 1px.)

**Fonts/weights/spacing recap:** all text uses font family **Cairo** (`ui()`/`money()` helpers). `money()` adds `FontFeature.tabularFigures()` so amounts align. Spacing scale (4-pt): `xs=4, sm=8, md=12, lg=16, xl=24, xxl=32`. Radius scale: `xs=8, sm=12, md=16, lg=20, xl=24, xxl=32`.

---

### 1B. Printed receipt PDF — `_buildReceiptPdf` (`printer_service.dart` L288–514)

This is the truth for what physically prints. Differs from the on-screen card.

**Paper / layout constants:**
- Page format: `PdfPageFormat(72 * mm, double.infinity)` → **72mm wide** (≈ 80mm-class paper), infinite roll height. Margins **2mm** all sides.
- Rasteriser target width `_printerWidth = 576` dots (printer_service L20); Star prints PDF at `width:576`; Epson rasterised at **203 dpi** then converted to ESC/POS via `_pngToEscPos`.
- Fonts: bundled TTFs `assets/fonts/Cairo-Regular.ttf` (font) and `assets/fonts/Cairo-SemiBold.ttf` (fontB). All text sizes are tiny PDF points (7–10).

**LOGO (PDF):** `pw.Center(pw.Image(logo, width:56))`.
- `logo` resolution order: download `logoUrl` (org logo) via `_downloadImage` → else fallback bundled `assets/IconForeground.png` as `MemoryImage`. `_downloadImage` tries (1) `DefaultCacheManager` cache, (2) `getSingleFile` (4s timeout), (3) raw `HttpClient` GET (3s timeout) — returns null on all-fail, then asset fallback kicks in.
- **No org name on the PDF**, only branch name.

**Header block (L343–358):**
1. logo (width 56, centered)
2. `SizedBox(2)`
3. branch name centered, `ts(font, sz:7.5)`.
4. **Delivery banner** (only if `order.orderType=='delivery'`): centered `'*** DELIVERY — {channel} ***'` or `'*** DELIVERY ***'`, `ts(fontB, sz:9)`. Channel via `_channelLabel`: `in_mall→In-Mall`, `outside→Outside`.
5. `SizedBox(2)` → `_divider()`.

**Dividers (PDF):** `_divider()` = `pw.Divider(thickness:0.4, color:grey600, height:6)` (solid hairline). `_thinDivider()` = `thickness:0.2, grey400, height:4`. **NOTE: PDF uses SOLID grey dividers, the on-screen card uses DOTTED. They differ.**

**Order # + timestamp (L361–372):**
- Voided (if `order.isVoided`): centered `'*** VOIDED ***'`, `ts(fontB, sz:10)`, bottom pad 2.
- `_row('Order #${order.orderNumber}', dts, bold:true, sz:8)` — label left, timestamp flush-right.
- Ref (if non-null): `_row('Ref: ${order.orderRef}', dts, sz:8)`.
- `_divider()`.
- `dts = _fmtDt(order.createdAt)` → `'dd/MM/yyyy  hh:mm a'` in **branch timezone** (`AppTz.local`). (Different format string from the on-screen `dateTime()` `'MMM d, hh:mm a'`.)

**Delivery block (PDF only, if delivery — L376–402):** rows for Customer, Phone (`dInfo.customerPhone`), `Address: {parts}` (placeName, addressLine, `Unit X`, `Floor X`, landmark joined by `, `), Zone, Delivery Ref, Payment (hint), Notes — all `sz:7.5`, then `_divider()`. (Phone/address only present on the detail-fetched order.)

**Items (L405–475):** uses shared `_row(label, value)` (label `Expanded` + value right). All `_fmtPayment`/`egp` formatting. 
- Bundle line: `_row('${qty}x ${itemName}', egp(lineTotal), bold:true, sz:8)`; components `'  - ${itemName}${ (sizeLabel) }'` at `left:8 sz:7.5`; component addons/optionals `'    + name'` at `sz:7, leftIndent:8` (priced) or padded `left:12` (unpriced). Bundle sizePart format here is `' (${sizeLabel})'` **parentheses, raw — NOT normaliseName** (differs from on-screen ` · Title`).
- À-la-carte: `_row('${qty}x ${itemName}${(size)}', egp(lineTotal), bold:true, sz:8)`; addons `'  + ${addonName}'` value `'+${egp(unitPrice)}'` (`sz:7.5, leftIndent:4`); optionals `'  + ${fieldName}'` value `'+${egp(price)}'`. Unpriced → plain padded text.
- `_divider()`.

**Totals (L480–493):**
- Subtotal shown **only if** `discountAmount>0 || deliveryFee>0` (so the math reconciles): `_row('Subtotal', egp(subtotal), sz:8)`.
- Discount (if >0): `_row('Discount', '- ${egp}', valueColor:red700, sz:8)`.
- Tax (if >0): `_row('Tax', egp(taxAmount), sz:8)`. (PDF label is plain `"Tax"`, on-screen is `"Tax (14%)"`.)
- Delivery Fee (if >0): `_row('Delivery Fee', egp(deliveryFee), sz:8)`. **(PDF only — not on screen.)**
- `_row('TOTAL', egp(totalAmount), bold:true, boldValue:true, sz:10)`.
- `_divider()`.

**Footer metadata (L496–510):**
- `_row('Payment', _fmtPayment(order.paymentMethod, methods), sz:7.5)`. `_fmtPayment` prefers method's `labelTranslations['en']`, else title-cases & de-underscores raw.
- Customer (if not delivery & present): `_row('Customer', ..., sz:7.5)`.
- Teller (if present): `_row('Teller', tellerName, sz:7.5)`.
- `SizedBox(4)`, centered `'Thank you for visiting!'` (**hardcoded literal in PDF, not the arb key**), `ts(font sz:7.5)`, `SizedBox(2)`, `_divider()`.
- **No barcode/QR/social/tax-id anywhere.**

> Delivery customer receipt (`_buildDeliveryReceiptPdf`, L518–661) and shift report (`_buildShiftReportPdf`, L665–905) follow the same header/divider grammar; delivery footer literal is `'Thank you for your order!'`, shift report footer is `'— End of Report —'`.

---

## 2. PREVIEW / RECEIPT SHEETS (UX)

All sheets use **`ResponsiveSheet.show`** → a `showModalBottomSheet` (bottom-anchored, `isScrollControlled:true`, transparent bg) wrapped in `Align(bottomCenter) + ConstrainedBox(maxWidth: 600)`. So every sheet is a centered bottom sheet capped at **600pt wide**. Sheet top corners: `AppRadius.sheetRadius` = `vertical top Radius.circular(24)`.

### 2A. `ReceiptPreviewSheet` (reprint/preview — `receipt_preview_sheet.dart`)
- **Entry points:** order history row tap (`order_history_screen.dart:1355`), checkout draft preview (`checkout_sheet.dart:350`, with custom title), past-shift order tap (`shift_history_screen.dart:727`). `static show(context, order, {title})`.
- **Container:** `maxHeight: 92% screen`, `color: t.bg`, sheetRadius. `Column` of: header / `Expanded` scroll body / action footer.
- **Header (`_buildHeader`):** `t.surfaceRaised` bg, bottom border. Drag handle (36×4, `t.border`, radius 2), `SizedBox(14)`, then `Row`: `Expanded` title = `widget.title ?? l10n.orderReceiptPreview` ("Receipt Preview"), `ui(17, w700, textPrimary)`; trailing `IconButton(Icons.close_rounded)` → `Navigator.pop`. Padding `(20,12,16,14)`.
- **Body:** `SingleChildScrollView` (padding `horizontal xl=24, vertical lg=16`), `Center` → `ConstrainedBox(maxWidth:420)` → `ThermalReceiptCard(order, branch, methods)`. **The paper is scrollable; this is the only place it renders.** Binds: `paymentMethodProvider.items`, `authProvider.branch`.
- **No loading state for the paper itself** (it's pure synchronous UI; logo loads async via CachedNetworkImage with its own placeholder). There IS a print-in-progress state (below).
- **Action footer (`_buildActionFooter`):** `t.surfaceRaised`, top border, padding `(20, lg, 20, safeArea.bottom + lg)`. Contents:
  - Optional **error banner** (`_buildErrorBanner`): `t.dangerBg`, `danger@0.3` border, radius `xs`, `error_outline_rounded` 16 + error text `ui(12, danger)`. Shown when `printState.error != null && !printing`.
  - If `printState.printing` → **printing banner** (`_buildPrintingBanner`): full-width 52-high `t.navyBg` pill, animated `PrinterPainter` (LoopingIcon 1500ms) + `s.orderPrintingReceipt` ("Printing receipt…") `ui(13, w600, navy)`.
  - Else → single **`AppButton`** (full width, height 52). Label logic: no printer → `s.noPrinterConfigured`; has printer + prior error → `s.commonRetryPrint`; else → `s.printReceipt`. Icon: `refresh_rounded` if error else `print_rounded`. `onTap` enabled only when `hasPrinter && branch != null`.
- **Only actions: Print / Retry Print / Close.** No Share, no Export, no separate Reprint button (reprint = the Print button on an already-printed order).
- **Print logic (`_print`):** guards `branch.hasPrinter` (else warning snack `commonNoPrinterForBranch`). Sets `_previewPrintProvider(order.id)` → started. For **delivery orders missing `.delivery`**, re-fetches full order (`orderRepository.getOrder`) so address/phone print (falls back silently if offline). Calls `PrinterService.print(ip, port:printerPort??9100, brand, order, paymentMethods, branchName, logoUrl: branch.orgLogoUrl)`. On success → success snack `orderReceiptPrintedOk`. State is `autoDispose.family` keyed by order id (resets on close).

### 2B. `ReceiptSheet` (post-checkout success — `receipt_sheet.dart`)
- **Entry:** after checkout finalize (`checkout_sheet.dart:718`). `static show(ctx, {order, total, changeGiven})`. **Auto-prints once on first frame** (`initState` → postFrameCallback → `_print()`).
- **This is NOT a paper preview** — it's a success confirmation card, not `ThermalReceiptCard`. Single `t.surfaceRaised` sheet, scrollable, padding `(xl,14,xl, safeArea+xl)`.
- Contents top→bottom: drag handle; `SizedBox(lg)`; **`SuccessCheckIcon`** (88, `t.success`); `s.orderPlaced` ("Order Placed!") `ui(22, w800)`; then either `StatusChip(orderQueuedSyncs, warning, cloud_upload)` if `pending_sync`/no order#, else `s.orderNumber(n)` + optional `orderRef`.
- **`SurfaceCard` summary:** `_ReceiptRow`s — Payment method; Tip (if `tipAmount>0`): `'${egp(tip)} · ${methodLabel(tipPaymentMethod)}'`, green, money; Customer (if present); **Total** (emphasized, money); Time (`timeShort` = `hh:mm a`); Change Given (if `changeGiven>0`, green, money).
- **`_PrintStatus`** block (four states): no printer → `StatusChip(noPrinterConfigured, neutral, print_disabled)`; printing → navy pill w/ `PrinterPainter` + `orderPrintingReceipt`; error → `dangerBg` box with `orderReceiptFailed` ("Receipt didn't print") + error text + danger `AppButton(commonRetryPrint)`; printed → `successBg` row `check_circle` + `orderReceiptPrinted` ("Receipt printed") + trailing **`StatusChip(orderReprint, success, print_rounded)`** (this is the reprint affordance). Defensive fallback → `StatusChip(printReceipt, info)`.
- Bottom: full-width `AppButton(orderNewOrder, add_rounded)` → `Navigator.pop`.
- **Print logic differs:** passes `kickDrawer: isCashMethod(...)` (cash → opens drawer) and does **not** pass `logoUrl` here (so this auto-print uses the asset fallback logo, no org logo). State `autoDispose.family` keyed by order id, with extra `attempted` flag to distinguish "printed fine" from "never attempted."

### 2C. `ShiftReportPreviewSheet` (Z-report — `shift_report_preview_sheet.dart`)
- `ResponsiveSheet`, `maxHeight 92%`, `t.bg`. Header (`t.surface`): handle + `Row`(title `s.shiftReportTitle` "Shift Report" `ui(18,w700)` + teller name `ui(13,secondary)` ; trailing `StatusChip` Open/Closed). Body is **NOT a thermal card** — it's three `SurfaceCard`s (Shift details, Payment breakdown with per-method colored dots + `LinearProgressIndicator` bars, Cash movements list) + "Report generated {time}" footer. Footer print button identical pattern to the preview sheet (`commonPrintReport`/`commonRetryPrint`/`noPrinterConfigured`, loading flag). Print → `PrinterService.printShiftReport(..., logoUrl: branch.orgLogoUrl)` → success snack `shiftReportPrinted`. The printed PDF (`_buildShiftReportPdf`) is the thermal version and looks nothing like this rich on-screen card.

---

## 3. PRINT FLOW — `PrinterService` (`printer_service.dart`)

**Responsibilities:** build the PDF (`_buildReceiptPdf` / `_buildDeliveryReceiptPdf` / `_buildShiftReportPdf`), resolve & download the org logo (`_downloadImage`, 3-tier cache→file→raw-http fallback to asset), then transport over TCP per brand via `_send`.

**Three public entry points:** `print` (order), `printDeliveryReceipt` (delivery, from frozen cart), `printShiftReport`. All take `ip, port, brand, branchName, logoUrl?`; `print` also `kickDrawer`.

**Transport (`_send` L79):** strips anything after `/` from IP; switches on `PrinterBrand`:
- **Star** (`_printStar`): if `kickDrawer`, opens raw socket :9100 and sends `0x07` (BEL) first (failure ignored). Then `StarXpand.connect` → `printPdf(pdfBytes, width:576)` → disconnect. Returns error string or null.
- **Epson** (`_printEpson`): rasterises PDF at **203 dpi** (`Printing.raster`) → PNG → `_pngToEscPos` (1-bit threshold, `ESC @` init, `GS v 0` raster bitmap, feed+cut). Socket to `ip:port`, `tcpNoDelay`; if `kickDrawer` sends `ESC p 0 0x19 0xFA`. 5s timeouts.
- **unknown brand** → `'Unsupported printer brand'` (gated upstream by `Branch.hasPrinter`).
- Return convention: **`null` = success, non-null String = error message** (surfaced in the sheets' error banners).

**Same widget rendered to printer vs preview?** **No.** Preview = `ThermalReceiptCard` Flutter widget. Print = `pw`-package PDF. Independent code paths, intentionally hand-kept-in-sync, currently divergent (size labels, dividers solid-vs-dotted, "Tax" vs "Tax (14%)", thank-you literal-vs-arb, delivery fee, logo presence in auto-print).

---

## 4. i18n KEYS + LABELS (en) + hardcoded constants

**Receipt-card keys (`app_en.arb`):**
| key | label |
|---|---|
| `receiptOrderLabel` | `Order #` |
| `receiptDraft` | `DRAFT` |
| `receiptDate` | `Date` |
| `receiptItems` | `ITEMS` |
| `receiptNoItems` | `No items in cart` |
| `receiptVoidedStamp` | `*** VOIDED ***` |
| `receiptThankYou` | `Thank you for visiting!` |
| `commonTeller` | `Teller` |
| `orderReceiptCustomer` | `Customer` |
| `orderPaymentMethod` | `Payment` |
| `orderSubtotal` | `Subtotal` |
| `orderDiscount` | `Discount` |
| `orderTax14` | `Tax (14%)` |
| `orderTotal` | `Total` |

**Sheet chrome keys:**
| key | label |
|---|---|
| `orderReceiptPreview` | `Receipt Preview` |
| `printReceipt` | `Print Receipt` |
| `noPrinterConfigured` | `No Printer Configured` |
| `commonRetryPrint` | `Retry Print` |
| `commonNoPrinterForBranch` | `No printer configured for this branch` |
| `orderReceiptPrintedOk` | `Receipt printed successfully` |
| `orderPrintingReceipt` | `Printing receipt…` |
| `orderPlaced` | `Order Placed!` |
| `orderQueuedSyncs` | `Queued — syncs automatically` |
| `orderNumber` (param `{number}`) | `Order #{number}` |
| `orderReceiptTip` | `Tip` |
| `orderReceiptTime` | `Time` |
| `orderChangeGiven` | `Change Given` |
| `orderNewOrder` | `New Order` |
| `orderReceiptFailed` | `Receipt didn't print` |
| `orderReceiptPrinted` | `Receipt printed` |
| `orderReprint` | `Reprint` |

**Shift-report keys:** `shiftReportTitle`="Shift Report", `shiftReportOpenChip`="Open shift", `shiftStatusClosed`="Closed", `shiftReportDetails`="SHIFT DETAILS", `shiftColOpened`="Opened", `shiftColClosed`="Closed", `shiftOpeningCash`="Opening Cash", `shiftExpectedCash`="Expected Cash", `shiftDeclaredCash`="Declared cash", `shiftPaymentBreakdown`="PAYMENT BREAKDOWN", `shiftNoPayments`="No payments recorded", `commonOrdersCount`=`{count, plural, =1{1 order} other{{count} orders}}`, `shiftTotalPayments`="Total Payments", `shiftVoidedOrders`="Voided Orders", `shiftNetPayments`="Net Payments", `shiftCashMovementsHeader`="CASH MOVEMENTS", `shiftNoCashMovements`="No cash movements", `shiftPayIn`="Pay In", `shiftPayOut`="Pay Out", `shiftReportGenerated`=`Report generated {time}`, `shiftDrawerMatches`="Drawer matches", `shiftDrawerOver`=`Drawer is over by {amount}`, `shiftDrawerShort`=`Drawer is short by {amount}`, `commonPrintReport`="Print Report", `shiftReportPrinted`="Report printed".

**HARDCODED string literals (NOT in arb — must be ported as literals):**
- On-screen card: `'Ref'` label, `'Sufrix POS'` branch fallback, `'#${orderNumber}'`, ` · ` size separator, glyphs `+ • – ×`.
- PDF: ALL strings are hardcoded English literals — `'Order #...'`, `'Ref: ...'`, `'*** VOIDED ***'`, `'*** DELIVERY — {ch} ***'`/`'*** DELIVERY ***'`, `In-Mall`/`Outside`, `Customer`/`Phone`/`Address:`/`Unit`/`Floor`/`Zone`/`Delivery Ref`/`Payment (hint)`/`Notes:`, `Subtotal`/`Discount`/`Tax`/`Delivery Fee`/`TOTAL`/`Payment`/`Teller`, `'Thank you for visiting!'`, `'Thank you for your order!'`, `'Till Close Report'`, `'Business Date:'`, `'Printed at'`/`'Closed at'`, `PAYMENTS`/`DRAWER OPERATIONS`/`CASH RECONCILIATION`, `'Pay In'`/`'Pay Out'`, `'Opening Cash'`/`'Expected in Drawer'`/`'Actual in Drawer'`, `'Short by'`/`'Over by'`/`'Difference'`, `'— End of Report —'`, `'— Interim Report (Shift Still Open) —'`, `'(Shift not yet closed)'`. **The printed output is English-only; only the on-screen previews are localized.**

**Hardcoded layout constants:**
- Sheet width cap **600** (`ResponsiveSheet`); sheet maxHeight **92%** of screen.
- On-screen paper `ConstrainedBox maxWidth **420**`; logo **72×72**; info-row label column width **80**; handle **36×4**.
- Dotted divider: dash **4** / gap **4**, height **1**.
- PDF page width **72mm**, margins **2mm** (order/delivery) / **3mm** (shift report); logo width **56**; printer raster width **576** dots; Epson **203 dpi**; socket timeout **5s**, default port **9100**.
- PDF dividers: solid `thickness 0.4 grey600 height 6` / thin `0.2 grey400 height 4`. PDF font sizes 7–10pt.
- Date formats: on-screen `'MMM d, hh:mm a'` (items meta) & `'hh:mm a'` (success time); PDF `'dd/MM/yyyy  hh:mm a'`. All rendered in **branch timezone** via `AppTz.local`, not device zone.
- Money format (`egp`): `'EGP {value}'` where value = piastres/100, integer if whole else 2dp. Amounts use **tabular figures**.
- Fonts: family **Cairo** everywhere; weights — `ui` default w500, `money` default w700; receipt headers w700/w800. PDF uses Cairo-Regular + Cairo-SemiBold TTFs.
- Assets: org logo = network `Branch.orgLogoUrl`; fallback logo = bundled `assets/IconForeground.png`; PDF fonts = `assets/fonts/Cairo-Regular.ttf` + `assets/fonts/Cairo-SemiBold.ttf`.

**Color tokens used (light-ink, theme-invariant on the card):** paper `white`; ink `_ink.textPrimary/textSecondary/textMuted/border`; accents `_ink.navy` (qty, total value, bundle addons), `_ink.success` (discount green), `_ink.warning` (bundle optionals amber), `_ink.danger`+`_ink.dangerBg` (voided). PDF accents: `PdfColors.red700` (discount/short), `grey600/grey700/grey400` (dividers/labels), `green700` (pay-in/match), `orange700` (over).

**Relevant file paths (all absolute):**
- `/Users/shawket/Desktop/sufrix_pos/lib/features/order/widgets/receipt_preview_sheet.dart` — preview sheet + `ThermalReceiptCard` + `ReceiptItemRow` + dotted divider.
- `/Users/shawket/Desktop/sufrix_pos/lib/features/order/widgets/receipt_sheet.dart` — post-checkout success/auto-print sheet.
- `/Users/shawket/Desktop/sufrix_pos/lib/core/services/printer_service.dart` — PDF builders + Star/Epson transport + logo download.
- `/Users/shawket/Desktop/sufrix_pos/lib/features/shift/shift_report_preview_sheet.dart` — Z-report preview sheet.
- `/Users/shawket/Desktop/sufrix_pos/lib/shared/widgets/responsive_sheet.dart` — 600-wide bottom-sheet wrapper.
- `/Users/shawket/Desktop/sufrix_pos/lib/core/utils/formatting.dart` — `egp`, `dateTime`, `timeShort`, `normaliseName`.
- `/Users/shawket/Desktop/sufrix_pos/lib/core/utils/app_tz.dart` — branch-timezone clock.
- `/Users/shawket/Desktop/sufrix_pos/lib/core/theme/app_theme.dart` — `ui()`/`money()`, `AppSpace`, `AppRadius`, `sheetRadius`, Cairo family.
- `/Users/shawket/Desktop/sufrix_pos/lib/features/order/helpers/payment_helpers.dart` — `methodLabel`, `isCashMethod`.
- `/Users/shawket/Desktop/sufrix_pos/packages/sufrix_api/lib/src/model/branch.dart` — wire `Branch` (`orgLogoUrl`, `name`, `timezone`, `printer*`; `address`/`phone` exist but unused on receipts; no tax-id/org-name field).
- `/Users/shawket/Desktop/sufrix_pos/lib/l10n/app_en.arb` — all i18n keys above.
- Entry points: `order_history_screen.dart:1355`, `checkout_sheet.dart:350` & `:718`, `shift_history_screen.dart:727`.