//! Sufrix POS shared core.
//!
//! THE ONE RULE: all real logic lives here. The Swift (iPhone/iPad) and Kotlin
//! (Android + desktop) apps are UI and platform glue only — they call into this
//! library over UniFFI for data, business rules, API calls, offline/sync and
//! printing. If a piece of logic could ever differ between platforms, that's a
//! bug; it belongs in Rust.
//!
//! Build-out is phased (see ../../../PLAN.md):
//!   Phase 1 (here): core skeleton + UniFFI bindings proven on every platform.
//!   Phase 2: API client (crates/madar-api) + auth + online read/write.
//!   Phase 3: SQLite local store + read-through cache + durable outbox.
//!   Phase 4: sync engine + backend offline-first support.
//!   Phase 5: printing (ESC/POS) in Rust.

uniffi::setup_scaffolding!();

mod config;
pub use config::MadarConfig;

/// The client-authoritative pricing engine (pure; the money source of truth).
pub mod pricing;

/// Cart — client-only in-progress order state, priced via `pricing`.
pub mod cart;
/// Checkout — assemble an order from the cart + place it via the outbox.
pub mod checkout;
/// The coarse FFI error model the host reacts to (PLAN §7.6).
pub mod error;
/// Static UI-string localization — one source of truth for both hosts.
pub mod i18n;
/// Menu / catalog reads — branch-effective mirror + view DTOs (PLAN §R9).
pub mod menu;
/// Order history reads — synced + still-queued orders for the shift.
pub mod orders;
/// Thermal-receipt rendering (ESC/POS) + best-effort network printing.
pub mod receipt;
/// Receipt → 1-bit raster bitmap (logo + Arabic via the embedded Cairo font),
/// for the raster-only TSP143III. Mirrors the on-screen ReceiptPaper preview.
pub mod render;
/// Local recipe preview — effective ingredients for a configured item (parity
/// with Flutter's `computeRecipeLocally`).
pub mod recipe;
/// Category styling (icon + gradient palette) — port of Flutter's `CatStyle`.
pub mod catstyle;
/// Delivery-order management (teller side) — list/advance/cancel/finalize.
pub mod delivery;
/// HTTP layer — drives the generated `madar-api` reqwest client (PLAN §R4 net/).
pub mod net;
/// Client of the unified realtime bus — ONE SSE connection per device, hand-rolled
/// over `bytes_stream()`, dispatched to the host through one callback listener.
pub mod realtime;
/// Waiter open tickets — fire-now-pay-later dine-in tickets via the outbox.
pub mod tickets;
/// Kitchen Display System — station feed + per-line bump (kitchen topic consumer).
pub mod kds;
/// Device binding (branch / till / station / printer / reconfigure) — persisted in
/// the CORE store so the hosts hold no device state (THE ONE RULE).
pub mod device;
/// LAN offline relay (Phase E) — signed message envelope, per-branch HMAC, peer
/// registry; the second delivery path beside the cloud bus. Outbox stays the truth.
pub mod lan;
/// Session & auth — online login, offline unlock, token custody (PLAN §7.2).
pub mod session;
/// Shift lifecycle — open/current via the outbox (PLAN §7.4).
pub mod shift;
/// Local store — SQLite mirror + durable outbox + id_map + sync cursors (PLAN §8).
pub mod store;
/// Branch-timezone-aware timestamp formatting for display (mirrors Flutter AppTz).
pub mod timefmt;

use std::sync::{Arc, Mutex, RwLock};

use error::CoreError;

/// Crate version (semver of the core library).
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// The FFI *surface* contract version, independent of the crate version. Bump
/// on every breaking change to the exported API so all three apps can assert at
/// startup that they were built against a compatible core (see PLAN.md §"FFI
/// surface versioning").
#[uniffi::export]
pub fn ffi_surface_version() -> u32 {
    // 1: realtime SSE (`subscribe_realtime`/`unsubscribe_realtime` + `EventListener`)
    //    + `AppRoute` payload variants (KitchenDisplay/WaiterTickets).
    // 2: device config moved into the core store — `app_route()`/`open_shift`/
    //    `refresh_shift` drop their host-passed params; `DeviceMode` removed; new
    //    `device_config`/`set_device_*` surface + `kitchen` role drives the KDS route.
    // 3: LAN offline relay (Phase E) — `lan_start`/`lan_stop`/`lan_active`/
    //    `lan_peer_count`/`lan_branch_has_open_till`/`set_device_lan_hub` + the
    //    `DeviceConfigView.lan_hub` field.
    // 4: core-driven realtime — `start_realtime(listener, player)` + the
    //    `RealtimePlayer` callback (the core owns topics-per-role + the alert
    //    decision/dedup/localized text; the host just plays ping/notification/haptic).
    4
}

/// Smoke-test call used to prove the binding pipeline end-to-end from each host.
#[uniffi::export]
pub fn greet(name: String) -> String {
    format!("Sufrix core v{} says hello, {name}", core_version())
}

/// The screen the host should show, decided by the core (PLAN §R11). The host
/// consults this only at deliberate transitions (cold start, post-login,
/// post-open/close-shift, sign-out) — never as a side effect of connectivity.
#[derive(uniffi::Enum, Clone, Debug, PartialEq, Eq)]
pub enum AppRoute {
    /// Till not bound to a branch → manager device-setup.
    DeviceSetup,
    /// Configured but signed out → teller/waiter PIN login.
    Login,
    /// Signed in, no open shift → open-shift screen.
    OpenShift,
    /// Signed in with an open shift → order screen.
    Order,
    /// Device run as a kitchen display → the KDS for `station_id` (no shift needed).
    KitchenDisplay { station_id: String },
    /// A signed-in WAITER (holds no shift) → the open-tickets / take-order screen.
    WaiterTickets,
}

/// A till (physical drawer) the device can bind to — the device-setup / Settings
/// till picker. Cash continuity + the one-open-shift rule key on the till.
#[derive(uniffi::Record, Clone, Debug)]
pub struct TillView {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub is_active: bool,
}

/// Top-level handle the host creates once and keeps alive for the app lifetime.
///
/// Phase 1 exposes config + version only. Later phases hang the API client,
/// local store, sync engine and printer off this object — the host keeps
/// holding the same handle.
#[derive(uniffi::Object)]
pub struct MadarCore {
    config: MadarConfig,
    /// The ONE embedded store (single writer behind its internal Mutex). `Arc` so the
    /// LAN relay bridge shares this EXACT instance (never a second connection — that
    /// would break the single-writer invariant and contend on WAL). Phase E.
    store: Arc<store::Store>,
    /// Active UI locale (en/ar) — runtime-changeable via `set_locale`; seeds from
    /// `config.locale`. Drives `tr`/`is_rtl` + catalog `*_translations` resolution.
    /// Active UI locale — `Arc` so the realtime `AlertingListener` can read it to
    /// localize notification titles (the host plays them, the core writes them).
    locale: Arc<RwLock<String>>,
    /// HTTP client to the backend (holds the live bearer token).
    api: net::ApiClient,
    /// The live session (`None` = signed out). Set by login / offline unlock /
    /// cold-start restore; cleared on logout.
    session: RwLock<Option<session::SessionState>>,
    /// The host's secure-bytes vault for the session blob (Keychain/Keystore).
    token_store: Mutex<Option<Box<dyn session::TokenStore>>>,
    /// Server-vs-device clock skew in SECONDS — SHARED with the `ApiClient`, which
    /// refreshes it from the `Date` header of EVERY response (not just the ping), so
    /// `corrected_now` stays server-aligned between heartbeats. Persisted to kv on
    /// ping for a corrected cold-offline boot. Also drives the clock-skew banner.
    clock_skew_secs: Arc<std::sync::atomic::AtomicI64>,
    /// `true` after a drain hit a 401: the outbox is parked (no retry budget
    /// burned, no heartbeat hammering) until the next successful login clears it.
    auth_paused: std::sync::atomic::AtomicBool,
    /// A small in-memory ring buffer of diagnostic warnings (sync dead-letters,
    /// cascade failures, auth parks) — surfaced in Settings → Diagnostics so a
    /// teller/manager can see WHY something is stuck without a debugger.
    diag: Mutex<std::collections::VecDeque<DiagEntry>>,
    /// Single-flight guard for `drain_outbox`. The drain is triggered from many
    /// async entry points (login, checkout, open/close shift, cash movement,
    /// void, sync_now, retry, the connectivity heartbeat); without serialization
    /// two overlapping drains each snapshot the same `due_for_sync` backlog and
    /// double-send every row (and `recover_inflight` could reset a sibling's
    /// in-flight op). Held across the whole drain body so only one runs at a time
    /// — the second caller waits, then runs a fresh pass that sees any newly
    /// enqueued op. Mirrors the Flutter queue's `_drainFuture` single-flight.
    drain_lock: tokio::sync::Mutex<()>,
    /// The single live realtime subscription (the device's ONE SSE connection).
    /// Replacing or clearing it aborts the previous supervisor task, so a device
    /// never holds two streams. `None` = not subscribed.
    realtime: Mutex<Option<realtime::StreamHandle>>,
    /// The ONE unified event listener (set by `subscribe_realtime`), shared by BOTH
    /// the cloud SSE supervisor AND the LAN relay bridge so a cross-LAN event and its
    /// cloud twin reach the same sink (deduped by the host's snapshot-reload). Phase E.
    unified_listener: Arc<Mutex<Option<Arc<dyn realtime::EventListener>>>>,
    /// The running LAN relay (`None` = not started). The second delivery path beside
    /// the cloud bus; outbox stays the source of truth. Phase E.
    lan: Mutex<Option<Arc<lan::LanRelay>>>,
}

/// One diagnostic log line.
#[derive(uniffi::Record, Clone, Debug)]
pub struct DiagLogView {
    pub at: String,
    pub level: String,
    pub message: String,
}

#[derive(Clone, Debug)]
struct DiagEntry {
    at: String,
    level: String,
    message: String,
}

#[uniffi::export]
impl MadarCore {
    /// Construct with explicit config (the host fills `db_path` with an
    /// app-private file). Opens + migrates the local store and builds the HTTP
    /// client; the session starts empty (host calls `restore_session` at boot).
    #[uniffi::constructor]
    pub fn new(config: MadarConfig) -> Result<Arc<Self>, error::CoreError> {
        let store = Arc::new(store::Store::open(&config.db_path)?);
        // Restore the last-known server skew so even a cold OFFLINE boot (no ping
        // yet) stamps queued ops with corrected, non-future times. SHARED with the
        // ApiClient so every response's Date header keeps it fresh.
        let skew = store
            .kv_get("clock_skew_secs")
            .ok()
            .flatten()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let clock_skew_secs = Arc::new(std::sync::atomic::AtomicI64::new(skew));
        let api = net::ApiClient::new(config.base_url.clone(), clock_skew_secs.clone())?;
        let locale = Arc::new(RwLock::new(config.locale.clone()));
        Ok(Arc::new(Self {
            config,
            store,
            locale,
            api,
            session: RwLock::new(None),
            token_store: Mutex::new(None),
            clock_skew_secs,
            auth_paused: std::sync::atomic::AtomicBool::new(false),
            diag: Mutex::new(std::collections::VecDeque::new()),
            drain_lock: tokio::sync::Mutex::new(()),
            realtime: Mutex::new(None),
            unified_listener: Arc::new(Mutex::new(None)),
            lan: Mutex::new(None),
        }))
    }

    /// Construct from the baked-in `.env` defaults (in-memory store until the
    /// host supplies a `db_path`).
    #[uniffi::constructor]
    pub fn from_env() -> Result<Arc<Self>, error::CoreError> {
        Self::new(MadarConfig::from_env())
    }

    /// API base URL the core will talk to (from `.env`).
    pub fn base_url(&self) -> String {
        self.config.base_url.clone()
    }

    /// Environment name (`prod` | `staging` | `dev`).
    pub fn environment(&self) -> String {
        self.config.environment.clone()
    }

    /// SQLite path the host handed us (empty => in-memory).
    pub fn db_path(&self) -> String {
        self.config.db_path.clone()
    }

    /// Core crate version.
    pub fn version(&self) -> String {
        core_version()
    }

    /// Outbox items still waiting to sync (pending + in-flight) — the host shows
    /// this in the sync-status chrome.
    pub fn pending_outbox_count(&self) -> Result<u32, error::CoreError> {
        self.store.pending_count()
    }

    // ── session (sync) ──────────────────────────────────────────────────────

    /// Install the host's secure-bytes vault. Call once, right after `new`,
    /// before `restore_session`.
    pub fn set_token_store(&self, store: Box<dyn session::TokenStore>) {
        *self.token_store.lock().unwrap_or_else(|e| e.into_inner()) = Some(store);
    }

    /// Re-hydrate a session from the host's persisted blob at cold start. Returns
    /// the snapshot if the blob is valid, else `None` (fresh install / corrupt).
    pub fn restore_session(&self, blob: Vec<u8>) -> Option<session::SessionSnapshot> {
        let mut state = session::SessionState::from_blob(&blob)?;
        // A cold-restored session has NOT pinged yet — connectivity is only ever
        // truthful after a live heartbeat. Start offline-until-proven-online so we
        // never report a stale `online=true` (which would make the UI try a hard
        // online path before the first `refresh_connectivity`).
        state.snapshot.online = false;
        self.api.set_bearer(state.token.clone());
        let snapshot = state.snapshot.clone();
        *self.session.write().unwrap_or_else(|e| e.into_inner()) = Some(state);
        Some(snapshot)
    }

    pub fn is_authenticated(&self) -> bool {
        self.session.read().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    /// The cached session — never hits the network.
    pub fn current_session(&self) -> Option<session::SessionSnapshot> {
        self.session
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|s| s.snapshot.clone())
    }

    /// Permission check against the mirrored matrix. Optimistic while a session
    /// is offline-unlocked (permissions not yet loaded) — see `SessionState`.
    pub fn has_permission(&self, resource: String, action: String) -> bool {
        self.session
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|s| s.has_permission(&resource, &action))
            .unwrap_or(false)
    }

    /// Offline unlock: verify a typed PIN against the cached org bundle
    /// (argon2id). No network, no token; identity is the real server `user_id`.
    pub fn unlock_offline(
        &self,
        name: String,
        pin: String,
        branch_id: String,
    ) -> Result<session::SessionSnapshot, CoreError> {
        let state = session::unlock_from_bundle(&self.store, &name, &pin, &branch_id)?;
        let snapshot = state.snapshot.clone();
        self.api.set_bearer(None);
        self.persist_and_set(state);
        Ok(snapshot)
    }

    /// Sign out: clear the live session + token (the JWT is stateless — there is
    /// no server-side logout endpoint, so clearing locally is the revocation) and
    /// the host vault. Does NOT force-close the open shift, and KEEPS the cached
    /// shift: the open drawer is DEVICE state, not session state — it stays so the
    /// next sign-in can enforce that only its owner resumes it (and route them
    /// straight into it). The in-progress CART is session state, so it's dropped.
    /// Preserves the outbox unless `wipe_outbox`.
    pub fn logout(&self, wipe_outbox: bool) -> Result<(), CoreError> {
        self.api.set_bearer(None);
        *self.session.write().unwrap_or_else(|e| e.into_inner()) = None;
        if let Some(ts) = self.token_store.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            ts.clear_blob();
        }
        // NB: the cached shift is intentionally KEPT (device drawer state) — see
        // the ownership gate in `sign_in`.
        let _ = cart::clear(&self.store);
        if wipe_outbox {
            self.store.wipe_outbox()?;
        }
        Ok(())
    }
}

impl MadarCore {
    /// Fetch a synced order's full record, CACHING it write-through so its detail +
    /// reprint work OFFLINE. Online: fetch + `cache:order:{id}`. Offline / on error:
    /// the cached copy (populated here or by a prior list with items). Errors only
    /// for a synced order this device has never seen online. Non-exported (returns a
    /// raw `OrderFull`, not a uniffi type) — the public methods project it.
    async fn get_order_or_cache(&self, order_id: &str) -> Result<madar_api::models::OrderFull, CoreError> {
        use madar_api::apis::orders_api;
        let key = format!("cache:order:{order_id}");
        if self.current_session().map(|s| s.online).unwrap_or(false) {
            if let Ok(o) = orders_api::get_order(
                &self.api.config(),
                orders_api::GetOrderParams { order_id: order_id.to_string() },
            )
            .await
            {
                cache_views(&self.store, &key, std::slice::from_ref(&o));
                return Ok(o);
            }
        }
        cached_views::<madar_api::models::OrderFull>(&self.store, &key)
            .into_iter()
            .next()
            .ok_or_else(|| CoreError::Offline {
                detail: "order not cached yet — view it once online to enable offline reprint".into(),
            })
    }

