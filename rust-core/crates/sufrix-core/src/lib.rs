//! Sufrix POS shared core.
//!
//! THE ONE RULE: all real logic lives here. The Swift (iPhone/iPad) and Kotlin
//! (Android + desktop) apps are UI and platform glue only — they call into this
//! library over UniFFI for data, business rules, API calls, offline/sync and
//! printing. If a piece of logic could ever differ between platforms, that's a
//! bug; it belongs in Rust.
//!
//! Build-out is phased (see ../../../PLAN.md):
//!   Phase 1 (here): core skeleton + UniFFI bindings proven on every platform.
//!   Phase 2: API client (crates/sufrix-api) + auth + online read/write.
//!   Phase 3: SQLite local store + read-through cache + durable outbox.
//!   Phase 4: sync engine + backend offline-first support.
//!   Phase 5: printing (ESC/POS) in Rust.

uniffi::setup_scaffolding!();

mod config;
pub use config::SufrixConfig;

/// The client-authoritative pricing engine (pure; the money source of truth).
pub mod pricing;

/// Cart — client-only in-progress order state, priced via `pricing`.
pub mod cart;
/// Checkout — assemble an order from the cart + place it via the outbox.
pub mod checkout;
/// The coarse FFI error model the host reacts to (PLAN §7.6).
pub mod error;
/// Static UI-string localization — one source of truth for both hosts.
pub mod i18n;
/// Menu / catalog reads — branch-effective mirror + view DTOs (PLAN §R9).
pub mod menu;
/// Order history reads — synced + still-queued orders for the shift.
pub mod orders;
/// Thermal-receipt rendering (ESC/POS) + best-effort network printing.
pub mod receipt;
/// Local recipe preview — effective ingredients for a configured item (parity
/// with Flutter's `computeRecipeLocally`).
pub mod recipe;
/// Category styling (icon + gradient palette) — port of Flutter's `CatStyle`.
pub mod catstyle;
/// HTTP layer — drives the generated `sufrix-api` reqwest client (PLAN §R4 net/).
pub mod net;
/// Session & auth — online login, offline unlock, token custody (PLAN §7.2).
pub mod session;
/// Shift lifecycle — open/current via the outbox (PLAN §7.4).
pub mod shift;
/// Local store — SQLite mirror + durable outbox + id_map + sync cursors (PLAN §8).
pub mod store;

use std::sync::{Arc, Mutex, RwLock};

use error::CoreError;

/// Crate version (semver of the core library).
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// The FFI *surface* contract version, independent of the crate version. Bump
/// on every breaking change to the exported API so all three apps can assert at
/// startup that they were built against a compatible core (see PLAN.md §"FFI
/// surface versioning").
#[uniffi::export]
pub fn ffi_surface_version() -> u32 {
    0
}

/// Smoke-test call used to prove the binding pipeline end-to-end from each host.
#[uniffi::export]
pub fn greet(name: String) -> String {
    format!("Sufrix core v{} says hello, {name}", core_version())
}

/// The screen the host should show, decided by the core (PLAN §R11). The host
/// consults this only at deliberate transitions (cold start, post-login,
/// post-open/close-shift, sign-out) — never as a side effect of connectivity.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppRoute {
    /// Till not bound to a branch → manager device-setup.
    DeviceSetup,
    /// Configured but signed out → teller PIN login.
    Login,
    /// Signed in, no open shift → open-shift screen.
    OpenShift,
    /// Signed in with an open shift → order screen.
    Order,
}

/// Top-level handle the host creates once and keeps alive for the app lifetime.
///
/// Phase 1 exposes config + version only. Later phases hang the API client,
/// local store, sync engine and printer off this object — the host keeps
/// holding the same handle.
#[derive(uniffi::Object)]
pub struct SufrixCore {
    config: SufrixConfig,
    store: store::Store,
    /// Active UI locale (en/ar) — runtime-changeable via `set_locale`; seeds from
    /// `config.locale`. Drives `tr`/`is_rtl` + catalog `*_translations` resolution.
    locale: RwLock<String>,
    /// HTTP client to the backend (holds the live bearer token).
    api: net::ApiClient,
    /// The live session (`None` = signed out). Set by login / offline unlock /
    /// cold-start restore; cleared on logout.
    session: RwLock<Option<session::SessionState>>,
    /// The host's secure-bytes vault for the session blob (Keychain/Keystore).
    token_store: Mutex<Option<Box<dyn session::TokenStore>>>,
    /// Server-vs-device clock skew in SECONDS, refreshed by `refresh_connectivity`
    /// (from the server `Date` header). Drives the clock-skew banner.
    clock_skew_secs: std::sync::atomic::AtomicI64,
}

#[uniffi::export]
impl SufrixCore {
    /// Construct with explicit config (the host fills `db_path` with an
    /// app-private file). Opens + migrates the local store and builds the HTTP
    /// client; the session starts empty (host calls `restore_session` at boot).
    #[uniffi::constructor]
    pub fn new(config: SufrixConfig) -> Result<Arc<Self>, error::CoreError> {
        let store = store::Store::open(&config.db_path)?;
        let api = net::ApiClient::new(config.base_url.clone())?;
        let locale = RwLock::new(config.locale.clone());
        Ok(Arc::new(Self {
            config,
            store,
            locale,
            api,
            session: RwLock::new(None),
            token_store: Mutex::new(None),
            clock_skew_secs: std::sync::atomic::AtomicI64::new(0),
        }))
    }

    /// Construct from the baked-in `.env` defaults (in-memory store until the
    /// host supplies a `db_path`).
    #[uniffi::constructor]
    pub fn from_env() -> Result<Arc<Self>, error::CoreError> {
        Self::new(SufrixConfig::from_env())
    }

    /// API base URL the core will talk to (from `.env`).
    pub fn base_url(&self) -> String {
        self.config.base_url.clone()
    }

    /// Environment name (`prod` | `staging` | `dev`).
    pub fn environment(&self) -> String {
        self.config.environment.clone()
    }

    /// SQLite path the host handed us (empty => in-memory).
    pub fn db_path(&self) -> String {
        self.config.db_path.clone()
    }

    /// Core crate version.
    pub fn version(&self) -> String {
        core_version()
    }

    /// Outbox items still waiting to sync (pending + in-flight) — the host shows
    /// this in the sync-status chrome.
    pub fn pending_outbox_count(&self) -> Result<u32, error::CoreError> {
        self.store.pending_count()
    }

    // ── session (sync) ──────────────────────────────────────────────────────

    /// Install the host's secure-bytes vault. Call once, right after `new`,
    /// before `restore_session`.
    pub fn set_token_store(&self, store: Box<dyn session::TokenStore>) {
        *self.token_store.lock().unwrap_or_else(|e| e.into_inner()) = Some(store);
    }

    /// Re-hydrate a session from the host's persisted blob at cold start. Returns
    /// the snapshot if the blob is valid, else `None` (fresh install / corrupt).
    pub fn restore_session(&self, blob: Vec<u8>) -> Option<session::SessionSnapshot> {
        let state = session::SessionState::from_blob(&blob)?;
        self.api.set_bearer(state.token.clone());
        let snapshot = state.snapshot.clone();
        *self.session.write().unwrap_or_else(|e| e.into_inner()) = Some(state);
        Some(snapshot)
    }

    pub fn is_authenticated(&self) -> bool {
        self.session.read().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    /// The cached session — never hits the network.
    pub fn current_session(&self) -> Option<session::SessionSnapshot> {
        self.session
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|s| s.snapshot.clone())
    }

    /// Permission check against the mirrored matrix. Optimistic while a session
    /// is offline-unlocked (permissions not yet loaded) — see `SessionState`.
    pub fn has_permission(&self, resource: String, action: String) -> bool {
        self.session
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|s| s.has_permission(&resource, &action))
            .unwrap_or(false)
    }

    /// Offline unlock: verify a typed PIN against the cached org bundle
    /// (argon2id). No network, no token; identity is the real server `user_id`.
    pub fn unlock_offline(
        &self,
        name: String,
        pin: String,
        branch_id: String,
    ) -> Result<session::SessionSnapshot, CoreError> {
        let state = session::unlock_from_bundle(&self.store, &name, &pin, &branch_id)?;
        let snapshot = state.snapshot.clone();
        self.api.set_bearer(None);
        self.persist_and_set(state);
        Ok(snapshot)
    }

    /// Sign out: clear the live session + token (the JWT is stateless — there is
    /// no server-side logout endpoint, so clearing locally is the revocation) and
    /// the host vault. Drops the cached shift so a re-login reconciles fresh from
    /// the server. Does NOT force-close the open shift — it stays open on the
    /// server for whoever resumes it. Preserves the outbox unless `wipe_outbox`.
    pub fn logout(&self, wipe_outbox: bool) -> Result<(), CoreError> {
        self.api.set_bearer(None);
        *self.session.write().unwrap_or_else(|e| e.into_inner()) = None;
        if let Some(ts) = self.token_store.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            ts.clear_blob();
        }
        let _ = shift::clear(&self.store);
        let _ = cart::clear(&self.store);
        if wipe_outbox {
            self.store.wipe_outbox()?;
        }
        Ok(())
    }
}

