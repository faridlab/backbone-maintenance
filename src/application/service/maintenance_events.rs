//! Maintenance domain events (hand-authored, user-owned).
//!
//! On completing a visit maintenance emits `MaintenanceCompleted` (the cost journal landed; the asset was
//! serviced) so asset/reporting can react. A consuming service supplies the sink.
//!
//! On the request side, the transition verb emits `MaintenanceRequestStageChanged` (always) and
//! `SuccessorSpawned` (when the clone-on-done engine fired). Both are also staged to the module outbox —
//! they are the named seam for the activity family's deferred lifecycle feedbacks (the communication
//! module consumes maintenance.request stage_changed / successor_spawned through the host relay).

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A maintenance visit was completed and its cost posted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MaintenanceCompleted {
    pub visit_id: Uuid,
    pub company_id: Uuid,
    pub asset_id: Uuid,
    pub journal_id: Option<Uuid>,
    pub labor_cost: Decimal,
    pub parts_cost: Decimal,
    pub total_cost: Decimal,
}

/// A maintenance request changed stage (the request lifecycle event).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MaintenanceRequestStageChanged {
    pub request_id: Uuid,
    pub company_id: Uuid,
    pub from_stage_id: Option<Uuid>,
    pub to_stage_id: Uuid,
    /// The request's close_date after the transition (stamped when the target stage is done).
    pub close_date: Option<NaiveDate>,
    /// Set when this transition also spawned the recurring successor.
    pub spawned_successor_id: Option<Uuid>,
}

/// The clone-on-done engine spawned a request's successor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SuccessorSpawned {
    pub source_request_id: Uuid,
    pub successor_request_id: Uuid,
    pub company_id: Uuid,
    /// The successor's planned start (source base advanced by the repeat step).
    pub next_schedule_date: Option<DateTime<Utc>>,
    /// The successor's planned finish (start + duration).
    pub next_schedule_end: Option<DateTime<Utc>>,
}

/// The maintenance domain-event union.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum MaintenanceEvent {
    MaintenanceCompleted(MaintenanceCompleted),
    MaintenanceRequestStageChanged(MaintenanceRequestStageChanged),
    SuccessorSpawned(SuccessorSpawned),
}

/// Sink the write path publishes to. A consuming service supplies its own (bus, outbox, …).
pub trait MaintenanceEventSink: Send + Sync {
    fn publish(&self, event: &MaintenanceEvent);
}

/// A no-op/logging sink for tests and single-process composition.
#[derive(Debug, Default, Clone)]
pub struct LoggingSink;

impl MaintenanceEventSink for LoggingSink {
    fn publish(&self, event: &MaintenanceEvent) {
        tracing::info!(?event, "maintenance event");
    }
}
