//! The client side of the unified realtime bus (PLAN §"unified realtime bus").
//!
//! ONE SSE connection per device, opened against `GET /realtime/stream?branch_id&
//! topics=…`. The backend topic-multiplexes + permission-filters; the device asks
//! only for the topics its role/mode needs (delivery, tickets, kitchen, orders).
//! Events arrive as `event: <type>\ndata: <json>\n\n` frames; we hand each to the
//! host through ONE UniFFI callback listener (modeled on `session::TokenStore`).
//!
//! openapi-generator emits no usable `text/event-stream` method, so the transport
//! is hand-rolled here on `reqwest::Response::bytes_stream()` (the streaming client
//! in `net.rs` has no total timeout — see `ApiClient::realtime_client`).
//!
//! The supervisor is a single tokio task: connect → parse → dispatch; on drop of
//! the stream, flip the host to "disconnected", back off (jittered, shared with the
//! outbox), and reconnect, resuming from `Last-Event-ID`. A 401 stops the task (the
//! token is gone) WITHOUT touching the outbox's `auth_paused`. Offline simply means
//! repeated reconnect attempts; the UI shows its last cached snapshot meanwhile.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use futures_util::StreamExt;

use crate::error::{CoreError, CoreResult};

/// One decoded realtime frame handed to the host. `event_type` is the SSE `event:`
/// field (`delivery.updated`, `ticket.fired`, `kitchen.item_bumped`, …) and `data`
/// is the raw JSON `data:` payload — the host (or a thin core projection) matches on
/// `event_type` and decodes `data` for the topic it cares about. One shape for every
/// topic keeps the FFI surface tiny.
#[derive(uniffi::Record, Clone, Debug)]
pub struct RealtimeEvent {
    pub event_type: String,
    pub data: String,
}

/// The host's realtime sink — ONE per device, like `TokenStore`. The core calls
/// `on_event` as frames arrive and `on_connection_changed` on every connect/drop.
/// Implementations MUST return promptly (hop to the UI thread and return) — the
/// call runs on the supervisor task and blocking it stalls the stream.
#[uniffi::export(callback_interface)]
pub trait EventListener: Send + Sync {
    fn on_event(&self, event: RealtimeEvent);
    fn on_connection_changed(&self, connected: bool);
}

/// The host's thin platform-primitive sink for realtime ALERTS — the ONLY things a
/// host can do that the core can't: play the bundled ping sound, post an OS local
/// notification, and fire a haptic. NO decision logic lives here: the CORE decides
/// WHEN to alert (which events, deduped) and builds the localized title/body — the
/// host just performs the primitive. Like `EventListener`, calls must return promptly.
#[uniffi::export(callback_interface)]
pub trait RealtimePlayer: Send + Sync {
    /// Play the short "new work" ping (the host's bundled sound asset).
    fn play_ping(&self);
    /// Post / replace a local OS notification. `tag` identifies it (re-posting the
    /// same tag replaces, so an order's create→update doesn't stack notifications).
    fn post_notification(&self, title: String, body: String, tag: String);
    /// Fire a confirmation haptic (a no-op on platforms without one, e.g. desktop).
    fn haptic(&self);
}

/// The topics the device's ROLE needs on its ONE session-level subscription — the
/// single source of truth (the hosts no longer choose). A till sees delivery + the
/// kitchen queue + tickets-to-settle + orders; a waiter sees its tickets + the
/// kitchen; a KDS sees the kitchen. The backend still intersects with granted topics.
pub fn topics_for_role(role: &str) -> Vec<String> {
    match role {
        "kitchen" => vec!["kitchen".into()],
        "waiter" => vec!["tickets".into(), "kitchen".into()],
        // teller / till (and any other operating role): the full set.
        _ => vec!["delivery".into(), "kitchen".into(), "tickets".into(), "orders".into()],
    }
}

/// A localized alert the core decided to raise for an inbound event.
struct Alert {
    title: String,
    body: String,
    /// Stable per-(event-kind, entity) tag — the OS-notification id AND the dedup key.
    tag: String,
}

