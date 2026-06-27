//! Kitchen Display System — client side (PLAN §"Kitchen Display System").
//!
//! A KDS device subscribes to the `kitchen` topic on the unified bus (one SSE,
//! `realtime.rs`) and shows the branch's outstanding kitchen tickets — fed by BOTH
//! waiter rounds AND teller counter orders (the source-agnostic substrate). Each
//! station bumps its own lines; a ticket is "ready" when every line is bumped. The
//! seed/refresh list is `GET /kitchen/orders` (cached so the board survives a
//! reconnect); bump/unbump are online-direct writes.
//!
//! This module holds the FFI view DTOs + pure mappers + the feed sort. The exported
//! `MadarCore` methods (list_stations / feed / bump / unbump) live in `lib.rs`.
//!
//! Bump/unbump are OUTBOX-BACKED (Phase E §2): each tap writes a durable replay op
//! first, then drains to `POST /sync/replay` (online-direct when connected), so a
//! network blip never loses a bump. A read-time overlay of the still-pending bumps
//! ([`overlay_pending_bumps`]) reflects the cook's latest tap on the board instantly,
//! even offline, before the op syncs.

use serde::{Deserialize, Serialize};
use madar_api::models;

// ── Outbox command (durable bump intent) ──────────────────────────────────────

/// The payload of a queued bump/unbump op. The direction lives in the outbox
/// `op_type` (`bump_kitchen` / `unbump_kitchen`); the kitchen line `item_id` is the
/// natural idempotency key (re-bumping a bumped line is a server-side no-op).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BumpCommand {
    pub item_id: String,
}

// ── FFI view DTOs ─────────────────────────────────────────────────────────────

/// A kitchen station (Grill, Bar…) for the KDS station picker + chit printing.
#[derive(uniffi::Record, Clone, Debug)]
pub struct KdsStationView {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub is_active: bool,
    /// Wire name of the station's printer brand (e.g. "star", "epson"), if set.
    pub printer_brand: Option<String>,
    pub printer_ip: Option<String>,
    pub printer_port: Option<i32>,
}

/// One outstanding kitchen ticket (a fired waiter round or a teller order).
#[derive(uniffi::Record, Clone, Debug, Serialize, Deserialize)]
pub struct KdsTicketView {
    pub id: String,
    pub kitchen_ref: Option<String>,
    pub table_label: Option<String>,
    pub round_number: i32,
    /// `order` (teller) | `open_ticket` (waiter).
    pub source_type: String,
    /// firing | ready | voided.
    pub status: String,
    pub created_at: String,
    pub items: Vec<KdsLineView>,
}

/// One kitchen line (NO prices — the kitchen copy is slim by design).
#[derive(uniffi::Record, Clone, Debug, Serialize, Deserialize)]
pub struct KdsLineView {
    pub id: String,
    pub name: String,
    pub qty: i32,
    pub size_label: Option<String>,
    pub modifiers: Vec<String>,
    pub notes: Option<String>,
    pub station_id: Option<String>,
    pub station_name: Option<String>,
    pub bumped: bool,
}

// ── Mappers ───────────────────────────────────────────────────────────────────

fn flat<T: Clone>(o: &Option<Option<T>>) -> Option<T> {
    o.as_ref().and_then(|x| x.clone())
}

pub(crate) fn station_view(s: &models::KitchenStation) -> KdsStationView {
    let printer_brand = flat(&s.printer_brand)
        .and_then(|b| serde_json::to_value(b).ok())
        .and_then(|v| v.as_str().map(|x| x.to_string()));
    KdsStationView {
        id: s.id.to_string(),
        name: s.name.clone(),
        is_default: s.is_default,
        is_active: s.is_active,
        printer_brand,
        printer_ip: flat(&s.printer_ip),
        printer_port: flat(&s.printer_port),
    }
}

