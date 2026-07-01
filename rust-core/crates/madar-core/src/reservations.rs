//! Reservations & floor-plan view types for the native host UI.
//!
//! The geometry is authored in the dashboard; the POS renders the floor to scale
//! and drives host operations (seat a party, set a table's status, move a
//! ticket). All logic lives here in the core — Swift/Kotlin only render these
//! `uniffi::Record`s and call the exported `MadarCore` methods (in `lib.rs`).
//!
//! Backend nullable+optional columns come across the generated client as
//! `Option<Option<T>>`; we `.flatten()` them to a single `Option<T>` for the FFI.

use madar_api::models;

use crate::error::CoreError;

/// A floor area (e.g. Patio, Indoor) with its canvas extent for to-scale render.
#[derive(uniffi::Record, Clone, Debug)]
pub struct FloorSectionView {
    pub id: String,
    pub name: String,
    pub ordering: i32,
    pub canvas_w: i32,
    pub canvas_h: i32,
}

impl From<models::FloorSection> for FloorSectionView {
    fn from(s: models::FloorSection) -> Self {
        Self {
            id: s.id.to_string(),
            name: s.name,
            ordering: s.ordering,
            canvas_w: s.canvas_w,
            canvas_h: s.canvas_h,
        }
    }
}

/// A table's geometry + live status, ready to draw on the floor canvas.
#[derive(uniffi::Record, Clone, Debug)]
pub struct FloorTableView {
    pub id: String,
    pub section_id: Option<String>,
    pub label: String,
    pub seats: i32,
    /// `rect` | `circle`.
    pub shape: String,
    /// `free` | `held` | `seated` | `dirty`.
    pub status: String,
    pub pos_x: f64,
    pub pos_y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
}

impl From<models::FloorTable> for FloorTableView {
    fn from(t: models::FloorTable) -> Self {
        Self {
            id: t.id.to_string(),
            section_id: t.section_id.flatten().map(|u| u.to_string()),
            label: t.label,
            seats: t.seats,
            shape: t.shape,
            status: t.status,
            pos_x: t.pos_x,
            pos_y: t.pos_y,
            width: t.width,
            height: t.height,
            rotation: t.rotation,
        }
    }
}

/// A booking — reservation (`reserved_for` set) or waitlist entry (none).
#[derive(uniffi::Record, Clone, Debug)]
pub struct ReservationView {
    pub id: String,
    pub branch_id: String,
    /// `reservation` | `walk_in`.
    pub kind: String,
    pub customer_name: String,
    pub customer_phone: String,
    pub party_size: i32,
    /// RFC-3339 instant, or `None` for a waitlist entry.
    pub reserved_for: Option<String>,
    pub status: String,
    /// Assigned table ids (multiple ⇒ merged tables).
    pub table_ids: Vec<String>,
    pub customer_lat: Option<f64>,
    pub customer_lng: Option<f64>,
    pub notes: Option<String>,
}

impl From<models::BookingView> for ReservationView {
    fn from(b: models::BookingView) -> Self {
        Self {
            id: b.id.to_string(),
            branch_id: b.branch_id.to_string(),
            kind: b.kind,
            customer_name: b.customer_name,
            customer_phone: b.customer_phone,
            party_size: b.party_size,
            reserved_for: b.reserved_for.flatten().map(|d| d.to_rfc3339()),
            status: b.status,
            table_ids: b.table_ids.into_iter().map(|u| u.to_string()).collect(),
            customer_lat: b.customer_lat.flatten(),
            customer_lng: b.customer_lng.flatten(),
            notes: b.notes.flatten(),
        }
    }
}

/// Parse a host-supplied UUID string, surfacing a clean `Validation` error
/// rather than letting a malformed id reach the wire.
pub(crate) fn parse_uuid(field: &str, s: &str) -> Result<uuid::Uuid, CoreError> {
    uuid::Uuid::parse_str(s).map_err(|_| CoreError::Validation {
        field: field.to_string(),
        detail: format!("invalid id '{s}'"),
    })
}

/// Parse a list of UUID strings (e.g. merged-table assignment).
pub(crate) fn parse_uuids(field: &str, ids: &[String]) -> Result<Vec<uuid::Uuid>, CoreError> {
    ids.iter().map(|s| parse_uuid(field, s)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_uuid_rejects_garbage() {
        assert!(parse_uuid("table_id", "not-a-uuid").is_err());
        assert!(parse_uuid("table_id", "11111111-1111-1111-1111-111111111111").is_ok());
    }

    #[test]
    fn parse_uuids_collects_or_fails_fast() {
        let good = vec![
            "11111111-1111-1111-1111-111111111111".to_string(),
            "22222222-2222-2222-2222-222222222222".to_string(),
        ];
        assert_eq!(parse_uuids("table_ids", &good).unwrap().len(), 2);

        let bad = vec!["11111111-1111-1111-1111-111111111111".to_string(), "nope".to_string()];
        assert!(parse_uuids("table_ids", &bad).is_err());
    }

    #[test]
    fn table_view_flattens_optional_section() {
        // section_id arrives as Option<Option<Uuid>>; None and Some(None) ⇒ None.
        let id = uuid::Uuid::nil();
        let now = chrono::DateTime::parse_from_rfc3339("2026-06-30T12:00:00Z").unwrap();
        let model = models::FloorTable {
            id,
            org_id: id,
            branch_id: id,
            section_id: Some(None),
            label: "T1".into(),
            seats: 4,
            shape: "rect".into(),
            status: "free".into(),
            pos_x: 10.0,
            pos_y: 20.0,
            width: 80.0,
            height: 80.0,
            rotation: 0.0,
            is_active: true,
            created_at: now,
            updated_at: now,
        };
        let view = FloorTableView::from(model);
        assert_eq!(view.section_id, None);
        assert_eq!(view.label, "T1");
        assert_eq!(view.seats, 4);
        assert_eq!(view.status, "free");
    }
}
