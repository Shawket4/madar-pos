//! End-to-end offline → replay integration tests that drive the REAL
//! `MadarCore` (login, catalog, cart, checkout, the durable outbox drain)
//! against a LIVE dev backend — the proof that months-offline replay is safe.
//!
//! Ignored by default (no backend in CI). To run, bring up a local backend +
//! the self-contained fixture org, then:
//!
//! ```sh
//! MADAR_IT_BASE=http://127.0.0.1:8082 \
//! cargo test -p madar-core --test offline_replay -- --ignored --nocapture
//! ```
//!
//! Fixture defaults match the throwaway org/branch/teller seeded for these
//! tests (override via env). The teller PIN is `1234`.

use std::sync::{Arc, Mutex};
use madar_core::checkout::{CheckoutInput, ReceiptView};
use madar_core::session::{LoginMode, LoginRequest, TokenStore};
use madar_core::{MadarConfig, MadarCore};

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

fn core_at(base: String, db_path: String) -> Arc<MadarCore> {
    MadarCore::new(MadarConfig { base_url: base, environment: "dev".into(), db_path, locale: "en".into() })
        .expect("core")
}

fn env(k: &str, default: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| default.to_string())
}

/// A fully-ISOLATED test fixture: a freshly-provisioned branch + two tellers
/// (sharing the seeded bcrypt PIN `1234`), inserted straight into the dev Postgres.
/// Each test gets its own, so there's NO shared mutable state — the global "one
/// open shift per teller" DB constraint can't leak an open shift from one test into
/// another (the cross-test contamination that made the serialized suite flaky).
struct Fixture {
    base: String,
    branch: String,
    t1: String,
    t1_id: String,
    t2: String,
    t2_id: String,
    db: tokio_postgres::Client,
    branch_uuid: uuid::Uuid,
    teller_uuids: Vec<uuid::Uuid>,
}

impl Fixture {
    /// Best-effort teardown — delete this branch's data + the throwaway rows so the
    /// dev DB stays tidy. Fresh UUIDs mean a SKIPPED cleanup (on a panicking test)
    /// never contaminates another test; this is purely housekeeping. FK delete order:
    /// orders → shifts → branch (order_items + shift_cash_movements cascade), then
    /// the tellers (no shift references them once the shifts are gone).
    async fn cleanup(&self) {
        let b = &self.branch_uuid;
        let _ = self.db.execute("DELETE FROM orders WHERE branch_id = $1", &[b]).await;
        let _ = self.db.execute("DELETE FROM shifts WHERE branch_id = $1", &[b]).await;
        let _ = self.db.execute("DELETE FROM branches WHERE id = $1", &[b]).await;
        let _ = self.db.execute("DELETE FROM users WHERE id = ANY($1)", &[&self.teller_uuids]).await;
    }
}

/// Provision a throwaway branch + two tellers in the dev DB. The org + a verified
/// bcrypt('1234') PIN hash are COPIED from any seeded teller, so online PIN login
/// works (and the backend derives the offline_pin_hash on first login). Requires
/// the dev Postgres (`MADAR_IT_DB`, or the dev default).
async fn provision_fixture() -> Fixture {
    let base = env("MADAR_IT_BASE", "http://127.0.0.1:8082");
    // No real credentials in source — set MADAR_IT_DB in your local env/.env to run these
    // (ignored) integration tests. The default is a non-secret local placeholder.
    let db_url = env("MADAR_IT_DB", "postgres://madar:madar@localhost:5432/madar_dev");
    let (db, conn) = tokio_postgres::connect(&db_url, tokio_postgres::NoTls).await.expect("dev DB connect");
    tokio::spawn(async move {
        let _ = conn.await;
    });

    // Borrow the org + a valid bcrypt('1234') pin hash from the canonical seed teller.
    // It MUST be a teller whose PIN is actually '1234' (the new tellers inherit this
    // hash and then log in with '1234'); a bare `LIMIT 1` over all tellers can land on
    // a different org's teller whose PIN isn't '1234', which fails login. `RTeller` is
    // the seeded PIN-1234 teller these tests were built around.
    let seed = db
        .query_one(
            "SELECT org_id, pin_hash FROM users \
             WHERE role = 'teller' AND org_id IS NOT NULL AND pin_hash IS NOT NULL \
               AND name = 'RTeller' LIMIT 1",
            &[],
        )
        .await
        .expect("the seeded 'RTeller' PIN-1234 teller (run the dev seed first)");
    let org_id: uuid::Uuid = seed.get(0);
    let pin_hash: String = seed.get(1);

    // Fresh branch — the BEFORE INSERT trigger fills `code`; Cairo tz for the tests.
    // The name is derived from the fresh UUID (like the tellers below) so it's UNIQUE
    // per fixture: the org has a UNIQUE(org_id, name) constraint, and every test
    // provisions its own branch under the same shared org.
    let branch_uuid = uuid::Uuid::new_v4();
    let branch_name = format!("IT branch {}", &branch_uuid.simple().to_string()[..8]);
    db.execute(
        "INSERT INTO branches (id, org_id, name, timezone) VALUES ($1, $2, $3, 'Africa/Cairo')",
        &[&branch_uuid, &org_id, &branch_name],
    )
    .await
    .expect("insert branch");

    // Two fresh tellers (PIN 1234 via the copied hash) — enough for the switch tests.
    let mut tellers: Vec<(uuid::Uuid, String)> = Vec::new();
    for _ in 0..2 {
        let id = uuid::Uuid::new_v4();
        let name = format!("IT-{}", &id.simple().to_string()[..8]);
        db.execute(
            "INSERT INTO users (id, org_id, name, role, pin_hash) \
             VALUES ($1, $2, $3, 'teller'::public.user_role, $4)",
            &[&id, &org_id, &name, &pin_hash],
        )
        .await
        .expect("insert teller");
        tellers.push((id, name));
    }

    Fixture {
        base,
        branch: branch_uuid.to_string(),
        t1: tellers[0].1.clone(),
        t1_id: tellers[0].0.to_string(),
        t2: tellers[1].1.clone(),
        t2_id: tellers[1].0.to_string(),
        db,
        branch_uuid,
        teller_uuids: vec![tellers[0].0, tellers[1].0],
    }
}