pub(crate) fn ticket_view(t: &models::KitchenTicketView) -> KdsTicketView {
    KdsTicketView {
        id: t.id.to_string(),
        kitchen_ref: flat(&t.kitchen_ref),
        table_label: flat(&t.table_label),
        round_number: t.round_number,
        source_type: t.source_type.clone(),
        status: t.status.clone(),
        created_at: t.created_at.to_rfc3339(),
        items: t.items.iter().map(line_view).collect(),
    }
}

fn line_view(it: &models::KitchenTicketItemView) -> KdsLineView {
    // The slim `line` JSON is the `KitchenLine` shape (name/qty/size/modifiers/notes).
    let line = it.line.as_ref();
    let s = |k: &str| line.and_then(|l| l.get(k)).and_then(|v| v.as_str()).map(|x| x.to_string());
    let modifiers = line
        .and_then(|l| l.get("modifiers"))
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|m| m.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    KdsLineView {
        id: it.id.to_string(),
        name: s("name").unwrap_or_else(|| "Item".to_string()),
        qty: it.qty,
        size_label: s("size_label"),
        modifiers,
        notes: s("notes"),
        station_id: flat(&it.station_id).map(|u| u.to_string()),
        station_name: flat(&it.station_name),
        bumped: it.bumped,
    }
}

// ── Offline fire projection (Phase E) ─────────────────────────────────────────
//
// When a waiter fires while offline, the KDS must still show the ticket NOW. The
// fire publishes a projection of itself over the LAN; the KDS overlays it on the
// (stale) cached feed until the real server ticket arrives on reconnect. The ids are
// DERIVED from the round's client idempotency key by the SAME rule the backend uses
// (`kitchen::derive_*`), so the projection dedups against the server feed by id and a
// bump on a derived line id reconciles once the fire syncs.

/// Fixed namespace — MUST equal the backend's `KITCHEN_ID_NS` byte-for-byte. Pinned
/// by `id_tests::kitchen_id_derivation_matches_backend`. "madar_kitchen_ns" as bytes.
const KITCHEN_ID_NS: uuid::Uuid = uuid::Uuid::from_u128(0x6d61_6461_725f_6b69_7463_6865_6e5f_6e73);

/// The kitchen-ticket id a fire WILL create, from the round's client idempotency key.
pub(crate) fn derive_kitchen_ticket_id(round_idem: &str) -> Option<String> {
    let seed = uuid::Uuid::parse_str(round_idem).ok()?;
    Some(uuid::Uuid::new_v5(&KITCHEN_ID_NS, seed.as_bytes()).to_string())
}

/// The kitchen-line id for the line at `index` within its derived kitchen ticket.
pub(crate) fn derive_kitchen_item_id(kitchen_ticket_id: &str, index: usize) -> String {
    let kt = uuid::Uuid::parse_str(kitchen_ticket_id).unwrap_or(uuid::Uuid::nil());
    uuid::Uuid::new_v5(&kt, &(index as u32).to_le_bytes()).to_string()
}

/// Build the KDS projection of a just-fired round from the cart lines, with the
/// SAME ids the server will mint (so reconnect dedups it). `None` if the round id
/// isn't a UUID (a non-client fire — no projection to predict).
pub(crate) fn build_fire_projection(
    round_idem: &str,
    lines: &[crate::cart::CartLineView],
    table_label: Option<String>,
    round_number: i32,
    created_at: String,
) -> Option<KdsTicketView> {
    let kt = derive_kitchen_ticket_id(round_idem)?;
    let items = lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let mut modifiers: Vec<String> = l.addons.iter().map(|a| a.name.clone()).collect();
            modifiers.extend(l.optionals.iter().map(|o| o.name.clone()));
            KdsLineView {
                id: derive_kitchen_item_id(&kt, i),
                name: l.name.clone(),
                qty: l.qty as i32,
                size_label: l.size_label.clone(),
                modifiers,
                notes: l.notes.clone(),
                station_id: None,   // routing is server config; unknown offline
                station_name: None,
                bumped: false,
            }
        })
        .collect();
    Some(KdsTicketView {
        id: kt,
        kitchen_ref: None,
        table_label,
        round_number,
        source_type: "open_ticket".into(),
        status: "firing".into(),
        created_at,
        items,
    })
}

