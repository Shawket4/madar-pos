Confirmed: delivery mutations never touch the offline queue — they are online-only direct API calls. I have everything needed.

# Delivery + Realtime Subsystem Audit — Sufrix POS

## 1. Realtime delivery flow

**Transport: SSE primary + 30s poll fallback.** New orders arrive over Server-Sent Events; polling exists only as a gap-filler while SSE is down. The two never run concurrently.

- **API:** `GET /delivery-orders/stream?branch_id=…` opened in `delivery_api.dart:128` (`openStream`) with `ResponseType.stream`, `receiveTimeout: Duration.zero` (critical — the default 20s receive timeout would kill an idle keep-alive stream), `Accept: text/event-stream`. Bearer header injected by the shared `DioClient` interceptor; a 401 surfaces as a `DioException`.
- **Parser:** `SseFrameParser` (`delivery_realtime_service.dart:360`) — line-based, handles `:` keep-alive comments, multi-line `data:`, and `event:` (`created` / `updated`). Pure/unit-testable.
- **Engine:** `DeliveryRealtimeService` (`delivery_realtime_service.dart`) is an app-wide kept-alive provider (`deliveryRealtimeProvider`, line 350) so alerts fire regardless of current screen.

**Connect gate** (`apply`, line 65): connects only when `online && authed (non-offline session) && shiftOpen && branchId != null`. Driven by:
- `DeliveryRealtimeHost` (`features/delivery/widgets/delivery_realtime_host.dart`) — invisible widget mounted on the order screen that watches `_deliveryGateProvider` and calls `apply()` on any gate change.
- `main.dart` app-lifecycle observer (lines 129-138): `resumed` → `reevaluate()` (reconnect + re-GET); `paused`/`detached` → `pause()` (drop socket, release wakelock).

**Connection sequence** (`_connect`, line 135):
1. REST seed: `loadForBranch(force: true)` because the stream is **updates-only** (no initial snapshot in the stream itself).
2. `_reconcileFromList()` marks existing `received` orders as seen so they don't re-alert (first reconcile also triggers `requestPermissions()`).
3. Open SSE; on success: `_connected = true`, reset attempt counter, **stop the poll**, arm the stall watchdog.

**Liveness / reconnection:**
- **Stall watchdog** (`_resetWatchdog`/`_onStall`, 45s): server sends `:` keep-alive ~every 20s; *any* received line resets the timer. 45s of total silence ⇒ socket assumed silently dead (NAT/Wi-Fi drop with no FIN/RST) ⇒ force-cancel and reconnect. This is the key defense against a "connected but deaf" zombie socket.
- **Backoff** (`_scheduleReconnect`, line 239): exponential `(1<<attempt).clamp(1,30)` with **full jitter** + 250ms floor, attempt capped at 5. Starts the poll fallback for the duration.
- **Deterministic refusals** (`_connect` catch, line 198): 401 (interceptor logs out), 403/404 (perm revoked / branch gone) ⇒ `_teardown()`, **no retry**. Network / 5xx ⇒ backoff reconnect.
- **Wakelock** (`wakelock_plus`): held while the gate is active so the device never sleeps → backgrounds → drops the stream.

**Single detection/effect funnel:** Both SSE events (`_onEvent` → `isNewFromEvent`) and list refreshes (`_reconcileFromList` → `reconcile`) converge on `_fireNewOrderAlert` (line 289), which fires exactly four effects together: OS notification, in-app banner, sound (`Sounds.newOrder`), haptic. `NewOrderDetector` (`new_order_detector.dart`) dedups by order id across seed-GET + SSE + poll + reconnect overlap, alerting each order once.

## 2. Delivery order state machine

Status enum (`delivery_order.dart:39`, wire in `DeliveryStatusX`):

```
received ──(POST /status: confirmed)──▶ confirmed ──▶ preparing ──▶ ready ──▶ out_for_delivery
   │  (accept = POS prints receipt once, then advances)                              │
   │                                                                                 ▼
   ├──(POST /cancel, isReject=true)──▶ rejected (terminal)              (delivered node opens
   │                                                                     the finalize flow)
   └────── any active state ──(POST /cancel)──▶ cancelled (terminal)            │
                                                                                 ▼
                                              (POST /finalize) ──▶ delivered (terminal) + real orders row
```

- **`nextForward`** (`delivery_order.dart:88`) defines the single plain forward step (`received→confirmed→preparing→ready→out_for_delivery`). `delivered` is reached **only via finalize**; `cancelled`/`rejected` only via cancel.
- **Terminal:** `delivered`, `cancelled`, `rejected` (`isTerminal`, line 78). `isActive = !isTerminal` drives the queue + drawer badge.

