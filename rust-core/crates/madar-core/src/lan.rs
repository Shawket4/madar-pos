//! LAN offline relay (Phase E) — the second delivery path beside the cloud bus.
//!
//! On a venue's local Wi-Fi, waiters/KDS/tills reach each other directly so a fire,
//! round, or bump is visible instantly even with NO internet. This module is the
//! PURE-RUST core of that relay (works on every target the core compiles to); the
//! only host glue is discovery permissions + keeping the process alive (§5).
//!
//! **THE INVARIANT (do not violate):** the LAN is a *delivery path*, never the
//! source of truth. Every write still hits the durable SQLite outbox FIRST and
//! drains to `POST /sync/replay` on reconnect, deduped on the client-minted id.
//! The LAN just makes it visible NOW; nothing ever lives only on the LAN.
//!
//! This file holds the foundation — the signed message envelope, the per-branch
//! HMAC, and the peer registry. The async embedded relay server + mDNS/beacon
//! discovery + mesh gossip build on top (added next).
//!
//! Vocabulary mirrors the cloud bus (`realtime.rs`): a LAN message carries the same
//! `event_type` ("kitchen.fired", "ticket.fired", …) + `data` (raw JSON payload) the
//! cloud SSE emits, so the unified consumer forwards either path as the SAME
//! [`RealtimeEvent`](crate::realtime::RealtimeEvent) — a duplicate across paths is a
//! free, idempotent snapshot-reload on the host.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::task::JoinHandle;

use crate::error::CoreError;

type HmacSha256 = Hmac<Sha256>;

// ── Message envelope ──────────────────────────────────────────────────────────

/// One relayed event on the LAN. Branch-scoped (a device only accepts messages for
/// its own branch), idempotency-keyed (`msg_id`), hop-bounded (mesh gossip). The
/// `event_type`/`data` pair is byte-for-byte what the cloud bus emits, so a LAN
/// message and its cloud twin reduce to one snapshot-reload on the host.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LanMessage {
    /// Client-minted UUID — the dedup key across LAN, gossip, and cloud paths.
    pub msg_id: String,
    /// The branch this message belongs to. Receivers drop anything not their own
    /// branch (isolation), and the HMAC key is branch-scoped on top of that.
    pub branch_id: String,
    /// Cloud-bus topic: `kitchen` | `tickets` | `orders` | `delivery`.
    pub topic: String,
    /// Cloud-bus event type, e.g. `kitchen.fired`, `ticket.fired`, `kitchen.item_bumped`.
    pub event_type: String,
    /// The event payload as a raw JSON string (same shape the cloud SSE `data:` carries).
    pub data: String,
    /// Gossip hop count — incremented on each re-relay, dropped past [`MAX_HOPS`].
    pub hop: u8,
    /// The device that originally minted this message (loop-avoidance + logging).
    pub sender_id: String,
    /// Sender's clock-corrected wall time (ms) — staleness/heartbeat, not trusted for ordering.
    pub sent_at_ms: i64,
    /// For a WRITE event (fire/round/bump): the `/sync/replay` envelope JSON, so a
    /// receiver can MIRROR it into its own outbox (robustness #4 — the write reaches
    /// the cloud even if the originating device dies before its outbox drains). `None`
    /// for a pure display event (e.g. one relayed from the cloud). Signed with the rest.
    #[serde(default)]
    pub replay_op: Option<String>,
}

/// Max gossip re-relays before a message is dropped — bounds flooding on a mesh.
pub const MAX_HOPS: u8 = 4;

impl LanMessage {
    /// True once this message has been relayed too many times (drop it).
    pub fn hops_exhausted(&self) -> bool {
        self.hop >= MAX_HOPS
    }

    /// A copy advanced one gossip hop (for re-relay), or `None` if exhausted.
    pub fn relayed(&self) -> Option<LanMessage> {
        if self.hops_exhausted() {
            return None;
        }
        let mut next = self.clone();
        next.hop += 1;
        Some(next)
    }
}

// ── Per-branch HMAC + signed frame ────────────────────────────────────────────

/// The on-wire frame: a [`LanMessage`] serialized verbatim (`msg`) plus an
/// HMAC-SHA256 (`sig`, hex) over those EXACT bytes. Signing the transmitted string —
/// not a re-serialization — sidesteps JSON key-ordering canonicalization entirely:
/// the receiver verifies over the bytes it received, then parses.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SignedFrame {
    pub sig: String,
    pub msg: String,
}

