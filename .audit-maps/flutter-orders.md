I have everything I need. This is a read-only mapping task; returning the spec directly.

# Order History Screen — Rebuild Spec (Flutter → SwiftUI/Compose table)

Source: `/Users/shawket/Desktop/sufrix_pos/lib/features/order/order_history_screen.dart` (the whole screen lives in this one file), `order_history_notifier.dart`, `order.dart` (= generated `OrderFull`), `void_order_sheet.dart`, `receipt_preview_sheet.dart`, `helpers/payment_helpers.dart`, `utils/formatting.dart`, `shared/widgets/status_chip.dart`.

## 1. List or table?

**Both — responsive.** A single body width breakpoint `_kTableBreakpoint = 680` decides:
- width ≥ 680 → **sortable data table** (`_OrderTable`, header + rows inside a `SurfaceCard`).
- width < 680 → **stacked cards** (`_CardList`, `_OrderCard`).

For the rebuild table, mirror the **wide table** layout. Rows are tap-to-expand inline (NOT navigation): tapping a row expands an inline detail panel beneath it (line items + totals + meta + actions). Only one row expands at a time.

## 2. Columns (exact, in order)

The table is laid out as: `[# 104px] [Payment flex3] [Time flex2] [Teller flex2] [Amount 110px, right-aligned] [chevron 44px]`. Spec constants in `_ColSpec`.

| # | Header label | i18n key | Width/flex | Sortable (`_Col`) | Cell content |
|---|---|---|---|---|---|
| 1 | `#` (literal hash, not translated) | — | fixed **104px** | yes → `_Col.number` (sorts on `orderNumber`) | `#{orderNumber}` bold navy; second line = `orderRef` (tiny size-9 muted, ellipsis) if non-null. **If `status == 'pending_sync'`** the whole cell is replaced by a single `Icons.cloud_upload_outlined` (16px, warning) — no number shown. |
| 2 | `Payment` | `orderPaymentMethod` (AR: "طريقة الدفع") | flex **3** | yes → `_Col.payment` (sorts on `paymentMethod` string) | `_PaymentBadge` (colored pill, method label) + inline status chips: **Voided** chip (danger) if voided, **Pending sync** chip (warning, icon `sync_rounded`, taps → route `pending-orders`) if pending, then `customerName` (size-12 muted, ellipsis) if non-null. |
| 3 | `Time` | `orderColTime` (AR: "الوقت") | flex **2** | yes → `_Col.time` (sorts on `createdAt`) | `timeShort(createdAt)` = `hh:mm a` in branch TZ (size-13 secondary). |
| 4 | `Teller` | `commonTeller` (AR: "الكاشير") | flex **2** | yes → `_Col.teller` (sorts on `tellerName`) | `person_outline_rounded` (13px) + `tellerName` (size-12 secondary, ellipsis). |
| 5 | `Amount` | `orderColAmount` (AR: "المبلغ") | fixed **110px**, **right-aligned** | yes → `_Col.amount` (sorts on `totalAmount`) | `egp(totalAmount)` (size-14, w600). Voided → muted color + **line-through**. |
| 6 | (none) | — | fixed **44px** | no | Chevron `keyboard_arrow_down_rounded` (rotates 0.5 turn when expanded); shows a 14px `CircularProgressIndicator` while `loadingDetail`. |

There is **no** dedicated "items count", "status", or "type" column. Status surfaces as inline chips in the Payment column; order type/channel surfaces only in the narrow card (a "Delivery"/"In-mall"/"Outside" chip) and in the Delivery KPI strip — NOT in the wide table row.

**Row visuals:** height 56px; zebra striping — even rows transparent, odd rows `surfaceAlt`. Expanded row background = `navyBg`. Voided rows wrapped in `Opacity(0.55)`.

## 3. Header / above-table chrome (top → bottom)

