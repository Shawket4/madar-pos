# Madar POS Rebuild — Master Plan

> **One artifact, three apps.** A shared Rust core (UniFFI) drives thin SwiftUI (iPhone/iPad) and Compose (Android phone/tablet + JVM desktop) UIs. The teller can sell for days offline; everything reconciles when the network returns.

---

## Revision 2 — Post-Audit Consolidation (current source of truth)

> Folds in the two deep cross-repo audits — [docs/04-offline-audit.md](docs/04-offline-audit.md) (printing, offline/cache, routing, shift/auth, backend offline-support) and [docs/05-domain-audit.md](docs/05-domain-audit.md) (cart/pricing, checkout/void, menu, delivery/realtime, inventory) — plus decisions locked with the product owner. **Where this section differs from the original body below, this section wins;** the body remains for design detail.

### R1. Decisions locked since v1
- **D9 — Pricing is CLIENT-AUTHORITATIVE; the Rust `pricing` module IS the money.** `create_order` records the client's `subtotal/discount/tax/total` **verbatim**; it computes an "expected" total only to set an *unread* `price_flagged` advisory and **never rejects** (the only money-reject is split-sum ≠ total). No server safety net. → `pricing` is a pure, golden-vector-tested module built right after `store`, matching the server formula byte-for-byte: integer piastres; **ties-away-from-zero** rounding; order = subtotal → discount → **tax-on-discounted-base** → total; single org-wide **exclusive** tax rate; bundle base price **fixed** + component surcharge only; trust wire `unit_price` (don't recompute addon swap deltas). Always send the full breakdown. (doc 05 §2)
- **D10 — Printing is 100% shared Rust, NO native seam.** Fleet = **Epson** (ESC/POS raster) + **Star TSP100** (raster-only) — both LAN, both driven over **raw TCP :9100** from Rust, identical on iOS/iPadOS/Android/desktop. No StarXpand SDK. Sole cost = the Rust rasterizer (layout → 1-bpp bitmap, Arabic shaping via rustybuzz/harfbuzz), needed for both brands. Open: confirm no USB-only TSP143U units.
- **D11 — One drawer per branch.** One open shift per branch/teller; no register dimension. Simplifies checkout/cash reconcile.
- **D12 — Offline-first auth = logout-on-close + a server-provisioned ORG-scoped offline bundle (argon2id verifiers, software-encrypted at rest, NO Secure Enclave / no device-binding) + online-reauth-to-sync.** Cross-shift, cross-teller offline works; one online reconnect flushes ALL queued offline shifts. (R3)
- **D13 — Tellers are ORG-scoped, not branch-scoped.** Remove the per-branch lock — the login `403 BranchAccessError` and any per-branch operational gate — online AND offline. Any active org teller operates at any branch's device; the device still stamps its own branch on shifts/orders. (R3)

### R2. Headline fix — shift-close no longer strands a teller offline
Root cause (confirmed both sides): logout-on-close is a **pure client policy**. Backend `close_shift` only flips `status='closed'` and never touches the token; the JWT stays valid its full TTL. Flutter deliberately calls `logout()` on close, and offline `/login` can't mint a new token → trap. The backend already decouples auth from shift. (doc 04 §2)