/// Overlay LAN-received (un-synced) kitchen tickets onto the server feed: include a
/// projected ticket only when the server feed doesn't already have it (dedup by id —
/// the derived id == the eventual server id), and never a voided one. The host then
/// sees an offline fire instantly; on reconnect the server row replaces it cleanly.
pub(crate) fn overlay_lan_tickets(feed: &mut Vec<KdsTicketView>, lan: Vec<KdsTicketView>) {
    let have: std::collections::HashSet<String> = feed.iter().map(|t| t.id.clone()).collect();
    for t in lan {
        if !have.contains(&t.id) && t.status != "voided" {
            feed.push(t);
        }
    }
}

/// Mark a line bumped/un-bumped across a set of (LAN-overlay) tickets — so a bump
/// relayed over the LAN greys the line on a peer's board too, not just the bumper's.
pub(crate) fn apply_lan_bump(tickets: &mut [KdsTicketView], item_id: &str, bumped: bool) {
    for t in tickets.iter_mut() {
        for l in t.items.iter_mut() {
            if l.id == item_id {
                l.bumped = bumped;
            }
        }
        if t.status != "voided" {
            t.status = if !t.items.is_empty() && t.items.iter().all(|l| l.bumped) {
                "ready".into()
            } else {
                "firing".into()
            };
        }
    }
}

/// Overlay still-pending (un-synced) bumps onto a freshly-built feed so the board
/// reflects the cook's latest tap instantly — even offline, before the bump drains.
/// `bumps` is `(line_id, bumped)` in FIFO (enqueue) order, so a later tap on the
/// same line wins. A ticket whose lines are now all bumped is shown `ready` (unless
/// voided), keeping the optimistic state consistent with `sort_feed`'s grouping.
/// Pure + unit-tested.
pub(crate) fn overlay_pending_bumps(tickets: &mut [KdsTicketView], bumps: &[(String, bool)]) {
    if bumps.is_empty() {
        return;
    }
    for t in tickets.iter_mut() {
        let mut touched = false;
        for line in t.items.iter_mut() {
            // Last matching intent wins (FIFO order → fold left).
            for (id, bumped) in bumps.iter() {
                if *id == line.id {
                    line.bumped = *bumped;
                    touched = true;
                }
            }
        }
        // Re-derive display readiness from the overlaid lines (never resurrect a
        // voided ticket; never mark an empty ticket ready).
        if touched && t.status != "voided" {
            t.status = if !t.items.is_empty() && t.items.iter().all(|l| l.bumped) {
                "ready".to_string()
            } else {
                "firing".to_string()
            };
        }
    }
}

