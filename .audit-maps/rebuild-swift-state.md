I have everything needed. Here is the inventory.

---

# SwiftUI rebuild — Order History / Past Shifts / Receipts inventory

Tokens/components in use across all three screens: `SufrixCard`-equivalent inline cards (surface + `Radii.md`/`Radii.lg` rounded rect + 1pt `theme.colors.border` stroke), `Space.*` spacing scale, `Radii.*`, `StatusChip`, `SufrixButton`, `SufrixTextField`, `AmountField`, `NoticeBanner`, `ScreenHeader` (private), `SkeletonList`, `Money.format`, `.font(.ui())` / `.font(.money())`, `Haptics`, `Motion.standard`, `.sufrixSheet(item:)`. Note: there is no actual shared `SufrixCard` view — every card is hand-rolled `.background(surface).clipShape(RoundedRectangle…).overlay(strokeBorder…)`.

---

## 1. Order History — `OrderHistoryView.swift`

### Current presentation
**Cards only.** A vertical `ScrollView` → `VStack(spacing: Space.md)` of per-order rounded cards (`Radii.md`, surface fill, border stroke), capped at `maxWidth: 560`. Each card is collapsed by default and **expands in place on tap** to reveal line items + totals + actions. There is **no table/`DataTable` layout at any width** — it is single-column cards regardless of screen size.

### Fields shown
- Collapsed row: order number (`#N`, navy), time (`HH:MM` via `timeOf`), total (`.money`), and a `StatusChip` only for failed/queued/voided (completed orders show no chip).
- Expanded: per-line rows (`qty× name`, modifiers = sizeLabel · addons · optionals joined with " · ", line total) from `OrderDetailView.lines`; then `Subtotal`, `Tax`, payment label; and a `Print` (printer) + `Void` (trash) action.
- Filter bar (when history non-empty): a `SufrixTextField` search (matches order number + payment label) and a horizontal capsule chip row: All / Completed / Queued / Voided.
- Empty/loading states: `SkeletonList` while loading, `tray` icon empty state.

### Bound DTO (`OrderSummaryView`, `OrderDetailView`/`OrderDetailLineView` in `orders.rs`)
`OrderSummaryView`: `id, order_number?, subtotal_minor, tax_minor, total_minor, payment_label, status, created_at, queued`. `OrderDetailView`: adds `discount_minor` + `lines[]`; each line: `name, qty, size_label?, line_total_minor, addons[], optionals[]`. **Note:** `OrderSummaryView` has no `teller_name` field and no discount in the summary; `discount_minor` exists on the detail DTO but the expanded view does **not** render a Discount row (only Subtotal + Tax).

### Present vs. absent vs. Flutter (`lib/features/order/order_history_screen.dart`)
Flutter is a responsive **`SurfaceCard` DataTable** above `_kTableBreakpoint = 680` and `_CardList` below it, with:
- **Sortable columns** `_Col { number, payment, time, teller, amount }` via a `_TableHeader` with sort direction toggling. → **ABSENT in Swift** (no table, no sort, no teller column).
- **A `_StatsHeader`** (net payments / order count, per-method breakdown, pulling from the live `ShiftReport`). → **ABSENT in Swift.**
- **Two independent filter axes**: a sync/origin `_SyncFilter` (All/…) *and* a type filter (All / Dine-in / Delivery). Swift has only the single status chip row (All/Completed/Queued/Voided) — **the Dine-in/Delivery type axis is absent.**
- **Paged list** (`_kOrderPageSize`, `visibleLimit`, load-more). → **ABSENT in Swift** (renders the full `app.history` at once).

Present in both: search, queued/voided/failed status chips, tap-to-expand line detail, per-order Print + Void.

### To reach Flutter spec
Add a width-driven table (`GeometryReader`, break at 680) with a column header (Number / Payment / Time / Teller / Amount) and sortable columns; add a stats header card (net + count, fed by `app.shiftReport`); add the Dine-in/Delivery type filter; add pagination/load-more. Requires a **DTO change**: `OrderSummaryView` needs `teller_name` (and ideally `order_type`/`is_delivery`) for the Teller column + type filter.

---

## 2. Past Shifts — `ShiftHistoryView` in `CashAndShiftsView.swift`

