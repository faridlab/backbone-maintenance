//! Maintenance domain events (hand-authored, user-owned).
//!
//! On completing a visit maintenance emits `MaintenanceCompleted` (the cost journal landed; the asset was
//! serviced) so asset/reporting can react. A consuming service supplies the sink.

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

/// The maintenance domain-event union.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum MaintenanceEvent {
    MaintenanceCompleted(MaintenanceCompleted),
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
