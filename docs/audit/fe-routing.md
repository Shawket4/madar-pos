I have everything needed. Here is the complete audit.

---

# Cache-Based Routing Audit — Madar POS

## 1. Architecture as-found

Routing is split across **three layers**, not one. Moving it into the Rust core requires capturing all three — the central redirect is only ~60% of the real decision logic.

| Layer | File | Role |
|---|---|---|
| **Central redirect/guard** | `lib/core/router/router.dart` `redirect()` | Auth + device-config gate; picks `/order` vs `/open-shift` from cached shift. |
| **Re-validation hop (force-logout)** | `lib/core/api/client.dart` interceptor → `auth_notifier.dart` callbacks | 401/403 → `_forceLogout` → state change → `refreshListenable` re-fires redirect → `/login`. |
| **Imperative per-screen navigation** | `order_screen.dart`, `open_shift_screen.dart`, `close_shift_screen.dart`, `action_drawer.dart`, `device_setup_screen.dart` | "Cache said `/order` but live server says shift is closed → `goNamed('open-shift')`", post-close routing, etc. |

The central redirect is **pure-read** by design (comment at `router.dart:50-53`): it must not mutate providers, so it reads the cache directly. The "reconcile cache vs. live server" decisions are therefore pushed *out* into screens (`order_screen.dart:162-168`), which is the main source of fragility.

---

## 2. State-transition table

Inputs (abbreviated): `loading` = `auth.isLoading`; `authed` = `auth.user != null`; `configured` = `storage.isDeviceConfigured` (= `deviceOrgId != null && deviceBranchId != null`); `hasShift` = cached open shift (see §3); `loc` = `state.matchedLocation`.

### A. Central `redirect()` (`router.dart:31-67`)

| # | loading | authed | configured | hasShift | loc (current) | → Destination | Source |
|---|---|---|---|---|---|---|---|
| 1 | **true** | * | * | * | any | `null` (stay) — never redirect mid-auth | `:40` |
| 2 | false | false | **false** | * | `/device-setup` | `null` (stay) | `:44` |
| 3 | false | false | **false** | * | anything else | `/device-setup` | `:44` |
| 4 | false | false | true | * | `/login` or `/device-setup` | `null` (stay) | `:47` |
| 5 | false | false | true | * | anything else | `/login` | `:47` |
| 6 | false | **true** | * | **true** | `/login` | `/order` | `:62-63` |
| 7 | false | **true** | * | **false** | `/login` | `/open-shift` | `:62-63` |
| 8 | false | true | true | true | `/device-setup` | `/order` | `:65` |
| 9 | false | true | true | false | `/device-setup` | `/open-shift` | `:65` |
| 10 | false | true | **false** | * | `/device-setup` | `null` (stay) | `:65` |
| 11 | false | true | * | * | any other authed route | `null` (stay — no guard) | `:67` |

Note row 11: once authed, **every** route (`/close-shift`, `/cash-movements`, `/settings`, `/order-history`, `/delivery-orders`, `/pending-orders`) is freely reachable — the redirect does **not** gate `/order`-family routes on `hasShift`. A teller deep-linked to `/order` with no shift is *not* bounced by the redirect; the order screen handles it imperatively (row 17).

### B. Force-logout path (401 / dead-token-as-403) — the "force-logout → /login" path

| # | Trigger | Guard | Action | → Result | Source |
|---|---|---|---|---|---|
| 12 | HTTP **401** on any request | `user != null && !isOfflineSession` | `onUnauthorizedCallback` → `_forceLogout(expired)` | clears auth (keeps cart) → state change → redirect re-fires → **`/login`** with `SessionExpiry.expired` banner | `client.dart:48-50`, `auth_notifier.dart:82-90,378-384` |
| 13 | HTTP **403** on any request | `user != null && !isOfflineSession` | `onForbiddenCallback` → `revalidateAfterForbidden()` → probe `/auth/me` | probe 401 → `_forceLogout(expired)` → **`/login`**; probe OK → stay (true permission denial); network err → stay | `client.dart:54`, `auth_notifier.dart:91-98,121-136` |
| 14 | App **resumed** from background | `user != null && !isOfflineSession && !isLoading` | `revalidateSession()` → `/auth/me`; 401 trips interceptor (→ row 12) | dead JWT → **`/login`**; else stay | `main.dart:129-130`, `auth_notifier.dart:329-336` |
| 15 | 401 **during login/restore hydrate** | inside `_hydrateAfterAuth` / `restoreSession` | `_expireDuringHydrate()` / `clearAuth()` | never becomes authed → **`/login`** (`expired`) | `auth_notifier.dart:258,295,370-375`; `auth_repository.dart:72-76` |
| 16 | Login blocked: **another teller's open shift** | server preFill `openShift.tellerId != user.id` (or HTTP 409) | `logout()` + `SessionExpiry.blockedByOtherShift` | stays **`/login`** with blocked banner | `auth_notifier.dart:271-285`; `auth_repository.dart:150-156` |