    /// Persist a session to the host vault and install it as the live session.
    fn persist_and_set(&self, state: session::SessionState) {
        if let Some(ts) = self.token_store.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            ts.save_blob(state.to_blob());
        }
        *self.session.write().unwrap_or_else(|e| e.into_inner()) = Some(state);
    }

    /// The active runtime locale (defaults to `config.locale`).
    fn current_locale(&self) -> String {
        self.locale.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// `(org_id, branch_id)` from the live session — needed for branch-effective
    /// catalog fetches. Errors if signed out / no org.
    fn org_branch(&self) -> Result<(String, Option<String>), CoreError> {
        let g = self.session.read().unwrap_or_else(|e| e.into_inner());
        let s = g.as_ref().ok_or_else(|| CoreError::Unauthenticated {
            detail: "not signed in".into(),
        })?;
        let org = s.snapshot.org_id.clone().ok_or_else(|| CoreError::Validation {
            field: "org_id".into(),
            detail: "session has no org".into(),
        })?;
        Ok((org, s.snapshot.branch_id.clone()))
    }

    /// Whether a shift command of `op_type` for the CURRENTLY CACHED shift is
    /// still queued — scoped to that shift's id. The outbox is device-global and
    /// survives sign-out, so an unrelated teller's orphaned command must NOT
    /// count (it would keep a force-closed shift alive for the next teller on a
    /// shared till). open_shift ops are keyed by the shift PK; close_shift ops by
    /// `{shift_id}:close` (so open + close for one shift don't collide in the
    /// idempotent outbox).
    fn shift_command_pending(&self, op_type: &str) -> Result<bool, CoreError> {
        let sid = match shift::current(&self.store)? {
            Some(s) => s.id,
            None => return Ok(false),
        };
        let close_id = format!("{sid}:close");
        Ok(self
            .store
            .pending()?
            .iter()
            .any(|i| i.op_type == op_type && (i.id == sid || i.id == close_id)))
    }

    /// Whether the device currently has an OPEN shift — the deterministic,
    /// offline-safe answer that enforces the SEQUENTIAL-ONLY shift model (one
    /// shift at a time per device, even offline). True iff:
    ///   • the cached shift is open, OR
    ///   • an `open_shift` command is still queued for a shift that has NO
    ///     matching `close_shift` queued — a defense for the case where a bad
    ///     reconcile dropped the cache while the open hadn't synced yet.
    /// A shift CLOSED locally (its close already queued) is NOT open here, so the
    /// next shift may open immediately — that's the normal offline "close A, then
    /// open B" flow, and the FIFO drain still replays close-A before open-B.
    fn device_has_open_shift(&self) -> Result<bool, CoreError> {
        if shift::current(&self.store)?.map(|s| s.is_open).unwrap_or(false) {
            return Ok(true);
        }
        let pending = self.store.pending()?;
        let has_uncovered_open = pending.iter().any(|op| {
            op.op_type == "open_shift"
                && !pending
                    .iter()
                    .any(|c| c.op_type == "close_shift" && c.id == format!("{}:close", op.id))
        });
        Ok(has_uncovered_open)
    }

    /// Comma-separated shift ids the device has a queued `close_shift` for — sent
    /// as the login acknowledgment so the server's open-shift login guard permits
    /// the legitimate offline handover (this device closed that shift offline; the
    /// close will replay right after login) while still rejecting a takeover.
    fn closing_shift_ids_csv(&self) -> String {
        self.store
            .pending()
            .unwrap_or_default()
            .iter()
            .filter(|i| i.op_type == "close_shift")
            .filter_map(|i| i.shift_id.clone())
            .collect::<Vec<_>>()
            .join(",")
    }

    /// (enqueuing teller id, device→server clock skew in ms) stamped on every
    /// queued op — the drain scopes by teller and re-bases timestamps at sync.
    fn outbox_meta(&self) -> (Option<String>, Option<i64>) {
        let user_id = self
            .session
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|s| s.snapshot.user_id.clone());
        let skew_ms = self.clock_skew_secs.load(std::sync::atomic::Ordering::Relaxed) * 1000;
        (user_id, Some(skew_ms))
    }

    /// Append a diagnostic line (capped ring buffer of 200) — surfaced in
    /// Settings → Diagnostics. Best-effort; never fails the caller.
    fn push_diag(&self, level: &str, message: impl Into<String>) {
        let mut g = self.diag.lock().unwrap_or_else(|e| e.into_inner());
        g.push_back(DiagEntry { at: chrono::Utc::now().to_rfc3339(), level: level.into(), message: message.into() });
        while g.len() > 200 {
            g.pop_front();
        }
    }

    /// Drain the durable outbox — the single place outbox writes hit the network.
    /// Ports the Flutter offline-queue engine (offline_queue.dart) so a device
    /// can run months offline and replay safely:
    ///   • backoff-gated, FIFO, user-scoped `due_for_sync`;
    ///   • crash recovery (inflight → pending) + acked-row retention purge;
    ///   • dependency gating that WAITS on an unsynced/dead prerequisite (never
    ///     cascades the dependent dead — that would strand its sale; the dead ROOT
    ///     surfaces the jam, and resolving it flows the whole chain);
    ///   • close-shift-must-be-LAST (shift-scoped);
    ///   • exactly-once via in-body idempotency keys (a lost-response retry dedups);
    ///   • precise per-status handling — 401 parks the queue, network blips
    ///     reschedule without burning retry budget, genuine 4xx dead-letter,
    ///     idempotent 409/404 ack, 5xx exponential-backoff up to 8 tries.
    async fn drain_outbox(&self) -> Result<(), CoreError> {
        use std::sync::atomic::Ordering::Relaxed;
        // Single-flight: only one drain iterates the backlog at a time. A second
        // concurrent trigger (heartbeat + a fresh checkout, say) waits here, then
        // runs its own pass once we finish — so it picks up anything we hadn't
        // snapshotted, but never double-sends a row this pass already owns.
        let _drain_guard = self.drain_lock.lock().await;
        // Crash recovery + retention housekeeping (cheap, idempotent). Safe to run
        // under the guard: with drains serialized, the only `inflight` rows here
        // are genuinely crash-stranded, never a live sibling's in-flight op.
        let _ = self.store.recover_inflight();
        let _ = self.store.purge_acked_older_than(now_ms() - K_ACKED_RETENTION_MS);
        // A 401-parked queue burns nothing until the next successful login.
        if self.auth_paused.load(Relaxed) {
            return Ok(());
        }

        // Flush the ENTIRE device backlog regardless of which teller is signed in:
        // /sync/replay attributes each op to its own EMBEDDED teller, so any teller
        // (or a device principal) drains everyone's queued work. The old
        // teller-scoped drain stranded a prior teller's ops on a shared till — the
        // "must be the same teller to sync" bug.
        for item in self.store.due_for_sync(now_ms(), None)? {
            // A shift close must be the LAST op for its shift — wait while any of
            // that shift's orders/voids/cash are still live (shift-scoped).
            if item.op_type == "close_shift" {
                if let Some(sid) = item.shift_id.as_deref() {
                    if self.store.has_live_shift_writes(sid, item.seq)? {
                        continue;
                    }
                }
            }

            // Prerequisite gating: don't send until the dependency is acked.
            if let Some(dep) = item.depends_on_seq {
                match self.store.status_of_seq(dep)?.as_deref() {
                    // Still in flight OR dead-lettered → WAIT, don't cascade. Marking
                    // this op dead too would STRAND its sales (the field bug: an
                    // order's open dead-letters → the order cascades dead → the sale
                    // is lost). Waiting on a dead dependency keeps the whole chain
                    // RECOVERABLE: resolving the root op (the user retries it, or
                    // discards it — both surfaced in the stuck list) lets every
                    // dependent flow on the next drain. Waiting burns no retry budget,
                    // so this never loops; the dead ROOT is what surfaces the problem.
                    Some("pending") | Some("inflight") | Some("dead") => continue,
                    _ => {} // acked / discarded → safe to proceed
                }
            }

            self.store.mark_inflight(item.seq)?;
            match self.send_outbox_item(&item).await {
                // Applied server-side (or idempotently already-applied).
                SendOutcome::Acked(server_id) => {
                    self.store.mark_acked(item.seq, server_id.as_deref())?;
                }
                // Permanent rejection — surface in the stuck list, never silently drop.
                SendOutcome::Dead(err) => {
                    self.store.mark_dead(item.seq, &err)?;
                    self.push_diag("error", format!("{} rejected: {err}", item.op_type));
                    // A rejected open leaves the teller selling against a phantom
                    // shift — clear the optimistic local shift.
                    if item.op_type == "open_shift"
                        && shift::current(&self.store)?.map(|s| s.id) == Some(item.id.clone())
                    {
                        let _ = shift::clear(&self.store);
                    }
                }
                // Token expired → park the whole queue (no budget burned) until
                // the next successful login re-drains.
                SendOutcome::AuthExpired => {
                    self.store.mark_retry_no_count(item.seq, now_ms() + K_NETWORK_RETRY_MS)?;
                    self.auth_paused.store(true, Relaxed);
                    self.push_diag("warn", "sync paused — session expired; sign in again to resume");
                    return Ok(());
                }
                // Connectivity blip — reschedule WITHOUT consuming retry budget,
                // and stop this pass (the network is down for the rest too).
                SendOutcome::Offline => {
                    self.store.mark_retry_no_count(item.seq, now_ms() + K_NETWORK_RETRY_MS)?;
                    return Ok(());
                }
                // Server error (5xx) / undecodable 2xx → counted exponential
                // backoff; dead-letter after the retry budget is exhausted.
                SendOutcome::Retry(err) => {
                    let attempts = item.attempts + 1;
                    if attempts >= K_MAX_RETRIES {
                        self.store.mark_dead(item.seq, &err)?;
                    } else {
                        let backoff = compute_backoff_ms(attempts, item.seq);
                        self.store.mark_retry(item.seq, &err, now_ms() + backoff)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Dispatch one queued op to the network and classify the result into a
    /// `SendOutcome`. Timestamps are re-based to the fresh server offset first
    /// (correct-at-sync), so a sale rung on a wrong-by-a-constant clock records
    /// the right time. Idempotency keys live in the persisted payload, so a
    /// replay after a lost response dedups server-side.
    /// Queue a bump/unbump as a durable replay op and try to drain it now. Each tap
    /// is its OWN op (unique id) so a rapid bump→unbump→bump replays in FIFO order to
    /// the correct final state; `/sync/replay` dedups idempotently on the line.
    async fn enqueue_bump(&self, item_id: String, bumped: bool) -> Result<(), CoreError> {
        let cmd = kds::BumpCommand { item_id: item_id.clone() };
        let (user_id, clock_offset_ms) = self.outbox_meta();
        let op_id = uuid::Uuid::new_v4().to_string();
        self.store.enqueue(&store::NewOutboxOp {
            id: op_id,
            op_type: if bumped { "bump_kitchen" } else { "unbump_kitchen" }.into(),
            idempotency_key: format!("{}:{}", cmd.item_id, if bumped { "bump" } else { "unbump" }),
            payload: serde_json::to_string(&cmd)?,
            event_at: self.corrected_now().to_rfc3339(),
            depends_on_seq: None,
            user_id: user_id.clone(),
            clock_offset_ms,
            shift_id: None,
        })?;
        // Instant cross-device delivery over the LAN (carrying the replay op so a
        // peer can mirror it for durability, + the item_id so a peer greys the line
        // on its overlay). The outbox above stays the source of truth; this is just
        // the fast path. No-op when the relay isn't running.
        let op = if bumped { "bump_kitchen_item" } else { "unbump_kitchen_item" };
        let envelope =
            serde_json::json!({ "op": op, "teller_id": user_id, "item_id": item_id }).to_string();
        let data = serde_json::json!({ "item_id": item_id }).to_string();
        let ev = if bumped { "kitchen.item_bumped" } else { "kitchen.item_unbumped" };
        self.lan_publish("kitchen", ev, data, Some(envelope)).await;
        let _ = self.drain_outbox().await;
        Ok(())
    }

    /// The still-pending bump intents `(line_id, bumped)` in FIFO order — overlaid
    /// onto the KDS feed so the board reflects un-synced taps. Best-effort.
    fn pending_bumps(&self) -> Vec<(String, bool)> {
        self.store
            .pending()
            .unwrap_or_default()
            .iter()
            .filter_map(|i| {
                let bumped = match i.op_type.as_str() {
                    "bump_kitchen" => true,
                    "unbump_kitchen" => false,
                    _ => return None,
                };
                let cmd: kds::BumpCommand = serde_json::from_str(&i.payload).ok()?;
                Some((cmd.item_id, bumped))
            })
            .collect()
    }

    async fn send_outbox_item(&self, item: &store::OutboxItem) -> SendOutcome {
        let delta = self.rebase_delta_ms(item);

        // Every queued op flushes through ONE endpoint — `POST /sync/replay` —
        // carrying its ORIGINAL teller so the backend attributes it to the teller
        // who rang it, not to whoever is signed in now. This is what lets ANY
        // teller (or a device principal) drain the whole shared-till backlog.
        let teller_id = match item.user_id.clone() {
            Some(t) => t,
            // A legacy/un-attributed op can't be replayed safely — surface it.
            None => return SendOutcome::Dead("queued op has no teller attribution".into()),
        };

        // Deserialize the stored command, re-base its timestamp to the fresh
        // server skew (correct-at-sync), and wrap it in the replay envelope.
        // `idem` is how a 409/404 is read for this op (unchanged from the live
        // per-resource path). The envelope's `request` is the GENERATED type, so
        // the wire shape is identical to the live endpoint's body.
        let (envelope, idem): (serde_json::Value, Idem) = match item.op_type.as_str() {
            "open_shift" => {
                let mut cmd: shift::OpenShiftCommand = match serde_json::from_str(&item.payload) {
                    Ok(c) => c,
                    Err(e) => return SendOutcome::Dead(format!("payload: {e}")),
                };
                rebase_dopt(&mut cmd.request.opened_at, delta);
                (
                    serde_json::json!({ "op": "open_shift", "teller_id": teller_id, "branch_id": cmd.branch_id, "request": cmd.request }),
                    Idem::No,
                )
            }
            "close_shift" => {
                let mut cmd: shift::CloseShiftCommand = match serde_json::from_str(&item.payload) {
                    Ok(c) => c,
                    Err(e) => return SendOutcome::Dead(format!("payload: {e}")),
                };
                rebase_dopt(&mut cmd.request.closed_at, delta);
                (
                    serde_json::json!({ "op": "close_shift", "teller_id": teller_id, "shift_id": cmd.shift_id, "request": cmd.request }),
                    Idem::Yes,
                )
            }
            "create_order" => {
                let mut cmd: checkout::CheckoutCommand = match serde_json::from_str(&item.payload) {
                    Ok(c) => c,
                    Err(e) => return SendOutcome::Dead(format!("payload: {e}")),
                };
                rebase_dopt(&mut cmd.request.created_at, delta);
                (
                    serde_json::json!({ "op": "create_order", "teller_id": teller_id, "request": cmd.request }),
                    Idem::No,
                )
            }
            "void_order" => {
                let mut cmd: orders::VoidOrderCommand = match serde_json::from_str(&item.payload) {
                    Ok(c) => c,
                    Err(e) => return SendOutcome::Dead(format!("payload: {e}")),
                };
                rebase_dopt(&mut cmd.request.voided_at, delta);
                (
                    serde_json::json!({ "op": "void_order", "teller_id": teller_id, "order_id": cmd.order_id, "request": cmd.request }),
                    Idem::VoidIdem,
                )
            }
            "cash_movement" => {
                let mut cmd: shift::CashMovementCommand = match serde_json::from_str(&item.payload) {
                    Ok(c) => c,
                    Err(e) => return SendOutcome::Dead(format!("payload: {e}")),
                };
                rebase_dopt(&mut cmd.request.created_at, delta);
                (
                    serde_json::json!({ "op": "cash_movement", "teller_id": teller_id, "shift_id": cmd.shift_id, "request": cmd.request }),
                    Idem::Yes,
                )
            }
            // ── Waiter open tickets (fire-now-pay-later) ──────────────────────
            // The waiter fires/rounds; the cashier settles; either voids. Each is
            // idempotent on a client-minted key and lands through the same replay
            // envelope as orders. No timestamp to rebase (the request carries none;
            // the settle order is stamped server-side at replay).
            "open_ticket" => {
                let cmd: tickets::FireTicketCommand = match serde_json::from_str(&item.payload) {
                    Ok(c) => c,
                    Err(e) => return SendOutcome::Dead(format!("payload: {e}")),
                };
                (
                    serde_json::json!({ "op": "fire_open_ticket", "teller_id": teller_id, "request": cmd.request }),
                    Idem::Yes,
                )
            }
            "ticket_add_round" => {
                let cmd: tickets::AddRoundCommand = match serde_json::from_str(&item.payload) {
                    Ok(c) => c,
                    Err(e) => return SendOutcome::Dead(format!("payload: {e}")),
                };
                (
                    serde_json::json!({ "op": "add_ticket_round", "teller_id": teller_id, "ticket_id": cmd.ticket_id, "request": cmd.request }),
                    Idem::Yes,
                )
            }
            "settle_open_ticket" => {
                let cmd: tickets::SettleTicketCommand = match serde_json::from_str(&item.payload) {
                    Ok(c) => c,
                    Err(e) => return SendOutcome::Dead(format!("payload: {e}")),
                };
                (
                    serde_json::json!({ "op": "settle_open_ticket", "teller_id": teller_id, "ticket_id": cmd.ticket_id, "request": cmd.request }),
                    Idem::Yes,
                )
            }
            "void_ticket" => {
                let cmd: tickets::VoidTicketCommand = match serde_json::from_str(&item.payload) {
                    Ok(c) => c,
                    Err(e) => return SendOutcome::Dead(format!("payload: {e}")),
                };
                (
                    serde_json::json!({ "op": "void_open_ticket", "teller_id": teller_id, "ticket_id": cmd.ticket_id, "request": cmd.request }),
                    Idem::Yes,
                )
            }
            // ── KDS bump / unbump (Phase E §2) ────────────────────────────────
            // Idempotent on the line: a 409/404 (re-bump of a gone/bumped line) is
            // a success. Replay returns 204 No Content — handled specially below.
            "bump_kitchen" | "unbump_kitchen" => {
                let cmd: kds::BumpCommand = match serde_json::from_str(&item.payload) {
                    Ok(c) => c,
                    Err(e) => return SendOutcome::Dead(format!("payload: {e}")),
                };
                let op = if item.op_type == "bump_kitchen" { "bump_kitchen_item" } else { "unbump_kitchen_item" };
                (
                    serde_json::json!({ "op": op, "teller_id": teller_id, "item_id": cmd.item_id }),
                    Idem::Yes,
                )
            }
            // A LAN mirror-relay backup: the payload IS the `/sync/replay` envelope
            // (verbatim from the originating device, with the ORIGINAL teller_id), so
            // it posts as-is and dedups server-side against the originator's own copy.
            "lan_mirror" => {
                let envelope: serde_json::Value = match serde_json::from_str(&item.payload) {
                    Ok(v) => v,
                    Err(e) => return SendOutcome::Dead(format!("payload: {e}")),
                };
                (envelope, Idem::Yes)
            }
            other => return SendOutcome::Dead(format!("unknown op_type {other}")),
        };

        match self.api.post_json("/sync/replay", &envelope).await {
            Ok(body) => {
                // Bump/unbump reply 204 No Content. An EMPTY body is the real
                // backend's ack; a NON-empty 200 for these is a captive-portal stub
                // → keep queued. (Checked before the JSON-object guard below, which
                // reads an empty body as portal-suspicious — correct for the JSON
                // ops, wrong for these 204s.)
                if matches!(item.op_type.as_str(), "bump_kitchen" | "unbump_kitchen") {
                    return if body.trim().is_empty() {
                        SendOutcome::Acked(None)
                    } else {
                        SendOutcome::Offline
                    };
                }
                // A LAN mirror wraps any op (a 204 bump or a JSON-object fire/settle);
                // an empty body OR a real JSON object is the backend's ack. A non-empty
                // non-object (HTML portal) keeps the backup queued.
                if item.op_type == "lan_mirror" {
                    return if body.trim().is_empty() || replay_backend_object(&body).is_some() {
                        SendOutcome::Acked(None)
                    } else {
                        SendOutcome::Offline
                    };
                }
                // Captive-portal / transparent-proxy defense. Our backend ALWAYS
                // answers /sync/replay with a JSON object (the op's result). A 200
                // carrying a Wi-Fi login page (HTML), a redirect stub, or an empty
                // body is NOT our backend — acking it would SILENTLY DROP a queued
                // sale. Treat any non-JSON-object 200 as a connectivity blip so the
                // op stays queued and reschedules (no retry budget burned).
                let json = match replay_backend_object(&body) {
                    Some(v) => v,
                    None => return SendOutcome::Offline,
                };
                let obj = &json;
                match item.op_type.as_str() {
                    // Cache the server's authoritative shift so the device reflects
                    // server-derived fields (opening_cash_was_edited, etc.). A 2xx
                    // object we can't decode as a Shift still means the open LANDED
                    // (replay is idempotent) — count it, don't loop forever.
                    "open_shift" => {
                        if let Ok(server) =
                            serde_json::from_value::<madar_api::models::Shift>(obj.clone())
                        {
                            let _ = shift::save(&self.store, &server);
                            SendOutcome::Acked(Some(server.id.to_string()))
                        } else {
                            SendOutcome::Acked(None)
                        }
                    }
                    // The money path. `OrderFull` flattens the order, so a real
                    // create response carries the order `id` at the TOP level. A
                    // JSON-object 200 WITHOUT a top-level `id` isn't a successful
                    // create (a proxy/portal stub) — keep it queued rather than ack
                    // a phantom sale. (Also finally captures the server order id,
                    // which the old `order.id` lookup never found post-flatten.)
                    "create_order" => match obj.get("id").and_then(|x| x.as_str()) {
                        Some(id) => {
                            // Advance this shift's synced base to the number the
                            // server actually assigned, so the NEXT ring-up predicts
                            // the right `#N` online too (where the order leaves the
                            // queue the instant it acks).
                            if let (Some(sid), Some(n)) =
                                (item.shift_id.as_deref(), obj.get("order_number").and_then(|v| v.as_i64()))
                            {
                                checkout::bump_order_base(&self.store, sid, n);
                            }
                            SendOutcome::Acked(Some(id.to_string()))
                        }
                        None => SendOutcome::Offline,
                    },
                    // Ticket ops return their view/order, which carries a top-level
                    // `id`. A JSON-object 200 WITHOUT one is a portal/proxy stub —
                    // keep it queued rather than ack a phantom fire/settle.
                    "open_ticket" | "ticket_add_round" | "settle_open_ticket" | "void_ticket" => {
                        match obj.get("id").and_then(|x| x.as_str()) {
                            Some(id) => SendOutcome::Acked(Some(id.to_string())),
                            None => SendOutcome::Offline,
                        }
                    }
                    _ => SendOutcome::Acked(None),
                }
            }
            Err(e) => classify_send(e, idem),
        }
    }

    /// Milliseconds to add to a queued timestamp to re-base it from the skew the
    /// device had at enqueue to the fresh skew we hold now (correct-at-sync). 0
    /// when either offset is unknown (legacy rows) — never makes things worse.
    fn rebase_delta_ms(&self, item: &store::OutboxItem) -> i64 {
        let now_skew_ms = self.clock_skew_secs.load(std::sync::atomic::Ordering::Relaxed) * 1000;
        match item.clock_offset_ms {
            Some(then) => now_skew_ms - then,
            None => 0,
        }
    }

    /// Wall-clock time CORRECTED by the last-known server skew. Queued ops must be
    /// stamped with this (not raw `Utc::now()`) so a till whose clock is wrong
    /// doesn't future-date its writes — the backend's `reject_if_future` would
    /// 400 a future-stamped open_shift/order and dead-letter the whole chain.
    /// Mirrors Flutter's `TimeUtils.now = DateTime.now() + offset`; the drain's
    /// `rebase_delta_ms` then only corrects for CHANGES in the skew between
    /// enqueue and send. The stamped offset is recorded per row in
    /// `clock_offset_ms` (via `outbox_meta`), keeping the two halves consistent.
    fn corrected_now(&self) -> chrono::DateTime<chrono::Utc> {
        let skew_ms = self.clock_skew_secs.load(std::sync::atomic::Ordering::Relaxed) * 1000;
        chrono::Utc::now() + chrono::Duration::milliseconds(skew_ms)
    }
}

/// What a single send attempt resolved to (drives the outbox state machine).
enum SendOutcome {
    /// Applied (or idempotently already-applied); carries the server id if known.
    Acked(Option<String>),
    /// Permanent rejection — dead-letter, surface in the stuck list.
    Dead(String),
    /// 401 — park the whole queue until the next successful login.
    AuthExpired,
    /// Connectivity blip — reschedule without burning retry budget; stop the pass.
    Offline,
    /// Retryable server/transport error — counted exponential backoff.
    Retry(String),
}

/// Idempotency profile of an endpoint, deciding how 409/404 are read.
enum Idem {
    /// Not idempotent for our purposes: 409/404 are genuine rejections → dead.
    No,
    /// Idempotent already-applied: 409/404 → treat as success.
    Yes,
    /// Void: 409 → already-voided (success); 404 → order never synced → dead.
    VoidIdem,
}

/// Captive-portal / transparent-proxy guard for a `/sync/replay` 200. Our backend
/// always answers with a JSON OBJECT (the op's result). A 200 carrying a Wi-Fi
/// login page (HTML), a redirect stub, an empty body, or a bare JSON array/scalar
/// is NOT our backend — return `None` so the caller keeps the op queued instead
/// of acking (and silently dropping) a real sale. Returns the parsed object only
/// when the body is genuinely our backend's shape.
fn replay_backend_object(body: &str) -> Option<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .filter(serde_json::Value::is_object)
}

/// Classify a `CoreError` from a send into a `SendOutcome` per the endpoint's
/// idempotency profile. Mirrors the Flutter drain's per-status branches.
fn classify_send(err: CoreError, idem: Idem) -> SendOutcome {
    match err {
        CoreError::Offline { .. } => SendOutcome::Offline,
        CoreError::Unauthenticated { .. } => SendOutcome::AuthExpired,
        // 5xx / timeouts / undecodable 2xx — retry with backoff.
        CoreError::Transient { detail } | CoreError::Internal { detail } => SendOutcome::Retry(detail),
        // Permanent validation/permission — retrying can't help.
        CoreError::Validation { detail, .. } | CoreError::Forbidden { action: detail, .. } => {
            SendOutcome::Dead(detail)
        }
        CoreError::Server { status, detail, .. } => match (status, idem) {
            (409, Idem::Yes) | (409, Idem::VoidIdem) | (404, Idem::Yes) => SendOutcome::Acked(None),
            // void 404 = the order never landed → don't silently swallow the void.
            (404, Idem::VoidIdem) => SendOutcome::Dead(format!("order not found on server — {detail}")),
            _ => SendOutcome::Dead(detail),
        },
    }
}

// ── offline read cache (server lists mirrored to kv) ─────────────────────────

/// Write-through cache for a server-fetched list, keyed in the kv store. Persists
/// the projected views as JSON so the NEXT read returns the last-synced snapshot
/// when offline (or when the live fetch fails) — the history screens (orders,
/// shifts, cash, delivery) stay populated offline instead of collapsing to only
/// the locally-queued rows. Free functions, not methods, because `#[uniffi::export]`
/// can't carry a generic across the FFI. Best-effort: a write failure skips the cache.
fn cache_views<T: serde::Serialize>(store: &store::Store, key: &str, views: &[T]) {
    if let Ok(json) = serde_json::to_string(views) {
        let _ = store.kv_put(key, &json);
    }
}

/// Synthesize an offline-overlay `TicketView` for a still-queued fire so the waiter
/// sees the ticket immediately, before it syncs. Status `"queued"`; the subtotal is
/// a rough estimate from the priced items (addons excluded — the server recomputes
/// authoritatively on sync); detailed lines arrive with the server view.
fn queued_ticket_view(cmd: &tickets::FireTicketCommand, event_at: &str) -> tickets::TicketView {
    let subtotal_minor: i64 = cmd
        .request
        .items
        .iter()
        .map(|it| it.unit_price.flatten().unwrap_or(0) as i64 * it.quantity as i64)
        .sum();
    tickets::TicketView {
        id: cmd.ticket_id.clone(),
        ticket_ref: None,
        table_id: cmd.request.table_id.flatten().map(|u| u.to_string()),
        status: "queued".into(),
        customer_name: cmd.request.customer_name.clone().flatten(),
        guest_count: cmd.request.guest_count.flatten(),
        subtotal_minor,
        order_id: None,
        opened_at: event_at.to_string(),
        queued_offline: true,
        lines: Vec::new(),
    }
}

/// Read a previously cached server list (empty when nothing's been synced yet).
fn cached_views<T: serde::de::DeserializeOwned>(store: &store::Store, key: &str) -> Vec<T> {
    store
        .kv_get(key)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str::<Vec<T>>(&s).ok())
        .unwrap_or_default()
}

// ── outbox backoff (mirrors offline_queue.dart constants) ────────────────────
const K_MAX_RETRIES: i64 = 8;
const K_BASE_BACKOFF_MS: i64 = 2_000; // 2s
const K_MAX_BACKOFF_MS: i64 = 300_000; // 5min
const K_NETWORK_RETRY_MS: i64 = 15_000; // fixed reschedule for connectivity blips
const K_ACKED_RETENTION_MS: i64 = 48 * 60 * 60 * 1000; // keep acked rows 48h

/// Epoch milliseconds (matches the outbox's `next_attempt_at` / `synced_at`).
fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Exponential backoff with jitter: BASE·2^(attempts-1), capped at MAX, plus a
/// deterministic per-item jitter (0–999ms, seeded by seq — no RNG dep) to avoid
/// a thundering herd when many items came due together.
fn compute_backoff_ms(attempts: i64, seq: i64) -> i64 {
    let shift = (attempts.clamp(1, 30) - 1) as u32;
    let exp = K_BASE_BACKOFF_MS.saturating_mul(1i64.checked_shl(shift).unwrap_or(i64::MAX));
    let capped = exp.min(K_MAX_BACKOFF_MS);
    let jitter = (seq.wrapping_mul(2_654_435_761)).rem_euclid(1000);
    (capped + jitter).min(K_MAX_BACKOFF_MS)
}

/// Re-base a double-`Option` timestamp (the generated `Option<Option<DateTime>>`).
fn rebase_dopt(field: &mut Option<Option<chrono::DateTime<chrono::FixedOffset>>>, delta_ms: i64) {
    if delta_ms != 0 {
        if let Some(Some(dt)) = field.as_mut().map(|o| o.as_mut()) {
            *dt += chrono::Duration::milliseconds(delta_ms);
        }
    }
}

// ── shift + routing (sync reads) ─────────────────────────────────────────────
#[uniffi::export]
impl MadarCore {
    /// The device's current shift (open or closed), served from the local store.
    pub fn current_shift(&self) -> Result<Option<shift::ShiftView>, CoreError> {
        shift::current(&self.store)
    }

    /// Suggested opening cash for the next shift (minor units) — the previous
    /// shift's declared closing, for cash continuity. 0 when none is known. The
    /// open-shift screen prefills this; deviating from it requires a reason.
    pub fn suggested_opening_cash_minor(&self) -> Result<i64, CoreError> {
        shift::suggested_opening_cash(&self.store)
    }

    /// The screen to show — decided ENTIRELY from core state (no host params; the
    /// device binding lives in the core store now). Resolution order: device-setup
    /// (unbound or mid-reconfigure) → login → kitchen-role device → the KDS for its
    /// configured station → waiter → tickets (no shift) → teller open/closed shift.
    pub fn app_route(&self) -> AppRoute {
        let cfg = device::load(&self.store);
        if !cfg.configured() {
            return AppRoute::DeviceSetup;
        }
        let guard = self.session.read().unwrap_or_else(|e| e.into_inner());
        let session = match guard.as_ref() {
            Some(s) => s,
            None => return AppRoute::Login,
        };
        // A kitchen-role device shows the KDS for its configured station (it needs
        // the session for the bus + kitchen permission, but holds no shift). With
        // no station bound yet, it must finish device setup first.
        if session.snapshot.role == "kitchen" {
            return match cfg.station_id {
                Some(station_id) => AppRoute::KitchenDisplay { station_id },
                None => AppRoute::DeviceSetup,
            };
        }
        // Waiters take orders and fire tickets but hold NO shift — route them to the
        // waiter screen BEFORE the open-shift gate (which they could never satisfy).
        if session.snapshot.role == "waiter" {
            return AppRoute::WaiterTickets;
        }
        // An open shift counts only if it belongs to THIS teller (a stale shift
        // from a previous teller on the device must not route them past setup).
        match shift::current(&self.store) {
            Ok(Some(s)) if s.is_open && s.teller_id == session.snapshot.user_id => AppRoute::Order,
            _ => AppRoute::OpenShift,
        }
    }

    /// Tear down the device's realtime subscription, if any. Idempotent — safe to
    /// call on sign-out, branch switch, or before re-subscribing with new topics.
    pub fn unsubscribe_realtime(&self) {
        let mut slot = self.realtime.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(h) = slot.take() {
            h.stop();
        }
    }

    /// Whether a realtime subscription is currently held (the supervisor task is
    /// alive — not a statement about live connectivity, which the listener reports).
    pub fn is_realtime_subscribed(&self) -> bool {
        self.realtime.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    // ── device binding (branch / till / station / printer) ────────────────────
    // All persisted in the core store; the host reads `device_config()` to render
    // device-setup / Settings and calls the setters. No host-side device state.

    /// The device's current binding (for device-setup / Settings + screen chrome).
    pub fn device_config(&self) -> device::DeviceConfigView {
        device::load(&self.store).into()
    }

    /// Bind the device to a branch (device setup). Clears the reconfigure flag.
    pub fn set_device_branch(&self, branch_id: String, branch_name: Option<String>) -> Result<(), CoreError> {
        device::update(&self.store, |c| {
            c.branch_id = Some(branch_id);
            c.branch_name = branch_name;
            c.reconfiguring = false;
        })?;
        Ok(())
    }

    /// Bind the device's till (POS drawer). `None` = use the branch default till.
    pub fn set_device_till(&self, till_id: Option<String>) -> Result<(), CoreError> {
        device::update(&self.store, |c| c.till_id = till_id.filter(|s| !s.is_empty()))?;
        Ok(())
    }

    /// Bind the device's kitchen station (a KDS device). `None` clears it.
    pub fn set_device_station(&self, station_id: Option<String>) -> Result<(), CoreError> {
        device::update(&self.store, |c| c.station_id = station_id.filter(|s| !s.is_empty()))?;
        Ok(())
    }

    /// Set the device's receipt/chit printer (host:port + brand `"epson"`/`"star"`).
    /// `None` host clears it.
    pub fn set_device_printer(&self, host: Option<String>, port: Option<u16>, brand: Option<String>) -> Result<(), CoreError> {
        device::update(&self.store, |c| {
            c.printer_host = host.filter(|s| !s.is_empty());
            c.printer_port = port;
            c.printer_brand = brand.filter(|s| !s.is_empty());
        })?;
        Ok(())
    }

    /// Re-enter device setup (keeps the binding but forces the setup screen until
    /// `set_device_branch` confirms a — possibly new — branch).
    pub fn start_reconfigure(&self) -> Result<(), CoreError> {
        device::update(&self.store, |c| c.reconfiguring = true)?;
        Ok(())
    }

    /// Wipe the device binding entirely (factory reset of the device config).
    pub fn clear_device(&self) -> Result<(), CoreError> {
        device::save(&self.store, &device::DeviceConfig::default())
    }
}

// ── realtime bus (one SSE per device) ─────────────────────────────────────────
#[uniffi::export(async_runtime = "tokio")]
impl MadarCore {
    /// Open (or REPLACE) the device's ONE realtime subscription for `branch_id`,
    /// asking only for `topics` the device's role/mode needs (e.g. `["delivery"]`
    /// on a till, `["kitchen"]` on a KDS, `["tickets","kitchen"]` on a waiter
    /// device). Any prior subscription is torn down first — never two connections.
    /// Events flow to `listener` until `unsubscribe_realtime`; the supervisor
    /// reconnects on drops (jittered backoff) and resumes from `Last-Event-ID`. A
    /// 401 stops it (re-subscribe after the next login). Must run on the tokio
    /// runtime so the supervisor task can spawn.
    pub async fn subscribe_realtime(
        &self,
        branch_id: String,
        topics: Vec<String>,
        listener: Box<dyn realtime::EventListener>,
    ) {
        // Share the listener (Arc) so the LAN relay bridge forwards to the SAME sink
        // as the cloud SSE — a cross-LAN event and its cloud twin both land here and
        // dedup via the host's snapshot-reload.
        let listener: Arc<dyn realtime::EventListener> = Arc::from(listener);
        *self.unified_listener.lock().unwrap_or_else(|e| e.into_inner()) = Some(listener.clone());
        let client = self.api.realtime_client();
        let handle = realtime::spawn_supervisor(client, branch_id, topics, listener);
        let mut slot = self.realtime.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(old) = slot.take() {
            old.stop();
        }
        *slot = Some(handle);
    }

    /// Start the device's ONE session-level realtime subscription — the unified entry
    /// the hosts call once after login (and on connectivity-regain). The CORE owns all
    /// the policy: it derives the topics from the signed-in role ([`topics_for_role`]),
    /// opens/replaces the single SSE, and wires an [`AlertingListener`] so every event
    /// (cloud OR LAN) both refreshes the host's board (via `listener`) AND raises a
    /// deduped, localized alert through `player` (ping + notification + haptic). The
    /// host's `player` is pure platform primitive — no decision logic. Idempotent in
    /// effect (replaces any prior subscription). Errs only if not signed in.
    pub async fn start_realtime(
        &self,
        listener: Box<dyn realtime::EventListener>,
        player: Box<dyn realtime::RealtimePlayer>,
    ) -> Result<(), CoreError> {
        // Idempotent: the supervisor auto-reconnects, so a re-call (e.g. on
        // connectivity-regain or a screen re-appearing) is a no-op. A fresh login
        // starts clean — `unsubscribe_realtime` in signOut cleared the handle.
        if self.realtime.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
            return Ok(());
        }
        let session = self.current_session().ok_or_else(|| CoreError::Unauthenticated {
            detail: "sign in before starting realtime".into(),
        })?;
        let branch_id = session.branch_id.clone().ok_or_else(|| CoreError::Validation {
            field: "branch".into(),
            detail: "no branch bound".into(),
        })?;
        let topics = realtime::topics_for_role(&session.role);
        // The alerting wrapper is the unified listener → the LAN bridge alerts too.
        let alerting: Arc<dyn realtime::EventListener> = Arc::new(realtime::AlertingListener::new(
            Arc::from(listener),
            Arc::from(player),
            self.locale.clone(),
            session.role.clone(),
        ));
        *self.unified_listener.lock().unwrap_or_else(|e| e.into_inner()) = Some(alerting.clone());
        let client = self.api.realtime_client();
        let handle = realtime::spawn_supervisor(client, branch_id, topics, alerting);
        let mut slot = self.realtime.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(old) = slot.take() {
            old.stop();
        }
        *slot = Some(handle);
        Ok(())
    }
}

// ── LAN offline relay (Phase E) ───────────────────────────────────────────────

/// The relay's inbound sink: forward a verified LAN event to the unified listener
/// (so the board refreshes just like a cloud event) and — for a write carrying a
/// replay op — MIRROR that op into the outbox so the write reaches the cloud even if
/// the originating device dies first. Shares the core's ONE `Store` (`Arc`, the SAME
/// connection — single-writer invariant preserved, no second WAL writer to contend),
/// plus a clone of the shared listener slot.
struct LanBridge {
    listener: Arc<Mutex<Option<Arc<dyn realtime::EventListener>>>>,
    store: Arc<store::Store>,
}

impl lan::LanInbound for LanBridge {
    fn on_lan_message(&self, msg: &lan::LanMessage) {
        // 1. Merge into the LAN-KDS overlay so an offline fire/bump shows on THIS
        //    device's board (the host refresh below reads the cached feed + overlay).
        match msg.event_type.as_str() {
            "kitchen.fired" => {
                if let Ok(t) = serde_json::from_str::<kds::KdsTicketView>(&msg.data) {
                    lan_kds_merge_ticket(&self.store, t);
                }
            }
            "kitchen.item_bumped" | "kitchen.item_unbumped" => {
                if let Some(id) = serde_json::from_str::<serde_json::Value>(&msg.data)
                    .ok()
                    .and_then(|v| v.get("item_id").and_then(|x| x.as_str()).map(String::from))
                {
                    lan_kds_apply_bump(&self.store, &id, msg.event_type == "kitchen.item_bumped");
                }
            }
            _ => {}
        }
        // 2. Forward to the unified listener (host refreshes the relevant board).
        if let Some(l) = self.listener.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            l.on_event(realtime::RealtimeEvent {
                event_type: msg.event_type.clone(),
                data: msg.data.clone(),
            });
        }
        // 3. Mirror-relay: enqueue the carried replay op as our own durable backup,
        //    idempotency-keyed so the cloud dedups it against the originator's copy.
        if let Some(op) = &msg.replay_op {
            mirror_replay_op(&self.store, op);
        }
    }
}

/// kv key for the LAN-overlay kitchen feed (projections of un-synced fires).
const LAN_KDS_CACHE: &str = "cache:kds:lan";

fn lan_kds_read(store: &store::Store) -> Vec<kds::KdsTicketView> {
    store
        .kv_get(LAN_KDS_CACHE)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}
fn lan_kds_write(store: &store::Store, v: &[kds::KdsTicketView]) {
    if let Ok(j) = serde_json::to_string(v) {
        let _ = store.kv_put(LAN_KDS_CACHE, &j);
    }
}
/// Upsert a LAN-projected kitchen ticket (by derived id) into the overlay.
fn lan_kds_merge_ticket(store: &store::Store, t: kds::KdsTicketView) {
    let mut v = lan_kds_read(store);
    match v.iter_mut().find(|x| x.id == t.id) {
        Some(slot) => *slot = t,
        None => v.push(t),
    }
    lan_kds_write(store, &v);
}
/// Apply a LAN-relayed bump to the overlay (greys the line on a peer's board too).
fn lan_kds_apply_bump(store: &store::Store, item_id: &str, bumped: bool) {
    let mut v = lan_kds_read(store);
    kds::apply_lan_bump(&mut v, item_id, bumped);
    lan_kds_write(store, &v);
}

/// Enqueue a received LAN write-op into the local outbox (robustness #4). The
/// envelope is the `/sync/replay` body; we re-wrap it as a `lan_mirror` outbox row
/// keyed on the op's idempotency key so a duplicate (we + the originator both drain
/// it) collapses server-side. Best-effort — a parse/enqueue failure just drops the
/// backup (the originator's own outbox is still the primary path).
fn mirror_replay_op(store: &store::Store, envelope_json: &str) {
    let Ok(env) = serde_json::from_str::<serde_json::Value>(envelope_json) else { return };
    let op = env.get("op").and_then(|v| v.as_str()).unwrap_or_default();
    let teller_id = env.get("teller_id").and_then(|v| v.as_str()).map(|s| s.to_string());
    // A stable id for dedup: the op kind + its primary idempotency handle.
    let handle = env
        .get("request")
        .and_then(|r| r.get("idempotency_key"))
        .and_then(|v| v.as_str())
        .or_else(|| env.get("item_id").and_then(|v| v.as_str()))
        .or_else(|| env.get("ticket_id").and_then(|v| v.as_str()))
        .unwrap_or(op);
    let op_id = format!("lanmirror:{op}:{handle}");
    let _ = store.enqueue(&store::NewOutboxOp {
        id: op_id,
        op_type: "lan_mirror".into(),
        idempotency_key: format!("{op}:{handle}"),
        payload: envelope_json.to_string(),
        event_at: chrono::Utc::now().to_rfc3339(),
        depends_on_seq: None,
        user_id: teller_id,
        clock_offset_ms: None,
        shift_id: None,
    });
}

/// Split a manual hub address (`host` or `host:port`) → (`host`, `port`), defaulting
/// to the fixed relay port so a manager need only type the hub's IP.
fn parse_hub_addr(addr: &str) -> (String, u16) {
    match addr.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().unwrap_or(lan::DEFAULT_TCP_PORT)),
        None => (addr.to_string(), lan::DEFAULT_TCP_PORT),
    }
}

