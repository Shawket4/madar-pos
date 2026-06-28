Confirmed: the only token issuance for app auth is `auth/handlers.rs::login`, and validation is `Validation::default()` (HS256, `exp` enforced, no leeway tuning). I have a complete and accurate picture of the auth model.

---

# Madar Backend Auth/Token Audit — Offline Re-Auth & Shift-Reopen

## 1. Token / Claims Model

The token is a **stateless HS256 JWT** with no server-side session record. Issued only in `auth/handlers.rs::login` via `jwt.rs::create_token`; validated only in `middleware.rs` via `jwt.rs::verify_token` using `Validation::default()`.

**Claims** (`auth/jwt.rs:10-18`):

| Claim | Type | Meaning |
|---|---|---|
| `sub` | String (UUID) | user id |
| `org_id` | Option\<String\> | org; `None` for super_admin |
| `role` | UserRole | `teller` / `branch_manager` / `org_admin` / `super_admin` |
| `branch_id` | Option\<String\> | **set only for tellers** (`handlers.rs:284-288`); the branch they signed into |
| `iat` / `exp` | usize | issued-at / expiry |

**Critically, the token does NOT encode a shift.** There is no `shift_id`, no session id, no "shift open/closed" state inside the token. It binds `user + org + role + branch` only. (`branch_id` is the branch, not a shift.)

**TTL** (`handlers.rs:290`): **teller = 12h**, everyone else = 24h. Hard expiry, no sliding window.

**Refresh token: NONE.** Grepping the entire `src/` for refresh-token machinery returns nothing. There is no refresh endpoint, no refresh table, no rotation. When the 12h JWT expires, the only way to get a new one is to hit `POST /auth/login` again — which requires the server.

## 2. Login Requirements (what each mode needs from the server)

Login is dual-mode (`handlers.rs:115-228`). **Both modes require a live server round-trip** because every verification step is a DB query:

**Email + password** (admins/managers):
- Requires `email`, `password`, optional `org_id`.
- Server: `SELECT … FROM users WHERE email=$1 …`, then `bcrypt::verify(password, password_hash)`.
- The bcrypt **hash lives only in the DB** — never sent to the device.

**PIN + name** (tellers — the relevant path for POS offline):
- Requires `name`, `pin`, **`branch_id`** (mandatory).
- Server does **four** DB queries: (a) derive `org_id` from `branches` by `branch_id`; (b) load all tellers matching `name`+org with a `pin_hash`; (c) `bcrypt::verify(pin, pin_hash)` per candidate; (d) check `user_branch_assignments` for branch access. Then two more shift-state checks.
- The `pin_hash` (bcrypt) **lives only in the DB**; the device never receives it.

Plus `branch_id` itself normally comes from `POST /auth/resolve-branch` (GPS → branch) — **also a server call**.

## 3. What the middleware validates per request, and offline behavior

`middleware.rs:58-99`: extract `Bearer` token → `verify_token` (HS256 signature + `exp`) → inject `Claims`. **No DB lookup, no denylist check, no shift-state check.** This is purely cryptographic and stateless.

**Consequence — this is the one thing that already works offline:** the POS holds the signed JWT locally. As long as it hasn't expired, the device could in principle keep validating it itself with the same secret… except the device does **not** have the JWT secret (only the server does), so the device can't *re-validate* offline either — it just trusts the stored token until `exp`. An **expired** token is rejected by `Validation::default()` (no leeway tuning anywhere — confirmed `src/auth/jwt.rs:79` is the only validation config). Once `exp` passes, the token is dead and there is **no offline path to mint a new one.**

## 4. Is the token invalidated on shift close? (Server-side vs client-side)

**Server-side: NO.** `close_shift` (`shifts/handlers.rs:835-930`) does exactly one mutation: `UPDATE shifts SET status='closed', closing_cash…`. It **never touches the user, the token, or any session.** Same for `force_close_shift`. There is no token store to revoke against — the JWT is stateless.

So **closing a shift does not log the teller out at the protocol level.** The 12h JWT remains cryptographically valid after close. Any "you're logged out after close" behavior is **purely a client-side (POS) decision** — the backend would still accept that same Bearer token.

This is reinforced by login's own comment (`handlers.rs:234-241`): re-login is explicitly allowed to *resume* a still-open shift after token expiry. The backend treats auth and shift as **already decoupled** — the only coupling is at *login time*, where it runs shift-state guards (`handlers.rs:242-282`) that can *reject* a login (e.g. open shift elsewhere), and that's the same path that's unavailable offline.

## 5. Data the server holds that a device would need for OFFLINE PIN verification

To verify a PIN with zero network, a device needs, per teller assigned to that branch:
- the **bcrypt `pin_hash`** (`users.pin_hash`, `models/mod.rs:30`),
- `id`, `name`, `org_id`, `role`, `is_active`,
- the `user_branch_assignments` rows for that branch,
- branch→org mapping.

**Today none of this is provisionable.** `UserPublic` (`models/mod.rs:40-64`) — the only user shape ever serialized — **deliberately omits `password_hash` and `pin_hash`** (the `From<User>` impl at `:66-79` drops them). No endpoint anywhere ships a hash to the client. So the device structurally cannot verify a PIN offline. That, combined with "no refresh token" and "expired JWT is hard-rejected," is the **exact reason offline re-auth fails.**

---

## The exact reason offline re-auth fails (one paragraph)