### C. Imperative per-screen navigation (out-of-band)

| # | Screen | Condition | → Destination | Source |
|---|---|---|---|---|
| 17 | Order screen mount (`_prefetch`) | live shift fetch `freshness==live && !hasOpenShift` (cache was stale-open) | `goNamed('open-shift')` | `order_screen.dart:162-167` |
| 18 | Open-shift screen, on reconnect | live `load()` shows `hasOpenShift` (another device opened it) | `goNamed('order')` | `open_shift_screen.dart:109-113` |
| 19 | Open-shift screen, after opening | `openShift()` success | `goNamed('order')` | `open_shift_screen.dart:111,172` |
| 20 | Close-shift screen, after close | `canLogout()` true → logout → `login`; else `open-shift` | `goNamed('login')` / `goNamed('open-shift')` | `close_shift_screen.dart:306-313` |
| 21 | Action drawer / settings "end shift" | user confirms | `goNamed('close-shift')` | `action_drawer.dart:218,237`; `settings_screen.dart:59` |
| 22 | Offline shift-open **rejected** by server | `onShiftOpenRejected` → `handleOpenRejected` clears phantom shift | next redirect/nav lands `/open-shift` | `main.dart:107-112`; `shift_notifier.dart:277-286` |
| 23 | Device setup confirmed | manager picks branch | `goNamed('login')` | `device_setup_screen.dart:150` |

---

## 3. Exact cached values the decision depends on

All reads are synchronous off `KvStore._cache` (kv table) or the keychain. **Force-closed is NOT a distinct cached state** — see §5.

| Cached value | Storage key / source | Read at | Routing meaning |
|---|---|---|---|
| **JWT token** | keychain (`SecureTokenStore`), in-memory `_currentToken` (`client.dart:9`) | `restoreSession` `:56`; every request header | present+valid → authed path; absent/401 → `/login` |
| **device_org_id** | kv `device_org_id` | `isDeviceConfigured` `:81` | half of `configured` |
| **device_branch_id** | kv `device_branch_id` | `isDeviceConfigured` `:81`; `login()` `:127` | half of `configured`; the branch all shift lookups key on |
| device_branch_name | kv `device_branch_name` | (display only) | not routing |
| **cached_user** | kv `cached_user` | `restoreSession` offline branch `:66-69` | offline restore → `authed` without live `/auth/me` |
| **cached shift** | kv `shift_{branchId}` → `Shift.fromJson` | `loadShiftLocal` (`shift_repository.dart:51`) via `router.dart:55-60` | `hasShift = shift.status == 'open'` (`shift.dart:7`) → `/order` vs `/open-shift` |
| in-memory `shiftProvider.hasOpenShift` | `ShiftState` | fallback at `router.dart:60` when `branchId == null` | secondary `hasShift` source |
| **offline_unlock_{name}** | kv (salted PIN hash) | `offlineUnlock` | enables offline `authed` with `isOfflineSession=true` (no token) |
| offline_user_{name} | kv | `loadOfflineUser` | user snapshot for offline session |
| branch_{id} | kv | `_hydrateAfterAuth` cache fallback `:259` | not routing (timezone/printer) |
| **Connectivity** | `ConnectivityService.instance.isOnline` (in-memory, not cache) | `shift_notifier.load`, open-shift screen | gates whether live reconcile (rows 17-18) ever runs |

`hasShift` derivation precedence (`router.dart:54-60`):
1. `branchId = auth.user?.branchId`
2. if `branchId != null`: `loadShiftLocal(branchId)?.hasOpenShift`
3. else: fall back to live `shiftProvider.hasOpenShift`

---

## 4. Proposed Rust-core routing API

