//! The single, coarse FFI error model (PLAN §7.6). The variant tells the host
//! how to *react*; rich diagnostics ride as fields. Every wire quirk (open
//! enums, int splits, BigDecimal-as-string, multipart) is absorbed *below* this
//! boundary — hosts only ever see a `CoreError`.

#[derive(uniffi::Error, Debug, thiserror::Error)]
pub enum CoreError {
    /// An online-only op was attempted while disconnected. Hot-path *commands*
    /// never return this — they queue to the outbox instead.
    #[error("offline: {detail}")]
    Offline { detail: String },
    /// 401 + refresh failed → the host should surface sign-in.
    #[error("auth required: {detail}")]
    Unauthenticated { detail: String },
    #[error("forbidden: {resource}/{action}")]
    Forbidden { resource: String, action: String },
    /// Local validation: mode invariants, empty cart, future-dated event, …
    #[error("invalid: {field}: {detail}")]
    Validation { field: String, detail: String },
    #[error("server {status}: {code}")]
    Server { status: u16, code: String, detail: String },
    /// 5xx / timeout — sync already retries; informational for the host.
    #[error("transient: {detail}")]
    Transient { detail: String },
    /// Store/migration/serde, or an FFI-version mismatch.
    #[error("internal: {detail}")]
    Internal { detail: String },
}

pub type CoreResult<T> = Result<T, CoreError>;

impl From<rusqlite::Error> for CoreError {
    fn from(e: rusqlite::Error) -> Self {
        CoreError::Internal { detail: format!("db: {e}") }
    }
}
impl From<serde_json::Error> for CoreError {
    fn from(e: serde_json::Error) -> Self {
        CoreError::Internal { detail: format!("serde: {e}") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Display formatting per variant (the host renders these) ──────────

    #[test]
    fn offline_display() {
        let e = CoreError::Offline { detail: "no network".into() };
        assert_eq!(e.to_string(), "offline: no network");
    }

    #[test]
    fn unauthenticated_display() {
        let e = CoreError::Unauthenticated { detail: "token expired".into() };
        assert_eq!(e.to_string(), "auth required: token expired");
    }

    #[test]
    fn forbidden_display_includes_resource_and_action() {
        let e = CoreError::Forbidden { resource: "orders".into(), action: "void".into() };
        assert_eq!(e.to_string(), "forbidden: orders/void");
    }

    #[test]
    fn validation_display_includes_field_and_detail() {
        let e = CoreError::Validation { field: "pin".into(), detail: "is required".into() };
        assert_eq!(e.to_string(), "invalid: pin: is required");
    }

    #[test]
    fn server_display_includes_status_and_code() {
        let e = CoreError::Server { status: 404, code: "not_found".into(), detail: "missing".into() };
        // detail is intentionally not part of the Display string.
        assert_eq!(e.to_string(), "server 404: not_found");
    }

    #[test]
    fn transient_display() {
        let e = CoreError::Transient { detail: "504 gateway timeout".into() };
        assert_eq!(e.to_string(), "transient: 504 gateway timeout");
    }

    #[test]
    fn internal_display() {
        let e = CoreError::Internal { detail: "migration failed".into() };
        assert_eq!(e.to_string(), "internal: migration failed");
    }

    #[test]
    fn empty_detail_still_formats() {
        let e = CoreError::Offline { detail: String::new() };
        assert_eq!(e.to_string(), "offline: ");
    }

    // ── Error trait + Debug ──────────────────────────────────────────────

    #[test]
    fn implements_std_error_trait() {
        fn assert_error<T: std::error::Error>(_: &T) {}
        assert_error(&CoreError::Internal { detail: "x".into() });
    }

    #[test]
    fn debug_is_available_and_names_variant() {
        let e = CoreError::Forbidden { resource: "r".into(), action: "a".into() };
        let dbg = format!("{e:?}");
        assert!(dbg.contains("Forbidden"));
        assert!(dbg.contains("\"r\""));
        assert!(dbg.contains("\"a\""));
    }

    // ── From conversions ─────────────────────────────────────────────────

    #[test]
    fn from_serde_json_error_maps_to_internal() {
        let err: serde_json::Error = serde_json::from_str::<i32>("not json").unwrap_err();
        let core: CoreError = err.into();
        match core {
            CoreError::Internal { detail } => assert!(detail.starts_with("serde: ")),
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn from_rusqlite_error_maps_to_internal() {
        let err = rusqlite::Error::QueryReturnedNoRows;
        let core: CoreError = err.into();
        match core {
            CoreError::Internal { detail } => assert!(detail.starts_with("db: ")),
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn question_mark_operator_converts_serde_error() {
        // Exercise the `?`-driven From path through CoreResult.
        fn parse() -> CoreResult<i32> {
            let n: i32 = serde_json::from_str("oops")?;
            Ok(n)
        }
        assert!(matches!(parse(), Err(CoreError::Internal { .. })));
    }
}