1. **AppTopBar** — title `orderHistoryTitle` ("Orders" / "الطلبات"); subtitle `orderCurrentShift` ("Current shift" / "الوردية الحالية") when a shift is open. Actions: `SyncStatusChip`, then a **RefreshButton** (the refresh/pull control — there is no pull-to-refresh gesture; refresh is this button only). Button is disabled when no shift, shows loading spinner while `history.isLoading`, taps → `orderHistoryProvider.refresh(shift.id)`.
2. **OfflineBanner** (shared widget, shows when offline).
3. **`_StatsHeader`** — horizontal-scroll summary row: `Orders` stat (`orderStatOrders` count of non-voided), divider, `Total` stat (`orderTotal`, `egp`, success color), then one `StatusChip` per payment method showing `"{label} · {egp(amount)} · {pct}%"`. A `Mixed` (`orderMixed`) chip is appended if mixed-payment sum > 0. Uses shift-report `netPayments`/`paymentSummary` when available, else local fold over orders. Chip tone: cash → success, mixed → warning, else → info.
4. **`_DeliveryKpis`** — only rendered if the shift has ≥1 non-voided delivery order. Horizontal row inside a card: icon `local_shipping_outlined`, then stats `Delivery orders` (count), `Delivery revenue` (`egp`, success), `Delivery fees` (`egp` of `deliveryFee` sum), `Avg ticket` (`egp(revenue/count)`), then chips `"In-mall · {n} · {egp}"` (info) and `"Outside · {n} · {egp}"` (accent) when those channels have orders. **All these labels are hardcoded English literals** ("Delivery orders", "Delivery revenue", "Delivery fees", "Avg ticket", "In-mall", "Outside") — not i18n keys.
5. **`_TypeFilterRow`** — order-origin filter chips (see §4).
6. **`_FilterRow`** — sync-status filter chips (see §4).

## 4. Filters

Two **independent** filter axes (both combine; AND), each a horizontal-scroll row of `StatusChip`s. Each chip shows its own filtered count: `"{label} · {count}"`. Selected chip uses its active tone; unselected = `ChipTone.neutral`. Selecting any filter resets `visibleLimit` to 20.

**A. Type filter (`_TypeFilter`)** — labels are **hardcoded English literals** (no i18n):
| Value | Label | Icon | Active tone | Match rule |
|---|---|---|---|---|
| all | `All` | `tune_rounded` | accent | all |
| dineIn | `Dine-in` | `restaurant_rounded` | accent | `orderType != 'delivery'` |
| delivery | `Delivery` | `local_shipping_outlined` | accent | `orderType == 'delivery'` |

**B. Sync filter (`_SyncFilter`)** — i18n labels:
| Value | Label | i18n key | Icon | Active tone | Match rule |
|---|---|---|---|---|---|
| all | All | `orderFilterAll` | `list_alt_rounded` | accent | all |
| synced | Synced | `orderFilterSynced` | `cloud_done_rounded` | success | `status != 'voided' && status != 'pending_sync'` |
| pending | Pending sync | `commonPendingSync` | `cloud_upload_rounded` | warning | `status == 'pending_sync'` |
| voided | Voided | `commonVoided` | `cancel_outlined` | danger | `status == 'voided'` |