impl MadarCore {
    /// Stable per-device id for the LAN mesh — minted once, persisted in the store.
    fn lan_device_id(&self) -> String {
        if let Ok(Some(id)) = self.store.kv_get("lan_device_id") {
            if !id.is_empty() {
                return id;
            }
        }
        let id = uuid::Uuid::new_v4().to_string();
        let _ = self.store.kv_put("lan_device_id", &id);
        id
    }

    /// The org's LAN secret (hex) from the cached offline-auth bundle — the HMAC root.
    fn lan_secret_hex(&self) -> Option<String> {
        let raw = self.store.kv_get(session::BUNDLE_KEY).ok()??;
        let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
        v.get("lan_secret").and_then(|x| x.as_str()).map(|s| s.to_string())
    }

    /// This device's current OPEN shift id (advertised to the LAN shift gate), if any.
    fn current_open_shift_id(&self) -> Option<String> {
        shift::current(&self.store).ok().flatten().filter(|s| s.is_open).map(|s| s.id)
    }

    /// Push the current open-shift state to the running relay (the till advert) — call
    /// after a shift opens/closes so the LAN shift gate reflects it within seconds.
    fn lan_sync_open_shift(&self) {
        if let Some(relay) = self.lan.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            relay.set_open_shift(self.current_open_shift_id());
        }
    }

    /// Publish a write event over the LAN (instant cross-device delivery): `data` is
    /// the display payload (e.g. a fire projection) and `replay_op` the mirror-relay
    /// envelope. No-op when the relay isn't up.
    async fn lan_publish(
        &self,
        topic: &str,
        event_type: &str,
        data: String,
        replay_op: Option<String>,
    ) {
        let relay = self.lan.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(relay) = relay {
            let at = self.corrected_now().timestamp_millis();
            relay.publish(topic, event_type, data, replay_op, at).await;
        }
    }
}

// ── LAN relay control (Phase E) ───────────────────────────────────────────────
#[uniffi::export(async_runtime = "tokio")]
impl MadarCore {
    /// Start the LAN relay for the signed-in branch (idempotent). Needs a session +
    /// the cached bundle's LAN secret; binds the embedded server, begins discovery
    /// (mDNS + UDP beacon), advertises this till's open shift, and wires any manual
    /// hub. Safe to call after every login — a no-op if already running.
    pub async fn lan_start(&self) -> Result<(), CoreError> {
        if self.lan.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
            return Ok(());
        }
        let session = self.current_session().ok_or_else(|| CoreError::Unauthenticated {
            detail: "sign in before starting the LAN relay".into(),
        })?;
        let branch_id = session.branch_id.clone().ok_or_else(|| CoreError::Validation {
            field: "branch".into(),
            detail: "no branch bound".into(),
        })?;
        let secret = self.lan_secret_hex().ok_or_else(|| CoreError::Validation {
            field: "lan_secret".into(),
            detail: "no LAN secret — sign in online once to fetch the bundle".into(),
        })?;
        let dev = device::load(&self.store);
        let cfg = lan::LanConfig {
            device_id: self.lan_device_id(),
            branch_id: branch_id.clone(),
            role: session.role.clone(),
            station_id: dev.station_id.clone(),
            key: lan::branch_key(&secret, &branch_id),
            tcp_port: lan::DEFAULT_TCP_PORT,
            beacon_port: lan::DEFAULT_BEACON_PORT,
        };
        let bridge = Arc::new(LanBridge {
            listener: self.unified_listener.clone(),
            store: self.store.clone(), // the SAME store instance, shared via Arc
        });
        let relay = Arc::new(lan::LanRelay::new(cfg, bridge));
        relay.start().await?;
        relay.set_open_shift(self.current_open_shift_id());
        if let Some(hub) = dev.lan_hub.filter(|s| !s.trim().is_empty()) {
            let (host, port) = parse_hub_addr(hub.trim());
            relay.add_manual_hub(host, port);
        }
        *self.lan.lock().unwrap_or_else(|e| e.into_inner()) = Some(relay);
        Ok(())
    }
}

#[uniffi::export]
impl MadarCore {
    /// Stop + tear down the LAN relay (idempotent). Call on logout / branch switch.
    pub fn lan_stop(&self) {
        if let Some(relay) = self.lan.lock().unwrap_or_else(|e| e.into_inner()).take() {
            relay.stop();
        }
    }

    /// Whether the LAN relay is currently running.
    pub fn lan_active(&self) -> bool {
        self.lan.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    /// Live discovered peers + manual hubs (a "LAN: N devices" diagnostics chip).
    pub fn lan_peer_count(&self) -> u32 {
        self.lan
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|r| r.peer_count())
            .unwrap_or(0)
    }

    /// The LAN shift-open gate: is a till at this branch advertising a FRESH open
    /// shift right now? The freshest "is the branch operating" signal (it beats the
    /// backend, which may not yet know a till opened/closed). `false` if not running.
    pub fn lan_branch_has_open_till(&self) -> bool {
        self.lan
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|r| r.branch_has_open_till())
            .unwrap_or(false)
    }

    /// Persist a manual LAN hub-IP (`host` or `host:port`) in the device config and,
    /// if the relay is running, register it immediately. `None`/empty clears it.
    pub fn set_device_lan_hub(&self, hub: Option<String>) -> Result<(), CoreError> {
        let hub = hub.filter(|s| !s.trim().is_empty());
        device::update(&self.store, |c| c.lan_hub = hub.clone())?;
        if let (Some(addr), Some(relay)) =
            (hub, self.lan.lock().unwrap_or_else(|e| e.into_inner()).clone())
        {
            let (host, port) = parse_hub_addr(addr.trim());
            relay.add_manual_hub(host, port);
        }
        Ok(())
    }
}

// ── localization (sync) ──────────────────────────────────────────────────────
#[uniffi::export]
impl MadarCore {
    /// Localized UI string for `key` in the device locale (en/ar; falls back to
    /// en, then the key). The single source of truth for both hosts.
    pub fn tr(&self, key: String) -> String {
        i18n::tr(&self.current_locale(), &key)
    }
    /// The active locale (BCP-47).
    pub fn locale(&self) -> String {
        self.current_locale()
    }
    /// Change the active UI locale at runtime (e.g. "en" / "ar"). Strings,
    /// RTL, and catalog `*_translations` all re-resolve on the next read; the
    /// host persists the choice and re-renders.
    pub fn set_locale(&self, locale: String) {
        *self.locale.write().unwrap_or_else(|e| e.into_inner()) = locale;
    }
    /// Whether the locale is right-to-left (host flips layout direction).
    pub fn is_rtl(&self) -> bool {
        i18n::is_rtl(&self.current_locale())
    }
}

// ── receipt rendering (sync; pure byte assembly) ─────────────────────────────
#[uniffi::export]
impl MadarCore {
    /// Render a placed order's receipt to printer bytes ready to stream to a
    /// thermal printer. The receipt is rasterized to a 1-bit bitmap (logo +
    /// Arabic, matching the on-screen preview) and wrapped in the brand's raster
    /// protocol — text commands can't drive the raster-only TSP143III. Labels
    /// resolve from the active locale; `store_name` (branch) and `currency` come
    /// from the host. `width` is retained for API stability but unused (the raster
    /// width is fixed at 576 dots). Pair with `send_to_printer`.
    pub fn render_receipt(
        &self,
        mut receipt: checkout::ReceiptView,
        store_name: String,
        currency: String,
        width: u32,
        brand: receipt::PrinterBrand,
    ) -> Vec<u8> {
        // Stamp the printed receipt in the BRANCH timezone — the ESC/POS formatter
        // renders the timestamp in its own offset, so convert it first (a Cairo
        // store prints Cairo time even on a device set to another zone).
        receipt.created_at = timefmt::to_branch_local(&self.store, &receipt.created_at);
        let loc = self.current_locale();
        let tr = |k: &str| i18n::tr(&loc, k);
        let ctx = receipt::EscPosCtx {
            store_name,
            currency,
            width,
            labels: receipt::ReceiptLabels {
                order: tr("receipt.order"),
                reference: tr("receipt.ref"),
                voided: tr("receipt.voided"),
                delivery: tr("receipt.delivery"),
                channel_in_mall: tr("delivery.in_mall"),
                channel_outside: tr("delivery.outside"),
                customer: tr("receipt.customer"),
                phone: tr("receipt.phone"),
                address: tr("receipt.address"),
                zone: tr("receipt.zone"),
                delivery_ref: tr("receipt.delivery_ref"),
                payment_hint: tr("receipt.payment_hint"),
                notes: tr("receipt.notes"),
                subtotal: tr("order.subtotal"),
                discount: tr("order.discount"),
                tax: tr("order.tax"),
                delivery_fee: tr("receipt.delivery_fee"),
                total: tr("order.total"),
                tip: tr("order.tip"),
                cash: tr("receipt.cash"),
                change: tr("order.change"),
                payment: tr("receipt.payment"),
                teller: tr("receipt.teller"),
                served_by: tr("receipt.served_by"),
                queued: tr("order.queued_hint"),
                thank_you: tr("receipt.thank_you"),
            },
        };
        // Logo bytes were cached (online) into the blob store by the branch fetch;
        // read-through here so printing stays offline-capable. None → name-only header.
        let logo = self.store.blob_get(checkout::KEY_ORG_LOGO_PNG).ok().flatten();
        let bitmap = render::render_receipt(&receipt, &ctx, logo.as_deref());
        receipt::raster_for(brand, &bitmap)
    }

    /// Cash-drawer kick bytes for the chosen printer dialect — send via
    /// `send_to_printer` right after a CASH sale's receipt so the till pops.
    /// Caller gates on `receipt.is_cash` (and skips it on reprints).
    pub fn cash_drawer_kick(&self, brand: receipt::PrinterBrand) -> Vec<u8> {
        receipt::drawer_kick_for(brand)
    }

    /// Render the shift report (Z-report) to printer bytes — rasterized like
    /// `render_receipt` (text commands can't drive the TSP143III). `width` is
    /// retained for API stability but unused (raster width is fixed at 576 dots).
    /// Pair with `send_to_printer`.
    pub fn render_shift_report(
        &self,
        mut report: shift::ShiftReportView,
        store_name: String,
        currency: String,
        width: u32,
        brand: receipt::PrinterBrand,
    ) -> Vec<u8> {
        let _ = width;
        // Stamp the report's timestamps in the BRANCH timezone (as render_receipt
        // does for created_at), so the printed times read in the store's local time.
        report.opened_at = timefmt::to_branch_local(&self.store, &report.opened_at);
        report.printed_at = timefmt::to_branch_local(&self.store, &report.printed_at);
        if let Some(c) = report.closed_at.clone() {
            report.closed_at = Some(timefmt::to_branch_local(&self.store, &c));
        }
        let loc = self.current_locale();
        let tr = |k: &str| i18n::tr(&loc, k);
        let labels = receipt::ShiftReportLabels {
            title: tr("shift.report_title"),
            business_date: tr("shift.business_date"),
            printed_at: tr("shift.printed_at"),
            teller: tr("shift.teller"),
            opened: tr("shift.opened_at"),
            closed: tr("shifts.closed"),
            interim: tr("shift.interim"),
            payments: tr("shift.payments"),
            orders: tr("shift.orders"),
            total_collected: tr("shift.total_collected"),
            drawer_ops: tr("shift.drawer_ops"),
            cash_in: tr("shift.cash_in"),
            cash_out: tr("shift.cash_out"),
            cash_recon: tr("shift.cash_recon"),
            opening: tr("shift.opening_cash"),
            opening_mismatch: tr("shift.opening_mismatch"),
            opening_reason: tr("shift.opening_reason_label"),
            expected: tr("shift.expected_cash"),
            actual: tr("shift.counted_cash"),
            not_closed: tr("shift.not_closed"),
            difference: tr("shift.difference"),
            short_by: tr("shift.drawer_short"),
            over_by: tr("shift.drawer_over"),
            voided: tr("history.voided"),
            transactions: tr("shift.transactions"),
            end_of_report: tr("shift.end_of_report"),
            cash_moves: tr("shift.cash_moves"),
            by_method: tr("shift.by_method"),
        };
        let bitmap = render::render_shift_report(&report, &store_name, &currency, &labels);
        receipt::raster_for(brand, &bitmap)
    }
}

// ── catalog reads (sync; serve the local mirror, always succeed offline) ─────
#[uniffi::export]
impl MadarCore {
    /// Themed style (icon key + gradient palette) for a category/item name —
    /// the host maps `icon` to a glyph and paints the gradient. Pure; mirrors
    /// Flutter's `CatStyle.of`. `dark` picks the dark-mode palette.
    pub fn category_style(&self, name: String, dark: bool) -> catstyle::CatStyleView {
        catstyle::category_style(&name, dark)
    }

    pub fn list_menu_items(&self) -> Result<Vec<menu::MenuItemView>, CoreError> {
        menu::menu_items(&self.store, &self.current_locale())
    }
    pub fn list_categories(&self) -> Result<Vec<menu::CategoryView>, CoreError> {
        menu::categories(&self.store, &self.current_locale())
    }
    pub fn list_addon_catalog(&self) -> Result<Vec<menu::AddonItemView>, CoreError> {
        menu::addons(&self.store, &self.current_locale())
    }
    /// Bundles orderable right now — status active and within their date/time
    /// window at `now` (branch-local). The host passes its local time so the
    /// window is evaluated in the till's timezone (Flutter parity).
    pub fn available_bundles(&self, now_rfc3339: String) -> Result<Vec<menu::BundleView>, CoreError> {
        let now = chrono::DateTime::parse_from_rfc3339(&now_rfc3339)
            .map_err(|_| CoreError::Validation { field: "now".into(), detail: "bad timestamp".into() })?;
        Ok(menu::bundles(&self.store, &self.current_locale())?
            .into_iter()
            .filter(|b| menu::bundle_available(b, now))
            .collect())
    }
    pub fn list_payment_methods(&self) -> Result<Vec<menu::PaymentMethodView>, CoreError> {
        menu::payment_methods(&self.store, &self.current_locale())
    }
    pub fn list_discounts(&self) -> Result<Vec<menu::DiscountView>, CoreError> {
        menu::discounts(&self.store, &self.current_locale())
    }
}