/// Decide whether an event warrants an alert (ping + notification), and build its
/// localized title/body. Only NEW-work events alert — a fire/round/new-delivery/
/// ready; bumps, settles, voids and plain updates refresh the board silently.
/// Pure + unit-tested. Returns `None` for non-alert events or undecodable data.
fn alert_for(event_type: &str, data: &str, locale: &str) -> Option<Alert> {
    let key = match event_type {
        "delivery.created" => "notif.new_delivery",
        "ticket.fired" => "notif.new_ticket",
        "ticket.round_added" => "notif.new_round",
        "kitchen.fired" => "notif.new_kitchen",
        "kitchen.ticket_ready" | "ticket.ready" => "notif.ready",
        _ => return None,
    };
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    let pick = |keys: &[&str]| -> Option<String> {
        keys.iter()
            .find_map(|k| v.get(k).and_then(|x| x.as_str()))
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
    };
    let id = pick(&["id", "order_id", "open_ticket_id", "msg_id"]).unwrap_or_default();
    let reff = pick(&["delivery_ref", "ticket_ref", "kitchen_ref", "order_ref", "ref"]);
    let who = pick(&["customer_name", "table_label", "name"]);
    let body = [reff, who].into_iter().flatten().collect::<Vec<_>>().join(" · ");
    Some(Alert { title: crate::i18n::tr(locale, key), body, tag: format!("{event_type}:{id}") })
}

/// Per-role alert relevance: a device pings/notifies only for events that are
/// INCOMING work for its role, never the work it produces itself. A waiter FIRES
/// tickets (and the kitchen tickets behind them), so a fire is not "new work" to
/// them — only "ready" (food up → go serve) is. The kitchen COOKS, so only a new
/// kitchen ticket (`kitchen.fired`) is new work — not its own ready/bump stamps.
/// Teller / till RECEIVE delivery + tickets-to-settle + ready → everything alerts.
fn role_wants_alert(event_type: &str, role: &str) -> bool {
    match role {
        "waiter" => matches!(event_type, "ticket.ready" | "kitchen.ticket_ready"),
        "kitchen" => event_type == "kitchen.fired",
        // Teller / cashier / manager: incoming work to settle / handle — but NOT
        // `kitchen.fired`. That's the kitchen device's new-work signal; one waiter
        // fire emits BOTH `ticket.fired` (the open ticket) and `kitchen.fired` (the
        // kitchen copy), so alerting on both double-pinged the till for one order.
        // The till already gets `ticket.fired`; the kitchen copy is not its concern.
        _ => event_type != "kitchen.fired",
    }
}

/// Bounded recent-tag set so a re-delivered alert (LAN + cloud both carry it, or a
/// quick reconnect) fires only once.
struct AlertDedup {
    seen: std::collections::HashSet<String>,
    order: std::collections::VecDeque<String>,
}
impl AlertDedup {
    fn new() -> Self {
        Self { seen: std::collections::HashSet::new(), order: std::collections::VecDeque::new() }
    }
    /// `true` if `tag` was NOT seen before (→ raise the alert).
    fn insert(&mut self, tag: &str) -> bool {
        if self.seen.contains(tag) {
            return false;
        }
        self.seen.insert(tag.to_string());
        self.order.push_back(tag.to_string());
        if self.order.len() > 512 {
            if let Some(old) = self.order.pop_front() {
                self.seen.remove(&old);
            }
        }
        true
    }
}

/// Wraps the host's `EventListener` so every realtime event (cloud SSE OR LAN relay)
/// (1) reaches the host to refresh its board, AND (2) — for a NEW, alert-worthy event
/// — fires the host's `RealtimePlayer` (ping + notification + haptic). All the policy
/// (which events alert, dedup, localized text) is here in the core; the host's player
/// is pure platform primitive.
pub(crate) struct AlertingListener {
    inner: Arc<dyn EventListener>,
    player: Arc<dyn RealtimePlayer>,
    locale: Arc<RwLock<String>>,
    /// The signed-in role — gates which events are "incoming work" worth a ping.
    role: String,
    dedup: std::sync::Mutex<AlertDedup>,
}