impl SufrixCore {
    /// Persist a session to the host vault and install it as the live session.
    fn persist_and_set(&self, state: session::SessionState) {
        if let Some(ts) = self.token_store.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            ts.save_blob(state.to_blob());
        }
        *self.session.write().unwrap_or_else(|e| e.into_inner()) = Some(state);
    }

    /// The active runtime locale (defaults to `config.locale`).
    fn current_locale(&self) -> String {
        self.locale.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// `(org_id, branch_id)` from the live session — needed for branch-effective
    /// catalog fetches. Errors if signed out / no org.
    fn org_branch(&self) -> Result<(String, Option<String>), CoreError> {
        let g = self.session.read().unwrap_or_else(|e| e.into_inner());
        let s = g.as_ref().ok_or_else(|| CoreError::Unauthenticated {
            detail: "not signed in".into(),
        })?;
        let org = s.snapshot.org_id.clone().ok_or_else(|| CoreError::Validation {
            field: "org_id".into(),
            detail: "session has no org".into(),
        })?;
        Ok((org, s.snapshot.branch_id.clone()))
    }

    /// Whether a shift command of `op_type` for the CURRENTLY CACHED shift is
    /// still queued — scoped to that shift's id. The outbox is device-global and
    /// survives sign-out, so an unrelated teller's orphaned command must NOT
    /// count (it would keep a force-closed shift alive for the next teller on a
    /// shared till). open_shift ops are keyed by the shift PK; close_shift ops by
    /// `{shift_id}:close` (so open + close for one shift don't collide in the
    /// idempotent outbox).
    fn shift_command_pending(&self, op_type: &str) -> Result<bool, CoreError> {
        let sid = match shift::current(&self.store)? {
            Some(s) => s.id,
            None => return Ok(false),
        };
        let close_id = format!("{sid}:close");
        Ok(self
            .store
            .pending()?
            .iter()
            .any(|i| i.op_type == op_type && (i.id == sid || i.id == close_id)))
    }

    /// Drain the durable outbox in FIFO order. Each op dispatches by `op_type`;
    /// a connectivity error stops the drain (items stay pending for next time),
    /// a 4xx marks the item dead. The single place outbox writes hit the network.
    async fn drain_outbox(&self) -> Result<(), CoreError> {
        use sufrix_api::apis::shifts_api;
        for item in self.store.pending()? {
            match item.op_type.as_str() {
                "open_shift" => {
                    let cmd: shift::OpenShiftCommand = serde_json::from_str(&item.payload)?;
                    let res = shifts_api::open_shift(
                        &self.api.config(),
                        shifts_api::OpenShiftParams {
                            branch_id: cmd.branch_id,
                            open_shift_request: cmd.request,
                        },
                    )
                    .await;
                    match res {
                        Ok(server) => {
                            shift::save(&self.store, &server)?;
                            self.store.mark_acked(item.seq, Some(&server.id.to_string()))?;
                        }
                        Err(e) => match net::map_api_error(e) {
                            // Network down → stop; retry the whole queue later.
                            CoreError::Offline { .. } | CoreError::Transient { .. } => return Ok(()),
                            // 4xx/terminal → dead-letter, keep draining the rest.
                            other => {
                                self.store.mark_dead(item.seq, &other.to_string())?;
                                // The server REJECTED the open (e.g. 409 already
                                // open). Drop the optimistic shift so the teller
                                // isn't left selling against a shift that doesn't
                                // exist server-side (checkout only checks is_open).
                                if shift::current(&self.store)?.map(|s| s.id) == Some(item.id.clone()) {
                                    let _ = shift::clear(&self.store);
                                }
                            }
                        },
                    }
                }
                "close_shift" => {
                    let cmd: shift::CloseShiftCommand = serde_json::from_str(&item.payload)?;
                    let res = shifts_api::close_shift(
                        &self.api.config(),
                        shifts_api::CloseShiftParams {
                            shift_id: cmd.shift_id,
                            close_shift_request: cmd.request,
                        },
                    )
                    .await;
                    match res {
                        Ok(_) => self.store.mark_acked(item.seq, None)?,
                        Err(e) => match net::map_api_error(e) {
                            CoreError::Offline { .. } | CoreError::Transient { .. } => return Ok(()),
                            // close_shift is idempotent (keyed `{shift_id}:close`): a
                            // 409 means the server already closed it → treat as done.
                            CoreError::Server { status: 409, .. } => self.store.mark_acked(item.seq, None)?,
                            other => self.store.mark_dead(item.seq, &other.to_string())?,
                        },
                    }
                }
                "create_order" => {
                    use sufrix_api::apis::orders_api;
                    let cmd: checkout::CheckoutCommand = serde_json::from_str(&item.payload)?;
                    let res = orders_api::create_order(
                        &self.api.config(),
                        orders_api::CreateOrderParams { create_order_request: cmd.request },
                    )
                    .await;
                    match res {
                        Ok(order) => {
                            self.store.mark_acked(item.seq, Some(&order.id.to_string()))?;
                        }
                        Err(e) => match net::map_api_error(e) {
                            CoreError::Offline { .. } | CoreError::Transient { .. } => return Ok(()),
                            other => self.store.mark_dead(item.seq, &other.to_string())?,
                        },
                    }
                }
                "void_order" => {
                    use sufrix_api::apis::orders_api;
                    let cmd: orders::VoidOrderCommand = serde_json::from_str(&item.payload)?;
                    let res = orders_api::void_order(
                        &self.api.config(),
                        orders_api::VoidOrderParams {
                            order_id: cmd.order_id,
                            void_order_request: cmd.request,
                        },
                    )
                    .await;
                    match res {
                        Ok(_) => self.store.mark_acked(item.seq, None)?,
                        Err(e) => match net::map_api_error(e) {
                            CoreError::Offline { .. } | CoreError::Transient { .. } => return Ok(()),
                            // void is idempotent (guarded CAS, keyed `{order_id}:void`):
                            // a 409 means it was already voided → treat as done.
                            CoreError::Server { status: 409, .. } => self.store.mark_acked(item.seq, None)?,
                            other => self.store.mark_dead(item.seq, &other.to_string())?,
                        },
                    }
                }
                _ => {} // future op types
            }
        }
        Ok(())
    }
}

// ── shift + routing (sync reads) ─────────────────────────────────────────────
#[uniffi::export]
impl SufrixCore {
    /// The device's current shift (open or closed), served from the local store.
    pub fn current_shift(&self) -> Result<Option<shift::ShiftView>, CoreError> {
        shift::current(&self.store)
    }

    /// Suggested opening cash for the next shift (minor units) — the previous
    /// shift's declared closing, for cash continuity. 0 when none is known. The
    /// open-shift screen prefills this; deviating from it requires a reason.
    pub fn suggested_opening_cash_minor(&self) -> Result<i64, CoreError> {
        shift::suggested_opening_cash(&self.store)
    }

    /// The screen to show. `branch_configured` + `reconfiguring` are host-owned
    /// bits (the device branch lives in the host vault); the rest is core state.
    pub fn app_route(&self, branch_configured: bool, reconfiguring: bool) -> AppRoute {
        if reconfiguring || !branch_configured {
            return AppRoute::DeviceSetup;
        }
        let guard = self.session.read().unwrap_or_else(|e| e.into_inner());
        let session = match guard.as_ref() {
            Some(s) => s,
            None => return AppRoute::Login,
        };
        // An open shift counts only if it belongs to THIS teller (a stale shift
        // from a previous teller on the device must not route them past setup).
        match shift::current(&self.store) {
            Ok(Some(s)) if s.is_open && s.teller_id == session.snapshot.user_id => AppRoute::Order,
            _ => AppRoute::OpenShift,
        }
    }
}

// ── localization (sync) ──────────────────────────────────────────────────────
#[uniffi::export]
impl SufrixCore {
    /// Localized UI string for `key` in the device locale (en/ar; falls back to
    /// en, then the key). The single source of truth for both hosts.
    pub fn tr(&self, key: String) -> String {
        i18n::tr(&self.current_locale(), &key)
    }
    /// The active locale (BCP-47).
    pub fn locale(&self) -> String {
        self.current_locale()
    }
    /// Change the active UI locale at runtime (e.g. "en" / "ar"). Strings,
    /// RTL, and catalog `*_translations` all re-resolve on the next read; the
    /// host persists the choice and re-renders.
    pub fn set_locale(&self, locale: String) {
        *self.locale.write().unwrap_or_else(|e| e.into_inner()) = locale;
    }
    /// Whether the locale is right-to-left (host flips layout direction).
    pub fn is_rtl(&self) -> bool {
        i18n::is_rtl(&self.current_locale())
    }
}