### Current presentation
**This screen is the closest to Flutter — it already does the responsive table.** `GeometryReader` with `wide = width >= 680`:
- **Wide:** a single `Radii.lg` card containing a `columnHeader` (42pt, `surfaceAlt` fill, uppercase) + table rows separated by hairlines.
- **Narrow:** per-shift `Radii.md` cards.
Capped `maxWidth: 880`. Each row **expands in place** to show that shift's orders + a reprint action. Mirrors Flutter's `_TableHeader`/`_Cols` and `compact = maxWidth < 680`.

### Columns / fields shown (`ShiftSummaryView` in `shift.rs`)
- Wide table columns: **Opened-at** (`shortDate` → `YYYY-MM-DD HH:MM`), **Teller/status** (a `StatusChip` Open/Closed — labelled `shift.teller` in the header but renders status, see gap below), **Opening cash**, **Declared (closing)**, **Discrepancy** (signed, red if non-zero), chevron.
- Narrow card: opened-at, status chip, Opening / Declared / Discrepancy metrics.
- Expansion: an uppercase "Orders" label + per-order rows (`#N`, time, voided chip, payment label, total) from `app.shiftOrders[shift.id]`, plus a **Print report** (reprint Z-report) action with spinner.
- Bound fields: `id, branch_name?, opened_at, closed_at?, opening_cash_minor, closing_declared_minor?, closing_system_minor?, discrepancy_minor?, status, is_open`. `tellerName` is **not on `ShiftSummaryView`** (only on `ShiftView`).

