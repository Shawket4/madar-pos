Confirmed: no Rust/UniFFI scaffold exists yet (pure Flutter + generated Dart `sufrix_api`), and the API generation pattern (`tool/generate_api.sh`) and offline strategy match the brief. This is a greenfield FFI design. Here is the PLAN.md section.

---

# Sufrix POS Rust Core — UniFFI FFI Surface (v0)

> **Status legend:** **[P1]** ships in Phase 1 (login + order hot-path + shift open/close + outbox). **[P2]** Phase 2 (full read-cache mirror, reactive observers, delivery tickets). **[Later]** post-MVP / back-office, not on the teller path. Anything unmarked is the stable contract that all three hosts (SwiftUI, Compose-Android, Compose-Desktop) bind to.
>
> **Naming convention:** all generated wire structs live in the `sufrix_api` crate (Rust port of the OpenAPI spec, regenerated via `./tool/generate_api.sh`). The FFI crate (`sufrix_ffi`) re-exports a **curated subset** of those as `Record`s and defines **hand-written view DTOs** where the wire shape is hostile (allOf flattening, untyped `*_translations`, int32/int64 money splits). Hosts never see raw wire types they shouldn't.

---

## 0. Crate layout & where the boundary sits

```
sufrix_core/                      (Rust workspace)
├── sufrix_api/        # generated wire models + raw HTTP client (Orval-equivalent). NEVER exported directly.
├── sufrix_domain/     # business logic: pricing, cart math, outbox, sync engine, recipe depletion.
├── sufrix_store/      # SQLite (sqlite via rusqlite/sqlx) read-mirror + outbox tables.
└── sufrix_ffi/        # #[uniffi::export] surface. THIS is the binding boundary. Thin.
```

- **Everything** the prompt lists as logic — API calls, offline store, sync, printing, pricing, recipe depletion — lives below `sufrix_ffi`. The hosts are glue: render DTOs, fire commands, subscribe to observers.
- UniFFI is used in **proc-macro mode** (`#[uniffi::export]`, `#[derive(uniffi::Record/Enum/Object)]`), not a `.udl` file. The library exposes `uniffi::setup_scaffolding!()`.

---

## 1. Top-level handle: `SufrixCore`

A single long-lived `Object` (Arc-wrapped, `Send + Sync`) owns the DB pool, HTTP client, token store, sync engine, and observer registry. Hosts create exactly one and keep it for app lifetime.

**Config / base-URL / env come from a Rust-side `.env` compiled/loaded by the core — NOT injected by the host.** The host only supplies *device-local* facts the core cannot know (writable data dir, platform tag, persisted token blob). This keeps environment promotion (dev→staging→prod) a Rust-build concern, identical across all three apps.

```rust
/// Built once at app launch and held for the process lifetime.
#[derive(uniffi::Object)]
pub struct SufrixCore { /* opaque: db pool, http, token store, sync engine, observers */ }

/// Host-supplied, device-local only. Base URL / env are NOT here — they come from
/// the core's bundled .env (compile-time `env!` + runtime dotenv override file).
#[derive(uniffi::Record)]
pub struct CoreConfig {
    /// Absolute path to an app-private, writable dir for the SQLite file + WAL.
    pub data_dir: String,
    /// Platform tag for diagnostics/telemetry & printer backend selection.
    pub platform: HostPlatform,
    /// Opaque token blob previously persisted by the host (Keychain/Keystore),
    /// or None on a fresh install. See §2.
    pub persisted_token_blob: Option<Vec<u8>>,
    /// BCP-47 UI locale so the core can resolve `*_translations` server-side fallbacks.
    pub locale: String,
    /// Optional override of the .env-selected environment, for QA builds only.
    /// Production hosts pass None and get whatever the bundled .env selected.
    pub env_override: Option<CoreEnv>,
}

#[derive(uniffi::Enum)]
pub enum HostPlatform { Iphone, Ipad, AndroidPhone, AndroidTablet, Desktop }

#[derive(uniffi::Enum)]
pub enum CoreEnv { Dev, Staging, Prod }

#[uniffi::export(async_runtime = "tokio")]
impl SufrixCore {
    /// Opens the DB, runs migrations, loads the token blob, starts the sync engine paused.
    /// Async because it touches disk + runs migrations; fast-fails on a corrupt store.
    #[uniffi::constructor]
    pub async fn new(config: CoreConfig) -> Result<Arc<SufrixCore>, CoreError>;

    /// Effective config the core resolved (base_url, env) — for the "About/Debug" screen.
    pub fn runtime_info(&self) -> RuntimeInfo;

    /// Graceful shutdown: flush WAL, stop sync tasks. Host calls on terminate.
    pub async fn shutdown(&self);
}

#[derive(uniffi::Record)]
pub struct RuntimeInfo {
    pub base_url: String,        // resolved from .env, shown read-only
    pub env: CoreEnv,
    pub ffi_version: u32,        // see §7
    pub core_build: String,      // git sha
    pub db_schema_version: u32,
}
```