// ── cart (sync; client-only order state, offline-safe, kv-persisted) ──────────
#[uniffi::export]
impl MadarCore {
    /// The current cart lines (empty when none).
    pub fn cart_lines(&self) -> Result<Vec<cart::CartLineView>, CoreError> {
        cart::lines(&self.store)
    }
    /// Add one unit of a menu item (merges into the matching line). The host
    /// passes the resolved display name + unit price so the cart is self-contained.
    pub fn cart_add(
        &self,
        item_id: String,
        name: String,
        unit_price_minor: i64,
    ) -> Result<Vec<cart::CartLineView>, CoreError> {
        cart::add(&self.store, &item_id, &name, unit_price_minor)
    }
    /// Add a CONFIGURED line (size + addons + optionals + notes). The core
    /// resolves the charged prices from the cached catalog (size unit price;
    /// addon swap-delta vs additive; optional prices) and merges identical
    /// configs. `addons` carry the chosen ids + quantities; the prices are
    /// resolved here, not trusted from the host.
    pub fn cart_add_configured(
        &self,
        item_id: String,
        size_label: Option<String>,
        addons: Vec<cart::AddonSelection>,
        optional_field_ids: Vec<String>,
        qty: i64,
        notes: Option<String>,
    ) -> Result<Vec<cart::CartLineView>, CoreError> {
        let items = menu::menu_items(&self.store, &self.current_locale())?;
        let item = items
            .iter()
            .find(|i| i.id == item_id)
            .ok_or_else(|| CoreError::Validation { field: "item".into(), detail: "unknown item".into() })?;
        let addon_catalog = menu::addons(&self.store, &self.current_locale())?;
        let line = cart::resolve_line(item, &addon_catalog, size_label, &addons, &optional_field_ids, qty, notes);
        cart::add_resolved(&self.store, line)
    }
    /// Add a configured BUNDLE line: the fixed bundle price + each component's
    /// chosen item/size/addons/optionals. The core resolves the component
    /// up-charges from the catalog (component base/size price is never charged —
    /// the bundle price covers it) and merges identical bundle configs.
    pub fn cart_add_bundle(
        &self,
        bundle_id: String,
        components: Vec<cart::BundleComponentSelection>,
        qty: i64,
    ) -> Result<Vec<cart::CartLineView>, CoreError> {
        let locale = self.current_locale();
        let bundles = menu::bundles(&self.store, &locale)?;
        let bundle = bundles
            .iter()
            .find(|b| b.id == bundle_id)
            .ok_or_else(|| CoreError::Validation { field: "bundle".into(), detail: "unknown bundle".into() })?;
        let items = menu::menu_items(&self.store, &locale)?;
        let addon_catalog = menu::addons(&self.store, &locale)?;
        let line = cart::resolve_bundle_line(bundle, &items, &addon_catalog, &components, qty);
        cart::add_resolved(&self.store, line)
    }
    /// Active addons offered for an item, with their CHARGED price resolved (swap
    /// delta / full) — the customization sheet groups these by `addon_type`.
    pub fn list_item_addons(&self, item_id: String) -> Result<Vec<cart::ItemAddonView>, CoreError> {
        let items = menu::menu_items(&self.store, &self.current_locale())?;
        let item = items
            .iter()
            .find(|i| i.id == item_id)
            .ok_or_else(|| CoreError::Validation { field: "item".into(), detail: "unknown item".into() })?;
        let addon_catalog = menu::addons(&self.store, &self.current_locale())?;
        Ok(cart::item_addons(item, &addon_catalog))
    }
    /// Live recipe preview for the current selection (size + addons + optionals).
    /// Pure projection over the mirrored catalog, so the customization sheet can
    /// recompute on every toggle, online or offline. Mirrors the Flutter teller
    /// app: base by size, milk/coffee swaps, additive addons (× qty), and
    /// optional-field ingredient contributions.
    pub fn compute_recipe(
        &self,
        item_id: String,
        size_label: Option<String>,
        addons: Vec<cart::AddonSelection>,
        optional_field_ids: Vec<String>,
    ) -> Result<Vec<recipe::ComputedRecipeLineView>, CoreError> {
        let items = menu::menu_items(&self.store, &self.current_locale())?;
        let item = items
            .iter()
            .find(|i| i.id == item_id)
            .ok_or_else(|| CoreError::Validation { field: "item".into(), detail: "unknown item".into() })?;
        let addon_catalog = menu::addons(&self.store, &self.current_locale())?;
        Ok(recipe::compute_recipe(item, &addon_catalog, size_label.as_deref(), &addons, &optional_field_ids))
    }
    /// Set a line's absolute quantity (by its key); `qty <= 0` removes the line.
    pub fn cart_set_qty(
        &self,
        item_id: String,
        qty: i64,
    ) -> Result<Vec<cart::CartLineView>, CoreError> {
        cart::set_qty(&self.store, &item_id, qty)
    }
    /// Remove a line entirely (stashed for undo — see `cart_restore_removed`).
    pub fn cart_remove(&self, item_id: String) -> Result<Vec<cart::CartLineView>, CoreError> {
        cart::remove(&self.store, &item_id)
    }
    /// Undo the last `cart_remove` — re-inserts the swiped-away line. No-op if
    /// nothing was removed (or it was already restored / the cart was cleared).
    pub fn cart_restore_removed(&self) -> Result<Vec<cart::CartLineView>, CoreError> {
        cart::restore_last_removed(&self.store)
    }
    /// Empty the cart.
    pub fn cart_clear(&self) -> Result<(), CoreError> {
        cart::clear(&self.store)
    }
    /// Park the current cart as a named draft (held order) and empty the cart.
    pub fn hold_cart(&self, name: String) -> Result<(), CoreError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        cart::hold(&self.store, id, name, now)
    }
    /// The parked drafts (held orders), newest first.
    pub fn list_drafts(&self) -> Result<Vec<cart::DraftView>, CoreError> {
        cart::drafts(&self.store)
    }
    /// Restore a draft into the cart (replaces current lines) and drop it.
    pub fn restore_draft(&self, id: String) -> Result<Vec<cart::CartLineView>, CoreError> {
        cart::restore_draft(&self.store, &id)
    }
    /// Discard a parked draft.
    pub fn discard_draft(&self, id: String) -> Result<(), CoreError> {
        cart::discard_draft(&self.store, &id)
    }
    /// Apply a discount (by id) to the cart — reflected in `cart_totals`.
    pub fn cart_set_discount(&self, discount_id: String) -> Result<(), CoreError> {
        cart::set_discount(&self.store, &discount_id)
    }
    /// Remove the cart discount.
    pub fn cart_clear_discount(&self) -> Result<(), CoreError> {
        cart::clear_discount(&self.store)
    }
    /// The selected discount id (for the tender UI), or `None`.
    pub fn cart_discount_id(&self) -> Result<Option<String>, CoreError> {
        cart::discount_id(&self.store)
    }
    /// Priced cart summary at the session's org tax rate (0 when signed out),
    /// computed through the pricing engine.
    pub fn cart_totals(&self) -> Result<cart::CartTotals, CoreError> {
        let tax_rate = self.current_session().map(|s| s.tax_rate).unwrap_or(0.0);
        cart::totals(&self.store, tax_rate)
    }
}

/// A queued/failed outbox command, projected for the sync center.
#[derive(uniffi::Record, Clone, Debug)]
pub struct OutboxItemView {
    pub id: String,
    /// `open_shift` | `close_shift` | `create_order` | …
    pub op_type: String,
    /// `pending` | `inflight` | `dead`.
    pub status: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub event_at: String,
}

/// One-shot sync health for the action-bar chip + offline banner. `pending` is
/// the in-flight/queued set, `failed` the stuck (dead) set, `online` the
/// session's connectivity. The host maps these to the chip label/tone.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct SyncStatusView {
    pub pending: u32,
    pub failed: u32,
    pub online: bool,
    /// `true` when the outbox is parked on a 401 — the host prompts a re-login
    /// to resume syncing (nothing drains until then).
    pub auth_paused: bool,
}

// ── sync center (outbox visibility + retry/discard) ──────────────────────────
#[uniffi::export]
impl MadarCore {
    /// Queued + failed commands for the sync center (acked rows hidden), oldest
    /// first. Always succeeds offline.
    pub fn list_outbox(&self) -> Result<Vec<OutboxItemView>, CoreError> {
        Ok(self
            .store
            .list_active()?
            .into_iter()
            .map(|i| OutboxItemView {
                id: i.id,
                op_type: i.op_type,
                status: i.status,
                attempts: i.attempts,
                last_error: i.last_error,
                event_at: i.event_at,
            })
            .collect())
    }

    /// Discard a single DEAD command (the teller gives up on it). Returns true
    /// if a dead command with that id was removed.
    pub fn discard_outbox_item(&self, id: String) -> Result<bool, CoreError> {
        self.store.discard_dead(&id)
    }

    /// Sync health for the action-bar chip + offline banner (counts + online),
    /// in one cheap local read. Always succeeds offline.
    pub fn sync_status(&self) -> Result<SyncStatusView, CoreError> {
        Ok(SyncStatusView {
            pending: self.store.pending_count()?,
            failed: self.store.dead_count()?,
            online: self.current_session().map(|s| s.online).unwrap_or(false),
            auth_paused: self.auth_paused.load(std::sync::atomic::Ordering::Relaxed),
        })
    }

    /// Recent diagnostic warnings (newest first) — the Settings → Diagnostics
    /// feed. Captures sync dead-letters, cascade failures, and auth parks.
    pub fn recent_logs(&self) -> Vec<DiagLogView> {
        let g = self.diag.lock().unwrap_or_else(|e| e.into_inner());
        g.iter()
            .rev()
            .map(|e| DiagLogView { at: e.at.clone(), level: e.level.clone(), message: e.message.clone() })
            .collect()
    }