There is **no search box**, **no date filter**, and **no date-range picker** — scope is always the current shift only (one shift's orders, loaded by `shiftId`).

## 5. Sort

Tap any sortable header cell to sort. Tapping the active column toggles asc/desc; tapping a new column sets it active (default direction: ascending only for `#`/number, descending for all others). Active header is `accent`-colored with an up/down arrow (`arrow_upward_rounded` / `arrow_downward_rounded`, 11px). Default sort: `_Col.number`, **descending** (newest order number first). Changing sort resets `visibleLimit` to 20.

## 6. Pagination

**Client-side "show more", not infinite scroll.** The full shift is always in memory (so offline/unsynced orders and stats stay correct). Only the first `visibleLimit` rows paint (`_kOrderPageSize = 20`). If more remain, a footer button `_ShowMoreFooter` appears at the list end: icon `expand_more_rounded` + text `orderShowMore` → `"Show {count} more"` / `"عرض {count} إضافية"`. Tapping adds 20 to the limit. No scroll-triggered loading.

## 7. Loading / empty / error states

Decided in `OrderHistoryScreen.build`:
- **No shift open** → `EmptyState` icon `lock_outline_rounded`, title `shiftNoOpenShift` ("No open shift"), body `orderOpenShiftToSell` ("Open a shift to start selling — its orders will appear here.").
- **Loading + no orders yet** → centered `CircularProgressIndicator`.
- **Error** (`history.error != null`) → `ErrorState` with the message + retry → `_load`. Error message string (from notifier): `"Could not load orders — check connection"` (hardcoded, only set when list is empty).
- **Loaded, zero orders** → `EmptyState` icon `receipt_long_rounded`, title `orderNoOrdersYet` ("No orders yet"), body `orderOrdersAppearHere` ("Orders completed during this shift will appear here.").
- **Loaded, but filters exclude everything** → `EmptyState` icon `filter_alt_off_rounded`, title `orderNothingHere` ("Nothing here"), body `orderNoFilterMatch` → `"No orders match the \"{filter}\" filter."` (interpolates the active sync-filter label).

## 8. Status chips — statuses + colors

`ChipTone` → (bg, fg) from theme tokens (`status_chip.dart`):
- `neutral` → surfaceAlt / textSecondary
- `accent` → accentBg / accent (terracotta)
- `success` → successBg / success (green)
- `danger` → dangerBg / danger (red)
- `warning` → warningBg / warning (amber)
- `info` → navyBg / navy

Order statuses observed (from `o.status` string): `'voided'`, `'pending_sync'`, and "normal/synced" (anything else). Voided → **Voided** chip (danger) + row opacity 0.55 + line-through amount + muted colors. Pending → **Pending sync** chip (warning, tappable to `pending-orders` route) + the `#` cell becomes a cloud-upload icon.

**Payment badge** (`_PaymentBadge`, NOT a StatusChip): rounded pill, bg = method color @ 12% opacity, fg = method color (lightened in dark mode via HSL). Label from `methodLabel(methods, locale, paymentMethod)`. Voided → muted fg + surfaceAlt bg. Method color/label come from server `PaymentMethod` list; fallbacks in `payment_helpers.dart`: card → `#7C3AED` "Card"/"بطاقة"; cash/talabat_cash → `#22C55E` "Cash"/"نقدي"; `mixed` → mixed stub.

## 9. Row tap behavior & per-row actions

**Row/card tap → toggles inline expansion** (`toggleExpand`). Does NOT open a receipt or navigate. On first expand, if `items` is empty it fetches the full order (`orderRepository.getOrder(id)`, shows row spinner, caches result via `updateOrder`).

**Expanded detail panel** (`_OrderDetail`, shared by table + card):
- **Line items** (`_ItemRow`): qty bubble (28×28, navy) + item name (+ ` · {sizeLabel}` normalised) + a **Combo** chip (`orderCombo`) for bundle lines; bundle components listed as `– {name} · {size} × {qty}`; addon chips (`_TinyChip`, accent) `"{name}  +{egp}"`; optional chips (warning) `"{name}  +{egp}"`; an **Ingredients** chip (`orderIngredients`, icon `science_rounded`) that opens `OrderIngredientsSheet` when `deductions` non-empty or pending. Line total `egp(lineTotal)` on the right. If items empty after load → text `orderNoItemDetails` ("No item details available").
- **Totals block**: `Subtotal` (`orderSubtotal`), `Discount` (`orderDiscount`, shown as `"− {egp}"` in success) if `discountAmount > 0`, `Tax (14%)` (`orderTax14`) if `taxAmount > 0`, `Delivery fee` (hardcoded literal "Delivery fee") if `deliveryFee > 0`, then bold **Total** (`orderTotal`, `egp(totalAmount)`, size-18 w800; line-through if voided).
- **Meta row** (`_Meta`, icon+text): payment method (`payments_outlined` + method label), teller (`person_outline_rounded` + `tellerName`), and if voided with a reason — `cancel_outlined` + void-reason label (danger).
- **Action buttons** (`_RowAction`, right-aligned):
  - **Print** (`commonPrint` "Print", icon `print_rounded`, navy) → `ReceiptPreviewSheet.show(context, order)` — this is the **reprint/preview** path. The preview sheet renders the receipt; its primary button is "Print" / refresh (icon `refresh_rounded` when already-printed, `print_rounded` otherwise), gated on `branch.hasPrinter`; on reprint it enriches the order via `getOrder` first. Sheet has a close (`close_rounded`) button. (No "share" action.)
  - **Void** (`orderVoid` "Void", icon `cancel_outlined`, danger) → `VoidOrderSheet.show(...)`. **Only rendered when not voided.** No refund action exists anywhere.

There is **no** separate refund, no per-row overflow menu — exactly two actions (Print, Void) inside the expanded panel.

## 10. Void sheet (`VoidOrderSheet`) strings & flow

Title `orderVoidTitle` → "Void Order #{number}" (+ `orderRef` line if present). Subtitle `orderVoidCannotUndo` "This action cannot be undone". Offline warning `orderVoidOfflineQueued`. Reason header `orderVoidReasonHeader` "Reason". Reasons (radio tiles), wire value → label:
- `customer_request` → `orderVoidReasonCustomerRequest` "Customer request"
- `wrong_order` → `orderVoidReasonWrongOrder` "Wrong order"
- `quality_issue` → `orderVoidReasonQualityIssue` "Quality issue"
- `other` → `orderVoidReasonOther` "Other" (+ free-text field, hint `orderVoidDescribeHint` "Describe the reason…")

Restore toggle: title `orderVoidRestoreTitle` "Return items to inventory", body `orderVoidRestoreBody` "Ingredients go back into stock" (default ON). Validation errors: `orderVoidSelectReason` "Please select a reason", `orderVoidSpecifyReason" "Please specify the other reason". Buttons: `cancelAction` "Cancel"; primary = `orderVoidAction` "Void Order" (online) / `orderQueueVoid` "Queue Void" (offline). Confirm sheet: title `orderVoidConfirmTitle` "Void order #{number}?", body `orderVoidConfirmBodyOnline`/`...Offline`, action `orderVoidConfirmAction` "Void order". Void label decoder for meta: `other: {text}` strips the prefix; known reasons map to their labels.

## 11. Money / qty / time formatting (`utils/formatting.dart`)

- **Money** `egp(int piastres)`: value = piastres/100; whole numbers render with no decimals, else 2 decimals; prefix `"EGP "`. E.g. 1500 → `"EGP 15"`, 1550 → `"EGP 15.50"`. All amounts (`totalAmount`, `subtotal`, `taxAmount`, `discountAmount`, `deliveryFee`, line totals, addon/optional prices, `tipAmount`) are **integer piastres**. Money text uses a dedicated `money(...)` text style (tabular).
- **Discount** displayed as `"− {egp}"` (minus + space).
- **Qty**: integer, rendered raw (`{quantity}`); bundle component effective qty = `component.quantity * line.quantity`; addon line price = `priceModifier * quantity`.
- **Percent** (stats): `(amount / total * 100).round()` with `%` suffix.
- **Time** `timeShort` = `hh:mm a` (12-hour, AM/PM) rendered in the **branch-configured timezone** `AppTz.local(dt)`, never device local. (`dateShort` = `MMM d`, `dateTime` = `MMM d, hh:mm a` exist but are not used on this screen.)
- **Name normalisation** `normaliseName`: title-cases each space-separated word (used on size labels, addon/optional/component names).

## 12. Underlying data model (`OrderFull`, fields used)

`id`, `orderNumber` (int, per-shift), `orderRef` (String?, unique ref shown under #), `tellerName` (String), `paymentMethod` (String wire), `tipPaymentMethod` (String?), `tipAmount` (int?), `totalAmount`/`subtotal`/`taxAmount`/`discountAmount`/`deliveryFee` (int piastres), `status` (String: `voided`/`pending_sync`/other), `orderType` (String, `'delivery'` vs other), `deliveryChannel` (String? `'in_mall'`/`'outside'`), `customerName` (String?), `createdAt` (DateTime), `voidReason` (String?), `shiftId`, `items` (List<OrderItemFull>, may be empty in list payload → lazy-fetched on expand). `isVoided` = `status == 'voided'`.

## 13. Exact literal strings (non-i18n, hardcoded English — note for rebuild)

`#` (header), `All` / `Dine-in` / `Delivery` (type filter), `Delivery orders`, `Delivery revenue`, `Delivery fees`, `Avg ticket`, `In-mall`, `Outside`, `Delivery fee` (totals line), `"Could not load orders — check connection"` (error). `_channelLabel`: `in_mall`→"In-mall", `outside`→"Outside", else→"Delivery". These should likely be promoted to i18n keys in the rebuild but currently are not.