/// Order the feed for the board: OLDEST first (rush-to-top — the longest-waiting
/// ticket leads), still-firing ahead of already-ready at the same instant. Pure +
/// unit-tested; the host renders the result verbatim.
pub(crate) fn sort_feed(tickets: &mut [KdsTicketView]) {
    tickets.sort_by(|a, b| {
        // Ready tickets sink below still-firing ones; within a group, oldest first.
        let rank = |s: &str| if s == "ready" { 1 } else { 0 };
        rank(&a.status)
            .cmp(&rank(&b.status))
            .then(a.created_at.cmp(&b.created_at))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tk(id: &str, status: &str, created_at: &str) -> KdsTicketView {
        KdsTicketView {
            id: id.into(),
            kitchen_ref: None,
            table_label: None,
            round_number: 1,
            source_type: "open_ticket".into(),
            status: status.into(),
            created_at: created_at.into(),
            items: vec![],
        }
    }

    #[test]
    fn sort_puts_oldest_firing_first_ready_last() {
        let mut v = vec![
            tk("new", "firing", "2026-06-25T10:05:00Z"),
            tk("ready_old", "ready", "2026-06-25T10:00:00Z"),
            tk("old", "firing", "2026-06-25T10:01:00Z"),
        ];
        sort_feed(&mut v);
        let order: Vec<&str> = v.iter().map(|t| t.id.as_str()).collect();
        // firing oldest → firing newer → ready (even though ready is chronologically oldest).
        assert_eq!(order, ["old", "new", "ready_old"]);
    }

    fn line(id: &str, bumped: bool) -> KdsLineView {
        KdsLineView {
            id: id.into(),
            name: "Item".into(),
            qty: 1,
            size_label: None,
            modifiers: vec![],
            notes: None,
            station_id: None,
            station_name: None,
            bumped,
        }
    }

    #[test]
    fn overlay_applies_latest_pending_bump_and_rederives_ready() {
        let mut t = tk("t1", "firing", "2026-06-25T10:00:00Z");
        t.items = vec![line("a", false), line("b", false)];
        let mut feed = vec![t];

        // Bump both lines → ticket should flip to ready.
        overlay_pending_bumps(&mut feed, &[("a".into(), true), ("b".into(), true)]);
        assert!(feed[0].items.iter().all(|l| l.bumped));
        assert_eq!(feed[0].status, "ready", "all lines bumped → ready");

        // A later un-bump of one line wins over the earlier bump → back to firing.
        overlay_pending_bumps(&mut feed, &[("a".into(), true), ("a".into(), false)]);
        assert!(!feed[0].items.iter().find(|l| l.id == "a").unwrap().bumped);
        assert_eq!(feed[0].status, "firing", "a re-opened → not ready");
    }

    #[test]
    fn kitchen_id_derivation_matches_backend() {
        // CROSS-REPO CONTRACT: these MUST equal the backend's pinned values
        // (`kitchen::id_tests::kitchen_id_derivation_is_pinned`). Same namespace + v5.
        let nil = "00000000-0000-0000-0000-000000000000";
        let kt = derive_kitchen_ticket_id(nil).unwrap();
        assert_eq!(kt, "e9b2a598-f8ea-5510-8382-927f5e218fff");
        assert_eq!(derive_kitchen_item_id(&kt, 0), "0b40ac60-7d15-5bef-858f-849b09850f69");
        assert_eq!(derive_kitchen_item_id(&kt, 1), "50cef3f1-fced-57d3-bb6c-daa1c917a8b6");
    }

    #[test]
    fn lan_overlay_adds_unsynced_and_dedups_synced() {
        let mut feed = vec![tk("server-1", "firing", "2026-06-25T10:00:00Z")];
        let lan = vec![
            tk("server-1", "firing", "2026-06-25T10:00:00Z"), // already in feed → dropped
            tk("lan-2", "firing", "2026-06-25T10:01:00Z"),    // not synced → added
            tk("lan-void", "voided", "2026-06-25T10:02:00Z"), // voided → never shown
        ];
        overlay_lan_tickets(&mut feed, lan);
        let ids: Vec<&str> = feed.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["server-1", "lan-2"]);
    }

    #[test]
    fn overlay_never_resurrects_a_voided_ticket() {
        let mut t = tk("t1", "voided", "2026-06-25T10:00:00Z");
        t.items = vec![line("a", false)];
        let mut feed = vec![t];
        overlay_pending_bumps(&mut feed, &[("a".into(), true)]);
        assert_eq!(feed[0].status, "voided", "voided stays voided");
    }

    #[test]
    fn line_view_reads_slim_kitchen_json() {
        let it = models::KitchenTicketItemView {
            bumped: false,
            id: uuid::Uuid::new_v4(),
            line: Some(serde_json::json!({
                "name": "Steak", "size_label": null, "qty": 2,
                "modifiers": ["Medium rare"], "notes": "no salt"
            })),
            qty: 2,
            station_id: None,
            station_name: Some(Some("Grill".into())),
        };
        let lv = line_view(&it);
        assert_eq!(lv.name, "Steak");
        assert_eq!(lv.qty, 2);
        assert_eq!(lv.modifiers, vec!["Medium rare"]);
        assert_eq!(lv.notes.as_deref(), Some("no salt"));
        assert_eq!(lv.station_name.as_deref(), Some("Grill"));
        assert!(!lv.bumped);
    }
}
