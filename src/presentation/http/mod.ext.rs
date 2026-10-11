// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

pub mod maintenance_lifecycle_handler;
pub mod maintenance_request_lifecycle_handler;
pub use maintenance_lifecycle_handler::{create_maintenance_lifecycle_routes, MaintenanceLifecycleState};
pub use maintenance_request_lifecycle_handler::{
    create_maintenance_request_lifecycle_routes, MaintenanceRequestLifecycleState,
};