// ── receipt rendering (sync; pure byte assembly) ─────────────────────────────
#[uniffi::export]
impl SufrixCore {
    /// Render a placed order's receipt to ESC/POS bytes ready to stream to a
    /// thermal printer. Labels resolve from the active locale; `store_name`
    /// (branch) and `currency` come from the host, `width` is the paper's
    /// column count (58mm ≈ 32, 80mm ≈ 48). Pair with `send_to_printer`.
    pub fn render_receipt(
        &self,
        receipt: checkout::ReceiptView,
        store_name: String,
        currency: String,
        width: u32,
    ) -> Vec<u8> {
        let loc = self.current_locale();
        let tr = |k: &str| i18n::tr(&loc, k);
        let ctx = receipt::EscPosCtx {
            store_name,
            currency,
            width,
            labels: receipt::ReceiptLabels {
                order: tr("receipt.order"),
                reference: tr("receipt.ref"),
                voided: tr("receipt.voided"),
                delivery: tr("receipt.delivery"),
                channel_in_mall: tr("delivery.in_mall"),
                channel_outside: tr("delivery.outside"),
                customer: tr("receipt.customer"),
                phone: tr("receipt.phone"),
                address: tr("receipt.address"),
                zone: tr("receipt.zone"),
                delivery_ref: tr("receipt.delivery_ref"),
                payment_hint: tr("receipt.payment_hint"),
                notes: tr("receipt.notes"),
                subtotal: tr("order.subtotal"),
                discount: tr("order.discount"),
                tax: tr("order.tax"),
                delivery_fee: tr("receipt.delivery_fee"),
                total: tr("order.total"),
                payment: tr("receipt.payment"),
                teller: tr("receipt.teller"),
                queued: tr("order.queued_hint"),
                thank_you: tr("receipt.thank_you"),
            },
        };
        receipt::escpos(&receipt, &ctx)
    }

    /// Render the shift report (Z-report) to ESC/POS bytes — same printer path
    /// as `render_receipt`; pair with `send_to_printer`.
    pub fn render_shift_report(
        &self,
        report: shift::ShiftReportView,
        store_name: String,
        currency: String,
        width: u32,
    ) -> Vec<u8> {
        let loc = self.current_locale();
        let tr = |k: &str| i18n::tr(&loc, k);
        let labels = receipt::ShiftReportLabels {
            title: tr("shift.report_title"),
            opening: tr("shifts.opening"),
            payments: tr("shift.payments"),
            cash_moves: tr("shift.cash_moves"),
            expected: tr("shift.expected_cash"),
            voided: tr("history.voided"),
            by_method: tr("shift.by_method"),
        };
        receipt::escpos_shift_report(&report, &store_name, &currency, width, &labels)
    }
}

// ── catalog reads (sync; serve the local mirror, always succeed offline) ─────
#[uniffi::export]
impl SufrixCore {
    /// Themed style (icon key + gradient palette) for a category/item name —
    /// the host maps `icon` to a glyph and paints the gradient. Pure; mirrors
    /// Flutter's `CatStyle.of`. `dark` picks the dark-mode palette.
    pub fn category_style(&self, name: String, dark: bool) -> catstyle::CatStyleView {
        catstyle::category_style(&name, dark)
    }

    pub fn list_menu_items(&self) -> Result<Vec<menu::MenuItemView>, CoreError> {
        menu::menu_items(&self.store, &self.current_locale())
    }
    pub fn list_categories(&self) -> Result<Vec<menu::CategoryView>, CoreError> {
        menu::categories(&self.store, &self.current_locale())
    }
    pub fn list_addon_catalog(&self) -> Result<Vec<menu::AddonItemView>, CoreError> {
        menu::addons(&self.store, &self.current_locale())
    }
    /// Bundles orderable right now — status active and within their date/time
    /// window at `now` (branch-local). The host passes its local time so the
    /// window is evaluated in the till's timezone (Flutter parity).
    pub fn available_bundles(&self, now_rfc3339: String) -> Result<Vec<menu::BundleView>, CoreError> {
        let now = chrono::DateTime::parse_from_rfc3339(&now_rfc3339)
            .map_err(|_| CoreError::Validation { field: "now".into(), detail: "bad timestamp".into() })?;
        Ok(menu::bundles(&self.store, &self.current_locale())?
            .into_iter()
            .filter(|b| menu::bundle_available(b, now))
            .collect())
    }
    pub fn list_payment_methods(&self) -> Result<Vec<menu::PaymentMethodView>, CoreError> {
        menu::payment_methods(&self.store, &self.current_locale())
    }
    pub fn list_discounts(&self) -> Result<Vec<menu::DiscountView>, CoreError> {
        menu::discounts(&self.store, &self.current_locale())
    }
}

// ── cart (sync; client-only order state, offline-safe, kv-persisted) ──────────
#[uniffi::export]
impl SufrixCore {
    /// The current cart lines (empty when none).
    pub fn cart_lines(&self) -> Result<Vec<cart::CartLineView>, CoreError> {
        cart::lines(&self.store)
    }
    /// Add one unit of a menu item (merges into the matching line). The host
    /// passes the resolved display name + unit price so the cart is self-contained.
    pub fn cart_add(
        &self,
        item_id: String,
        name: String,
        unit_price_minor: i64,
    ) -> Result<Vec<cart::CartLineView>, CoreError> {
        cart::add(&self.store, &item_id, &name, unit_price_minor)
    }
    /// Add a CONFIGURED line (size + addons + optionals + notes). The core
    /// resolves the charged prices from the cached catalog (size unit price;
    /// addon swap-delta vs additive; optional prices) and merges identical
    /// configs. `addons` carry the chosen ids + quantities; the prices are
    /// resolved here, not trusted from the host.
    pub fn cart_add_configured(
        &self,
        item_id: String,
        size_label: Option<String>,
        addons: Vec<cart::AddonSelection>,
        optional_field_ids: Vec<String>,
        qty: i64,
        notes: Option<String>,
    ) -> Result<Vec<cart::CartLineView>, CoreError> {
        let items = menu::menu_items(&self.store, &self.current_locale())?;
        let item = items
            .iter()
            .find(|i| i.id == item_id)
            .ok_or_else(|| CoreError::Validation { field: "item".into(), detail: "unknown item".into() })?;
        let addon_catalog = menu::addons(&self.store, &self.current_locale())?;
        let line = cart::resolve_line(item, &addon_catalog, size_label, &addons, &optional_field_ids, qty, notes);
        cart::add_resolved(&self.store, line)
    }
    /// Add a configured BUNDLE line: the fixed bundle price + each component's
    /// chosen item/size/addons/optionals. The core resolves the component
    /// up-charges from the catalog (component base/size price is never charged —
    /// the bundle price covers it) and merges identical bundle configs.
    pub fn cart_add_bundle(
        &self,
        bundle_id: String,
        components: Vec<cart::BundleComponentSelection>,
        qty: i64,
    ) -> Result<Vec<cart::CartLineView>, CoreError> {
        let locale = self.current_locale();
        let bundles = menu::bundles(&self.store, &locale)?;
        let bundle = bundles
            .iter()
            .find(|b| b.id == bundle_id)
            .ok_or_else(|| CoreError::Validation { field: "bundle".into(), detail: "unknown bundle".into() })?;
        let items = menu::menu_items(&self.store, &locale)?;
        let addon_catalog = menu::addons(&self.store, &locale)?;
        let line = cart::resolve_bundle_line(bundle, &items, &addon_catalog, &components, qty);
        cart::add_resolved(&self.store, line)
    }
    /// Active addons offered for an item, with their CHARGED price resolved (swap
    /// delta / full) — the customization sheet groups these by `addon_type`.
    pub fn list_item_addons(&self, item_id: String) -> Result<Vec<cart::ItemAddonView>, CoreError> {
        let items = menu::menu_items(&self.store, &self.current_locale())?;
        let item = items
            .iter()
            .find(|i| i.id == item_id)
            .ok_or_else(|| CoreError::Validation { field: "item".into(), detail: "unknown item".into() })?;
        let addon_catalog = menu::addons(&self.store, &self.current_locale())?;
        Ok(cart::item_addons(item, &addon_catalog))
    }
    /// Live recipe preview for the current selection (size + addons + optionals).
    /// Pure projection over the mirrored catalog, so the customization sheet can
    /// recompute on every toggle, online or offline. Mirrors the Flutter teller
    /// app: base by size, milk/coffee swaps, additive addons (× qty), and
    /// optional-field ingredient contributions.
    pub fn compute_recipe(
        &self,
        item_id: String,
        size_label: Option<String>,
        addons: Vec<cart::AddonSelection>,
        optional_field_ids: Vec<String>,
    ) -> Result<Vec<recipe::ComputedRecipeLineView>, CoreError> {
        let items = menu::menu_items(&self.store, &self.current_locale())?;
        let item = items
            .iter()
            .find(|i| i.id == item_id)
            .ok_or_else(|| CoreError::Validation { field: "item".into(), detail: "unknown item".into() })?;
        let addon_catalog = menu::addons(&self.store, &self.current_locale())?;
        Ok(recipe::compute_recipe(item, &addon_catalog, size_label.as_deref(), &addons, &optional_field_ids))
    }
    /// Set a line's absolute quantity (by its key); `qty <= 0` removes the line.
    pub fn cart_set_qty(
        &self,
        item_id: String,
        qty: i64,
    ) -> Result<Vec<cart::CartLineView>, CoreError> {
        cart::set_qty(&self.store, &item_id, qty)
    }
    /// Remove a line entirely (stashed for undo — see `cart_restore_removed`).
    pub fn cart_remove(&self, item_id: String) -> Result<Vec<cart::CartLineView>, CoreError> {
        cart::remove(&self.store, &item_id)
    }
    /// Undo the last `cart_remove` — re-inserts the swiped-away line. No-op if
    /// nothing was removed (or it was already restored / the cart was cleared).
    pub fn cart_restore_removed(&self) -> Result<Vec<cart::CartLineView>, CoreError> {
        cart::restore_last_removed(&self.store)
    }
    /// Empty the cart.
    pub fn cart_clear(&self) -> Result<(), CoreError> {
        cart::clear(&self.store)
    }
    /// Park the current cart as a named draft (held order) and empty the cart.
    pub fn hold_cart(&self, name: String) -> Result<(), CoreError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        cart::hold(&self.store, id, name, now)
    }
    /// The parked drafts (held orders), newest first.
    pub fn list_drafts(&self) -> Result<Vec<cart::DraftView>, CoreError> {
        cart::drafts(&self.store)
    }
    /// Restore a draft into the cart (replaces current lines) and drop it.
    pub fn restore_draft(&self, id: String) -> Result<Vec<cart::CartLineView>, CoreError> {
        cart::restore_draft(&self.store, &id)
    }
    /// Discard a parked draft.
    pub fn discard_draft(&self, id: String) -> Result<(), CoreError> {
        cart::discard_draft(&self.store, &id)
    }
    /// Apply a discount (by id) to the cart — reflected in `cart_totals`.
    pub fn cart_set_discount(&self, discount_id: String) -> Result<(), CoreError> {
        cart::set_discount(&self.store, &discount_id)
    }
    /// Remove the cart discount.
    pub fn cart_clear_discount(&self) -> Result<(), CoreError> {
        cart::clear_discount(&self.store)
    }
    /// The selected discount id (for the tender UI), or `None`.
    pub fn cart_discount_id(&self) -> Result<Option<String>, CoreError> {
        cart::discount_id(&self.store)
    }
    /// Priced cart summary at the session's org tax rate (0 when signed out),
    /// computed through the pricing engine.
    pub fn cart_totals(&self) -> Result<cart::CartTotals, CoreError> {
        let tax_rate = self.current_session().map(|s| s.tax_rate).unwrap_or(0.0);
        cart::totals(&self.store, tax_rate)
    }
}

