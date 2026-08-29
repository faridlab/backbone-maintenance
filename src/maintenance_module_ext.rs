//! Hand-authored extension on [`crate::MaintenanceModule`] — the safe default route composer.
//!
//! **Why a separate file:** the generator rewrites `src/lib.rs`'s generated `impl` region on every
//! `metaphor make`, so anything hand-added there is clobbered. This file is never touched by the
//! generator (declared in `metaphor.codegen.yaml` `user_owned`, wired via a `// <<< CUSTOM` `pub mod`
//! in `lib.rs`), so the delivery survives regen. Rust permits multiple `impl MaintenanceModule` blocks,
//! so the method lives here while the struct, builder, and generated `all_crud_routes()` stay in
//! `lib.rs`.
//!
//! **The invariant-bypass closure (council 2026-08-02, rec #1):** the generated
//! [`crate::MaintenanceModule::all_crud_routes`] mounts unguarded generic CRUD — including a PATCH
//! whose DTO exposes `status` / `parts_cost` / `total_cost` / `journal_id` / `accounting_post_id` — on
//! `MaintenanceVisit`, so a caller can flip a visit to `completed` with no inventory issue and no
//! journal, routing around every invariant [`crate::MaintenanceWriteService`] enforces. The same
//! closure covers `MaintenanceRequest`: its generated PATCH would expose `stage_id`,
//! `kanban_state`, `close_date`, and the successor markers — the exact columns the
//! clone-on-done engine owns. The schema offers no attribute to restrict those PATCHes and
//! `all_crud_routes` is generated wholesale, so the closure is delivered here, at the composer: use
//! [`Self::read_only_routes`] as the production surface; reserve `all_crud_routes()` / `routes()` for
//! trusted/admin/seeding contexts. The validated write path (wiring `MaintenanceWriteService` and
//! `MaintenanceRequestWriteService` into the module) is a separate, later step.

use axum::Router;

impl crate::MaintenanceModule {
    /// The safe default route surface: full CRUD on the two benign masters (`MaintenanceSchedule`,
    /// `MaintenanceStage`), and **read-only** on the engine-owned tables (`MaintenanceVisit`,
    /// `MaintenanceVisitPart`, `MaintenanceRequest`) whose state must only change through
    /// [`crate::MaintenanceWriteService`] / [`crate::MaintenanceRequestWriteService`].
    ///
    /// Stage CRUD is mounted in full because stages are company-agnostic master data (shared across
    /// the fence): creating/reordering them is an administrative act with no engine invariants, and
    /// referential safety is enforced in the DB (RESTRICT on the request FK, plus the soft-delete
    /// guard raising `maintenance_stage_in_use` while any live request still sits on the stage).
    ///
    /// Use this in place of the generated [`crate::MaintenanceModule::all_crud_routes`], which mounts
    /// unguarded writes on the visit/part/request tables. Direct mirror of
    /// `backbone-asset::AssetsModule::read_only_routes` (CRUD on the benign master, read-only on the
    /// engine-owned financial tables).
    pub fn read_only_routes(&self) -> Router {
        use crate::presentation::http::{
            create_maintenance_request_read_routes, create_maintenance_schedule_routes,
            create_maintenance_stage_routes, create_maintenance_visit_part_read_routes,
            create_maintenance_visit_read_routes,
        };

        Router::new()
            .merge(create_maintenance_schedule_routes(
                self.maintenance_schedule_service.clone(),
            ))
            .merge(create_maintenance_stage_routes(
                self.maintenance_stage_service.clone(),
            ))
            .merge(create_maintenance_request_read_routes(
                self.maintenance_request_service.clone(),
            ))
            .merge(create_maintenance_visit_read_routes(
                self.maintenance_visit_service.clone(),
            ))
            .merge(create_maintenance_visit_part_read_routes(
                self.maintenance_visit_part_service.clone(),
            ))
    }

    /// The published read contract for sibling modules — the now-implemented
    /// [`crate::exports::MaintenanceQueryService`] over the module's DTOs. Mirror of
    /// `backbone-asset::AssetsModule::query_service`.
    pub fn query_service(&self) -> std::sync::Arc<dyn crate::exports::MaintenanceQueryService> {
        self.query.clone()
    }

    /// The validated, GL/inventory-backed visit write surface — plan/start/add-part/complete/cancel.
    /// These are the only verbs that may change a visit's state; compose the production surface as
    /// `module.read_only_routes().merge(module.lifecycle_routes())` (reads + validated writes). Do
    /// NOT merge with [`crate::MaintenanceModule::all_crud_routes`] — both mount `POST /maintenance_visits`.
    ///
    /// Requires a `GlPostSink` and an `InventoryPort` supplied via the builder
    /// ([`crate::MaintenanceModuleBuilder::with_gl_sink`] / `with_inventory_port`); missing either
    /// is a wiring error and panics at startup. The event sink defaults to `LoggingSink`.
    pub fn lifecycle_routes(&self) -> Router {
        use crate::presentation::http::{
            create_maintenance_lifecycle_routes, create_maintenance_request_lifecycle_routes,
            MaintenanceLifecycleState, MaintenanceRequestLifecycleState,
        };

        let gl = self.gl_sink.clone().expect(
            "MaintenanceModule::lifecycle_routes() requires a GlPostSink — pass one via \
             MaintenanceModuleBuilder::with_gl_sink(...)",
        );
        let inventory = self.inventory_port.clone().expect(
            "MaintenanceModule::lifecycle_routes() requires an InventoryPort — pass one via \
             MaintenanceModuleBuilder::with_inventory_port(...)",
        );
        create_maintenance_lifecycle_routes(MaintenanceLifecycleState {
            write_svc: self.write_svc.clone(),
            inventory,
            gl,
            event_sink: self.event_sink.clone(),
        })
        .merge(create_maintenance_request_lifecycle_routes(
            MaintenanceRequestLifecycleState {
                write_svc: self.request_write_svc.clone(),
                event_sink: self.event_sink.clone(),
            },
        ))
    }
}