### R3. Offline-auth plan (FINAL — org-scoped bundle, no enclave, full cross-shift offline)
**Model: logout-on-close + offline re-auth against a server-provisioned ORG-scoped bundle + online-reauth-to-sync. Many shifts by many tellers run offline; one online reconnect flushes them all.**
- **Logout on close** (offline AND online) — each shift is a per-teller boundary; the next shift begins with a login (which may be an *offline* unlock).
- **Tellers ORG-scoped (D13):** remove the per-branch lock (login `403 BranchAccessError` + any per-branch operational gate). Any active **org** teller may operate at any branch's device, online or offline. The device is still configured to a branch (shifts/orders carry that `branch_id`); only teller *authorization* loosens to org level. (Branch-assignment data may persist for management/reporting but is no longer an operational gate.)
- **Offline-auth bundle (ORG-scoped):** `GET /orgs/{id}/offline-auth-bundle` → for every active org teller `{user_id, name, role, is_active, offline_pin_hash}`, using a **dedicated `users.offline_pin_hash` (argon2id, memory-hard)** — NOT the login hash. Fetched while online, refreshed periodically, **encrypted at rest with a software key — NO Secure Enclave, no device-binding** (the hardening that was dropped). This lets **any org teller, even one who has never logged in on this device, unlock offline.**
- **Offline re-auth:** verify the typed PIN against the cached bundle (argon2id); no token minted, identity = the real server `user_id`.
- **Cross-shift / cross-teller offline:** many shifts by many tellers queue offline; **each shift carries its own client `teller_id`**; its orders/cash reference the shift's client UUID and inherit the teller. The outbox is intentionally multi-teller.
- **Reconnect → alert → sync everything:** on reconnect with a queue but no valid online token, a **non-blocking alert (R11): "sign in online to sync."** Any org teller's online re-auth then flushes the **entire** multi-shift, multi-teller backlog via `POST /sync/replay` (ordered, idempotent); the backend honors **each shift's client `teller_id`, validated as an active ORG teller** (not the authenticating token's teller, not branch). One reconnect syncs all previous shifts + all offline data.
- **Security tradeoff (accepted):** the org bundle ships argon2id PIN verifiers for all org tellers, **software-protected (no hardware backing)**; the re-authing teller vouches for the device's whole queue (server validates each shift's teller is an active org member). Mitigations: dedicated argon2id (≠ login hash), bundle TTL + rotation, on-device attempt lockout, encrypted at rest. No hardware non-repudiation — that's what the dropped device credential bought.
- **TTL:** size the online access token to comfortably exceed a max online shift (so a teller isn't kicked mid-sale); the *offline* path doesn't depend on it (offline-unlock carries no token, and sync always does a fresh online re-auth). Refresh token (`/auth/refresh`) optional smoothing.

### R4. rust-core module build sequence (refines Phase 1)
`store`/`sync`/`outbox` (incl. the **unified per-order idempotency key**, F1) → **`pricing`** (pure, golden-vector) → `cart` + `menu` (override/i18n resolution) → `checkout` (split-sum invariant, change calc, idempotency mint) + `print` (TCP) → `orders::void` → `session`/auth (Layers) → `realtime`/`delivery` (online-only first; queue after backend B3) → `recipe` (preview only). **No inventory-authority module** — depletion is server-side and replay-safe (doc 05 §5).

### R5. Fix-in-port checklist (do NOT carry the bugs forward) — detail in doc 05 §6
- **F1 (P0)** Unify the order idempotency key — mint ONE uuid per placement *before* the online attempt, reuse on the queued path. Today online uses the cart tab id (default literal `"order_1"`), the queue mints a fresh uuid → lost-response retry ⇒ **double order + double stock depletion**.
- **F2 (P0)** Kill the reusable `"order_1"` default key (subsumed by F1).
- **F3 (P1)** Void of a `pending_sync` order must force-queue (online path sends a local uuid → 404).
- **F4 (P1)** One change-due formula (UI shows `tendered−total−cashTip`; receipt/DB record `tendered−total`).
- **F5 (P1)** `restore_inventory` default → true.
- **F6 (P1)** SSE reconnect reconcile (real fix is the backend cursor, R6).
- **F8 (P2)** Clamp percentage discount to subtotal (avoid negative totals); preserve zero-tax & 100%-discount correctness.
- **F9 (P1, upgraded — see R12)** Recipe-expansion exists in **3 places** (Flutter preview, backend `preview_recipe`, backend `create_order` depletion) and has **already drifted** — the depletion does ingredient-unit conversion on swaps that the Flutter preview omits (preview ≠ actual). Fix via ONE shared Rust expansion crate. **F10 (P2)** Drop dead inventory/void plumbing; fix `menu_notifier` `cachedAt` key.

### R6. Backend offline-first workstream (consolidated, additive/backward-compatible)
- **P0:** cash-movement `client_ref` + unique index (unblocks offline cash) ✓ done; `force_close` idempotent (200 on replay; dashboard-only); central `idempotency_keys` middleware extended to `void_order`/inventory/stocktake/purchasing; **remove the per-branch teller lock → org-scoped authorization (D13)** (drop login `403 BranchAccessError` + per-branch operational gates; authorize by org membership); **auth/shift decouple + teller TTL** sized to exceed a max online shift.
- **P1 (the cross-shift-offline enabler):** **`GET /orgs/{id}/offline-auth-bundle`** (org-scoped argon2id verifiers) + a dedicated **`users.offline_pin_hash`**; **shift-open / replay honors a client-supplied `teller_id`, validated as an active ORG teller** (cross-teller attribution — orders/cash inherit via the shift's client UUID); **`POST /sync/replay`** ordered, **multi-teller** batch flushing the whole device queue under any one org teller's online session; `client_ref` echo-back for reconciliation; `set_status`/`set_prep_time` idempotency + WhatsApp-send de-dup; client timestamp on `force_close`; `POST /auth/refresh` + revocable `refresh_tokens` (optional smoothing). *(Dropped — the rejected hardening: Secure-Enclave/device key, `POST /devices` registration, signed-device-auth on `/sync/*`. The bundle is software-encrypted, not hardware-bound.)*
- **P2:** **change-feed cursor** (the orders `updated_after` is an unsafe offset — fix first), tombstones, idempotency on waste/transfer/stocktake, optimistic concurrency (`version`/`If-Match`) on shared rows (delivery status, delivery settings), SSE replay cursor (`Last-Event-ID`/`after_seq`) + Postgres `LISTEN/NOTIFY`, optional real push (FCM/APNs), document finalize/cancel **409-as-success**.

### R7. Corrections to the v1 body below
- **Inventory is server-authoritative; the POS holds NO local stock ledger.** v1 §4 (domain 5) and §8 mention "local stock-depletion math" / "provisional recipe depletion offline" — **wrong** per doc 05 §5. The POS only renders a recipe *preview*; depletion is server-side, atomic, idempotent on the order key. No provisional decrement, no stale stock counter. (Body corrected inline.)
- **Printing transport (v1 §10 risk #4) is RESOLVED** — see D10. (Body corrected inline.)

### R8. Still-open decisions
Offline-PIN silent-derive at first online login vs explicit enrollment (R3) · token TTL value · `price_flagged`: add a reconciliation report or drop the columns · SSE: global `change_log` vs per-table keyset · confirm no USB-only Star units · **verify removing the per-branch teller lock (D13) doesn't break dashboard/reporting that assumes branch-assignment as a gate** (it should only be management metadata, not authorization).

### R9. Menu/catalog module (post focused re-audit — doc `docs/audit/fe-menu-catalog.md`)
- **Decision: the POS consumes the server's branch-effective catalog and caches it (read-cache); it does NOT re-implement the §3 branch-override merge client-side.** The teller is bound to one branch, so the server's already-merged `/menu-items?branch_id=` + `/addon-items?branch_id=` snapshot is exactly the sellable list — mirror it. (The 3-table merge — `branch_menu_overrides` item price+availability, `branch_menu_size_overrides` per-(item,size) absolute price, `branch_addon_overrides` addon price+availability — is documented for parity and only needed if multi-branch-on-device ever arrives.) `rust-core::menu` still owns **translation resolution, `is_active`/availability/time-window filtering, `priceForSize`, and line pricing** (the money), which is why it sits beside `pricing`.
- **Confirmed rules to port exactly:** size `price_override` is **absolute** (not a delta), `base_price` fallback on null/unknown label; addon swap families (`milk_type`/`coffee_type`) charge only the positive delta over the item's default-milk base, others charge full `default_price`; bundle price is **org-global** (branches vary only availability + date/time window); translation fallback = locale→`en`→base field; `max_selections==null` ⇒ multi-select no cap; unknown `BundleStatus` ⇒ unavailable.
- **New fix-in-port items:** **F11** AddonSlot/OptionalField `*_translations` are dropped today — resolve them in the port. **F12** `GET /menu-items/{id}` is **not branch-aware** (returns org prices) — never refetch single items for pricing (resolve from the cached branch list), or make the endpoint branch-aware. The `cachedAt` bug (now pinned to two defects at `menu_notifier.dart:144,236`) folds into F10.

### R10. Device-binding (Secure Enclave) — DROPPED; the org bundle is KEPT (R3)
Only the **hardware hardening** is dropped: the `DeviceKeystore` signer/attest/seal, `POST /devices` registration, and signed-device-auth on `/sync/*`. The **org-scoped offline-auth bundle is KEPT** (R3) — but **software-encrypted at rest, not hardware-bound**, and argon2id (a memory-hard verifier matters *more* without an enclave). `TokenStore` (§7.2) stays for the access/refresh-token blob. **Cross-teller `/sync/replay` attribution is real** (the device legitimately holds many tellers' offline shifts) and is handled by an **org-validated client `teller_id` per shift**, authorized by any one org teller's online re-auth — not by a device credential.

### R11. Connectivity & auth UX rules (no auto-routing; never strand a working teller)
- **Mid-shift reconnect = silent background sync, shift stays open, no login.** When connectivity returns and the teller's cached token is still valid, the core drains the outbox + pulls deltas **silently under the cached token**; the open shift continues in its open state. The teller never leaves the order screen and is never asked to re-login. *(Confirmed correct flow.)*
- **Token-expired-while-working ≠ logout.** A 401 during background sync **parks sync** and the teller **keeps selling offline-first** (everything queues). The core surfaces a `NeedsReauth` status as a **non-blocking banner**, not a route change; re-auth happens at a convenient moment (or via optional Layer-2 refresh). A connectivity blip that would otherwise land them on `/login` must instead let them **continue offline-first without re-login (online or offline)**.
- **No automatic navigation on connectivity/auth/session events.** The host consults `app_route()` **only at deliberate transitions** (cold start, post-login, post-open-shift, post-close-shift, explicit sign-out). `CoreObserver.on_status_changed` drives **chrome only** — an order-screen banner/popup (online/offline · syncing · synced · needs-reauth) — **never a route**. This is the explicit fix for today's `_forceLogout`-on-401 → `/login` yank (`auth_notifier.dart`).
- Login is required only at genuine boundaries: **cold start with no valid local session**, or a **deliberate teller switch** (R3) — never as a side effect of connectivity.

### R12. Recipe usage & ingredient swaps (milk/coffee replacements) — verified
Flutter `computeRecipeLocally` (`recipe_api.dart:73-218`) computes a drink's ingredient usage: base recipe rows filtered by size → **`milk_type`/`coffee_type` addons SWAP the base ingredient of the matching category** (`milk`/`coffee_bean`) in place (replace name/unit/ingredient, tag `addon_swap:<name>`; if the picked addon IS the default ingredient → no swap) → non-swap addons **ADD** (× qty) → optionals add a row when mapped to an ingredient.
- **Recipe usage is SERVER-authoritative (unlike pricing).** Authoritative depletion runs in `create_order` → `orders/component_resolve.rs:188-267` with the same swap mapping; it snapshots `deductions` and deducts `branch_inventory` atomically (doc 05 §5). `computeRecipeLocally` is only the offline **preview** for the recipe/ingredients sheet. The order sends *selections*; the server derives usage — so an offline order computes its depletion at **replay time against the recipe as-of-then** (rare edge; acceptable).
- **⚠ Divergence (first audit missed it):** the server swap **converts the recipe quantity into the replacement ingredient's unit (g↔kg / ml↔l) via `units::convert`** before swapping (`component_resolve.rs:241-260` — "otherwise mis-deducted by up to 1000×"); the Flutter preview keeps the raw base quantity (`recipe_api.dart:162`). So **preview ≠ actual depletion when base and replacement units differ.** Swap pricing also differs in derivation (client uses `default_milk_addon_id`; server queries the base milk addon price) though both yield the clamped positive delta.
- **Decision:** the rust-core `recipe` module is the **single shared expansion implementation**, factored into a crate consumed by BOTH rust-core (client preview) **and** SufrixRust (`preview_recipe` + `create_order` depletion). Port `units::convert`. One implementation → preview always equals depletion, zero drift across all three call sites. (This is the upgraded F9.) Late, low-risk module (server owns authority), but spec the swap + unit-conversion faithfully.

---

## 1. Overview & the one rule

**THE ONE RULE: all logic lives in Rust. The UIs are glue.**

Every behavior that is not "draw a pixel" or "read a touch" belongs to the shared Rust core: HTTP, the offline SQLite mirror, the outbox + sync engine, pricing & cart math, recipe-driven stock depletion, receipt/Z-report rendering, printer transport, token custody, permission gating, money normalization, and defensive deserialization of a hostile wire format. The hosts render DTOs the core hands them, fire commands, and subscribe to a status stream. Nothing domain-shaped is implemented twice.

This is enforced structurally:

- **One binding boundary** — the `madar-core` crate's `#[uniffi::export]` surface. `uniffi-bindgen` generates the `.swift` and `.kt` bindings from that one crate in CI; no host ever hand-writes a binding or a wire model.
- **Hosts receive only curated view DTOs** with money pre-normalized to `i64` minor-units, `*_translations` pre-resolved to the device locale, open enums kept as `String`, and temp-id/idempotency/timestamps all owned by Rust. Hosts never see a raw OpenAPI wire type.
- **Three thin UIs, one error path** — a single coarse `CoreError` enum crosses the FFI so all three apps share one `catch`/`when`.

The payoff: a bug fix or a pricing-rule change ships once in Rust and lands on all three platforms in the same release train. The risk it trades for — surface drift between the apps and the core — is handled by FFI-surface versioning (§3, §7).

---

## 2. Monorepo layout

Monorepo root: `~/Desktop/madar-rebuild/`. The Rust scaffold already exists; the two app shells are empty and filled in Phase 3+.

```
madar-rebuild/
├── PLAN.md                         # this file — the canonical roadmap
├── rust-core/                      # the shared core (Cargo workspace)
│   ├── Cargo.toml                  # workspace: members=[madar-core], excludes madar-api until P2
│   ├── rust-toolchain.toml         # pinned 1.90.0 + iOS/Android/desktop cross targets
│   ├── .env / .env.example         # MADAR_BASE_URL, MADAR_ENV — baked at build via build.rs
│   ├── openapitools.json           # pins openapi-generator-cli 7.23.0
│   ├── tool/
│   │   └── generate_api.sh          # regenerates crates/madar-api from SufrixRust/openapi.json
│   └── crates/
│       ├── madar-api/             # GENERATED openapi-generator -g rust client (excluded from workspace until P2)
│       │   └── …                    # async reqwest, single-request-param structs, wire models
│       └── madar-core/            # THE crate the apps link. lib name `madar_core`.
│           ├── Cargo.toml          # crate-type = [lib, cdylib, staticlib]; uniffi 0.28 (proc-macro)
│           ├── build.rs            # bakes .env (base_url/env) via cargo:rustc-env
│           └── src/
│               ├── lib.rs          # uniffi::setup_scaffolding!(); MadarCore Object + exports
│               ├── bin/uniffi-bindgen.rs   # standalone bindgen used in library mode
│               ├── ffi/            # #[uniffi::export] surface: Records, Enums, callback traits (§7)
│               ├── domain/         # pricing, cart math, recipe depletion, outbox, sync engine
│               ├── store/          # rusqlite pool, migrations (refinery), mirror+outbox tables (§8)
│               ├── net/            # http wiring over madar-api, token refresh, idempotency headers
│               └── print/          # receipt/Z-report render + Star/Epson transport
├── swift-app/                      # thin SwiftUI host (iPhone + iPad). Links madar_core.xcframework.
│                                   #   SPM package consuming the CI-published binding artifact.
└── kotlin-app/                     # thin Compose host (Android phone/tablet + JVM desktop).
                                    #   Compose Multiplatform; links the JNA-loaded .so/.dylib/.dll + .kt bindings.
```

Three native artifacts come out of the **one** `madar-core` build (`crate-type = ["lib","cdylib","staticlib"]`):

| Artifact | Consumer | Form |
|---|---|---|
| `.xcframework` (staticlib) + `.swift` | swift-app | SPM binary target |
| `.so` / `.dylib` / `.dll` (cdylib) + `.kt` | kotlin-app | Maven `.aar` (Android) + `.jar` (desktop) |
| `lib` | Rust tests + `uniffi-bindgen` bin | internal |

---

## 3. Locked decisions

These are settled; everything below builds on them. (Most are already committed in the scaffold — noted inline.)

| # | Decision | Rationale / ground truth |
|---|---|---|
| D1 | **FFI = UniFFI 0.28, proc-macro mode** (`#[uniffi::export]`, no `.udl`). `uniffi::setup_scaffolding!()`. | Already in `madar-core/Cargo.toml`. One crate → Swift + Kotlin bindings via `uniffi-bindgen` in library mode. Async support + host-side cancellation since 0.28. |
| D2 | **Wire codegen = `openapi-generator -g rust` 7.23.0** into `crates/madar-api`, regenerated by `tool/generate_api.sh` from `SufrixRust/openapi.json` (3.1.0, **230 ops, 264 schemas**). Flags: `library=reqwest, supportAsync=true, useSingleRequestParameter=true, preferUnsignedInt=true, bestFitInt=true`. | Already wired. POS-side equivalent of Flutter's `tool/generate_api.sh` and the dashboard's `npm run generate:api`. The generated crate is **never exported across the FFI** — it is an internal transport detail. |
| D3 | **Local DB = SQLite via `rusqlite` + `r2d2_sqlite` pool**, WAL mode. Single writer (sync worker), many readers (UI). Migrations via **`refinery`** embedded forward-only SQL. | Offline-first mandate from `CLAUDE.md`. WAL gives snapshot-consistent UI reads while the sync worker writes. |
| D4 | **Async over FFI via embedded Tokio** (`#[uniffi::export(async_runtime = "tokio")]`). Network/write ops are `async` (→ Kotlin `suspend` / Swift `async`); pure cache reads are sync `fn`. Host-side cancellation drops the Rust future — **except outbox enqueues are non-cancellable past local commit.** | The host never manages threads. A committed sale cannot be lost to a cancelled UI task. |
| D5 | **Environment via Rust-side `.env`, baked at build time** (`build.rs` → `cargo:rustc-env`, `MADAR_BASE_URL`/`MADAR_ENV`). Hosts supply only device-local facts (data dir, platform tag, persisted token blob, locale). | Already in `.env.example`. Dev→staging→prod promotion is a Rust-build concern, identical across all three apps. A QA-only `env_override` exists but is `[Later]`. |
| D6 | **FFI-surface versioning = SemVer with a runtime handshake.** A monotonic `FfiVersion{major,minor}` is baked into the core and asserted at `MadarCore::new`. Host major < core major → refuse to run (force app update). Additive-only within a major. | Three apps must not drift against the core. Bindings are published as versioned artifacts tagged `ffi-vMAJOR.MINOR`; a host pins exactly one. (§7) |
| D7 | **Money = `i64` minor-units (piastres) at the FFI boundary, always.** All wire int32/int64 splits and any BigDecimal-as-string fields are normalized in `madar-core` before crossing. | Hosts format `*_minor` with `currency_code` from the session. See the BigDecimal reconciliation in §10. |
| D8 | **Toolchain pinned 1.90.0** with iOS (device+sim), Android (arm64/armv7/x86_64), and desktop (macOS+Linux) targets provisioned on checkout. | Already in `rust-toolchain.toml`. |

---

## 4. Domain map

The 230 operations group into the domains below. **POS-criticality** is about the *teller selling path*, not business importance. The four offline strategies are: **outbox** (durable local write, replayed in order), **read-cache** (mirrored to SQLite, served offline, refreshed by delta sync), **online-only** (pass-through; FFI returns `Offline` when disconnected, no mirror, no queue).

| # | Domain | Crit. | ~Ops | Offline strategy summary |
|---|---|---|---|---|
| 1 | **Identity & multi-tenant** (auth, users, permissions, orgs, branches) | critical | ~34 | `login`/`resolve-branch` **online-only** (no new session offline); `me`/`permissions`/own-branch/`get_org`/`timezones` **read-cache** (mirror at login → offline identity, tax_rate, currency, printer cfg, permission gating); all user/permission/org/branch CRUD **online-only**. JWT cached so a logged-in teller survives days offline. |
| 2 | **Menu / catalog & pricing** (menu-items, categories, addons, bundles, discounts, payment-methods, branch overrides) | critical | ~52 | The **read hot-path**: every selling GET (menu, categories, addons, `bundles/available`, discounts, payment-methods, branch/addon overrides) **read-cache**. Branch overrides layered over base catalog in Rust into one sellable list. All catalog/pricing **writes are online-only** back-office. |
| 3 | **Orders & delivery** (order capture / **the** hot path) | critical | ~42 | `create_order`, `void_order`, delivery `set_status`/`set_prep_time`/`finalize`/`cancel` → **outbox**. `list_orders`/`get_order`/`list_delivery_orders`/tables/zones → **read-cache**. All `/public/*`, `/otp/*`, QR images, channel overrides, SSE stream → **online-only** (customer surface, not teller). |
| 4 | **Shifts, reports & costing** | critical | ~36 | `open_shift`/`close_shift`/`add_cash_movement` → **outbox** (client-supplied id + real timestamps). `current_shift`/`get_shift`/`list_shifts`/cash-movements/`shift_report`/`shift_summary` → **read-cache** (print Z-reports offline). `force_close`/`delete_shift`, all `/reports/*` analytics & costing → **online-only**. |
| 5 | **Inventory & back-office** (inventory, purchasing, stocktakes, recipes, menu-advisor) | backoffice | ~58 | One outbox op: `create_waste` (teller logs spoilage mid-shift). `list_branch_stock`/`list_catalog`/`inventory_settings`/drink & addon recipes → **read-cache**; recipes drive an offline **recipe preview only** — the POS holds **NO local stock ledger**, depletion is server-side & idempotent on the order key (doc 05 §5). Everything else (transfers, suppliers, POs, stocktakes, advisor) **online-only**. |
| 6 | **Integrations & misc** (whatsapp, uploads, qr, tables, timezones, public) | backoffice | ~26 | Zero outbox. `timezones` + `branches/{id}/tables` → **read-cache** (dine-in labels offline). WhatsApp/uploads/QR/marketing-links/all `/public/*` → **online-only**. QR/WhatsApp-status return giant base64 data-URLs — **never cached.** |

> Note: integrations endpoints (QR, tables, timezones, public) overlap domains 1/3 by path-prefix; counted once. The ~248 row-count exceeds 230 because shared endpoints (tables/QR/timezones) appear in multiple domain analyses.

**The POS selling hot-path — the spine of Phase 1 — is exactly:**

```
login (online, once)
  → resolve identity/permissions/tax/currency  (read-cache)
  → open_shift                                 (outbox)
  → render sellable menu = catalog ⊕ overrides (read-cache, merged in Rust)
  → build cart: add/update/remove lines, discount, tender  (in-memory, sync, no network)
  → submit_order                              (outbox; resolves at LOCAL COMMIT, not server-ack)
  → print receipt                             (Rust, offline)
  → void_order if needed                      (outbox)
  → add_cash_movement                         (outbox)
  → close_shift + print Z-report              (outbox + read-cache)
```

Everything on this spine works fully offline. Delivery-ticket handling (`set_status`/`finalize`/etc.) and `create_waste` are the same outbox machinery, scheduled into Phase 4.

---

## 5. Phased roadmap

Eight phases (0–7). Each boundary is **shippable** — a real artifact a teller or engineer can run.

### Phase 0 — Foundations & spike (the scaffold, already begun)
**Goal:** prove the toolchain end-to-end with a trivial exported function before any domain logic.
**Deliverables:**
- Cargo workspace builds `madar-core` as `lib + cdylib + staticlib`; `uniffi-bindgen` emits `.swift` + `.kt` from a one-method `ffi_version()` export.
- `tool/generate_api.sh` runs `openapi-generator -g rust` against `SufrixRust/openapi.json` into `crates/madar-api` (excluded from the workspace; first run is *expected* not to fully compile — see §10/D2).
- `build.rs` bakes `MADAR_BASE_URL`/`MADAR_ENV` from `.env`.
- CI cross-compiles all eight targets; a "hello core" call returns `FfiVersion` from a SwiftUI stub and a Compose stub.

**Exit criteria:** a SwiftUI app on a device and a Compose app on an Android emulator + desktop window each print the core's `FfiVersion` and `runtime_info().base_url`. No domain logic yet.

### Phase 1 — Store, session, and the order hot-path (offline-capable MVP)
**Goal:** a teller can log in once online, then **sell, void, open/close shifts, and print receipts fully offline**, with everything queued and replayed.
**Deliverables:**
- `store/`: refinery migrations create the mirror tables for the hot-path read-cache (session, permissions, branches, org, timezones, categories, menu items/sizes/addon-slots, addon catalog, bundles, discounts, payment methods, orders, order_items, shifts, cash_movements, shift_reports), the durable **outbox**, `id_map`, `sync_cursors`. WAL + pragmas. (§8)
- `ffi/`: `MadarCore::new/shutdown/runtime_info`; `TokenStore` callback + `set_token_store`; `login/logout/is_authenticated/current_session/has_permission`; the cart lifecycle (`start_order/add_line/update_line_qty/remove_line/apply_discount/cart_totals/set_payment`); the outbox writes (`submit_order/void_order/open_shift/close_shift/add_cash_movement`); the read APIs (menu/categories/addon-catalog/bundles/discounts/payment-methods/orders/current_shift); `CoreObserver` + `set_observer/sync_status`; the full `CoreError` enum. (§7)
- `domain/`: the **pricing/cart engine** (subtotal/tax/discount/total) matching the server's math so offline totals equal the eventual server order; outbox enqueue + drain + temp-id reconciliation; defensive serde (open strings, `#[serde(other)]` closed enums, money→i64, translations→String).
- `net/`: token refresh on 401, client-generated `Idempotency-Key` on every outbox POST, delta-pull loop.
- `print/`: receipt + Z-report rendering offline from the mirror (transport stub — real printer in P4).
- Backend Phase A+B (idempotency store + change-feed shadow) deployed so the client has something to sync against (§9).

**Exit criteria:** airplane-mode demo — log in online, go offline, open a shift, ring up several orders with addons/discounts/tender, void one, close the shift, see correct Z-report totals; reconnect; watch the outbox drain in order, temp-ids reconcile to server ids/order-numbers, and the dashboard show the same orders. Kill the app mid-order → the draft cart and the queue survive.

### Phase 2 — Sync engine hardening & reactive read-through
**Goal:** make sync robust against days-offline backlogs, clock skew, and partial failures; make every read stale-while-revalidate.
**Deliverables:**
- Cursor/delta pull per stream (`/sync/changes?since=seq`), tombstones, server-authoritative timestamps + clock-skew recording.
- Batch replay (`POST /sync/replay`) with stop-on-dependency / continue-on-independent semantics; `depends_on_seq` + late temp-id substitution.
- `on_data_changed`/`DataChanged` events drive host re-queries; `madar-api` finally **wired into `madar-core`** (workspace `exclude` removed) with the BigDecimal-string post-processing from §10 completed.
- `refresh_session` manual button; `pending_outbox` listing; FFI-version artifact registry.

**Exit criteria:** simulate a 3-day offline backlog of 400+ mixed commands → one reconnect drains them in order with correct parent/child resolution; a single poisoned 4xx command goes dead-letter without freezing the queue; a wrong device clock does not mis-sequence the shift report.

### Phase 3 — Swift host (iPhone + iPad) to feature parity with the hot-path
**Goal:** a production-quality SwiftUI teller app on iPhone and iPad over the Phase-1/2 surface.
**Deliverables:** Login, Shift open/close, Catalog grid + Order entry, Cart, Payment, Receipt/Print, Orders list, Sync-status chrome, Settings/About. iPad uses a split layout (catalog + cart side-by-side); iPhone is a stacked flow. `TokenStore` backed by Keychain.
**Exit criteria:** an iPad and an iPhone run the full offline selling spine against staging; UX review passes; no business logic in Swift (audit: only layout + DTO rendering + command calls).

### Phase 4 — Compose host (Android phone/tablet + desktop) + printing + delivery + waste
**Goal:** bring up the second UI family and the remaining outbox features.
**Deliverables:** Compose Multiplatform Login/Shift/Catalog/Cart/Payment/Receipt/Orders/Sync/Settings for Android phone, Android tablet, and JVM desktop. `TokenStore` via Android Keystore / desktop OS secret store. **Real printer transport** (Star/Epson) wired into `print/` with a platform transport matrix. Delivery-ticket outbox (`set_status/set_prep_time/finalize/cancel`, `list_delivery_tickets`) + late-replay notification suppression. `create_waste` outbox + `log_waste`.
**Exit criteria:** an Android tablet and a desktop build run the full offline spine and print a real receipt + Z-report; delivery tickets advance offline and reconcile; the same binding artifact version is pinned by both app families.

### Phase 5 — Back-office passthrough (online-only admin)
**Goal:** expose the non-hot-path admin surface as thin async passthroughs where a teller-adjacent manager needs them, without polluting the hot-path FFI.
**Deliverables:** an `admin` FFI namespace of online-only async calls (force-close shift, basic catalog/branch reads, reports the POS chooses to surface). No mirror, no outbox; `Offline` error when disconnected. Reports/costing/menu-advisor remain dashboard-primary.
**Exit criteria:** managers can force-close a shift and view a sales summary from the POS while online; offline these cleanly show "requires connection."

### Phase 6 — Reliability, dead-letter UX, observability
**Goal:** production resilience.
**Deliverables:** `on_outbox_failed` + dead-letter management sheet (`pending_outbox`/retry-dead); `NeedsUpgrade` safe-degrade path; structured telemetry (sync lag, outbox depth, replay failures) routed through the core; idempotency-store TTL sweep verified against multi-day offline windows.
**Exit criteria:** chaos test (flapping connectivity, killed app, skewed clocks, server 5xx storms) leaves zero lost sales and no silent drops; every failure is visible to a manager.

### Phase 7 — Cutover & legacy decommission
**Goal:** retire the Flutter POS and the backend's per-table idempotency fallback.
**Deliverables:** backend Phase D (deprecate `?updated_after=`, drop per-table `idempotency_key` once telemetry shows zero legacy traffic); Flutter POS frozen then removed; `lib/core/models/pending_action.dart` outbox concept fully superseded by the Rust outbox.
**Exit criteria:** all field devices on the Rust-core apps; legacy endpoints dark; one source of truth for the teller path.

---

## 6. UI module breakdown (both hosts)

Every module below is **shared logic in Rust** with **per-platform layout only** in the host. The Rust column names the FFI calls a module binds to; the host columns describe layout deltas by form factor. SwiftUI = iPhone/iPad; Compose = Android phone/Android tablet/JVM desktop.

| Module | Shared logic (Rust FFI) | Phone layout | Tablet / Desktop layout |
|---|---|---|---|
| **Login** | `login`, `set_token_store`, `is_authenticated`, `current_session` | Single-column PIN pad / email form; org slug field | Centered card; desktop adds keyboard-first email mode |
| **Shift** | `open_shift`, `close_shift`, `current_shift`, `add_cash_movement`, `print_shift_report` | Full-screen open/close sheets; cash-movement modal | Persistent shift summary rail; Z-report preview pane |
| **Catalog / Order-entry** | `list_categories`, `list_menu_items` (merged sellable), `list_addon_catalog`, `available_bundles`, `start_order`, `add_line` | Category strip + scrolling item grid; tap → addon sheet | Catalog grid **alongside** the live cart (split view); larger grid density |
| **Cart** | `update_line_qty`, `remove_line`, `apply_discount`, `cart_totals` | Slide-up cart panel | Always-visible cart column with live totals |
| **Payment / Tender** | `list_payment_methods`, `set_payment`, `submit_order` | Full-screen tender flow | Inline tender within the cart column; change-due calc |
| **Receipt / Print** | `print_receipt`, `get_order` | Post-sale confirmation + reprint | Side preview + reprint; desktop print-dialog fallback |
| **Orders list** | `list_orders`, `get_order`, `void_order` | Stacked list → detail push; "syncing" chips on local rows | Master-detail split; filter rail |
| **Delivery tickets** (P4) | `list_delivery_tickets`, `set_status`, `set_prep_time`, `finalize`, `cancel` | Queue list → ticket detail | Kanban-style step columns (confirmed→preparing→ready→out) |
| **Waste log** (P4) | `log_waste` | Quick-entry modal from shift menu | Side panel during stock view |
| **Sync status** | `set_observer`, `sync_status`, `pending_outbox`, retry-dead | Compact status chip + pull-down badge | Persistent status bar + dead-letter sheet |
| **Settings / About** | `runtime_info`, `ffi_version`, `logout`, `refresh_session` | Standard settings list | Adds debug pane (base_url, schema version, cursors) |

**Shared-vs-platform rule of thumb:** if a screen contains a number, a price, a status string, a translation, a total, or a decision about what's sellable, that value was computed in Rust and the host only positions it. The host owns navigation chrome, gesture/layout, the Keychain/Keystore vault behind `TokenStore`, and hopping core callbacks onto the UI thread.

---

## 7. FFI surface v0 (the Phase 1–2 contract)

UniFFI proc-macro mode. One `MadarCore` Object (Arc, `Send+Sync`) owns the DB pool, HTTP client, token store, sync engine, and observer registry. **[P1]** ships in Phase 1; **[P2]** in Phase 2; **[Later]** post-MVP.

### 7.1 Handle, config, lifecycle — [P1]
```rust
#[derive(uniffi::Object)] pub struct MadarCore { /* opaque */ }

#[derive(uniffi::Record)]
pub struct CoreConfig {
    pub data_dir: String,                    // app-private writable dir for SQLite+WAL
    pub platform: HostPlatform,
    pub persisted_token_blob: Option<Vec<u8>>, // from Keychain/Keystore, None on fresh install
    pub locale: String,                      // BCP-47, for *_translations resolution
    pub env_override: Option<CoreEnv>,       // QA only; prod passes None → bundled .env wins  [Later]
}
#[derive(uniffi::Enum)] pub enum HostPlatform { Iphone, Ipad, AndroidPhone, AndroidTablet, Desktop }
#[derive(uniffi::Enum)] pub enum CoreEnv { Dev, Staging, Prod }

#[uniffi::export(async_runtime = "tokio")]
impl MadarCore {
    #[uniffi::constructor] pub async fn new(config: CoreConfig) -> Result<Arc<MadarCore>, CoreError>; // opens DB, migrates, loads token, asserts FFI major
    pub fn runtime_info(&self) -> RuntimeInfo;
    pub async fn shutdown(&self);
}
#[derive(uniffi::Record)] pub struct RuntimeInfo { pub base_url: String, pub env: CoreEnv, pub ffi_version: u32, pub core_build: String, pub db_schema_version: u32 }
#[uniffi::export] pub fn ffi_version() -> FfiVersion;
#[derive(uniffi::Record)] pub struct FfiVersion { pub major: u32, pub minor: u32 }
```

### 7.2 Auth & token custody — [P1] (manual `refresh_session` is [P2]; auto-refresh is [P1])
The **core owns the live session**; the host is a dumb secure-bytes vault. The core hands the host an **opaque encrypted `Vec<u8>` blob** to persist; token expiry/refresh/rotation stay in Rust.
```rust
#[uniffi::export(callback_interface)]
pub trait TokenStore: Send + Sync { fn save_blob(&self, blob: Vec<u8>); fn clear_blob(&self); }

#[uniffi::export(async_runtime = "tokio")]
impl MadarCore {
    pub fn set_token_store(&self, store: Box<dyn TokenStore>);
    pub async fn login(&self, req: LoginRequest) -> Result<SessionSnapshot, CoreError>; // online-only; mirrors me+perms+branch+org+tz
    pub fn is_authenticated(&self) -> bool;
    pub fn current_session(&self) -> Option<SessionSnapshot>;          // cached, never network
    pub fn has_permission(&self, resource: String, action: String) -> bool; // opaque strings, offline-safe
    pub async fn refresh_session(&self) -> Result<SessionSnapshot, CoreError>; // [P2] manual button
    pub async fn logout(&self, wipe_outbox: bool) -> Result<(), CoreError>; // preserves outbox unless wipe_outbox
}
#[derive(uniffi::Record)] pub struct LoginRequest { pub mode: LoginMode, pub org_slug: Option<String>, pub email: Option<String>, pub password: Option<String>, pub pin: Option<String> }
#[derive(uniffi::Enum)] pub enum LoginMode { Pin, Email }   // PIN xor email enforced in Rust, not the all-Option wire
#[derive(uniffi::Record)]
pub struct SessionSnapshot { pub user_id: String, pub display_name: String, pub role: String, pub org_id: Option<String>, pub branch_id: Option<String>, pub currency_code: String, pub tax_rate: f64, pub permissions_loaded: bool }
```

### 7.3 Reads (sync, cached DTOs) — [P1] (`list_delivery_tickets` [P2])
Reads serve SQLite, layer branch/channel overrides over base catalog in Rust, and pre-resolve `*_translations` to a `String`. All money is `i64` minor-units.
```rust
#[uniffi::export]
impl MadarCore {
    pub fn list_menu_items(&self) -> Vec<MenuItemView>;       // base ∪ branch overrides, soft-deletes filtered
    pub fn list_categories(&self) -> Vec<CategoryView>;
    pub fn list_addon_catalog(&self) -> Vec<AddonItemView>;
    pub fn available_bundles(&self) -> Vec<BundleView>;
    pub fn list_discounts(&self) -> Vec<DiscountView>;
    pub fn list_payment_methods(&self) -> Vec<PaymentMethodView>;
    pub fn list_orders(&self, filter: OrderFilter) -> Vec<OrderSummaryView>;
    pub fn get_order(&self, order_id: String) -> Option<OrderDetailView>;
    pub fn current_shift(&self, branch_id: String) -> Option<ShiftView>;
    pub fn list_delivery_tickets(&self) -> Vec<DeliveryTicketView>; // [P2]
}
#[derive(uniffi::Record)]
pub struct MenuItemView { pub id: String, pub name: String, pub category_id: Option<String>, pub base_price_minor: i64, pub image_url: Option<String>, pub is_available: bool, pub sizes: Vec<ItemSizeView>, pub addon_slots: Vec<AddonSlotView> }
```

### 7.4 Commands — cart (sync, in-memory) + outbox writes (async, resolve at local commit) — [P1]
The cart is **core-owned and journaled** (survives app kill). `submit_order` is the single outbox write and returns at **local commit**, not server-ack.
```rust
#[uniffi::export(async_runtime = "tokio")]
impl MadarCore {
    // cart — synchronous, no network
    pub fn start_order(&self, branch_id: String, shift_id: String, order_type: String) -> CartView;
    pub fn add_line(&self, cart_id: String, line: NewCartLine) -> Result<CartView, CoreError>; // menu_item_id XOR bundle_id (Rust-enforced)
    pub fn update_line_qty(&self, cart_id: String, line_id: String, qty: i32) -> Result<CartView, CoreError>;
    pub fn remove_line(&self, cart_id: String, line_id: String) -> Result<CartView, CoreError>;
    pub fn apply_discount(&self, cart_id: String, discount_id: Option<String>) -> Result<CartView, CoreError>;
    pub fn cart_totals(&self, cart_id: String) -> CartTotals;          // same engine as the server
    pub fn set_payment(&self, cart_id: String, tender: TenderInput) -> Result<CartView, CoreError>;
    // outbox writes — async, resolve at LOCAL COMMIT
    pub async fn submit_order(&self, cart_id: String) -> Result<OrderDetailView, CoreError>; // stamps created_at + idempotency key
    pub async fn void_order(&self, order_id: String, req: VoidOrderRequest) -> Result<(), CoreError>;
    pub async fn open_shift(&self, branch_id: String, req: OpenShiftRequest) -> Result<ShiftView, CoreError>;   // client id + opened_at
    pub async fn close_shift(&self, shift_id: String, req: CloseShiftRequest) -> Result<ShiftReportView, CoreError>; // closed_at client-stamped; expected_cash from cached Z-report
    pub async fn add_cash_movement(&self, shift_id: String, req: CashMovementInput) -> Result<(), CoreError>;
    pub async fn log_waste(&self, branch_id: String, req: WasteInput) -> Result<(), CoreError>;          // [P2]
    pub async fn print_receipt(&self, order_id: String) -> Result<(), CoreError>;                        // [P2]
    pub async fn print_shift_report(&self, shift_id: String) -> Result<(), CoreError>;                   // [P2]
    pub async fn sync_now(&self) -> Result<SyncReport, CoreError>;     // forced sync; cancellable
}
#[derive(uniffi::Record)] pub struct CartTotals { pub subtotal_minor: i64, pub discount_minor: i64, pub tax_minor: i64, pub total_minor: i64, pub currency_code: String }
```
Delivery-ticket commands (`set_status`/`set_prep_time`/`finalize`/`cancel`) join this set in **[P2]/P4**. All back-office CRUD is **[Later]** in a separate `admin` namespace (Phase 5).

### 7.5 Reactive status (observers) — [P1] (`on_outbox_failed`/`pending_outbox` [P2])
Event-as-signal, not event-as-payload: the host re-pulls the named read on each event. The core probes connectivity **internally** (the host does not report it).
```rust
#[uniffi::export(callback_interface)]
pub trait CoreObserver: Send + Sync {
    fn on_status_changed(&self, status: SyncStatus);
    fn on_data_changed(&self, domains: Vec<DataDomain>);
    fn on_outbox_failed(&self, item: OutboxFailure);   // [P2]
}
#[derive(uniffi::Enum)] pub enum DataDomain { Menu, Orders, Shift, DeliveryTickets, Permissions, Session }
#[derive(uniffi::Record)] pub struct SyncStatus { pub connectivity: Connectivity, pub pending_outbox: u32, pub last_sync_at: Option<i64>, pub syncing: bool, pub failed_outbox: u32 }
#[derive(uniffi::Enum)] pub enum Connectivity { Online, Offline, Reconnecting }

#[uniffi::export]
impl MadarCore {
    pub fn set_observer(&self, observer: Box<dyn CoreObserver>) -> SyncStatus; // returns current snapshot for cold start
    pub fn sync_status(&self) -> SyncStatus;
    pub fn pending_outbox(&self) -> Vec<OutboxItemView>; // [P2]
}
```

### 7.6 Error model — [P1]
**One coarse, host-actionable `CoreError`** — the variant tells the host how to *react*; rich diagnostics ride as fields. All wire quirks (open enums, untyped arrays, int splits, PATCH skip-if-none, multipart) are absorbed *below* this boundary.
```rust
#[derive(uniffi::Error, Debug, thiserror::Error)]
pub enum CoreError {
    #[error("offline: {message}")]            Offline { message: String },          // online-only op while disconnected; hot-path commands NEVER return this
    #[error("auth required: {message}")]      Unauthenticated { message: String },  // 401 + refresh failed → login screen
    #[error("forbidden: {resource}/{action}")] Forbidden { resource: String, action: String },
    #[error("invalid: {field}: {message}")]   Validation { field: String, message: String }, // mode invariants, empty cart, future-dated cash, note-required-when-reason-other
    #[error("server {status}: {code}")]       Server { status: u16, code: String, message: String },
    #[error("transient: {message}")]          Transient { message: String },        // 5xx/timeout; sync already retries
    #[error("internal: {message}")]           Internal { message: String },         // store/migration/serde; also FFI-version-too-old
}
```

### 7.7 FFI versioning (lockstep)
`uniffi-bindgen` emits the one true `.swift`/`.kt` in CI, published as `ffi-vMAJOR.MINOR` artifacts; hosts pin one. `MadarCore::new` asserts: host major < core major → `Internal{"ffi version too old, update app"}` and refuse; host minor < core minor → allowed (additive). Within a major, **only add** methods / `Option` fields / enum variants (with a host default arm). Any rename/removal/type-change ⇒ major bump ⇒ all three apps rebuild in one release train.

---

## 8. Local store & sync (condensed)

**Crate:** `madar-core/store/` — one embedded SQLite DB (rusqlite + r2d2 pool), the **source of truth the UI reads from**, online or offline.

**Pragmas (every pooled conn):** `journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON`, `busy_timeout=5000`. Single writer (sync worker) + many snapshot-consistent UI readers.

**Mirror tables (read-cache):** two-layer per entity — extracted **indexable columns** (id, branch_id, availability, soft-delete, sort keys, money/qty needed for merge) + a **`payload BLOB`** holding the full canonical wire JSON + bookkeeping (`server_seq`, `server_updated_at`, `deleted` tombstone, `synced_at`). Additive wire fields flow through `payload` with **no migration**; a migration is needed only when a *new column must be indexed/filtered*. A SQL view `v_sellable_menu` layers branch overrides (price + availability) over base items with tombstones filtered, so the host gets one flat sellable list.

**Durable outbox** (`outbox` table, append-only, `seq` PK = global FIFO): one row per outbox-mutation with its own `id`, `client_temp_id`, `op_type`, unique `idempotency_key`, wire `payload`, `event_at` (client real-event time) vs `enqueued_at` vs `server_acked_at`, `status` (pending→inflight→acked / failed→dead / superseded), `attempts`/`next_attempt_at` backoff, `last_http_status` (4xx terminal vs 5xx retry), `server_id`/`server_number` after ack, and `depends_on_seq` for cross-entity ordering. Companion **`id_map`** (entity_type, client_temp_id ↔ server_id) is the durable temp-id bridge. **`sync_cursors`** (one row per stream) holds `last_server_seq` for days-offline catch-up.

**Optimistic local rows:** mirror tables carry `origin('server'|'local')`, `local_state('pending'|'synced'|'rejected')`, `client_temp_id`. Enqueue writes **three things in one txn** — optimistic mirror row + `id_map` placeholder + outbox command — so a new order shows on the shift screen instantly and survives process death.

**Migrations:** `refinery` embedded forward-only `.sql`, run in a txn on init **before** the pool is exposed. Defensive serde is mandatory crate-wide: closed enums → `#[serde(other)] Unknown`; open strings (`order_type`, `status`, `dtype`, `movement_type`, …) stay `String`; PATCH bodies → `skip_serializing_if=None` (absent-vs-null trap); `*_translations`/`revenue_by_method` → `serde_json::Value`. A DB-newer-than-binary mismatch triggers a **safe degrade**: refuse drains, force full resync, emit `NeedsUpgrade`.

**Read-through:** UI reads hit SQLite only and **always succeed offline**; the sync worker refreshes mirrors via delta pull and emits `DataChanged{stream}` → host re-queries (stale-while-revalidate). Online-only reads return `Offline` when disconnected.

**Outbox lifecycle:** single drain worker, `ORDER BY seq ASC` (never reorder), sends `X-Idempotency-Key` on every retry. 2xx → ack + reconcile (`id_map`, promote optimistic row, rewrite child FKs, fold server `warnings[]`); 409/duplicate-200 → treat as ack; 4xx → `dead` + `OutboxRejected` (dependents cascade to needs-attention, never orphaned); 5xx/timeout → backoff. **Late temp-id substitution at send time** swaps resolved parent ids into child payloads; `depends_on_seq` gates dependents until the parent acks.

**Delta pull:** per-stream `?since=seq` change feed (not wall-clock) → upsert by higher `server_seq`, deletes arrive as tombstones, server time is authoritative (records `server_time_skew_ms`; client time only stamps offline event times, corrected on ack). **Drain before pull** on reconnect so the feed already reflects the teller's own writes (no flicker).

**Conflict strategy:** teller-path entities (orders/void/status, shifts/cash, waste) are **append-only & conflict-free by construction** (unique client ids, forward-only status, frozen unit prices) — no merge. Dashboard-owned catalog/pricing/identity is **LWW by `server_seq`** (POS read-only). Branch stock is **fully server-authoritative** — the POS holds **NO local stock ledger** and applies **NO provisional decrement**; depletion runs server-side, atomically, idempotent on the order key (doc 05 §5). The POS only renders a recipe *preview* and pushes the orders/waste that deplete. The one true merge case — **stocktakes** — is deliberately kept **online-only**.

**Status events to the host** flow through the `CoreObserver` callback (§7.5): `Connectivity`, drain progress, `OutboxAcked`/`IdReconciled` (flip the "syncing" chip, swap temp id → order_number), `OutboxRejected` (dead-letter banner), `DataChanged` (re-query), `ClockSkew`, `NeedsUpgrade`. Pull-style getters back the dead-letter/sync-health screen.

---

## 9. Backend offline-first changes (checklist)

Backend = `SufrixRust` (Actix-Web). Everything is **additive**; no field removed, no existing status code changed. (Several pieces already exist — reuse, don't rebuild.)

**Already exists — do not rebuild:** header-idempotency on `create_order` (`orders.idempotency_key` + partial unique idx) and `create_delivery_order` (`uq_delivery_orders_idem`); client-supplied PK replay on `open_shift`; client event timestamps on order/shift/cash/void; 5-min future-skew guard (`clock.rs::reject_if_future`); `updated_after`+`include_items` on `list_orders`.

**To build:**

- [ ] **Central idempotency store** — migration `idempotency_keys(org_id, key, endpoint, request_hash, status_code, response_body, target_id, completed_at)`, PK `(org_id,key,endpoint)`, **30-day TTL** (offline windows are days). Middleware: miss → claim+run-in-txn+store outcome; hit+match → replay stored 2xx verbatim + `Idempotency-Replayed: true`; hit+body-mismatch → **422 `IdempotencyKeyReuse`**; hit+in-flight → **409 `IdempotencyInFlight`** + `Retry-After`.
- [ ] **Apply `Idempotency-Key` to every outbox-mutation** that lacks it: `void_order`, `set_status`, `set_prep_time`, `finalize_delivery_order`, `cancel_delivery_order`, `close_shift`, `add_cash_movement`, `create_waste`; migrate `create_order`/`create_delivery_order` to the central store (keep per-table columns during dual-write). Online-only admin POSTs (`create_org/user/branch`, `assign_branch`, `complete_onboarding`, catalog creates, `create_zone/table`) also accept the header (lower priority; not in the batch).
- [ ] **Late-replay side-effect suppression** — `set_status`/`finalize` skip the WhatsApp/customer-tracking send when the client `created_at` is stale (> ~6h) or the order already advanced past the target step; persist the state change regardless.
- [ ] **Client temp-id reconciliation** — add nullable `client_ref uuid` to `orders`/`shifts`/`shift_cash_movements`/`delivery_orders`/waste; every outbox-mutation echoes `client_ref` + server `id`/`order_number`/`order_ref` in its response. Prefer **client-PK passthrough** (shifts already use client UUID as PK → children referencing it are valid with no rewrite); **ref-rewrite** only for server-minted PKs (`finalize` → order).
- [ ] **Change feed** — migration `change_log(seq DEFAULT nextval('change_seq'), org_id, branch_id, domain, entity_id, op('upsert'|'delete'), version, changed_at)` populated by write-path helpers (not triggers, to keep ordering inside the idempotency txn). `GET /sync/changes?since=<seq>&domains=…&branch_id=…&limit=500` → `{changes[], next_since, has_more}`, strictly `WHERE seq > $since ORDER BY seq LIMIT`, tombstones carry `entity:null`. Domains = exactly the read-cache set (catalog/pricing/payment-methods/overrides/orders/shifts/cash/shift-report/branch-stock/catalog/recipes/tables/zones/delivery-settings/timezones). Excludes reports/costing/advisor/movement-ledger/PO+stocktake detail.
- [ ] **Batch replay** — `POST /sync/replay` (batch-level `Idempotency-Key`), ordered `items[]` of `{seq, op, idempotency_key, client_ref, path, body}`; each item in its **own txn**, per-item idempotency reused; **stop-on-dependency, continue-on-independent** (failed item → its `client_ref` consumers `skipped`, independents apply); per-item `results[]` with `applied|failed|skipped` + `http_status` + echoed ids; envelope is `200` whenever processed; `items.length ≤ 500` (paginate larger backlogs).
- [ ] **Versioning** — `version bigint DEFAULT 1` on mirrored mutable entities, bumped in the same write, copied into `change_log`. `If-Match`/`ETag` optimistic concurrency only on genuinely shared rows (delivery-ticket status, branch delivery settings) → **412 `PreconditionFailed`**; pure outbox creates need none.
- [ ] **`errors.rs`** — add `IdempotencyKeyReuse→422`, `IdempotencyInFlight→409`, `PreconditionFailed→412` (`422`/`412` are new mappings).
- [ ] **Rollout** (no break to current Flutter POS or dashboard): A dual-write idempotency → B change-feed shadow + ship `/sync/*` routes → C client cutover → D deprecate `?updated_after=` and drop per-table fallback once telemetry shows zero legacy traffic. Regenerate `openapi.json` (utoipa) **only when stable**, then run `tool/generate_api.sh` (POS) / `npm run generate:api` (dashboard). New envelope enums (`op`, item `status`) ship as **open strings with `#[serde(other)]`**, money stays int minor-units, new optionals `Option<T>`.

**Files touched:** new `src/sync/mod.rs` + 3 migrations; edit `src/errors.rs`, `src/main.rs` (routes + TTL sweep), `src/orders/handlers.rs`, `src/shifts/handlers.rs`, `src/delivery/staff.rs`, `src/inventory/*`, plus a `change_log` append in each mutating handler.

---

## 10. Risks & open questions

1. **BigDecimal-as-JSON-string — RESOLVED CONTRADICTION, treat as REAL.** The domain-inventory artifacts repeatedly assert the BigDecimal-string quirk "does not appear." **The actual scaffold contradicts them:** `rust-core/tool/generate_api.sh` step 3 explicitly states *"the backend serializes BigDecimal columns as JSON STRINGS (`current_stock`, `reorder_threshold`, prices, `quantity_used`, …) but the generator types them as `f64`,"* and defers string-tolerant fixups to Phase 2. **Decision:** trust the scaffold. The generator (`preferUnsignedInt`/`bestFitInt`) handles the int32/int64 *minor-units* money split fine, but **inventory/recipe/stock decimal fields that arrive as strings must get `serde_with` string-tolerant deserialization** during the Phase-2 wire-in (`net/`), normalized to `i64` minor-units (money) or `f64` (quantities) before crossing the FFI. This is the single most likely silent-corruption bug; the CI round-trip test (deser sample → reserialize from `payload`) must cover these fields. *Open question: enumerate the exact string-typed decimal fields from a live spec dump before P2 — the four artifacts disagree with the scaffold on which they are.*
2. **First codegen run does not fully compile (expected).** `generate_api.sh` step 4 already anticipates this. `madar-api` stays **excluded from the workspace** through Phase 1 so the foundation builds; wiring it in is a gated Phase-2 task with the decimal fixups above. Risk: the generator mis-emits the 3.1 `oneOf:[{type:null},$ref]` pattern (`OrderFull.delivery`, `printer_brand`), `allOf` flattening (`MenuItemFull`/`BundleWithComponents`/`PurchaseOrderFull`), untyped arrays (`addons`/`bundle_components`→`Vec<Value>`), and multipart (`create_org`/`upload_*_image`). **Mitigation:** the FFI never exposes raw wire types — every hostile shape is re-mapped to a hand-written view DTO; multipart endpoints are verified by hand.
3. **Android SDK / build-machine readiness.** `kotlin-app/` is empty and the SDK/emulator may not be installed on the build machine yet. Compose Multiplatform targeting Android **and** JVM desktop from one module is non-trivial. **Mitigation:** Phase 0 stands up the Compose stub + JNA native-lib loading before Phase 4 domain work; provision Android SDK + NDK and the cross-compile linkers in CI early. iOS toolchain is already pinned (`rust-toolchain.toml`).
4. **Printing transport — RESOLVED (see Revision 2 / D10).** Fleet is Epson (ESC/POS raster) + Star **TSP100** (raster-only), all LAN — both driven over **raw TCP :9100 from Rust**, identical on every platform incl. desktop; no native SDK seam. The only real cost is the Rust rasterizer (layout → 1-bpp bitmap + Arabic shaping). Residual: confirm no **USB-only TSP143U** units (USB would need a native transport and fails on iOS).
5. **No server `Idempotency-Key` contract today.** Until backend Phase A lands, exactly-once leans on **client-id-as-dedup** (client-supplied PKs + duplicate-200/409-as-ack). The header is sent forward-compatibly so it activates for free. Risk window: "server committed, ack lost, client retries with a server-minted-PK endpoint" (`finalize`) — mitigated by the `client_ref` echo + change-feed re-keying.
6. **Stale side-effects on late replay.** A `set_status`/`finalize` replayed hours later could fire a stale "order ready" WhatsApp. Backend suppression (§9) is the fix, but until it ships the client must locally suppress re-toasting on late acks. **Open question:** is a 6h staleness threshold right for this business, or should it be branch-configurable?
7. **Concurrent offline shift-open on two devices.** Two tellers opening a shift for the same branch offline both enqueue `open_shift`; the server enforces one-open-shift, the loser gets a conflict. **Mitigation:** loser's command → `dead`, surface to manager, re-home its orders under the winning shift via `id_map` remap. **Open question:** auto-remap vs manager-confirmed remap?
8. **FFI cancellation semantics on outbox writes.** Outbox enqueues resolve at *local commit* and are intentionally non-cancellable past that point. Hosts must understand a cancelled `submit_order` Task may still have committed the sale. Documented in §3/D4 and the binding docs, but it is a footgun if a host treats cancellation as rollback.
9. **`tax_rate` is `f64` (JSON number), not a decimal string** — confirmed real for the auth/me path and kept as `f64` in `SessionSnapshot`. This is the one place the "no decimal string" claim holds; do not over-apply the string-tolerant fix from risk #1 here.