impl AlertingListener {
    pub(crate) fn new(
        inner: Arc<dyn EventListener>,
        player: Arc<dyn RealtimePlayer>,
        locale: Arc<RwLock<String>>,
        role: String,
    ) -> Self {
        Self { inner, player, locale, role, dedup: std::sync::Mutex::new(AlertDedup::new()) }
    }
}

impl EventListener for AlertingListener {
    fn on_event(&self, event: RealtimeEvent) {
        // 1. Always refresh the host's board (the list updates regardless of alerts).
        self.inner.on_event(event.clone());
        // 2. Raise an alert ONLY for a NEW, alert-worthy event that is incoming work
        //    for THIS role (a waiter doesn't ping on the tickets it fires itself).
        if !role_wants_alert(&event.event_type, &self.role) {
            return;
        }
        let locale = self.locale.read().map(|g| g.clone()).unwrap_or_default();
        if let Some(alert) = alert_for(&event.event_type, &event.data, &locale) {
            let fresh = self.dedup.lock().map(|mut d| d.insert(&alert.tag)).unwrap_or(true);
            if fresh {
                self.player.play_ping();
                self.player.post_notification(alert.title, alert.body, alert.tag);
                self.player.haptic();
            }
        }
    }
    fn on_connection_changed(&self, connected: bool) {
        self.inner.on_connection_changed(connected);
    }
}

/// A self-contained, cloneable handle to the streaming endpoint (the supervisor
/// task owns one; it can't hold the core). Built by `ApiClient::realtime_client`.
#[derive(Clone)]
pub struct RealtimeClient {
    base_url: String,
    http: reqwest::Client,
    bearer: Arc<RwLock<Option<String>>>,
}

impl RealtimeClient {
    pub(crate) fn new(
        base_url: String,
        http: reqwest::Client,
        bearer: Arc<RwLock<Option<String>>>,
    ) -> Self {
        Self { base_url, http, bearer }
    }

    /// Open the live SSE response, attaching the current bearer, `Accept:
    /// text/event-stream`, and an optional `Last-Event-ID` resume header. A non-2xx
    /// status maps to a `CoreError` so the supervisor stops on 401 / backs off else.
    async fn open(
        &self,
        branch_id: &str,
        topics_csv: &str,
        last_event_id: Option<&str>,
    ) -> CoreResult<reqwest::Response> {
        let url = format!("{}/realtime/stream", self.base_url);
        let mut rb = self
            .http
            .request(reqwest::Method::GET, &url)
            .query(&[("branch_id", branch_id), ("topics", topics_csv)])
            .header(reqwest::header::ACCEPT, "text/event-stream");
        if let Some(tok) = self.bearer.read().unwrap_or_else(|e| e.into_inner()).clone() {
            rb = rb.bearer_auth(tok);
        }
        if let Some(id) = last_event_id {
            rb = rb.header("Last-Event-ID", id);
        }
        let resp = rb.send().await.map_err(|e| crate::net::classify_reqwest(&e))?;
        let status = resp.status();
        if status.is_success() {
            Ok(resp)
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(crate::net::status_to_error(status.as_u16(), &body))
        }
    }
}

/// A live subscription's stop handle. Dropping it (or calling `stop`) aborts the
/// supervisor task; the host's listener simply stops receiving events. Held by the
/// core in a `Mutex<Option<StreamHandle>>`; replacing or clearing it tears down the
/// previous stream so there is never more than ONE connection per device.
pub(crate) struct StreamHandle {
    abort: tokio::task::AbortHandle,
}

impl StreamHandle {
    pub(crate) fn stop(self) {
        self.abort.abort();
    }
}

impl Drop for StreamHandle {
    fn drop(&mut self) {
        self.abort.abort();
    }
}

/// Spawn the supervisor task for one subscription and return its stop handle. The
/// task reconnects forever (jittered backoff) until aborted or a 401 ends it.
pub(crate) fn spawn_supervisor(
    client: RealtimeClient,
    branch_id: String,
    topics: Vec<String>,
    listener: std::sync::Arc<dyn EventListener>,
) -> StreamHandle {
    let topics_csv = topics.join(",");
    // Deterministic per-subscription jitter seed (no RNG dep): hash the branch.
    let seed = branch_id.bytes().fold(0i64, |a, b| a.wrapping_mul(31).wrapping_add(b as i64));
    let handle = tokio::spawn(async move {
        run_supervisor(client, branch_id, topics_csv, listener, seed).await;
    });
    StreamHandle { abort: handle.abort_handle() }
}