### Present vs. absent vs. Flutter (`lib/features/shift/shift_history_screen.dart`)
Flutter columns are `status-dot · Teller · Opened · Closed · Declared`. Differences:
- **Teller name column: ABSENT in Swift.** The Swift header labels its second column `shift.teller` but the cell renders a **status chip**, not the teller's name — because `ShiftSummaryView` carries no `teller_name`. Flutter shows the actual teller name + a status dot.
- **Closed-at column: ABSENT in Swift** as a discrete column. Flutter has a dedicated `closedF` column; Swift only surfaces `closed_at` implicitly (narrow card shows `opened → closed` in Flutter; Swift doesn't render `closingDeclaredMinor`/`closed_at` pairing the same way).
- Swift adds an **Opening-cash column** Flutter folds differently; column set is close but not identical.
- Flutter pins a **locally-open (offline) shift** to the top of the first page. Swift relies on `app.shiftHistory` order — **no explicit local-open pinning** visible here.
- Present in both: responsive table/cards at 680, expand-to-orders, reprint Z-report, status chip, discrepancy coloring.

### To reach Flutter spec
**DTO change required:** add `teller_name` to `ShiftSummaryView` (it only lives on `ShiftView` today), then render it as a real column (with a status dot rather than a full chip), add a discrete **Closed** column, and pin a locally-open shift to the top. Otherwise structurally complete.

---

## 3. Receipts — `Components/ReceiptPaper.swift`, `ShiftReportPreview.swift`, `TenderView.swift`

### a) Order receipt — `ReceiptPaper`
**Already a faithful thermal-paper render**, theme-invariant (white paper / dark ink / monospaced), `maxWidth: 360`, used in three places: post-checkout (`ReceiptConfirmation` in `TenderView`), reprint preview (`ReceiptPreviewSheet`), and order-history Print. Mirrors `receipt.rs` `layout()`. Renders, from `ReceiptView`:
- Header: optional `*** VOIDED ***`, store name (uppercase), delivery channel line (`IN-MALL` / `DELIVERY`).
- Meta: order title (`Order #N` or local-id segment) + `dateTime` (`dd/MM/yyyy hh:mm a`), optional `Ref:`.
- Delivery block: customer, phone, address, zone.
- Item lines: `qty× name (size)` + line total; **bundle component breakdown** (`– component`, indented `+ addon/optional`) and per-line addons/optionals with prices.
- Totals: Subtotal, Discount, Tax, Delivery fee, **TOTAL** (bold), Tip, and for cash: Cash tendered + Change.
- Footer: payment label (uppercase), `Served by {teller}`, "Thank you!".

This already binds the full `ReceiptView` DTO (the same fields `receipt.rs` `layout()` emits — store header, meta, delivery, lines w/ bundle components + modifiers, totals, footer). **Receipt parity is essentially done** for the on-screen preview; the strings are hard-coded English here (`"Subtotal"`, `"TOTAL"`, `"Served by"`, `"Thank you!"`) whereas the core has a localizable `ReceiptLabels` struct — so the **preview is not localized** even though the printed ESC/POS path is.

### b) Post-checkout receipt — `ReceiptConfirmation` (`TenderView.swift`)
Flips the tender sheet to a success state: status icon (check / clock for queued-offline), "Order placed", a queued/sent `StatusChip`, the `ReceiptPaper` preview, then a `printControl` (printing / printed / no-printer states via `app.printState`) + a "New order" outline button. Complete and matches the Flutter checkout→receipt flow.

### c) Shift / Z-report — `ShiftReportPreview.swift`
`ShiftReportBreakdown` (embeddable) + `ShiftReportPreviewView` (mid-shift sheet). Renders `ShiftReportView`:
- Per-method payment lines with icon (cash/card), method name, **order count**, total, and a **proportional bar** (cash=success, else accent) — mirrors Flutter `ShiftReportPreviewSheet`.
- Drawer movements: cash-in/out totals + per-movement lines (note / mover, signed amount) from `cash_movements[]`.
- Voided amount, total payments, **Expected cash** (emphasized).
- Header: title + teller name + online/offline (`from_server`) chip; footer print control with `app.printState`.
- Bound DTO fields used: `expected_cash_minor, opening_cash_minor, total_payments_minor, voided_amount_minor, cash_in_minor, cash_out_minor, payment_lines[]{method,is_cash,order_count,total_minor}, cash_movements[]{amount_minor,note,moved_by_name}, from_server`. **Not surfaced:** `net_payments_minor`, `cash_movements_net_minor` (computed but not displayed), and `opening_cash_minor` is bound but not shown as its own row.

### Present vs. absent vs. Flutter
- **Order receipt + post-checkout + Z-report preview: present and close to 1:1** (the rebuild explicitly set out to mirror `ReceiptPreviewSheet` / `ShiftReportPreviewSheet`).
- **Absent / gaps:** (1) `ReceiptPaper` strings are hard-coded English, not driven by the core's `ReceiptLabels` (Arabic/localization gap in the *preview*); (2) Z-report preview omits an explicit **Opening cash** row and the net figures; (3) no QR/barcode or logo image on the paper (if Flutter renders one — `ReceiptLabels` has no image fields, so likely parity-OK).

### To reach Flutter spec
Thread the core's `ReceiptLabels` / `ShiftReportLabels` (already defined in `receipt.rs`) into `ReceiptPaper` and `ShiftReportBreakdown` instead of hard-coded English so the preview localizes like the printed output; add an Opening-cash row to the Z-report breakdown. Otherwise the receipt surface is the most complete of the three.

---

## Summary of distance from Flutter

| Screen | State today | Distance |
|---|---|---|
| **Order History** | Cards-only, status filter + search, tap-to-expand, print/void | **Furthest.** Missing the responsive DataTable + sortable columns, stats header, Dine-in/Delivery type filter, pagination, Teller column. Needs `teller_name`/`order_type` added to `OrderSummaryView`. |
| **Past Shifts** | Responsive table(680)/cards, expand-to-orders, reprint Z-report | **Closest structurally.** Missing real Teller-name column (renders status chip in its place) + a Closed column + local-open pinning. Needs `teller_name` on `ShiftSummaryView`. |
| **Receipts** | Faithful thermal `ReceiptPaper`, post-checkout confirmation, Z-report breakdown w/ bars | **Most complete.** Main gap: preview strings hard-coded English (not using core `ReceiptLabels`/`ShiftReportLabels`); minor missing Z-report opening-cash row. |

**Key files:** `/Users/shawket/Desktop/sufrix-rebuild/swift-app/Sources/SufrixUI/OrderHistoryView.swift`, `/Users/shawket/Desktop/sufrix-rebuild/swift-app/Sources/SufrixUI/CashAndShiftsView.swift` (`ShiftHistoryView` + `ShiftRow`), `/Users/shawket/Desktop/sufrix-rebuild/swift-app/Sources/SufrixUI/ShiftReportPreview.swift`, `/Users/shawket/Desktop/sufrix-rebuild/swift-app/Sources/SufrixUI/Components/ReceiptPaper.swift`, `/Users/shawket/Desktop/sufrix-rebuild/swift-app/Sources/SufrixUI/TenderView.swift` (`ReceiptConfirmation`). **Core DTOs:** `OrderSummaryView`/`OrderDetailView` in `orders.rs`, `ShiftSummaryView`/`ShiftReportView`/`CashMovementView` in `shift.rs`, `ReceiptView` + `ReceiptLabels`/`ShiftReportLabels` in `receipt.rs`. (No `teller_name` on `OrderSummaryView` or `ShiftSummaryView` — the binding blocker for both table parity gaps.)