    /// Clear the diagnostics feed.
    pub fn clear_logs(&self) {
        self.diag.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// Server-vs-device clock skew in MINUTES (server minus device, refreshed by
    /// `refresh_connectivity`). The host shows a banner past a threshold so the
    /// teller fixes the clock before offline work is mis-timestamped.
    pub fn clock_skew_minutes(&self) -> i32 {
        (self.clock_skew_secs.load(std::sync::atomic::Ordering::Relaxed) / 60) as i32
    }

    /// Format a stored RFC3339 timestamp for DISPLAY in the BRANCH's timezone (not
    /// the device's) — the single source of truth so Swift + Kotlin render every
    /// order/shift/cash/receipt time identically (and correctly, regardless of where
    /// the device sits). Mirrors Flutter's `AppTz.local()` + `formatting.dart`.
    pub fn format_time(&self, rfc3339: String, style: timefmt::TimeStyle) -> String {
        timefmt::format(&self.store, &rfc3339, style)
    }

    /// The branch's IANA timezone name (cached at login, or the Cairo fallback) —
    /// for any host that needs the raw zone (e.g. a platform date picker).
    pub fn branch_timezone(&self) -> String {
        timefmt::branch_tz(&self.store).name().to_string()
    }

    /// Live shift stats (sales total + order count) for the action-bar pill,
    /// derived from the orders the host already loaded via `list_shift_orders`
    /// (synced + queued), voided excluded. Pure — no extra network.
    pub fn shift_stats(&self, orders: Vec<orders::OrderSummaryView>) -> orders::ShiftStatsView {
        orders::shift_stats(&orders)
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl MadarCore {
    /// Online login (PIN or email). Mints a bearer, mirrors permissions, caches
    /// the org's offline-auth bundle for later offline unlock, and persists the
    /// session to the host vault. Returns `Offline` if disconnected.
    pub async fn login(
        &self,
        req: session::LoginRequest,
    ) -> Result<session::SessionSnapshot, CoreError> {
        use madar_api::apis::{auth_api, orgs_api};

        let wire = session::wire_login_request(&req)?;
        // Tell the server which open shifts THIS device is already closing (queued
        // close commands). The login guard rejects signing in over ANOTHER teller's
        // open shift, EXCEPT one we acknowledge here — that's a legitimate offline
        // handover whose close lands via /sync/replay moments after this login.
        let ack_closing = self.closing_shift_ids_csv();
        let body = self
            .api
            .post_with_header("/auth/login", &wire, ("X-Sufrix-Closing-Shifts", &ack_closing))
            .await?;
        let resp: madar_api::models::LoginResponse = serde_json::from_str(&body)
            .map_err(|e| CoreError::Internal { detail: format!("decode: {e}") })?;

        // Token is live from here on. A fresh token un-parks a 401-stalled
        // outbox so a re-login resumes syncing immediately.
        self.api.set_bearer(Some(resp.token.clone()));
        self.auth_paused.store(false, std::sync::atomic::Ordering::Relaxed);

        // PIN login carries the device branch; email login has none.
        let branch_id = req.branch_id.clone();
        let mut snapshot = session::snapshot_from_login(&resp, branch_id);

        // Mirror permissions (best-effort — a perms blip must not void a good login).
        let permissions = match auth_api::get_my_permissions(&self.api.config()).await {
            Ok(p) => {
                snapshot.permissions_loaded = true;
                session::permissions_from(&p)
            }
            Err(_) => Vec::new(),
        };

        // Cache the org's offline-auth bundle so any org teller can unlock offline
        // later (best-effort — failure just means no offline unlock until next login).
        if let Some(org_id) = snapshot.org_id.clone() {
            if let Ok(bundle) = orgs_api::offline_auth_bundle(
                &self.api.config(),
                orgs_api::OfflineAuthBundleParams { id: org_id },
            )
            .await
            {
                session::cache_bundle(&self.store, &bundle, &snapshot);
            }
        }

        let state = session::SessionState {
            snapshot: snapshot.clone(),
            permissions,
            token: Some(resp.token),
        };
        self.persist_and_set(state);

        // Drain BEFORE the host reconciles the shift. On a shared till the device
        // may hold a previous teller's backlog (e.g. the close of an online shift
        // they left offline); flushing it now — attributed to its own teller via
        // /sync/replay — means the just-signed-in teller sees the CURRENT server
        // state, not a stale open shift that's about to be closed. Best-effort.
        let _ = self.drain_outbox().await;
        // Adopt THIS teller's current shift right here (teller-scoped server query)
        // so the device cache + routing are correct the instant login returns — the
        // host needn't win a race with a separate reconcile, and a stale or another
        // teller's open shift can never leave us on the wrong screen. Best-effort.
        let _ = self.refresh_shift().await;
        // Cache the branch timezone + the open shift's order-number base / branch
        // code so an OFFLINE checkout can predict the EXACT number/ref the server
        // will mint (identical post-checkout + reprint receipts). Best-effort.
        let _ = self.cache_numbering_context().await;
        Ok(snapshot)
    }

    /// Cache the branch code + IANA timezone (from `get_branch`) so an OFFLINE
    /// checkout can MINT the exact number/ref the server stores — from first boot,
    /// no synced order needed (the first login is always online, so this always runs
    /// before any offline stretch). Best-effort + online-only.
    async fn cache_numbering_context(&self) {
        let Ok((_, Some(branch_id))) = self.org_branch() else { return };
        if let Ok(b) = madar_api::apis::branches_api::get_branch(
            &self.api.config(),
            madar_api::apis::branches_api::GetBranchParams { id: branch_id },
        )
        .await
        {
            let _ = self.store.kv_put(checkout::KEY_BRANCH_TZ, &b.timezone);
            if let Some(code) = b.code.flatten().filter(|s| !s.is_empty()) {
                let _ = self.store.kv_put(checkout::KEY_BRANCH_CODE, &code);
            }
            // Persist the org logo URL the SAME way as the branch code/tz (durable
            // kv from the same get_branch), so it survives restarts/offline and a
            // manual sync re-pulls it. Only overwrite with a non-empty value, so a
            // transient blank can't wipe a good cached logo.
            if let Some(logo) = b.org_logo_url.flatten().filter(|s| !s.is_empty()) {
                let _ = self.store.kv_put(checkout::KEY_ORG_LOGO_URL, &logo);
                // Pull the logo BYTES too, so the (offline-capable) receipt
                // rasterizer can composite it without ever hitting the network.
                // Best-effort: a failure just leaves the last good cached logo
                // (or none) in place — the receipt still prints with the name.
                if let Ok(bytes) = self.api.get_url_bytes(&logo).await {
                    if !bytes.is_empty() {
                        let _ = self.store.blob_put(checkout::KEY_ORG_LOGO_PNG, &bytes);
                    }
                }
            }
        }

        // Seed the open shift's synced order-number base from the server, so the
        // very next ring-up predicts MAX(order_number)+1 (not #1) even online and
        // even right after resuming a shift that already has orders. Best-effort:
        // offline this no-ops and the base advances on ack instead.
        if let Ok(Some(shift)) = shift::current(&self.store) {
            if shift.is_open {
                if let Ok(orders) = self.list_orders_for_shift(shift.id.clone()).await {
                    let max = orders.iter().filter_map(|o| o.order_number).max().unwrap_or(0) as i64;
                    checkout::bump_order_base(&self.store, &shift.id, max);
                }
            }
        }
    }

    /// This device's managed code — the `<DEVICE>` segment of every order_ref.
    /// Auto-assigned (stable random) on first use; the manager renames it in
    /// Settings (e.g. `T1`/`W2`/`K1`) so a branch's devices are distinct.
    pub fn device_code(&self) -> String {
        checkout::device_code_or_default(&self.store)
    }

    /// Set this device's managed code (Settings). Sanitized to short A-Z0-9; an
    /// empty/blank value is ignored (keeps the current code).
    pub fn set_device_code(&self, code: String) {
        let clean: String = code
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(6)
            .collect::<String>()
            .to_uppercase();
        if !clean.is_empty() {
            let _ = self.store.kv_put(checkout::KEY_DEVICE_CODE, &clean);
        }
    }

    /// One-call sign-in. The online→offline decision lives HERE, not in the host
    /// UI (the One Rule): try an online `login` first; if the network is down and
    /// this is a teller PIN login, fall back to an offline unlock against the
    /// cached org bundle. Validation / auth errors propagate (no fallback) so a
    /// wrong PIN online doesn't silently try the offline path.
    pub async fn sign_in(
        &self,
        req: session::LoginRequest,
    ) -> Result<session::SessionSnapshot, CoreError> {
        // The device's bound branch is authoritative — read it from the core device
        // config (the host no longer tracks it). PIN login derives the org from it.
        let mut req = req;
        if let Some(b) = device::load(&self.store).branch_id {
            req.branch_id = Some(b);
        }

        // OWNERSHIP GATE (online OR offline): the device may hold an OPEN shift
        // left by a previous teller who signed out without closing it. A shift is
        // its owner's drawer — only they may resume it. Anyone else is rejected
        // (they must close it first), so no teller can ever take over a shift they
        // don't own. Checked up-front, by name, against the device's cached shift
        // (kept across logout). A shift the device already CLOSED locally is not
        // `is_open`, so the normal close-then-switch handover still works.
        if let Some(name) = req.name.as_deref() {
            if let Some(s) = shift::current(&self.store)? {
                if s.is_open && !s.teller_name.eq_ignore_ascii_case(name.trim()) {
                    return Err(CoreError::Forbidden {
                        resource: "shift".into(),
                        action: format!(
                            "This device has an open shift belonging to {}. It must be closed before signing in.",
                            s.teller_name
                        ),
                    });
                }
            }
        }

        // Whether a connectivity failure may fall back to an offline unlock.
        let offline_ok = matches!(req.mode, session::LoginMode::Pin)
            && req.name.is_some()
            && req.pin.is_some()
            && req.branch_id.is_some();
        let offline = |this: &Self| {
            this.unlock_offline(
                req.name.clone().unwrap_or_default(),
                req.pin.clone().unwrap_or_default(),
                req.branch_id.clone().unwrap_or_default(),
            )
        };

        // Hard-bound the online attempt: a black-holed/slow network must never
        // leave the teller on an endless spinner. On timeout → treat as offline.
        let attempt =
            tokio::time::timeout(std::time::Duration::from_secs(7), self.login(req.clone())).await;

        match attempt {
            Ok(Ok(snapshot)) => Ok(snapshot),
            // We never really reached the backend — transport loss OR a captive
            // portal / proxy answering in its place (HTML we can't decode, or a
            // 511/407/408). PIN sign-in falls through to the cached offline
            // verifier. A genuine rejection (401/403/400) propagates so a wrong
            // PIN online is never silently retried offline.
            Ok(Err(e)) if offline_ok && net::is_connectivity_failure(&e) => offline(self),
            Ok(Err(e)) => Err(e),
            // Timed out → treat as offline.
            Err(_elapsed) if offline_ok => offline(self),
            Err(_elapsed) => Err(CoreError::Offline {
                detail: "sign-in timed out — check your connection".into(),
            }),
        }
    }

    /// List the org's active branches — for the device-setup picker. Requires a
    /// live (manager) session; online-only.
    pub async fn list_branches(&self) -> Result<Vec<session::BranchView>, CoreError> {
        use madar_api::apis::branches_api;
        let (org_id, _) = self.org_branch()?;
        let branches = branches_api::list_branches(
            &self.api.config(),
            branches_api::ListBranchesParams { org_id },
        )
        .await
        .map_err(net::map_api_error)?;
        Ok(branches
            .into_iter()
            .filter(|b| b.is_active)
            .map(|b| session::BranchView {
                id: b.id.to_string(),
                name: b.name,
                is_active: b.is_active,
                org_logo_url: b.org_logo_url.flatten().filter(|s| !s.is_empty()),
            })
            .collect())
    }

    /// Pull the branch-effective catalog (items + categories + addons + bundles +
    /// payment methods + discounts) and mirror the canonical JSON into the local
    /// store. Online-only; the offline reads (`list_*`) then serve this mirror.
    /// Atomic-ish: every stream is fetched before any is written, so a mid-pull
    /// failure leaves the previous mirror intact.
    pub async fn refresh_catalog(&self) -> Result<(), CoreError> {
        use madar_api::apis::{bundles_api, discounts_api, menu_api, payment_methods_api};
        use madar_api::models::BundleStatus;

        let (org_id, branch_id) = self.org_branch()?;

        // Menu items — full, branch-effective shape via raw GET (the typed
        // `list_menu_items` is `Vec<MenuItem>` and would drop sizes/slots).
        let mut q: Vec<(&str, String)> = vec![("org_id", org_id.clone()), ("full", "true".into())];
        if let Some(b) = &branch_id {
            q.push(("branch_id", b.clone()));
        }
        let menu_items_json = self.api.get_text("/menu-items", &q).await?;

        // Addons — the plain `/addon-items` array (NOT `/catalog`): it's
        // branch-effective AND embeds each addon's ingredients (the recipe
        // preview needs them). Raw GET because the embedded `quantity_used` is a
        // BigDecimal string the generated `AddonItem` (f64) can't decode.
        let mut aq: Vec<(&str, String)> = vec![("org_id", org_id.clone())];
        if let Some(b) = &branch_id {
            aq.push(("branch_id", b.clone()));
        }
        let addons_json = self.api.get_text("/addon-items", &aq).await?;

        let categories = menu_api::list_categories(
            &self.api.config(),
            menu_api::ListCategoriesParams { org_id: org_id.clone() },
        )
        .await
        .map_err(net::map_api_error)?;

        let bundles = bundles_api::list_bundles(
            &self.api.config(),
            bundles_api::ListBundlesParams {
                org_id: Some(org_id.clone()),
                status: Some(BundleStatus::Active),
                branch_id: branch_id.clone(),
                search: None,
                page: Some(1),
                per_page: Some(500),
                sort: None,
            },
        )
        .await
        .map_err(net::map_api_error)?;

        // Payment methods + discounts are CHECKOUT-time data — not needed to render
        // or FIRE the menu. A role that can read the menu but not these (a WAITER
        // fires tickets and never tenders, so it has no payment_methods:read grant)
        // must STILL get its catalog. So these are best-effort: a 403/failure leaves
        // them empty rather than aborting the whole catalog and blanking the menu.
        let payment_methods = payment_methods_api::list_payment_methods(&self.api.config())
            .await
            .unwrap_or_default();

        let discounts = discounts_api::list_discounts(
            &self.api.config(),
            discounts_api::ListDiscountsParams { org_id: org_id.clone() },
        )
        .await
        .unwrap_or_default();

        // All streams fetched OK → commit the mirror.
        self.store.kv_put(menu::K_MENU_ITEMS, &menu_items_json)?;
        self.store.kv_put(menu::K_CATEGORIES, &serde_json::to_string(&categories)?)?;
        self.store.kv_put(menu::K_ADDONS, &addons_json)?;
        self.store.kv_put(menu::K_BUNDLES, &serde_json::to_string(&bundles.data)?)?;
        self.store.kv_put(menu::K_PAYMENT_METHODS, &serde_json::to_string(&payment_methods)?)?;
        self.store.kv_put(menu::K_DISCOUNTS, &serde_json::to_string(&discounts)?)?;

        // A catalog sync also re-pulls the branch context (code, timezone, ORG LOGO
        // URL) + re-seeds the order-number base — the same get_branch persisted the
        // same durable kv way. So the manual "sync data" button refreshes a changed
        // logo/branch too, not just the menu. Best-effort: a branch-fetch hiccup
        // never fails the catalog commit above.
        let _ = self.cache_numbering_context().await;
        Ok(())
    }

    /// The org's logo URL for the current branch, from the durable kv mirror
    /// (`cache_numbering_context`/`refresh_catalog` persist it from `get_branch`).
    /// `None` until the first online branch fetch. The host reads this as the
    /// source of truth for the receipt logo, so it survives restarts + offline and
    /// refreshes on a manual data sync.
    pub fn org_logo_url(&self) -> Option<String> {
        self.store
            .kv_get(checkout::KEY_ORG_LOGO_URL)
            .ok()
            .flatten()
            .filter(|s| !s.is_empty())
    }

    /// Open a shift. Writes an optimistic local shift + queues an idempotent
    /// open-shift command (client UUID = shift PK), then drains best-effort. The
    /// shift is usable immediately, online or offline. Returns the current shift.
    pub async fn open_shift(
        &self,
        opening_cash_minor: i64,
        edit_reason: Option<String>,
    ) -> Result<shift::ShiftView, CoreError> {
        // SEQUENTIAL-ONLY: refuse to open a second shift while one is still open
        // on this device. Without this guard a second open silently overwrote the
        // cached shift (orphaning the first server-side and losing its close) —
        // the "shift open-or-not isn't robust" bug. The teller must close the
        // current shift first; the close may still be syncing, that's fine.
        if self.device_has_open_shift()? {
            return Err(CoreError::Validation {
                field: "shift".into(),
                detail: "A shift is already open on this device. Close it before opening a new one.".into(),
            });
        }
        let (branch_id, teller_id, teller_name) = {
            let g = self.session.read().unwrap_or_else(|e| e.into_inner());
            let s = g.as_ref().ok_or_else(|| CoreError::Unauthenticated {
                detail: "not signed in".into(),
            })?;
            let branch = s.snapshot.branch_id.clone().ok_or_else(|| CoreError::Validation {
                field: "branch_id".into(),
                detail: "session has no branch".into(),
            })?;
            (branch, s.snapshot.user_id.clone(), s.snapshot.display_name.clone())
        };
        let branch_uuid = uuid::Uuid::parse_str(&branch_id)
            .map_err(|_| CoreError::Validation { field: "branch_id".into(), detail: "bad uuid".into() })?;
        let teller_uuid = uuid::Uuid::parse_str(&teller_id)
            .map_err(|_| CoreError::Validation { field: "teller_id".into(), detail: "bad uuid".into() })?;
        // The device's bound till (drawer) comes from the core device config, NOT a
        // host param. `None` lets the backend pick the branch's default till.
        let till_uuid = match device::load(&self.store).till_id.filter(|s| !s.is_empty()) {
            Some(s) => Some(uuid::Uuid::parse_str(&s).map_err(|_| CoreError::Validation {
                field: "till_id".into(),
                detail: "bad till id".into(),
            })?),
            None => None,
        };
        let shift_id = uuid::Uuid::new_v4();
        let opened_at = self.corrected_now().fixed_offset();
        let opening_cash = opening_cash_minor as i32;
        // A non-empty discrepancy reason ⇒ the teller deviated from the carried-
        // over closing. The server re-derives this authoritatively; we mirror it
        // locally for display and pass the reason through.
        let edit_reason = edit_reason.filter(|r| !r.trim().is_empty());
        let was_edited = edit_reason.is_some();

        // Optimistic local shift — visible immediately on every read.
        let local = madar_api::models::Shift {
            branch_id: branch_uuid,
            id: shift_id,
            opened_at,
            opening_cash,
            opening_cash_was_edited: was_edited,
            status: "open".into(),
            teller_id: teller_uuid,
            teller_name,
            till_id: till_uuid.map(Some),
            ..Default::default()
        };
        shift::save(&self.store, &local)?;

        // Queue the durable command (idempotent on the client shift UUID).
        let request = madar_api::models::OpenShiftRequest {
            id: Some(Some(shift_id)),
            opened_at: Some(Some(opened_at)),
            opening_cash,
            edit_reason: edit_reason.map(Some),
            till_id: till_uuid.map(Some),
            ..Default::default()
        };
        let cmd = shift::OpenShiftCommand { branch_id, request };
        let (user_id, clock_offset_ms) = self.outbox_meta();
        // Sequential handover: if a prior shift's close is still queued, this open
        // DEPENDS on it. The branch must be confirmed free (the close fully drained)
        // before the open replays — otherwise the open races the still-open prior
        // shift and 409s ("a shift is already open for this branch"), dead-letters,
        // cascades its orders, and clears the local shift back to the open screen.
        // None when no close is queued (the prior shift closed online → branch free).
        let depends_on_seq = self.store.latest_unsynced_close_seq()?;
        self.store.enqueue(&store::NewOutboxOp {
            id: shift_id.to_string(),
            op_type: "open_shift".into(),
            idempotency_key: shift_id.to_string(),
            payload: serde_json::to_string(&cmd)?,
            event_at: opened_at.to_rfc3339(),
            depends_on_seq,
            user_id,
            clock_offset_ms,
            shift_id: Some(shift_id.to_string()),
        })?;

        // Best-effort: send now if online (offline just leaves it queued).
        let _ = self.drain_outbox().await;

        // Advertise this till's now-open shift to the LAN gate (if the relay is up).
        self.lan_sync_open_shift();

        shift::current(&self.store)?
            .ok_or_else(|| CoreError::Internal { detail: "shift not persisted".into() })
    }

    /// Close the current open shift: count the closing drawer cash + an optional
    /// note. Marks the shift closed locally (routing flips to open-shift now) and
    /// queues an idempotent `close_shift` command; works offline. Errors if there
    /// is no open shift.
    pub async fn close_shift(
        &self,
        closing_cash_minor: i64,
        cash_note: Option<String>,
    ) -> Result<(), CoreError> {
        let shift = shift::current(&self.store)?
            .filter(|s| s.is_open)
            .ok_or_else(|| CoreError::Validation { field: "shift".into(), detail: "no open shift".into() })?;

        let closed_at = self.corrected_now().fixed_offset();
        let mut request = madar_api::models::CloseShiftRequest::new(closing_cash_minor as i32);
        request.cash_note = Some(cash_note);
        request.closed_at = Some(Some(closed_at));

        // Optimistic: mark closed locally so routing flips to open-shift now,
        // and drop the in-progress cart (a closed shift sells nothing).
        shift::close_local(&self.store)?;
        cart::clear(&self.store)?;
        // Carry the declared closing into the NEXT shift's suggested opening, so
        // cash continuity holds even before this close syncs.
        shift::cache_suggested_opening_cash(&self.store, closing_cash_minor)?;

        // Queue the durable command. Keyed by `{shift_id}:close` so it doesn't
        // collide with the still-pending open_shift command (id == shift PK).
        let cmd = shift::CloseShiftCommand { shift_id: shift.id.clone(), request };
        let (user_id, clock_offset_ms) = self.outbox_meta();
        self.store.enqueue(&store::NewOutboxOp {
            id: format!("{}:close", shift.id),
            op_type: "close_shift".into(),
            idempotency_key: format!("{}:close", shift.id),
            payload: serde_json::to_string(&cmd)?,
            event_at: closed_at.to_rfc3339(),
            // Gate behind the shift's open (if still queued); the close-last
            // drain rule then also waits for every order/cash of this shift.
            depends_on_seq: self.store.live_seq_of(&shift.id)?,
            user_id,
            clock_offset_ms,
            shift_id: Some(shift.id.clone()),
        })?;

        // Best-effort: the FIFO drain runs the open + orders before the close,
        // so the close never races ahead of them.
        let _ = self.drain_outbox().await;

        // Stop advertising an open shift to the LAN gate (this till just closed).
        self.lan_sync_open_shift();
        Ok(())
    }

    /// The current shift's report — drives the close-shift system-cash +
    /// discrepancy. Online: the server report plus still-queued cash sales.
    /// Offline / on error: opening cash + queued cash (`from_server = false`).
    pub async fn shift_report(&self) -> Result<shift::ShiftReportView, CoreError> {
        use madar_api::apis::shifts_api;
        let shift = shift::current(&self.store)?
            .ok_or_else(|| CoreError::Validation { field: "shift".into(), detail: "no shift".into() })?;
        let queued_cash = checkout::queued_cash_total(&self.store)?;
        let online = self.current_session().map(|s| s.online).unwrap_or(false);
        if online {
            let res = shifts_api::get_shift_report(
                &self.api.config(),
                shifts_api::GetShiftReportParams { shift_id: shift.id.clone() },
            )
            .await;
            if let Ok(report) = res {
                return Ok(shift::report_view(&report, queued_cash));
            }
        }
        // Offline: reconstruct the drawer block from the still-queued movements.
        let teller = self.current_session().map(|s| s.display_name).unwrap_or_default();
        let movements: Vec<shift::ShiftReportCashLine> = self
            .store
            .list_active()?
            .into_iter()
            .filter(|i| i.op_type == "cash_movement" && i.shift_id.as_deref() == Some(shift.id.as_str()))
            .filter_map(|i| {
                serde_json::from_str::<shift::CashMovementCommand>(&i.payload).ok().map(|cmd| shift::ShiftReportCashLine {
                    amount_minor: cmd.request.amount as i64,
                    note: cmd.request.note,
                    moved_by_name: teller.clone(),
                    created_at: i.event_at.clone(),
                })
            })
            .collect();
        Ok(shift::offline_report_view(
            shift.opening_cash_minor,
            queued_cash,
            movements,
            shift.teller_name.clone(),
            shift.opened_at.clone(),
            chrono::Utc::now().to_rfc3339(),
        ))
    }

    /// Record a cash-drawer movement against the open shift — pay-IN when
    /// `amount_minor > 0`, pay-OUT when `< 0`. OFFLINE-FIRST: queued through the
    /// durable outbox (gated behind the shift's open) and idempotent on a minted
    /// `client_ref`, so a replay after a lost response never double-applies cash.
    pub async fn record_cash_movement(
        &self,
        amount_minor: i64,
        note: String,
    ) -> Result<shift::CashMovementView, CoreError> {
        let shift = shift::current(&self.store)?
            .filter(|s| s.is_open)
            .ok_or_else(|| CoreError::Validation { field: "shift".into(), detail: "no open shift".into() })?;

        // The client_ref IS the outbox id — stable across replays so the backend
        // dedups on its `client_ref` unique index.
        let client_ref = uuid::Uuid::new_v4();
        let created_at = self.corrected_now().fixed_offset();
        let mut request = madar_api::models::CashMovementRequest::new(amount_minor as i32, note.clone());
        request.client_ref = Some(Some(client_ref));
        request.created_at = Some(Some(created_at));
        let cmd = shift::CashMovementCommand { shift_id: shift.id.clone(), request };

        let (user_id, clock_offset_ms) = self.outbox_meta();
        self.store.enqueue(&store::NewOutboxOp {
            id: client_ref.to_string(),
            op_type: "cash_movement".into(),
            idempotency_key: client_ref.to_string(),
            payload: serde_json::to_string(&cmd)?,
            event_at: created_at.to_rfc3339(),
            depends_on_seq: self.store.live_seq_of(&shift.id)?,
            user_id,
            clock_offset_ms,
            shift_id: Some(shift.id.clone()),
        })?;
        // Best-effort send now; offline just leaves it queued.
        let _ = self.drain_outbox().await;

        // Optimistic view (the drawer moved regardless of sync state).
        let teller = self.current_session().map(|s| s.display_name).unwrap_or_default();
        Ok(shift::CashMovementView {
            id: client_ref.to_string(),
            amount_minor,
            note,
            moved_by_name: teller,
            created_at: created_at.to_rfc3339(),
        })
    }

    /// Cash movements for the open shift — server rows merged with still-queued
    /// (offline) ones, so the drawer view is complete with or without a connection.
    pub async fn list_cash_movements(&self) -> Result<Vec<shift::CashMovementView>, CoreError> {
        use madar_api::apis::shifts_api;
        let shift = shift::current(&self.store)?
            .ok_or_else(|| CoreError::Validation { field: "shift".into(), detail: "no shift".into() })?;

        // Queued (not-yet-synced) movements for this shift, parsed from the outbox.
        let teller = self.current_session().map(|s| s.display_name).unwrap_or_default();
        let queued: Vec<shift::CashMovementView> = self
            .store
            .list_active()?
            .into_iter()
            .filter(|i| i.op_type == "cash_movement" && i.shift_id.as_deref() == Some(shift.id.as_str()))
            .filter_map(|i| {
                serde_json::from_str::<shift::CashMovementCommand>(&i.payload).ok().map(|cmd| shift::CashMovementView {
                    id: i.id.clone(),
                    amount_minor: cmd.request.amount as i64,
                    note: cmd.request.note,
                    moved_by_name: teller.clone(),
                    created_at: i.event_at.clone(),
                })
            })
            .collect();

        // Server rows: live when online (cached write-through for offline), else the
        // last-synced snapshot. So the drawer shows ALL movements offline — both the
        // ones synced before the outage and the ones rung during it — not just queued.
        let key = format!("cache:cash:{}", shift.id);
        let mut server: Vec<shift::CashMovementView> =
            if self.current_session().map(|s| s.online).unwrap_or(false) {
                match shifts_api::list_cash_movements(
                    &self.api.config(),
                    shifts_api::ListCashMovementsParams { shift_id: shift.id.clone() },
                )
                .await
                {
                    Ok(list) => {
                        let views: Vec<_> = list.iter().map(shift::cash_movement_view).collect();
                        cache_views(&self.store, &key, &views);
                        views
                    }
                    Err(_) => cached_views(&self.store, &key),
                }
            } else {
                cached_views(&self.store, &key)
            };
        // Server first (chronological), then the still-queued tail. Dedup by id
        // so a movement that synced between enqueue and this read isn't doubled.
        let seen: std::collections::HashSet<String> = server.iter().map(|m| m.id.clone()).collect();
        server.extend(queued.into_iter().filter(|q| !seen.contains(&q.id)));
        Ok(server)
    }

    /// Past shifts for this branch, newest first (the history screen). Live when
    /// online (cached write-through), else the last-synced snapshot — so the past-
    /// shifts table still populates offline instead of erroring to an empty screen.
    pub async fn list_shifts(&self) -> Result<Vec<shift::ShiftSummaryView>, CoreError> {
        use madar_api::apis::shifts_api;
        let (_, branch_id) = self.org_branch()?;
        let branch = branch_id.unwrap_or_else(|| "00000000-0000-0000-0000-000000000000".into());
        const KEY: &str = "cache:shifts";
        let mut views: Vec<shift::ShiftSummaryView> =
            if self.current_session().map(|s| s.online).unwrap_or(false) {
                match shifts_api::list_shifts(
                    &self.api.config(),
                    shifts_api::ListShiftsParams { branch_id: branch, page: None, per_page: None },
                )
                .await
                {
                    Ok(paginated) => {
                        let v: Vec<_> = paginated.data.iter().map(shift::shift_summary_view).collect();
                        // Cache the SERVER truth (pre-overlay); the offline-close
                        // overlay is re-applied on every read from the queue below.
                        cache_views(&self.store, KEY, &v);
                        v
                    }
                    Err(_) => cached_views(&self.store, KEY),
                }
            } else {
                cached_views(&self.store, KEY)
            };

        // Overlay shifts CLOSED OFFLINE: the server snapshot still has them open
        // (the close is only queued), so without this a shift closed offline shows
        // as still-active in the list. Drops off automatically once the close syncs
        // (the queued op clears) and the server list reflects the closed shift.
        let overlay = shift::queued_close_overlay(&self.store);
        if !overlay.is_empty() {
            for v in views.iter_mut() {
                if let Some((closed_at, declared)) = overlay.get(&v.id) {
                    v.is_open = false;
                    v.status = "closed".into();
                    if v.closed_at.is_none() {
                        v.closed_at = closed_at.clone();
                    }
                    if v.closing_declared_minor.is_none() {
                        v.closing_declared_minor = Some(*declared);
                    }
                }
            }
        }

        // Add shifts opened OFFLINE that aren't on the server yet (the normal
        // offline workflow: open AND close a whole shift with no connection). They
        // live only in the outbox until they sync, so without this they'd be missing
        // from past shifts entirely. Dedup by id against the server list.
        let server_ids: std::collections::HashSet<String> = views.iter().map(|v| v.id.clone()).collect();
        for local in shift::local_shifts(&self.store) {
            if !server_ids.contains(&local.id) {
                views.push(local);
            }
        }
        // Newest-first by opened_at (the merge of server + local needs a re-sort).
        views.sort_by(|a, b| b.opened_at.cmp(&a.opened_at));
        Ok(views)
    }

    /// Place the current cart as an order: price it (client-authoritative),
    /// queue an idempotent `create_order` command, clear the cart, and try to
    /// send now. Works offline — the order stays queued and `queued_offline` is
    /// `true` on the receipt until it syncs. Errors if there's no open shift,
    /// the cart is empty, or the payment method is unknown.
    pub async fn checkout(
        &self,
        input: checkout::CheckoutInput,
    ) -> Result<checkout::ReceiptView, CoreError> {
        let (branch_id, tax_rate, teller_name) = {
            let g = self.session.read().unwrap_or_else(|e| e.into_inner());
            let s = g.as_ref().ok_or_else(|| CoreError::Unauthenticated {
                detail: "not signed in".into(),
            })?;
            let branch = s.snapshot.branch_id.clone().ok_or_else(|| CoreError::Validation {
                field: "branch_id".into(),
                detail: "session has no branch".into(),
            })?;
            (branch, s.snapshot.tax_rate, s.snapshot.display_name.clone())
        };
        let shift = shift::current(&self.store)?
            .filter(|s| s.is_open)
            .ok_or_else(|| CoreError::Validation { field: "shift".into(), detail: "no open shift".into() })?;

        let now = self.corrected_now().to_rfc3339();
        let prepared = checkout::prepare(
            &self.store,
            &self.current_locale(),
            &branch_id,
            &shift.id,
            &input,
            tax_rate,
            now,
        )?;

        // Queue the durable command. Idempotent on the client order UUID (both
        // the outbox `id` and the in-body `idempotency_key`), gated behind the
        // shift's open if that hasn't synced yet.
        let (user_id, clock_offset_ms) = self.outbox_meta();
        self.store.enqueue(&store::NewOutboxOp {
            id: prepared.order_id.to_string(),
            op_type: "create_order".into(),
            idempotency_key: prepared.order_id.to_string(),
            payload: serde_json::to_string(&prepared.command)?,
            event_at: prepared.event_at.clone(),
            depends_on_seq: self.store.live_seq_of(&shift.id)?,
            user_id,
            clock_offset_ms,
            shift_id: Some(shift.id.clone()),
        })?;
        // The sale is committed locally; the cart is now spent.
        cart::clear(&self.store)?;

        // Best-effort: send now if online (offline leaves it queued).
        let _ = self.drain_outbox().await;

        // If the order is no longer pending, the drain sent it.
        let order_id = prepared.order_id.to_string();
        let still_pending = self.store.pending()?.iter().any(|i| i.id == order_id);
        let mut receipt = prepared.receipt;
        receipt.queued_offline = still_pending;
        receipt.teller_name = Some(teller_name).filter(|s| !s.trim().is_empty());
        Ok(receipt)
    }

    /// Force a sync now — drains the outbox. Cancellable/idempotent.
    pub async fn sync_now(&self) -> Result<(), CoreError> {
        // An explicit sync clears the offline (no-count) backoff so a backlog built
        // during an outage flushes NOW, not after the ~15s network-retry window.
        let _ = self.store.clear_network_backoff();
        self.drain_outbox().await
    }

    /// Requeue every dead command (clearing its error) and try to send now.
    /// Best-effort — offline just leaves them pending again.
    pub async fn retry_outbox(&self) -> Result<(), CoreError> {
        self.store.requeue_dead()?;
        self.drain_outbox().await
    }

    /// The connectivity heartbeat: ping the backend, update the live `online`
    /// flag + clock skew, and drain the outbox on success. The host calls this on
    /// foreground + on a timer so the offline/clock-skew banners and the sync
    /// chip reflect reality without waiting for the next deliberate action.
    /// Returns the new online state.
    pub async fn refresh_connectivity(&self) -> bool {
        match self.api.ping().await {
            Ok(skew) => {
                if let Some(s) = skew {
                    self.clock_skew_secs.store(s, std::sync::atomic::Ordering::Relaxed);
                    // Persist so a later cold offline boot stamps corrected times.
                    let _ = self.store.kv_put("clock_skew_secs", &s.to_string());
                }
                if let Some(sess) = self.session.write().unwrap_or_else(|e| e.into_inner()).as_mut() {
                    sess.snapshot.online = true;
                }
                // Connectivity is CONFIRMED — un-gate the offline backlog so it
                // drains on this pass instead of waiting out the network window.
                let _ = self.store.clear_network_backoff();
                let _ = self.drain_outbox().await; // best-effort
                true
            }
            Err(_) => {
                if let Some(sess) = self.session.write().unwrap_or_else(|e| e.into_inner()).as_mut() {
                    sess.snapshot.online = false;
                }
                false
            }
        }
    }

    /// The current shift's orders — the still-queued sales (from the outbox,
    /// shown first, always available offline) plus the server's synced orders
    /// when online (best-effort). Errors if there's no current shift.
    pub async fn list_shift_orders(&self) -> Result<Vec<orders::OrderSummaryView>, CoreError> {
        use madar_api::apis::orders_api;
        let shift = shift::current(&self.store)?
            .ok_or_else(|| CoreError::Validation { field: "shift".into(), detail: "no shift".into() })?;
        let (branch_id, online) = {
            let g = self.session.read().unwrap_or_else(|e| e.into_inner());
            let s = g.as_ref().ok_or_else(|| CoreError::Unauthenticated { detail: "not signed in".into() })?;
            let b = s.snapshot.branch_id.clone().ok_or_else(|| CoreError::Validation {
                field: "branch_id".into(),
                detail: "session has no branch".into(),
            })?;
            (b, s.snapshot.online)
        };

        // Always show the still-queued sales (offline-safe).
        let mut all = orders::queued(&self.store, &shift.id)?;

        // The shift's SYNCED orders: live when online (cached write-through), else
        // the last-synced snapshot. So going offline keeps the orders already synced
        // this shift visible — not just the ones rung during the outage.
        let key = format!("cache:shift_orders:{}", shift.id);
        let mut server: Vec<orders::OrderSummaryView> = if online {
            let params = orders_api::ListOrdersParams {
                branch_id: Some(branch_id),
                shift_id: Some(shift.id.clone()),
                updated_after: None,
                page: None,
                per_page: Some(200),
                teller_name: None,
                payment_method: None,
                status: None,
                from: None,
                to: None,
                order_type: None,
                channel: None,
                include_items: Some(true),
            };
            match orders_api::list_orders(&self.api.config(), params).await {
                Ok(page) => {
                    // Preload each full order for OFFLINE detail + reprint.
                    for o in &page.data {
                        cache_views(&self.store, &format!("cache:order:{}", o.id), std::slice::from_ref(o));
                    }
                    let views: Vec<_> = page.data.iter().map(|o| orders::from_server(o)).collect();
                    cache_views(&self.store, &key, &views);
                    views
                }
                Err(_) => cached_views(&self.store, &key),
            }
        } else {
            cached_views(&self.store, &key)
        };
        // Overlay an optimistic "voided" status for orders with a queued void command
        // (the void hasn't synced yet) — applies to fresh OR cached server rows.
        let voiding = orders::pending_void_ids(&self.store)?;
        for v in server.iter_mut() {
            if voiding.contains(&v.id) {
                v.status = "voided".into();
            }
        }
        all.extend(server);
        Ok(all)
    }

    /// Fetch a synced order's full detail (lines + modifiers) — the expanded
    /// history row. Offline-durable for any order seen online (cached).
    pub async fn order_detail(&self, order_id: String) -> Result<orders::OrderDetailView, CoreError> {
        let o = self.get_order_or_cache(&order_id).await?;
        Ok(orders::order_detail_view(&o))
    }

    /// Re-render a synced order as a receipt for reprint — same ESC/POS path as a
    /// fresh receipt. Offline-durable for any order seen online (cached).
    pub async fn render_order_receipt(
        &self,
        order_id: String,
        store_name: String,
        currency: String,
        width: u32,
        brand: receipt::PrinterBrand,
    ) -> Result<Vec<u8>, CoreError> {
        let o = self.get_order_or_cache(&order_id).await?;
        let receipt = orders::order_to_receipt(&o, &self.current_locale());
        Ok(self.render_receipt(receipt, store_name, currency, width, brand))
    }

    /// Project a synced order into a ReceiptView (no bytes) — for an on-screen
    /// receipt preview before reprinting. Offline-durable for any order seen online.
    pub async fn order_receipt_view(&self, order_id: String) -> Result<checkout::ReceiptView, CoreError> {
        let o = self.get_order_or_cache(&order_id).await?;
        Ok(orders::order_to_receipt(&o, &self.current_locale()))
    }

    /// A PAST shift's synced orders (history-screen expansion). Live when online
    /// (cached write-through, same key as the current-shift list), else the last-
    /// synced snapshot — so an expanded past shift keeps its orders offline.
    pub async fn list_orders_for_shift(
        &self,
        shift_id: String,
    ) -> Result<Vec<orders::OrderSummaryView>, CoreError> {
        use madar_api::apis::orders_api;
        let (branch_id, online) = {
            let g = self.session.read().unwrap_or_else(|e| e.into_inner());
            let s = g.as_ref().ok_or_else(|| CoreError::Unauthenticated { detail: "not signed in".into() })?;
            let b = s.snapshot.branch_id.clone().ok_or_else(|| CoreError::Validation {
                field: "branch_id".into(),
                detail: "session has no branch".into(),
            })?;
            (b, s.snapshot.online)
        };
        let key = format!("cache:shift_orders:{shift_id}");
        // Queued (offline-rung) orders for THIS shift first — a shift opened AND
        // sold on entirely offline has ALL its orders here, not on the server, so
        // without this its history would be empty offline.
        let mut all = orders::queued(&self.store, &shift_id)?;
        let mut server: Vec<orders::OrderSummaryView> = if online {
            let params = orders_api::ListOrdersParams {
                branch_id: Some(branch_id),
                shift_id: Some(shift_id.clone()),
                updated_after: None,
                page: None,
                per_page: Some(200),
                teller_name: None,
                payment_method: None,
                status: None,
                from: None,
                to: None,
                order_type: None,
                channel: None,
                include_items: Some(true),
            };
            match orders_api::list_orders(&self.api.config(), params).await {
                Ok(page) => {
                    // Preload each full order for OFFLINE detail + reprint.
                    for o in &page.data {
                        cache_views(&self.store, &format!("cache:order:{}", o.id), std::slice::from_ref(o));
                    }
                    let views: Vec<_> = page.data.iter().map(|o| orders::from_server(o)).collect();
                    cache_views(&self.store, &key, &views);
                    views
                }
                Err(_) => cached_views(&self.store, &key),
            }
        } else {
            cached_views(&self.store, &key)
        };
        // Optimistic "voided" overlay for orders with a queued void (fresh OR cached).
        let voiding = orders::pending_void_ids(&self.store)?;
        for v in server.iter_mut() {
            if voiding.contains(&v.id) {
                v.status = "voided".into();
            }
        }
        all.extend(server);
        Ok(all)
    }

    /// Search the branch's orders ACROSS shifts (history lookup) with optional
    /// filters (status / teller / payment method / from-to dates) + pagination
    /// (50/page, 1-based). Online-only — the shift-scoped list is the offline path,
    /// so a Server/Network error surfaces rather than returning a stale snapshot.
    pub async fn search_orders(
        &self,
        status: Option<String>,
        teller_name: Option<String>,
        payment_method: Option<String>,
        from: Option<String>,
        to: Option<String>,
        page: u32,
    ) -> Result<orders::OrderSearchPage, CoreError> {
        use madar_api::apis::orders_api;
        let branch_id = {
            let g = self.session.read().unwrap_or_else(|e| e.into_inner());
            let s = g.as_ref().ok_or_else(|| CoreError::Unauthenticated { detail: "not signed in".into() })?;
            s.snapshot.branch_id.clone().ok_or_else(|| CoreError::Validation {
                field: "branch_id".into(),
                detail: "session has no branch".into(),
            })?
        };
        let blank = |s: Option<String>| s.filter(|x| !x.is_empty());
        let date = |s: Option<String>| {
            s.filter(|x| !x.is_empty()).and_then(|x| chrono::DateTime::parse_from_rfc3339(&x).ok())
        };
        let per_page = 50i64;
        let params = orders_api::ListOrdersParams {
            branch_id: Some(branch_id),
            shift_id: None,
            updated_after: None,
            page: Some(page.max(1) as i64),
            per_page: Some(per_page),
            teller_name: blank(teller_name),
            payment_method: blank(payment_method),
            status: blank(status),
            from: date(from),
            to: date(to),
            order_type: None,
            channel: None,
            include_items: Some(false),
        };
        let resp = orders_api::list_orders(&self.api.config(), params).await.map_err(net::map_api_error)?;
        let orders: Vec<_> = resp.data.iter().map(orders::from_server).collect();
        let total = resp.total.max(0) as u32;
        let has_more = resp.page * resp.per_page < resp.total;
        Ok(orders::OrderSearchPage { orders, page: page.max(1), total, has_more })
    }

    /// A PAST shift's Z-report (history-screen reprint). Live when online (cached
    /// write-through), else the cached report; and for a shift opened+closed
    /// entirely OFFLINE — which never had a server report — reconstructed from the
    /// local opening cash + that shift's queued cash sales + movements.
    pub async fn shift_report_for(&self, shift_id: String) -> Result<shift::ShiftReportView, CoreError> {
        use madar_api::apis::shifts_api;
        let key = format!("cache:shift_report:{shift_id}");
        if self.current_session().map(|s| s.online).unwrap_or(false) {
            if let Ok(report) = shifts_api::get_shift_report(
                &self.api.config(),
                shifts_api::GetShiftReportParams { shift_id: shift_id.clone() },
            )
            .await
            {
                cache_views(&self.store, &key, std::slice::from_ref(&report));
                return Ok(shift::report_view(&report, 0));
            }
        }
        // Offline / fetch failed: the last-synced report if we have one…
        if let Some(report) =
            cached_views::<madar_api::models::ShiftReportResponse>(&self.store, &key).into_iter().next()
        {
            return Ok(shift::report_view(&report, 0));
        }
        // …otherwise an offline-only shift: reconstruct the drawer from local state.
        self.offline_report_for(&shift_id)
    }

    /// Reconstruct a shift's Z-report from purely LOCAL state (opening cash + that
    /// shift's queued cash sales + movements) — for a shift opened+closed offline
    /// that the server has never seen. Mirrors the current-shift `shift_report`
    /// offline branch, scoped to an arbitrary shift id.
    fn offline_report_for(&self, shift_id: &str) -> Result<shift::ShiftReportView, CoreError> {
        // Resolve opening cash + teller + opened-at from the current shift if it
        // matches, else from the reconstructed local-shift list (distinct types,
        // so pull the three values out of each rather than unifying the objects).
        let (opening, teller_name, opened_at) =
            if let Some(s) = shift::current(&self.store)?.filter(|s| s.id == shift_id) {
                (s.opening_cash_minor, Some(s.teller_name), s.opened_at)
            } else if let Some(s) = shift::local_shifts(&self.store).into_iter().find(|s| s.id == shift_id) {
                (s.opening_cash_minor, s.teller_name, s.opened_at)
            } else {
                (0, None, String::new())
            };
        let teller = teller_name
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| self.current_session().map(|s| s.display_name).unwrap_or_default());
        let queued_cash = checkout::queued_cash_total_for(&self.store, shift_id)?;
        let movements: Vec<shift::ShiftReportCashLine> = self
            .store
            .list_active()?
            .into_iter()
            .filter(|i| i.op_type == "cash_movement" && i.shift_id.as_deref() == Some(shift_id))
            .filter_map(|i| {
                serde_json::from_str::<shift::CashMovementCommand>(&i.payload).ok().map(|cmd| {
                    shift::ShiftReportCashLine {
                        amount_minor: cmd.request.amount as i64,
                        note: cmd.request.note,
                        moved_by_name: teller.clone(),
                        created_at: i.event_at.clone(),
                    }
                })
            })
            .collect();
        Ok(shift::offline_report_view(
            opening,
            queued_cash,
            movements,
            teller,
            opened_at,
            chrono::Utc::now().to_rfc3339(),
        ))
    }

    /// Void a synced order (mistake/refund). Queues an idempotent `void_order`
    /// command keyed `{order_id}:void` and tries to send now; works offline.
    /// History reflects it immediately via the pending-void overlay. Only synced
    /// orders (with a server id) can be voided — a queued order isn't on the
    /// server yet.
    pub async fn void_order(
        &self,
        order_id: String,
        reason: String,
        note: Option<String>,
        restore_inventory: bool,
    ) -> Result<(), CoreError> {
        // Must be signed in (the replay needs a token).
        if !self.is_authenticated() {
            return Err(CoreError::Unauthenticated { detail: "not signed in".into() });
        }
        let voided_at = self.corrected_now().fixed_offset();
        let mut request = madar_api::models::VoidOrderRequest::new(reason);
        request.note = Some(note);
        request.restore_inventory = Some(Some(restore_inventory));
        request.voided_at = Some(Some(voided_at));

        // The void targets a SYNCED order (server id), so it has no queued
        // create_order to gate behind. If that order was created offline and is
        // still queued, the void depends on it (and the drain's 404-on-void
        // handling protects against the order never landing).
        let cmd = orders::VoidOrderCommand { order_id: order_id.clone(), request };
        let (user_id, clock_offset_ms) = self.outbox_meta();
        self.store.enqueue(&store::NewOutboxOp {
            id: format!("{order_id}:void"),
            op_type: "void_order".into(),
            idempotency_key: format!("{order_id}:void"),
            payload: serde_json::to_string(&cmd)?,
            event_at: voided_at.to_rfc3339(),
            depends_on_seq: self.store.live_seq_of(&order_id)?,
            user_id,
            clock_offset_ms,
            // Stamp the void with the current shift so the close-last gate
            // (`has_live_shift_writes`, which lists `void_order`) holds this
            // shift's close back until the void has synced. Without it the close
            // could replay before the void and freeze the Z-report's
            // closing_cash_system too high (the voided sale still counted).
            shift_id: shift::current(&self.store)?.map(|s| s.id),
        })?;
        let _ = self.drain_outbox().await;
        Ok(())
    }

    /// Reconcile the device's shift with the server (online). Caches the server's
    /// open shift, or CLEARS the local cache when the server reports none — e.g.
    /// a dashboard force-close, or a shift opened on another device. The server
    /// is the source of truth when online; call this on login and on app resume.
    pub async fn refresh_shift(&self) -> Result<Option<shift::ShiftView>, CoreError> {
        use madar_api::apis::shifts_api;
        let till_id = device::load(&self.store).till_id;
        let (branch_id, signed_in_teller, role) = {
            let g = self.session.read().unwrap_or_else(|e| e.into_inner());
            let s = g.as_ref().ok_or_else(|| CoreError::Unauthenticated {
                detail: "not signed in".into(),
            })?;
            let branch = s.snapshot.branch_id.clone().ok_or_else(|| CoreError::Validation {
                field: "branch_id".into(),
                detail: "session has no branch".into(),
            })?;
            (branch, s.snapshot.user_id.clone(), s.snapshot.role.clone())
        };
        // Only tellers hold a shift. Waiters/kitchen devices never open one, so
        // skip `/shifts/current` for them — they lack `shifts:read`, and the call
        // would 403 (harmlessly swallowed by the host, but noisy in the logs). They
        // simply have no shift.
        if role != "teller" {
            return Ok(None);
        }
        let prefill = shifts_api::get_current_shift(
            &self.api.config(),
            // The device's bound till scopes the carryover suggestion; `None` =
            // the branch default-till (single-till behavior).
            shifts_api::GetCurrentShiftParams { branch_id, till_id: till_id.filter(|s| !s.is_empty()) },
        )
        .await
        .map_err(net::map_api_error)?;

        // Refresh the carried-over opening-cash suggestion from the server's
        // prefill (the last *synced* declared closing). Only overwrite with a
        // positive value, so a stale server 0 can't clobber a fresher local
        // close that hasn't synced yet.
        if prefill.suggested_opening_cash > 0 {
            shift::cache_suggested_opening_cash(&self.store, prefill.suggested_opening_cash as i64)?;
        }

        // The server's "no open shift" is only authoritative once our own
        // open_shift command has actually reached it. While it's still queued,
        // the optimistic local shift stands — clearing it here is what bounced
        // the teller straight back to the open-shift screen.
        let open_pending = self.shift_command_pending("open_shift")?;
        let close_pending = self.shift_command_pending("close_shift")?;
        match shift::reconcile(&prefill, &signed_in_teller, open_pending, close_pending) {
            shift::ShiftReconcile::Adopt(server_shift) => {
                // Recover orphaned offline sales. If this teller has queued/dead ops
                // on a shift the device opened optimistically OFFLINE that never
                // became real server-side (the optimistic open conflicted on the
                // branch and dead-lettered, cascading its orders), those sales
                // belong on the teller's REAL open shift — the one we're adopting.
                // Re-point them onto it and requeue the dead ones so they sync,
                // instead of stranding the sales forever.
                let server_id = server_shift.id.to_string();
                let teller_id = server_shift.teller_id.to_string();
                let mut remapped = 0u32;
                for orphan in self.store.orphan_open_shift_ids(&teller_id, &server_id)? {
                    remapped += self.store.remap_shift(&orphan, &server_id)?;
                }
                let requeued = if remapped > 0 {
                    self.store.requeue_dead_for_shift(&server_id)?
                } else {
                    0
                };
                shift::save(&self.store, &server_shift)?;
                if remapped > 0 {
                    self.push_diag(
                        "info",
                        format!("recovered {remapped} queued op(s) ({requeued} re-tried) onto the active shift after an offline shift conflict"),
                    );
                    // Flush the re-pointed sales now (single-flight-guarded).
                    let _ = self.drain_outbox().await;
                }
                Ok(Some(shift::view_from(&server_shift)))
            }
            shift::ShiftReconcile::KeepLocal => shift::current(&self.store),
            shift::ShiftReconcile::Clear => {
                shift::clear(&self.store)?;
                Ok(None)
            }
        }
    }

    /// Print pre-rendered ESC/POS bytes to the DEVICE's configured printer (from the
    /// core device config — the host passes no host:port). Errors if no printer is
    /// bound. Thin wrapper over `send_to_printer`; the device binding is the source
    /// of truth so the hosts hold no printer state.
    pub async fn print_to_device(&self, bytes: Vec<u8>) -> Result<(), CoreError> {
        let cfg = device::load(&self.store);
        let host = cfg.printer_host.filter(|s| !s.is_empty()).ok_or_else(|| CoreError::Validation {
            field: "printer".into(),
            detail: "no printer configured for this device".into(),
        })?;
        let port = cfg.printer_port.unwrap_or(9100);
        self.send_to_printer(host, port, bytes).await
    }

    /// Best-effort raw-TCP send of pre-rendered ESC/POS bytes to a network
    /// (JetDirect / port 9100) thermal printer. Opens a short-lived socket,
    /// writes, flushes. Errors map to `Transient` so the host can offer a retry.
    ///
    /// NOTE: unverifiable here without hardware — the rendered bytes are the
    /// tested contract (`receipt` module); delivery is the host's to confirm.
    pub async fn send_to_printer(&self, host: String, port: u16, bytes: Vec<u8>) -> Result<(), CoreError> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::time::{sleep, timeout, Duration};
        let addr = format!("{host}:{port}");

        // Connect with a short retry. The auto-print fires the instant checkout
        // finishes, when the FIRST connect can transiently fail — the printer is
        // still finishing the previous job (single-session #9100), the ARP entry is
        // cold, or the post-sale backend sync is saturating the link. A manual
        // reprint a moment later always succeeds, so a couple of quick retries make
        // the automatic print as reliable as the manual one. Only the CONNECT is
        // retried; the write is never replayed, so a partial send can't double-print.
        let mut stream = {
            let mut last: Option<CoreError> = None;
            let mut ok = None;
            for attempt in 0..3u32 {
                if attempt > 0 {
                    sleep(Duration::from_millis(300)).await;
                }
                match timeout(Duration::from_secs(4), tokio::net::TcpStream::connect(&addr)).await {
                    Ok(Ok(s)) => {
                        ok = Some(s);
                        break;
                    }
                    Ok(Err(e)) => last = Some(CoreError::Transient { detail: format!("printer connect: {e}") }),
                    Err(_) => last = Some(CoreError::Transient { detail: format!("printer timeout: {addr}") }),
                }
            }
            match ok {
                Some(s) => s,
                None => return Err(last.unwrap()),
            }
        };

        // Star printers (TSP143IIILAN and friends) continuously push ASB status
        // bytes back to the host. If we only write and never read, the printer's
        // TX buffer fills, which back-pressures its RX and stalls the job *before
        // it prints* — a silent "connection succeeds, nothing prints" failure,
        // confirmed on hardware. So we drain (and discard) the status channel
        // concurrently with the write. This is what the Star SDK does, and it
        // works whether or not the printer's "#9100 Multi Session" option is on.
        let (mut rd, mut wr) = stream.split();
        let write_job = async {
            wr.write_all(&bytes).await?;
            wr.flush().await
        };
        let drain = async {
            let mut buf = [0u8; 512];
            // Read until the printer stops sending or closes; bytes are discarded.
            while let Ok(n) = rd.read(&mut buf).await {
                if n == 0 {
                    break;
                }
            }
        };
        tokio::pin!(drain);
        // Completing the write is success; keep draining throughout so the job is
        // never starved. (A printer that closes first hits the drain branch.)
        tokio::select! {
            w = write_job => w.map_err(|e| CoreError::Transient {
                detail: format!("printer write: {e}"),
            })?,
            _ = &mut drain => {}
        }
        // The write completing means every byte was accepted by the printer (the
        // concurrent drain kept its RX from stalling), so the job will print from
        // its buffer regardless of when we close. A brief final drain consumes any
        // trailing status without making the UI wait — the old 2s grace was the
        // ~3s "loading" the teller saw on every sale.
        let _ = timeout(Duration::from_millis(200), drain).await;
        Ok(())
    }
}

// ── delivery-order management (online; teller works the live branch queue) ───
impl MadarCore {
    /// The signed-in session's branch id, or a validation error.
    fn session_branch_id(&self) -> Result<String, CoreError> {
        let g = self.session.read().unwrap_or_else(|e| e.into_inner());
        let s = g.as_ref().ok_or_else(|| CoreError::Unauthenticated { detail: "not signed in".into() })?;
        s.snapshot.branch_id.clone().ok_or_else(|| CoreError::Validation {
            field: "branch_id".into(),
            detail: "session has no branch".into(),
        })
    }
}

// ── waiter open tickets (fire-now-pay-later via the outbox) ───────────────────
#[uniffi::export(async_runtime = "tokio")]
impl MadarCore {
    /// FIRE the current cart as a new dine-in open ticket (round 1). Prices the
    /// cart client-authoritatively (same engine as checkout), enqueues the durable
    /// fire op (offline-first), clears the cart, and best-effort drains. Returns a
    /// slim "sent to kitchen" confirmation — NOT a money receipt. The branch must
    /// be operating (the server enforces an open till at fire time).
    pub async fn fire_ticket(
        &self,
        table_id: Option<String>,
        customer_name: Option<String>,
        notes: Option<String>,
        guest_count: Option<i32>,
    ) -> Result<tickets::TicketFiredView, CoreError> {
        let branch_id = self.session_branch_id()?;
        let branch_uuid = uuid::Uuid::parse_str(&branch_id)
            .map_err(|_| CoreError::Validation { field: "branch_id".into(), detail: "bad branch id".into() })?;
        let lines = cart::lines(&self.store)?;
        if lines.is_empty() {
            return Err(CoreError::Validation { field: "cart".into(), detail: "cart is empty".into() });
        }
        let items = checkout::lines_to_wire_items(&lines);
        let ticket_id = uuid::Uuid::new_v4();
        let round_id = uuid::Uuid::new_v4();
        let table_uuid = table_id.as_deref().and_then(|s| uuid::Uuid::parse_str(s).ok());
        let request = tickets::build_fire_request(
            branch_uuid, items, ticket_id, round_id, table_uuid, customer_name, notes, guest_count,
        );
        let cmd = tickets::FireTicketCommand { ticket_id: ticket_id.to_string(), request };

        let (user_id, clock_offset_ms) = self.outbox_meta();
        let teller = user_id.clone();
        self.store.enqueue(&store::NewOutboxOp {
            id: ticket_id.to_string(),
            op_type: "open_ticket".into(),
            idempotency_key: ticket_id.to_string(),
            payload: serde_json::to_string(&cmd)?,
            event_at: self.corrected_now().to_rfc3339(),
            depends_on_seq: None, // a ticket floats free of any shift/till
            user_id,
            clock_offset_ms,
            shift_id: None, // the waiter holds no shift
        })?;
        cart::clear(&self.store)?;
        // Instant LAN delivery → the KDS sees the fire NOW. `data` is a projection of
        // the ticket with the SAME derived ids the server will mint (so it dedups on
        // reconnect); `replay_op` lets a kitchen peer mirror it for durability. The
        // outbox stays the truth.
        let projection = kds::build_fire_projection(
            &round_id.to_string(), &lines, None, 1, self.corrected_now().to_rfc3339(),
        );
        let data = projection
            .as_ref()
            .and_then(|p| serde_json::to_string(p).ok())
            .unwrap_or_else(|| "{}".into());
        let envelope = serde_json::json!({
            "op": "fire_open_ticket", "teller_id": teller, "request": cmd.request
        })
        .to_string();
        self.lan_publish("kitchen", "kitchen.fired", data, Some(envelope)).await;
        let _ = self.drain_outbox().await;

        let tid = ticket_id.to_string();
        let queued_offline = self.store.pending()?.iter().any(|i| i.id == tid);
        Ok(tickets::TicketFiredView { ticket_id: tid, ticket_ref: None, queued_offline })
    }