/// A queued/failed outbox command, projected for the sync center.
#[derive(uniffi::Record, Clone, Debug)]
pub struct OutboxItemView {
    pub id: String,
    /// `open_shift` | `close_shift` | `create_order` | …
    pub op_type: String,
    /// `pending` | `inflight` | `dead`.
    pub status: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub event_at: String,
}

/// One-shot sync health for the action-bar chip + offline banner. `pending` is
/// the in-flight/queued set, `failed` the stuck (dead) set, `online` the
/// session's connectivity. The host maps these to the chip label/tone.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct SyncStatusView {
    pub pending: u32,
    pub failed: u32,
    pub online: bool,
}

// ── sync center (outbox visibility + retry/discard) ──────────────────────────
#[uniffi::export]
impl SufrixCore {
    /// Queued + failed commands for the sync center (acked rows hidden), oldest
    /// first. Always succeeds offline.
    pub fn list_outbox(&self) -> Result<Vec<OutboxItemView>, CoreError> {
        Ok(self
            .store
            .list_active()?
            .into_iter()
            .map(|i| OutboxItemView {
                id: i.id,
                op_type: i.op_type,
                status: i.status,
                attempts: i.attempts,
                last_error: i.last_error,
                event_at: i.event_at,
            })
            .collect())
    }

    /// Discard a single DEAD command (the teller gives up on it). Returns true
    /// if a dead command with that id was removed.
    pub fn discard_outbox_item(&self, id: String) -> Result<bool, CoreError> {
        self.store.discard_dead(&id)
    }

    /// Sync health for the action-bar chip + offline banner (counts + online),
    /// in one cheap local read. Always succeeds offline.
    pub fn sync_status(&self) -> Result<SyncStatusView, CoreError> {
        Ok(SyncStatusView {
            pending: self.store.pending_count()?,
            failed: self.store.dead_count()?,
            online: self.current_session().map(|s| s.online).unwrap_or(false),
        })
    }

    /// Server-vs-device clock skew in MINUTES (server minus device, refreshed by
    /// `refresh_connectivity`). The host shows a banner past a threshold so the
    /// teller fixes the clock before offline work is mis-timestamped.
    pub fn clock_skew_minutes(&self) -> i32 {
        (self.clock_skew_secs.load(std::sync::atomic::Ordering::Relaxed) / 60) as i32
    }

