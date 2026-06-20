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
    /// HTTP client to the backend (holds the live bearer token).
    api: net::ApiClient,
    /// The live session (`None` = signed out). Set by login / offline unlock /
    /// cold-start restore; cleared on logout.
    session: RwLock<Option<session::SessionState>>,
    /// The host's secure-bytes vault for the session blob (Keychain/Keystore).
    token_store: Mutex<Option<Box<dyn session::TokenStore>>>,
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
        Ok(Arc::new(Self {
            config,
            store,
            api,
            session: RwLock::new(None),
            token_store: Mutex::new(None),
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

    /// `(org_id, branch_id)` from the live session — needed for branch-effective
    /// catalog fetches. Errors if signed out / no org.
    fn org_branch(&self) -> Result<(String, Option<String>), CoreError> {
        let g = self.session.read().unwrap_or_else(|e| e.into_inner());
        let s = g.as_ref().ok_or_else(|| CoreError::Unauthenticated {
            message: "not signed in".into(),
        })?;
        let org = s.snapshot.org_id.clone().ok_or_else(|| CoreError::Validation {
            field: "org_id".into(),
            message: "session has no org".into(),
        })?;
        Ok((org, s.snapshot.branch_id.clone()))
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
        i18n::tr(&self.config.locale, &key)
    }
    /// The active locale (BCP-47).
    pub fn locale(&self) -> String {
        self.config.locale.clone()
    }
    /// Whether the locale is right-to-left (host flips layout direction).
    pub fn is_rtl(&self) -> bool {
        i18n::is_rtl(&self.config.locale)
    }
}

// ── catalog reads (sync; serve the local mirror, always succeed offline) ─────
#[uniffi::export]
impl SufrixCore {
    pub fn list_menu_items(&self) -> Result<Vec<menu::MenuItemView>, CoreError> {
        menu::menu_items(&self.store, &self.config.locale)
    }
    pub fn list_categories(&self) -> Result<Vec<menu::CategoryView>, CoreError> {
        menu::categories(&self.store, &self.config.locale)
    }
    pub fn list_addon_catalog(&self) -> Result<Vec<menu::AddonItemView>, CoreError> {
        menu::addons(&self.store, &self.config.locale)
    }
    pub fn available_bundles(&self) -> Result<Vec<menu::BundleView>, CoreError> {
        menu::bundles(&self.store, &self.config.locale)
    }
    pub fn list_payment_methods(&self) -> Result<Vec<menu::PaymentMethodView>, CoreError> {
        menu::payment_methods(&self.store, &self.config.locale)
    }
    pub fn list_discounts(&self) -> Result<Vec<menu::DiscountView>, CoreError> {
        menu::discounts(&self.store, &self.config.locale)
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
    /// Set a line's absolute quantity; `qty <= 0` removes the line.
    pub fn cart_set_qty(
        &self,
        item_id: String,
        qty: i64,
    ) -> Result<Vec<cart::CartLineView>, CoreError> {
        cart::set_qty(&self.store, &item_id, qty)
    }
    /// Remove a line entirely.
    pub fn cart_remove(&self, item_id: String) -> Result<Vec<cart::CartLineView>, CoreError> {
        cart::remove(&self.store, &item_id)
    }
    /// Empty the cart.
    pub fn cart_clear(&self) -> Result<(), CoreError> {
        cart::clear(&self.store)
    }
    /// Priced cart summary at the session's org tax rate (0 when signed out),
    /// computed through the pricing engine.
    pub fn cart_totals(&self) -> Result<cart::CartTotals, CoreError> {
        let tax_rate = self.current_session().map(|s| s.tax_rate).unwrap_or(0.0);
        cart::totals(&self.store, tax_rate)
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
                message: "sign-in timed out — check your connection".into(),
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

        let categories = menu_api::list_categories(
            &self.api.config(),
            menu_api::ListCategoriesParams { org_id: org_id.clone() },
        )
        .await
        .map_err(net::map_api_error)?;

        let addons = menu_api::list_addon_catalog(
            &self.api.config(),
            menu_api::ListAddonCatalogParams {
                org_id: org_id.clone(),
                addon_type: None,
                search: None,
                page: Some(1),
                per_page: Some(500),
                branch_id: branch_id.clone(),
                overridden: None,
                sort: None,
            },
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
        self.store.kv_put(menu::K_ADDONS, &serde_json::to_string(&addons.data)?)?;
        self.store.kv_put(menu::K_BUNDLES, &serde_json::to_string(&bundles.data)?)?;
        self.store.kv_put(menu::K_PAYMENT_METHODS, &serde_json::to_string(&payment_methods)?)?;
        self.store.kv_put(menu::K_DISCOUNTS, &serde_json::to_string(&discounts)?)?;
        Ok(())
    }

    /// Open a shift. Writes an optimistic local shift + queues an idempotent
    /// open-shift command (client UUID = shift PK), then drains best-effort. The
    /// shift is usable immediately, online or offline. Returns the current shift.
    pub async fn open_shift(&self, opening_cash_minor: i64) -> Result<shift::ShiftView, CoreError> {
        let (branch_id, teller_id, teller_name) = {
            let g = self.session.read().unwrap_or_else(|e| e.into_inner());
            let s = g.as_ref().ok_or_else(|| CoreError::Unauthenticated {
                message: "not signed in".into(),
            })?;
            let branch = s.snapshot.branch_id.clone().ok_or_else(|| CoreError::Validation {
                field: "branch_id".into(),
                message: "session has no branch".into(),
            })?;
            (branch, s.snapshot.user_id.clone(), s.snapshot.display_name.clone())
        };
        let branch_uuid = uuid::Uuid::parse_str(&branch_id)
            .map_err(|_| CoreError::Validation { field: "branch_id".into(), message: "bad uuid".into() })?;
        let teller_uuid = uuid::Uuid::parse_str(&teller_id)
            .map_err(|_| CoreError::Validation { field: "teller_id".into(), message: "bad uuid".into() })?;
        let shift_id = uuid::Uuid::new_v4();
        let opened_at = chrono::Utc::now().fixed_offset();
        let opening_cash = opening_cash_minor as i32;

        // Optimistic local shift — visible immediately on every read.
        let local = sufrix_api::models::Shift {
            branch_id: branch_uuid,
            id: shift_id,
            opened_at,
            opening_cash,
            opening_cash_was_edited: false,
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
            .ok_or_else(|| CoreError::Internal { message: "shift not persisted".into() })
    }

    /// Place the current cart as an order: price it (client-authoritative),
    /// queue an idempotent `create_order` command, clear the cart, and try to
    /// send now. Works offline — the order stays queued and `queued_offline` is
    /// `true` on the receipt until it syncs. Errors if there's no open shift,
    /// the cart is empty, or the payment method is unknown.
    pub async fn checkout(
        &self,
        payment_method_id: String,
        amount_tendered_minor: i64,
    ) -> Result<checkout::ReceiptView, CoreError> {
        let (branch_id, tax_rate) = {
            let g = self.session.read().unwrap_or_else(|e| e.into_inner());
            let s = g.as_ref().ok_or_else(|| CoreError::Unauthenticated {
                message: "not signed in".into(),
            })?;
            let branch = s.snapshot.branch_id.clone().ok_or_else(|| CoreError::Validation {
                field: "branch_id".into(),
                message: "session has no branch".into(),
            })?;
            (branch, s.snapshot.tax_rate)
        };
        let shift = shift::current(&self.store)?
            .filter(|s| s.is_open)
            .ok_or_else(|| CoreError::Validation { field: "shift".into(), message: "no open shift".into() })?;

        let now = chrono::Utc::now().to_rfc3339();
        let prepared = checkout::prepare(
            &self.store,
            &self.config.locale,
            &branch_id,
            &shift.id,
            &payment_method_id,
            amount_tendered_minor,
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
        Ok(receipt)
    }

    /// Force a sync now — drains the outbox. Cancellable/idempotent.
    pub async fn sync_now(&self) -> Result<(), CoreError> {
        self.drain_outbox().await
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
                message: "not signed in".into(),
            })?;
            s.snapshot.branch_id.clone().ok_or_else(|| CoreError::Validation {
                field: "branch_id".into(),
                message: "session has no branch".into(),
            })?
        };
        let prefill = shifts_api::get_current_shift(
            &self.api.config(),
            shifts_api::GetCurrentShiftParams { branch_id },
        )
        .await
        .map_err(net::map_api_error)?;

        if prefill.has_open_shift {
            if let Some(Some(server_shift)) = prefill.open_shift {
                shift::save(&self.store, &server_shift)?;
                return Ok(Some(shift::view_from(&server_shift)));
            }
        }
        shift::clear(&self.store)?;
        Ok(None)
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