    /// Add a ROUND of the current cart to an existing open ticket. Same offline-first
    /// path as `fire_ticket`; gated behind the original fire if it hasn't synced.
    pub async fn add_ticket_round(
        &self,
        ticket_id: String,
    ) -> Result<tickets::TicketFiredView, CoreError> {
        let lines = cart::lines(&self.store)?;
        if lines.is_empty() {
            return Err(CoreError::Validation { field: "cart".into(), detail: "cart is empty".into() });
        }
        let items = checkout::lines_to_wire_items(&lines);
        let round_id = uuid::Uuid::new_v4();
        let request = tickets::build_round_request(items, round_id);
        let cmd = tickets::AddRoundCommand {
            ticket_id: ticket_id.clone(),
            round_id: round_id.to_string(),
            request,
        };
        let (user_id, clock_offset_ms) = self.outbox_meta();
        let teller = user_id.clone();
        self.store.enqueue(&store::NewOutboxOp {
            id: round_id.to_string(),
            op_type: "ticket_add_round".into(),
            idempotency_key: round_id.to_string(),
            payload: serde_json::to_string(&cmd)?,
            event_at: self.corrected_now().to_rfc3339(),
            // The round can't land before the ticket exists — gate on the queued fire.
            depends_on_seq: self.store.live_seq_of(&ticket_id)?,
            user_id,
            clock_offset_ms,
            shift_id: None,
        })?;
        cart::clear(&self.store)?;
        // Instant LAN delivery of the new round — its own kitchen ticket (derived
        // from THIS round's id), projected for offline visibility + a mirror envelope.
        let projection = kds::build_fire_projection(
            &round_id.to_string(), &lines, None, 0, self.corrected_now().to_rfc3339(),
        );
        let data = projection
            .as_ref()
            .and_then(|p| serde_json::to_string(p).ok())
            .unwrap_or_else(|| "{}".into());
        let envelope = serde_json::json!({
            "op": "add_ticket_round", "teller_id": teller, "ticket_id": ticket_id, "request": cmd.request
        })
        .to_string();
        self.lan_publish("kitchen", "kitchen.fired", data, Some(envelope)).await;
        let _ = self.drain_outbox().await;
        let rid = round_id.to_string();
        let queued_offline = self.store.pending()?.iter().any(|i| i.id == rid);
        Ok(tickets::TicketFiredView { ticket_id, ticket_ref: None, queued_offline })
    }

    /// VOID an open ticket (and pull its kitchen tickets off the KDS). Offline-first.
    pub async fn void_ticket(&self, ticket_id: String, reason: Option<String>) -> Result<bool, CoreError> {
        let mut request = madar_api::models::VoidOpenTicketRequest::new();
        request.reason = reason.filter(|s| !s.trim().is_empty()).map(Some);
        let cmd = tickets::VoidTicketCommand { ticket_id: ticket_id.clone(), request };
        let (user_id, clock_offset_ms) = self.outbox_meta();
        let op_id = format!("{ticket_id}:void");
        self.store.enqueue(&store::NewOutboxOp {
            id: op_id.clone(),
            op_type: "void_ticket".into(),
            idempotency_key: op_id.clone(),
            payload: serde_json::to_string(&cmd)?,
            event_at: self.corrected_now().to_rfc3339(),
            depends_on_seq: self.store.live_seq_of(&ticket_id)?,
            user_id,
            clock_offset_ms,
            shift_id: None,
        })?;
        let _ = self.drain_outbox().await;
        Ok(self.store.pending()?.iter().any(|i| i.id == op_id))
    }