The JWT is the *only* credential the device holds, it is short-lived (12h teller), and there is **no refresh token**. Every way to obtain a fresh token (`/auth/login`, `/auth/resolve-branch`) is a server round-trip, because PIN/password verification runs against bcrypt hashes that **exist only in the DB and are never provisioned to the device** (`UserPublic` strips them). The middleware can't help — it has no offline re-issue path and `Validation::default()` hard-rejects an expired token. Shift close doesn't cause this (it never touches the token); the wall is simply **token expiry + no offline credential to re-mint against.** Opening a new shift offline after a close fails for the same root cause: `open_shift` is an authenticated endpoint, so it needs a live, unexpired token, and after a close+expiry the device can't produce one.

---

## Menu of Backend Options to Enable Offline Re-Auth / Shift-Reopen

Ordered roughly easiest→strongest. Each is independent; (B) + (C/D) compose well.

### Option A — Long-lived / refresh token (decouple "device session" from short JWT)
Issue a second long-lived **refresh token** at login (e.g. 30–90 days), stored in a new `refresh_tokens` table (hashed, revocable). POS uses it to silently mint new access JWTs when online; raise the access-token TTL for tellers (12h → e.g. 7d) so brief outages don't lock anyone out.
- **Pros:** small, well-understood change; keeps verification server-side; revocable per device.
- **Cons:** **does not actually solve true-offline** — refresh still needs the server. It only widens the window. A stolen long-lived JWT is valid until expiry (no revocation on the stateless access token unless you add a denylist).
- **Verdict:** necessary hygiene, insufficient alone for hard-offline shift reopen.

### Option B — Decouple auth from shift entirely (cheapest correctness fix; mostly already true)
Make it explicit policy that **a valid token survives shift close**, and that **`open_shift` requires only a valid token, not a re-login.** The backend already does this (close doesn't revoke; `open_shift` only needs auth). The "must re-auth to open a new shift" pain is a **client/TTL artifact**: bump teller TTL and stop forcing logout on close.
- **Pros:** zero new attack surface; arguably just a config + POS-flow change. Solves "open a new shift after a close" **as long as the token is still within TTL.**
- **Cons:** still bounded by `exp`. If the device is offline *past* expiry, you're back to needing A or C/D.
- **Verdict:** do this regardless — it's the correct framing and removes most of the friction.

### Option C — Provision a per-branch PIN-hash bundle for true offline PIN verification
New authenticated endpoint, e.g. `GET /branches/{id}/offline-auth-bundle`, returning, for each teller assigned to that branch: `user_id, name, role, is_active`, and the **bcrypt `pin_hash`**. POS caches it (sqflite) and verifies PINs locally with bcrypt; on success it **self-issues a locally-signed offline token** (or just unlocks the app and queues an `open_shift` action to sync later). Bundle is fetched while online and refreshed periodically.
- **Pros:** the only option that delivers **genuine offline PIN re-auth and offline shift-open**. Bcrypt is verifiable client-side. Fits the existing offline-first POS (sqflite) and pending-action sync model.
- **Cons / tradeoffs:** **ships password-equivalent hashes to devices.** Mitigate:
  - Use a **separate offline PIN credential** (a distinct, higher-cost bcrypt/argon2 hash) rather than reusing the login `pin_hash`, so a leaked bundle ≠ full credential.
  - Encrypt the bundle at rest with an OS-keystore key; bind it to the device (see D).
  - Scope strictly to the branch's own tellers; short bundle freshness window; ability to rotate.
  - Accept that a *deactivated* or *PIN-changed* teller stays valid offline until the next bundle refresh (inherent to offline). Sync reconciles on reconnect.
- **Verdict:** the real answer for hard-offline. Pair with B.

### Option D — Device-bound long-lived token (provisioned-device model)
At first online login, POS generates a keypair; server registers the device (`device_id`, public key, branch) and issues a **long-lived, device-bound token** (token bound to the device public key; POS proves possession of the private key on each local unlock). Offline unlock = local PIN gates access to the device key; the device key authorizes offline shift operations that sync later.
- **Pros:** strong: a stolen token is useless without the device private key (in secure enclave/keystore); cleanly revocable server-side by `device_id`; no password-hash export.
- **Cons:** most engineering (device registration, key custody, attestation, revocation list, sync of device state). PIN here only locally gates the device key — server can't *verify the PIN* offline, so PIN-change enforcement is still deferred to reconnect.
- **Verdict:** best security posture; heaviest build. Strong long-term direction, often layered over C.

### Recommended combination
1. **B now** — bump teller access TTL, stop treating close as logout, confirm `open_shift` needs only a valid token. Removes most real-world pain with zero new surface.
2. **A** — add refresh tokens + a revocation/denylist table so longer-lived access can still be killed.
3. **C (with a dedicated offline-PIN hash) + D's device binding** — for true offline reopen across long outages: a device-bound, encrypted, branch-scoped PIN-hash bundle that the POS verifies locally and reconciles on sync.

---

**Files audited (all absolute):**
`/Users/shawket/Desktop/SufrixRust/src/auth/jwt.rs`, `/auth/handlers.rs`, `/auth/middleware.rs`, `/auth/guards.rs`, `/auth/routes.rs`, `/auth/mod.rs`; cross-checked `/src/models/mod.rs` (User / UserPublic, lines 23-79) and `/src/shifts/handlers.rs` (open_shift 303-, close_shift 835-930). No refresh-token or token-revocation code exists anywhere in `src/`.