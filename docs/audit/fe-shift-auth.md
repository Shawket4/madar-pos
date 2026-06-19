I have a complete and precise picture of all five flows. Here is the audit.

---

# Sufrix POS — Shift / Cash-Movement / Auth-Session Audit

## TL;DR

The headline bug is **real and by design, not accidental**. Closing a shift does not merely clear shift state — `close_shift_screen.dart._close()` deliberately calls `authProvider.notifier.logout()`, which **wipes the JWT from secure storage**. Online this is fine; **offline / on an offline-PIN session it is a trap**, because re-opening a shift requires `/auth/login` (network) and the user is now back at a `/login` screen they can only pass with `offlineUnlock`. The offline path technically exists, but several conditions make it fail in practice (detailed in §6). The token itself is **never cleared on close in any way that's recoverable offline** — `logout()` removes it, and there is no token-refresh endpoint.

Key files: `lib/features/shift/close_shift_screen.dart` (lines 306–313), `lib/core/providers/auth_notifier.dart` (`logout`, `_forceLogout`, `offlineUnlock`), `lib/core/repositories/auth_repository.dart` (`login`, `restoreSession`, `verifyOfflineUnlock`, `logout`).

---

## 1. Open-shift flow, step by step

Entry point: `features/shift/open_shift_screen.dart._submit()` → `ShiftNotifier.openShift()` (`core/providers/shift_notifier.dart:155`).

**Online** (`ConnectivityService.isOnline == true`):
1. `_submit()` validates cash, computes `editReason` (required only if the entered cash deviates from `suggestedOpeningCash`), shows a `ConfirmSheet`.
2. `ShiftNotifier.openShift` → `ShiftRepository.openShift` → `ShiftApi.open` `POST /shifts/branches/{id}/open` (server generates the UUID).
3. On success the shift is saved to local storage (`saveShift`), state gets `freshness: live`, screen navigates to `order`.
4. On error: state error set, button re-enabled.

**Offline** (`shift_notifier.dart:174–211`):
1. Reads `authProvider.user`; if null → "User not authenticated", abort.
2. Generates a client UUID, builds an optimistic `Shift(status: 'open', ...)`, persists it (`saveShift`), and **enqueues a `PendingShiftOpen`** into the outbox.
3. State: `shift: localShift, isLocalShift: true, freshness: stale`; navigate to `order`.
4. Later, when online, the outbox drains via `OfflineQueueNotifier._processEntry` → `ShiftApi.openWithId` (idempotent, client UUID + `opened_at`). A `409` (another shift already open for branch/teller) marks it **dead** and calls `onShiftOpenRejected` → `ShiftNotifier.handleOpenRejected` clears the phantom shift.

**Verdict: open-shift is fully offline-capable** (queued).

---

## 2. Close-shift flow, step by step

Entry: `features/shift/close_shift_screen.dart._close()` → `ShiftNotifier.closeShift()` (`shift_notifier.dart:214`).

**Online & not an offline session** (`shift_notifier.dart:253–270`):
1. `closeShift` → `ShiftRepository.closeShift` → `ShiftApi.close` `POST /shifts/{id}/close`.
2. Clears the cart scope, `removeShift(branchId)`, state `clearShift: true`.
3. Back in `_close()` (close_shift_screen.dart:296–313): clears draft, `_autoPrintReport`, then:
   - `canLogout()` — `auth_notifier.dart:338` — re-checks `/shifts/.../current`; returns `true` when no open shift remains (which it just closed).
   - **If `canLogout()` → `authProvider.notifier.logout()` then `goNamed('login')`.** ← the logout.
   - Else → `goNamed('open-shift')`.

**Offline OR offline session** (`shift_notifier.dart:226–251`):
1. `isOnline == false` **or** `authProvider.isOfflineSession == true` → enqueue `PendingShiftClose`, clear cart scope, `removeShift`, `clearShift: true`. Returns `true`.
2. Back in `_close()`: skips auto-print (`willQueue` true), then **still calls `canLogout()`** (offline → `/shifts/current` throws → falls back to cached shift, which was just removed → returns `true`) → **`logout()` → `/login`.**

**Verdict: close itself queues offline. But the post-close logout fires in both branches**, including offline.