/// The reconnect loop. Each pass: connect, drain frames to the listener until the
/// stream drops, flip to disconnected, then back off (unless a 401 stopped us).
async fn run_supervisor(
    client: RealtimeClient,
    branch_id: String,
    topics_csv: String,
    listener: std::sync::Arc<dyn EventListener>,
    seed: i64,
) {
    let mut attempt: i64 = 0;
    let mut last_event_id: Option<String> = None;
    loop {
        match client.open(&branch_id, &topics_csv, last_event_id.as_deref()).await {
            Ok(resp) => {
                attempt = 0;
                listener.on_connection_changed(true);
                drain_stream(resp, listener.as_ref(), &mut last_event_id).await;
                listener.on_connection_changed(false);
            }
            // The token is gone — stop. Do NOT touch the outbox's `auth_paused`;
            // a fresh login + re-subscribe revives the stream.
            Err(CoreError::Unauthenticated { .. }) => {
                listener.on_connection_changed(false);
                return;
            }
            // Offline / 5xx / portal — back off and retry.
            Err(_) => {
                listener.on_connection_changed(false);
            }
        }
        attempt += 1;
        let backoff = compute_stream_backoff_ms(attempt, seed);
        tokio::time::sleep(Duration::from_millis(backoff as u64)).await;
    }
}

/// Drive `bytes_stream()` through the SSE parser, dispatching each decoded event to
/// the listener and tracking the latest `id:` for resume. Returns when the stream
/// ends or errors (the supervisor reconnects).
async fn drain_stream(
    resp: reqwest::Response,
    listener: &dyn EventListener,
    last_event_id: &mut Option<String>,
) {
    let mut stream = resp.bytes_stream();
    let mut parser = SseParser::new();
    while let Some(chunk) = stream.next().await {
        let bytes = match chunk {
            Ok(b) => b,
            Err(_) => break, // read timeout / reset → reconnect
        };
        for frame in parser.push(&bytes) {
            if let Some(id) = frame.id {
                *last_event_id = Some(id);
            }
            // A comment-only keepalive (`: ping`) yields no data → skip.
            if !frame.data.is_empty() {
                listener.on_event(RealtimeEvent { event_type: frame.event_type, data: frame.data });
            }
        }
    }
}

/// Stream-reconnect backoff: BASE·2^(n-1) capped, + deterministic jitter. Kept
/// local (not reusing the outbox constants) so the two subsystems tune apart; the
/// shape mirrors `lib::compute_backoff_ms`.
fn compute_stream_backoff_ms(attempt: i64, seed: i64) -> i64 {
    const BASE: i64 = 1_000;
    const MAX: i64 = 30_000;
    let shift = (attempt.clamp(1, 30) - 1) as u32;
    let exp = BASE.saturating_mul(1i64.checked_shl(shift).unwrap_or(i64::MAX));
    let capped = exp.min(MAX);
    let jitter = (seed.wrapping_add(attempt).wrapping_mul(2_654_435_761)).rem_euclid(1000);
    (capped + jitter).min(MAX)
}

// ── SSE wire parser ───────────────────────────────────────────────────────────

/// One fully-parsed SSE event (after a blank-line dispatch).
#[derive(Debug, PartialEq, Eq)]
struct SseFrame {
    event_type: String,
    data: String,
    id: Option<String>,
}