/// Derive the per-branch LAN key from the org's hex `lan_secret` (shipped in the
/// offline-auth bundle): `HMAC-SHA256(org_secret, branch_id)`. A distinct key per
/// branch means a leak is contained to one branch, and a foreign-branch device
/// (different key) can't forge messages a receiver will accept.
pub fn branch_key(org_secret_hex: &str, branch_id: &str) -> Vec<u8> {
    let secret = from_hex(org_secret_hex).unwrap_or_else(|| org_secret_hex.as_bytes().to_vec());
    let mut mac = HmacSha256::new_from_slice(&secret).expect("HMAC accepts any key length");
    mac.update(branch_id.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// Sign an arbitrary string body with the branch key → the on-wire frame. The
/// signature covers the EXACT bytes carried in `msg`, so the receiver verifies what
/// it received (no canonicalization). Used for both [`LanMessage`]s and beacons.
pub fn sign_str(key: &[u8], msg: String) -> SignedFrame {
    let sig = to_hex(&hmac(key, msg.as_bytes()));
    SignedFrame { sig, msg }
}

/// Verify a frame and return its body string, or `None` on a bad signature
/// (foreign-branch / tampered / wrong key). Constant-time compare (`verify_slice`).
pub fn verify_str<'a>(key: &[u8], frame: &'a SignedFrame) -> Option<&'a str> {
    let expected = from_hex(&frame.sig)?;
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(frame.msg.as_bytes());
    mac.verify_slice(&expected).ok()?;
    Some(&frame.msg)
}

/// Sign a [`LanMessage`] with the branch key, producing the on-wire frame.
pub fn sign_frame(key: &[u8], message: &LanMessage) -> SignedFrame {
    sign_str(key, serde_json::to_string(message).unwrap_or_default())
}

/// Verify a received frame and return the parsed message, or `None` if the
/// signature fails or the body doesn't parse.
pub fn verify_frame(key: &[u8], frame: &SignedFrame) -> Option<LanMessage> {
    verify_str(key, frame).and_then(|s| serde_json::from_str(s).ok())
}

fn hmac(key: &[u8], bytes: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(bytes);
    mac.finalize().into_bytes().to_vec()
}

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

// ── Peer registry ─────────────────────────────────────────────────────────────

/// A discovered LAN peer (via mDNS, the UDP beacon, or a manual hub-IP). A till that
/// holds an OPEN shift advertises `open_shift_id` — the freshly-heartbeated truth the
/// shift-open gate trusts over the (possibly-behind) backend. Liveness is the
/// `last_seen_ms` heartbeat; a peer that stops advertising expires within [`PEER_TTL_MS`].
#[derive(Clone, Debug, PartialEq)]
pub struct Peer {
    pub device_id: String,
    pub branch_id: String,
    /// `kitchen` | `waiter` | `teller`.
    pub role: String,
    pub host: String,
    pub port: u16,
    /// The station a KDS device shows (None for a waiter/till).
    pub station_id: Option<String>,
    /// `Some(shift_id)` when this is a till advertising an OPEN shift — the LAN
    /// shift-open gate's freshest signal. `None` otherwise.
    pub open_shift_id: Option<String>,
    /// Last heartbeat (clock-corrected wall ms); drives TTL expiry.
    pub last_seen_ms: i64,
}

/// How long after its last heartbeat a peer is considered gone. Short enough that a
/// till closing its shift stops counting toward "branch operating" within seconds.
pub const PEER_TTL_MS: i64 = 12_000;

/// The live set of discovered peers, keyed by `device_id`. Pure + heartbeat-driven;
/// the async discovery layer feeds it and reads relay targets / the shift gate.
#[derive(Default)]
pub struct PeerRegistry {
    peers: HashMap<String, Peer>,
}

impl PeerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or refresh a peer (its `last_seen_ms` heartbeat).
    pub fn upsert(&mut self, peer: Peer) {
        self.peers.insert(peer.device_id.clone(), peer);
    }

    /// Drop a peer outright (e.g. an mDNS "service removed").
    pub fn remove(&mut self, device_id: &str) {
        self.peers.remove(device_id);
    }

    /// Evict peers whose last heartbeat is older than the TTL at `now_ms`.
    pub fn prune(&mut self, now_ms: i64) {
        self.peers.retain(|_, p| now_ms - p.last_seen_ms <= PEER_TTL_MS);
    }

    /// The still-live peers for a branch at `now_ms` (TTL applied on read, so a
    /// stale-but-not-yet-pruned peer never counts).
    pub fn live_for_branch(&self, branch_id: &str, now_ms: i64) -> Vec<&Peer> {
        self.peers
            .values()
            .filter(|p| p.branch_id == branch_id && now_ms - p.last_seen_ms <= PEER_TTL_MS)
            .collect()
    }

    /// `(host, port)` of every live peer in the branch except `self_id` — the mesh
    /// fan-out targets a waiter/teller pushes a fire/round/bump to.
    pub fn relay_targets(&self, branch_id: &str, self_id: &str, now_ms: i64) -> Vec<(String, u16)> {
        self.live_for_branch(branch_id, now_ms)
            .into_iter()
            .filter(|p| p.device_id != self_id)
            .map(|p| (p.host.clone(), p.port))
            .collect()
    }

    /// The LAN shift-open gate: is a till at this branch advertising a FRESH open
    /// shift? `true` means "the branch is operating" per the most current signal —
    /// it beats the backend, which may not yet know a till closed (or opened) if
    /// that change hasn't synced. A closed till stops advertising and expires within
    /// the TTL, so this reflects reality, not a stale cache.
    pub fn branch_has_open_till(&self, branch_id: &str, now_ms: i64) -> bool {
        self.live_for_branch(branch_id, now_ms)
            .iter()
            .any(|p| p.open_shift_id.is_some())
    }
}