**[P1]** `new`, `shutdown`, `runtime_info`. `env_override` is **[Later]** (QA convenience).

---

## 2. Auth & token custody

**The Rust core owns the live session** (in-memory JWT/refresh + decoded claims). The host owns **durable secret storage only** (iOS Keychain / Android Keystore / desktop OS secret store). The core never asks the host to parse the token; it hands back an **opaque `Vec<u8>` blob** and asks the host to persist/return it. This makes the host a dumb secure-bytes vault and keeps all token semantics (expiry, refresh, rotation) in Rust.

Two host-implemented callback interfaces wire the secure store to the core:

```rust
/// Implemented by the HOST. The core calls these to persist/clear the opaque
/// session blob in Keychain/Keystore. The blob is encrypted-at-rest by the core
/// before it ever leaves Rust, so the host treats it as opaque secret bytes.
#[uniffi::export(callback_interface)]
pub trait TokenStore: Send + Sync {
    /// Persist the latest session blob (after login or a successful refresh).
    fn save_blob(&self, blob: Vec<u8>);
    /// Wipe on logout / 401-hard-expiry.
    fn clear_blob(&self);
}
```

The host registers its `TokenStore` once; thereafter token rotation is invisible to UI code.

```rust
#[uniffi::export(async_runtime = "tokio")]
impl SufrixCore {
    /// Host registers its secure-store callback exactly once, right after `new`.
    pub fn set_token_store(&self, store: Box<dyn TokenStore>);

    /// Dual-mode PIN/email login (/auth/login). Online-only — a brand-new session
    /// cannot be established offline. On success the core: stores the live session,
    /// calls TokenStore.save_blob, mirrors /auth/me + /auth/permissions + own branch
    /// + org + timezones into SQLite, then emits SessionState=Authenticated.
    pub async fn login(&self, req: LoginRequest) -> Result<SessionSnapshot, CoreError>;

    /// True when a previously-persisted blob restored a usable session at startup.
    /// An already-logged-in teller survives days offline without hitting /auth/login.
    pub fn is_authenticated(&self) -> bool;

    /// Cached identity for offline session restore (/auth/me mirror). Never hits network.
    pub fn current_session(&self) -> Option<SessionSnapshot>;

    /// Cached effective permission gate (/auth/permissions mirror). resource+action
    /// are opaque strings (server-side they are free-form). Offline-safe.
    pub fn has_permission(&self, resource: String, action: String) -> bool;

    /// Refresh is automatic & internal (the core refreshes on 401 before retry and on a
    /// timer). Exposed only for a manual "re-sync session" button. Online-only.
    pub async fn refresh_session(&self) -> Result<SessionSnapshot, CoreError>;

    /// Clears live session, calls TokenStore.clear_blob, and (optionally) wipes the
    /// read-mirror. Pending outbox is preserved unless `wipe_outbox` is true.
    pub async fn logout(&self, wipe_outbox: bool) -> Result<(), CoreError>;
}

#[derive(uniffi::Record)]
pub struct LoginRequest {
    /// Mode invariants (PIN xor email/password) are enforced in Rust, not by the
    /// generated all-Option wire struct. PIN matches ^[0-9]{4,6}$.
    pub mode: LoginMode,
    pub org_slug: Option<String>,
    pub email: Option<String>,
    pub password: Option<String>,
    pub pin: Option<String>,
}

#[derive(uniffi::Enum)]
pub enum LoginMode { Pin, Email }

/// Hand-rolled view DTO over MeResponse — tax_rate is a real f64 here (JSON number,
/// not a decimal string), currency_code required. role is a string with an
/// unknown-default so a future server role never breaks the host.
#[derive(uniffi::Record)]
pub struct SessionSnapshot {
    pub user_id: String,
    pub display_name: String,
    pub role: String,              // open string; UI gates on permissions, not role
    pub org_id: Option<String>,
    pub branch_id: Option<String>,
    pub currency_code: String,
    pub tax_rate: f64,
    pub permissions_loaded: bool,  // false if running on a stale offline cache
}
```

