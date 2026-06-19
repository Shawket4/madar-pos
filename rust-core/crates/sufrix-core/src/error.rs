//! The single, coarse FFI error model (PLAN §7.6). The variant tells the host
//! how to *react*; rich diagnostics ride as fields. Every wire quirk (open
//! enums, int splits, BigDecimal-as-string, multipart) is absorbed *below* this
//! boundary — hosts only ever see a `CoreError`.

#[derive(uniffi::Error, Debug, thiserror::Error)]
pub enum CoreError {
    /// An online-only op was attempted while disconnected. Hot-path *commands*
    /// never return this — they queue to the outbox instead.
    #[error("offline: {message}")]
    Offline { message: String },
    /// 401 + refresh failed → the host should surface sign-in.
    #[error("auth required: {message}")]
    Unauthenticated { message: String },
    #[error("forbidden: {resource}/{action}")]
    Forbidden { resource: String, action: String },
    /// Local validation: mode invariants, empty cart, future-dated event, …
    #[error("invalid: {field}: {message}")]
    Validation { field: String, message: String },
    #[error("server {status}: {code}")]
    Server { status: u16, code: String, message: String },
    /// 5xx / timeout — sync already retries; informational for the host.
    #[error("transient: {message}")]
    Transient { message: String },
    /// Store/migration/serde, or an FFI-version mismatch.
    #[error("internal: {message}")]
    Internal { message: String },
}

pub type CoreResult<T> = Result<T, CoreError>;

impl From<rusqlite::Error> for CoreError {
    fn from(e: rusqlite::Error) -> Self {
        CoreError::Internal { message: format!("db: {e}") }
    }
}
impl From<serde_json::Error> for CoreError {
    fn from(e: serde_json::Error) -> Self {
        CoreError::Internal { message: format!("serde: {e}") }
    }
}
