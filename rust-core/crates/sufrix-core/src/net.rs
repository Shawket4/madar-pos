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

use std::sync::{Arc, RwLock};
use std::time::Duration;

use sufrix_api::apis::configuration::Configuration;
use sufrix_api::apis::Error as ApiError;

use crate::error::{CoreError, CoreResult};

/// The TLS config every backend request rides on: rustls with the **ring** crypto
/// provider and the bundled Mozilla CA roots (`webpki-roots`). reqwest 0.13's stock
/// `rustls` feature would drag in aws-lc-rs (needs cmake to cross-compile) plus
/// `rustls-platform-verifier` (needs Android `Context`/JNI init), so we enable
/// `rustls-no-provider` and hand reqwest a fully-built config here instead. Result:
/// pure-Rust, cross-compiles clean to Android/iOS, same trust anchors everywhere —
/// no OpenSSL and no platform cert-store wiring.
fn default_tls_config() -> rustls::ClientConfig {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("ring provider supports rustls' default protocol versions")
    .with_root_certificates(roots)
    .with_no_client_auth()
}

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
            // ring + bundled Mozilla roots (see default_tls_config) — keeps cert
            // verification identical on Android/iOS/desktop with no OpenSSL.
            .use_preconfigured_tls(default_tls_config())
            // Short connect timeout so an unreachable server fails fast and the
            // hot path can fall back to offline instead of stranding a teller.
            .connect_timeout(Duration::from_secs(4))
            .timeout(Duration::from_secs(20))
            .user_agent(user_agent.clone())
            .build()
            .map_err(|e| CoreError::Internal { detail: format!("http client: {e}") })?;
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

    /// Ping the backend to refresh reachability + read its clock. ANY HTTP
    /// response (even 4xx) means we reached the server → online; only a transport
    /// failure errs. Returns the server-vs-device skew in SECONDS when the
    /// response carries a parseable `Date` header (for the clock-skew banner).
    pub async fn ping(&self) -> CoreResult<Option<i64>> {
        let mut rb = self.http.request(reqwest::Method::GET, &self.base_url);
        if let Some(token) = self.bearer.read().unwrap_or_else(|e| e.into_inner()).clone() {
            rb = rb.bearer_auth(token);
        }
        let resp = rb.send().await.map_err(|e| classify_reqwest(&e))?;
        let skew = resp
            .headers()
            .get(reqwest::header::DATE)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_http_date)
            .map(|server_epoch| server_epoch - chrono::Utc::now().timestamp());
        Ok(skew)
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
        ApiError::Io(io) => CoreError::Transient { detail: io.to_string() },
        // A 2xx body we couldn't decode = wire drift / our bug, never the user's.
        ApiError::Serde(se) => CoreError::Internal { detail: format!("decode: {se}") },
        ApiError::ResponseError(rc) => status_to_error(rc.status.as_u16(), &rc.content),
    }
}

/// A refused/unreachable connection means we're offline; timeouts/other are
/// transient (sync retries).
fn classify_reqwest(e: &reqwest::Error) -> CoreError {
    if e.is_connect() {
        CoreError::Offline { detail: e.to_string() }
    } else {
        CoreError::Transient { detail: e.to_string() }
    }
}