    /// Live shift stats (sales total + order count) for the action-bar pill,
    /// derived from the orders the host already loaded via `list_shift_orders`
    /// (synced + queued), voided excluded. Pure — no extra network.
    pub fn shift_stats(&self, orders: Vec<orders::OrderSummaryView>) -> orders::ShiftStatsView {
        orders::shift_stats(&orders)
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl SufrixCore {
    /// Online login (PIN or email). Mints a bearer, mirrors permissions, caches
    /// the org's offline-auth bundle for later offline unlock, and persists the
    /// session to the host vault. Returns `Offline` if disconnected.
    pub async fn login(
        &self,
        req: session::LoginRequest,
    ) -> Result<session::SessionSnapshot, CoreError> {
        use sufrix_api::apis::{auth_api, orgs_api};

        let wire = session::wire_login_request(&req)?;
        let resp = auth_api::login(&self.api.config(), auth_api::LoginParams { login_request: wire })
            .await
            .map_err(net::map_api_error)?;

        // Token is live from here on.
        self.api.set_bearer(Some(resp.token.clone()));

        // PIN login carries the device branch; email login has none.
        let branch_id = req.branch_id.clone();
        let mut snapshot = session::snapshot_from_login(&resp, branch_id);

        // Mirror permissions (best-effort — a perms blip must not void a good login).
        let permissions = match auth_api::get_my_permissions(&self.api.config()).await {
            Ok(p) => {
                snapshot.permissions_loaded = true;
                session::permissions_from(&p)
            }
            Err(_) => Vec::new(),
        };

        // Cache the org's offline-auth bundle so any org teller can unlock offline
        // later (best-effort — failure just means no offline unlock until next login).
        if let Some(org_id) = snapshot.org_id.clone() {
            if let Ok(bundle) = orgs_api::offline_auth_bundle(
                &self.api.config(),
                orgs_api::OfflineAuthBundleParams { id: org_id },
            )
            .await
            {
                session::cache_bundle(&self.store, &bundle, &snapshot);
            }
        }

        let state = session::SessionState {
            snapshot: snapshot.clone(),
            permissions,
            token: Some(resp.token),
        };
        self.persist_and_set(state);
        Ok(snapshot)
    }

    /// One-call sign-in. The online→offline decision lives HERE, not in the host
    /// UI (the One Rule): try an online `login` first; if the network is down and
    /// this is a teller PIN login, fall back to an offline unlock against the
    /// cached org bundle. Validation / auth errors propagate (no fallback) so a
    /// wrong PIN online doesn't silently try the offline path.
    pub async fn sign_in(
        &self,
        req: session::LoginRequest,
    ) -> Result<session::SessionSnapshot, CoreError> {
        // Whether a connectivity failure may fall back to an offline unlock.
        let offline_ok = matches!(req.mode, session::LoginMode::Pin)
            && req.name.is_some()
            && req.pin.is_some()
            && req.branch_id.is_some();
        let offline = |this: &Self| {
            this.unlock_offline(
                req.name.clone().unwrap_or_default(),
                req.pin.clone().unwrap_or_default(),
                req.branch_id.clone().unwrap_or_default(),
            )
        };

        // Hard-bound the online attempt: a black-holed/slow network must never
        // leave the teller on an endless spinner. On timeout → treat as offline.
        let attempt =
            tokio::time::timeout(std::time::Duration::from_secs(7), self.login(req.clone())).await;

        match attempt {
            Ok(Ok(snapshot)) => Ok(snapshot),
            // Connectivity error → offline fallback (PIN only); auth/validation propagate.
            Ok(Err(e)) => match e {
                CoreError::Offline { .. } | CoreError::Transient { .. } if offline_ok => offline(self),
                other => Err(other),
            },
            // Timed out → treat as offline.
            Err(_elapsed) if offline_ok => offline(self),
            Err(_elapsed) => Err(CoreError::Offline {
                detail: "sign-in timed out — check your connection".into(),
            }),
        }
    }

    /// List the org's active branches — for the device-setup picker. Requires a
    /// live (manager) session; online-only.
    pub async fn list_branches(&self) -> Result<Vec<session::BranchView>, CoreError> {
        use sufrix_api::apis::branches_api;
        let (org_id, _) = self.org_branch()?;
        let branches = branches_api::list_branches(
            &self.api.config(),
            branches_api::ListBranchesParams { org_id },
        )
        .await
        .map_err(net::map_api_error)?;
        Ok(branches
            .into_iter()
            .filter(|b| b.is_active)
            .map(|b| session::BranchView { id: b.id.to_string(), name: b.name, is_active: b.is_active })
            .collect())
    }

    /// Pull the branch-effective catalog (items + categories + addons + bundles +
    /// payment methods + discounts) and mirror the canonical JSON into the local
    /// store. Online-only; the offline reads (`list_*`) then serve this mirror.
    /// Atomic-ish: every stream is fetched before any is written, so a mid-pull
    /// failure leaves the previous mirror intact.
    pub async fn refresh_catalog(&self) -> Result<(), CoreError> {
        use sufrix_api::apis::{bundles_api, discounts_api, menu_api, payment_methods_api};
        use sufrix_api::models::BundleStatus;

        let (org_id, branch_id) = self.org_branch()?;

        // Menu items — full, branch-effective shape via raw GET (the typed
        // `list_menu_items` is `Vec<MenuItem>` and would drop sizes/slots).
        let mut q: Vec<(&str, String)> = vec![("org_id", org_id.clone()), ("full", "true".into())];
        if let Some(b) = &branch_id {
            q.push(("branch_id", b.clone()));
        }
        let menu_items_json = self.api.get_text("/menu-items", &q).await?;

        // Addons — the plain `/addon-items` array (NOT `/catalog`): it's
        // branch-effective AND embeds each addon's ingredients (the recipe
        // preview needs them). Raw GET because the embedded `quantity_used` is a
        // BigDecimal string the generated `AddonItem` (f64) can't decode.
        let mut aq: Vec<(&str, String)> = vec![("org_id", org_id.clone())];
        if let Some(b) = &branch_id {
            aq.push(("branch_id", b.clone()));
        }
        let addons_json = self.api.get_text("/addon-items", &aq).await?;

        let categories = menu_api::list_categories(
            &self.api.config(),
            menu_api::ListCategoriesParams { org_id: org_id.clone() },
        )
        .await
        .map_err(net::map_api_error)?;

        let bundles = bundles_api::list_bundles(
            &self.api.config(),
            bundles_api::ListBundlesParams {
                org_id: Some(org_id.clone()),
                status: Some(BundleStatus::Active),
                branch_id: branch_id.clone(),
                search: None,
                page: Some(1),
                per_page: Some(500),
                sort: None,
            },
        )
        .await
        .map_err(net::map_api_error)?;

        let payment_methods = payment_methods_api::list_payment_methods(&self.api.config())
            .await
            .map_err(net::map_api_error)?;

        let discounts = discounts_api::list_discounts(
            &self.api.config(),
            discounts_api::ListDiscountsParams { org_id: org_id.clone() },
        )
        .await
        .map_err(net::map_api_error)?;

        // All streams fetched OK → commit the mirror.
        self.store.kv_put(menu::K_MENU_ITEMS, &menu_items_json)?;
        self.store.kv_put(menu::K_CATEGORIES, &serde_json::to_string(&categories)?)?;
        self.store.kv_put(menu::K_ADDONS, &addons_json)?;
        self.store.kv_put(menu::K_BUNDLES, &serde_json::to_string(&bundles.data)?)?;
        self.store.kv_put(menu::K_PAYMENT_METHODS, &serde_json::to_string(&payment_methods)?)?;
        self.store.kv_put(menu::K_DISCOUNTS, &serde_json::to_string(&discounts)?)?;
        Ok(())
    }

    /// Open a shift. Writes an optimistic local shift + queues an idempotent
    /// open-shift command (client UUID = shift PK), then drains best-effort. The
    /// shift is usable immediately, online or offline. Returns the current shift.
    pub async fn open_shift(
        &self,
        opening_cash_minor: i64,
        edit_reason: Option<String>,
    ) -> Result<shift::ShiftView, CoreError> {
        let (branch_id, teller_id, teller_name) = {
            let g = self.session.read().unwrap_or_else(|e| e.into_inner());
            let s = g.as_ref().ok_or_else(|| CoreError::Unauthenticated {
                detail: "not signed in".into(),
            })?;
            let branch = s.snapshot.branch_id.clone().ok_or_else(|| CoreError::Validation {
                field: "branch_id".into(),
                detail: "session has no branch".into(),
            })?;
            (branch, s.snapshot.user_id.clone(), s.snapshot.display_name.clone())
        };
        let branch_uuid = uuid::Uuid::parse_str(&branch_id)
            .map_err(|_| CoreError::Validation { field: "branch_id".into(), detail: "bad uuid".into() })?;
        let teller_uuid = uuid::Uuid::parse_str(&teller_id)
            .map_err(|_| CoreError::Validation { field: "teller_id".into(), detail: "bad uuid".into() })?;
        let shift_id = uuid::Uuid::new_v4();
        let opened_at = chrono::Utc::now().fixed_offset();
        let opening_cash = opening_cash_minor as i32;
        // A non-empty discrepancy reason ⇒ the teller deviated from the carried-
        // over closing. The server re-derives this authoritatively; we mirror it
        // locally for display and pass the reason through.
        let edit_reason = edit_reason.filter(|r| !r.trim().is_empty());
        let was_edited = edit_reason.is_some();

        // Optimistic local shift — visible immediately on every read.
        let local = sufrix_api::models::Shift {
            branch_id: branch_uuid,
            id: shift_id,
            opened_at,
            opening_cash,
            opening_cash_was_edited: was_edited,
            status: "open".into(),
            teller_id: teller_uuid,
            teller_name,
            ..Default::default()
        };
        shift::save(&self.store, &local)?;

        // Queue the durable command (idempotent on the client shift UUID).
        let request = sufrix_api::models::OpenShiftRequest {
            id: Some(Some(shift_id)),
            opened_at: Some(Some(opened_at)),
            opening_cash,
            edit_reason: edit_reason.map(Some),
            ..Default::default()
        };
        let cmd = shift::OpenShiftCommand { branch_id, request };
        self.store.enqueue(&store::NewOutboxOp {
            id: shift_id.to_string(),
            op_type: "open_shift".into(),
            idempotency_key: shift_id.to_string(),
            payload: serde_json::to_string(&cmd)?,
            event_at: opened_at.to_rfc3339(),
            depends_on_seq: None,
        })?;

        // Best-effort: send now if online (offline just leaves it queued).
        let _ = self.drain_outbox().await;

        shift::current(&self.store)?
            .ok_or_else(|| CoreError::Internal { detail: "shift not persisted".into() })
    }

    /// Close the current open shift: count the closing drawer cash + an optional
    /// note. Marks the shift closed locally (routing flips to open-shift now) and
    /// queues an idempotent `close_shift` command; works offline. Errors if there
    /// is no open shift.
    pub async fn close_shift(
        &self,
        closing_cash_minor: i64,
        cash_note: Option<String>,
    ) -> Result<(), CoreError> {
        let shift = shift::current(&self.store)?
            .filter(|s| s.is_open)
            .ok_or_else(|| CoreError::Validation { field: "shift".into(), detail: "no open shift".into() })?;

        let closed_at = chrono::Utc::now().fixed_offset();
        let mut request = sufrix_api::models::CloseShiftRequest::new(closing_cash_minor as i32);
        request.cash_note = Some(cash_note);
        request.closed_at = Some(Some(closed_at));

        // Optimistic: mark closed locally so routing flips to open-shift now,
        // and drop the in-progress cart (a closed shift sells nothing).
        shift::close_local(&self.store)?;
        cart::clear(&self.store)?;
        // Carry the declared closing into the NEXT shift's suggested opening, so
        // cash continuity holds even before this close syncs.
        shift::cache_suggested_opening_cash(&self.store, closing_cash_minor)?;

        // Queue the durable command. Keyed by `{shift_id}:close` so it doesn't
        // collide with the still-pending open_shift command (id == shift PK).
        let cmd = shift::CloseShiftCommand { shift_id: shift.id.clone(), request };
        self.store.enqueue(&store::NewOutboxOp {
            id: format!("{}:close", shift.id),
            op_type: "close_shift".into(),
            idempotency_key: format!("{}:close", shift.id),
            payload: serde_json::to_string(&cmd)?,
            event_at: closed_at.to_rfc3339(),
            depends_on_seq: None,
        })?;

        // Best-effort: the FIFO drain runs the open + orders before the close,
        // so the close never races ahead of them.
        let _ = self.drain_outbox().await;
        Ok(())
    }

    /// The current shift's report — drives the close-shift system-cash +
    /// discrepancy. Online: the server report plus still-queued cash sales.
    /// Offline / on error: opening cash + queued cash (`from_server = false`).
    pub async fn shift_report(&self) -> Result<shift::ShiftReportView, CoreError> {
        use sufrix_api::apis::shifts_api;
        let shift = shift::current(&self.store)?
            .ok_or_else(|| CoreError::Validation { field: "shift".into(), detail: "no shift".into() })?;
        let queued_cash = checkout::queued_cash_total(&self.store)?;
        let online = self.current_session().map(|s| s.online).unwrap_or(false);
        if online {
            let res = shifts_api::get_shift_report(
                &self.api.config(),
                shifts_api::GetShiftReportParams { shift_id: shift.id.clone() },
            )
            .await;
            if let Ok(report) = res {
                return Ok(shift::report_view(&report, queued_cash));
            }
        }
        Ok(shift::offline_report_view(shift.opening_cash_minor, queued_cash))
    }

    /// Record a cash-drawer movement against the open shift — pay-IN when
    /// `amount_minor > 0`, pay-OUT when `< 0`. ONLINE-ONLY (Flutter parity: cash
    /// movements are never queued, so the drawer total stays authoritative).
    pub async fn record_cash_movement(
        &self,
        amount_minor: i64,
        note: String,
    ) -> Result<shift::CashMovementView, CoreError> {
        use sufrix_api::apis::shifts_api;
        let shift = shift::current(&self.store)?
            .ok_or_else(|| CoreError::Validation { field: "shift".into(), detail: "no shift".into() })?;
        if !self.current_session().map(|s| s.online).unwrap_or(false) {
            return Err(CoreError::Offline { detail: "cash movements need a connection".into() });
        }
        let req = sufrix_api::models::CashMovementRequest::new(amount_minor as i32, note);
        let cm = shifts_api::add_cash_movement(
            &self.api.config(),
            shifts_api::AddCashMovementParams { shift_id: shift.id.clone(), cash_movement_request: req },
        )
        .await
        .map_err(net::map_api_error)?;
        Ok(shift::cash_movement_view(&cm))
    }

    /// Cash movements recorded against the open shift (online read).
    pub async fn list_cash_movements(&self) -> Result<Vec<shift::CashMovementView>, CoreError> {
        use sufrix_api::apis::shifts_api;
        let shift = shift::current(&self.store)?
            .ok_or_else(|| CoreError::Validation { field: "shift".into(), detail: "no shift".into() })?;
        let list = shifts_api::list_cash_movements(
            &self.api.config(),
            shifts_api::ListCashMovementsParams { shift_id: shift.id.clone() },
        )
        .await
        .map_err(net::map_api_error)?;
        Ok(list.iter().map(shift::cash_movement_view).collect())
    }

    /// Past shifts for this branch, newest first (the history screen; online read).
    pub async fn list_shifts(&self) -> Result<Vec<shift::ShiftSummaryView>, CoreError> {
        use sufrix_api::apis::shifts_api;
        let (_, branch_id) = self.org_branch()?;
        let branch = branch_id.unwrap_or_else(|| "00000000-0000-0000-0000-000000000000".into());
        let paginated = shifts_api::list_shifts(
            &self.api.config(),
            shifts_api::ListShiftsParams { branch_id: branch, page: None, per_page: None },
        )
        .await
        .map_err(net::map_api_error)?;
        Ok(paginated.data.iter().map(shift::shift_summary_view).collect())
    }

    /// Place the current cart as an order: price it (client-authoritative),
    /// queue an idempotent `create_order` command, clear the cart, and try to
    /// send now. Works offline — the order stays queued and `queued_offline` is
    /// `true` on the receipt until it syncs. Errors if there's no open shift,
    /// the cart is empty, or the payment method is unknown.
    pub async fn checkout(
        &self,
        input: checkout::CheckoutInput,
    ) -> Result<checkout::ReceiptView, CoreError> {
        let (branch_id, tax_rate, teller_name) = {
            let g = self.session.read().unwrap_or_else(|e| e.into_inner());
            let s = g.as_ref().ok_or_else(|| CoreError::Unauthenticated {
                detail: "not signed in".into(),
            })?;
            let branch = s.snapshot.branch_id.clone().ok_or_else(|| CoreError::Validation {
                field: "branch_id".into(),
                detail: "session has no branch".into(),
            })?;
            (branch, s.snapshot.tax_rate, s.snapshot.display_name.clone())
        };
        let shift = shift::current(&self.store)?
            .filter(|s| s.is_open)
            .ok_or_else(|| CoreError::Validation { field: "shift".into(), detail: "no open shift".into() })?;

        let now = chrono::Utc::now().to_rfc3339();
        let prepared = checkout::prepare(
            &self.store,
            &self.current_locale(),
            &branch_id,
            &shift.id,
            &input,
            tax_rate,
            now,
        )?;

        // Queue the durable command (idempotent on the client order UUID).
        self.store.enqueue(&store::NewOutboxOp {
            id: prepared.order_id.to_string(),
            op_type: "create_order".into(),
            idempotency_key: prepared.order_id.to_string(),
            payload: serde_json::to_string(&prepared.command)?,
            event_at: prepared.event_at.clone(),
            depends_on_seq: None,
        })?;
        // The sale is committed locally; the cart is now spent.
        cart::clear(&self.store)?;

        // Best-effort: send now if online (offline leaves it queued).
        let _ = self.drain_outbox().await;

        // If the order is no longer pending, the drain sent it.
        let order_id = prepared.order_id.to_string();
        let still_pending = self.store.pending()?.iter().any(|i| i.id == order_id);
        let mut receipt = prepared.receipt;
        receipt.queued_offline = still_pending;
        receipt.teller_name = Some(teller_name).filter(|s| !s.trim().is_empty());
        Ok(receipt)
    }

    /// Force a sync now — drains the outbox. Cancellable/idempotent.
    pub async fn sync_now(&self) -> Result<(), CoreError> {
        self.drain_outbox().await
    }

    /// Requeue every dead command (clearing its error) and try to send now.
    /// Best-effort — offline just leaves them pending again.
    pub async fn retry_outbox(&self) -> Result<(), CoreError> {
        self.store.requeue_dead()?;
        self.drain_outbox().await
    }

    /// The connectivity heartbeat: ping the backend, update the live `online`
    /// flag + clock skew, and drain the outbox on success. The host calls this on
    /// foreground + on a timer so the offline/clock-skew banners and the sync
    /// chip reflect reality without waiting for the next deliberate action.
    /// Returns the new online state.
    pub async fn refresh_connectivity(&self) -> bool {
        match self.api.ping().await {
            Ok(skew) => {
                if let Some(s) = skew {
                    self.clock_skew_secs.store(s, std::sync::atomic::Ordering::Relaxed);
                }
                if let Some(sess) = self.session.write().unwrap_or_else(|e| e.into_inner()).as_mut() {
                    sess.snapshot.online = true;
                }
                let _ = self.drain_outbox().await; // best-effort
                true
            }
            Err(_) => {
                if let Some(sess) = self.session.write().unwrap_or_else(|e| e.into_inner()).as_mut() {
                    sess.snapshot.online = false;
                }
                false
            }
        }
    }

    /// The current shift's orders — the still-queued sales (from the outbox,
    /// shown first, always available offline) plus the server's synced orders
    /// when online (best-effort). Errors if there's no current shift.
    pub async fn list_shift_orders(&self) -> Result<Vec<orders::OrderSummaryView>, CoreError> {
        use sufrix_api::apis::orders_api;
        let shift = shift::current(&self.store)?
            .ok_or_else(|| CoreError::Validation { field: "shift".into(), detail: "no shift".into() })?;
        let (branch_id, online) = {
            let g = self.session.read().unwrap_or_else(|e| e.into_inner());
            let s = g.as_ref().ok_or_else(|| CoreError::Unauthenticated { detail: "not signed in".into() })?;
            let b = s.snapshot.branch_id.clone().ok_or_else(|| CoreError::Validation {
                field: "branch_id".into(),
                detail: "session has no branch".into(),
            })?;
            (b, s.snapshot.online)
        };

        // Always show the still-queued sales (offline-safe).
        let mut all = orders::queued(&self.store, &shift.id)?;

        // Add the server's synced orders when online (best-effort — a failure
        // just leaves the queued-only view).
        if online {
            let params = orders_api::ListOrdersParams {
                branch_id: Some(branch_id),
                shift_id: Some(shift.id.clone()),
                updated_after: None,
                page: None,
                per_page: Some(200),
                teller_name: None,
                payment_method: None,
                status: None,
                from: None,
                to: None,
                order_type: None,
                channel: None,
                include_items: None,
            };
            if let Ok(page) = orders_api::list_orders(&self.api.config(), params).await {
                // Overlay an optimistic "voided" status for orders with a queued
                // void command (the void hasn't synced yet).
                let voiding = orders::pending_void_ids(&self.store)?;
                all.extend(page.data.iter().map(|o| {
                    let mut v = orders::from_server(o);
                    if voiding.contains(&v.id) {
                        v.status = "voided".into();
                    }
                    v
                }));
            }
        }
        Ok(all)
    }

    /// Fetch a synced order's full detail (lines + modifiers) — the expanded
    /// history row. Online (the queued ones already have their lines locally).
    pub async fn order_detail(&self, order_id: String) -> Result<orders::OrderDetailView, CoreError> {
        use sufrix_api::apis::orders_api;
        let o = orders_api::get_order(&self.api.config(), orders_api::GetOrderParams { order_id })
            .await
            .map_err(net::map_api_error)?;
        Ok(orders::order_detail_view(&o))
    }

    /// Re-render a synced order as a receipt for reprint — fetches the order and
    /// projects it through the same ESC/POS path as a fresh receipt. Pair with
    /// `send_to_printer`.
    pub async fn render_order_receipt(
        &self,
        order_id: String,
        store_name: String,
        currency: String,
        width: u32,
    ) -> Result<Vec<u8>, CoreError> {
        use sufrix_api::apis::orders_api;
        let o = orders_api::get_order(&self.api.config(), orders_api::GetOrderParams { order_id })
            .await
            .map_err(net::map_api_error)?;
        let receipt = orders::order_to_receipt(&o, &self.current_locale());
        Ok(self.render_receipt(receipt, store_name, currency, width))
    }

    /// Void a synced order (mistake/refund). Queues an idempotent `void_order`
    /// command keyed `{order_id}:void` and tries to send now; works offline.
    /// History reflects it immediately via the pending-void overlay. Only synced
    /// orders (with a server id) can be voided — a queued order isn't on the
    /// server yet.
    pub async fn void_order(
        &self,
        order_id: String,
        reason: String,
        note: Option<String>,
        restore_inventory: bool,
    ) -> Result<(), CoreError> {
        // Must be signed in (the replay needs a token).
        if !self.is_authenticated() {
            return Err(CoreError::Unauthenticated { detail: "not signed in".into() });
        }
        let voided_at = chrono::Utc::now().fixed_offset();
        let mut request = sufrix_api::models::VoidOrderRequest::new(reason);
        request.note = Some(note);
        request.restore_inventory = Some(Some(restore_inventory));
        request.voided_at = Some(Some(voided_at));

        let cmd = orders::VoidOrderCommand { order_id: order_id.clone(), request };
        self.store.enqueue(&store::NewOutboxOp {
            id: format!("{order_id}:void"),
            op_type: "void_order".into(),
            idempotency_key: format!("{order_id}:void"),
            payload: serde_json::to_string(&cmd)?,
            event_at: voided_at.to_rfc3339(),
            depends_on_seq: None,
        })?;
        let _ = self.drain_outbox().await;
        Ok(())
    }

    /// Reconcile the device's shift with the server (online). Caches the server's
    /// open shift, or CLEARS the local cache when the server reports none — e.g.
    /// a dashboard force-close, or a shift opened on another device. The server
    /// is the source of truth when online; call this on login and on app resume.
    pub async fn refresh_shift(&self) -> Result<Option<shift::ShiftView>, CoreError> {
        use sufrix_api::apis::shifts_api;
        let branch_id = {
            let g = self.session.read().unwrap_or_else(|e| e.into_inner());
            let s = g.as_ref().ok_or_else(|| CoreError::Unauthenticated {
                detail: "not signed in".into(),
            })?;
            s.snapshot.branch_id.clone().ok_or_else(|| CoreError::Validation {
                field: "branch_id".into(),
                detail: "session has no branch".into(),
            })?
        };
        let prefill = shifts_api::get_current_shift(
            &self.api.config(),
            shifts_api::GetCurrentShiftParams { branch_id },
        )
        .await
        .map_err(net::map_api_error)?;

        // Refresh the carried-over opening-cash suggestion from the server's
        // prefill (the last *synced* declared closing). Only overwrite with a
        // positive value, so a stale server 0 can't clobber a fresher local
        // close that hasn't synced yet.
        if prefill.suggested_opening_cash > 0 {
            shift::cache_suggested_opening_cash(&self.store, prefill.suggested_opening_cash as i64)?;
        }

        // The server's "no open shift" is only authoritative once our own
        // open_shift command has actually reached it. While it's still queued,
        // the optimistic local shift stands — clearing it here is what bounced
        // the teller straight back to the open-shift screen.
        let open_pending = self.shift_command_pending("open_shift")?;
        let close_pending = self.shift_command_pending("close_shift")?;
        match shift::reconcile(&prefill, open_pending, close_pending) {
            shift::ShiftReconcile::Adopt(server_shift) => {
                shift::save(&self.store, &server_shift)?;
                Ok(Some(shift::view_from(&server_shift)))
            }
            shift::ShiftReconcile::KeepLocal => shift::current(&self.store),
            shift::ShiftReconcile::Clear => {
                shift::clear(&self.store)?;
                Ok(None)
            }
        }
    }

    /// Best-effort raw-TCP send of pre-rendered ESC/POS bytes to a network
    /// (JetDirect / port 9100) thermal printer. Opens a short-lived socket,
    /// writes, flushes. Errors map to `Transient` so the host can offer a retry.
    ///
    /// NOTE: unverifiable here without hardware — the rendered bytes are the
    /// tested contract (`receipt` module); delivery is the host's to confirm.
    pub async fn send_to_printer(&self, host: String, port: u16, bytes: Vec<u8>) -> Result<(), CoreError> {
        use tokio::io::AsyncWriteExt;
        use tokio::time::{timeout, Duration};
        let addr = format!("{host}:{port}");
        let connect = tokio::net::TcpStream::connect(&addr);
        let mut stream = timeout(Duration::from_secs(5), connect)
            .await
            .map_err(|_| CoreError::Transient { detail: format!("printer timeout: {addr}") })?
            .map_err(|e| CoreError::Transient { detail: format!("printer connect: {e}") })?;
        stream
            .write_all(&bytes)
            .await
            .map_err(|e| CoreError::Transient { detail: format!("printer write: {e}") })?;
        stream
            .flush()
            .await
            .map_err(|e| CoreError::Transient { detail: format!("printer flush: {e}") })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greet_includes_version() {
        let msg = greet("Teller".into());
        assert!(msg.contains("Teller"));
        assert!(msg.contains(&core_version()));
    }

    #[test]
    fn core_reads_env_config() {
        let core = SufrixCore::from_env().unwrap();
        assert!(core.base_url().starts_with("http"));
        assert!(!core.environment().is_empty());
        assert_eq!(core.pending_outbox_count().unwrap(), 0);
    }

    #[test]
    fn surface_version_is_pinned() {
        assert_eq!(ffi_surface_version(), 0);
    }

    #[test]
    fn set_locale_changes_strings_and_rtl_at_runtime() {
        let core = SufrixCore::from_env().unwrap();
        core.set_locale("en".into());
        assert_eq!(core.tr("login.sign_in".into()), "Sign in");
        assert!(!core.is_rtl());
        core.set_locale("ar".into());
        assert_eq!(core.tr("login.sign_in".into()), "تسجيل الدخول");
        assert!(core.is_rtl());
        assert_eq!(core.locale(), "ar");
    }

    /// sign_in falls back to an offline unlock when the network is unreachable
    /// and a cached bundle holds the teller's PIN. Points the core at a dead
    /// port so the online `login` fails fast with `Offline`.
    #[tokio::test]
    async fn sign_in_falls_back_to_offline_unlock_when_network_down() {
        use argon2::password_hash::SaltString;
        use argon2::{Argon2, PasswordHasher};

        let core = SufrixCore::new(SufrixConfig {
            base_url: "http://127.0.0.1:1".into(), // nothing listening → connect refused
            environment: "dev".into(),
            db_path: String::new(),
            locale: "en".into(),
        })
        .unwrap();

        // Seed the org bundle the offline unlock verifies against.
        let salt = SaltString::encode_b64(b"sufrix-test-salt").unwrap();
        let phc = Argon2::default().hash_password(b"1234", &salt).unwrap().to_string();
        let bundle = serde_json::json!({
            "org_id": "00000000-0000-0000-0000-0000000000aa",
            "generated_at": "2026-06-19T10:00:00Z",
            "tellers": [{
                "user_id": "00000000-0000-0000-0000-0000000000bb",
                "name": "Sara", "role": "teller", "is_active": true,
                "offline_pin_hash": phc,
            }]
        });
        core.store.kv_put(session::BUNDLE_KEY, &bundle.to_string()).unwrap();
        core.store
            .kv_put(session::ORG_CONFIG_KEY, r#"{"org_id":"00000000-0000-0000-0000-0000000000aa","currency_code":"EGP","tax_rate":0.14}"#)
            .unwrap();

        let req = session::LoginRequest {
            mode: session::LoginMode::Pin,
            name: Some("Sara".into()),
            pin: Some("1234".into()),
            branch_id: Some("00000000-0000-0000-0000-000000000001".into()),
            email: None,
            password: None,
            org_id: None,
        };
        let snap = core.sign_in(req).await.expect("offline fallback should sign in");
        assert_eq!(snap.display_name, "Sara");
        assert!(!snap.online);
        assert!(core.is_authenticated());

        // A wrong PIN offline (still network-down) must NOT sign in.
        let bad = session::LoginRequest {
            pin: Some("0000".into()),
            ..session::LoginRequest {
                mode: session::LoginMode::Pin,
                name: Some("Sara".into()),
                pin: None,
                branch_id: Some("00000000-0000-0000-0000-000000000001".into()),
                email: None, password: None, org_id: None,
            }
        };
        assert!(core.sign_in(bad).await.is_err());
    }
}

/// Routing-lifecycle tests — the class of bug behind the open-shift "bounce".
/// These poke the private session/store (same-crate) to drive `app_route`
/// through every transition without a network.
#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    fn teller_session(user_id: &str, branch: Option<&str>) -> session::SessionState {
        session::SessionState {
            snapshot: session::SessionSnapshot {
                user_id: user_id.into(),
                display_name: "Sara".into(),
                role: "teller".into(),
                org_id: Some("org-1".into()),
                branch_id: branch.map(Into::into),
                currency_code: "EGP".into(),
                tax_rate: 0.14,
                online: true,
                permissions_loaded: true,
            },
            permissions: vec![],
            token: None,
        }
    }

    fn set_session(core: &SufrixCore, state: Option<session::SessionState>) {
        *core.session.write().unwrap_or_else(|e| e.into_inner()) = state;
    }

    fn seed_shift(core: &SufrixCore, teller: uuid::Uuid, status: &str) {
        let _ = seed_shift_returning_id(core, teller, status);
    }

    fn seed_shift_returning_id(core: &SufrixCore, teller: uuid::Uuid, status: &str) -> String {
        let id = uuid::Uuid::new_v4();
        let s = sufrix_api::models::Shift {
            id,
            branch_id: uuid::Uuid::new_v4(),
            teller_id: teller,
            teller_name: "Sara".into(),
            opening_cash: 50000,
            status: status.into(),
            ..Default::default()
        };
        shift::save(&core.store, &s).unwrap();
        id.to_string()
    }

    fn enqueue_open_shift(core: &SufrixCore, id: &str) {
        core.store
            .enqueue(&store::NewOutboxOp {
                id: id.into(),
                op_type: "open_shift".into(),
                idempotency_key: id.into(),
                payload: "{}".into(),
                event_at: "2026-06-20T12:00:00+00:00".into(),
                depends_on_seq: None,
            })
            .unwrap();
    }

    /// Skeptic-1 regression: the open-shift pending guard must be scoped to the
    /// cached shift's id, NOT device-global. A foreign teller's orphaned command
    /// (left in the shared outbox after sign-out) must not keep a shift alive.
    #[test]
    fn open_pending_is_scoped_to_the_cached_shift_not_device_global() {
        let core = SufrixCore::from_env().unwrap();
        let shift_id = seed_shift_returning_id(&core, uuid::Uuid::new_v4(), "open");
        assert!(!core.shift_command_pending("open_shift").unwrap()); // nothing queued
        // A DIFFERENT shift's orphaned open_shift command does NOT count.
        enqueue_open_shift(&core, &uuid::Uuid::new_v4().to_string());
        assert!(!core.shift_command_pending("open_shift").unwrap());
        // Our own cached shift's command DOES.
        enqueue_open_shift(&core, &shift_id);
        assert!(core.shift_command_pending("open_shift").unwrap());
    }

    /// End-to-end offline: open a shift, sell nothing, then close it. The shift
    /// flips to closed locally (route → open-shift) and the close command queues
    /// behind the open (FIFO); the cart is dropped.
    #[tokio::test]
    async fn closing_a_shift_offline_routes_back_to_open_shift() {
        use argon2::password_hash::SaltString;
        use argon2::{Argon2, PasswordHasher};

        let core = SufrixCore::new(SufrixConfig {
            base_url: "http://127.0.0.1:1".into(),
            environment: "dev".into(),
            db_path: String::new(),
            locale: "en".into(),
        })
        .unwrap();
        let salt = SaltString::encode_b64(b"sufrix-test-salt").unwrap();
        let phc = Argon2::default().hash_password(b"1234", &salt).unwrap().to_string();
        core.store
            .kv_put(
                session::BUNDLE_KEY,
                &serde_json::json!({
                    "org_id": "00000000-0000-0000-0000-0000000000aa",
                    "generated_at": "2026-06-19T10:00:00Z",
                    "tellers": [{ "user_id": "00000000-0000-0000-0000-0000000000bb",
                        "name": "Sara", "role": "teller", "is_active": true, "offline_pin_hash": phc }]
                })
                .to_string(),
            )
            .unwrap();
        core.store
            .kv_put(session::ORG_CONFIG_KEY, r#"{"org_id":"00000000-0000-0000-0000-0000000000aa","currency_code":"EGP","tax_rate":0.14}"#)
            .unwrap();
        core.sign_in(session::LoginRequest {
            mode: session::LoginMode::Pin,
            name: Some("Sara".into()),
            pin: Some("1234".into()),
            branch_id: Some("00000000-0000-0000-0000-000000000001".into()),
            email: None,
            password: None,
            org_id: None,
        })
        .await
        .unwrap();

        core.open_shift(50000, None).await.unwrap();
        core.cart_add("item-1".into(), "Latte".into(), 1000).unwrap();
        assert_eq!(core.app_route(true, false), AppRoute::Order);

        core.close_shift(48000, Some("short by 20".into())).await.unwrap();
        // Routed back to open-shift, cart dropped, and both commands queued.
        assert_eq!(core.app_route(true, false), AppRoute::OpenShift);
        assert!(core.cart_lines().unwrap().is_empty());
        assert_eq!(core.pending_outbox_count().unwrap(), 2); // open + close
        assert!(core.shift_command_pending("close_shift").unwrap());
    }

    #[tokio::test]
    async fn close_shift_without_an_open_shift_is_rejected() {
        let core = SufrixCore::from_env().unwrap();
        let err = core.close_shift(1000, None).await;
        assert!(matches!(err, Err(CoreError::Validation { .. })));
    }

    #[test]
    fn route_device_setup_until_branch_bound() {
        let core = SufrixCore::from_env().unwrap();
        assert_eq!(core.app_route(false, false), AppRoute::DeviceSetup); // unconfigured
        assert_eq!(core.app_route(true, true), AppRoute::DeviceSetup); // reconfiguring
    }

    #[test]
    fn route_login_when_configured_but_signed_out() {
        let core = SufrixCore::from_env().unwrap();
        assert_eq!(core.app_route(true, false), AppRoute::Login);
    }

    #[test]
    fn route_open_shift_when_signed_in_without_a_shift() {
        let core = SufrixCore::from_env().unwrap();
        set_session(&core, Some(teller_session(&uuid::Uuid::new_v4().to_string(), Some("b"))));
        assert_eq!(core.app_route(true, false), AppRoute::OpenShift);
    }

    #[test]
    fn route_order_when_own_shift_is_open() {
        // The regression: a teller's own open shift routes to Order and STAYS.
        let core = SufrixCore::from_env().unwrap();
        let teller = uuid::Uuid::new_v4();
        set_session(&core, Some(teller_session(&teller.to_string(), Some("b"))));
        seed_shift(&core, teller, "open");
        assert_eq!(core.app_route(true, false), AppRoute::Order);
    }

    #[test]
    fn route_open_shift_for_a_foreign_tellers_shift() {
        // A stale shift left by a DIFFERENT teller must not route the new one in.
        let core = SufrixCore::from_env().unwrap();
        let me = uuid::Uuid::new_v4();
        set_session(&core, Some(teller_session(&me.to_string(), Some("b"))));
        seed_shift(&core, uuid::Uuid::new_v4(), "open");
        assert_eq!(core.app_route(true, false), AppRoute::OpenShift);
    }

    #[test]
    fn route_open_shift_when_the_shift_is_closed() {
        let core = SufrixCore::from_env().unwrap();
        let teller = uuid::Uuid::new_v4();
        set_session(&core, Some(teller_session(&teller.to_string(), Some("b"))));
        seed_shift(&core, teller, "closed");
        assert_eq!(core.app_route(true, false), AppRoute::OpenShift);
    }

    /// End-to-end offline: sign in offline, open a shift (the open_shift command
    /// can't reach the server, so it stays queued), and assert the route lands —
    /// and STAYS — on Order. This is the open-shift "bounce" reproduced E2E.
    #[tokio::test]
    async fn opening_a_shift_offline_routes_to_order_and_stays() {
        use argon2::password_hash::SaltString;
        use argon2::{Argon2, PasswordHasher};

        let core = SufrixCore::new(SufrixConfig {
            base_url: "http://127.0.0.1:1".into(), // nothing listening → offline
            environment: "dev".into(),
            db_path: String::new(),
            locale: "en".into(),
        })
        .unwrap();

        let salt = SaltString::encode_b64(b"sufrix-test-salt").unwrap();
        let phc = Argon2::default().hash_password(b"1234", &salt).unwrap().to_string();
        let bundle = serde_json::json!({
            "org_id": "00000000-0000-0000-0000-0000000000aa",
            "generated_at": "2026-06-19T10:00:00Z",
            "tellers": [{
                "user_id": "00000000-0000-0000-0000-0000000000bb",
                "name": "Sara", "role": "teller", "is_active": true,
                "offline_pin_hash": phc,
            }]
        });
        core.store.kv_put(session::BUNDLE_KEY, &bundle.to_string()).unwrap();
        core.store
            .kv_put(session::ORG_CONFIG_KEY, r#"{"org_id":"00000000-0000-0000-0000-0000000000aa","currency_code":"EGP","tax_rate":0.14}"#)
            .unwrap();

        let branch = "00000000-0000-0000-0000-000000000001";
        let snap = core
            .sign_in(session::LoginRequest {
                mode: session::LoginMode::Pin,
                name: Some("Sara".into()),
                pin: Some("1234".into()),
                branch_id: Some(branch.into()),
                email: None,
                password: None,
                org_id: None,
            })
            .await
            .expect("offline sign-in");
        assert!(!snap.online);
        // Signed in, no shift yet → open-shift.
        assert_eq!(core.app_route(true, false), AppRoute::OpenShift);

        let shift = core.open_shift(50000, None).await.expect("open shift offline");
        assert!(shift.is_open);
        // The command is queued (couldn't reach the server)…
        assert_eq!(core.pending_outbox_count().unwrap(), 1);
        // …and the route is Order — and stays there (the bounce is gone).
        assert_eq!(core.app_route(true, false), AppRoute::Order);
        assert_eq!(core.app_route(true, false), AppRoute::Order);
    }

    #[test]
    fn cart_totals_use_the_session_tax_rate() {
        let core = SufrixCore::from_env().unwrap();
        set_session(&core, Some(teller_session(&uuid::Uuid::new_v4().to_string(), Some("b"))));
        core.cart_add("item-1".into(), "Latte".into(), 1000).unwrap();
        core.cart_add("item-1".into(), "Latte".into(), 1000).unwrap(); // qty 2
        let t = core.cart_totals().unwrap();
        assert_eq!(t.item_count, 2);
        assert_eq!(t.subtotal_minor, 2000);
        assert_eq!(t.tax_minor, 280); // 0.14 * 2000
        assert_eq!(t.total_minor, 2280);
    }

    #[test]
    fn cart_totals_are_tax_free_when_signed_out() {
        let core = SufrixCore::from_env().unwrap();
        core.cart_add("i".into(), "X".into(), 1000).unwrap();
        let t = core.cart_totals().unwrap();
        assert_eq!(t.tax_minor, 0);
        assert_eq!(t.total_minor, 1000);
    }
}
