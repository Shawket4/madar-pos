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

    // ── wire_login_request: PIN mode ─────────────────────────────────────

    #[test]
    fn pin_request_missing_name_is_validation_error() {
        let mut r = pin_req();
        r.name = None;
        assert!(matches!(wire_login_request(&r), Err(CoreError::Validation { field, .. }) if field == "name"));
    }

    #[test]
    fn pin_request_missing_pin_is_validation_error() {
        let mut r = pin_req();
        r.pin = None;
        assert!(matches!(wire_login_request(&r), Err(CoreError::Validation { field, .. }) if field == "pin"));
    }

    #[test]
    fn pin_request_blank_name_is_validation_error() {
        let mut r = pin_req();
        r.name = Some("   ".into());
        assert!(matches!(wire_login_request(&r), Err(CoreError::Validation { field, .. }) if field == "name"));
    }

    #[test]
    fn pin_request_trims_name_and_pin() {
        let mut r = pin_req();
        r.name = Some("  Sara  ".into());
        r.pin = Some("  1234 ".into());
        let w = wire_login_request(&r).unwrap();
        assert_eq!(w.name, Some(Some("Sara".into())));
        assert_eq!(w.pin, Some(Some("1234".into())));
    }

    #[test]
    fn pin_request_ignores_email_fields_and_leaves_them_unset() {
        let mut r = pin_req();
        r.email = Some("ignored@example.com".into());
        r.password = Some("ignored".into());
        r.org_id = Some("00000000-0000-0000-0000-0000000000ff".into());
        let w = wire_login_request(&r).unwrap();
        // PIN branch never copies email/password/org_id onto the wire.
        assert!(w.email.is_none());
        assert!(w.password.is_none());
        assert!(w.org_id.is_none());
    }

    #[test]
    fn pin_request_blank_branch_is_validation_error() {
        let mut r = pin_req();
        r.branch_id = Some("   ".into());
        assert!(matches!(wire_login_request(&r), Err(CoreError::Validation { field, .. }) if field == "branch_id"));
    }

    // ── wire_login_request: Email mode ───────────────────────────────────

    fn email_req() -> LoginRequest {
        LoginRequest {
            mode: LoginMode::Email,
            name: None,
            pin: None,
            branch_id: None,
            email: Some("manager@example.com".into()),
            password: Some("hunter2".into()),
            org_id: None,
        }
    }

    #[test]
    fn email_request_minimal_builds_wire() {
        let w = wire_login_request(&email_req()).unwrap();
        assert_eq!(w.email, Some(Some("manager@example.com".into())));
        assert_eq!(w.password, Some(Some("hunter2".into())));
        assert!(w.org_id.is_none());
        // PIN-only fields stay unset in email mode.
        assert!(w.name.is_none());
        assert!(w.pin.is_none());
        assert!(w.branch_id.is_none());
    }

    #[test]
    fn email_request_missing_password_is_validation_error() {
        let mut r = email_req();
        r.password = None;
        assert!(matches!(wire_login_request(&r), Err(CoreError::Validation { field, .. }) if field == "password"));
    }

    #[test]
    fn email_request_missing_email_is_validation_error() {
        let mut r = email_req();
        r.email = None;
        assert!(matches!(wire_login_request(&r), Err(CoreError::Validation { field, .. }) if field == "email"));
    }

    #[test]
    fn email_request_with_valid_org_id_is_parsed_onto_wire() {
        let mut r = email_req();
        r.org_id = Some("00000000-0000-0000-0000-0000000000aa".into());
        let w = wire_login_request(&r).unwrap();
        assert!(w.org_id.flatten().is_some());
    }

    #[test]
    fn email_request_empty_org_id_is_treated_as_absent() {
        let mut r = email_req();
        r.org_id = Some(String::new());
        // Empty string is filtered out, so it must NOT be a validation error and
        // org_id stays unset on the wire.
        let w = wire_login_request(&r).unwrap();
        assert!(w.org_id.is_none());
    }

    #[test]
    fn email_request_bad_org_id_uuid_is_validation_error() {
        let mut r = email_req();
        r.org_id = Some("not-a-uuid".into());
        assert!(matches!(wire_login_request(&r), Err(CoreError::Validation { field, .. }) if field == "org_id"));
    }

    #[test]
    fn email_request_trims_email_and_password() {
        let mut r = email_req();
        r.email = Some("  manager@example.com  ".into());
        r.password = Some("  hunter2  ".into());
        let w = wire_login_request(&r).unwrap();
        assert_eq!(w.email, Some(Some("manager@example.com".into())));
        assert_eq!(w.password, Some(Some("hunter2".into())));
    }

    // ── snapshot_from_login ──────────────────────────────────────────────

    fn login_resp(org: Option<&str>) -> models::LoginResponse {
        let mut user = models::UserPublic::new(
            uuid::Uuid::parse_str("00000000-0000-0000-0000-0000000000cc").unwrap(),
            true,
            "Mona".into(),
            models::UserRole::Teller,
        );
        user.org_id = org.map(|o| Some(uuid::Uuid::parse_str(o).unwrap()));
        models::LoginResponse {
            currency_code: "EGP".into(),
            tax_rate: 0.14,
            token: "jwt.abc.def".into(),
            user: Box::new(user),
        }
    }

    #[test]
    fn snapshot_from_login_maps_all_fields() {
        let resp = login_resp(Some("00000000-0000-0000-0000-0000000000aa"));
        let snap = snapshot_from_login(&resp, Some("branch-7".into()));
        assert_eq!(snap.user_id, "00000000-0000-0000-0000-0000000000cc");
        assert_eq!(snap.display_name, "Mona");
        assert_eq!(snap.role, "teller");
        assert_eq!(snap.org_id.as_deref(), Some("00000000-0000-0000-0000-0000000000aa"));
        assert_eq!(snap.branch_id.as_deref(), Some("branch-7"));
        assert_eq!(snap.currency_code, "EGP");
        assert_eq!(snap.tax_rate, 0.14);
        assert!(snap.online);
        // The caller flips this later once /auth/permissions is mirrored.
        assert!(!snap.permissions_loaded);
    }

    #[test]
    fn snapshot_from_login_handles_no_org() {
        // org_id absent entirely (None) → snapshot org_id is None.
        let resp = login_resp(None);
        let snap = snapshot_from_login(&resp, None);
        assert!(snap.org_id.is_none());
        assert!(snap.branch_id.is_none());
    }

    #[test]
    fn snapshot_from_login_handles_explicit_null_org() {
        // org_id present but inner None (Some(None)) → flatten yields None.
        let mut resp = login_resp(None);
        resp.user.org_id = Some(None);
        let snap = snapshot_from_login(&resp, None);
        assert!(snap.org_id.is_none());
    }

    // ── permissions_from ─────────────────────────────────────────────────

    #[test]
    fn permissions_from_maps_each_item() {
        let resp = models::AuthPermissionsResponse {
            permissions: vec![
                models::UserPermissionItem::new("create".into(), true, "orders".into()),
                models::UserPermissionItem::new("void".into(), false, "orders".into()),
            ],
        };
        let perms = permissions_from(&resp);
        assert_eq!(perms.len(), 2);
        assert_eq!(perms[0].resource, "orders");
        assert_eq!(perms[0].action, "create");
        assert!(perms[0].granted);
        assert_eq!(perms[1].action, "void");
        assert!(!perms[1].granted);
    }

    #[test]
    fn permissions_from_empty_is_empty() {
        let resp = models::AuthPermissionsResponse { permissions: vec![] };
        assert!(permissions_from(&resp).is_empty());
    }

    // ── SessionState::has_permission ─────────────────────────────────────

    fn state_with(perms: Vec<PermissionEntry>, loaded: bool, online: bool) -> SessionState {
        SessionState {
            snapshot: SessionSnapshot {
                user_id: "u".into(),
                display_name: "n".into(),
                role: "teller".into(),
                org_id: None,
                branch_id: None,
                currency_code: "EGP".into(),
                tax_rate: 0.0,
                online,
                permissions_loaded: loaded,
            },
            permissions: perms,
            token: if online { Some("t".into()) } else { None },
        }
    }

    #[test]
    fn has_permission_is_optimistic_when_not_loaded() {
        let s = state_with(vec![], false, false);
        // Anything is granted while permissions_loaded == false.
        assert!(s.has_permission("orders", "void"));
        assert!(s.has_permission("anything", "at_all"));
    }

    #[test]
    fn has_permission_grants_matching_loaded_entry() {
        let s = state_with(
            vec![PermissionEntry { resource: "orders".into(), action: "create".into(), granted: true }],
            true,
            true,
        );
        assert!(s.has_permission("orders", "create"));
    }

    #[test]
    fn has_permission_denies_unlisted_when_loaded() {
        let s = state_with(
            vec![PermissionEntry { resource: "orders".into(), action: "create".into(), granted: true }],
            true,
            true,
        );
        assert!(!s.has_permission("orders", "void"));
        assert!(!s.has_permission("shifts", "create"));
    }

    #[test]
    fn has_permission_denies_explicitly_revoked_entry() {
        let s = state_with(
            vec![PermissionEntry { resource: "orders".into(), action: "void".into(), granted: false }],
            true,
            true,
        );
        // Present but granted == false → denied.
        assert!(!s.has_permission("orders", "void"));
    }

    #[test]
    fn has_permission_requires_both_resource_and_action_to_match() {
        let s = state_with(
            vec![PermissionEntry { resource: "orders".into(), action: "create".into(), granted: true }],
            true,
            true,
        );
        assert!(!s.has_permission("orders", "delete")); // right resource, wrong action
        assert!(!s.has_permission("menu", "create")); // wrong resource, right action
    }

    // ── SessionState blob round-trip ─────────────────────────────────────

    #[test]
    fn session_state_blob_roundtrips() {
        let s = state_with(
            vec![PermissionEntry { resource: "orders".into(), action: "create".into(), granted: true }],
            true,
            true,
        );
        let blob = s.to_blob();
        assert!(!blob.is_empty());
        let back = SessionState::from_blob(&blob).expect("decode");
        assert_eq!(back.snapshot.user_id, s.snapshot.user_id);
        assert_eq!(back.token, s.token);
        assert_eq!(back.permissions.len(), 1);
        assert!(back.has_permission("orders", "create"));
    }

    #[test]
    fn from_blob_rejects_garbage() {
        assert!(SessionState::from_blob(b"not json at all").is_none());
        assert!(SessionState::from_blob(b"").is_none());
    }

    #[test]
    fn from_blob_roundtrips_offline_session_with_no_token() {
        let s = state_with(vec![], false, false);
        let blob = s.to_blob();
        let back = SessionState::from_blob(&blob).unwrap();
        assert!(back.token.is_none());
        assert!(!back.snapshot.online);
        assert!(!back.snapshot.permissions_loaded);
    }

    // ── cache_bundle ─────────────────────────────────────────────────────

    #[test]
    fn cache_bundle_persists_bundle_and_org_config() {
        let store = Store::open("").unwrap();
        let bundle: models::OfflineAuthBundle = serde_json::from_value(serde_json::json!({
            "org_id": "00000000-0000-0000-0000-0000000000aa",
            "generated_at": "2026-06-19T10:00:00Z",
            "tellers": []
        })).unwrap();
        let snapshot = SessionSnapshot {
            user_id: "u".into(),
            display_name: "n".into(),
            role: "teller".into(),
            org_id: Some("00000000-0000-0000-0000-0000000000aa".into()),
            branch_id: Some("b".into()),
            currency_code: "EGP".into(),
            tax_rate: 0.14,
            online: true,
            permissions_loaded: true,
        };
        cache_bundle(&store, &bundle, &snapshot);

        // Both keys are now populated and parseable.
        let stored_bundle = store.kv_get(BUNDLE_KEY).unwrap().expect("bundle stored");
        let _: models::OfflineAuthBundle = serde_json::from_str(&stored_bundle).unwrap();

        let cfg_raw = store.kv_get(ORG_CONFIG_KEY).unwrap().expect("cfg stored");
        let cfg: serde_json::Value = serde_json::from_str(&cfg_raw).unwrap();
        assert_eq!(cfg["org_id"], "00000000-0000-0000-0000-0000000000aa");
        assert_eq!(cfg["currency_code"], "EGP");
        assert_eq!(cfg["tax_rate"], 0.14);
    }

    #[test]
    fn cache_bundle_then_unlock_full_flow() {
        // End-to-end: cache from a snapshot, then unlock against the cached data.
        let store = Store::open("").unwrap();
        let phc = backend_hash("9999");
        let bundle: models::OfflineAuthBundle = serde_json::from_value(serde_json::json!({
            "org_id": "00000000-0000-0000-0000-0000000000aa",
            "generated_at": "2026-06-19T10:00:00Z",
            "tellers": [{
                "user_id": "00000000-0000-0000-0000-0000000000bb",
                "name": "Mona", "role": "teller", "is_active": true,
                "offline_pin_hash": phc,
            }]
        })).unwrap();
        let snapshot = SessionSnapshot {
            user_id: "00000000-0000-0000-0000-0000000000bb".into(),
            display_name: "Mona".into(),
            role: "teller".into(),
            org_id: Some("00000000-0000-0000-0000-0000000000aa".into()),
            branch_id: Some("b".into()),
            currency_code: "USD".into(),
            tax_rate: 0.07,
            online: true,
            permissions_loaded: true,
        };
        cache_bundle(&store, &bundle, &snapshot);

        let s = unlock_from_bundle(&store, "Mona", "9999", "00000000-0000-0000-0000-000000000002").unwrap();
        assert_eq!(s.snapshot.currency_code, "USD");
        assert_eq!(s.snapshot.tax_rate, 0.07);
        assert_eq!(s.snapshot.org_id.as_deref(), Some("00000000-0000-0000-0000-0000000000aa"));
        assert_eq!(s.snapshot.branch_id.as_deref(), Some("00000000-0000-0000-0000-000000000002"));
    }

    // ── unlock_from_bundle: edge cases ───────────────────────────────────

    /// Stash a one-teller bundle hashing `pin`, with the teller flagged
    /// `is_active`. Returns the open store.
    fn store_with_teller(name: &str, pin: &str, is_active: bool) -> Store {
        let phc = backend_hash(pin);
        let bundle = serde_json::json!({
            "org_id": "00000000-0000-0000-0000-0000000000aa",
            "generated_at": "2026-06-19T10:00:00Z",
            "tellers": [{
                "user_id": "00000000-0000-0000-0000-0000000000bb",
                "name": name, "role": "teller", "is_active": is_active,
                "offline_pin_hash": phc,
            }]
        });
        let store = Store::open("").unwrap();
        store.kv_put(BUNDLE_KEY, &bundle.to_string()).unwrap();
        store
    }

    #[test]
    fn unlock_right_pin_succeeds_case_insensitive_name() {
        let store = store_with_teller("Sara", "1234", true);
        // name match is ASCII-case-insensitive.
        let s = unlock_from_bundle(&store, "SARA", "1234", "00000000-0000-0000-0000-000000000001").unwrap();
        assert_eq!(s.snapshot.display_name, "Sara"); // canonical name from bundle
        assert_eq!(s.snapshot.role, "teller");
        assert!(!s.snapshot.online);
        assert!(!s.snapshot.permissions_loaded);
        assert!(s.token.is_none());
        assert!(s.permissions.is_empty());
    }

    #[test]
    fn unlock_wrong_pin_is_unauthenticated() {
        let store = store_with_teller("Sara", "1234", true);
        assert!(matches!(
            unlock_from_bundle(&store, "Sara", "0000", "00000000-0000-0000-0000-000000000001"),
            Err(CoreError::Unauthenticated { .. })
        ));
    }

    #[test]
    fn unlock_unknown_name_is_unauthenticated() {
        let store = store_with_teller("Sara", "1234", true);
        assert!(matches!(
            unlock_from_bundle(&store, "Ghost", "1234", "00000000-0000-0000-0000-000000000001"),
            Err(CoreError::Unauthenticated { .. })
        ));
    }

    #[test]
    fn unlock_inactive_teller_is_rejected_even_with_right_pin() {
        let store = store_with_teller("Sara", "1234", false);
        assert!(matches!(
            unlock_from_bundle(&store, "Sara", "1234", "00000000-0000-0000-0000-000000000001"),
            Err(CoreError::Unauthenticated { .. })
        ));
    }

    #[test]
    fn unlock_teller_with_null_pin_hash_cannot_unlock() {
        // offline_pin_hash null → teller has never logged in online; no offline auth.
        let bundle = serde_json::json!({
            "org_id": "00000000-0000-0000-0000-0000000000aa",
            "generated_at": "2026-06-19T10:00:00Z",
            "tellers": [{
                "user_id": "00000000-0000-0000-0000-0000000000bb",
                "name": "Sara", "role": "teller", "is_active": true,
                "offline_pin_hash": null,
            }]
        });
        let store = Store::open("").unwrap();
        store.kv_put(BUNDLE_KEY, &bundle.to_string()).unwrap();
        assert!(matches!(
            unlock_from_bundle(&store, "Sara", "1234", "00000000-0000-0000-0000-000000000001"),
            Err(CoreError::Unauthenticated { .. })
        ));
    }

    #[test]
    fn unlock_missing_org_config_yields_empty_currency_and_zero_tax() {
        // No ORG_CONFIG_KEY stored → currency "" and tax 0.0, but unlock still works.
        let store = store_with_teller("Sara", "1234", true);
        let s = unlock_from_bundle(&store, "Sara", "1234", "00000000-0000-0000-0000-000000000001").unwrap();
        assert_eq!(s.snapshot.currency_code, "");
        assert_eq!(s.snapshot.tax_rate, 0.0);
    }

    #[test]
    fn unlock_passes_through_branch_id_argument() {
        let store = store_with_teller("Sara", "1234", true);
        let s = unlock_from_bundle(&store, "Sara", "1234", "branch-xyz").unwrap();
        // branch_id comes from the caller (device config), not the bundle.
        assert_eq!(s.snapshot.branch_id.as_deref(), Some("branch-xyz"));
    }

    #[test]
    fn unlock_malformed_bundle_json_is_internal_error() {
        let store = Store::open("").unwrap();
        store.kv_put(BUNDLE_KEY, "{ this is not valid json").unwrap();
        // serde error → From → CoreError::Internal.
        assert!(matches!(
            unlock_from_bundle(&store, "Sara", "1234", "00000000-0000-0000-0000-000000000001"),
            Err(CoreError::Internal { .. })
        ));
    }

    #[test]
    fn unlock_picks_correct_teller_among_many() {
        let phc_a = backend_hash("1111");
        let phc_b = backend_hash("2222");
        let bundle = serde_json::json!({
            "org_id": "00000000-0000-0000-0000-0000000000aa",
            "generated_at": "2026-06-19T10:00:00Z",
            "tellers": [
                {"user_id": "00000000-0000-0000-0000-0000000000b1",
                 "name": "Alice", "role": "teller", "is_active": true, "offline_pin_hash": phc_a},
                {"user_id": "00000000-0000-0000-0000-0000000000b2",
                 "name": "Bob", "role": "manager", "is_active": true, "offline_pin_hash": phc_b},
            ]
        });
        let store = Store::open("").unwrap();
        store.kv_put(BUNDLE_KEY, &bundle.to_string()).unwrap();

        let bob = unlock_from_bundle(&store, "Bob", "2222", "00000000-0000-0000-0000-000000000001").unwrap();
        assert_eq!(bob.snapshot.display_name, "Bob");
        assert_eq!(bob.snapshot.role, "manager");
        assert_eq!(bob.snapshot.user_id, "00000000-0000-0000-0000-0000000000b2");

        // Alice's PIN against Bob's name must fail (no cross-match).
        assert!(unlock_from_bundle(&store, "Bob", "1111", "00000000-0000-0000-0000-000000000001").is_err());
    }

    // ── verify_offline_pin ───────────────────────────────────────────────

    #[test]
    fn verify_offline_pin_accepts_correct_and_rejects_wrong() {
        let phc = backend_hash("4242");
        assert!(verify_offline_pin("4242", &phc));
        assert!(!verify_offline_pin("0000", &phc));
        assert!(!verify_offline_pin("", &phc));
    }

    #[test]
    fn verify_offline_pin_rejects_malformed_phc() {
        // A non-PHC string can't be parsed → false, never panics.
        assert!(!verify_offline_pin("4242", "not-a-phc-hash"));
        assert!(!verify_offline_pin("4242", ""));
    }

    // ── LoginMode ────────────────────────────────────────────────────────

    #[test]
    fn login_mode_is_copy_and_eq() {
        let a = LoginMode::Pin;
        let b = a; // Copy
        assert_eq!(a, b);
        assert_ne!(LoginMode::Pin, LoginMode::Email);
    }
}
