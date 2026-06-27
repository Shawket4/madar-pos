Confirmed: Kotlin `ToastData` has no `icon` field; Swift's renders a leading SF Symbol. BundleDetailSheet.kt is a faithful mirror (uses glyphs ✓/slider/› instead of SF Symbols, but logically identical). 

I now have a complete file-by-file picture. Compiling the final parity report.

---

# Kotlin/Compose vs SwiftUI — File-by-File Parity Gap

Swift is source of truth. Kotlin dir: `…/kotlin-app/composeApp/src/commonMain/kotlin/app/sufrix/`. Every Swift screen/component has a Kotlin counterpart that **exists** — there are **no fully-missing screens**. Gaps are missing *features/states/affordances* and a systematic icon-omission in the shared component library.

## P0 — Missing screen / broken-flow gaps

### 1. ReauthView → no Kotlin counterpart (entire re-auth flow absent)
- **Swift**: `ReauthView.swift` (full PIN-pad re-auth sheet) + `AppModel.showReauth`, `reauth(pin:)`, `reauthSwitchTeller()`. `OrderView` presents it via `.sufrixSheet(isPresented: $app.showReauth …)` and the `syncAuthPaused` banner is a **tappable button** (`actionLabel: t("chrome.auth_paused_action")`) that sets `app.showReauth = true`.
- **Kotlin**: **No `ReauthScreen`/`ReauthSheet` file. `AppModel.kt` has no `showReauth`, `reauth()`, or `reauthSwitchTeller()`.** In `OrderScreen.kt` the `syncAuthPaused` banner is a plain non-interactive `NoticeBanner` — a teller whose token expires mid-shift has **no in-app way to re-authenticate and resume syncing**. This is a functional dead-end.
- Action: port `ReauthView`, add `showReauth`/`reauth`/`reauthSwitchTeller` to Kotlin `AppModel`, make the auth-paused banner tappable, present the sheet.

## P1 — Missing feature / behavior gaps

### 2. AppModel — shift adoption on reconnect missing (Kotlin)
- **Swift** `refreshConnectivity()` captures `wasOnline`, and on `!wasOnline && isOnline` calls `await reconcileShift()` — re-adopts an active server shift when the network returns (prevents duplicate-shift / stranded-on-open-shift). This is task #41 ("completed") on the Swift side.
- **Kotlin** `refreshConnectivity()` does **not** capture `wasOnline` and never calls `reconcileShift()` on reconnect. The reconnect-adoption behavior is absent.

### 3. OpenShift — connectivity heartbeat missing (Kotlin)
- **Swift** `OpenShiftView` has a **second** `.task` running a 15s `refreshConnectivity()` loop, so a teller who landed on open-shift offline re-adopts their shift the moment the network returns.
- **Kotlin** `OpenShiftScreen` has only the prefill `LaunchedEffect` — **no heartbeat loop**. Combined with #2, an offline-opened teller stays stranded on the open-shift screen.

