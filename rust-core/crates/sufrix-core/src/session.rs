//! Session & auth (PLAN §7.2, §R3). The core owns the live session; the host is
//! a dumb secure-bytes vault (`TokenStore`, backed by Keychain/Keystore).
//!
//! Two entry paths, one identity model:
//!   - **online `login`** — PIN (`name`+`pin`+device `branch_id`, org derived
//!     server-side) or email+password; mints a bearer, mirrors permissions, and
//!     caches the org's offline-auth bundle for later offline unlock.
//!   - **offline `unlock_offline`** — verifies a typed PIN against the cached
//!     org bundle (argon2id, byte-compatible with the backend). No token; the
//!     identity is the real server `user_id`. Sync forces a fresh online re-auth.
//!
//! This module holds the FFI types + pure logic; the exported `SufrixCore`
//! methods that orchestrate the network live in `lib.rs`.

use argon2::{Argon2, PasswordHash, PasswordVerifier};
use serde::{Deserialize, Serialize};
use sufrix_api::models;

use crate::error::{CoreError, CoreResult};
use crate::store::Store;

/// kv key holding the org's offline-auth bundle (one org per device).
pub(crate) const BUNDLE_KEY: &str = "offline_auth_bundle";
/// kv key holding `{org_id, currency_code, tax_rate}` cached at online login, so
/// an offline unlock can build a complete `SessionSnapshot`.
pub(crate) const ORG_CONFIG_KEY: &str = "org_config";

// ── FFI surface ─────────────────────────────────────────────────────────────

/// The host's secure-bytes vault. The core hands it one opaque blob to persist;
/// token custody (expiry/refresh/rotation) stays in Rust.
#[uniffi::export(callback_interface)]
pub trait TokenStore: Send + Sync {
    fn save_blob(&self, blob: Vec<u8>);
    fn clear_blob(&self);
}

/// PIN (tellers) xor email+password (managers/admins). Enforced in Rust, not the
/// all-`Option` wire.
#[derive(uniffi::Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoginMode {
    Pin,
    Email,
}

/// A login attempt. Field requirements depend on `mode` (validated in
/// `wire_login_request`): PIN needs `name`+`pin`+`branch_id`; Email needs
/// `email`+`password` (`org_id` optional).
#[derive(uniffi::Record, Clone, Debug)]
pub struct LoginRequest {
    pub mode: LoginMode,
    /// Teller display name (PIN mode).
    pub name: Option<String>,
    pub pin: Option<String>,
    /// The device's configured branch (PIN mode). The server derives the org
    /// from it; post-D13 the teller need not be assigned to it.
    pub branch_id: Option<String>,
    pub email: Option<String>,
    pub password: Option<String>,
    /// Optional org disambiguator (Email mode).
    pub org_id: Option<String>,
}

/// A selectable branch (device-setup picker).
#[derive(uniffi::Record, Clone, Debug)]
pub struct BranchView {
    pub id: String,
    pub name: String,
    pub is_active: bool,
}

/// The cached session the host renders chrome from. Money/tax are pre-resolved.
#[derive(uniffi::Record, Clone, Debug, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub user_id: String,
    pub display_name: String,
    pub role: String,
    pub org_id: Option<String>,
    pub branch_id: Option<String>,
    pub currency_code: String,
    pub tax_rate: f64,
    /// `false` for an offline-unlocked session (no live token; sync will force a
    /// fresh online re-auth before it flushes the queue).
    pub online: bool,
    /// `true` once `/auth/permissions` has been mirrored. While `false`
    /// (offline unlock), `has_permission` is optimistic — the backend is the
    /// authority and validates the teller at replay.
    pub permissions_loaded: bool,
}

// ── internal state (held by SufrixCore) ─────────────────────────────────────

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct PermissionEntry {
    pub resource: String,
    pub action: String,
    pub granted: bool,
}

/// The live session the core holds in memory (and persists, minus nothing, into
/// the host's secure blob).
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SessionState {
    pub snapshot: SessionSnapshot,
    pub permissions: Vec<PermissionEntry>,
    /// `None` for an offline-unlocked session.
    pub token: Option<String>,
}

impl SessionState {
    /// Does this session grant `resource`/`action`? Optimistic until permissions
    /// are loaded (offline unlock) — see `permissions_loaded`.
    pub fn has_permission(&self, resource: &str, action: &str) -> bool {
        if !self.snapshot.permissions_loaded {
            return true;
        }
        self.permissions
            .iter()
            .any(|p| p.resource == resource && p.action == action && p.granted)
    }