The goal: the UI calls one function, gets back a destination enum. Rust owns the cache (token/device-config/shift) and the decision matrix. The UI keeps only the *imperative side-effects* it triggers (calling `login`, `open_shift`, etc.), and after each it re-asks `app_route()`.

```rust
/// Every routing-relevant input the UI must surface to the core.
/// Connectivity and "current location" are runtime, not cached, so they're params.
pub struct RouteContext {
    pub current: Route,              // where the UI is now (for stay-vs-move)
    pub is_online: bool,             // ConnectivityService.isOnline
    pub auth_in_progress: bool,      // == auth.isLoading; core returns Stay while true
}

pub enum AppDestination {
    Stay,                            // null in current redirect
    DeviceSetup,
    Login { reason: LoginReason },   // carries the banner the UI shows
    OpenShift,
    Order,
    CloseShift,
}

pub enum LoginReason {
    None,
    SessionExpired,                  // SessionExpiry.expired  (force-logout)
    BlockedByOtherShift { teller_name: String },
}

impl Core {
    /// Pure function of cached state + ctx. No I/O. Mirrors router.dart redirect().
    pub fn app_route(&self, ctx: &RouteContext) -> AppDestination;

    /// The cached inputs, exposed so the UI never re-implements key parsing.
    pub fn is_device_configured(&self) -> bool;        // device_org_id && device_branch_id
    pub fn auth_status(&self) -> AuthStatus;           // SignedOut | Online | Offline
    pub fn cached_shift_state(&self, branch: &str) -> ShiftCacheState; // Open|Closed|None

    /// Side-effecting transitions. Each returns the NEXT destination so the UI
    /// can navigate without re-deriving — this replaces the imperative goNamed()
    /// scattered across screens (rows 17-23).
    pub async fn login(&self, name: &str, pin: &str) -> Result<AppDestination, AuthError>;
    pub async fn offline_unlock(&self, name: &str, pin: &str) -> Result<AppDestination, AuthError>;
    pub async fn open_shift(&self, branch: &str, opening_cash: i64) -> Result<AppDestination, _>;
    pub async fn close_shift(&self, ..) -> Result<AppDestination, _>; // → Login if canLogout else OpenShift
    pub async fn revalidate_session(&self) -> AppDestination;          // resume / 401 / 403 probe → Login|Stay
    pub fn force_logout(&self, reason: LoginReason) -> AppDestination; // → Login (keeps cart)
}
```

**Migration mapping** (Flutter → Rust core):
- `router.dart` redirect rows 1-11 → `app_route()` (pure).
- Force-logout rows 12-15 → core owns the 401/403 interceptor; `force_logout`/`revalidate_session` return `Login{SessionExpired}`. The UI's `_AuthListenable` becomes "core emits a route-changed event → UI calls `app_route()`".
- **Critically, fold rows 17-18 into the core**: instead of the order/open-shift screens reconciling live-vs-cache with `goNamed`, `open_shift`/a `refresh_shift` core call returns the corrected `AppDestination`. This removes the split-brain where the redirect says `/order` but a screen immediately re-routes.
- Row 16/20/22 (`blockedByOtherShift`, post-close, open-rejected) → encoded in `LoginReason` / the `Result<AppDestination>` returns.

**Recommended UI contract:** UI navigates *only* in response to (a) the result of a core transition call, or (b) a core "route invalidated" event followed by `app_route()`. No screen computes a destination itself.

---

## 5. Edge cases that are currently fragile

1. **"Force-closed" is invisible to the cache.** There is no `force_closed` status anywhere — `Shift.isOpen` is purely `status == 'open'` (`shift.dart:7`). When an admin force-closes a shift from the dashboard, the local `shift_{branchId}` cache still says `status:'open'`, so `redirect()` sends the teller to `/order` (row 6). Correction happens **only** if `order_screen._prefetch` reaches a *live* server (`freshness==live`) and re-routes (row 17). **Offline, the teller sells against a dead shift indefinitely.** In the Rust core this should be a first-class `ShiftCacheState::ForceClosed` reconciled on every fetch, and `app_route` should treat unknown/closed as `/open-shift`.

2. **Redirect doesn't gate `/order` on `hasShift`.** Row 11: any authed deep-link to `/order` (OS notification via `rootNavigatorKey`, restored route) is allowed even with no open shift. The guard lives in the screen, not the router — a screen that fails to run its `_prefetch` reconcile leaves a teller on an order screen with no shift.

