// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

pub mod maintenance_gl;
pub mod maintenance_ports;
pub mod maintenance_events;
pub mod maintenance_write_service;
pub mod maintenance_query_service;
pub mod maintenance_request_write_service;
pub use maintenance_gl::{AccountingPostEnvelope, GlPostAck, GlPostLine, GlPostRejected, GlPostSink};
pub use maintenance_ports::{InventoryPort, InventoryRejected, IssueAck, IssueLine, IssuedLineValue, PartsIssue};
pub use maintenance_events::{LoggingSink, MaintenanceCompleted, MaintenanceEvent, MaintenanceEventSink, MaintenanceRequestStageChanged, SuccessorSpawned};
pub use maintenance_write_service::{CompleteOutcome, MaintenanceError, MaintenanceWriteService, NewSchedule, NewVisit};
pub use maintenance_query_service::MaintenanceQueryServiceImpl;
pub use maintenance_request_write_service::{
    MaintenanceRequestUpdate, MaintenanceRequestWriteService, NewMaintenanceRequest,
    RequestWriteError, TransitionOutcome,
};
