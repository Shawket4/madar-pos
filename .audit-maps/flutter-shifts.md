I have all the information needed. Here is the exhaustive map.

# Flutter POS — Shift History (Past Shifts) Screen Map

## 1. Layout: Table vs List (responsive, width-driven)

The screen is **width-adaptive**, decided by `LayoutBuilder` on `constraints.maxWidth`:

| Width | Layout | Notes |
|---|---|---|
| `maxWidth >= 680` | **Data TABLE** | Sticky column header (`_TableHeader`) + a `Divider`, then rows. |
| `maxWidth < 680` (`compact`) | **Stacked LIST** (2-line rows) | No table header; teller + opened→closed range collapse into a single cell. |

- Container: a `SurfaceCard` (radius `AppRadius.lg`, clipped antiAlias, zero padding) wrapping `Column[ header?, Divider, Expanded(ListView.builder) ]`, padded `AppSpace.lg` all around.
- Rows alternate background (zebra): even rows `transparent`, odd rows `surfaceAlt`. Expanded row uses `hoverOverlay`.
- The list is **infinite-scroll paginated** (see Features → Pagination). Each row is **expandable** (not a navigation tap) — tapping a row expands an inline panel of that shift's orders.

## 2. Columns per shift row (TABLE mode, `maxWidth >= 680`)

Row height 56px. Column source of truth is class `_Cols`. Left→right:

| # | Column | Header label (i18n key) | Width / flex | Cell content | Formatting |
|---|---|---|---|---|---|
| 1 | Status dot | `''` (blank header) | fixed `26` (`statusW = 10+16`) | 8×8 colored circle | color by status (see §5) |
| 2 | Teller | `commonTeller` | `flex: 2` | `shift.tellerName` | `ui(14, w600, textPrimary)`, ellipsis |
| 3 | Opened | `shiftColOpened` | `flex: 2` | `dateTime(shift.openedAt)` | `ui(13, textSecondary)` |
| 4 | Closed | `shiftColClosed` | `flex: 2` | `dateTime(shift.closedAt)` or `'—'` if null | `ui(13, textSecondary/textMuted)` |
| 5 | Declared cash | `shiftDeclaredCash` (end-aligned) | fixed `110` (`declaredW`) | `egp(shift.closingCashDeclared)` or `'—'` | `money(14, w600)`, right-aligned; muted when null |
| 6 | Chevron | (none) | fixed `44` (`chevW`) | rotating `keyboard_arrow_down_rounded` (turns 0→0.5 when expanded) | — |

Notable: **there is no Cash-sales / Total-sales / Variance column in the table.** Those figures live only inside the expandable panel chips and the full Shift Report sheet. Opening float (`openingCash`) is NOT a table column — it appears as a chip inside the expanded panel.

### Columns in COMPACT/LIST mode (`maxWidth < 680`)
Row height 64px. Layout: `[ statusDot ][ Expanded 2-line cell ][ declaredCash ][ chevron ]`
- Line 1: `shift.tellerName` (14, w600, textPrimary, ellipsis)
- Line 2: if closed → `"${dateTime(openedAt)} → ${dateTime(closedAt)}"`; if open → `shiftOpenedOn(dateTime(openedAt))` (key `shiftOpenedOn`). (11, textSecondary, ellipsis)
- Trailing: declared cash (same `egp`/`'—'` rule) + chevron.

## 3. Features (exhaustive)

### Top bar (`AppTopBar`)
- Title: `shiftHistoryTitle`; Subtitle: `shiftHistorySubtitle`.
- One action: **`RefreshButton`** — `onTap: _load`, spinner bound to `history.loading`.

### Loading / Empty / Error states
- **First-load spinner**: centered `CircularProgressIndicator` when `loading && shifts.isEmpty`.
- **Error**: `ErrorState(message, onRetry: _load)` — only shown when there are no rows (`shifts.isEmpty`); errors during refresh with existing data are swallowed (error set to null).
- **Empty**: `EmptyState(icon: schedule_rounded, title: shiftNoShiftsYet, body: shiftShiftsAppearHere)`.
- **No branch assigned**: controller `noBranch(l10n.shiftErrorNoBranchAssigned)` sets error.

### Refresh / Loading model
- `_load()` reads `authProvider.user.branchId`; calls `_HistoryController.load(branchId)`.
- **Two-phase load**: instant paint from `repo.loadShiftsLocal(branchId)` (offline-safe), then `repo.fetchShiftsPage(branchId, page: 1)`, then `repo.cacheShifts(...)`.
- **Local open shift surfacing**: `_withLocalOpenShift` prepends a locally-opened-but-unsynced shift (`repo.loadShiftLocal().openShift`) to the top of page 1 if not already present (so the live shift always shows).

