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

/// The coarse FFI error model the host reacts to (PLAN §7.6).
pub mod error;
/// Menu / catalog reads — branch-effective mirror + view DTOs (PLAN §R9).
pub mod menu;
/// HTTP layer — drives the generated `sufrix-api` reqwest client (PLAN §R4 net/).
pub mod net;
/// Session & auth — online login, offline unlock, token custody (PLAN §7.2).
pub mod session;
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

    /// Sign out: drop the live session + token, clear the host vault. Preserves
    /// the outbox unless `wipe_outbox` (offline shifts are real sales — only wipe
    /// on an explicit destructive sign-out).
    pub fn logout(&self, wipe_outbox: bool) -> Result<(), CoreError> {
        self.api.set_bearer(None);
        *self.session.write().unwrap_or_else(|e| e.into_inner()) = None;
        if let Some(ts) = self.token_store.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            ts.clear_blob();
        }
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
}
