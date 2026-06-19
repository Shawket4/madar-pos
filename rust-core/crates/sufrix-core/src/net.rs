//! HTTP layer — the one place the typed `sufrix-api` reqwest client is driven
//! (PLAN §R4 `net/`). Everything above this boundary speaks `CoreError`, never a
//! reqwest/serde/HTTP detail.
//!
//! Responsibilities:
//!   - own the shared connection-pooled `reqwest::Client` (one per core),
//!   - mint a `sufrix_api::Configuration` per call with the live bearer token,
//!   - translate the generated client's `Error<T>` into the coarse `CoreError`
//!     the host reacts to (§7.6).
//!
//! Idempotency-key injection (X-Idempotency-Key on outbox-mutation replays)
//! lands with the cart/checkout module that needs it — the header name is
//! verified against the backend there, not guessed here.

use std::sync::RwLock;
use std::time::Duration;

use sufrix_api::apis::configuration::Configuration;
use sufrix_api::apis::Error as ApiError;

use crate::error::{CoreError, CoreResult};

/// The single HTTP client the core talks to the backend through. Cheap to clone
/// the inner `reqwest::Client` (it's `Arc`-backed), so every call gets a fresh
/// `Configuration` over the same pool.
pub struct ApiClient {
    base_url: String,
    user_agent: String,
    http: reqwest::Client,
    /// Live access token, swapped on login / refresh / logout. `None` = no
    /// bearer (unauthenticated or offline-unlocked).
    bearer: RwLock<Option<String>>,
}

impl ApiClient {
    pub fn new(base_url: String) -> CoreResult<Self> {
        let user_agent = format!("sufrix-core/{}", env!("CARGO_PKG_VERSION"));
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(30))
            .user_agent(user_agent.clone())
            .build()
            .map_err(|e| CoreError::Internal { message: format!("http client: {e}") })?;
        Ok(Self {
            base_url,
            user_agent,
            http,
            bearer: RwLock::new(None),
        })
    }

    /// Swap the live access token (login/refresh sets `Some`, logout sets `None`).
    pub fn set_bearer(&self, token: Option<String>) {
        *self.bearer.write().unwrap_or_else(|e| e.into_inner()) = token;
    }

    pub fn has_bearer(&self) -> bool {
        self.bearer.read().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    /// Authenticated raw GET returning the response body as text. Used where the
    /// generated typed call has the wrong return type for the shape we need —
    /// e.g. `GET /menu-items?full=true` returns the rich `MenuItemFull` array but
    /// the generator types it `Vec<MenuItem>` (dropping sizes/slots). The mirror
    /// stores canonical JSON anyway (§8), so a text body is exactly what we want.
    pub async fn get_text(&self, path: &str, query: &[(&str, String)]) -> CoreResult<String> {
        let url = format!("{}{}", self.base_url, path);
        let mut rb = self.http.request(reqwest::Method::GET, &url).query(query);
        if let Some(token) = self.bearer.read().unwrap_or_else(|e| e.into_inner()).clone() {
            rb = rb.bearer_auth(token);
        }
        let resp = rb.send().await.map_err(|e| classify_reqwest(&e))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| classify_reqwest(&e))?;
        if status.is_success() {
            Ok(body)
        } else {
            Err(status_to_error(status.as_u16(), &body))
        }
    }

    /// A `Configuration` for a normal call, carrying the current bearer token.
    pub fn config(&self) -> Configuration {
        Configuration {
            base_path: self.base_url.clone(),
            user_agent: Some(self.user_agent.clone()),
            client: self.http.clone(),
            basic_auth: None,
            oauth_access_token: None,
            bearer_access_token: self.bearer.read().unwrap_or_else(|e| e.into_inner()).clone(),
            api_key: None,
        }
    }
}

/// Translate the generated client's transport/response error into the coarse,
/// host-actionable `CoreError`. Generic over the per-endpoint error entity `T`:
/// the typed entity is ignored — we classify by status + parse the backend's
/// `{ "error": "…" }` envelope for a human message.
pub(crate) fn map_api_error<T>(e: ApiError<T>) -> CoreError {
    match e {
        // Transport failures — classified the same way for typed + raw calls.
        ApiError::Reqwest(re) => classify_reqwest(&re),
        ApiError::Io(io) => CoreError::Transient { message: io.to_string() },
        // A 2xx body we couldn't decode = wire drift / our bug, never the user's.
        ApiError::Serde(se) => CoreError::Internal { message: format!("decode: {se}") },
        ApiError::ResponseError(rc) => status_to_error(rc.status.as_u16(), &rc.content),
    }
}

/// A refused/unreachable connection means we're offline; timeouts/other are
/// transient (sync retries).
fn classify_reqwest(e: &reqwest::Error) -> CoreError {
    if e.is_connect() {
        CoreError::Offline { message: e.to_string() }
    } else {
        CoreError::Transient { message: e.to_string() }
    }
}

/// Map an HTTP status + raw body to a `CoreError` variant.
fn status_to_error(status: u16, body: &str) -> CoreError {
    let message = extract_error_message(body)
        .unwrap_or_else(|| reason(status).to_string());
    match status {
        401 => CoreError::Unauthenticated { message },
        // Network 403s carry no resource/action pair (that's `has_permission`'s
        // job); surface the server's message in `action` so the host can show it.
        403 => CoreError::Forbidden { resource: "api".into(), action: message },
        400 | 422 => CoreError::Validation { field: String::new(), message },
        s if s >= 500 => CoreError::Transient { message },
        s => CoreError::Server { status: s, code: reason(s).to_string(), message },
    }
}

/// Pull a human message out of the backend's `{ "error": "…" }` envelope
/// (`errors.rs::ErrorBody`); tolerate `message`/`detail` shapes from any
/// middleware too.
fn extract_error_message(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    for key in ["error", "message", "detail"] {
        if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
            return Some(s.to_string());
        }
    }
    None
}

fn reason(status: u16) -> &'static str {
    reqwest::StatusCode::from_u16(status)
        .ok()
        .and_then(|s| s.canonical_reason())
        .unwrap_or("error")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_builds_and_swaps_bearer() {
        let c = ApiClient::new("http://example.test".into()).unwrap();
        assert!(!c.has_bearer());
        c.set_bearer(Some("tok".into()));
        assert!(c.has_bearer());
        assert_eq!(c.config().bearer_access_token.as_deref(), Some("tok"));
        c.set_bearer(None);
        assert!(!c.has_bearer());
    }

    #[test]
    fn status_maps_to_variant() {
        let body = r#"{"error":"Invalid credentials"}"#;
        assert!(matches!(status_to_error(401, body), CoreError::Unauthenticated { .. }));
        assert!(matches!(status_to_error(403, body), CoreError::Forbidden { .. }));
        assert!(matches!(status_to_error(400, body), CoreError::Validation { .. }));
        assert!(matches!(status_to_error(409, body), CoreError::Server { status: 409, .. }));
        assert!(matches!(status_to_error(503, body), CoreError::Transient { .. }));
    }

    #[test]
    fn extracts_backend_error_message() {
        assert_eq!(
            extract_error_message(r#"{"error":"nope"}"#).as_deref(),
            Some("nope")
        );
        // Falls back to the status reason when the body isn't the envelope.
        if let CoreError::Validation { message, .. } = status_to_error(400, "not json") {
            assert_eq!(message, "Bad Request");
        } else {
            panic!("expected validation");
        }
    }
}