    /// SETTLE an open ticket into a paid order in the cashier's shift (a till
    /// action). Offline-first: the order is materialized server-side at replay,
    /// deduped on the ticket id. Returns true when still queued (offline). The
    /// cashier's settle-time discount/tip override the ticket's own.
    #[allow(clippy::too_many_arguments)]
    pub async fn settle_ticket(
        &self,
        ticket_id: String,
        shift_id: String,
        // The host passes payment-method IDS (like checkout); the core resolves the
        // raw method NAME the backend validates against.
        payment_method_id: String,
        amount_tendered_minor: Option<i64>,
        tip_minor: Option<i64>,
        tip_payment_method_id: Option<String>,
        discount_id: Option<String>,
        discount_type: Option<String>,
        discount_value: Option<i32>,
    ) -> Result<bool, CoreError> {
        let shift_uuid = uuid::Uuid::parse_str(&shift_id)
            .map_err(|_| CoreError::Validation { field: "shift_id".into(), detail: "bad shift id".into() })?;
        let payment_method = checkout::raw_payment_method(&self.store, &payment_method_id)?
            .map(|p| p.name)
            .ok_or_else(|| CoreError::Validation { field: "payment_method".into(), detail: "unknown payment method".into() })?;
        let tip_method = tip_payment_method_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .and_then(|id| checkout::raw_payment_method(&self.store, id).ok().flatten())
            .map(|p| p.name);
        let mut request = madar_api::models::SettleOpenTicketRequest::new(payment_method, shift_uuid);
        request.amount_tendered = amount_tendered_minor.map(|v| Some(v as i32));
        request.tip_amount = tip_minor.filter(|v| *v > 0).map(|v| Some(v as i32));
        request.tip_payment_method = tip_method.map(Some);
        request.discount_id = discount_id.as_deref().and_then(|s| uuid::Uuid::parse_str(s).ok()).map(Some);
        request.discount_type = discount_type.filter(|s| !s.trim().is_empty()).map(Some);
        request.discount_value = discount_value.map(Some);
        let cmd = tickets::SettleTicketCommand { ticket_id: ticket_id.clone(), request };

        let (user_id, clock_offset_ms) = self.outbox_meta();
        let op_id = format!("{ticket_id}:settle");
        self.store.enqueue(&store::NewOutboxOp {
            id: op_id.clone(),
            op_type: "settle_open_ticket".into(),
            idempotency_key: op_id.clone(),
            payload: serde_json::to_string(&cmd)?,
            event_at: self.corrected_now().to_rfc3339(),
            // Settle attaches to the cashier's shift; gate behind that shift's open
            // if it's still queued (mirrors the order create-path gating).
            depends_on_seq: self.store.live_seq_of(&shift_id)?,
            user_id,
            clock_offset_ms,
            shift_id: Some(shift_id.clone()),
        })?;
        let _ = self.drain_outbox().await;
        Ok(self.store.pending()?.iter().any(|i| i.id == op_id))
    }

    /// The branch's OPEN/READY open tickets (newest first). Server list (write-through
    /// cached, so it survives offline) PLUS any still-queued local fires overlaid as
    /// `status = "queued"` — offline-first visibility before the fire syncs.
    pub async fn list_open_tickets(&self) -> Result<Vec<tickets::TicketView>, CoreError> {
        use madar_api::apis::open_tickets_api as ot;
        let branch_id = self.session_branch_id()?;
        let server: Vec<madar_api::models::OpenTicketView> =
            match ot::list_open_tickets(&self.api.config(), ot::ListOpenTicketsParams { branch_id, status: None }).await {
                Ok(list) => {
                    cache_views(&self.store, "cache:open_tickets", &list);
                    list
                }
                Err(_) => cached_views(&self.store, "cache:open_tickets"),
            };
        let mut out: Vec<tickets::TicketView> = server
            .iter()
            .filter(|v| v.status != "settled" && v.status != "voided")
            .map(|v| tickets::to_view(v, false))
            .collect();
        // Overlay still-queued fires (a pending fire is never in the server list).
        for item in self.store.pending()?.iter().filter(|i| i.op_type == "open_ticket") {
            if let Ok(cmd) = serde_json::from_str::<tickets::FireTicketCommand>(&item.payload) {
                out.push(queued_ticket_view(&cmd, &item.event_at));
            }
        }
        Ok(out)
    }

    /// One open ticket by server id (the detail screen). Online; a queued (unsynced)
    /// ticket has no server id yet — read it from `list_open_tickets` instead.
    pub async fn get_ticket(&self, ticket_id: String) -> Result<tickets::TicketView, CoreError> {
        use madar_api::apis::open_tickets_api as ot;
        let v = ot::get_open_ticket(&self.api.config(), ot::GetOpenTicketParams { id: ticket_id })
            .await
            .map_err(net::map_api_error)?;
        Ok(tickets::to_view(&v, false))
    }
}

// ── Kitchen Display System (station feed + bump) ─────────────────────────────
#[uniffi::export(async_runtime = "tokio")]
impl MadarCore {
    /// The branch's kitchen stations (the KDS device-setup / chit-routing picker).
    /// Write-through cached so the picker survives offline.
    pub async fn kds_list_stations(&self) -> Result<Vec<kds::KdsStationView>, CoreError> {
        use madar_api::apis::kitchen_api as k;
        let branch_id = self.session_branch_id()?;
        let stations: Vec<madar_api::models::KitchenStation> =
            match k::list_stations(&self.api.config(), k::ListStationsParams { branch_id }).await {
                Ok(list) => {
                    cache_views(&self.store, "cache:kds_stations", &list);
                    list
                }
                Err(_) => cached_views(&self.store, "cache:kds_stations"),
            };
        Ok(stations.iter().map(kds::station_view).collect())
    }

    /// The KDS feed: outstanding kitchen tickets for the branch (optionally filtered
    /// to a `station_id` — tickets with pending work for it). Sorted oldest-first
    /// (rush to top), ready tickets last. Write-through cached per station so the
    /// board still shows the last snapshot after a reconnect.
    pub async fn kds_list(&self, station_id: Option<String>) -> Result<Vec<kds::KdsTicketView>, CoreError> {
        use madar_api::apis::kitchen_api as k;
        let branch_id = self.session_branch_id()?;
        let cache_key = match &station_id {
            Some(s) => format!("cache:kds:{s}"),
            None => "cache:kds:all".to_string(),
        };
        let feed: Vec<madar_api::models::KitchenTicketView> = match k::feed(
            &self.api.config(),
            k::FeedParams { branch_id, station_id },
        )
        .await
        {
            Ok(list) => {
                cache_views(&self.store, &cache_key, &list);
                list
            }
            Err(_) => cached_views(&self.store, &cache_key),
        };
        let mut out: Vec<kds::KdsTicketView> = feed.iter().map(kds::ticket_view).collect();
        // Overlay LAN-projected fires not yet in the server feed (offline visibility);
        // prune any whose derived id now appears in the feed (they synced → server wins).
        let mut lan = lan_kds_read(&self.store);
        let synced: std::collections::HashSet<String> = out.iter().map(|t| t.id.clone()).collect();
        let before = lan.len();
        lan.retain(|t| !synced.contains(&t.id));
        if lan.len() != before {
            lan_kds_write(&self.store, &lan);
        }
        kds::overlay_lan_tickets(&mut out, lan);
        // Overlay still-pending (un-synced) bumps so the board shows the cook's
        // latest tap instantly — even offline, before the bump drains to the server.
        kds::overlay_pending_bumps(&mut out, &self.pending_bumps());
        kds::sort_feed(&mut out);
        Ok(out)
    }

    /// Bump a kitchen line (mark it done at its station). OUTBOX-FIRST (Phase E §2):
    /// the bump is written to the durable replay queue, then drained immediately —
    /// online-direct when connected, queued through a network blip otherwise. A
    /// ticket goes "ready" server-side once all its lines are bumped; the board
    /// reflects this tap instantly via the pending-bump overlay.
    pub async fn kds_bump(&self, item_id: String) -> Result<(), CoreError> {
        self.enqueue_bump(item_id, true).await
    }

    /// Un-bump a kitchen line (undo a mistaken bump). Same outbox-first path.
    pub async fn kds_unbump(&self, item_id: String) -> Result<(), CoreError> {
        self.enqueue_bump(item_id, false).await
    }