**[P1]** `set_token_store`, `login`, `logout`, `is_authenticated`, `current_session`, `has_permission`. `refresh_session` manual button is **[P2]** (automatic refresh is **[P1]**, just not host-visible).

---

## 3. Async model & cancellation

- **Anything that may touch the network or a write transaction is `async fn`.** UniFFI maps these to `suspend` (Kotlin) and `async`/`await` (Swift), driven by the core's embedded Tokio runtime (`async_runtime = "tokio"`). The host never manages threads.
- **Pure cache reads are synchronous** (`fn`, no `async`). They hit SQLite on the calling thread via a connection pool and return immediately — list_menu_items, has_permission, current_session, totals previews. Keeping them sync avoids forcing the UI into a coroutine for a grid render.
- **Cancellation:** UniFFI async export futures are cancellable from the host side as of UniFFI ≥ 0.28 — dropping the Swift `Task` / cancelling the Kotlin `Job` drops the Rust future. The core's network and DB ops are cancel-safe (Tokio `select!` against an internal `CancellationToken`; a dropped future aborts the in-flight HTTP request and rolls back any open write txn). **Outbox enqueues are intentionally NOT cancellable past the local-commit point** — once a command is durably written, cancelling the host call cannot un-enqueue it (the sale exists). The returned future resolves at *local commit*, not at server-ack, precisely so a flaky network cannot lose a committed sale.

```rust
#[uniffi::export(async_runtime = "tokio")]
impl SufrixCore {
    /// Async: hits network if online, else serves cache. Cancellable (drops HTTP).
    pub async fn sync_now(&self) -> Result<SyncReport, CoreError>;
}
```

| Call class | Async? | Cancellable? | Example |
|---|---|---|---|
| Network read / forced sync | yes | yes (aborts request) | `sync_now`, `login`, `refresh_session` |
| Cache read | no | n/a | `list_menu_items`, `current_session` |
| Cart math (in-memory) | no | n/a | `add_line`, `cart_totals` |
| Outbox command (write) | yes | **only before local commit** | `submit_order`, `open_shift` |

**[P1]** the whole model. Host-driven cancellation polish is **[P2]**.

---

## 4. Reads (cached DTOs) vs Commands (outbox)

### 4a. Read APIs — synchronous, return mirrored DTOs

Reads serve the SQLite read-mirror. They **layer branch/channel overrides over the base catalog** inside Rust so the host gets a single flat sellable list. `*_translations` are pre-resolved against `CoreConfig.locale` to a plain `String` (no host-side map juggling).