async fn signed_in_core(fx: &Fixture) -> std::sync::Arc<MadarCore> {
    let core = MadarCore::new(MadarConfig {
        base_url: fx.base.clone(),
        environment: "dev".into(),
        db_path: String::new(), // in-memory store per test run
        locale: "en".into(),
    })
    .expect("core");

    core.login(LoginRequest {
        mode: LoginMode::Pin,
        name: Some(fx.t1.clone()),
        pin: Some("1234".into()),
        branch_id: Some(fx.branch.clone()),
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

async fn ensure_open_shift(core: &MadarCore) {
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

fn cash_checkout(core: &MadarCore) -> CheckoutInput {
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

async fn place_order(core: &MadarCore) -> ReceiptView {
    let item = core.list_menu_items().expect("items").into_iter().next().expect("a menu item");
    core.cart_add(item.id.clone(), item.name.clone(), item.base_price_minor).expect("add");
    core.checkout(cash_checkout(core)).await.expect("checkout")
}

/// REPRO (#1): offline login. Online login on a persistent store must cache the
/// org's offline-auth bundle (incl. THIS teller's freshly-derived offline_pin_hash);
/// then a fresh core sharing that store, with the network gone, must be able to
/// `unlock_offline(RTeller, 1234, branch)`. Prints the exact CoreError on failure
/// so we can fix the real cause rather than guess.
#[tokio::test]
#[ignore]
async fn offline_login_unlocks_after_an_online_login() {
    let fx = provision_fixture().await;
    let base = fx.base.clone();
    let branch = fx.branch.clone();
    let teller = fx.t1.clone();

    let db = std::env::temp_dir().join(format!("madar_it_offlogin_{}.sqlite", std::process::id()));
    let db_path = db.to_string_lossy().to_string();
    let _ = std::fs::remove_file(&db);

    // 1) ONLINE login on the shared store — this is what caches the bundle.
    let live = core_at(base.clone(), db_path.clone());
    live.login(LoginRequest {
        mode: LoginMode::Pin,
        name: Some(teller.clone()),
        pin: Some("1234".into()),
        branch_id: Some(branch.clone()),
        email: None,
        password: None,
        org_id: None,
    })
    .await
    .expect("online login");

    // 2) FRESH core, same store, network GONE (dead port). Try the offline unlock.
    let offline = core_at("http://127.0.0.1:1".into(), db_path.clone());
    let r = offline.unlock_offline(teller.clone(), "1234".into(), branch.clone());
    match &r {
        Ok(s) => eprintln!("OFFLINE-LOGIN OK: user_id={} name={} org={:?}", s.user_id, s.display_name, s.org_id),
        Err(e) => eprintln!("OFFLINE-LOGIN ERR: {e:?}"),
    }

    // 3) And the way the host actually calls it: sign_in falls through to offline.
    let offline2 = core_at("http://127.0.0.1:1".into(), db_path.clone());
    let r2 = offline2
        .sign_in(LoginRequest {
            mode: LoginMode::Pin,
            name: Some(teller.clone()),
            pin: Some("1234".into()),
            branch_id: Some(branch.clone()),
            email: None,
            password: None,
            org_id: None,
        })
        .await;
    match &r2 {
        Ok(s) => eprintln!("SIGN_IN(offline) OK: user_id={} name={}", s.user_id, s.display_name),
        Err(e) => eprintln!("SIGN_IN(offline) ERR: {e:?}"),
    }

    fx.cleanup().await;
    let _ = std::fs::remove_file(&db);
    r.expect("offline unlock should succeed after an online login cached the bundle");
    r2.expect("sign_in should fall back to an offline unlock");
}

/// REPRO (#1, the field bug): a CAPTIVE PORTAL — a mall/cafe WiFi splash that
/// answers `POST /auth/login` with an HTML 200 instead of our JSON. The online
/// login decodes to an `Internal{decode:…}`, which the OLD `sign_in` propagated
/// (no offline fallback) → the teller was stranded on the login screen even
/// though a valid offline bundle was cached. After the fix, `sign_in` recognizes
/// the portal as "never reached the backend" and unlocks offline.
#[tokio::test]
#[ignore]
async fn captive_portal_login_falls_back_to_offline() {
    use std::io::{Read, Write};

    let fx = provision_fixture().await;
    let base = fx.base.clone();
    let branch = fx.branch.clone();
    let teller = fx.t1.clone();

    let db = std::env::temp_dir().join(format!("madar_it_portal_{}.sqlite", std::process::id()));
    let db_path = db.to_string_lossy().to_string();
    let _ = std::fs::remove_file(&db);

    // 1) ONLINE login on the shared store — caches the offline bundle.
    let live = core_at(base.clone(), db_path.clone());
    live.login(LoginRequest {
        mode: LoginMode::Pin,
        name: Some(teller.clone()),
        pin: Some("1234".into()),
        branch_id: Some(branch.clone()),
        email: None,
        password: None,
        org_id: None,
    })
    .await
    .expect("online login");

    // 2) Stand up a captive portal: every request gets an HTML 200 splash page.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind portal");
    let portal_url = format!("http://{}", listener.local_addr().unwrap());
    let portal = std::thread::spawn(move || {
        // Serve a handful of connections, then exit (the test only logs in once or
        // twice). Each gets the splash, exactly what a real portal returns.
        for _ in 0..4 {
            match listener.accept() {
                Ok((mut sock, _)) => {
                    let mut buf = [0u8; 1024];
                    let _ = sock.read(&mut buf); // drain the request line/headers
                    let body = "<html><body>Sign in to GuestWiFi to continue</body></html>";
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = sock.write_all(resp.as_bytes());
                    let _ = sock.flush();
                }
                Err(_) => break,
            }
        }
    });

    // 3) Fresh core behind the portal, sharing the store. sign_in must unlock OFFLINE.
    let behind_portal = core_at(portal_url, db_path.clone());
    let session = behind_portal
        .sign_in(LoginRequest {
            mode: LoginMode::Pin,
            name: Some(teller.clone()),
            pin: Some("1234".into()),
            branch_id: Some(branch.clone()),
            email: None,
            password: None,
            org_id: None,
        })
        .await
        .expect("captive-portal sign-in must fall back to an offline unlock");
    assert!(!session.online, "a portal-fallback session is offline, not online");
    assert_eq!(session.display_name, teller, "unlocked as the right teller");

    drop(portal); // the thread exits on its own once connections stop
    fx.cleanup().await;
    let _ = std::fs::remove_file(&db);
}

/// A sale rung while online syncs immediately and leaves nothing queued.
#[tokio::test]
#[ignore]
async fn online_checkout_syncs_immediately() {
    let fx = provision_fixture().await;
    let core = signed_in_core(&fx).await;
    ensure_open_shift(&core).await;

    let receipt = place_order(&core).await;
    core.sync_now().await.expect("drain");

    let status = core.sync_status().expect("status");
    assert_eq!(status.pending, 0, "order should have synced (status={status:?})");
    assert_eq!(status.failed, 0, "nothing should be dead");
    assert!(!receipt.queued_offline, "receipt should report sent, not queued");
    fx.cleanup().await;
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
    let fx = provision_fixture().await;
    let core = signed_in_core(&fx).await;
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
    fx.cleanup().await;
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
    let fx = provision_fixture().await;
    let base = fx.base.clone();
    let branch = fx.branch.clone();
    let teller = fx.t1.clone();

    let db = std::env::temp_dir().join(format!("madar_it_offline_{}.sqlite", std::process::id()));
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

    fx.cleanup().await;
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
    let fx = provision_fixture().await;
    let base = fx.base.clone();
    let branch = fx.branch.clone();
    let teller = fx.t1.clone();

    let db = std::env::temp_dir().join(format!("madar_it_day_{}.sqlite", std::process::id()));
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

    fx.cleanup().await;
    let _ = std::fs::remove_file(&db);
}

/// THE multi-teller fix (#3), end to end: teller A opens a shift ONLINE, the
/// network drops, A closes it + opens another + sells — all queued — then teller
/// B (a DIFFERENT teller) signs in on the same till and flushes the whole backlog.
/// Pre-fix this was impossible twice over: B couldn't even sign in (the branch
/// still showed A's open shift), and the drain was teller-scoped so only A's token
/// could flush A's ops. After the fix B signs in freely and `/sync/replay`
/// attributes every op to its EMBEDDED teller — so the reopened shift lands under
/// A, not B. This is the literal scenario the user reported.
#[tokio::test]
#[ignore]
async fn another_teller_flushes_the_backlog_attributed_to_the_original() {
    let fx = provision_fixture().await;
    let base = fx.base.clone();
    let branch = fx.branch.clone();
    let teller_a = fx.t1.clone();
    let teller_a_id = fx.t1_id.clone();
    let teller_b = fx.t2.clone();
    let teller_b_id = fx.t2_id.clone();

    let db = std::env::temp_dir().join(format!("madar_it_xteller_{}.sqlite", std::process::id()));
    let db_path = db.to_string_lossy().to_string();
    let _ = std::fs::remove_file(&db);

    let login = |name: &str| LoginRequest {
        mode: LoginMode::Pin,
        name: Some(name.into()),
        pin: Some("1234".into()),
        branch_id: Some(branch.clone()),
        email: None,
        password: None,
        org_id: None,
    };

    // 1) Teller A signs in ONLINE on a persistent store, catalog loaded. Start
    //    from a clean slate (close any leftover open shift), then open S1 ONLINE
    //    so the SERVER genuinely shows A's shift open at the branch.
    let captured = Arc::new(Mutex::new(None));
    let a = core_at(base.clone(), db_path.clone());
    a.set_token_store(Box::new(CaptureStore(captured.clone())));
    a.login(login(&teller_a)).await.expect("A login");
    a.refresh_connectivity().await;
    a.refresh_catalog().await.expect("catalog");
    if let Ok(Some(s)) = a.refresh_shift().await {
        if s.is_open {
            a.close_shift(0, None).await.ok();
            a.sync_now().await.ok();
        }
    }
    a.open_shift(10_000, Some("xteller fixture".into())).await.expect("A opens S1 online");
    a.sync_now().await.expect("S1 syncs");
    let s1 = a.refresh_shift().await.expect("refresh").expect("S1");
    assert!(s1.is_open && s1.teller_id == teller_a_id, "S1 is open and owned by A server-side");
    let blob = captured.lock().unwrap_or_else(|e| e.into_inner()).clone().expect("A session blob");

    // S1 was opened ONLINE (server-stamped opened_at); the offline close below is
    // device-stamped. On a brand-new fixture both land in the same wall-clock second,
    // and the backend rejects closed_at < opened_at — so guarantee a >1s gap.
    tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;

    // 2) OFFLINE (dead-url core, A's session restored, shared store): A closes S1,
    //    opens S2, and rings a sale — all queued, each stamped with A's teller id.
    let offline = core_at("http://127.0.0.1:1".into(), db_path.clone());
    offline.restore_session(blob.clone());
    offline.close_shift(11_000, None).await.expect("A closes S1 offline");
    offline.open_shift(11_000, Some("S2 offline".into())).await.expect("A opens S2 offline");
    let item = offline.list_menu_items().expect("items").into_iter().next().expect("a menu item");
    offline.cart_add(item.id.clone(), item.name.clone(), item.base_price_minor).expect("add");
    offline.checkout(cash_checkout(&offline)).await.expect("sale queues offline");
    let queued = offline.sync_status().expect("status");
    assert!(queued.pending >= 3, "close S1 + open S2 + order must be queued (got {queued:?})");

    // 3) Teller B signs in on the SAME till. The server still shows A's S1 open —
    //    the relaxed login guard must let B in anyway (the queued close will land).
    let b = core_at(base.clone(), db_path.clone());
    let b_session = b.login(login(&teller_b)).await.expect("B must be able to sign in despite A's open shift");
    assert_eq!(b_session.user_id, teller_b_id, "signed in as B");
    b.refresh_connectivity().await;

    // 4) B drains the WHOLE backlog (A's ops) via /sync/replay. Twice for lost-ack.
    b.sync_now().await.expect("B drains A's backlog");
    b.sync_now().await.expect("replay drain");
    let done = b.sync_status().expect("status");
    assert_eq!(done.pending, 0, "B must fully flush A's backlog ({done:?})");
    assert_eq!(done.failed, 0, "nothing may dead-letter on the cross-teller replay ({done:?})");

    // 5) Attribution proof — verified via the OWNER. Shift state is now
    //    TELLER-SCOPED: B, who only FLUSHED the backlog, does not own S2, so B's
    //    current shift is empty (they're correctly NOT dropped into a shift that
    //    isn't theirs). A signs in (A owns S2 → allowed) and sees S2 as their
    //    active shift, attributed to A.
    let bshift = b.refresh_shift().await.ok().flatten();
    assert!(bshift.is_none(), "B must NOT be placed in a shift they don't own (got {bshift:?})");

    let a2 = core_at(base.clone(), db_path.clone());
    a2.login(login(&teller_a)).await.expect("A re-login (owns S2)");
    a2.refresh_connectivity().await;
    let s2 = a2.refresh_shift().await.expect("refresh").expect("A's S2 is open");
    assert!(s2.is_open, "S2 is open server-side after replay");
    assert_eq!(s2.teller_id, teller_a_id, "S2 attributed to A — the teller who opened it");
    let _ = teller_b_id; // (used above for the sign-in assertion)

    // Cleanup: A closes S2 so the shared fixture is left tidy for other tests.
    a2.close_shift(0, None).await.ok();
    a2.sync_now().await.ok();

    fx.cleanup().await;
    let _ = std::fs::remove_file(&db);
}

/// REPRO of the user's "deeper" multi-teller bug: teller 1 opens a shift ONLINE,
/// goes offline, CLOSES it, switches to teller 2 (sign-out + offline unlock), who
/// OPENS a new shift — then teller 2 comes back online. Expected: after the drain
/// the server has teller 2's S2 open (S1 closed), so teller 2 routes into S2 and
/// teller 1 has no active shift. The bug report: teller 2 lands on open-shift and
/// teller 1 is thrown into a still-"open" S1 — i.e. the offline close/open never
/// took effect server-side. This test asserts the server's real shift state.
#[tokio::test]
#[ignore]
async fn offline_teller_switch_close_open_syncs_on_reconnect() {
    let fx = provision_fixture().await;
    let base = fx.base.clone();
    let branch = fx.branch.clone();
    let t1 = fx.t1.clone();
    let t1_id = fx.t1_id.clone();
    let t2 = fx.t2.clone();
    let t2_id = fx.t2_id.clone();

    let login_req = |name: &str| LoginRequest {
        mode: LoginMode::Pin,
        name: Some(name.into()),
        pin: Some("1234".into()),
        branch_id: Some(branch.clone()),
        email: None, password: None, org_id: None,
    };

    let db = std::env::temp_dir().join(format!("madar_it_tswitch_{}.sqlite", std::process::id()));
    let db_path = db.to_string_lossy().to_string();
    let _ = std::fs::remove_file(&db);

    // 1) FIRST warm t2's offline_pin_hash (the backend derives it only on an online
    //    PIN login). t2 is a brand-new fixture teller, so this MUST happen before t1's
    //    login caches the org's offline-auth bundle — otherwise the cached bundle
    //    predates t2's hash and the later offline unlock can't verify it. (On the old
    //    shared fixture t2's hash was already derived in a prior run, masking this.)
    let warmdb = std::env::temp_dir().join(format!("madar_it_warm_{}.sqlite", std::process::id()));
    let warm = core_at(base.clone(), warmdb.to_string_lossy().into());
    warm.login(login_req(&t2)).await.expect("warm t2 online");
    let _ = std::fs::remove_file(&warmdb);

    // Teller 1 ONLINE on the shared store: caches the bundle (now incl. t2's hash).
    // Start from a CLEAN branch (close any leftover open shift — it's t1's own, so t1
    // may log in and close it; the new ownership guard would otherwise reject a
    // different teller).
    let captured = Arc::new(Mutex::new(None));
    let live = core_at(base.clone(), db_path.clone());
    live.set_token_store(Box::new(CaptureStore(captured.clone())));
    live.login(login_req(&t1)).await.expect("t1 login");
    live.refresh_connectivity().await;
    live.refresh_catalog().await.expect("catalog");
    if let Ok(Some(s)) = live.refresh_shift().await {
        if s.is_open { live.close_shift(0, None).await.ok(); live.sync_now().await.ok(); }
    }

    live.open_shift(10_000, Some("t1 S1".into())).await.expect("t1 opens S1");
    live.sync_now().await.expect("S1 syncs");
    let s1 = live.refresh_shift().await.expect("refresh").expect("S1");
    assert!(s1.is_open && s1.teller_id == t1_id, "S1 open by t1");
    let blob = captured.lock().unwrap_or_else(|e| e.into_inner()).clone().expect("blob");

    // S1 was opened ONLINE (server-stamped opened_at); the offline close below is
    // device-stamped. On a brand-new fixture both land in the same wall-clock second,
    // and the backend rejects closed_at < opened_at — so guarantee a >1s gap.
    tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;

    // 2) OFFLINE (dead url, shared store): t1 closes S1, signs out, t2 unlocks
    //    offline and opens S2.
    let off = core_at("http://127.0.0.1:1".into(), db_path.clone());
    off.restore_session(blob);
    off.close_shift(11_000, None).await.expect("t1 closes S1 offline");
    off.logout(false).expect("sign out preserves the outbox");
    off.unlock_offline(t2.clone().into(), "1234".into(), branch.clone()).expect("t2 offline unlock");
    off.open_shift(11_000, Some("t2 S2".into())).await.expect("t2 opens S2 offline");
    let q = off.sync_status().expect("status");
    assert!(q.pending >= 2, "close S1 + open S2 must be queued ({q:?})");

    // 3) Teller 2 comes back ONLINE (login drains the backlog). Then read the
    //    server's authoritative shift.
    let t2core = core_at(base.clone(), db_path.clone());
    t2core.login(login_req(&t2)).await.expect("t2 online login");
    t2core.refresh_connectivity().await;
    t2core.sync_now().await.ok();
    let shift = t2core.refresh_shift().await.expect("refresh");
    eprintln!("AFTER RECONNECT: server shift = {shift:?}");
    let shift = shift.expect("there should be an active shift (t2's S2)");
    assert!(shift.is_open, "an active shift must exist after reconnect");
    assert_eq!(
        shift.teller_id, t2_id,
        "the branch's active shift must be teller 2's S2 — NOT teller 1's S1 (close didn't sync) — got teller {}",
        shift.teller_id
    );
    assert_ne!(shift.teller_id, t1_id, "teller 1's S1 must be CLOSED server-side");

    // Cleanup: close S2 as t2 so the fixture is tidy.
    t2core.close_shift(0, None).await.ok();
    t2core.sync_now().await.ok();
    fx.cleanup().await;
    let _ = std::fs::remove_file(&db);
}

/// REPRO: an offline-opened shift that can never be created server-side (the
/// branch already had the teller's REAL shift open — opened on another device, or
/// after a local cache loss) dead-letters its `open_shift` and cascades its
/// orders. On reconnect, reconcile must ADOPT the real shift AND re-point the
/// orphaned ops onto it, so the offline SALES are recovered instead of stranded.
/// This is the data-loss case from the field (dead open + "a required earlier
/// action failed to sync" orders).
#[tokio::test]
#[ignore]
async fn offline_orphaned_orders_recover_onto_the_real_shift() {
    let fx = provision_fixture().await;
    let base = fx.base.clone();
    let branch = fx.branch.clone();
    let teller = fx.t1.clone();
    let login_req = LoginRequest {
        mode: LoginMode::Pin,
        name: Some(teller.clone()),
        pin: Some("1234".into()),
        branch_id: Some(branch.clone()),
        email: None, password: None, org_id: None,
    };

    // Device OTHER opens the teller's REAL shift A online and leaves it open.
    let otherdb = std::env::temp_dir().join(format!("madar_it_orphan_other_{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&otherdb);
    let captured = Arc::new(Mutex::new(None));
    let other = core_at(base.clone(), otherdb.to_string_lossy().into());
    other.set_token_store(Box::new(CaptureStore(captured.clone())));
    other.login(login_req.clone()).await.expect("other login");
    other.refresh_connectivity().await;
    other.refresh_catalog().await.expect("catalog");
    if let Ok(Some(s)) = other.refresh_shift().await {
        if s.is_open { other.close_shift(0, None).await.ok(); other.sync_now().await.ok(); }
    }
    other.open_shift(10_000, Some("real A".into())).await.expect("open A");
    other.sync_now().await.expect("A syncs");
    let a = other.refresh_shift().await.expect("refresh").expect("A");
    assert!(a.is_open, "A open server-side");
    let blob = captured.lock().unwrap_or_else(|e| e.into_inner()).clone().expect("blob");

    // Device MAIN gets the catalog via restore_session WITHOUT reconciling the
    // shift, so it never learns about A — mimicking a fresh device / cache loss.
    let maindb = std::env::temp_dir().join(format!("madar_it_orphan_main_{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&maindb);
    let main_online = core_at(base.clone(), maindb.to_string_lossy().into());
    main_online.restore_session(blob.clone());
    main_online.refresh_catalog().await.expect("main catalog");
    assert!(!main_online.list_menu_items().unwrap().is_empty(), "catalog present on main");

    // OFFLINE: main opens B' (it believes no shift is open) and rings an order.
    let off = core_at("http://127.0.0.1:1".into(), maindb.to_string_lossy().into());
    off.restore_session(blob.clone());
    off.open_shift(10_000, Some("offline B".into())).await.expect("open B offline");
    let item = off.list_menu_items().unwrap().into_iter().next().expect("item");
    off.cart_add(item.id.clone(), item.name.clone(), item.base_price_minor).unwrap();
    off.checkout(cash_checkout(&off)).await.expect("offline order");
    assert!(off.sync_status().unwrap().pending >= 2, "open B + order queued offline");

    // RECONNECT (restore the session rather than re-login, to spare the login rate
    // limiter): the drain dead-letters open B' + cascades the order, then reconcile
    // adopts A and recovers them onto it.
    let main = core_at(base.clone(), maindb.to_string_lossy().into());
    main.restore_session(blob.clone());
    main.refresh_connectivity().await; // pings online + drains (B' dead-letters)
    main.sync_now().await.ok();
    let _ = main.refresh_shift().await; // adopts A + remaps the orphan + requeues + drains
    main.sync_now().await.ok();

    let q = main.sync_status().expect("status");
    eprintln!("AFTER RECOVERY: {q:?}");
    assert_eq!(q.failed, 0, "no dead/orphaned ops remain — offline sales recovered ({q:?})");
    assert_eq!(q.pending, 0, "everything synced after recovery ({q:?})");
    let cur = main.refresh_shift().await.expect("refresh").expect("active shift");
    assert!(cur.is_open && cur.id == a.id, "main routes into the REAL shift A ({} vs {})", cur.id, a.id);

    // Cleanup: close A so the fixture branch is tidy.
    main.close_shift(0, None).await.ok();
    main.sync_now().await.ok();
    fx.cleanup().await;
    let _ = std::fs::remove_file(&maindb);
    let _ = std::fs::remove_file(&otherdb);
}

/// INVARIANT: the order_number + order_ref shown on the OFFLINE post-checkout
/// receipt must equal what the server mints — so the receipt at ring-up matches the
/// reprint, online AND offline. Proves the deterministic per-shift order_ref
/// (BRANCH-YYMMDD-SHIFT6-NNN) is computed identically on device and server.
#[tokio::test]
#[ignore]
async fn offline_receipt_number_and_ref_match_the_server() {
    let fx = provision_fixture().await;
    let base = fx.base.clone();
    let branch = fx.branch.clone();
    let teller = fx.t1.clone();
    let login_req = LoginRequest {
        mode: LoginMode::Pin, name: Some(teller), pin: Some("1234".into()),
        branch_id: Some(branch), email: None, password: None, org_id: None,
    };
    let db = std::env::temp_dir().join(format!("madar_it_refmatch_{}.sqlite", std::process::id()));
    let db_path = db.to_string_lossy().to_string();
    let _ = std::fs::remove_file(&db);

    // Online: open a shift + ring one order so branch_code / timezone / base cache.
    let captured = Arc::new(Mutex::new(None));
    let live = core_at(base.clone(), db_path.clone());
    live.set_token_store(Box::new(CaptureStore(captured.clone())));
    live.login(login_req.clone()).await.expect("login");
    live.refresh_connectivity().await;
    live.refresh_catalog().await.expect("catalog");
    // Start from a FRESH shift so the per-shift order_number predicts from #1
    // (login already cached branch_code+timezone, so the offline mint works).
    if let Ok(Some(s)) = live.refresh_shift().await {
        if s.is_open { live.close_shift(0, None).await.ok(); live.sync_now().await.ok(); }
    }
    live.open_shift(10_000, Some("refmatch fixture".into())).await.expect("open shift");
    live.sync_now().await.expect("shift syncs");
    let shift_id = live.refresh_shift().await.ok().flatten().expect("shift").id;
    let blob = captured.lock().unwrap_or_else(|e| e.into_inner()).clone().expect("blob");

    // Offline: ring the next order → capture the PREDICTED number/ref + the order id.
    let off = core_at("http://127.0.0.1:1".into(), db_path.clone());
    off.restore_session(blob.clone());
    let item = off.list_menu_items().unwrap().into_iter().next().expect("item");
    off.cart_add(item.id.clone(), item.name.clone(), item.base_price_minor).unwrap();
    let predicted = off.checkout(cash_checkout(&off)).await.expect("offline order");
    eprintln!("OFFLINE PREDICTED: #{:?} ref={:?}", predicted.order_number, predicted.order_ref);
    assert!(predicted.order_ref.is_some(), "offline receipt must carry a predicted ref");
    let order_id = predicted.local_order_id.clone();

    let _ = order_id;
    // Reconnect + sync, then read the SERVER's orders for the shift: the order it
    // minted must carry the EXACT number/ref the offline device predicted.
    let back = core_at(base.clone(), db_path.clone());
    back.restore_session(blob);
    back.refresh_connectivity().await;
    back.sync_now().await.ok();
    let server_orders = back.list_orders_for_shift(shift_id.clone()).await.expect("list shift orders");
    let minted = server_orders.iter().find(|o| o.order_ref == predicted.order_ref);
    eprintln!(
        "SERVER REFS: {:?}",
        server_orders.iter().map(|o| (o.order_number, o.order_ref.clone())).collect::<Vec<_>>()
    );
    let minted = minted.unwrap_or_else(|| panic!(
        "server did not mint the predicted ref {:?} (number {:?})",
        predicted.order_ref, predicted.order_number
    ));
    assert_eq!(minted.order_number.map(|n| n as i64), predicted.order_number, "order_number must match the server's");

    back.close_shift(0, None).await.ok();
    back.sync_now().await.ok();
    fx.cleanup().await;
    let _ = std::fs::remove_file(&db);
}

/// The ownership guard's REJECTION half: a teller may NOT sign in over another
/// teller's live shift when their device has no queued close for it (a takeover,
/// not a handover). Teller 1 opens a shift and does NOT close it; a fresh device
/// signing in as teller 2 (no acknowledgment) is rejected by the server.
#[tokio::test]
#[ignore]
async fn login_rejected_taking_over_another_tellers_open_shift() {
    let fx = provision_fixture().await;
    let base = fx.base.clone();
    let branch = fx.branch.clone();
    let login_req = |name: &str| LoginRequest {
        mode: LoginMode::Pin,
        name: Some(name.into()),
        pin: Some("1234".into()),
        branch_id: Some(branch.clone()),
        email: None, password: None, org_id: None,
    };

    let db = std::env::temp_dir().join(format!("madar_it_takeover_{}.sqlite", std::process::id()));
    let live = core_at(base.clone(), db.to_string_lossy().into());
    live.login(login_req(&fx.t1)).await.expect("t1 login");
    live.refresh_connectivity().await;
    let _ = live.refresh_catalog().await;
    if let Ok(Some(s)) = live.refresh_shift().await {
        if s.is_open { live.close_shift(0, None).await.ok(); live.sync_now().await.ok(); }
    }
    live.open_shift(10_000, Some("t1 open, not closed".into())).await.expect("t1 opens");
    live.sync_now().await.expect("syncs");

    // Teller 2 on a FRESH device (no queued close → no acknowledgment) must be
    // rejected: they can't take over teller 1's live shift.
    let t2db = std::env::temp_dir().join(format!("madar_it_takeover_t2_{}.sqlite", std::process::id()));
    let t2core = core_at(base.clone(), t2db.to_string_lossy().into());
    let r = t2core.login(login_req(&fx.t2)).await;
    eprintln!("TAKEOVER login result = {r:?}");
    assert!(r.is_err(), "teller 2 must NOT sign in over teller 1's open shift (no handover ack)");

    // Cleanup.
    live.close_shift(0, None).await.ok();
    live.sync_now().await.ok();
    fx.cleanup().await;
    let _ = std::fs::remove_file(&db);
    let _ = std::fs::remove_file(&t2db);
}

/// A cash movement is OFFLINE-FIRST: recording it queues + drains through the
/// outbox (idempotent on client_ref) and shows up in the cash list — proving the
/// drawer op no longer requires a connection.
#[tokio::test]
#[ignore]
async fn cash_movement_is_offline_first_and_idempotent() {
    let fx = provision_fixture().await;
    let core = signed_in_core(&fx).await;
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
    fx.cleanup().await;
}

/// BUG: offline, the history lists collapsed to ONLY the locally-queued rows — an
/// order or cash movement that synced BEFORE the outage vanished from the screen.
/// The server lists are now cached write-through, so offline they show the last-
/// synced snapshot (merged with anything queued during the outage).
#[tokio::test]
#[ignore]
async fn offline_lists_show_last_synced_server_rows() {
    let fx = provision_fixture().await;
    let base = fx.base.clone();
    let branch = fx.branch.clone();
    let teller = fx.t1.clone();
    let login_req = LoginRequest {
        mode: LoginMode::Pin, name: Some(teller), pin: Some("1234".into()),
        branch_id: Some(branch), email: None, password: None, org_id: None,
    };
    let db = std::env::temp_dir().join(format!("madar_it_offlists_{}.sqlite", std::process::id()));
    let db_path = db.to_string_lossy().to_string();
    let _ = std::fs::remove_file(&db);

    // ONLINE: fresh shift, ring an order + record a cash movement (both sync).
    let captured = Arc::new(Mutex::new(None));
    let live = core_at(base.clone(), db_path.clone());
    live.set_token_store(Box::new(CaptureStore(captured.clone())));
    live.login(login_req.clone()).await.expect("login");
    live.refresh_connectivity().await;
    live.refresh_catalog().await.expect("catalog");
    let _ = fresh_shift(&live, "offline-list fixture").await;

    let item = live.list_menu_items().unwrap().into_iter().next().expect("item");
    live.cart_add(item.id.clone(), item.name.clone(), item.base_price_minor).unwrap();
    let receipt = live.checkout(cash_checkout(&live)).await.expect("order");
    assert!(!receipt.queued_offline, "the order should sync online");
    live.record_cash_movement(2_500, "online float".into()).await.expect("cash");
    live.sync_now().await.ok();

    // Read the lists ONLINE — this CACHES the server rows write-through.
    let on_orders = live.list_shift_orders().await.expect("orders online");
    let on_cash = live.list_cash_movements().await.expect("cash online");
    let on_shifts = live.list_shifts().await.expect("shifts online");
    assert!(on_orders.iter().any(|o| o.order_ref == receipt.order_ref), "synced order present online");
    assert!(on_cash.iter().any(|m| m.note == "online float"), "synced cash present online");
    assert!(!on_shifts.is_empty(), "past shifts present online");
    let blob = captured.lock().unwrap_or_else(|e| e.into_inner()).clone().expect("blob");

    // OFFLINE (dead url, same store): the lists must STILL show the synced rows
    // (from the write-through cache), not collapse to an empty/queued-only view.
    let off = core_at("http://127.0.0.1:1".into(), db_path.clone());
    off.restore_session(blob);
    let off_orders = off.list_shift_orders().await.expect("orders offline");
    let off_cash = off.list_cash_movements().await.expect("cash offline");
    let off_shifts = off.list_shifts().await.expect("shifts offline");
    eprintln!("OFFLINE orders={} cash={} shifts={}", off_orders.len(), off_cash.len(), off_shifts.len());
    assert!(
        off_orders.iter().any(|o| o.order_ref == receipt.order_ref),
        "an order synced before the outage must still show offline (from cache), not vanish",
    );
    assert!(
        off_cash.iter().any(|m| m.note == "online float"),
        "a cash movement synced before the outage must still show offline (from cache)",
    );
    assert!(!off_shifts.is_empty(), "past shifts must still show offline (from cache)");

    // Cleanup.
    live.close_shift(0, None).await.ok();
    live.sync_now().await.ok();
    fx.cleanup().await;
    let _ = std::fs::remove_file(&db);
}

/// BUG: in the past-shifts list OFFLINE, a shift CLOSED OFFLINE still showed as
/// active — the list is projected from the cached SERVER snapshot, which keeps the
/// shift open until the close syncs. The queued-close overlay must mark it closed.
#[tokio::test]
#[ignore]
async fn offline_closed_shift_shows_closed_not_active_in_past_shifts() {
    let fx = provision_fixture().await;
    let base = fx.base.clone();
    let branch = fx.branch.clone();
    let teller = fx.t1.clone();
    let login_req = LoginRequest {
        mode: LoginMode::Pin, name: Some(teller), pin: Some("1234".into()),
        branch_id: Some(branch), email: None, password: None, org_id: None,
    };
    let db = std::env::temp_dir().join(format!("madar_it_offclosed_{}.sqlite", std::process::id()));
    let db_path = db.to_string_lossy().to_string();
    let _ = std::fs::remove_file(&db);

    // ONLINE: open a fresh shift, then view past shifts (caches the list with this
    // shift OPEN).
    let captured = Arc::new(Mutex::new(None));
    let live = core_at(base.clone(), db_path.clone());
    live.set_token_store(Box::new(CaptureStore(captured.clone())));
    live.login(login_req.clone()).await.expect("login");
    live.refresh_connectivity().await;
    live.refresh_catalog().await.expect("catalog");
    let shift_id = fresh_shift(&live, "offline-close-display fixture").await;
    let online = live.list_shifts().await.expect("shifts online");
    assert!(
        online.iter().any(|s| s.id == shift_id && s.is_open),
        "the fresh shift is open + in the cached list",
    );
    let blob = captured.lock().unwrap_or_else(|e| e.into_inner()).clone().expect("blob");

    // OFFLINE: close the shift (queues the close), then view past shifts. The row
    // must read CLOSED — overlaid from the queued close — not still-active.
    let off = core_at("http://127.0.0.1:1".into(), db_path.clone());
    off.restore_session(blob.clone());
    off.close_shift(11_000, None).await.expect("close offline");
    let past = off.list_shifts().await.expect("shifts offline");
    let row = past.iter().find(|s| s.id == shift_id).expect("the shift is in the cached list");
    assert!(!row.is_open, "a shift closed offline must show CLOSED, not active, in past shifts");
    assert_eq!(row.status, "closed", "status overlaid to closed");
    assert_eq!(row.closing_declared_minor, Some(11_000), "shows the locally-declared closing cash");

    // Cleanup: reconnect, sync the close, tidy.
    let back = core_at(base.clone(), db_path.clone());
    back.restore_session(blob.clone());
    back.refresh_connectivity().await;
    back.sync_now().await.ok();
    fx.cleanup().await;
    let _ = std::fs::remove_file(&db);
}

/// THE OFFLINE WORKFLOW: a shift opened AND closed entirely offline — never seen by
/// the server — must still be a first-class citizen offline: it appears in past
/// shifts (closed), its orders show, and its Z-report reconstructs. Reconstructed
/// from the outbox, since the server snapshot can't contain it yet.
#[tokio::test]
#[ignore]
async fn offline_opened_and_closed_shift_is_complete_in_history() {
    let fx = provision_fixture().await;
    let base = fx.base.clone();
    let branch = fx.branch.clone();
    let teller = fx.t1.clone();
    let login_req = LoginRequest {
        mode: LoginMode::Pin, name: Some(teller), pin: Some("1234".into()),
        branch_id: Some(branch), email: None, password: None, org_id: None,
    };
    let db = std::env::temp_dir().join(format!("madar_it_offwhole_{}.sqlite", std::process::id()));
    let db_path = db.to_string_lossy().to_string();
    let _ = std::fs::remove_file(&db);

    // ONLINE: log in (caches the bundle + branch), leave the branch with NO open
    // shift, and prime the past-shifts cache (this shift won't be in it).
    let captured = Arc::new(Mutex::new(None));
    let live = core_at(base.clone(), db_path.clone());
    live.set_token_store(Box::new(CaptureStore(captured.clone())));
    live.login(login_req.clone()).await.expect("login");
    live.refresh_connectivity().await;
    live.refresh_catalog().await.expect("catalog");
    if let Ok(Some(s)) = live.refresh_shift().await {
        if s.is_open { live.close_shift(0, None).await.ok(); live.sync_now().await.ok(); }
    }
    live.list_shifts().await.ok(); // prime cache:shifts (without our shift)
    let blob = captured.lock().unwrap_or_else(|e| e.into_inner()).clone().expect("blob");

    // OFFLINE: open shift B, ring an order, close B — all with no connection.
    let off = core_at("http://127.0.0.1:1".into(), db_path.clone());
    off.restore_session(blob.clone());
    off.open_shift(10_000, Some("whole offline shift".into())).await.expect("open B offline");
    let b = off.current_shift().unwrap().expect("B local").id;
    let item = off.list_menu_items().unwrap().into_iter().next().expect("item");
    off.cart_add(item.id.clone(), item.name.clone(), item.base_price_minor).unwrap();
    off.checkout(cash_checkout(&off)).await.expect("order offline");
    off.close_shift(9_500, None).await.expect("close B offline");

    // Past shifts OFFLINE: B must appear, CLOSED, with its cash.
    let past = off.list_shifts().await.expect("shifts offline");
    let row = past
        .iter()
        .find(|s| s.id == b)
        .expect("a shift opened+closed offline must appear in past shifts");
    assert!(!row.is_open, "it shows CLOSED");
    assert_eq!(row.opening_cash_minor, 10_000, "carries the opening cash");
    assert_eq!(row.closing_declared_minor, Some(9_500), "carries the declared closing cash");

    // B's orders OFFLINE: the queued sale shows in its history.
    let orders = off.list_orders_for_shift(b.clone()).await.expect("orders offline");
    assert_eq!(orders.len(), 1, "the offline shift's order shows in its history");

    // B's Z-report OFFLINE: reconstructs from local (no server report exists).
    let report = off.shift_report_for(b.clone()).await.expect("offline Z-report reconstructs");
    assert!(!report.from_server, "reconstructed locally, not from the server");

    // Cleanup: reconnect, drain (creates + closes B server-side), tidy.
    let recon = core_at(base.clone(), db_path.clone());
    recon.restore_session(blob.clone());
    recon.refresh_connectivity().await;
    recon.sync_now().await.ok();
    let _ = recon.refresh_shift().await;
    recon.sync_now().await.ok();
    fx.cleanup().await;
    let _ = std::fs::remove_file(&db);
}

/// Ring an order ONLINE and return its receipt, asserting it actually synced.
async fn place_order_online(core: &MadarCore) -> ReceiptView {
    let r = place_order(core).await;
    core.sync_now().await.ok();
    r
}

/// Open a guaranteed-FRESH shift (close any leftover first) so numbering starts at
/// #1, and return its id. Login must already have cached branch_code + timezone.
async fn fresh_shift(core: &MadarCore, tag: &str) -> String {
    if let Ok(Some(s)) = core.refresh_shift().await {
        if s.is_open {
            core.close_shift(0, None).await.ok();
            core.sync_now().await.ok();
        }
    }
    core.open_shift(10_000, Some(tag.into())).await.expect("open fresh shift");
    core.sync_now().await.expect("shift syncs");
    core.refresh_shift().await.ok().flatten().expect("fresh shift").id
}

/// ISSUE 2 (the online mismatch): ringing an order up ONLINE must display the SAME
/// `order_number` the server stores — and the sequence must INCREMENT, not stick at
/// #1. Before the fix the prediction was `queued.len()+1`, and online the queue
/// drains instantly → it always predicted #1 while the server stored 1,2,3…
#[tokio::test]
#[ignore]
async fn online_order_numbers_match_the_server_and_increment() {
    let fx = provision_fixture().await;
    let core = signed_in_core(&fx).await;
    let shift_id = fresh_shift(&core, "number-match fixture").await;

    // Three online sales: the receipts must read #1, #2, #3.
    let mut receipts = Vec::new();
    for _ in 0..3 {
        let r = place_order_online(&core).await;
        assert!(!r.queued_offline, "an online sale syncs immediately, not queued");
        receipts.push(r);
    }
    let nums: Vec<_> = receipts.iter().map(|r| r.order_number).collect();
    assert_eq!(nums, vec![Some(1), Some(2), Some(3)], "online numbers must increment, not stick at #1");

    // Every receipt's number must equal the server's stored number for that ref.
    core.sync_now().await.ok();
    let server = core.list_orders_for_shift(shift_id.clone()).await.expect("server orders");
    for r in &receipts {
        let matched = server
            .iter()
            .find(|o| o.order_ref == r.order_ref)
            .unwrap_or_else(|| panic!("server is missing the ring-up ref {:?}", r.order_ref));
        assert_eq!(
            matched.order_number.map(|n| n as i64),
            r.order_number,
            "ring-up #{:?} must equal the server's #{:?}",
            r.order_number,
            matched.order_number
        );
    }

    core.close_shift(0, None).await.ok();
    core.sync_now().await.ok();
    fx.cleanup().await;
}

/// RECEIPT IDENTITY: the receipt the teller sees at RING-UP must be identical to a
/// REPRINT of the same order pulled back from the server — same order_number, same
/// order_ref. (Online here; the offline-mint equivalence is proven by
/// offline_receipt_number_and_ref_match_the_server.) Together they cover all four
/// cases the field needs to match: ring-up vs reprint × online vs offline.
#[tokio::test]
#[ignore]
async fn ringup_receipt_matches_the_server_reprint() {
    let fx = provision_fixture().await;
    let core = signed_in_core(&fx).await;
    let shift_id = fresh_shift(&core, "reprint fixture").await;

    let ringup = place_order_online(&core).await;
    core.sync_now().await.ok();
    assert!(ringup.order_number.is_some() && ringup.order_ref.is_some(), "ring-up carries both");

    // Pull the synced order back by its ref and reprint it from the server.
    let server = core.list_orders_for_shift(shift_id.clone()).await.expect("orders");
    let row = server.iter().find(|o| o.order_ref == ringup.order_ref).expect("order on server");
    let reprint = core.order_receipt_view(row.id.clone()).await.expect("reprint");

    assert_eq!(reprint.order_number, ringup.order_number, "reprint #N == ring-up #N");
    assert_eq!(reprint.order_ref, ringup.order_ref, "reprint ref == ring-up ref");

    core.close_shift(0, None).await.ok();
    core.sync_now().await.ok();
    fx.cleanup().await;
}

/// ISSUE 2 (the resume case — "logging in mid shift"): a FRESH login on a shift that
/// already has orders must SEED the synced base from the server, so the very next
/// ring-up predicts MAX(order_number)+1 — not #1 (which would mismatch the receipt
/// and collide on the server's UNIQUE(shift_id, order_number)).
#[tokio::test]
#[ignore]
async fn resuming_a_shift_mid_day_predicts_max_plus_one() {
    let fx = provision_fixture().await;
    let base = fx.base.clone();
    let branch = fx.branch.clone();
    let teller = fx.t1.clone();
    let login_req = LoginRequest {
        mode: LoginMode::Pin, name: Some(teller), pin: Some("1234".into()),
        branch_id: Some(branch), email: None, password: None, org_id: None,
    };
    let db = std::env::temp_dir().join(format!("madar_it_resume_{}.sqlite", std::process::id()));
    let db_path = db.to_string_lossy().to_string();
    let _ = std::fs::remove_file(&db);

    // Device A: open a fresh shift, ring two online (server now holds #1, #2).
    let a = core_at(base.clone(), db_path.clone());
    a.login(login_req.clone()).await.expect("A login");
    a.refresh_connectivity().await;
    a.refresh_catalog().await.expect("catalog");
    let shift_id = fresh_shift(&a, "resume fixture").await;
    let r1 = place_order_online(&a).await;
    let r2 = place_order_online(&a).await;
    assert_eq!((r1.order_number, r2.order_number), (Some(1), Some(2)), "first two online");

    // Device B: a FRESH core+store logs in mid-shift. Login seeds the base from the
    // server (MAX=2), so B's first ring-up is #3 — not #1.
    let bdb = std::env::temp_dir().join(format!("madar_it_resume_b_{}.sqlite", std::process::id()));
    let b = core_at(base.clone(), bdb.to_string_lossy().into());
    b.login(login_req.clone()).await.expect("B login");
    b.refresh_connectivity().await;
    b.refresh_catalog().await.expect("catalog");
    let adopted = b.refresh_shift().await.ok().flatten().expect("B adopts the open shift");
    assert_eq!(adopted.id, shift_id, "B resumes the same shift");

    let r3 = place_order_online(&b).await;
    assert_eq!(r3.order_number, Some(3), "resumed shift continues at MAX+1 (#3), not #1");

    // Confirm against the server.
    b.sync_now().await.ok();
    let server = b.list_orders_for_shift(shift_id.clone()).await.expect("server orders");
    let matched = server.iter().find(|o| o.order_ref == r3.order_ref).expect("ref on server");
    assert_eq!(matched.order_number.map(|n| n as i64), Some(3), "server stored #3 too");

    b.close_shift(0, None).await.ok();
    b.sync_now().await.ok();
    fx.cleanup().await;
    let _ = std::fs::remove_file(&db);
    let _ = std::fs::remove_file(&bdb);
}

/// ISSUE 2 (mixed mode): numbers must stay CONTIGUOUS across an online→offline→online
/// transition. The base advances on each online ack, freezes while offline (the queue
/// grows), and the offline receipts must equal what the server mints on replay.
#[tokio::test]
#[ignore]
async fn online_then_offline_order_numbers_stay_contiguous() {
    let fx = provision_fixture().await;
    let base = fx.base.clone();
    let branch = fx.branch.clone();
    let teller = fx.t1.clone();
    let login_req = LoginRequest {
        mode: LoginMode::Pin, name: Some(teller), pin: Some("1234".into()),
        branch_id: Some(branch), email: None, password: None, org_id: None,
    };
    let db = std::env::temp_dir().join(format!("madar_it_mixednum_{}.sqlite", std::process::id()));
    let db_path = db.to_string_lossy().to_string();
    let _ = std::fs::remove_file(&db);

    let captured = Arc::new(Mutex::new(None));
    let live = core_at(base.clone(), db_path.clone());
    live.set_token_store(Box::new(CaptureStore(captured.clone())));
    live.login(login_req.clone()).await.expect("login");
    live.refresh_connectivity().await;
    live.refresh_catalog().await.expect("catalog");
    let shift_id = fresh_shift(&live, "mixed-number fixture").await;
    let blob = captured.lock().unwrap_or_else(|e| e.into_inner()).clone().expect("blob");

    // Two ONLINE (#1, #2).
    let o1 = place_order_online(&live).await;
    let o2 = place_order_online(&live).await;
    assert_eq!((o1.order_number, o2.order_number), (Some(1), Some(2)));

    // Two OFFLINE on the same shift (#3, #4 predicted while the queue grows).
    let off = core_at("http://127.0.0.1:1".into(), db_path.clone());
    off.restore_session(blob.clone());
    let o3 = place_order(&off).await;
    let o4 = place_order(&off).await;
    assert_eq!((o3.order_number, o4.order_number), (Some(3), Some(4)), "offline continues the sequence");
    assert!(o3.queued_offline && o4.queued_offline, "offline sales are queued");

    // Reconnect → the offline sales replay; the server's numbers match the receipts.
    let back = core_at(base.clone(), db_path.clone());
    back.restore_session(blob.clone());
    back.refresh_connectivity().await;
    back.sync_now().await.ok();
    assert_eq!(back.sync_status().unwrap().failed, 0, "no dead-letters");

    let server = back.list_orders_for_shift(shift_id.clone()).await.expect("server orders");
    let mut server_nums: Vec<i64> = server.iter().filter_map(|o| o.order_number.map(|n| n as i64)).collect();
    server_nums.sort_unstable();
    assert_eq!(server_nums, vec![1, 2, 3, 4], "server holds a contiguous 1..4 with no gaps or dups");
    for r in [&o3, &o4] {
        let m = server.iter().find(|o| o.order_ref == r.order_ref).expect("offline ref on server");
        assert_eq!(m.order_number.map(|n| n as i64), r.order_number, "offline receipt == server");
    }

    back.close_shift(0, None).await.ok();
    back.sync_now().await.ok();
    fx.cleanup().await;
    let _ = std::fs::remove_file(&db);
}

/// ISSUE 1 (end-to-end): the offline handover the field hit — a shift A with work
/// on it, CLOSED offline, then a NEW shift B opened offline and SOLD on. On reconnect
/// everything must drain IN ORDER (A's orders → A's close → B's open → B's orders),
/// B must become the active shift (routing to Orders, not the open-shift screen), and
/// NOTHING may dead-letter. The sequential-handover gate (open B depends on A's close)
/// is what stops B's open from racing the still-open branch and 409-ing.
#[tokio::test]
#[ignore]
async fn offline_close_a_then_open_b_with_orders_all_sync() {
    let fx = provision_fixture().await;
    let base = fx.base.clone();
    let branch = fx.branch.clone();
    let teller = fx.t1.clone();
    let login_req = LoginRequest {
        mode: LoginMode::Pin, name: Some(teller), pin: Some("1234".into()),
        branch_id: Some(branch), email: None, password: None, org_id: None,
    };
    let db = std::env::temp_dir().join(format!("madar_it_handover_{}.sqlite", std::process::id()));
    let db_path = db.to_string_lossy().to_string();
    let _ = std::fs::remove_file(&db);

    // Online: open shift A on a clean branch. Capture the session + A's id.
    let captured = Arc::new(Mutex::new(None));
    let live = core_at(base.clone(), db_path.clone());
    live.set_token_store(Box::new(CaptureStore(captured.clone())));
    live.login(login_req.clone()).await.expect("login");
    live.refresh_connectivity().await;
    live.refresh_catalog().await.expect("catalog");
    let a_id = fresh_shift(&live, "handover A").await;
    let blob = captured.lock().unwrap_or_else(|e| e.into_inner()).clone().expect("blob");

    // A was opened ONLINE (server-stamped opened_at); the close below is OFFLINE
    // (device-stamped closed_at). On a brand-new fixture both happen in the same
    // wall-clock second, and the backend rejects closed_at < opened_at. A >1s gap
    // guarantees the device close is strictly after the server open (mixing the two
    // clocks is unique to this test — the all-offline lifecycle is monotonic).
    tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;

    // OFFLINE: ring an order on A, close A, open B, ring TWO orders on B.
    let off = core_at("http://127.0.0.1:1".into(), db_path.clone());
    off.restore_session(blob.clone());
    let item = off.list_menu_items().unwrap().into_iter().next().expect("item");
    off.cart_add(item.id.clone(), item.name.clone(), item.base_price_minor).unwrap();
    off.checkout(cash_checkout(&off)).await.expect("A order offline");
    off.close_shift(11_000, None).await.expect("close A offline");
    off.open_shift(11_000, Some("handover B".into())).await.expect("open B offline");
    let b = off.current_shift().unwrap().expect("B local");
    assert_ne!(b.id, a_id, "B is a brand-new shift");
    for _ in 0..2 {
        off.cart_add(item.id.clone(), item.name.clone(), item.base_price_minor).unwrap();
        off.checkout(cash_checkout(&off)).await.expect("B order offline");
    }
    assert!(off.sync_status().unwrap().pending >= 5, "A-order + close A + open B + 2 B-orders queued");

    // RECONNECT: drain + reconcile. Nothing may dead-letter; B is the active shift.
    let back = core_at(base.clone(), db_path.clone());
    back.restore_session(blob.clone());
    back.refresh_connectivity().await;
    back.sync_now().await.ok();
    let _ = back.refresh_shift().await;
    back.sync_now().await.ok();

    let q = back.sync_status().expect("status");
    eprintln!("AFTER HANDOVER: {q:?}");
    assert_eq!(q.failed, 0, "no dead-letters — open B never raced A's close ({q:?})");
    assert_eq!(q.pending, 0, "the whole handover drained ({q:?})");
    let cur = back.refresh_shift().await.expect("refresh").expect("active shift");
    assert!(
        cur.is_open && cur.id == b.id,
        "routes into B (Orders), not the open-shift screen ({} vs {})",
        cur.id, b.id
    );
    let b_orders = back.list_orders_for_shift(b.id.clone()).await.expect("B orders");
    assert_eq!(b_orders.len(), 2, "both of B's offline sales recovered onto the server");

    back.close_shift(0, None).await.ok();
    back.sync_now().await.ok();
    fx.cleanup().await;
    let _ = std::fs::remove_file(&db);
}