**Steps the POS performs** (all in `delivery_orders_screen.dart`, routed through `DeliveryOrderRepository` → `DeliveryApi`):

| Step | POS action | Endpoint |
|------|-----------|----------|
| Intake | (server-side; POS only receives) | SSE/GET |
| **Accept** | `_confirm()` (line 474) — **prints customer receipt once** (`_maybePrintReceipt`, print-once via `receiptPrintedAt`), then advances to `confirmed`; clears the new-order alert | `POST /delivery-orders/{id}/status` |
| **Advance** | `_advance(to)` (line 508) — preparing / ready / out_for_delivery | `POST …/status` |
| **Prep time** | `_stepPrepTime` (±5, floored at 0, server-authoritative) | `POST …/prep-time` |
| **Finalize** | `_finalize()` (line 529) — requires an **open shift**, picks payment method, replays the frozen cart snapshot into a real `orders` row; surfaces oversold `warnings` | `POST …/finalize` |
| **Cancel / Reject** | `_cancel({isReject})` (line 566) — reason + `restore_inventory` flag (false ⇒ food made, logged as waste) | `POST …/cancel` |

**Important:** every mutation returns the authoritative updated `DeliveryOrder`, mirrored into list+cache via `upsertOrder`/`applyServerOrder`. The POS owns no client-side transition logic — the server is authoritative, the POS just invokes the right endpoint.

## 3. Notifications — native vs Rust split

**Current state: 100% client-side local notifications. No push (FCM/APNs) anywhere.** There is no push token registration, no FCM/APNs code in the codebase.

- `NotificationService` (`notification_service.dart`) — singleton, `flutter_local_notifications`. `init()` in `main()` sets up an Android channel `delivery_new_orders` (`Importance.high`, sound). Permissions deferred to `requestPermissions()` after first authenticated load (contextual prompt). `showNewOrder` derives the notification id from `order.id.hashCode` (repeat replaces rather than stacks); `cancel(orderId)` clears it on accept. Tap → routes to `delivery-orders`.
- **The decision of *what is new* lives entirely in Dart**: `NewOrderDetector` + `DeliveryRealtimeService._fireNewOrderAlert`. The notification fires only while the app is foreground/alive and the SSE/poll engine is running.

**What must stay native vs what should move to Rust:**

- **Must stay native (platform-only, cannot move):** the actual OS notification post — Android `NotificationManager` (channel creation, importance, `notify`) and iOS `UNUserNotificationCenter` (authorization, banner presentation), plus the permission prompts. These are OS APIs with no Rust equivalent on-device. `flutter_local_notifications` already wraps both; a Rust core can only *ask* the platform layer to post.
- **Should move to Rust (the detection/decision logic):** `NewOrderDetector` (seed/seen-set dedup, "is this genuinely new"), the SSE frame parsing + reconnect/stall/backoff state machine (`SseFrameParser`, `_onStall`, `_scheduleReconnect`), and the gate/lifecycle decisions. This logic is pure and platform-agnostic — it is the natural Rust-core boundary, with a thin native shim that (a) keeps the connection alive in the background and (b) posts the OS notification when the core decides.

**The architectural gap this exposes:** because detection is in-process Dart and there is **no server push**, **no order can ever alert while the app is backgrounded or killed** (the socket is dropped on `pause`, line 137, and nothing re-detects until resume). Moving detection to Rust + a background service, or adding real FCM/APNs push, is the only way to alert a backgrounded teller. This is the strongest argument for the Rust migration of this subsystem.

## 4. Delivery settings

`DeliverySettings` (`delivery_settings.dart`) + `DeliverySettingsNotifier` (`delivery_settings_notifier.dart`), via `GET /delivery/settings` and `POST /delivery/accepting`.

**Ownership split (mirrors backend):**
- **Dashboard/manager owns:** `in_mall_enabled` / `outside_enabled` master switches, `in_mall_fee`, `prep_time_minutes` (branch BASE prep). Read-only to the POS.
- **POS/teller owns only:** the per-channel accepting **override** `auto | open | closed` (`inMallOverride`/`outsideOverride`), flipped via `setMode` → `POST /delivery/accepting`.
- The POS **cannot** re-open a dashboard-disabled channel — the backend returns **409** (`setMode` rethrows so the UI can surface it; `delivery_api.dart:49`).
- Helpers: `enabledFor`, `overrideFor`, `anyEnabled`. Channels: `in_mall` / `outside` (`DeliveryChannel`).
- Hand-written tolerant `fromJson` (defaults everything) so an older/newer wire shape never hard-fails the queue.