```rust
#[uniffi::export]
impl SufrixCore {
    /// Sellable menu = base items ∪ branch overrides (availability + price_override),
    /// soft-deleted rows filtered. base_price is i64 minor-units in the DTO even though
    /// the wire is int32 — the FFI normalizes ALL money to i64 minor-units (see §money).
    pub fn list_menu_items(&self) -> Vec<MenuItemView>;
    pub fn list_categories(&self) -> Vec<CategoryView>;
    pub fn list_addon_catalog(&self) -> Vec<AddonItemView>;
    pub fn available_bundles(&self) -> Vec<BundleView>;
    pub fn list_discounts(&self) -> Vec<DiscountView>;
    pub fn list_payment_methods(&self) -> Vec<PaymentMethodView>;

    pub fn list_orders(&self, filter: OrderFilter) -> Vec<OrderSummaryView>;
    pub fn get_order(&self, order_id: String) -> Option<OrderDetailView>;
    pub fn current_shift(&self, branch_id: String) -> Option<ShiftView>;
    pub fn list_delivery_tickets(&self) -> Vec<DeliveryTicketView>;   // [P2]
}

/// All money normalized to i64 minor-units (piastres) at the FFI boundary, killing the
/// int32/int64-split and BigDecimal-string traps before they reach hosts.
#[derive(uniffi::Record)]
pub struct MenuItemView {
    pub id: String,
    pub name: String,                 // translations pre-resolved to locale
    pub category_id: Option<String>,
    pub base_price_minor: i64,
    pub image_url: Option<String>,
    pub is_available: bool,           // base ∧ branch-override merged
    pub sizes: Vec<ItemSizeView>,     // price_override folded in, keyed by label
    pub addon_slots: Vec<AddonSlotView>,
}
```

### 4b. Command APIs — the order hot-path (outbox)

Commands **never block on the server.** They validate + price in Rust, write to the outbox + read-mirror in one txn, bump the pending count, and return the locally-committed entity with a **client temp-id**. The sync engine replays in order, reconciles the temp-id → server id, and emits `DataChanged`.

The cart is **server-stateless and core-owned**: the host opens a draft, mutates lines, and the core holds the in-progress cart in memory (and journals it so an app kill mid-order survives). `submit_order` is the single outbox write.

