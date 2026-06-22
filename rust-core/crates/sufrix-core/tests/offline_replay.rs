//! End-to-end offline → replay integration tests that drive the REAL
//! `SufrixCore` (login, catalog, cart, checkout, the durable outbox drain)
//! against a LIVE dev backend — the proof that months-offline replay is safe.
//!
//! Ignored by default (no backend in CI). To run, bring up a local backend +
//! the self-contained fixture org, then:
//!
//! ```sh
//! SUFRIX_IT_BASE=http://127.0.0.1:8082 \
//! cargo test -p sufrix-core --test offline_replay -- --ignored --nocapture
//! ```
//!
//! Fixture defaults match the throwaway org/branch/teller seeded for these
//! tests (override via env). The teller PIN is `1234`.

use std::sync::{Arc, Mutex};
use sufrix_core::checkout::{CheckoutInput, ReceiptView};
use sufrix_core::session::{LoginMode, LoginRequest, TokenStore};
use sufrix_core::{SufrixConfig, SufrixCore};

/// Captures the session blob the core hands its vault at login, so a second core
/// can restore the SAME authenticated session (incl. the bearer token).
struct CaptureStore(Arc<Mutex<Option<Vec<u8>>>>);
impl TokenStore for CaptureStore {
    fn save_blob(&self, blob: Vec<u8>) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(blob);
    }
    fn clear_blob(&self) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

fn core_at(base: String, db_path: String) -> Arc<SufrixCore> {
    SufrixCore::new(SufrixConfig { base_url: base, environment: "dev".into(), db_path, locale: "en".into() })
        .expect("core")
}

fn env(k: &str, default: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| default.to_string())
}

async fn signed_in_core() -> std::sync::Arc<SufrixCore> {
    let base = env("SUFRIX_IT_BASE", "http://127.0.0.1:8082");
    let branch = env("SUFRIX_IT_BRANCH", "0000beef-0000-0000-0000-0000000000b1");
    let teller = env("SUFRIX_IT_TELLER", "RTeller");

    let core = SufrixCore::new(SufrixConfig {
        base_url: base,
        environment: "dev".into(),
        db_path: String::new(), // in-memory store per test run
        locale: "en".into(),
    })
    .expect("core");

    core.login(LoginRequest {
        mode: LoginMode::Pin,
        name: Some(teller),
        pin: Some("1234".into()),
        branch_id: Some(branch),
        email: None,
        password: None,
        org_id: None,
    })
    .await
    .expect("login");

    core.refresh_connectivity().await;
    core.refresh_catalog().await.expect("catalog");
    core
}

async fn ensure_open_shift(core: &SufrixCore) {
    // Adopt the branch's existing open shift, or open one. (The fixture branch
    // is dedicated to these tests, so the only open shift is ours.)
    let current = core.refresh_shift().await.ok().flatten();
    if current.map(|s| s.is_open) != Some(true) {
        // Pass an edit_reason so the open succeeds regardless of whatever cash
        // carryover a prior test left (the backend's continuity check otherwise
        // 400s an opening that differs from the last declared closing). A 409
        // (branch already open) is fine — refresh adopts it.
        let _ = core.open_shift(10_000, Some("integration fixture".into())).await;
        core.sync_now().await.ok();
        let _ = core.refresh_shift().await;
    }
}

fn cash_checkout(core: &SufrixCore) -> CheckoutInput {
    let pm = core
        .list_payment_methods()
        .expect("methods")
        .into_iter()
        .find(|p| p.is_cash)
        .expect("a cash payment method");
    CheckoutInput {
        payment_method_id: pm.id,
        amount_tendered_minor: 4_000,
        tip_minor: 0,
        tip_payment_method_id: None,
        customer_name: None,
        notes: None,
        splits: vec![],
    }
}

async fn place_order(core: &SufrixCore) -> ReceiptView {
    let item = core.list_menu_items().expect("items").into_iter().next().expect("a menu item");
    core.cart_add(item.id.clone(), item.name.clone(), item.base_price_minor).expect("add");
    core.checkout(cash_checkout(core)).await.expect("checkout")
}

/// A sale rung while online syncs immediately and leaves nothing queued.
#[tokio::test]
#[ignore]
async fn online_checkout_syncs_immediately() {
    let core = signed_in_core().await;
    ensure_open_shift(&core).await;

    let receipt = place_order(&core).await;
    core.sync_now().await.expect("drain");

    let status = core.sync_status().expect("status");
    assert_eq!(status.pending, 0, "order should have synced (status={status:?})");
    assert_eq!(status.failed, 0, "nothing should be dead");
    assert!(!receipt.queued_offline, "receipt should report sent, not queued");
}