// ── The running relay (async) ─────────────────────────────────────────────────

/// Local wall-clock in ms — relative liveness/heartbeat timing (not order_ref math).
fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// mDNS service type for Sufrix LAN peers.
const SERVICE_TYPE: &str = "_madar._tcp.local.";
/// Default TCP relay port (fixed so a manual hub-IP needs only the host typed in).
pub const DEFAULT_TCP_PORT: u16 = 47600;
/// Default UDP beacon port (shared across a branch's devices).
pub const DEFAULT_BEACON_PORT: u16 = 47601;
/// Cap on a single relay frame (bytes) — refuse oversized lines (abuse guard).
const MAX_FRAME: usize = 256 * 1024;
/// How many recent `msg_id`s to remember for dedup (mesh gossip de-storm).
const SEEN_CAP: usize = 4096;
/// Beacon cadence — re-advertise + heartbeat every few seconds (< PEER_TTL_MS).
const BEACON_EVERY: Duration = Duration::from_millis(3_000);

/// What the relay does with a verified, deduped inbound message — implemented by
/// `MadarCore`: forward it to the unified realtime listener (instant board update)
/// and, for a write event carrying a replay op, mirror that op into the outbox
/// (robustness #4 — the fire reaches the cloud even if the originator dies first).
pub trait LanInbound: Send + Sync {
    fn on_lan_message(&self, msg: &LanMessage);
}

/// Bounded recent-id set for at-least-once dedup across LAN + gossip + cloud.
struct SeenSet {
    ids: HashSet<String>,
    order: VecDeque<String>,
}
impl SeenSet {
    fn new() -> Self {
        Self { ids: HashSet::new(), order: VecDeque::new() }
    }
    /// Record `id`; returns `true` if it was NOT seen before (i.e. process it).
    fn insert(&mut self, id: &str) -> bool {
        if self.ids.contains(id) {
            return false;
        }
        self.ids.insert(id.to_string());
        self.order.push_back(id.to_string());
        if self.order.len() > SEEN_CAP {
            if let Some(old) = self.order.pop_front() {
                self.ids.remove(&old);
            }
        }
        true
    }
}

/// This device's relay identity + crypto + ports.
#[derive(Clone)]
pub struct LanConfig {
    pub device_id: String,
    pub branch_id: String,
    pub role: String,
    pub station_id: Option<String>,
    /// Per-branch HMAC key (from [`branch_key`]).
    pub key: Vec<u8>,
    /// TCP relay port (0 = OS-assigned; read back via [`LanRelay::tcp_port`]).
    pub tcp_port: u16,
    /// UDP beacon port (shared across the branch's devices).
    pub beacon_port: u16,
}

/// The signed UDP discovery/heartbeat beacon. The source IP is taken from
/// `recv_from` (no local-IP detection needed), so the payload carries only the TCP
/// port + identity + the open-shift advert (the LAN shift-open gate's signal).
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Beacon {
    device_id: String,
    branch_id: String,
    role: String,
    station_id: Option<String>,
    open_shift_id: Option<String>,
    tcp_port: u16,
    sent_at_ms: i64,
}

/// State shared with the spawned relay tasks.
struct RelayShared {
    cfg: LanConfig,
    registry: Mutex<PeerRegistry>,
    seen: Mutex<SeenSet>,
    inbound: Arc<dyn LanInbound>,
    /// This till's currently-open shift, advertised in the beacon (or `None`).
    open_shift: Mutex<Option<String>>,
    /// The actually-bound TCP port (after OS assignment), advertised in the beacon.
    bound_tcp_port: Mutex<u16>,
    /// Manually-configured hub peers (`host`, `port`) — the always-works fallback
    /// when discovery is filtered. Unlike registry peers these never TTL-expire; we
    /// always push to them (and receive from them via our own signed server).
    manual: Mutex<Vec<(String, u16)>>,
}

/// A running LAN relay: an embedded TCP server (accepts signed pushes → forwards +
/// gossips), a UDP beacon (discovery + heartbeat + shift advert), optional mDNS, and
/// a TTL pruner. Pure tokio — cancellable, non-blocking, cross-compiles everywhere.
pub struct LanRelay {
    shared: Arc<RelayShared>,
    handles: Mutex<Vec<JoinHandle<()>>>,
    mdns: Mutex<Option<mdns_sd::ServiceDaemon>>,
}