```rust
#[uniffi::export(async_runtime = "tokio")]
impl SufrixCore {
    // ---- cart lifecycle: in-memory, synchronous, no network ----

    /// Open a draft cart for an order type. Returns a CartView with a client cart_id.
    pub fn start_order(&self, branch_id: String, shift_id: String, order_type: String) -> CartView;

    /// Add a line. menu_item_id XOR bundle_id (enforced in Rust). addons / optional_field_ids
    /// / bundle_components are typed here even though the wire arrays are untyped.
    pub fn add_line(&self, cart_id: String, line: NewCartLine) -> Result<CartView, CoreError>;
    pub fn update_line_qty(&self, cart_id: String, line_id: String, qty: i32) -> Result<CartView, CoreError>;
    pub fn remove_line(&self, cart_id: String, line_id: String) -> Result<CartView, CoreError>;

    /// Apply/clear a discount (dtype is an open string; value semantics resolved in Rust).
    pub fn apply_discount(&self, cart_id: String, discount_id: Option<String>) -> Result<CartView, CoreError>;

    /// Live totals (subtotal/tax/discount/total) computed by the SAME Rust pricing
    /// engine the server uses, so the offline receipt matches the eventual server order.
    pub fn cart_totals(&self, cart_id: String) -> CartTotals;

    /// Capture tender. payment_method is an open string (mirrored from payment-methods).
    pub fn set_payment(&self, cart_id: String, tender: TenderInput) -> Result<CartView, CoreError>;

    // ---- the single outbox write ----

    /// Validates note-required-when-reason-other style cross-field rules, stamps a
    /// client-side created_at + idempotency key, writes CreateOrderRequest to the outbox
    /// AND the order into the read-mirror in one txn. Resolves at LOCAL COMMIT (not server
    /// ack). The returned order carries a temp order_number until sync reconciles it.
    pub async fn submit_order(&self, cart_id: String) -> Result<OrderDetailView, CoreError>;

    /// Void during selling — outbox-mutation. voided_at stamped client-side.
    pub async fn void_order(&self, order_id: String, req: VoidOrderRequest) -> Result<(), CoreError>;

    // ---- shift hot-path (outbox-mutation) ----

    /// Client-supplied id + opened_at for offline reconcile (spec's documented contract).
    /// opening_cash is i64 minor-units. Idempotency key owned by the client.
    pub async fn open_shift(&self, branch_id: String, req: OpenShiftRequest) -> Result<ShiftView, CoreError>;

    /// closing_cash_declared i64 minor-units; closed_at stamped client-side so the offline
    /// close keeps its true time. expected_cash is read from the cached Z-report, not re-derived.
    pub async fn close_shift(&self, shift_id: String, req: CloseShiftRequest) -> Result<ShiftReportView, CoreError>;

    /// Cash in/out mid-shift — outbox-mutation. created_at stamped client-side.
    pub async fn add_cash_movement(&self, shift_id: String, req: CashMovementInput) -> Result<(), CoreError>;

    /// Teller waste log — the only inventory outbox-mutation. Idempotency key client-owned.
    pub async fn log_waste(&self, branch_id: String, req: WasteInput) -> Result<(), CoreError>;  // [P2]

    // ---- printing (Rust owns it) ----

    /// Renders + prints a receipt/Z-report via the Rust printing backend (Star/Epson).
    /// Works fully offline from the read-mirror.
    pub async fn print_receipt(&self, order_id: String) -> Result<(), CoreError>;       // [P2]
    pub async fn print_shift_report(&self, shift_id: String) -> Result<(), CoreError>;  // [P2]
}

#[derive(uniffi::Record)]
pub struct CartTotals {
    pub subtotal_minor: i64,
    pub discount_minor: i64,
    pub tax_minor: i64,
    pub total_minor: i64,
    pub currency_code: String,
}
```

**[P1]:** `start_order`, `add_line`, `update_line_qty`, `remove_line`, `apply_discount`, `cart_totals`, `set_payment`, `submit_order`, `void_order`, `open_shift`, `close_shift`, `add_cash_movement`, and the menu/category/discount/payment-method/order/shift reads.
**[P2]:** delivery tickets (`list_delivery_tickets`, `set_status`, `set_prep_time`, `finalize`, `cancel`), `log_waste`, printing, `apply_discount` on bundles.
**[Later]:** all back-office CRUD (users, branches, catalog edits, purchasing, stocktakes, advisor, reports) — these stay **online-only** and are NOT in the FFI hot-path; they are exposed (if at all) as thin async passthroughs in a separate `admin` namespace.

---

## 5. Reactive status (observers)

