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
//! journal, routing around every invariant [`crate::MaintenanceWriteService`] enforces. The schema
//! offers no attribute to restrict that PATCH and `all_crud_routes` is generated wholesale, so the
//! closure is delivered here, at the composer: use [`Self::read_only_routes`] as the production
//! surface; reserve `all_crud_routes()` / `routes()` for trusted/admin/seeding contexts. The validated
//! write path (wiring `MaintenanceWriteService` into the module) is a separate, later step.

use axum::Router;

impl crate::MaintenanceModule {
    /// The safe default route surface: full CRUD on the `MaintenanceSchedule` master, and
    /// **read-only** on the two engine-owned, cost/inventory-bearing tables (`MaintenanceVisit`,
    /// `MaintenanceVisitPart`) whose state must only change through [`crate::MaintenanceWriteService`].
    ///
    /// Use this in place of the generated [`crate::MaintenanceModule::all_crud_routes`], which mounts
    /// unguarded writes on the visit/part tables. Direct mirror of
    /// `backbone-asset::AssetsModule::read_only_routes` (CRUD on the benign master, read-only on the
    /// engine-owned financial tables).
    pub fn read_only_routes(&self) -> Router {
        use crate::presentation::http::{
            create_maintenance_schedule_routes, create_maintenance_visit_part_read_routes,
            create_maintenance_visit_read_routes,
        };

        Router::new()
            .merge(create_maintenance_schedule_routes(
                self.maintenance_schedule_service.clone(),
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
}