impl LanRelay {
    pub fn new(cfg: LanConfig, inbound: Arc<dyn LanInbound>) -> Self {
        let bound = cfg.tcp_port;
        Self {
            shared: Arc::new(RelayShared {
                cfg,
                registry: Mutex::new(PeerRegistry::new()),
                seen: Mutex::new(SeenSet::new()),
                inbound,
                open_shift: Mutex::new(None),
                bound_tcp_port: Mutex::new(bound),
                manual: Mutex::new(Vec::new()),
            }),
            handles: Mutex::new(Vec::new()),
            mdns: Mutex::new(None),
        }
    }

    /// The actually-bound TCP relay port (meaningful after [`start`](Self::start)).
    pub fn tcp_port(&self) -> u16 {
        *self.shared.bound_tcp_port.lock().unwrap()
    }

    /// Update the advertised open-shift (the LAN shift-open gate's truth). Pass the
    /// open shift's id when this till opens, `None` when it closes.
    pub fn set_open_shift(&self, shift_id: Option<String>) {
        *self.shared.open_shift.lock().unwrap() = shift_id;
    }

    /// Inject a discovered peer (TTL-tracked) — the iOS-Bonjour bridge feeds peers
    /// resolved via Network.framework in here; the host re-injects on each refresh.
    pub fn add_peer(&self, peer: Peer) {
        self.shared.registry.lock().unwrap().upsert(peer);
    }

    /// Register a manual hub peer (`host`, `port`) — the always-works fallback. Never
    /// TTL-expires; we always push to it. Idempotent (no duplicate host:port).
    pub fn add_manual_hub(&self, host: String, port: u16) {
        let mut m = self.shared.manual.lock().unwrap();
        if !m.iter().any(|(h, p)| *h == host && *p == port) {
            m.push((host, port));
        }
    }

    /// Count of currently-live discovered peers + manual hubs (a diagnostics signal).
    pub fn peer_count(&self) -> u32 {
        let live = self
            .shared
            .registry
            .lock()
            .unwrap()
            .live_for_branch(&self.shared.cfg.branch_id, now_ms())
            .len();
        let manual = self.shared.manual.lock().unwrap().len();
        (live + manual) as u32
    }

    /// The LAN shift-open gate for this branch (a fresh-advertising open till).
    pub fn branch_has_open_till(&self) -> bool {
        self.shared
            .registry
            .lock()
            .unwrap()
            .branch_has_open_till(&self.shared.cfg.branch_id, now_ms())
    }