One **callback interface** delivers a coarse-grained event stream; the host re-pulls the relevant sync read on each event (event-as-signal, not event-as-payload — avoids shipping large diffs over FFI and keeps the host's render path the single source of truth). A snapshot getter covers cold start.

```rust
/// Implemented by the HOST (SwiftUI ObservableObject / Compose StateFlow bridge).
/// Called on a core-owned thread; the host must hop to its main/UI dispatcher.
#[uniffi::export(callback_interface)]
pub trait CoreObserver: Send + Sync {
    /// Connectivity flipped (online↔offline) or sync engine state changed.
    fn on_status_changed(&self, status: SyncStatus);
    /// A domain the host renders was mutated locally or by an incoming sync.
    /// The host re-pulls only the named domains. Coarse on purpose.
    fn on_data_changed(&self, domains: Vec<DataDomain>);
    /// An outbox item permanently failed (e.g. server 4xx on replay) and needs
    /// teller/manager attention — surfaced as a banner, not a silent drop.
    fn on_outbox_failed(&self, item: OutboxFailure);
}

#[derive(uniffi::Enum)]
pub enum DataDomain { Menu, Orders, Shift, DeliveryTickets, Permissions, Session }

#[derive(uniffi::Record)]
pub struct SyncStatus {
    pub connectivity: Connectivity,         // Online / Offline / Reconnecting
    pub pending_outbox: u32,                 // count of un-acked commands
    pub last_sync_at: Option<i64>,           // epoch millis, None if never
    pub syncing: bool,                       // a sync pass is in flight
    pub failed_outbox: u32,                  // items in the dead-letter state
}

#[derive(uniffi::Enum)]
pub enum Connectivity { Online, Offline, Reconnecting }

#[derive(uniffi::Record)]
pub struct OutboxFailure {
    pub client_id: String,                   // temp-id of the failed command
    pub kind: String,                        // "order" | "void" | "shift_close" | ...
    pub http_status: Option<u16>,
    pub message: String,
    pub retryable: bool,
}

#[uniffi::export]
impl SufrixCore {
    /// Register one observer (replaces any prior). Returns the current snapshot so the
    /// host can render before the first event.
    pub fn set_observer(&self, observer: Box<dyn CoreObserver>) -> SyncStatus;

    /// Pull-style snapshot for cold start / pull-to-refresh badge.
    pub fn sync_status(&self) -> SyncStatus;

    /// List the outbox for a "pending sales" debug/management sheet.
    pub fn pending_outbox(&self) -> Vec<OutboxItemView>;  // [P2]
}
```

The core internally drives `on_status_changed` from `connectivity_plus`-equivalent reachability **inside Rust** (the host does NOT report connectivity — the core probes it), the sync engine's pass lifecycle, and the outbox counter.

**[P1]:** `set_observer`, `on_status_changed`, `on_data_changed`, `sync_status`. **[P2]:** `on_outbox_failed`, `pending_outbox`, `failed_outbox` dead-letter UX.

---

## 6. Error model

**One top-level `CoreError` enum** crosses the FFI — hosts write a single `catch`/`when`. It is *coarse and host-actionable*, not a 1:1 mirror of HTTP/serde errors. Rich diagnostics ride along as fields; the variant tells the host how to *react* (retry, re-login, show validation, queue offline). Per-domain error explosions are avoided because the three thin hosts must share one error-handling path.

```rust
#[derive(uniffi::Error, Debug, thiserror::Error)]
pub enum CoreError {
    /// No connectivity AND the op is online-only (login, resolve-branch, admin).
    /// Hot-path commands NEVER return this — they queue and succeed locally.
    #[error("offline: {message}")]
    Offline { message: String },

    /// 401/expired and refresh failed — host must route to the login screen.
    #[error("auth required: {message}")]
    Unauthenticated { message: String },

    /// 403 / permission gate failed locally.
    #[error("forbidden: {resource}/{action}")]
    Forbidden { resource: String, action: String },

    /// Client-side validation (mode invariants, note-required-when-reason-other,
    /// empty cart, future-dated cash movement). field names the offending input.
    #[error("invalid: {field}: {message}")]
    Validation { field: String, message: String },

    /// Server rejected a request (4xx other than auth) — carries status + server code.
    #[error("server {status}: {code}")]
    Server { status: u16, code: String, message: String },

    /// Transient server/network 5xx/timeout — host may retry; sync already will.
    #[error("transient: {message}")]
    Transient { message: String },

    /// Local store/migration/serialization failure — non-recoverable, report+log.
    #[error("internal: {message}")]
    Internal { message: String },
}
```

The sync engine and serde-fallbacks absorb the *quirks* the prompt flags (open enums via `#[serde(other)]`, untyped arrays/objects, int32/int64 splits, PATCH skip-if-none, multipart) **below** this boundary — none of them surface as host-facing error variants.

**[P1]:** the full enum.

---

## 7. FFI surface versioning (lockstep across 3 apps)

The risk: SwiftUI / Compose-Android / Compose-Desktop drift against the core. Three layers keep them locked:

1. **Single source of truth, single artifact.** The `.swift` and `.kt` bindings are **generated from the one `sufrix_ffi` crate** by `uniffi-bindgen` in CI, and published as versioned artifacts (Swift package + Maven `.aar` + desktop `.jar`) tagged `ffi-vMAJOR.MINOR`. No host hand-writes bindings. A host pins exactly one artifact version.

2. **`FFI_VERSION` handshake at runtime.** A monotonic integer constant is baked into the core and echoed in `RuntimeInfo.ffi_version` (§1). The host bundles the `FFI_VERSION` it was built against; on `SufrixCore::new` the core compares:
   - host major < core major → core returns `CoreError::Internal{"ffi version too old, update app"}` and refuses to run (forces an app update before a teller hits a broken surface).
   - host minor < core minor → allowed (additive-only minor changes are backward compatible).

```rust
/// Bumped MAJOR on any breaking change to an exported signature/record/enum;
/// MINOR on purely additive changes (new optional field, new method, new enum variant
/// that has a serde-other fallback). Exposed so the host can assert at startup.
#[uniffi::export]
pub fn ffi_version() -> FfiVersion;

#[derive(uniffi::Record)]
pub struct FfiVersion { pub major: u32, pub minor: u32 }
```

3. **Additive-only discipline (SemVer for the FFI).** Within a major: only *add* methods / *add* `Option` record fields / *add* enum variants paired with a host-side default arm. Renames, removals, type changes, or making an `Option` field required ⇒ **major bump** ⇒ all three apps rebuild against the new artifact in the same release train. Every enum the host matches on is documented as "exhaustive within major; add a default arm" so a minor variant-add never crashes a not-yet-updated host build path.

**[P1]:** `ffi_version()`, the `new`-time major handshake, the CI codegen pipeline. **[P2]:** the artifact registry + automated host-bump PRs.

---

## Appendix — money & quirk normalization (boundary rules)

These are core-internal invariants the hosts rely on but never implement:

- **All money → `i64` minor-units at the FFI.** The wire's int32 (catalog/order) vs int64 (summaries/costing) split, and any BigDecimal-as-string in inventory/recipe schemas, are normalized in `sufrix_ffi`. Hosts format `*_minor` with `currency_code` from `SessionSnapshot`.
- **All open string enums stay `String`** in DTOs (`order_type`, `payment_method`, `status`, `dtype`, `addon_type`, `channel`, `reason`, `movement_type`, `variance_reason`). Closed enums with no server-default (`UserRole`, `PrinterBrand`, `BundleStatus`, `RunStatus`, `SuggestionKind`, `Decision`, `Action`) get `#[serde(other)] Unknown` below the boundary; the FFI either maps `Unknown` to a string or hides it.
- **`*_translations` resolved to a single `String`** against `CoreConfig.locale` before crossing the FFI — hosts never receive raw `serde_json::Value` maps.
- **Idempotency keys + client temp-ids + client timestamps** are all generated in Rust for every outbox command (no server Idempotency-Key contract exists; the client owns it). Replay ordering and temp-id→server-id reconciliation are core-internal.
- **`tax_rate` is `f64`** (JSON number) in `SessionSnapshot`, matching the auth/me wire reality — not a decimal string.

---

Relevant existing files referenced while grounding this design: `/Users/shawket/Desktop/sufrix_pos/tool/generate_api.sh` (the `./tool/generate_api.sh` regen entrypoint the Rust `sufrix_api` crate would mirror), `/Users/shawket/Desktop/sufrix_pos/packages/sufrix_api/` (current generated Dart wire-model package the Rust port replaces), and `/Users/shawket/Desktop/sufrix_pos/CLAUDE.md` (offline-first + generated-wire-model conventions). No Rust/UniFFI scaffolding exists yet — this is a greenfield surface.