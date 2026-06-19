//! Core configuration.
//!
//! Base URL and environment come from the Rust-side `.env`, baked in at build
//! time by `build.rs` — hosts never inject endpoint/base-url knowledge. Hosts
//! pass only runtime knobs (where to put the SQLite file).

/// Runtime configuration handed to [`crate::SufrixCore`].
#[derive(Debug, Clone, uniffi::Record)]
pub struct SufrixConfig {
    /// API base URL, e.g. `https://api.sufrix.app`.
    pub base_url: String,
    /// Environment name: `prod` | `staging` | `dev`.
    pub environment: String,
    /// Absolute path to the SQLite store (host app sandbox dir). Empty string
    /// means in-memory — used by tests and on first boot before a path exists.
    pub db_path: String,
    /// BCP-47 locale (e.g. `ar-EG`, `en`) used to resolve `*_translations` to a
    /// single display string in the read DTOs. The host passes the device locale.
    pub locale: String,
}

impl SufrixConfig {
    /// Build config from the compiled-in `.env` values, with safe fallbacks.
    /// `db_path` is left empty for the host to fill in (it's a plain Record —
    /// the host sets the field directly when constructing).
    pub fn from_env() -> Self {
        Self {
            base_url: option_env!("SUFRIX_BASE_URL")
                .unwrap_or("https://sufrix.duckdns.org")
                .to_string(),
            environment: option_env!("SUFRIX_ENV").unwrap_or("prod").to_string(),
            db_path: String::new(),
            locale: "en".to_string(),
        }
    }
}

/// Hand the host the baked-in `.env` defaults as a `SufrixConfig` Record it can
/// tweak (e.g. fill `db_path`) before passing to [`crate::SufrixCore::new`].
/// (Records can't carry exported methods, so this is a free function.)
#[uniffi::export]
pub fn default_config() -> SufrixConfig {
    SufrixConfig::from_env()
}