    /// Start the embedded server + discovery + pruner. Binds the TCP relay (fatal if
    /// it can't); the beacon + mDNS degrade gracefully (logged, not fatal) so a relay
    /// still works on a network that filters one discovery layer but not unicast.
    pub async fn start(&self) -> Result<(), CoreError> {
        let listener = TcpListener::bind(("0.0.0.0", self.shared.cfg.tcp_port))
            .await
            .map_err(|e| CoreError::Internal { detail: format!("lan tcp bind: {e}") })?;
        if let Ok(addr) = listener.local_addr() {
            *self.shared.bound_tcp_port.lock().unwrap() = addr.port();
        }

        // 1. TCP relay accept loop.
        let shared = self.shared.clone();
        self.spawn(tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let s = shared.clone();
                        tokio::spawn(async move { handle_conn(s, stream).await });
                    }
                    Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
                }
            }
        }));

        // 2. UDP beacon (send + receive). Best-effort.
        if let Ok(sock) = bind_beacon(self.shared.cfg.beacon_port).await {
            let sock = Arc::new(sock);
            let send_shared = self.shared.clone();
            let send_sock = sock.clone();
            self.spawn(tokio::spawn(async move {
                beacon_send_loop(send_shared, send_sock).await;
            }));
            let recv_shared = self.shared.clone();
            self.spawn(tokio::spawn(async move {
                beacon_recv_loop(recv_shared, sock).await;
            }));
        }

        // 3. mDNS advertise + browse. Best-effort.
        self.start_mdns();

        // 4. TTL pruner.
        let prune_shared = self.shared.clone();
        self.spawn(tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(4_000)).await;
                prune_shared.registry.lock().unwrap().prune(now_ms());
            }
        }));

        Ok(())
    }

    /// Publish a local event to every discovered peer (hop 0). The originator records
    /// its own `msg_id` so a gossip bounce-back is deduped. Fire-and-forget per peer.
    /// `replay_op` carries the `/sync/replay` envelope for a write (mirror-relay), or
    /// `None` for a pure display trigger.
    pub async fn publish(
        &self,
        topic: &str,
        event_type: &str,
        data: String,
        replay_op: Option<String>,
        sent_at_ms: i64,
    ) {
        let msg = LanMessage {
            msg_id: uuid::Uuid::new_v4().to_string(),
            branch_id: self.shared.cfg.branch_id.clone(),
            topic: topic.to_string(),
            event_type: event_type.to_string(),
            data,
            hop: 0,
            sender_id: self.shared.cfg.device_id.clone(),
            sent_at_ms,
            replay_op,
        };
        self.shared.seen.lock().unwrap().insert(&msg.msg_id);
        let frame = sign_frame(&self.shared.cfg.key, &msg);
        if let Ok(json) = serde_json::to_string(&frame) {
            fanout(&self.shared, format!("MSG {json}")).await;
        }
    }

    /// Abort every spawned task + shut the mDNS daemon — idempotent.
    pub fn stop(&self) {
        for h in self.handles.lock().unwrap().drain(..) {
            h.abort();
        }
        if let Some(d) = self.mdns.lock().unwrap().take() {
            let _ = d.shutdown();
        }
    }

    fn spawn(&self, h: JoinHandle<()>) {
        self.handles.lock().unwrap().push(h);
    }

    /// Advertise `_madar._tcp` + browse for peers, feeding resolved services into the
    /// registry. Unsigned TXT (discovery only) — message acceptance is still HMAC-gated,
    /// and the shift gate trusts only the SIGNED beacon, so mDNS can't spoof either.
    fn start_mdns(&self) {
        let daemon = match mdns_sd::ServiceDaemon::new() {
            Ok(d) => d,
            Err(_) => return,
        };
        let port = self.tcp_port();
        let props: HashMap<String, String> = [
            ("device_id", self.shared.cfg.device_id.as_str()),
            ("branch_id", self.shared.cfg.branch_id.as_str()),
            ("role", self.shared.cfg.role.as_str()),
            ("station_id", self.shared.cfg.station_id.as_deref().unwrap_or("")),
            ("tcp_port", &port.to_string()),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let instance = format!("sufrix-{}", self.shared.cfg.device_id);
        let host = format!("{}.local.", self.shared.cfg.device_id);
        if let Ok(info) =
            mdns_sd::ServiceInfo::new(SERVICE_TYPE, &instance, &host, "", port, props)
                .map(|i| i.enable_addr_auto())
        {
            let _ = daemon.register(info);
        }
        if let Ok(rx) = daemon.browse(SERVICE_TYPE) {
            let shared = self.shared.clone();
            self.spawn(tokio::spawn(async move {
                while let Ok(event) = rx.recv_async().await {
                    if let mdns_sd::ServiceEvent::ServiceResolved(info) = event {
                        ingest_mdns(&shared, &info);
                    }
                }
            }));
        }
        *self.mdns.lock().unwrap() = Some(daemon);
    }
}

impl Drop for LanRelay {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Fan a signed line out to every live same-branch peer (excluding self), each send
/// independent so a slow/dead peer can't stall the others.
async fn fanout(shared: &Arc<RelayShared>, line: String) {
    let mut targets = {
        let reg = shared.registry.lock().unwrap();
        reg.relay_targets(&shared.cfg.branch_id, &shared.cfg.device_id, now_ms())
    };
    // Always include manual hubs (no TTL), de-duped against discovered peers.
    for (host, port) in shared.manual.lock().unwrap().iter() {
        if !targets.iter().any(|(h, p)| h == host && p == port) {
            targets.push((host.clone(), *port));
        }
    }
    for (host, port) in targets {
        let line = line.clone();
        tokio::spawn(async move {
            send_frame(&host, port, &line).await;
        });
    }
}

/// One outbound push: connect, write `line\n`, done. Bounded by a short timeout so a
/// dead peer fails fast (LAN round-trips are sub-ms).
async fn send_frame(host: &str, port: u16, line: &str) {
    let attempt = async {
        let mut stream = TcpStream::connect((host, port)).await.ok()?;
        stream.write_all(line.as_bytes()).await.ok()?;
        stream.write_all(b"\n").await.ok()?;
        stream.flush().await.ok()?;
        Some(())
    };
    let _ = tokio::time::timeout(Duration::from_millis(1_500), attempt).await;
}

/// Handle one inbound relay connection: read one `VERB payload` line and dispatch.
async fn handle_conn(shared: Arc<RelayShared>, mut stream: TcpStream) {
    let Some(line) = read_line(&mut stream).await else { return };
    let (verb, payload) = line.split_once(' ').unwrap_or((line.as_str(), ""));
    match verb {
        "MSG" => {
            handle_msg(&shared, payload).await;
            let _ = stream.write_all(b"ok\n").await;
        }
        "PING" => {
            let _ = stream.write_all(b"pong\n").await;
        }
        _ => {
            let _ = stream.write_all(b"err\n").await;
        }
    }
}

/// Verify, branch-gate, dedup, forward to the host, then gossip one hop further.
async fn handle_msg(shared: &Arc<RelayShared>, payload: &str) {
    let Ok(frame) = serde_json::from_str::<SignedFrame>(payload) else { return };
    let Some(msg) = verify_frame(&shared.cfg.key, &frame) else { return };
    // Branch isolation + ignore our own gossip echo.
    if msg.branch_id != shared.cfg.branch_id || msg.sender_id == shared.cfg.device_id {
        return;
    }
    let is_new = shared.seen.lock().unwrap().insert(&msg.msg_id);
    if !is_new {
        return;
    }
    shared.inbound.on_lan_message(&msg);
    // Mesh gossip: re-relay one hop so it reaches peers we can't directly see.
    if let Some(relayed) = msg.relayed() {
        let frame = sign_frame(&shared.cfg.key, &relayed);
        if let Ok(json) = serde_json::to_string(&frame) {
            fanout(shared, format!("MSG {json}")).await;
        }
    }
}

/// Read a single `\n`-terminated line (frames are single-line JSON), capped.
async fn read_line(stream: &mut TcpStream) -> Option<String> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 2048];
    loop {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        if let Some(pos) = chunk[..n].iter().position(|&b| b == b'\n') {
            buf.extend_from_slice(&chunk[..pos]);
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > MAX_FRAME {
            return None;
        }
    }
    if buf.is_empty() {
        return None;
    }
    String::from_utf8(buf).ok()
}