---

## 3. ROOT CAUSE of "closing a shift logs the user out"

It is a **deliberate redirect, executed by clearing the token** — not session==shift, not an accidental 401.

`close_shift_screen.dart`, lines 306–313:
```dart
final canNowLogout = await ref.read(authProvider.notifier).canLogout();
if (canNowLogout) {
  await ref.read(authProvider.notifier).logout();   // ← clears JWT + cached user
  if (mounted && !_navigatedAway) context.goNamed('login');
} else {
  context.goNamed('open-shift');
}
```

`canLogout()` (`auth_notifier.dart:338`) returns `true` whenever there is **no open shift** — which is always true immediately after a successful close. So the `else` (`open-shift`) branch is effectively dead after a normal close; the path taken is **always logout → /login**.

`logout()` (`auth_notifier.dart:360`) → `authRepository.logout()` (`auth_repository.dart:192`):
```dart
Future<void> logout() async {
  setAuthToken(null);          // in-memory Dio token gone
  await _storage.clearAuth();  // removeToken() (Keychain) + removeUser()
}
```
So the JWT is **physically deleted from secure storage** (`SecureTokenStore.clear`). The design intent (encoded in the model `MEMORY.md` and the OpenShiftBlockError logic) is: **one teller per till, sign out at end of shift so the next teller signs in fresh, and the backend enforces "one open shift per user" by refusing login (409) while a shift is open.** Closing the shift is meant to be the end of the session.

**Why this breaks remote/offline sites:** the logout assumes the next sign-in will be an *online* `/auth/login` (the only call that mints a token AND re-saves the offline-unlock hash). At a disconnected site the teller is now at `/login` with **no token**, and to start the next shift they must re-authenticate — see §6 for why that frequently fails.

Note there is no auto-relogin: the redirect goes to `/login`, and the router (`router.dart:42`) sees `!authed` and pins them there.

---

## 4. Offline capability matrix (shift open/close + cash)

| Action | Offline-capable? | Mechanism |
|---|---|---|
| **Open shift** | ✅ Yes | Optimistic local shift + `PendingShiftOpen` queued; synced via idempotent `openWithId` |
| **Close shift** | ✅ Yes (the close write) | `PendingShiftClose` queued; drain holds it until all orders/voids for the shift sync or die |
| **Orders** | ✅ Yes | `PendingOrder` with `idempotencyKey = localId` |
| **Void order** | ✅ Yes | `PendingVoidOrder`, depends-on its order |
| **Cash movement** | ❌ **No — online-only by design** | `enqueueCashMovement` throws `OfflineCashMovementError`; UI disables submit offline |
| **Post-close re-login** | ❌ **Effectively broken offline** | logout clears token; `/auth/login` needs network (§6) |

---

## 5. Cash-movement recording flow & offline status

UI: `features/shift/cash_movement_sheet.dart._submit()` and `cash_movements_screen.dart`.

1. Validates amount (`> 0`) and a **required** note.
2. **Hard offline gate** (`cash_movement_sheet.dart:118-122`): `if (!isOnline) { setError(...); return; }`. The submit button is also disabled offline (`onTap: (!isOnline || loading) ? null : _submit`).
3. Online: `ShiftApi.addCashMovement` `POST /shifts/{id}/cash-movements` (signed amount: +in / −out), then `loadSystemCash()` refresh.
4. Listing: `cashMovements`/`listCashMovements` `GET /shifts/{id}/cash-movements` — **also online-only**; `cash_movements_screen` has no local cache fallback, so offline it shows an error/empty state.

**Why online-only (documented in `offline_queue.dart:218-231`):** the backend has **no idempotency key for cash movements**, so a queued retry after an ambiguous timeout could double-apply cash. `PendingCashMovement` exists in the model and the drain can process it, but `enqueueCashMovement` refuses to queue while offline — it only fires immediately if somehow called online. So cash movements are effectively never queued.

**Honest gap:** offline orders' cash *is* tracked for drawer guidance (`_queuedCashForShift`, `shift_notifier.dart:315`), but a teller cannot record a manual paid-in/paid-out (e.g. petty cash, supplier payment) while offline at all.

---

