//! Concrete [`MaintenanceQueryService`] over the repositories (hand-authored, user-owned).
//!
//! Delivers the published read contract (defined in `exports::services`) that the skeleton shipped as
//! an unimplemented trait — the contract-seat finding of the maturity council (2026-08-02, rec #2).
//! It is placed in the application layer — not inside `exports/` — so `exports/` stays a pure,
//! decoupled contract surface while the realization lives next to the other services that depend on
//! infrastructure. Direct mirror of `backbone-asset::AssetsQueryServiceImpl`.
//!
//! Reads use the repositories' generic `find_by_id` / `exists` (available via `Deref` on each
//! hand-authored newtype). Under RLS (`app.company_id`), a read with no company scope set simply sees
//! no rows; a composing service binds the caller's company onto its connection as usual.

use anyhow::Result;
use async_trait::async_trait;
use sqlx::PgPool;

use crate::exports::{
    MaintenanceQueryService, MaintenanceScheduleDto, MaintenanceScheduleId, MaintenanceScheduleSummary,
    MaintenanceVisitDto, MaintenanceVisitId, MaintenanceVisitPartDto, MaintenanceVisitPartId,
    MaintenanceVisitPartSummary, MaintenanceVisitSummary,
};
use crate::infrastructure::persistence::{
    MaintenanceScheduleRepository, MaintenanceVisitPartRepository, MaintenanceVisitRepository,
};

/// Implemented [`MaintenanceQueryService`] — one `PgPool`, builds a repo per call (the pool is
/// `Arc`-internal, so cloning is cheap).
pub struct MaintenanceQueryServiceImpl {
    pool: PgPool,
}

impl MaintenanceQueryServiceImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MaintenanceQueryService for MaintenanceQueryServiceImpl {
    async fn get_maintenance_schedule(
        &self,
        id: MaintenanceScheduleId,
    ) -> Result<Option<MaintenanceScheduleDto>> {
        let s = MaintenanceScheduleRepository::new(self.pool.clone())
            .find_by_id(&id.into_inner().to_string())
            .await?;
        Ok(s.map(MaintenanceScheduleDto::from))
    }

    async fn get_maintenance_schedule_summary(
        &self,
        id: MaintenanceScheduleId,
    ) -> Result<Option<MaintenanceScheduleSummary>> {
        let s = MaintenanceScheduleRepository::new(self.pool.clone())
            .find_by_id(&id.into_inner().to_string())
            .await?;
        Ok(s.map(|s| MaintenanceScheduleSummary {
            id: MaintenanceScheduleId(s.id),
            name: s.name,
            status: s.status,
        }))
    }

    async fn maintenance_schedule_exists(&self, id: MaintenanceScheduleId) -> Result<bool> {
        MaintenanceScheduleRepository::new(self.pool.clone())
            .exists(&id.into_inner().to_string())
            .await
    }

    async fn get_maintenance_visit(&self, id: MaintenanceVisitId) -> Result<Option<MaintenanceVisitDto>> {
        let v = MaintenanceVisitRepository::new(self.pool.clone())
            .find_by_id(&id.into_inner().to_string())
            .await?;
        Ok(v.map(MaintenanceVisitDto::from))
    }

    async fn get_maintenance_visit_summary(
        &self,
        id: MaintenanceVisitId,
    ) -> Result<Option<MaintenanceVisitSummary>> {
        let v = MaintenanceVisitRepository::new(self.pool.clone())
            .find_by_id(&id.into_inner().to_string())
            .await?;
        Ok(v.map(|v| MaintenanceVisitSummary {
            id: MaintenanceVisitId(v.id),
            status: v.status,
        }))
    }

    async fn maintenance_visit_exists(&self, id: MaintenanceVisitId) -> Result<bool> {
        MaintenanceVisitRepository::new(self.pool.clone())
            .exists(&id.into_inner().to_string())
            .await
    }

    async fn get_maintenance_visit_part(
        &self,
        id: MaintenanceVisitPartId,
    ) -> Result<Option<MaintenanceVisitPartDto>> {
        let p = MaintenanceVisitPartRepository::new(self.pool.clone())
            .find_by_id(&id.into_inner().to_string())
            .await?;
        Ok(p.map(MaintenanceVisitPartDto::from))
    }

    async fn get_maintenance_visit_part_summary(
        &self,
        id: MaintenanceVisitPartId,
    ) -> Result<Option<MaintenanceVisitPartSummary>> {
        let p = MaintenanceVisitPartRepository::new(self.pool.clone())
            .find_by_id(&id.into_inner().to_string())
            .await?;
        Ok(p.map(|p| MaintenanceVisitPartSummary {
            id: MaintenanceVisitPartId(p.id),
        }))
    }

    async fn maintenance_visit_part_exists(&self, id: MaintenanceVisitPartId) -> Result<bool> {
        MaintenanceVisitPartRepository::new(self.pool.clone())
            .exists(&id.into_inner().to_string())
            .await
    }
}

// Entity → DTO conversions. Regen-safe home: this file is hand-authored (never overwritten by
// `metaphor make`), unlike `exports/types.rs` whose CUSTOM block the generator resets. The query
// methods above rely on these `From` impls to hand siblings the published DTO shapes.
impl From<crate::domain::entity::MaintenanceSchedule> for MaintenanceScheduleDto {
    fn from(s: crate::domain::entity::MaintenanceSchedule) -> Self {
        Self {
            id: MaintenanceScheduleId(s.id),
            company_id: s.company_id,
            asset_id: s.asset_id,
            name: s.name,
            interval_days: s.interval_days,
            next_due_date: s.next_due_date,
            status: s.status,
            metadata: serde_json::to_value(&s.metadata).unwrap_or_default(),
        }
    }
}

impl From<crate::domain::entity::MaintenanceVisit> for MaintenanceVisitDto {
    fn from(v: crate::domain::entity::MaintenanceVisit) -> Self {
        Self {
            id: MaintenanceVisitId(v.id),
            company_id: v.company_id,
            asset_id: v.asset_id,
            schedule_id: v.schedule_id,
            maintenance_type: v.maintenance_type,
            status: v.status,
            warehouse_id: v.warehouse_id,
            warranty_claim_id: v.warranty_claim_id,
            scheduled_date: v.scheduled_date,
            performed_date: v.performed_date,
            labor_cost: v.labor_cost,
            parts_cost: v.parts_cost,
            total_cost: v.total_cost,
            maintenance_expense_account_id: v.maintenance_expense_account_id,
            parts_inventory_account_id: v.parts_inventory_account_id,
            labor_payable_account_id: v.labor_payable_account_id,
            journal_id: v.journal_id,
            accounting_post_id: v.accounting_post_id,
            notes: v.notes,
            metadata: serde_json::to_value(&v.metadata).unwrap_or_default(),
        }
    }
}

impl From<crate::domain::entity::MaintenanceVisitPart> for MaintenanceVisitPartDto {
    fn from(p: crate::domain::entity::MaintenanceVisitPart) -> Self {
        Self {
            id: MaintenanceVisitPartId(p.id),
            company_id: p.company_id,
            visit_id: p.visit_id,
            item_id: p.item_id,
            quantity: p.quantity,
            unit_cost: p.unit_cost,
            amount: p.amount,
            metadata: serde_json::to_value(&p.metadata).unwrap_or_default(),
        }
    }
}