### 4. Past Shifts — responsive table layout missing (Kotlin)
- **Swift** `ShiftHistoryView` is fully responsive: `wide >= 680` renders a **table** (uppercase `columnHeader` — Opened / Teller / Opening / Declared / Discrepancy — + `tableRow`s inside one card); narrow renders cards. Matches Flutter `_TableHeader`.
- **Kotlin** `ShiftHistoryScreen` renders **only the card layout** — no `wide` branch, no column header, no table rows. (Pending tasks #44/#39 track this.) P1 layout gap.

### 5. Tender — cash presets & tip-cash logic differ (Kotlin)
- Quick-cash presets: **Swift** uses `[5000, 10000, 20000, 50000]` and takes **3** above-due; **Kotlin** uses `[5000, 10000, 20000]` and takes **2**. Kotlin is missing the 500-unit preset and shows one fewer chip.
- Tip-cash due: **Swift** computes `tipCash` from the **tip method's** `isCash` (`tipMethodIsCash` resolves `tipMethod ?? selectedMethod`), so a cash tip on a card order is correctly added to cash due. **Kotlin** `dueCash = total + (if (isCash) tip else 0L)` only adds the tip when the **order** method is cash — a separately-chosen cash tip method is ignored. Behavioral defect.

### 6. PayChip — payment icon missing (Kotlin, Tender)
- **Swift** `PayChip` renders an SF Symbol mapped from `method.icon` (`PayChip.symbol`: banknote/creditcard/wallet/bank/qr…).
- **Kotlin** `PayChip` renders only a **colored dot** — no per-method icon. The whole `symbol(icon:)` mapping is absent.

### 7. ReceiptConfirmation — status iconography differs (Kotlin, Tender)
- **Swift** shows a large success/queued status icon (`checkmark.circle.fill` vs `clock.badge.checkmark`), a queued/sent `StatusChip` *with* icon, and "New order" as an **outline** button with a `plus` icon.
- **Kotlin** shows `SufrixMark` (logo) instead of the success/queued status glyph; the queued/sent chip has no icon; "New order" is a default primary button. Also `ReceiptLineRow` is defined but appears **unused** (ReceiptPaper renders lines) — dead code.

### 8. Sync center — leading tone icon missing (Kotlin)
- **Swift** `SyncView` row has a 38×38 tone-bg square with an **op icon** (`opIcon`: play.circle / lock / doc.text / exclamationmark.circle for dead). 
- **Kotlin** `SyncScreen` row shows only label + status chip — **no leading op-type icon tile**.

### 9. Shared component library — systematic icon omissions (Kotlin `ui/Components.kt`)
The Kotlin component APIs drop the icon parameters that Swift's expose and that the screens rely on:
- **`SufrixButton`** — Swift has `icon:` (renders SF Symbol beside label, used everywhere: checkmark/printer/lock/plus/lock.open/creditcard…). Kotlin `SufrixButton` has **no icon param** → every Kotlin button is text-only.
- **`SufrixTextField`** — Swift has `icon:` + `caps:` (person/envelope/lock/magnifyingglass/note.text/text.bubble). Kotlin has **no icon/caps params** → all fields are icon-less.
- **`StatusChip`** — Swift has optional `icon:` (leading SF Symbol). Kotlin always renders a **colored dot**, no icon support.
- **`NoticeBanner`** — Swift takes a leading `icon:`. Kotlin renders **text only**, no icon.
- **`ToastData`/`ToastHost`** — Swift `ToastData.icon` renders a leading symbol; Kotlin `ToastData` has **no `icon` field** → toasts are icon-less.
- Net effect: Kotlin substitutes Unicode glyphs (≣ ⚙ ⋯ ✕ ⌫ ▤ ¤ 🗑 🛵 🔒 🖨 ↙ ↗ etc.) for SF Symbols throughout. Functional but visually divergent from the SwiftUI fidelity baseline — a coherent icon-asset pass is the biggest single parity lift.

## P2 — Cosmetic / minor

- **OrderView cart**: Swift animates cart-line insert/remove (`.transition(.move/opacity)` + `.animation`); Kotlin `CartPanel` `LazyColumn` has no item animation.
- **Catalog grid extent**: Swift item/bundle grid uses `maximum: 220`; Kotlin `MaxExtentCells(200.dp)` / `BundleGrid 200.dp` — slightly different column breakpoints.
- **Delivery card actions**: Swift uses an overflow `Menu` (`…`) for Add-prep / Finalize / Cancel; Kotlin lays all three out as inline buttons (different affordance, same actions).
- **Login/OpenShift/Settings/Drafts icons**: Swift uses leading SF Symbols (building.2 branch chip, person/envelope/lock fields, storefront/building.2 in Settings, tray.full draft tile, clock.arrow.circlepath carryover hint, `lock.open` open-shift button); Kotlin omits these (subsumed by #9).
- **AmountField**: Swift adds an iOS keyboard "Done" toolbar to dismiss the decimal pad; Kotlin has no IME-dismiss affordance (platform-specific).
- **History row Print/Void**: Swift gives each its own SF Symbol (printer/trash); Kotlin uses `⎙`/`✕` glyphs (subsumed by #9).
- **CachedAsyncImage**: Swift ships a custom FNV-keyed disk+memory `ImageStore` (explicit offline cache); Kotlin uses Coil3 `AsyncImage` (own disk cache). Roughly equivalent — not a true gap.
- **Toast**: Swift `showToast` accepts `icon:`; Kotlin `showToast` signature has no `icon` (subsumed by #9).

## Faithful mirrors (parity OK)
AppModel (modulo #2), ContentView/App, LoginView/LoginScreen, OpenShiftView/OpenShiftScreen (modulo #3), CloseShiftView/CloseShiftScreen, OrderView/OrderScreen (modulo #1 banner, #9 icons, P2 anim), ItemDetailView/ItemDetailSheet (size/addons/optionals/recipe/show-all/search/qty-chips all present), BundleDetailView/BundleDetailSheet, TenderView/TenderScreen (modulo #5–7), OrderHistoryView+VoidSheet/OrderHistoryScreen (void overlay folded in), CashMovements, DeliveryView/DeliveryScreen, DraftsView/DraftsScreen, SettingsView/SettingsScreen, SyncView/SyncScreen (modulo #8), ShiftReportPreview (breakdown + preview), ReceiptPaper + ReceiptPreviewScreen, PinPad, MenuItemCard (category HSL palette, monogram, in-cart badge, decorative ring all matched), BrandPanel, SufrixMark, Skeleton, Money, MaxExtentCells.

### Suggested port order
1. **Reauth flow** (P0 #1) + AppModel reconnect-adoption (#2) + OpenShift heartbeat (#3) — these are functional/offline-correctness defects.
2. **Tender** cash presets + tip-cash logic (#5) — money-correctness.
3. **Past-shifts table** (#4) and **Sync/PayChip/Receipt icons** (#6–8).
4. **Icon-asset pass** across the shared component library (#9) — unlocks the bulk of remaining visual parity.