## 6. Auth token: storage, lifetime, and whether offline re-login is possible

**Storage** (`core/storage/secure_token_store.dart`): JWT in platform secure storage (Keychain/Keystore/DPAPI), mirrored to an in-memory `_cached` for sync reads. Set into Dio via `setAuthToken` (`client.dart:10`).

**Lifetime:** opaque server JWT. **There is no refresh endpoint** (stated in `auth_repository.dart:170-172`). When it expires, a `401` on any call trips `onUnauthorizedCallback` → `_forceLogout` → token cleared → `/login`. The only way to get a new token is an online `/auth/login` (PIN) or `restoreSession`'s `/auth/me` (which still requires a network round-trip to validate).

**Offline re-login — possible in theory, fragile in practice.** `AuthRepository.verifyOfflineUnlock` (`auth_repository.dart:180`) + `AuthNotifier.offlineUnlock` (`auth_notifier.dart:141`) restore the teller's cached identity from a salted PIN hash (`offline_unlock_*`) and a user snapshot (`offline_user_*`), both deliberately **surviving `clearAuth`** (`storage_service.dart:90-109`). The login screen routes a 6-digit PIN to `_offlineUnlock()` when `!isOnline` (`login_screen.dart:148`).

**But it commonly fails after a close at a remote site for these reasons:**

1. **No token → `isOfflineSession = true`.** `offlineUnlock` sets `isOfflineSession: true` and **parks the outbox** (`pauseForAuth`). So even after "re-logging in" offline, every write (including the *next* shift's orders and its close) stays queued and **the queue never drains until a real online login** (`resumeAfterAuth`). The teller is in a degraded session for the entire next shift.

2. **The offline-unlock secret is only written during an online PIN `login`** (`auth_repository.dart:172-173`). If this teller has **never completed an online login on this physical device**, `verifyOfflineUnlock` returns null → "name or PIN not recognized on this device". A brand-new device provisioned at a remote site that then loses connectivity can lock everyone out.

3. **First-launch / cold-start dependency on `/auth/me`.** `restoreSession` (`auth_repository.dart:55`) only returns a cached session on a **network error**; with a *present but expired* token the `/auth/me` `401` path clears auth. Combined with logout-on-close (which removes the token entirely), a fresh app launch after a close has nothing to restore and lands on `/login`.

4. **Connectivity flapping mid-close.** `closeShift` chooses queue-vs-live on `isOnline || isOfflineSession` at call time, but `_close()` separately calls `canLogout()` which hits the network again. A blip can make the close queue but the logout still fire, leaving the teller signed out with a not-yet-synced close.

---

## What currently breaks offline (honest list)

1. **End-of-shift logout strands remote tellers.** Every successful close → `logout()` clears the JWT → `/login`. At a disconnected site the next shift can only be started via the fragile `offlineUnlock` path (§6), and only if that teller previously logged in online on this device.
2. **Offline-unlocked sessions never sync until an online login.** `isOfflineSession` parks the outbox (`pauseForAuth`); the next shift's orders + close pile up and only drain after a real `/auth/login`. There is no token-refresh, so reconnecting alone does not un-park.
3. **Manual cash movements are impossible offline** (in and out), by design — no backend idempotency. Petty-cash / supplier-payment recording is simply unavailable during an outage.
4. **Cash-movement history has no offline cache** — `cash_movements_screen` errors/empties offline even for movements already known.
5. **Devices never logged in online cannot offline-unlock at all** — the salted hash + user snapshot are only created during an online PIN login.
6. **`canLogout()` makes a network call during close** (`auth_notifier.dart:341`); a mid-close connectivity blip can still drive the logout branch even though the close was queued.

## Suggested direction (not implemented)
The cleanest fix for the headline bug: after an **offline / offline-session** close, do **not** call `logout()`. Keep the cached identity (and token if still present) and route to `/open-shift` so the same teller can immediately queue the next shift-open without re-authenticating. Reserve logout-on-close for the **online, real-token** path where `/auth/login` is reachable. This preserves the "one teller per till" intent online while removing the offline trap. The `else → 'open-shift'` branch already exists in `_close()`; the gate just needs to consider `willQueue`/`isOfflineSession` rather than only `canLogout()`.