/// A minimal, chunk-tolerant SSE parser (WHATWG event-stream subset we need). Feed
/// it arbitrary byte chunks (frames may split anywhere); it buffers the incomplete
/// tail and emits a frame per blank line. Handles `event:`/`data:`/`id:` fields,
/// `\n` and `\r\n` line endings, multi-`data:` joining, and `:`-comment keepalives.
struct SseParser {
    /// Bytes after the last newline — an incomplete line awaiting more chunks.
    tail: String,
    /// Accumulated `data:` lines for the in-progress event (joined by `\n`).
    data: String,
    /// The in-progress event's `event:` field (defaults to "message").
    event_type: Option<String>,
    /// The in-progress event's `id:` field, if any.
    id: Option<String>,
    /// Whether any field line has been seen since the last dispatch (so a blank
    /// line between events doesn't emit an empty frame).
    saw_field: bool,
}

impl SseParser {
    fn new() -> Self {
        Self { tail: String::new(), data: String::new(), event_type: None, id: None, saw_field: false }
    }

    /// Feed a chunk; return any events completed by it.
    fn push(&mut self, bytes: &[u8]) -> Vec<SseFrame> {
        self.tail.push_str(&String::from_utf8_lossy(bytes));
        let mut out = Vec::new();
        // Process every complete line (terminated by '\n'); keep the remainder.
        loop {
            let Some(nl) = self.tail.find('\n') else { break };
            let mut line: String = self.tail.drain(..=nl).collect();
            line.pop(); // drop '\n'
            if line.ends_with('\r') {
                line.pop();
            }
            if let Some(frame) = self.feed_line(&line) {
                out.push(frame);
            }
        }
        out
    }