/// Map an HTTP status + raw body to a `CoreError` variant.
fn status_to_error(status: u16, body: &str) -> CoreError {
    let message = extract_error_message(body)
        .unwrap_or_else(|| reason(status).to_string());
    match status {
        401 => CoreError::Unauthenticated { detail: message },
        // Network 403s carry no resource/action pair (that's `has_permission`'s
        // job); surface the server's message in `action` so the host can show it.
        403 => CoreError::Forbidden { resource: "api".into(), action: message },
        400 | 422 => CoreError::Validation { field: String::new(), detail: message },
        s if s >= 500 => CoreError::Transient { detail: message },
        s => CoreError::Server { status: s, code: reason(s).to_string(), detail: message },
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

/// Parse an HTTP `Date` header (RFC 7231 IMF-fixdate, e.g.
/// "Tue, 21 Jun 2026 10:00:00 GMT") to a UTC epoch second.
fn parse_http_date(s: &str) -> Option<i64> {
    chrono::NaiveDateTime::parse_from_str(s.trim(), "%a, %d %b %Y %H:%M:%S GMT")
        .ok()
        .map(|dt| dt.and_utc().timestamp())
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
    fn parses_http_date_header_to_epoch() {
        // "Thu, 01 Jan 1970 00:00:00 GMT" = epoch 0.
        assert_eq!(parse_http_date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        assert_eq!(parse_http_date("Thu, 01 Jan 1970 00:01:00 GMT"), Some(60));
        assert_eq!(parse_http_date("not a date"), None);
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
        if let CoreError::Validation { detail: message, .. } = status_to_error(400, "not json") {
            assert_eq!(message, "Bad Request");
        } else {
            panic!("expected validation");
        }
    }

    // ── extract_error_message: alternate envelope keys & malformed bodies ─────

    #[test]
    fn extract_error_message_reads_message_and_detail_keys() {
        // `error` is preferred, then `message`, then `detail`.
        assert_eq!(extract_error_message(r#"{"message":"middleware says no"}"#).as_deref(), Some("middleware says no"));
        assert_eq!(extract_error_message(r#"{"detail":"422 detail"}"#).as_deref(), Some("422 detail"));
        // `error` wins when several are present.
        assert_eq!(
            extract_error_message(r#"{"error":"first","message":"second"}"#).as_deref(),
            Some("first"),
        );
    }

    #[test]
    fn extract_error_message_is_none_for_non_envelope_bodies() {
        assert!(extract_error_message("").is_none());
        assert!(extract_error_message("plain text").is_none());
        assert!(extract_error_message("{}").is_none()); // valid JSON, no known key
        assert!(extract_error_message(r#"{"error":123}"#).is_none()); // non-string value
        assert!(extract_error_message("[1,2,3]").is_none()); // not an object
    }

    // ── status_to_error: the full classification table ───────────────────────

    #[test]
    fn status_to_error_403_surfaces_message_in_action() {
        // A network 403 has no resource/action pair; the server message rides in
        // `action` and `resource` is the generic "api".
        match status_to_error(403, r#"{"error":"insufficient role"}"#) {
            CoreError::Forbidden { resource, action } => {
                assert_eq!(resource, "api");
                assert_eq!(action, "insufficient role");
            }
            other => panic!("expected Forbidden, got {other:?}"),
        }
    }

    #[test]
    fn status_to_error_422_is_validation() {
        assert!(matches!(status_to_error(422, r#"{"error":"bad field"}"#), CoreError::Validation { .. }));
    }

    #[test]
    fn status_to_error_401_carries_message_in_detail() {
        match status_to_error(401, r#"{"error":"token expired"}"#) {
            CoreError::Unauthenticated { detail } => assert_eq!(detail, "token expired"),
            other => panic!("expected Unauthenticated, got {other:?}"),
        }
    }

    #[test]
    fn status_to_error_500_and_above_is_transient() {
        assert!(matches!(status_to_error(500, "{}"), CoreError::Transient { .. }));
        assert!(matches!(status_to_error(502, "{}"), CoreError::Transient { .. }));
        assert!(matches!(status_to_error(504, "{}"), CoreError::Transient { .. }));
    }

    #[test]
    fn status_to_error_other_4xx_is_server_with_status_and_reason() {
        // 404 isn't special-cased here → Server, carrying the status + canonical
        // reason as the code, and the parsed message as the detail.
        match status_to_error(404, r#"{"error":"missing"}"#) {
            CoreError::Server { status, code, detail } => {
                assert_eq!(status, 404);
                assert_eq!(code, "Not Found");
                assert_eq!(detail, "missing");
            }
            other => panic!("expected Server, got {other:?}"),
        }
        // 409 likewise (already covered for the matches!, here we check fields).
        match status_to_error(409, "not json") {
            CoreError::Server { status, code, detail } => {
                assert_eq!(status, 409);
                assert_eq!(code, "Conflict");
                assert_eq!(detail, "Conflict"); // falls back to reason
            }
            other => panic!("expected Server, got {other:?}"),
        }
    }

    #[test]
    fn status_to_error_unknown_status_uses_generic_reason() {
        // 499 has no canonical reason → "error" string for both code and detail.
        match status_to_error(499, "not json") {
            CoreError::Server { status, code, detail } => {
                assert_eq!(status, 499);
                assert_eq!(code, "error");
                assert_eq!(detail, "error");
            }
            other => panic!("expected Server, got {other:?}"),
        }
    }

    // ── map_api_error: ApiError<T> → CoreError ────────────────────────────────

    #[test]
    fn map_api_error_response_error_classifies_by_status() {
        let resp = sufrix_api::apis::ResponseContent::<()> {
            status: reqwest::StatusCode::UNAUTHORIZED,
            content: r#"{"error":"nope"}"#.into(),
            entity: None,
        };
        assert!(matches!(map_api_error(ApiError::ResponseError(resp)), CoreError::Unauthenticated { .. }));

        let resp = sufrix_api::apis::ResponseContent::<()> {
            status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            content: "boom".into(),
            entity: None,
        };
        assert!(matches!(map_api_error(ApiError::ResponseError(resp)), CoreError::Transient { .. }));
    }

    #[test]
    fn map_api_error_serde_is_internal_decode_error() {
        // A 2xx body we can't decode is wire drift / our bug, never the user's.
        let serde_err = serde_json::from_str::<i32>("not a number").unwrap_err();
        match map_api_error::<()>(ApiError::Serde(serde_err)) {
            CoreError::Internal { detail } => assert!(detail.starts_with("decode:")),
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn map_api_error_io_is_transient() {
        let io = std::io::Error::new(std::io::ErrorKind::TimedOut, "read timed out");
        assert!(matches!(map_api_error::<()>(ApiError::Io(io)), CoreError::Transient { .. }));
    }

    // ── parse_http_date: trimming, timezone token, partial dates ──────────────

    #[test]
    fn parse_http_date_trims_surrounding_whitespace() {
        assert_eq!(parse_http_date("  Thu, 01 Jan 1970 00:00:00 GMT  "), Some(0));
    }

    #[test]
    fn parse_http_date_known_imf_fixdate() {
        // 2026-06-21T10:00:00Z. Epoch = 1_771_668_000 (deterministic, recomputed
        // via chrono below to avoid a magic constant drifting).
        let expected = chrono::NaiveDate::from_ymd_opt(2026, 6, 21)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp();
        // chrono validates the %a weekday against the date — 2026-06-21 is a Sunday.
        assert_eq!(parse_http_date("Sun, 21 Jun 2026 10:00:00 GMT"), Some(expected));
    }

    #[test]
    fn parse_http_date_rejects_non_gmt_and_garbage() {
        assert_eq!(parse_http_date("Thu, 01 Jan 1970 00:00:00 UTC"), None); // not GMT token
        assert_eq!(parse_http_date("1970-01-01T00:00:00Z"), None); // ISO, not IMF
        assert_eq!(parse_http_date(""), None);
    }

    // ── reason() ──────────────────────────────────────────────────────────────

    #[test]
    fn reason_maps_known_and_unknown_statuses() {
        assert_eq!(reason(200), "OK");
        assert_eq!(reason(404), "Not Found");
        assert_eq!(reason(418), "I'm a teapot");
        assert_eq!(reason(299), "error"); // no canonical reason
        assert_eq!(reason(0), "error"); // not a valid status code
    }

    // ── ApiClient::config defaults ────────────────────────────────────────────

    #[test]
    fn config_carries_base_path_user_agent_and_no_bearer_by_default() {
        let c = ApiClient::new("http://example.test".into()).unwrap();
        let cfg = c.config();
        assert_eq!(cfg.base_path, "http://example.test");
        assert!(cfg.user_agent.as_deref().unwrap().starts_with("sufrix-core/"));
        assert!(cfg.bearer_access_token.is_none());
        assert!(cfg.basic_auth.is_none());
        assert!(cfg.api_key.is_none());
    }
}