3. **Two `hasShift` sources can disagree.** `router.dart:54-60` prefers `loadShiftLocal(branchId)` but falls back to the in-memory `shiftProvider` when `branchId == null`. A user with `branchId == null` (manager-ish account) routes off volatile in-memory state that `seedFromAuth`/`loadLocal` may not have populated yet at redirect time → race to `/open-shift` vs `/order`.

4. **Redirect only re-fires on auth `isLoading`/`isAuthenticated` changes** (`_AuthListenable`, `router.dart:87-95`). A pure *shift-cache* change (e.g. `handleOpenRejected` clearing the phantom shift, row 22) does **not** by itself re-trigger the redirect — it relies on a subsequent navigation event. So clearing a rejected offline shift doesn't proactively bounce the user off `/order`.

5. **403 → logout depends on a second network round-trip.** Row 13 only force-logs-out if the `/auth/me` probe *itself* returns 401. If the probe gets a network error (flaky link), a genuinely-dead-token-surfacing-as-403 is left authenticated, and the user stays on a screen that keeps 403-ing. Single-flight guard (`_forbiddenRevalidation`) is correct, but the outcome is connectivity-dependent.

6. **Offline session keys on `isOfflineSession` everywhere.** Rows 12-14 are all suppressed when `isOfflineSession` (no token by design). If that flag is ever wrong (e.g. an offline-unlock that later silently acquires a token), 401s would be swallowed and the teller never routed to `/login`. The flag is the single point of truth gating the entire force-logout machine.

7. **Corrupt-cache silently degrades routing.** `loadShiftLocal` and `_decode` swallow a corrupt `shift_{branchId}` blob → returns null → `hasShift=false` → `/open-shift` (`shift_repository.dart:61`, `storage_service.dart:34-39`). A corrupt cache silently demotes an open shift to "no shift," which is safe-ish but invisible. Similarly an undecodable branch cache is treated as a miss (`auth_notifier.dart:388-394`).

8. **`canLogout()` post-close depends on a live call with cache fallback.** Row 20 routes `/login` vs `/open-shift` based on `canLogout()`, which hits `/auth/me`-adjacent shift API and falls back to cached `loadShift` on error (`auth_notifier.dart:338-352`). Two different cache reads (`loadShift` here vs `loadShiftLocal` in the router) parse the same key through different paths — drift risk if one changes.

---

## Key files (absolute paths)
- `/Users/shawket/Desktop/sufrix_pos/lib/core/router/router.dart` — central redirect (rows 1-11) + `_AuthListenable`.
- `/Users/shawket/Desktop/sufrix_pos/lib/core/providers/auth_notifier.dart` — auth state, `_forceLogout`/`_expireDuringHydrate`, 401/403 callbacks, `revalidateSession`.
- `/Users/shawket/Desktop/sufrix_pos/lib/core/providers/shift_notifier.dart` — `hasOpenShift`, `seedFromAuth`/`loadLocal`/`load`, `handleOpenRejected`.
- `/Users/shawket/Desktop/sufrix_pos/lib/core/api/client.dart` — Dio interceptor wiring 401→`onUnauthorizedCallback`, 403→`onForbiddenCallback`.
- `/Users/shawket/Desktop/sufrix_pos/lib/core/repositories/auth_repository.dart` — `restoreSession`, `login` (401/403/409 mapping), `logout`, `validateToken`.
- `/Users/shawket/Desktop/sufrix_pos/lib/core/repositories/shift_repository.dart` — `loadShiftLocal` (cache→`hasOpenShift`).
- `/Users/shawket/Desktop/sufrix_pos/lib/core/storage/storage_service.dart` — `isDeviceConfigured`, `loadShift`/`loadBranch`, token, offline-unlock keys.
- `/Users/shawket/Desktop/sufrix_pos/lib/core/models/shift.dart` — `isOpen == status == 'open'` (no force-closed state).
- `/Users/shawket/Desktop/sufrix_pos/lib/main.dart` — lifecycle `revalidateSession` (row 14), `onShiftOpenRejected` (row 22).
- Imperative nav (rows 17-23): `lib/features/order/order_screen.dart`, `lib/features/shift/open_shift_screen.dart`, `lib/features/shift/close_shift_screen.dart`, `lib/features/order/widgets/action_drawer.dart`, `lib/features/setup/device_setup_screen.dart`.