    /// Serialize for the host's secure vault.
    pub fn to_blob(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    pub fn from_blob(blob: &[u8]) -> Option<SessionState> {
        serde_json::from_slice(blob).ok()
    }
}

// ── pure helpers ────────────────────────────────────────────────────────────

/// Validate the per-mode invariants and build the wire `LoginRequest`.
pub(crate) fn wire_login_request(req: &LoginRequest) -> CoreResult<models::LoginRequest> {
    let mut w = models::LoginRequest::new();
    match req.mode {
        LoginMode::Pin => {
            let name = nonblank(&req.name, "name")?;
            let pin = nonblank(&req.pin, "pin")?;
            let branch = nonblank(&req.branch_id, "branch_id")?;
            let branch = uuid::Uuid::parse_str(&branch)
                .map_err(|_| invalid("branch_id", "not a valid uuid"))?;
            w.name = Some(Some(name));
            w.pin = Some(Some(pin));
            w.branch_id = Some(Some(branch));
        }
        LoginMode::Email => {
            let email = nonblank(&req.email, "email")?;
            let password = nonblank(&req.password, "password")?;
            w.email = Some(Some(email));
            w.password = Some(Some(password));
            if let Some(org) = req.org_id.as_deref().filter(|s| !s.is_empty()) {
                let org = uuid::Uuid::parse_str(org)
                    .map_err(|_| invalid("org_id", "not a valid uuid"))?;
                w.org_id = Some(Some(org));
            }
        }
    }
    Ok(w)
}

/// Build the cached snapshot from a successful login. `permissions_loaded` is
/// flipped to `true` by the caller once `/auth/permissions` is mirrored.
pub(crate) fn snapshot_from_login(
    resp: &models::LoginResponse,
    branch_id: Option<String>,
) -> SessionSnapshot {
    SessionSnapshot {
        user_id: resp.user.id.to_string(),
        display_name: resp.user.name.clone(),
        role: resp.user.role.to_string(),
        org_id: resp.user.org_id.flatten().map(|u| u.to_string()),
        branch_id,
        currency_code: resp.currency_code.clone(),
        tax_rate: resp.tax_rate,
        online: true,
        permissions_loaded: false,
    }
}

pub(crate) fn permissions_from(resp: &models::AuthPermissionsResponse) -> Vec<PermissionEntry> {
    resp.permissions
        .iter()
        .map(|p| PermissionEntry {
            resource: p.resource.clone(),
            action: p.action.clone(),
            granted: p.granted,
        })
        .collect()
}

/// Cache the org's offline-auth bundle + a minimal org config so a later offline
/// unlock can verify a PIN and build a complete snapshot. Best-effort.
pub(crate) fn cache_bundle(store: &Store, bundle: &models::OfflineAuthBundle, snapshot: &SessionSnapshot) {
    if let Ok(json) = serde_json::to_string(bundle) {
        let _ = store.kv_put(BUNDLE_KEY, &json);
    }
    let cfg = serde_json::json!({
        "org_id": snapshot.org_id,
        "currency_code": snapshot.currency_code,
        "tax_rate": snapshot.tax_rate,
    });
    let _ = store.kv_put(ORG_CONFIG_KEY, &cfg.to_string());
}

/// Verify a typed PIN against the cached org bundle and build an offline
/// `SessionState`. Mirrors the backend's `auth::offline` (argon2id PHC).
pub(crate) fn unlock_from_bundle(
    store: &Store,
    name: &str,
    pin: &str,
    branch_id: &str,
) -> CoreResult<SessionState> {
    let raw = store
        .kv_get(BUNDLE_KEY)?
        .ok_or_else(|| CoreError::Unauthenticated {
            detail: "no offline bundle cached — sign in online once first".into(),
        })?;
    let bundle: models::OfflineAuthBundle = serde_json::from_str(&raw)?;

    let teller = bundle
        .tellers
        .iter()
        .find(|t| {
            t.is_active
                && t.name.eq_ignore_ascii_case(name)
                && t.offline_pin_hash
                    .clone()
                    .flatten()
                    .map(|h| verify_offline_pin(pin, &h))
                    .unwrap_or(false)
        })
        .ok_or_else(|| CoreError::Unauthenticated {
            detail: "PIN not recognized offline".into(),
        })?;

    let (currency_code, tax_rate) = match store.kv_get(ORG_CONFIG_KEY)? {
        Some(raw) => {
            let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
            (
                v.get("currency_code").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                v.get("tax_rate").and_then(|x| x.as_f64()).unwrap_or(0.0),
            )
        }
        None => (String::new(), 0.0),
    };

    let snapshot = SessionSnapshot {
        user_id: teller.user_id.to_string(),
        display_name: teller.name.clone(),
        role: teller.role.clone(),
        org_id: Some(bundle.org_id.to_string()),
        branch_id: Some(branch_id.to_string()),
        currency_code,
        tax_rate,
        online: false,
        permissions_loaded: false,
    };
    Ok(SessionState { snapshot, permissions: Vec::new(), token: None })
}

/// argon2id PHC verification — byte-compatible with `SufrixRust`
/// `auth::offline::hash_offline_pin` (params ride in the PHC string).
pub(crate) fn verify_offline_pin(pin: &str, phc: &str) -> bool {
    PasswordHash::new(phc)
        .map(|h| Argon2::default().verify_password(pin.as_bytes(), &h).is_ok())
        .unwrap_or(false)
}

fn nonblank(field: &Option<String>, name: &'static str) -> CoreResult<String> {
    match field.as_deref().map(str::trim) {
        Some(s) if !s.is_empty() => Ok(s.to_string()),
        _ => Err(invalid(name, "is required")),
    }
}

fn invalid(field: &str, message: &str) -> CoreError {
    CoreError::Validation { field: field.to_string(), detail: message.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin_req() -> LoginRequest {
        LoginRequest {
            mode: LoginMode::Pin,
            name: Some("Sara".into()),
            pin: Some("1234".into()),
            branch_id: Some("00000000-0000-0000-0000-000000000001".into()),
            email: None,
            password: None,
            org_id: None,
        }
    }

    #[test]
    fn pin_request_validates_and_builds_wire() {
        let w = wire_login_request(&pin_req()).unwrap();
        assert_eq!(w.name, Some(Some("Sara".into())));
        assert_eq!(w.pin, Some(Some("1234".into())));
        assert!(w.branch_id.flatten().is_some());
        assert!(w.email.is_none());
    }

    #[test]
    fn pin_request_missing_branch_is_validation_error() {
        let mut r = pin_req();
        r.branch_id = None;
        assert!(matches!(wire_login_request(&r), Err(CoreError::Validation { .. })));
    }

    #[test]
    fn pin_request_bad_branch_uuid_is_validation_error() {
        let mut r = pin_req();
        r.branch_id = Some("not-a-uuid".into());
        assert!(matches!(wire_login_request(&r), Err(CoreError::Validation { .. })));
    }

    #[test]
    fn email_request_requires_email_and_password() {
        let r = LoginRequest {
            mode: LoginMode::Email,
            name: None, pin: None, branch_id: None,
            email: Some("a@b.com".into()), password: None, org_id: None,
        };
        assert!(matches!(wire_login_request(&r), Err(CoreError::Validation { .. })));
    }

    #[test]
    fn offline_unlock_roundtrips_against_a_cached_bundle() {
        // Hash a PIN exactly as the backend would, stash a bundle, then unlock.
        let phc = backend_hash("4321");
        let bundle = serde_json::json!({
            "org_id": "00000000-0000-0000-0000-0000000000aa",
            "generated_at": "2026-06-19T10:00:00Z",
            "tellers": [{
                "user_id": "00000000-0000-0000-0000-0000000000bb",
                "name": "Sara", "role": "teller", "is_active": true,
                "offline_pin_hash": phc,
            }]
        });
        let store = Store::open("").unwrap();
        store.kv_put(BUNDLE_KEY, &bundle.to_string()).unwrap();
        store.kv_put(ORG_CONFIG_KEY, r#"{"org_id":"00000000-0000-0000-0000-0000000000aa","currency_code":"EGP","tax_rate":0.14}"#).unwrap();

        let s = unlock_from_bundle(&store, "sara", "4321", "00000000-0000-0000-0000-000000000001").unwrap();
        assert_eq!(s.snapshot.display_name, "Sara");
        assert_eq!(s.snapshot.currency_code, "EGP");
        assert_eq!(s.snapshot.tax_rate, 0.14);
        assert!(!s.snapshot.online);
        assert!(s.token.is_none());
        // optimistic permissions while offline
        assert!(s.has_permission("orders", "create"));

        // wrong PIN / unknown teller → unauthenticated
        assert!(unlock_from_bundle(&store, "Sara", "0000", "00000000-0000-0000-0000-000000000001").is_err());
        assert!(unlock_from_bundle(&store, "Nobody", "4321", "00000000-0000-0000-0000-000000000001").is_err());
    }

    #[test]
    fn unlock_without_a_bundle_is_unauthenticated() {
        let store = Store::open("").unwrap();
        assert!(matches!(
            unlock_from_bundle(&store, "Sara", "1234", "00000000-0000-0000-0000-000000000001"),
            Err(CoreError::Unauthenticated { .. })
        ));
    }

    /// Produce a real argon2id PHC string (default params, same as the backend's
    /// `Argon2::default()`), so the test exercises the live verifier rather than a
    /// brittle fixed vector. A deterministic salt avoids needing an RNG feature.
    fn backend_hash(pin: &str) -> String {
        use argon2::password_hash::SaltString;
        use argon2::PasswordHasher;
        let salt = SaltString::encode_b64(b"sufrix-test-salt").unwrap();
        Argon2::default().hash_password(pin.as_bytes(), &salt).unwrap().to_string()
    }
}