    /// The branch's active tills (the device-setup / Settings till picker). Write-
    /// through cached so the picker still works offline. Default till first.
    pub async fn list_tills(&self) -> Result<Vec<TillView>, CoreError> {
        use madar_api::apis::tills_api as t;
        let branch_id = self.session_branch_id()?;
        let tills: Vec<madar_api::models::Till> =
            match t::list_tills(&self.api.config(), t::ListTillsParams { branch_id }).await {
                Ok(list) => {
                    cache_views(&self.store, "cache:tills", &list);
                    list
                }
                Err(_) => cached_views(&self.store, "cache:tills"),
            };
        let mut out: Vec<TillView> = tills
            .iter()
            .filter(|t| t.is_active)
            .map(|t| TillView {
                id: t.id.to_string(),
                name: t.name.clone(),
                is_default: t.is_default,
                is_active: t.is_active,
            })
            .collect();
        out.sort_by(|a, b| b.is_default.cmp(&a.is_default).then(a.name.cmp(&b.name)));
        Ok(out)
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl MadarCore {
    /// The branch's delivery queue (newest first). `status` is a comma-separated
    /// wire filter (e.g. "received,confirmed"); `None` = all. Online-only.
    pub async fn list_delivery_orders(
        &self,
        status: Option<String>,
    ) -> Result<Vec<delivery::DeliveryOrderView>, CoreError> {
        use madar_api::apis::delivery_api as d;
        let branch = self.session_branch_id()?;
        let loc = self.current_locale();
        // Live when online (cached write-through, keyed by the status filter), else
        // the last-synced snapshot — the delivery board still shows offline.
        let key = format!("cache:delivery:{}", status.as_deref().unwrap_or("all"));
        if !self.current_session().map(|s| s.online).unwrap_or(false) {
            return Ok(cached_views(&self.store, &key));
        }
        match d::list_delivery_orders(
            &self.api.config(),
            d::ListDeliveryOrdersParams { branch_id: branch, status, limit: Some(200) },
        )
        .await
        {
            Ok(orders) => {
                let views: Vec<_> = orders.iter().map(|o| delivery::order_view(o, &loc)).collect();
                cache_views(&self.store, &key, &views);
                Ok(views)
            }
            Err(_) => Ok(cached_views(&self.store, &key)),
        }
    }

    /// A single delivery order by id.
    pub async fn delivery_order_detail(&self, id: String) -> Result<delivery::DeliveryOrderView, CoreError> {
        use madar_api::apis::delivery_api as d;
        let loc = self.current_locale();
        let o = d::get_delivery_order(&self.api.config(), d::GetDeliveryOrderParams { id })
            .await
            .map_err(net::map_api_error)?;
        Ok(delivery::order_view(&o, &loc))
    }

    /// Set a delivery order's status to an explicit wire value.
    pub async fn delivery_set_status(
        &self,
        id: String,
        status: String,
    ) -> Result<delivery::DeliveryOrderView, CoreError> {
        use madar_api::apis::delivery_api as d;
        let loc = self.current_locale();
        let o = d::set_status(
            &self.api.config(),
            d::SetStatusParams { id, status_input: madar_api::models::StatusInput::new(status) },
        )
        .await
        .map_err(net::map_api_error)?;
        Ok(delivery::order_view(&o, &loc))
    }

    /// Advance one step in the lifecycle from `current` (received→confirmed→…→
    /// delivered). Errors if there's no further forward step.
    pub async fn delivery_advance_status(
        &self,
        id: String,
        current: String,
    ) -> Result<delivery::DeliveryOrderView, CoreError> {
        let next = delivery::next_status(&current)
            .ok_or_else(|| CoreError::Validation { field: "status".into(), detail: "no further status".into() })?;
        self.delivery_set_status(id, next.to_string()).await
    }

    /// Set the per-order extra prep time (non-negative multiple of 5 minutes).
    pub async fn delivery_set_prep_time(
        &self,
        id: String,
        extra_minutes: i32,
    ) -> Result<delivery::DeliveryOrderView, CoreError> {
        use madar_api::apis::delivery_api as d;
        let loc = self.current_locale();
        let o = d::set_prep_time(
            &self.api.config(),
            d::SetPrepTimeParams { id, prep_time_input: madar_api::models::PrepTimeInput::new(extra_minutes) },
        )
        .await
        .map_err(net::map_api_error)?;
        Ok(delivery::order_view(&o, &loc))
    }

    /// Cancel a delivery order. `restore_inventory = false` means the food was
    /// made and is wasted (the frozen plan is deducted + logged as waste).
    pub async fn delivery_cancel(
        &self,
        id: String,
        reason: Option<String>,
        restore_inventory: bool,
    ) -> Result<delivery::DeliveryOrderView, CoreError> {
        use madar_api::apis::delivery_api as d;
        let loc = self.current_locale();
        let mut input = madar_api::models::CancelInput::new();
        input.reason = Some(reason.filter(|s| !s.trim().is_empty()));
        input.restore_inventory = Some(restore_inventory);
        let o = d::cancel_delivery_order(&self.api.config(), d::CancelDeliveryOrderParams { id, cancel_input: input })
            .await
            .map_err(net::map_api_error)?;
        Ok(delivery::order_view(&o, &loc))
    }

    /// Finalize a delivery order into a real completed sale on the current open
    /// shift — replays the frozen snapshot. `payment_method_id` resolves to the
    /// raw wire method. Returns the new order id/ref + any oversold warnings.
    pub async fn delivery_finalize(
        &self,
        id: String,
        payment_method_id: String,
    ) -> Result<delivery::DeliveryFinalizeView, CoreError> {
        use madar_api::apis::delivery_api as d;
        let raw = checkout::raw_payment_method(&self.store, &payment_method_id)?.ok_or_else(|| {
            CoreError::Validation { field: "payment_method".into(), detail: "unknown payment method".into() }
        })?;
        let shift = shift::current(&self.store)?
            .filter(|s| s.is_open)
            .ok_or_else(|| CoreError::Validation { field: "shift".into(), detail: "no open shift".into() })?;
        let shift_uuid = uuid::Uuid::parse_str(&shift.id)
            .map_err(|_| CoreError::Validation { field: "shift_id".into(), detail: "bad shift id".into() })?;
        let input = madar_api::models::FinalizeInput::new(raw.name, shift_uuid);
        let res = d::finalize_delivery_order(&self.api.config(), d::FinalizeDeliveryOrderParams { id, finalize_input: input })
            .await
            .map_err(net::map_api_error)?;
        Ok(delivery::DeliveryFinalizeView {
            order_id: res.order_id.to_string(),
            order_ref: res.order_ref.flatten().filter(|s| !s.is_empty()),
            warnings: res.warnings,
        })
    }

    /// The branch's delivery settings + accepting overrides.
    pub async fn delivery_settings(&self) -> Result<delivery::DeliverySettingsView, CoreError> {
        use madar_api::apis::delivery_api as d;
        let branch = self.session_branch_id()?;
        let s = d::get_branch_settings(&self.api.config(), d::GetBranchSettingsParams { branch_id: branch })
            .await
            .map_err(net::map_api_error)?;
        Ok(delivery::settings_view(&s))
    }

    /// Set a channel's accepting override. `channel` = "in_mall"/"outside",
    /// `mode` = "auto"/"open"/"closed". 409 if opening a dashboard-disabled channel.
    pub async fn delivery_set_accepting(
        &self,
        channel: String,
        mode: String,
    ) -> Result<delivery::DeliverySettingsView, CoreError> {
        use madar_api::apis::delivery_api as d;
        let branch = self.session_branch_id()?;
        let branch_uuid = uuid::Uuid::parse_str(&branch)
            .map_err(|_| CoreError::Validation { field: "branch_id".into(), detail: "bad branch id".into() })?;
        let input = madar_api::models::AcceptingInput::new(branch_uuid, channel, mode);
        let s = d::set_accepting(&self.api.config(), d::SetAcceptingParams { accepting_input: input })
            .await
            .map_err(net::map_api_error)?;
        Ok(delivery::settings_view(&s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greet_includes_version() {
        let msg = greet("Teller".into());
        assert!(msg.contains("Teller"));
        assert!(msg.contains(&core_version()));
    }

    #[test]
    fn backoff_is_exponential_capped_and_jittered() {
        // BASE·2^(n-1): 2s, 4s, 8s … then capped at 5min.
        assert!((2_000..3_000).contains(&compute_backoff_ms(1, 7)));
        assert!((4_000..5_000).contains(&compute_backoff_ms(2, 7)));
        assert!((8_000..9_000).contains(&compute_backoff_ms(3, 7)));
        // Far out it saturates at the 5-minute cap (never overflows).
        assert_eq!(compute_backoff_ms(40, 7), K_MAX_BACKOFF_MS);
        assert_eq!(compute_backoff_ms(100, 1), K_MAX_BACKOFF_MS);
        // Jitter spreads two different items apart at the same attempt.
        assert_ne!(compute_backoff_ms(1, 1), compute_backoff_ms(1, 999));
    }

    #[test]
    fn backoff_edge_cases_clamp_attempts_and_stay_in_band() {
        // attempts <= 0 are clamped to the first step (shift 0) → BASE band.
        assert!((2_000..3_000).contains(&compute_backoff_ms(0, 7)));
        assert!((2_000..3_000).contains(&compute_backoff_ms(-5, 7)));
        // Jitter is bounded to [0, 1000) and added on top of the (capped) base.
        let j = compute_backoff_ms(1, 7) - K_BASE_BACKOFF_MS;
        assert!((0..1000).contains(&j));
        // Same (attempts, seq) is deterministic — no RNG.
        assert_eq!(compute_backoff_ms(3, 42), compute_backoff_ms(3, 42));
        // The capped value never exceeds the max even with jitter added.
        assert!(compute_backoff_ms(8, 999_999) <= K_MAX_BACKOFF_MS);
    }

    #[test]
    fn rebase_dopt_shifts_a_present_timestamp_by_the_delta() {
        let base = chrono::DateTime::parse_from_rfc3339("2026-06-20T12:00:00+00:00").unwrap();
        let mut field = Some(Some(base));
        rebase_dopt(&mut field, 60_000); // +1 minute
        assert_eq!(field, Some(Some(base + chrono::Duration::minutes(1))));
        // A negative delta walks it back.
        rebase_dopt(&mut field, -120_000); // -2 minutes from the new value
        assert_eq!(field, Some(Some(base - chrono::Duration::minutes(1))));
    }

    #[test]
    fn rebase_dopt_zero_delta_is_a_noop() {
        let base = chrono::DateTime::parse_from_rfc3339("2026-06-20T12:00:00+00:00").unwrap();
        let mut field = Some(Some(base));
        rebase_dopt(&mut field, 0);
        assert_eq!(field, Some(Some(base)));
    }

    #[test]
    fn rebase_dopt_tolerates_absent_double_option_levels() {
        // Outer None (field omitted) — must not panic, stays None.
        let mut none: Option<Option<chrono::DateTime<chrono::FixedOffset>>> = None;
        rebase_dopt(&mut none, 60_000);
        assert_eq!(none, None);
        // Inner None (explicit null) — stays Some(None).
        let mut inner_none: Option<Option<chrono::DateTime<chrono::FixedOffset>>> = Some(None);
        rebase_dopt(&mut inner_none, 60_000);
        assert_eq!(inner_none, Some(None));
    }

    #[test]
    fn clock_skew_minutes_divides_seconds_truncating_toward_zero() {
        let core = MadarCore::from_env().unwrap();
        // Default is 0.
        assert_eq!(core.clock_skew_minutes(), 0);
        // 125s → 2 min (truncated).
        core.clock_skew_secs.store(125, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(core.clock_skew_minutes(), 2);
        // Negative skew truncates toward zero too: -125s → -2 min.
        core.clock_skew_secs.store(-125, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(core.clock_skew_minutes(), -2);
        // Under a minute → 0.
        core.clock_skew_secs.store(59, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(core.clock_skew_minutes(), 0);
    }

    #[test]
    fn sync_status_reflects_outbox_counts_and_default_flags() {
        // Signed out, empty outbox → all zero, offline, not auth-paused.
        let core = MadarCore::from_env().unwrap();
        let s = core.sync_status().unwrap();
        assert_eq!(s, SyncStatusView { pending: 0, failed: 0, online: false, auth_paused: false });
    }

    #[test]
    fn classify_send_maps_every_status_correctly() {
        let dead = |o: &SendOutcome| matches!(o, SendOutcome::Dead(_));
        let ack = |o: &SendOutcome| matches!(o, SendOutcome::Acked(_));
        let retry = |o: &SendOutcome| matches!(o, SendOutcome::Retry(_));

        // Connectivity / auth / server-error.
        assert!(matches!(classify_send(CoreError::Offline { detail: "x".into() }, Idem::No), SendOutcome::Offline));
        assert!(matches!(classify_send(CoreError::Unauthenticated { detail: "x".into() }, Idem::No), SendOutcome::AuthExpired));
        assert!(retry(&classify_send(CoreError::Transient { detail: "503".into() }, Idem::No)));
        // Permanent validation / permission → dead.
        assert!(dead(&classify_send(CoreError::Validation { field: "".into(), detail: "bad".into() }, Idem::No)));
        assert!(dead(&classify_send(CoreError::Forbidden { resource: "api".into(), action: "no".into() }, Idem::No)));
        // 409: order/open NOT recorded → dead; void/close already-applied → ack.
        assert!(dead(&classify_send(srv(409), Idem::No)));
        assert!(ack(&classify_send(srv(409), Idem::Yes)));
        assert!(ack(&classify_send(srv(409), Idem::VoidIdem)));
        // 404: idempotent gone → ack; but a VOID 404 (order never landed) → dead.
        assert!(ack(&classify_send(srv(404), Idem::Yes)));
        assert!(dead(&classify_send(srv(404), Idem::VoidIdem)));
    }

    fn srv(status: u16) -> CoreError {
        CoreError::Server { status, code: "x".into(), detail: "boom".into() }
    }

    #[test]
    fn replay_backend_object_rejects_captive_portal_and_non_objects() {
        // A Wi-Fi login page served as 200 text/html — NOT our backend. The whole
        // point: this must NOT be acked, or a queued sale is silently lost.
        assert!(replay_backend_object("<!DOCTYPE html><html><body>Sign in to WiFi</body></html>").is_none());
        // Empty body and whitespace (some proxies return 200 with no payload).
        assert!(replay_backend_object("").is_none());
        assert!(replay_backend_object("   ").is_none());
        // Bare JSON array / scalar / string are valid JSON but not our object shape.
        assert!(replay_backend_object("[1,2,3]").is_none());
        assert!(replay_backend_object("42").is_none());
        assert!(replay_backend_object("\"ok\"").is_none());
        // A genuine backend object passes (even an empty one — shape, not contents).
        assert!(replay_backend_object("{}").is_some());
        let order = replay_backend_object(r#"{"id":"abc-123","total_amount":1500}"#).unwrap();
        assert_eq!(order.get("id").and_then(|v| v.as_str()), Some("abc-123"));
    }

    #[test]
    fn cache_views_roundtrips_and_is_corruption_safe() {
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug, Clone)]
        struct Row {
            id: i64,
            name: String,
        }
        let store = store::Store::open("").unwrap();
        // Nothing cached yet → empty (the cold-start / never-synced case).
        assert!(cached_views::<Row>(&store, "cache:t").is_empty());
        // Write-through, then read back the exact snapshot.
        let rows = vec![Row { id: 1, name: "a".into() }, Row { id: 2, name: "b".into() }];
        cache_views(&store, "cache:t", &rows);
        assert_eq!(cached_views::<Row>(&store, "cache:t"), rows);
        // A re-sync overwrites (the snapshot is the latest, not appended).
        let fewer = vec![Row { id: 9, name: "z".into() }];
        cache_views(&store, "cache:t", &fewer);
        assert_eq!(cached_views::<Row>(&store, "cache:t"), fewer);
        // A corrupt/foreign payload reads back as empty rather than erroring the read.
        store.kv_put("cache:bad", "{not json").unwrap();
        assert!(cached_views::<Row>(&store, "cache:bad").is_empty());
    }

    #[test]
    fn diag_ring_buffer_caps_and_orders_newest_first() {
        let core = MadarCore::from_env().unwrap();
        assert!(core.recent_logs().is_empty());
        for i in 0..250 {
            core.push_diag("warn", format!("m{i}"));
        }
        let logs = core.recent_logs();
        assert_eq!(logs.len(), 200); // capped at 200
        assert_eq!(logs[0].message, "m249"); // newest first
        assert_eq!(logs[0].level, "warn");
        core.clear_logs();
        assert!(core.recent_logs().is_empty());
    }

    #[test]
    fn core_reads_env_config() {
        let core = MadarCore::from_env().unwrap();
        assert!(core.base_url().starts_with("http"));
        assert!(!core.environment().is_empty());
        assert_eq!(core.pending_outbox_count().unwrap(), 0);
    }

    #[test]
    fn surface_version_is_pinned() {
        // 1: realtime SSE + AppRoute payload variants. 2: core-owned device config.
        // 3: LAN offline relay surface. 4: core-driven realtime (start_realtime +
        // RealtimePlayer). Every breaking FFI change MUST bump this and this assertion.
        assert_eq!(ffi_surface_version(), 4);
    }

    #[test]
    fn mirror_replay_op_enqueues_a_dedup_keyed_backup() {
        let store = store::Store::open("").unwrap();
        // A received LAN bump → mirrored into our outbox as a `lan_mirror` backup,
        // attributed to the ORIGINAL teller, keyed for server-side dedup.
        let env =
            serde_json::json!({ "op": "bump_kitchen_item", "teller_id": "t1", "item_id": "k9" })
                .to_string();
        mirror_replay_op(&store, &env);
        let pending = store.pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].op_type, "lan_mirror");
        assert_eq!(pending[0].payload, env, "the envelope posts verbatim");
        assert_eq!(pending[0].user_id.as_deref(), Some("t1"), "original actor preserved");
        // A re-received duplicate (gossip / both paths) collapses to one row.
        mirror_replay_op(&store, &env);
        assert_eq!(store.pending().unwrap().len(), 1, "idempotent on the op handle");
        // A fire envelope keys on its request idempotency_key.
        let fire = serde_json::json!({
            "op": "fire_open_ticket", "teller_id": "w2",
            "request": { "idempotency_key": "tic-1", "items": [] }
        })
        .to_string();
        mirror_replay_op(&store, &fire);
        assert_eq!(store.pending().unwrap().len(), 2, "distinct op → distinct backup");
    }

    #[test]
    fn set_locale_changes_strings_and_rtl_at_runtime() {
        let core = MadarCore::from_env().unwrap();
        core.set_locale("en".into());
        assert_eq!(core.tr("login.sign_in".into()), "Sign in");
        assert!(!core.is_rtl());
        core.set_locale("ar".into());
        assert_eq!(core.tr("login.sign_in".into()), "تسجيل الدخول");
        assert!(core.is_rtl());
        assert_eq!(core.locale(), "ar");
    }

    /// sign_in falls back to an offline unlock when the network is unreachable
    /// and a cached bundle holds the teller's PIN. Points the core at a dead
    /// port so the online `login` fails fast with `Offline`.
    #[tokio::test]
    async fn sign_in_falls_back_to_offline_unlock_when_network_down() {
        use argon2::password_hash::SaltString;
        use argon2::{Argon2, PasswordHasher};

        let core = MadarCore::new(MadarConfig {
            base_url: "http://127.0.0.1:1".into(), // nothing listening → connect refused
            environment: "dev".into(),
            db_path: String::new(),
            locale: "en".into(),
        })
        .unwrap();

        // Seed the org bundle the offline unlock verifies against.
        let salt = SaltString::encode_b64(b"sufrix-test-salt").unwrap();
        let phc = Argon2::default().hash_password(b"1234", &salt).unwrap().to_string();
        let bundle = serde_json::json!({
            "org_id": "00000000-0000-0000-0000-0000000000aa",
            "generated_at": "2026-06-19T10:00:00Z",
            "tellers": [{
                "user_id": "00000000-0000-0000-0000-0000000000bb",
                "name": "Sara", "role": "teller", "is_active": true,
                "offline_pin_hash": phc,
            }]
        });
        core.store.kv_put(session::BUNDLE_KEY, &bundle.to_string()).unwrap();
        core.store
            .kv_put(session::ORG_CONFIG_KEY, r#"{"org_id":"00000000-0000-0000-0000-0000000000aa","currency_code":"EGP","tax_rate":0.14}"#)
            .unwrap();

        let req = session::LoginRequest {
            mode: session::LoginMode::Pin,
            name: Some("Sara".into()),
            pin: Some("1234".into()),
            branch_id: Some("00000000-0000-0000-0000-000000000001".into()),
            email: None,
            password: None,
            org_id: None,
        };
        let snap = core.sign_in(req).await.expect("offline fallback should sign in");
        assert_eq!(snap.display_name, "Sara");
        assert!(!snap.online);
        assert!(core.is_authenticated());

        // A wrong PIN offline (still network-down) must NOT sign in.
        let bad = session::LoginRequest {
            pin: Some("0000".into()),
            ..session::LoginRequest {
                mode: session::LoginMode::Pin,
                name: Some("Sara".into()),
                pin: None,
                branch_id: Some("00000000-0000-0000-0000-000000000001".into()),
                email: None, password: None, org_id: None,
            }
        };
        assert!(core.sign_in(bad).await.is_err());
    }
}

/// Routing-lifecycle tests — the class of bug behind the open-shift "bounce".
/// These poke the private session/store (same-crate) to drive `app_route`
/// through every transition without a network.
#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    fn teller_session(user_id: &str, branch: Option<&str>) -> session::SessionState {
        session::SessionState {
            snapshot: session::SessionSnapshot {
                user_id: user_id.into(),
                display_name: "Sara".into(),
                role: "teller".into(),
                org_id: Some("org-1".into()),
                branch_id: branch.map(Into::into),
                currency_code: "EGP".into(),
                tax_rate: 0.14,
                online: true,
                permissions_loaded: true,
            },
            permissions: vec![],
            token: None,
        }
    }

    fn kitchen_session(user_id: &str, branch: Option<&str>) -> session::SessionState {
        let mut s = teller_session(user_id, branch);
        s.snapshot.role = "kitchen".into();
        s
    }

    fn set_session(core: &MadarCore, state: Option<session::SessionState>) {
        *core.session.write().unwrap_or_else(|e| e.into_inner()) = state;
    }

    fn seed_shift(core: &MadarCore, teller: uuid::Uuid, status: &str) {
        let _ = seed_shift_returning_id(core, teller, status);
    }

    fn seed_shift_returning_id(core: &MadarCore, teller: uuid::Uuid, status: &str) -> String {
        let id = uuid::Uuid::new_v4();
        let s = madar_api::models::Shift {
            id,
            branch_id: uuid::Uuid::new_v4(),
            teller_id: teller,
            teller_name: "Sara".into(),
            opening_cash: 50000,
            status: status.into(),
            ..Default::default()
        };
        shift::save(&core.store, &s).unwrap();
        id.to_string()
    }

    fn enqueue_open_shift(core: &MadarCore, id: &str) {
        core.store
            .enqueue(&store::NewOutboxOp {
                id: id.into(),
                op_type: "open_shift".into(),
                idempotency_key: id.into(),
                payload: "{}".into(),
                event_at: "2026-06-20T12:00:00+00:00".into(),
                shift_id: Some(id.into()),
                ..Default::default()
            })
            .unwrap();
    }

    fn enqueue_close_shift(core: &MadarCore, shift_id: &str) {
        let id = format!("{shift_id}:close");
        core.store
            .enqueue(&store::NewOutboxOp {
                id: id.clone(),
                op_type: "close_shift".into(),
                idempotency_key: id,
                payload: "{}".into(),
                event_at: "2026-06-20T18:00:00+00:00".into(),
                shift_id: Some(shift_id.into()),
                ..Default::default()
            })
            .unwrap();
    }

    /// A real signed-in core pinned OFFLINE (dead url), against a cached bundle —
    /// for driving the genuine open/close/checkout FFI paths with no network.
    async fn signed_in_offline_core() -> Arc<MadarCore> {
        use argon2::password_hash::SaltString;
        use argon2::{Argon2, PasswordHasher};
        let core = MadarCore::new(MadarConfig {
            base_url: "http://127.0.0.1:1".into(),
            environment: "dev".into(),
            db_path: String::new(),
            locale: "en".into(),
        })
        .unwrap();
        let salt = SaltString::encode_b64(b"sufrix-test-salt").unwrap();
        let phc = Argon2::default().hash_password(b"1234", &salt).unwrap().to_string();
        core.store
            .kv_put(
                session::BUNDLE_KEY,
                &serde_json::json!({
                    "org_id": "00000000-0000-0000-0000-0000000000aa",
                    "generated_at": "2026-06-19T10:00:00Z",
                    "tellers": [{ "user_id": "00000000-0000-0000-0000-0000000000bb",
                        "name": "Sara", "role": "teller", "is_active": true, "offline_pin_hash": phc }]
                })
                .to_string(),
            )
            .unwrap();
        core.store
            .kv_put(session::ORG_CONFIG_KEY, r#"{"org_id":"00000000-0000-0000-0000-0000000000aa","currency_code":"EGP","tax_rate":0.14}"#)
            .unwrap();
        core.sign_in(session::LoginRequest {
            mode: session::LoginMode::Pin,
            name: Some("Sara".into()),
            pin: Some("1234".into()),
            branch_id: Some("00000000-0000-0000-0000-000000000001".into()),
            email: None,
            password: None,
            org_id: None,
        })
        .await
        .unwrap();
        core
    }

    /// ISSUE 1 fix (deterministic, no backend): the offline "close A → open B"
    /// handover wires open B's outbox DEPENDENCY onto A's still-queued close, so on
    /// reconnect B's open can NEVER replay before A's close commits. Without that
    /// gate the open races the still-open branch, 409s "a shift is already open for
    /// this branch", dead-letters, cascades B's orders, and strands the teller on
    /// the open-shift screen — the field bug. A first-ever open (no queued close)
    /// has no dependency, since the branch is already free.
    #[tokio::test]
    async fn offline_open_after_close_depends_on_the_close() {
        let core = signed_in_offline_core().await;

        // First-ever open: no prior close queued → no dependency (branch is free).
        core.open_shift(50_000, None).await.unwrap();
        let open_a = core
            .store
            .list_active()
            .unwrap()
            .into_iter()
            .find(|i| i.op_type == "open_shift")
            .unwrap();
        assert_eq!(open_a.depends_on_seq, None, "the first open has no close to wait on");

        // Close A (queues behind the open), then open B offline.
        core.close_shift(48_000, None).await.unwrap();
        core.open_shift(48_000, None).await.unwrap();

        let active = core.store.list_active().unwrap();
        let close_a = active.iter().find(|i| i.op_type == "close_shift").expect("close A queued");
        let open_b = active
            .iter()
            .filter(|i| i.op_type == "open_shift")
            .max_by_key(|i| i.seq)
            .expect("open B queued");
        assert!(open_b.seq > close_a.seq, "B opened after A's close");
        assert_eq!(
            open_b.depends_on_seq,
            Some(close_a.seq),
            "open B must DEPEND on A's close — the sequential-handover gate that prevents the 409",
        );
    }

    /// HARDENING (dependents WAIT on a dead dependency, never cascade-dead): if the
    /// prior shift's queued close DEAD-letters (e.g. a backend cash-continuity
    /// rejection), the dependent open — and by extension its orders — must stay
    /// PENDING (recoverable), not cascade dead and strand the sale. Resolving the
    /// root op later (retry/discard) then flows the whole chain. This is what keeps
    /// a teller-switch whose close fails from orphaning the next teller's sales.
    #[tokio::test]
    async fn dependent_op_waits_on_a_dead_dependency_instead_of_cascading() {
        let core = signed_in_offline_core().await;

        // Open A → close A → open B (B depends on A's close), all offline.
        core.open_shift(50_000, None).await.unwrap();
        core.close_shift(48_000, None).await.unwrap();
        core.open_shift(48_000, None).await.unwrap();

        let active = core.store.list_active().unwrap();
        let open_a = active.iter().filter(|i| i.op_type == "open_shift").min_by_key(|i| i.seq).unwrap().seq;
        let close_a = active.iter().find(|i| i.op_type == "close_shift").unwrap().seq;
        let open_b = active.iter().filter(|i| i.op_type == "open_shift").max_by_key(|i| i.seq).unwrap().seq;

        // Pretend A's open already synced, then A's close DIES on the server.
        core.store.mark_acked(open_a, Some("srv-a")).unwrap();
        core.store.mark_dead(close_a, "continuity: closing cash mismatch").unwrap();

        // Drain: open B's dependency (close A) is dead → it must WAIT, not cascade.
        let _ = core.drain_outbox().await;

        let after = core.store.list_active().unwrap();
        let ob = after.iter().find(|i| i.seq == open_b).expect("open B still in the outbox");
        assert_eq!(ob.status, "pending", "open B waits on the dead close — never cascade-dead");
        assert_eq!(core.store.dead_count().unwrap(), 1, "only the ROOT close is dead; the chain stays recoverable");
    }

    /// SEQUENTIAL-ONLY: `device_has_open_shift` is the deterministic gate. It's
    /// true for a cached OPEN shift and for an uncovered queued open (defense for
    /// a lost cache), and FALSE once the open is covered by a close — so the
    /// offline "close A → open B" flow is never blocked.
    #[test]
    fn device_has_open_shift_tracks_cache_and_uncovered_queued_opens() {
        let core = MadarCore::from_env().unwrap();
        assert!(!core.device_has_open_shift().unwrap()); // nothing yet

        // A cached OPEN shift counts.
        seed_shift_returning_id(&core, uuid::Uuid::new_v4(), "open");
        assert!(core.device_has_open_shift().unwrap());

        // Closed locally with nothing queued → no longer open.
        shift::close_local(&core.store).unwrap();
        assert!(!core.device_has_open_shift().unwrap());

        // Cache lost but an open is still queued with no close → still "open".
        shift::clear(&core.store).unwrap();
        let sid = uuid::Uuid::new_v4().to_string();
        enqueue_open_shift(&core, &sid);
        assert!(core.device_has_open_shift().unwrap());

        // Queue its close → the open is now covered → not open (reopen allowed).
        enqueue_close_shift(&core, &sid);
        assert!(!core.device_has_open_shift().unwrap());
    }

    /// The behavioral guarantee: a second `open_shift` while one is open is
    /// rejected; after a (local) close it's allowed again — the sequential
    /// offline shift cycle. Driven fully offline (dead url) on a real session.
    #[tokio::test]
    async fn open_shift_rejects_a_second_open_then_allows_reopen_after_close() {
        use argon2::password_hash::SaltString;
        use argon2::{Argon2, PasswordHasher};

        let core = MadarCore::new(MadarConfig {
            base_url: "http://127.0.0.1:1".into(),
            environment: "dev".into(),
            db_path: String::new(),
            locale: "en".into(),
        })
        .unwrap();
        let salt = SaltString::encode_b64(b"sufrix-test-salt").unwrap();
        let phc = Argon2::default().hash_password(b"1234", &salt).unwrap().to_string();
        core.store
            .kv_put(
                session::BUNDLE_KEY,
                &serde_json::json!({
                    "org_id": "00000000-0000-0000-0000-0000000000aa",
                    "generated_at": "2026-06-19T10:00:00Z",
                    "tellers": [{ "user_id": "00000000-0000-0000-0000-0000000000bb",
                        "name": "Sara", "role": "teller", "is_active": true, "offline_pin_hash": phc }]
                })
                .to_string(),
            )
            .unwrap();
        core.store
            .kv_put(session::ORG_CONFIG_KEY, r#"{"org_id":"00000000-0000-0000-0000-0000000000aa","currency_code":"EGP","tax_rate":0.14}"#)
            .unwrap();
        core.sign_in(session::LoginRequest {
            mode: session::LoginMode::Pin,
            name: Some("Sara".into()),
            pin: Some("1234".into()),
            branch_id: Some("00000000-0000-0000-0000-000000000001".into()),
            email: None,
            password: None,
            org_id: None,
        })
        .await
        .unwrap();

        // First open succeeds.
        core.open_shift(50000, None).await.unwrap();
        assert!(core.current_shift().unwrap().unwrap().is_open);

        // A second open while one is open is REJECTED (and leaves the first intact).
        let second = core.open_shift(60000, None).await;
        assert!(matches!(second, Err(CoreError::Validation { .. })), "got {second:?}");
        let still = core.current_shift().unwrap().unwrap();
        assert!(still.is_open);
        assert_eq!(still.opening_cash_minor, 50000, "the original shift must be untouched");

        // Close it (locally; the close just queues offline) → reopen is allowed.
        core.close_shift(48000, None).await.unwrap();
        assert!(!core.current_shift().unwrap().unwrap().is_open);
        core.open_shift(48000, None).await.expect("reopen after close must be allowed");
        assert!(core.current_shift().unwrap().unwrap().is_open);
    }

    /// Skeptic-1 regression: the open-shift pending guard must be scoped to the
    /// cached shift's id, NOT device-global. A foreign teller's orphaned command
    /// (left in the shared outbox after sign-out) must not keep a shift alive.
    #[test]
    fn open_pending_is_scoped_to_the_cached_shift_not_device_global() {
        let core = MadarCore::from_env().unwrap();
        let shift_id = seed_shift_returning_id(&core, uuid::Uuid::new_v4(), "open");
        assert!(!core.shift_command_pending("open_shift").unwrap()); // nothing queued
        // A DIFFERENT shift's orphaned open_shift command does NOT count.
        enqueue_open_shift(&core, &uuid::Uuid::new_v4().to_string());
        assert!(!core.shift_command_pending("open_shift").unwrap());
        // Our own cached shift's command DOES.
        enqueue_open_shift(&core, &shift_id);
        assert!(core.shift_command_pending("open_shift").unwrap());
    }

    /// End-to-end offline: open a shift, sell nothing, then close it. The shift
    /// flips to closed locally (route → open-shift) and the close command queues
    /// behind the open (FIFO); the cart is dropped.
    #[tokio::test]
    async fn closing_a_shift_offline_routes_back_to_open_shift() {
        use argon2::password_hash::SaltString;
        use argon2::{Argon2, PasswordHasher};

        let core = MadarCore::new(MadarConfig {
            base_url: "http://127.0.0.1:1".into(),
            environment: "dev".into(),
            db_path: String::new(),
            locale: "en".into(),
        })
        .unwrap();
        let salt = SaltString::encode_b64(b"sufrix-test-salt").unwrap();
        let phc = Argon2::default().hash_password(b"1234", &salt).unwrap().to_string();
        core.store
            .kv_put(
                session::BUNDLE_KEY,
                &serde_json::json!({
                    "org_id": "00000000-0000-0000-0000-0000000000aa",
                    "generated_at": "2026-06-19T10:00:00Z",
                    "tellers": [{ "user_id": "00000000-0000-0000-0000-0000000000bb",
                        "name": "Sara", "role": "teller", "is_active": true, "offline_pin_hash": phc }]
                })
                .to_string(),
            )
            .unwrap();
        core.store
            .kv_put(session::ORG_CONFIG_KEY, r#"{"org_id":"00000000-0000-0000-0000-0000000000aa","currency_code":"EGP","tax_rate":0.14}"#)
            .unwrap();
        // The device is bound to its branch in the CORE store; sign-in + app_route
        // both read it from there now (no host-passed branch).
        core.set_device_branch("00000000-0000-0000-0000-000000000001".into(), None).unwrap();
        core.sign_in(session::LoginRequest {
            mode: session::LoginMode::Pin,
            name: Some("Sara".into()),
            pin: Some("1234".into()),
            branch_id: Some("00000000-0000-0000-0000-000000000001".into()),
            email: None,
            password: None,
            org_id: None,
        })
        .await
        .unwrap();

        core.open_shift(50000, None).await.unwrap();
        core.cart_add("item-1".into(), "Latte".into(), 1000).unwrap();
        assert_eq!(core.app_route(), AppRoute::Order);

        core.close_shift(48000, Some("short by 20".into())).await.unwrap();
        // Routed back to open-shift, cart dropped, and both commands queued.
        assert_eq!(core.app_route(), AppRoute::OpenShift);
        assert!(core.cart_lines().unwrap().is_empty());
        assert_eq!(core.pending_outbox_count().unwrap(), 2); // open + close
        assert!(core.shift_command_pending("close_shift").unwrap());
    }

    #[tokio::test]
    async fn close_shift_without_an_open_shift_is_rejected() {
        let core = MadarCore::from_env().unwrap();
        let err = core.close_shift(1000, None).await;
        assert!(matches!(err, Err(CoreError::Validation { .. })));
    }

    #[test]
    fn route_device_setup_until_branch_bound() {
        let core = MadarCore::from_env().unwrap();
        assert_eq!(core.app_route(), AppRoute::DeviceSetup); // unbound (no device config)
        core.set_device_branch("b".into(), Some("Main".into())).unwrap();
        core.start_reconfigure().unwrap();
        assert_eq!(core.app_route(), AppRoute::DeviceSetup); // bound but mid-reconfigure
    }

    #[test]
    fn route_login_when_configured_but_signed_out() {
        let core = MadarCore::from_env().unwrap();
        core.set_device_branch("b".into(), None).unwrap();
        assert_eq!(core.app_route(), AppRoute::Login);
    }

    #[test]
    fn route_open_shift_when_signed_in_without_a_shift() {
        let core = MadarCore::from_env().unwrap();
        core.set_device_branch("b".into(), None).unwrap();
        set_session(&core, Some(teller_session(&uuid::Uuid::new_v4().to_string(), Some("b"))));
        assert_eq!(core.app_route(), AppRoute::OpenShift);
    }

    #[test]
    fn route_order_when_own_shift_is_open() {
        // The regression: a teller's own open shift routes to Order and STAYS.
        let core = MadarCore::from_env().unwrap();
        core.set_device_branch("b".into(), None).unwrap();
        let teller = uuid::Uuid::new_v4();
        set_session(&core, Some(teller_session(&teller.to_string(), Some("b"))));
        seed_shift(&core, teller, "open");
        assert_eq!(core.app_route(), AppRoute::Order);
    }

    #[test]
    fn route_open_shift_for_a_foreign_tellers_shift() {
        // A stale shift left by a DIFFERENT teller must not route the new one in.
        let core = MadarCore::from_env().unwrap();
        core.set_device_branch("b".into(), None).unwrap();
        let me = uuid::Uuid::new_v4();
        set_session(&core, Some(teller_session(&me.to_string(), Some("b"))));
        seed_shift(&core, uuid::Uuid::new_v4(), "open");
        assert_eq!(core.app_route(), AppRoute::OpenShift);
    }

    #[test]
    fn route_open_shift_when_the_shift_is_closed() {
        let core = MadarCore::from_env().unwrap();
        core.set_device_branch("b".into(), None).unwrap();
        let teller = uuid::Uuid::new_v4();
        set_session(&core, Some(teller_session(&teller.to_string(), Some("b"))));
        seed_shift(&core, teller, "closed");
        assert_eq!(core.app_route(), AppRoute::OpenShift);
    }

    #[test]
    fn route_kitchen_role_to_the_kds_for_its_station() {
        // A kitchen-role device routes to the KDS for its configured station,
        // needing NO shift — but stays in device-setup until a station is bound.
        let core = MadarCore::from_env().unwrap();
        core.set_device_branch("b".into(), None).unwrap();
        set_session(&core, Some(kitchen_session(&uuid::Uuid::new_v4().to_string(), Some("b"))));
        assert_eq!(core.app_route(), AppRoute::DeviceSetup, "kitchen device needs a station");
        core.set_device_station(Some("grill".into())).unwrap();
        assert_eq!(core.app_route(), AppRoute::KitchenDisplay { station_id: "grill".into() });
    }

    /// End-to-end offline: sign in offline, open a shift (the open_shift command
    /// can't reach the server, so it stays queued), and assert the route lands —
    /// and STAYS — on Order. This is the open-shift "bounce" reproduced E2E.
    #[tokio::test]
    async fn opening_a_shift_offline_routes_to_order_and_stays() {
        use argon2::password_hash::SaltString;
        use argon2::{Argon2, PasswordHasher};

        let core = MadarCore::new(MadarConfig {
            base_url: "http://127.0.0.1:1".into(), // nothing listening → offline
            environment: "dev".into(),
            db_path: String::new(),
            locale: "en".into(),
        })
        .unwrap();

        let salt = SaltString::encode_b64(b"sufrix-test-salt").unwrap();
        let phc = Argon2::default().hash_password(b"1234", &salt).unwrap().to_string();
        let bundle = serde_json::json!({
            "org_id": "00000000-0000-0000-0000-0000000000aa",
            "generated_at": "2026-06-19T10:00:00Z",
            "tellers": [{
                "user_id": "00000000-0000-0000-0000-0000000000bb",
                "name": "Sara", "role": "teller", "is_active": true,
                "offline_pin_hash": phc,
            }]
        });
        core.store.kv_put(session::BUNDLE_KEY, &bundle.to_string()).unwrap();
        core.store
            .kv_put(session::ORG_CONFIG_KEY, r#"{"org_id":"00000000-0000-0000-0000-0000000000aa","currency_code":"EGP","tax_rate":0.14}"#)
            .unwrap();

        let branch = "00000000-0000-0000-0000-000000000001";
        core.set_device_branch(branch.into(), None).unwrap();
        let snap = core
            .sign_in(session::LoginRequest {
                mode: session::LoginMode::Pin,
                name: Some("Sara".into()),
                pin: Some("1234".into()),
                branch_id: Some(branch.into()),
                email: None,
                password: None,
                org_id: None,
            })
            .await
            .expect("offline sign-in");
        assert!(!snap.online);
        // Signed in, no shift yet → open-shift.
        assert_eq!(core.app_route(), AppRoute::OpenShift);

        let shift = core.open_shift(50000, None).await.expect("open shift offline");
        assert!(shift.is_open);
        // The command is queued (couldn't reach the server)…
        assert_eq!(core.pending_outbox_count().unwrap(), 1);
        // …and the route is Order — and stays there (the bounce is gone).
        assert_eq!(core.app_route(), AppRoute::Order);
        assert_eq!(core.app_route(), AppRoute::Order);
    }

    #[test]
    fn cart_totals_use_the_session_tax_rate() {
        let core = MadarCore::from_env().unwrap();
        set_session(&core, Some(teller_session(&uuid::Uuid::new_v4().to_string(), Some("b"))));
        core.cart_add("item-1".into(), "Latte".into(), 1000).unwrap();
        core.cart_add("item-1".into(), "Latte".into(), 1000).unwrap(); // qty 2
        let t = core.cart_totals().unwrap();
        assert_eq!(t.item_count, 2);
        assert_eq!(t.subtotal_minor, 2000);
        assert_eq!(t.tax_minor, 280); // 0.14 * 2000
        assert_eq!(t.total_minor, 2280);
    }

    #[test]
    fn cart_totals_are_tax_free_when_signed_out() {
        let core = MadarCore::from_env().unwrap();
        core.cart_add("i".into(), "X".into(), 1000).unwrap();
        let t = core.cart_totals().unwrap();
        assert_eq!(t.tax_minor, 0);
        assert_eq!(t.total_minor, 1000);
    }
}