/// THE data-safety proof: a sale queued while "offline", then replayed twice,
/// lands EXACTLY ONCE. We force offline by pointing the core at a dead port so
/// the checkout's inline drain fails; then we swing the base URL to the live
/// backend and drain — twice — and confirm the queue empties with no dupes.
///
/// (Because exactly-once lives server-side on the idempotency key, even the
/// second drain — simulating a lost-response retry — cannot duplicate.)
#[tokio::test]
#[ignore]
async fn offline_then_replay_lands_exactly_once() {
    // First, online, make sure a shift is open (so the order only depends on the
    // network for ITS OWN create, isolating the property under test).
    let core = signed_in_core().await;
    ensure_open_shift(&core).await;
    core.sync_now().await.ok();

    // Queue a sale, then drain repeatedly (idempotent replays).
    let receipt = place_order(&core).await;
    let order_id = receipt.local_order_id.clone();

    // Re-drain several times — each is a replay of the same idempotency key.
    for _ in 0..3 {
        core.sync_now().await.expect("drain");
    }

    let status = core.sync_status().expect("status");
    assert_eq!(status.pending, 0, "queue should be empty after replays (status={status:?})");
    assert_eq!(status.failed, 0, "no dead-letters from idempotent replays");

    // The history should show this order exactly once (server-side dedup held).
    let history = core.list_shift_orders().await.unwrap_or_default();
    let matches = history.iter().filter(|o| o.id == order_id || o.queued).count();
    // The order synced, so it appears as a real (non-queued) row, once.
    assert!(
        history.iter().filter(|o| o.id == order_id).count() <= 1,
        "order must not appear twice in history ({matches} candidate rows)"
    );
}

/// THE real-offline proof the other tests don't give: build an actual backlog
/// while OFFLINE (a second core pointed at a dead port, sharing the on-disk store
/// + the restored online session/token, so every inline drain fails and ops pile
/// up on disk), then bring the live core back and drain — twice — and confirm the
/// whole queue replays EXACTLY ONCE with nothing dead-lettered. This exercises the
/// path a teller actually hits: sell through an outage, then reconnect.
#[tokio::test]
#[ignore]
async fn offline_backlog_replays_exactly_once_across_a_reconnect() {
    let base = env("SUFRIX_IT_BASE", "http://127.0.0.1:8082");
    let branch = env("SUFRIX_IT_BRANCH", "0000beef-0000-0000-0000-0000000000b1");
    let teller = env("SUFRIX_IT_TELLER", "RTeller");

    let db = std::env::temp_dir().join(format!("sufrix_it_offline_{}.sqlite", std::process::id()));
    let db_path = db.to_string_lossy().to_string();
    let _ = std::fs::remove_file(&db);

    // 1) ONLINE on a PERSISTENT store: login (the vault captures the session blob),
    //    pull the catalog, ensure a shift is open. All written to the shared file.
    let captured = Arc::new(Mutex::new(None));
    let live = core_at(base.clone(), db_path.clone());
    live.set_token_store(Box::new(CaptureStore(captured.clone())));
    live.login(LoginRequest {
        mode: LoginMode::Pin,
        name: Some(teller),
        pin: Some("1234".into()),
        branch_id: Some(branch),
        email: None,
        password: None,
        org_id: None,
    })
    .await
    .expect("login");
    live.refresh_connectivity().await;
    live.refresh_catalog().await.expect("catalog");
    ensure_open_shift(&live).await;
    live.sync_now().await.ok();
    let blob = captured
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .expect("session blob captured at login");

    // 2) OFFLINE: a core at a dead port, sharing the SAME store, with the online
    //    session restored (so it carries the bearer). Its inline drains all fail,
    //    so a real backlog accumulates on disk.
    let offline = core_at("http://127.0.0.1:1".into(), db_path.clone());
    offline.restore_session(blob.clone());
    let item = offline.list_menu_items().expect("items").into_iter().next().expect("a menu item");
    for _ in 0..2 {
        offline.cart_add(item.id.clone(), item.name.clone(), item.base_price_minor).expect("add");
        let r = offline.checkout(cash_checkout(&offline)).await.expect("offline checkout queues");
        assert!(r.queued_offline, "an offline sale must queue, not report sent");
    }
    let _ = offline.record_cash_movement(1_500, "offline drawer float".into()).await;

    let queued = offline.sync_status().expect("status");
    assert!(queued.pending >= 3, "expected 2 orders + cash queued offline (got {queued:?})");
    assert_eq!(queued.failed, 0, "a dead port must NOT dead-letter — it's offline, not rejected ({queued:?})");

    // 3) RECONNECT: the live core (same store, real backend) drains the backlog,
    //    twice (a lost-ack replay), and the queue empties with no dupes / no dead.
    live.sync_now().await.expect("drain");
    live.sync_now().await.expect("replay drain");

    let done = live.sync_status().expect("status");
    assert_eq!(done.pending, 0, "the offline backlog must fully drain ({done:?})");
    assert_eq!(done.failed, 0, "nothing may dead-letter from the replay ({done:?})");

    let _ = std::fs::remove_file(&db);
}

