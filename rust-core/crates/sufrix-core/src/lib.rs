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
/// Local store — SQLite mirror + durable outbox + id_map + sync cursors (PLAN §8).
pub mod store;

use std::sync::Arc;

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
}

#[uniffi::export]
impl SufrixCore {
    /// Construct with explicit config (the host fills `db_path` with an
    /// app-private file). Opens + migrates the local store.
    #[uniffi::constructor]
    pub fn new(config: SufrixConfig) -> Result<Arc<Self>, error::CoreError> {
        let store = store::Store::open(&config.db_path)?;
        Ok(Arc::new(Self { config, store }))
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