### Pagination (infinite scroll)
- `ScrollController` listener: when `pixels >= maxScrollExtent - 320`, calls `loadMore`.
- `loadMore` fetches `page+1` via `fetchShiftsPage`, de-dupes by `shift.id`, appends; guarded against concurrent/over-fetch via `loading/loadingMore/hasMore`.
- **`_LoadMoreFooter`**: a centered small spinner (20×20, strokeWidth 2) appended as an extra list item while `hasMore || loadingMore`.
- State carries: `shifts, loading, loadingMore, hasMore, page, error`.

### Sorting & Filtering
- **No client-side sort controls and no filter UI.** Order is whatever the server page returns (newest-first by backend convention). No search box, no date filter, no teller/status filter.

### Status chips
- In the table row, status is only a colored **dot** (§5). The textual `StatusChip` appears in the **expanded panel** header (`_statusLabel`, tone per `_statusTone`).

### Row tap behavior
- **Tap = expand/collapse**, NOT navigation. `GestureDetector → _RowController.toggleOrders()`.
- It does **not** open the Z-report on row tap. It loads & shows the **orders placed during that shift** inline.

### Expanded panel (per shift)
Rendered when `row.expanded`. A `SurfaceCard` containing:
1. **Meta header row**:
   - `StatusChip(label: _statusLabel, tone: _statusTone)`.
   - If `openingCash > 0`: `StatusChip(label: shiftOpeningChip(egp(openingCash)), tone: neutral, icon: account_balance_wallet_outlined)`.
   - `Spacer()`.
   - **Print Report chip** (right): `StatusChip(tone: info, icon: print_rounded)` — label `commonPrintReport`; while loading shows `commonLoading` + spinning `sync_rounded`. `onTap → _printReport`.
2. `Divider`.
3. **Orders section** header: `shiftOrdersInShift` (10, w700, muted, letterSpacing 0.7).
4. Orders body, one of:
   - loading spinner (`row.loadingOrders`),
   - `row.ordersError` text (danger),
   - empty → `shiftNoOrdersInShift` (muted),
   - else a bordered list of `_PastOrderRow` separated by indented dividers.

### `_printReport` action (per row)
- `setPrinting(true)` → `shiftRepository.getReport(shift.id)` → `setPrinting(false)` → **opens `ShiftReportPreviewSheet.show(context, report)`**.
- On error: SnackBar `commonFailedLoadReport('$e')`, danger background.
- Despite the "Print Report" label/icon, the chip actually **opens the report preview sheet** (which itself has the real print button). So this is the "view Z-report" affordance.

### `_PastOrderRow` (orders inside the shift)
Per order: number bubble `#<orderNumber>` (or `#?`), time `timeShort(createdAt)`, optional `commonVoided`/`commonPendingSync` chips, optional `customerName`, payment-method label (resolved via `paymentMethodProvider`; falls back to `o.paymentMethod`), optional `orderRef`, amount `egp(totalAmount)` (line-through + muted if voided).
- **Per-order print button**: prints the receipt. Tapping → `ReceiptPreviewSheet.show(context, full)`; if `items` empty, fetches full order via `orderRepository.getOrder`. Spinner state via `_orderPrintingProvider(orderId)`; while printing shows an animated `PrinterPainter` (`LoopingIcon`) instead of the print icon.

### Header summary / totals
- **The history screen has NO header summary or aggregate totals row.** (Totals exist only on the Cash-Movements screen and inside the Shift Report sheet.)

## 4. Shift Report / Z-Report Preview (`ShiftReportPreviewSheet`)

### Presentation
- **A modal bottom sheet** via `ResponsiveSheet.show` (responsive: bottom sheet on phones, likely centered dialog on wide — not full screen). Max height `screenHeight * 0.92`. Drag handle (36×4) at top.
- Structure: fixed header → scrollable body (`SingleChildScrollView`) → pinned footer print button.

### Header
- Title `shiftReportTitle` (18, w700); subtitle `report.tellerName` (13).
- Trailing `StatusChip`: open → `shiftReportOpenChip` (tone success); closed → `shiftStatusClosed` (tone neutral).

### Body sections (3 `SurfaceCard`s + footer line)

