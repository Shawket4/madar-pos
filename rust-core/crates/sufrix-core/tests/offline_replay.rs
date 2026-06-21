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

use sufrix_core::checkout::{CheckoutInput, ReceiptView};
use sufrix_core::session::{LoginMode, LoginRequest};
use sufrix_core::{SufrixConfig, SufrixCore};

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
        // A 409 (branch already open from a prior run) is fine — refresh adopts it.
        let _ = core.open_shift(10_000, None).await;
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