/// Bind the UDP beacon socket (broadcast-enabled). Fails gracefully (Err → no beacon).
async fn bind_beacon(port: u16) -> std::io::Result<UdpSocket> {
    let sock = UdpSocket::bind(("0.0.0.0", port)).await?;
    sock.set_broadcast(true)?;
    Ok(sock)
}

/// Broadcast a signed beacon every [`BEACON_EVERY`] — discovery + heartbeat + the
/// open-shift advert (refreshed each tick so a closed till stops counting fast).
async fn beacon_send_loop(shared: Arc<RelayShared>, sock: Arc<UdpSocket>) {
    loop {
        let beacon = Beacon {
            device_id: shared.cfg.device_id.clone(),
            branch_id: shared.cfg.branch_id.clone(),
            role: shared.cfg.role.clone(),
            station_id: shared.cfg.station_id.clone(),
            open_shift_id: shared.open_shift.lock().unwrap().clone(),
            tcp_port: *shared.bound_tcp_port.lock().unwrap(),
            sent_at_ms: now_ms(),
        };
        if let Ok(body) = serde_json::to_string(&beacon) {
            let frame = sign_str(&shared.cfg.key, body);
            if let Ok(json) = serde_json::to_string(&frame) {
                let _ = sock
                    .send_to(json.as_bytes(), ("255.255.255.255", shared.cfg.beacon_port))
                    .await;
            }
        }
        tokio::time::sleep(BEACON_EVERY).await;
    }
}

/// Receive beacons; verify the signature + branch, then upsert the peer using the
/// SOURCE IP (no local-IP detection needed) and the advertised TCP port.
async fn beacon_recv_loop(shared: Arc<RelayShared>, sock: Arc<UdpSocket>) {
    let mut buf = vec![0u8; 8192];
    loop {
        let Ok((n, src)) = sock.recv_from(&mut buf).await else {
            tokio::time::sleep(Duration::from_millis(200)).await;
            continue;
        };
        let Ok(text) = std::str::from_utf8(&buf[..n]) else { continue };
        let Ok(frame) = serde_json::from_str::<SignedFrame>(text) else { continue };
        let Some(body) = verify_str(&shared.cfg.key, &frame) else { continue };
        let Ok(beacon) = serde_json::from_str::<Beacon>(body) else { continue };
        if beacon.branch_id != shared.cfg.branch_id || beacon.device_id == shared.cfg.device_id {
            continue;
        }
        shared.registry.lock().unwrap().upsert(Peer {
            device_id: beacon.device_id,
            branch_id: beacon.branch_id,
            role: beacon.role,
            host: src.ip().to_string(),
            port: beacon.tcp_port,
            station_id: beacon.station_id,
            open_shift_id: beacon.open_shift_id,
            last_seen_ms: now_ms(),
        });
    }
}