**Section A — Shift details** (`_SectionTitle: shiftReportDetails`)
- `shiftColOpened` → `_fmtDt(openedAt)`
- `shiftColClosed` → `_fmtDt(closedAt)` (only if `closedAt != null`)
- `shiftOpeningCash` → `egp(openingCash)` (money)
- `shiftExpectedCash` → `egp(expectedCash)` (money) — server-authoritative `report.expectedCash`
- If `closingCashDeclared != null`:
  - `shiftDeclaredCash` → `egp(closingCashDeclared)` (money, bold)
  - Divider
  - **`_DiscrepancyRow`** (variance): `diff = declared - expected`:
    - `diff == 0` → success, `check_circle_outline`, label `shiftDrawerMatches`
    - `diff > 0` → warning, `arrow_upward`, label `shiftDrawerOver(egp(diff))`
    - `diff < 0` → danger, `arrow_downward`, label `shiftDrawerShort(egp(diff.abs()))`

**Section B — Payment breakdown** (`_SectionTitle: shiftPaymentBreakdown`)
- Empty → `shiftNoPayments` (muted).
- Else, per `report.paymentSummary` row (`PaymentSummaryRow`): color dot (`methodColor`), method label (`methodLabel(methods, locale, paymentMethod)`), `commonOrdersCount(orderCount)` subtitle, `egp(total)` amount, and a `LinearProgressIndicator` of `total / totalPayments`.
- Divider, then:
  - `shiftTotalPayments` → `egp(totalPayments)` (money, bold, large)
  - If `totalReturns > 0`: `shiftVoidedOrders` → `− egp(totalReturns)` (danger) — `totalReturns` maps to wire `voidedAmount`
  - If `totalReturns > 0`: `shiftNetPayments` → `egp(netPayments)` (money, bold, large, success)

**Section C — Cash movements** (`_SectionTitle: shiftCashMovementsHeader`)
- Empty → `shiftNoCashMovements` (muted).
- Else, per `report.cashMovements` row (`CashMovementSummaryRow`): in/out icon (`add_rounded`/`remove_rounded` in success/danger bg), `m.note`, subtitle `"${movedByName} · ${_fmtDt(createdAt)}"`, amount `"$sign egp(amount.abs())"` (`+`/`−`, success/danger). `isIn = amount > 0`.
- Divider, then:
  - `shiftPayIn` → `egp(cashMovementsIn)` (success)
  - `shiftPayOut` → `egp(cashMovementsOut)` (danger)

**Footer line**: centered `shiftReportGenerated(_fmtDt(printedAt))`.

### Pinned print footer
- Optional error banner (dangerBg) when `printError != null`.
- `AppButton`: label = `commonPrintReport` (or `commonRetryPrint` after error) when printer present; `noPrinterConfigured` when none. Icon `print_rounded`/`refresh_rounded`. Loading bound to `printState.printing`.
- `_print`: requires `branch.hasPrinter`; else SnackBar `commonNoPrinterForBranch` (warning). Calls `PrinterService.printShiftReport(ip, port (default 9100), brand, report, paymentMethods, branchName, logoUrl)`. Success → SnackBar `shiftReportPrinted` (success).

## 5. Status values, chips & colors

Status strings: `'open'`, `'closed'`, `'force_closed'`, (fallback: raw with `_`→space).

| status | dot/text color (`_statusColor`) | `_statusTone` (chip) | label (`_statusLabel`) key |
|---|---|---|---|
| `open` | `success` | `ChipTone.success` | `shiftStatusOpen` |
| `closed` | `textMuted` | `ChipTone.neutral` | `shiftStatusClosed` |
| `force_closed` | `danger` | `ChipTone.danger` | `shiftStatusForceClosed` |
| other | `textMuted` | `ChipTone.neutral` | `status.replaceAll('_',' ')` |

Report-sheet status chip: open → `shiftReportOpenChip` (success); else → `shiftStatusClosed` (neutral).

## 6. Money & time formatting

**Money** (`core/utils/formatting.dart`):
- `egp(int)` — formats integer minor/major units as EGP currency (used everywhere for cash/amounts).
- `money(...)` — a `TextStyle` builder (tabular money font) used to render currency text.
- Amounts are stored/passed as **`int`** (`openingCash`, `closingCashDeclared`, `expectedCash`, `totalPayments`, `netPayments`, `voidedAmount`, movement `amount`, etc.).

**Time** (`core/utils/formatting.dart` + `core/utils/app_tz.dart`):
- `dateTime(DateTime)` — table/list date-time cells (opened/closed, movement timestamp).
- `timeShort(DateTime)` — short time for past-order rows.
- Report sheet uses its own `_fmtDt`: `DateFormat('dd/MM/yyyy  hh:mm a').format(AppTz.local(dt))` (12-hour, AM/PM, localized to branch tz via `AppTz.local`).

## 7. i18n keys & exact labels (consolidated)