## 5. Offline behavior + gaps

**What works offline:**
- **Local-first reads:** `DeliveryOrdersNotifier.loadForBranch` two-phase: paint cache instantly (`loadDeliveryOrdersLocal`), then network refresh. Offline ⇒ `DataFreshness.offline`, cached list still painted. Branch-scoped cache (`saveDeliveryOrders`, `sync_meta`).
- **Gate teardown** preserves UX: a pure offline blip (shift still open) keeps the painted list + dedup set so reconnect is clean; a session end (shift close / sign out / branch change) clears the queue + dedup (`apply`, lines 84-92; belt-and-braces listener in the notifier, line 63).

**Gaps and risks:**

1. **SSE has no replay cursor — the central gap (confirmed in backend contract + client).** The stream is updates-only, and `openStream` sends **no `Last-Event-ID` / cursor** (`delivery_api.dart` query params are only `branch_id`/`status`). Any event emitted during a disconnect window — offline blip, backgrounded app, stall, backoff gap — is **lost from the stream**. The *only* recovery is the REST re-GET on reconnect (`_connect` seed) and the 30s poll. So:
   - A new order arriving during the gap is recovered as data, but its **alert is delayed** up to ~30s (next poll) or until reconnect, and a *status change* during the gap simply lands silently on the next list refresh.
   - There is no server-side guarantee the re-GET (`limit: 200`, newest-first) returns everything that changed — a high-volume branch could theoretically push a changed older order past the 200-row window, though unlikely in practice.

2. **No background alerting (see §3).** While backgrounded/killed the socket is dropped and detection is dead. Orders that arrive then are only surfaced when the teller reopens the app (resume → `reevaluate` → re-GET). No push, no background service.

3. **Mutations are online-only, never queued.** `grep` of `offline_queue.dart` shows **zero** delivery entries — accept/advance/finalize/cancel/prep-time all hit the network directly via `_run`/direct awaits and throw on failure (snackbar). Unlike regular orders (which use the outbox), a teller **cannot accept or finalize a delivery order offline**. This is a deliberate but notable asymmetry given the offline-first mandate.

4. **First-run + offline ⇒ no permission prompt and no cache.** `requestPermissions()` only fires on the first *successful* authenticated load (`_reconcileFromList`, wasSeed path). If the first session is offline, permissions are never requested and there's no cached queue to paint.

5. **`receiptPrinted` print-once is per-device state on the order.** Driven by server `receiptPrintedAt`; if the receipt prints but the status-advance call fails (`_confirm` catch), the local flag may be inconsistent with what the server recorded until the next refresh — low risk but worth noting.

---

### Key file paths
- Engine: `/Users/shawket/Desktop/sufrix_pos/lib/core/services/delivery_realtime_service.dart`
- Detector: `/Users/shawket/Desktop/sufrix_pos/lib/core/services/new_order_detector.dart`
- Notifications: `/Users/shawket/Desktop/sufrix_pos/lib/core/services/notification_service.dart`
- Queue state: `/Users/shawket/Desktop/sufrix_pos/lib/core/providers/delivery_orders_notifier.dart`
- Settings: `/Users/shawket/Desktop/sufrix_pos/lib/core/providers/delivery_settings_notifier.dart` + `/Users/shawket/Desktop/sufrix_pos/lib/core/models/delivery_settings.dart`
- Wire model / state machine: `/Users/shawket/Desktop/sufrix_pos/lib/core/models/delivery_order.dart`
- API / SSE open: `/Users/shawket/Desktop/sufrix_pos/lib/core/api/delivery_api.dart`
- Repository: `/Users/shawket/Desktop/sufrix_pos/lib/core/repositories/delivery_order_repository.dart`
- Gate host: `/Users/shawket/Desktop/sufrix_pos/lib/features/delivery/widgets/delivery_realtime_host.dart`
- Lifecycle wiring: `/Users/shawket/Desktop/sufrix_pos/lib/main.dart` (lines 57, 129-138)
- Actions UI: `/Users/shawket/Desktop/sufrix_pos/lib/features/delivery/delivery_orders_screen.dart` (lines 466-574)
- In-app banner: `/Users/shawket/Desktop/sufrix_pos/lib/shared/widgets/new_order_banner.dart`