    /// Apply one complete line. A blank line dispatches the buffered event.
    fn feed_line(&mut self, line: &str) -> Option<SseFrame> {
        if line.is_empty() {
            return self.dispatch();
        }
        if line.starts_with(':') {
            return None; // comment / keepalive
        }
        self.saw_field = true;
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""), // field with no value
        };
        match field {
            "data" => {
                self.data.push_str(value);
                self.data.push('\n');
            }
            "event" => self.event_type = Some(value.to_string()),
            "id" => self.id = Some(value.to_string()),
            _ => {} // retry / unknown → ignore
        }
        None
    }

    /// Emit the buffered event (if any) and reset for the next one.
    fn dispatch(&mut self) -> Option<SseFrame> {
        if !self.saw_field {
            return None;
        }
        // A trailing '\n' was appended after the last data line — strip it.
        let mut data = std::mem::take(&mut self.data);
        if data.ends_with('\n') {
            data.pop();
        }
        let frame = SseFrame {
            event_type: self.event_type.take().unwrap_or_else(|| "message".to_string()),
            data,
            id: self.id.clone(), // `id` is sticky across events per spec
        };
        self.saw_field = false;
        Some(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(chunks: &[&str]) -> Vec<SseFrame> {
        let mut p = SseParser::new();
        let mut out = Vec::new();
        for c in chunks {
            out.extend(p.push(c.as_bytes()));
        }
        out
    }

    #[test]
    fn parses_a_simple_event() {
        let f = frames(&["event: ticket.fired\ndata: {\"id\":\"1\"}\n\n"]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].event_type, "ticket.fired");
        assert_eq!(f[0].data, "{\"id\":\"1\"}");
        assert_eq!(f[0].id, None);
    }

    #[test]
    fn default_event_type_is_message() {
        let f = frames(&["data: hi\n\n"]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].event_type, "message");
        assert_eq!(f[0].data, "hi");
    }

    #[test]
    fn tracks_id_and_keeps_it_sticky() {
        let f = frames(&["id: 42\nevent: a\ndata: x\n\n", "event: b\ndata: y\n\n"]);
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].id.as_deref(), Some("42"));
        // `id` persists as the last-seen event id even though the 2nd event omits it.
        assert_eq!(f[1].id.as_deref(), Some("42"));
    }

    #[test]
    fn joins_multiple_data_lines() {
        let f = frames(&["data: line1\ndata: line2\n\n"]);
        assert_eq!(f[0].data, "line1\nline2");
    }

    #[test]
    fn ignores_comment_keepalives() {
        // A `: ping` comment between events must not emit an empty frame.
        let f = frames(&[": ping\n\n", "data: real\n\n"]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].data, "real");
    }

    #[test]
    fn handles_chunks_split_mid_line() {
        // The same event delivered in awkward byte splits parses identically.
        let f = frames(&["eve", "nt: kitchen", ".item_bumped\nda", "ta: {\"x\":1}", "\n\n"]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].event_type, "kitchen.item_bumped");
        assert_eq!(f[0].data, "{\"x\":1}");
    }

    #[test]
    fn handles_crlf_line_endings() {
        let f = frames(&["event: a\r\ndata: b\r\n\r\n"]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].event_type, "a");
        assert_eq!(f[0].data, "b");
    }

    #[test]
    fn field_with_no_value() {
        // `data` with no colon is a valid empty-value field per spec.
        let f = frames(&["data\n\n"]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].data, "");
    }

    #[test]
    fn backoff_grows_and_caps() {
        assert!(compute_stream_backoff_ms(1, 7) >= 1_000);
        assert!(compute_stream_backoff_ms(1, 7) < 2_100);
        assert_eq!(compute_stream_backoff_ms(30, 7), 30_000);
        // Monotonic-ish growth across early attempts (ignoring jitter band).
        assert!(compute_stream_backoff_ms(5, 7) > compute_stream_backoff_ms(2, 7));
    }

    #[test]
    fn topics_are_role_scoped() {
        assert_eq!(topics_for_role("kitchen"), ["kitchen"]);
        assert_eq!(topics_for_role("waiter"), ["tickets", "kitchen"]);
        // teller / unknown → the full operating set.
        assert!(topics_for_role("teller").contains(&"delivery".to_string()));
        assert!(topics_for_role("teller").contains(&"kitchen".to_string()));
    }

    #[test]
    fn only_new_work_events_alert() {
        // Alert-worthy → Some, with the entity ref in the body + a dedup tag.
        let a = alert_for("delivery.created", r#"{"id":"o1","delivery_ref":"D-9","customer_name":"Sam"}"#, "en").unwrap();
        assert_eq!(a.tag, "delivery.created:o1");
        assert!(a.body.contains("D-9") && a.body.contains("Sam"));
        assert!(alert_for("ticket.fired", r#"{"id":"t1","ticket_ref":"T-3"}"#, "en").is_some());
        assert!(alert_for("kitchen.fired", r#"{"id":"k1"}"#, "en").is_some());
        // Silent events → None.
        assert!(alert_for("kitchen.item_bumped", r#"{"id":"k1"}"#, "en").is_none());
        assert!(alert_for("ticket.settled", r#"{"id":"t1"}"#, "en").is_none());
        assert!(alert_for("delivery.updated", r#"{"id":"o1"}"#, "en").is_none());
    }

    #[test]
    fn role_gates_alerts_to_incoming_work() {
        // Teller / till receives incoming work → those alert-worthy events ping…
        assert!(role_wants_alert("ticket.fired", "teller"));
        assert!(role_wants_alert("delivery.created", "teller"));
        assert!(role_wants_alert("ticket.ready", "teller"));
        // …but NOT the kitchen copy of a fire — one waiter fire emits both
        // ticket.fired AND kitchen.fired, and the till must ping only once.
        assert!(!role_wants_alert("kitchen.fired", "teller"));
        // A waiter FIRES tickets → no self-ping on fire/round; only "ready" (serve).
        assert!(!role_wants_alert("ticket.fired", "waiter"));
        assert!(!role_wants_alert("ticket.round_added", "waiter"));
        assert!(!role_wants_alert("kitchen.fired", "waiter"));
        assert!(role_wants_alert("ticket.ready", "waiter"));
        assert!(role_wants_alert("kitchen.ticket_ready", "waiter"));
        // The kitchen COOKS → only a new kitchen ticket; not its own ready stamps.
        assert!(role_wants_alert("kitchen.fired", "kitchen"));
        assert!(!role_wants_alert("kitchen.ticket_ready", "kitchen"));
    }

    #[test]
    fn dedup_alerts_once_per_tag() {
        let mut d = AlertDedup::new();
        assert!(d.insert("delivery.created:o1"), "first → fresh");
        assert!(!d.insert("delivery.created:o1"), "repeat (LAN+cloud) → suppressed");
        assert!(d.insert("delivery.created:o2"), "a different order still alerts");
    }
}