/// Fold an mDNS-resolved service into the registry (discovery only — no shift advert;
/// only the signed beacon sets `open_shift_id`).
fn ingest_mdns(shared: &Arc<RelayShared>, info: &mdns_sd::ServiceInfo) {
    let prop = |k: &str| info.get_property_val_str(k).map(|s| s.to_string());
    let branch_id = prop("branch_id").unwrap_or_default();
    let device_id = prop("device_id").unwrap_or_default();
    if branch_id != shared.cfg.branch_id || device_id.is_empty() || device_id == shared.cfg.device_id
    {
        return;
    }
    let Some(host) = info.get_addresses_v4().iter().next().map(|ip| ip.to_string()) else { return };
    let port =
        prop("tcp_port").and_then(|p| p.parse::<u16>().ok()).unwrap_or_else(|| info.get_port());
    shared.registry.lock().unwrap().upsert(Peer {
        device_id,
        branch_id,
        role: prop("role").unwrap_or_default(),
        host,
        port,
        station_id: prop("station_id").filter(|s| !s.is_empty()),
        open_shift_id: None,
        last_seen_ms: now_ms(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: &str, branch: &str) -> LanMessage {
        LanMessage {
            msg_id: id.into(),
            branch_id: branch.into(),
            topic: "kitchen".into(),
            event_type: "kitchen.fired".into(),
            data: r#"{"id":"k1","items":[]}"#.into(),
            hop: 0,
            sender_id: "devA".into(),
            sent_at_ms: 1_700_000_000_000,
            replay_op: None,
        }
    }

    #[test]
    fn hex_roundtrips() {
        let bytes = vec![0u8, 1, 15, 16, 127, 128, 255];
        assert_eq!(from_hex(&to_hex(&bytes)).unwrap(), bytes);
        assert!(from_hex("xyz").is_none());
        assert!(from_hex("abc").is_none()); // odd length
    }

    #[test]
    fn sign_then_verify_roundtrips() {
        let key = branch_key("00ff00ff00ff00ff", "branch-1");
        let m = msg("m1", "branch-1");
        let frame = sign_frame(&key, &m);
        assert_eq!(verify_frame(&key, &frame), Some(m));
    }

    #[test]
    fn verify_rejects_tampered_body() {
        let key = branch_key("deadbeef", "branch-1");
        let mut frame = sign_frame(&key, &msg("m1", "branch-1"));
        frame.msg = frame.msg.replace("kitchen.fired", "kitchen.voided");
        assert!(verify_frame(&key, &frame).is_none(), "tampered body fails HMAC");
    }

    #[test]
    fn verify_rejects_foreign_branch_key() {
        // A different branch derives a different key from the same org secret, so it
        // can neither forge a message this branch accepts nor read this one's intent.
        let key_a = branch_key("cafebabecafebabe", "branch-A");
        let key_b = branch_key("cafebabecafebabe", "branch-B");
        let frame = sign_frame(&key_a, &msg("m1", "branch-A"));
        assert!(verify_frame(&key_b, &frame).is_none(), "branch-B key rejects branch-A's frame");
        assert!(verify_frame(&key_a, &frame).is_some(), "branch-A key accepts its own");
    }

    #[test]
    fn branch_key_is_deterministic_and_branch_specific() {
        assert_eq!(branch_key("aa", "b1"), branch_key("aa", "b1"), "same inputs → same key");
        assert_ne!(branch_key("aa", "b1"), branch_key("aa", "b2"), "branch-scoped");
        assert_ne!(branch_key("aa", "b1"), branch_key("bb", "b1"), "secret-scoped");
    }

    #[test]
    fn gossip_hop_bounds_relay() {
        let mut m = msg("m1", "b1");
        let mut relays = 0;
        while let Some(next) = m.relayed() {
            m = next;
            relays += 1;
            if relays > 100 {
                break;
            }
        }
        assert_eq!(relays, MAX_HOPS as i32, "relay stops at MAX_HOPS");
        assert!(m.hops_exhausted());
    }

    fn peer(id: &str, branch: &str, open_shift: Option<&str>, last_seen_ms: i64) -> Peer {
        Peer {
            device_id: id.into(),
            branch_id: branch.into(),
            role: "kitchen".into(),
            host: format!("10.0.0.{}", id.len()),
            port: 7777,
            station_id: None,
            open_shift_id: open_shift.map(|s| s.into()),
            last_seen_ms,
        }
    }

    #[test]
    fn registry_prunes_and_reads_by_ttl() {
        let now = 1_000_000;
        let mut reg = PeerRegistry::new();
        reg.upsert(peer("fresh", "b1", None, now - 1_000));
        reg.upsert(peer("stale", "b1", None, now - (PEER_TTL_MS + 1)));
        // Read-time TTL hides the stale peer even before prune.
        assert_eq!(reg.live_for_branch("b1", now).len(), 1);
        reg.prune(now);
        assert_eq!(reg.live_for_branch("b1", now).len(), 1, "stale evicted");
        assert!(reg.live_for_branch("b1", now).iter().any(|p| p.device_id == "fresh"));
    }

    #[test]
    fn relay_targets_exclude_self_and_other_branches() {
        let now = 1_000_000;
        let mut reg = PeerRegistry::new();
        reg.upsert(peer("self", "b1", None, now));
        reg.upsert(peer("peer", "b1", None, now));
        reg.upsert(peer("other", "b2", None, now));
        let targets = reg.relay_targets("b1", "self", now);
        assert_eq!(targets.len(), 1, "only the same-branch non-self peer");
    }

    #[test]
    fn shift_gate_tracks_fresh_open_tills_only() {
        let now = 1_000_000;
        let mut reg = PeerRegistry::new();
        assert!(!reg.branch_has_open_till("b1", now), "no peers → closed");
        // A KDS (no shift) doesn't make the branch "operating".
        reg.upsert(peer("kds", "b1", None, now));
        assert!(!reg.branch_has_open_till("b1", now));
        // A till advertising an open shift does.
        reg.upsert(peer("till", "b1", Some("shift-1"), now));
        assert!(reg.branch_has_open_till("b1", now));
        // …until its advert goes stale (the till closed and stopped advertising).
        reg.upsert(peer("till", "b1", Some("shift-1"), now - (PEER_TTL_MS + 1)));
        assert!(!reg.branch_has_open_till("b1", now), "stale open-till advert no longer counts");
    }

    // ── Live relay (loopback) ────────────────────────────────────────────────

    struct Recorder(Mutex<Vec<LanMessage>>);
    impl LanInbound for Recorder {
        fn on_lan_message(&self, msg: &LanMessage) {
            self.0.lock().unwrap().push(msg.clone());
        }
    }
    fn rec() -> Arc<Recorder> {
        Arc::new(Recorder(Mutex::new(Vec::new())))
    }
    fn lan_cfg(id: &str, key: Vec<u8>) -> LanConfig {
        // beacon_port 0 → OS-assigned, so two relays in one test never collide; the
        // tests drive delivery via manual peer injection, not the beacon.
        LanConfig {
            device_id: id.into(),
            branch_id: "b1".into(),
            role: "kitchen".into(),
            station_id: None,
            key,
            tcp_port: 0,
            beacon_port: 0,
        }
    }
    fn loopback_peer(id: &str, port: u16) -> Peer {
        Peer {
            device_id: id.into(),
            branch_id: "b1".into(),
            role: "kitchen".into(),
            host: "127.0.0.1".into(),
            port,
            station_id: None,
            open_shift_id: None,
            last_seen_ms: now_ms(),
        }
    }
    async fn wait_until<F: Fn() -> bool>(f: F) {
        for _ in 0..100 {
            if f() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
    }

    #[tokio::test]
    async fn relay_delivers_signed_message_to_a_peer() {
        let key = branch_key("aabbccdd", "b1");
        let rec_b = rec();
        let a = LanRelay::new(lan_cfg("A", key.clone()), rec());
        let b = LanRelay::new(lan_cfg("B", key.clone()), rec_b.clone());
        a.start().await.unwrap();
        b.start().await.unwrap();
        a.add_peer(loopback_peer("B", b.tcp_port()));

        a.publish("kitchen", "kitchen.fired", r#"{"id":"k1"}"#.into(), None, now_ms()).await;
        wait_until(|| !rec_b.0.lock().unwrap().is_empty()).await;

        let got = rec_b.0.lock().unwrap();
        assert_eq!(got.len(), 1, "B received exactly one message");
        assert_eq!(got[0].event_type, "kitchen.fired");
        assert_eq!(got[0].sender_id, "A");
        assert_eq!(got[0].data, r#"{"id":"k1"}"#);
    }

    #[tokio::test]
    async fn relay_rejects_foreign_branch_key_on_the_wire() {
        // B holds a different secret → a different branch key → it must drop A's frame.
        let rec_b = rec();
        let a = LanRelay::new(lan_cfg("A", branch_key("1111", "b1")), rec());
        let b = LanRelay::new(lan_cfg("B", branch_key("2222", "b1")), rec_b.clone());
        a.start().await.unwrap();
        b.start().await.unwrap();
        a.add_peer(loopback_peer("B", b.tcp_port()));

        a.publish("kitchen", "kitchen.fired", "{}".into(), None, now_ms()).await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(rec_b.0.lock().unwrap().is_empty(), "foreign-key frame is rejected, not delivered");
    }

    #[tokio::test]
    async fn relay_dedups_a_repeated_msg_id() {
        let key = branch_key("aabbccdd", "b1");
        let rec_b = rec();
        let b = LanRelay::new(lan_cfg("B", key.clone()), rec_b.clone());
        b.start().await.unwrap();
        let m = LanMessage {
            msg_id: "dup-1".into(),
            branch_id: "b1".into(),
            topic: "kitchen".into(),
            event_type: "kitchen.fired".into(),
            data: "{}".into(),
            hop: 0,
            sender_id: "A".into(),
            sent_at_ms: now_ms(),
            replay_op: None,
        };
        let line = format!("MSG {}", serde_json::to_string(&sign_frame(&key, &m)).unwrap());
        send_frame("127.0.0.1", b.tcp_port(), &line).await;
        send_frame("127.0.0.1", b.tcp_port(), &line).await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(rec_b.0.lock().unwrap().len(), 1, "the second identical msg_id is deduped");
    }
}