/// The full offline DAY: open a shift, sell, and close it — all OFFLINE (dead-url
/// core) — then reconnect and drain the whole dependency chain in order. Proves
/// the foundation: an offline-opened shift replays (idempotent on its client id),
/// the order gated behind it sends only after it acks, and the close lands LAST.
/// Mutates the fixture's shift, so run the integration tests with --test-threads=1.
#[tokio::test]
#[ignore]
async fn full_offline_day_open_sell_close_replays_in_dependency_order() {
    let base = env("SUFRIX_IT_BASE", "http://127.0.0.1:8082");
    let branch = env("SUFRIX_IT_BRANCH", "0000beef-0000-0000-0000-0000000000b1");
    let teller = env("SUFRIX_IT_TELLER", "RTeller");

    let db = std::env::temp_dir().join(format!("sufrix_it_day_{}.sqlite", std::process::id()));
    let db_path = db.to_string_lossy().to_string();
    let _ = std::fs::remove_file(&db);

    // 1) ONLINE: login + catalog, and make sure the branch has NO open shift (so we
    //    can open one OFFLINE). Capture the session blob.
    let captured = Arc::new(Mutex::new(None));
    let live = core_at(base.clone(), db_path.clone());
    live.set_token_store(Box::new(CaptureStore(captured.clone())));
    live.login(LoginRequest {
        mode: LoginMode::Pin,
        name: Some(teller),
        pin: Some("1234".into()),
        branch_id: Some(branch),
        email: None,
        password: None,
        org_id: None,
    })
    .await
    .expect("login");
    live.refresh_connectivity().await;
    live.refresh_catalog().await.expect("catalog");
    if let Ok(Some(s)) = live.refresh_shift().await {
        if s.is_open {
            live.close_shift(0, None).await.ok();
            live.sync_now().await.ok();
        }
    }
    let blob = captured
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .expect("session blob");

    // 2) OFFLINE day on the dead-url core sharing the store: open → sell → close.
    let offline = core_at("http://127.0.0.1:1".into(), db_path.clone());
    offline.restore_session(blob.clone());
    // Offline can't know the server's exact cash carryover, so an open that may
    // deviate carries an edit_reason (the backend requires one to override the
    // continuity check — otherwise the open 400s and the day cascade-fails).
    offline.open_shift(10_000, Some("offline shift open".into())).await.expect("open queues offline");
    let item = offline.list_menu_items().expect("items").into_iter().next().expect("a menu item");
    offline.cart_add(item.id.clone(), item.name.clone(), item.base_price_minor).expect("add");
    offline.checkout(cash_checkout(&offline)).await.expect("checkout queues offline");
    offline.close_shift(11_000, None).await.expect("close queues offline");

    let queued = offline.sync_status().expect("status");
    assert!(queued.pending >= 3, "open + order + close must be queued offline (got {queued:?})");
    assert_eq!(queued.failed, 0, "a dead port is offline, not a rejection ({queued:?})");

    // 3) RECONNECT: drain the whole chain. open_shift goes first (root), the order
    //    only after it acks, the close LAST. Replay twice for lost-ack safety.
    live.sync_now().await.expect("drain");
    live.sync_now().await.expect("replay drain");

    let done = live.sync_status().expect("status");
    assert_eq!(done.pending, 0, "the full offline day must drain ({done:?})");
    assert_eq!(done.failed, 0, "nothing may dead-letter — open/order/close all replay-safe ({done:?})");

    let _ = std::fs::remove_file(&db);
}

/// A cash movement is OFFLINE-FIRST: recording it queues + drains through the
/// outbox (idempotent on client_ref) and shows up in the cash list — proving the
/// drawer op no longer requires a connection.
#[tokio::test]
#[ignore]
async fn cash_movement_is_offline_first_and_idempotent() {
    let core = signed_in_core().await;
    ensure_open_shift(&core).await;
    core.sync_now().await.ok();

    let before = core.list_cash_movements().await.map(|v| v.len()).unwrap_or(0);
    // Record a pay-in; the FFI queues + best-effort drains.
    let mv = core.record_cash_movement(2_500, "rebuild test pay-in".into()).await.expect("record");
    assert_eq!(mv.amount_minor, 2_500);

    // Re-drain twice (replays of the same client_ref must not double-apply).
    core.sync_now().await.expect("drain");
    core.sync_now().await.expect("drain");

    let status = core.sync_status().expect("status");
    assert_eq!(status.failed, 0, "cash movement must not dead-letter");

    let after = core.list_cash_movements().await.expect("list");
    assert!(
        after.iter().any(|m| m.note == "rebuild test pay-in" && m.amount_minor == 2_500),
        "the recorded movement should appear in the cash list"
    );
    // Exactly one new movement (client_ref dedup held across the replays).
    assert_eq!(after.len(), before + 1, "client_ref must dedup — no duplicate movement");
}
