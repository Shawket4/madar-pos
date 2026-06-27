//! Core configuration.
//!
//! Base URL and environment come from the Rust-side `.env`, baked in at build
//! time by `build.rs` — hosts never inject endpoint/base-url knowledge. Hosts
//! pass only runtime knobs (where to put the SQLite file).

/// Runtime configuration handed to [`crate::MadarCore`].
#[derive(Debug, Clone, uniffi::Record)]
pub struct MadarConfig {
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

impl MadarConfig {
    /// Build config from the compiled-in `.env` values, with safe fallbacks.
    /// `db_path` is left empty for the host to fill in (it's a plain Record —
    /// the host sets the field directly when constructing).
    pub fn from_env() -> Self {
        Self {
            base_url: option_env!("MADAR_BASE_URL")
                .unwrap_or("https://sufrix.duckdns.org")
                .to_string(),
            environment: option_env!("MADAR_ENV").unwrap_or("prod").to_string(),
            db_path: String::new(),
            locale: "en".to_string(),
        }
    }
}

/// Hand the host the baked-in `.env` defaults as a `MadarConfig` Record it can
/// tweak (e.g. fill `db_path`) before passing to [`crate::MadarCore::new`].
/// (Records can't carry exported methods, so this is a free function.)
#[uniffi::export]
pub fn default_config() -> MadarConfig {
    MadarConfig::from_env()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_env_db_path_is_empty_for_host_to_fill() {
        // The host fills `db_path`; the env defaults must leave it blank
        // (empty => in-memory store).
        let c = MadarConfig::from_env();
        assert!(c.db_path.is_empty());
    }

    #[test]
    fn from_env_defaults_locale_to_en() {
        assert_eq!(MadarConfig::from_env().locale, "en");
    }

    #[test]
    fn from_env_base_url_is_an_http_url() {
        // Either the baked-in MADAR_BASE_URL or the hard-coded fallback — both
        // are absolute http(s) URLs.
        let c = MadarConfig::from_env();
        assert!(c.base_url.starts_with("http"), "base_url = {}", c.base_url);
    }

    #[test]
    fn from_env_environment_is_non_empty() {
        // Baked-in MADAR_ENV or the "prod" fallback; never blank.
        assert!(!MadarConfig::from_env().environment.is_empty());
    }

    #[test]
    fn default_config_equals_from_env_field_by_field() {
        // `default_config` is the FFI wrapper over `from_env` — they must agree on
        // every field so the host and Rust see the same defaults.
        let a = default_config();
        let b = MadarConfig::from_env();
        assert_eq!(a.base_url, b.base_url);
        assert_eq!(a.environment, b.environment);
        assert_eq!(a.db_path, b.db_path);
        assert_eq!(a.locale, b.locale);
    }

    #[test]
    fn config_is_cloneable_and_field_writable() {
        // It's a plain Record: the host mutates fields directly (e.g. db_path).
        let mut c = MadarConfig::from_env();
        c.db_path = "/tmp/sufrix.sqlite".to_string();
        c.locale = "ar".to_string();
        let cloned = c.clone();
        assert_eq!(cloned.db_path, "/tmp/sufrix.sqlite");
        assert_eq!(cloned.locale, "ar");
    }
}