**History screen**: `shiftHistoryTitle`, `shiftHistorySubtitle`, `shiftNoShiftsYet`, `shiftShiftsAppearHere`, `shiftErrorNoBranchAssigned`, `commonTeller`, `shiftColOpened`, `shiftColClosed`, `shiftDeclaredCash`, `shiftOpenedOn(<dt>)`, `shiftOpeningChip(<egp>)`, `commonLoading`, `commonPrintReport`, `shiftOrdersInShift`, `shiftNoOrdersInShift`, `commonFailedLoadReport(<err>)`, `shiftStatusOpen`, `shiftStatusClosed`, `shiftStatusForceClosed`.

**Past-order rows**: `commonVoided`, `commonPendingSync`.

**Report sheet**: `shiftReportTitle`, `shiftReportOpenChip`, `shiftStatusClosed`, `shiftReportDetails`, `shiftColOpened`, `shiftColClosed`, `shiftOpeningCash`, `shiftExpectedCash`, `shiftDeclaredCash`, `shiftDrawerMatches`, `shiftDrawerOver(<egp>)`, `shiftDrawerShort(<egp>)`, `shiftPaymentBreakdown`, `shiftNoPayments`, `commonOrdersCount(<n>)`, `shiftTotalPayments`, `shiftVoidedOrders`, `shiftNetPayments`, `shiftCashMovementsHeader`, `shiftNoCashMovements`, `shiftPayIn`, `shiftPayOut`, `shiftReportGenerated(<dt>)`, `commonRetryPrint`, `commonPrintReport`, `noPrinterConfigured`, `commonNoPrinterForBranch`, `shiftReportPrinted`.

**Cash movements screen** (related, not part of history): `cashMovementsTitle`, `cashMovementsSubtitle`, `cashMovementsNew`, `cashMovementsNeedShift`, `cashMovementsEmptyBody`, `cashMovementsNet`, `cashIn`, `cashOut`, `shiftNoCashMovements`.

## 8. Data model fields (for table reimplementation)

**`Shift`** (generated `Shift`, façade `core/models/shift.dart`; `isOpen => status=='open'`): `id`, `branchId`, `tellerId`, `tellerName`, `status`, `openingCash:int`, `openingCashWasEdited:bool`, `openingCashEditReason:String?`, `openedAt:DateTime`, `closedAt:DateTime?`, `closingCashDeclared:int?`, `closingCashSystem:int?`.

**`ShiftReport`** = `ShiftReportResponse` (façade `core/models/shift_report.dart`): embeds `shift`; plus `expectedCash:int`, `totalPayments:int`, `netPayments:int`, `voidedAmount:int` (`totalReturns`), `cashMovementsIn:int`, `cashMovementsOut:int`, `printedAt:DateTime`, `paymentSummary: List<PaymentSummaryRow>` (`paymentMethod`, `orderCount`, `total`), `cashMovements: List<CashMovementSummaryRow>` (`amount`, `note`, `movedByName`, `createdAt`; `isIn => amount>0`).

## 9. Key reimplementation notes (table-ization)
- The history list is **already a table** above 680px; below that it degrades to stacked rows — a pure table reimplementation can keep the 6 columns and add a horizontal scroll or hide/merge columns under the breakpoint.
- Missing-from-table-but-available data you may want as columns: **opening float** (`openingCash`), **status text** (currently dot-only), and report-derived **total sales / cash sales / variance** (require a per-row `getReport(shift.id)` fetch — not in the page payload, so they'd need lazy/expanded loading or a backend list enrichment).
- Row "action" today is a single expand affordance; explicit per-row actions worth surfacing as table buttons: **View/Print report** (`_printReport` → `ShiftReportPreviewSheet`) and per-order **print receipt** (inside the expansion).

Relevant files (absolute):
- `/Users/shawket/Desktop/sufrix_pos/lib/features/shift/shift_history_screen.dart`
- `/Users/shawket/Desktop/sufrix_pos/lib/features/shift/shift_report_preview_sheet.dart`
- `/Users/shawket/Desktop/sufrix_pos/lib/features/shift/cash_movements_screen.dart`
- `/Users/shawket/Desktop/sufrix_pos/lib/core/models/shift.dart`
- `/Users/shawket/Desktop/sufrix_pos/lib/core/models/shift_report.dart`
- `/Users/shawket/Desktop/sufrix_pos/lib/core/providers/shift_notifier.dart`
- Formatting helpers: `/Users/shawket/Desktop/sufrix_pos/lib/core/utils/formatting.dart`, `/Users/shawket/Desktop/sufrix_pos/lib/core/utils/app_tz.dart`