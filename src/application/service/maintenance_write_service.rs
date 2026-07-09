//! The hand-authored maintenance write path (user-owned; survives regen).
//!
//! A maintenance visit: plan it against an asset (preventive from a schedule, or corrective ad-hoc), add
//! parts, then complete it — issue the parts out of inventory (valued at inventory's moving-average),
//! roll parts + labor into the visit cost, and post ONE balanced maintenance-cost journal — the **9th GL
//! producer**: `Dr Maintenance Expense (total) · Cr Inventory Parts (parts) · Cr Labor Payable (labor)`.
//! Because `total = parts + labor`, it balances. Idempotent per visit. Money is IDR, 2dp, half-away.

use chrono::{NaiveDate, Utc};
use rust_decimal::{Decimal, RoundingStrategy};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::maintenance_events::*;
use super::maintenance_gl::*;
use super::maintenance_ports::*;

fn money(v: Decimal) -> Decimal {
    v.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

#[derive(Debug, thiserror::Error)]
pub enum MaintenanceError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("not found: {0}")]
    NotFound(&'static str),
    #[error("invalid state: {0}")]
    InvalidState(&'static str),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("unbalanced posting")]
    Unbalanced,
    #[error("gl rejected: {0}")]
    GlRejected(String),
    #[error("inventory rejected: {0}")]
    InventoryRejected(String),
}

pub struct NewSchedule {
    pub company_id: Uuid,
    pub asset_id: Uuid,
    pub name: String,
    pub interval_days: i32,
    pub next_due_date: NaiveDate,
}

pub struct NewVisit {
    pub company_id: Uuid,
    pub asset_id: Uuid,
    pub schedule_id: Option<Uuid>,
    pub maintenance_type: String, // preventive | corrective
    pub scheduled_date: NaiveDate,
    pub warehouse_id: Option<Uuid>,
    pub warranty_claim_id: Option<Uuid>,
    pub labor_cost: Decimal,
    pub maintenance_expense_account_id: Uuid,
    pub parts_inventory_account_id: Uuid,
    pub labor_payable_account_id: Uuid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteOutcome {
    pub visit_id: Uuid,
    pub journal_id: Option<Uuid>,
    pub total_cost: Decimal,
    pub already: bool,
}

pub struct MaintenanceWriteService {
    pool: PgPool,
}

impl MaintenanceWriteService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Define a preventive maintenance schedule for an asset.
    pub async fn create_schedule(&self, s: NewSchedule) -> Result<Uuid, MaintenanceError> {
        if s.interval_days <= 0 {
            return Err(MaintenanceError::Invalid("interval_days must be positive".into()));
        }
        let id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO maintenance.maintenance_schedules
                 (id, company_id, asset_id, name, interval_days, next_due_date, is_active)
               VALUES ($1,$2,$3,$4,$5,$6,true)"#,
        )
        .bind(id).bind(s.company_id).bind(s.asset_id).bind(&s.name).bind(s.interval_days).bind(s.next_due_date)
        .execute(&self.pool).await?;
        Ok(id)
    }

    /// Plan a visit (planned). Preventive visits reference a schedule; corrective ones don't.
    pub async fn plan_visit(&self, v: NewVisit) -> Result<Uuid, MaintenanceError> {
        if v.labor_cost < Decimal::ZERO {
            return Err(MaintenanceError::Invalid("labor_cost must be non-negative".into()));
        }
        let id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO maintenance.maintenance_visits
                 (id, company_id, asset_id, schedule_id, maintenance_type, status, warehouse_id,
                  warranty_claim_id, scheduled_date, labor_cost, parts_cost, total_cost,
                  maintenance_expense_account_id, parts_inventory_account_id, labor_payable_account_id)
               VALUES ($1,$2,$3,$4,$5::maintenance_type,'planned'::visit_status,$6,$7,$8,$9,0,0,$10,$11,$12)"#,
        )
        .bind(id).bind(v.company_id).bind(v.asset_id).bind(v.schedule_id).bind(&v.maintenance_type)
        .bind(v.warehouse_id).bind(v.warranty_claim_id).bind(v.scheduled_date).bind(money(v.labor_cost))
        .bind(v.maintenance_expense_account_id).bind(v.parts_inventory_account_id).bind(v.labor_payable_account_id)
        .execute(&self.pool).await?;
        Ok(id)
    }

    /// Add a part line to a planned/in-progress visit (quantity only; valued on completion).
    pub async fn add_part(&self, visit_id: Uuid, item_id: Uuid, quantity: Decimal) -> Result<Uuid, MaintenanceError> {
        if quantity <= Decimal::ZERO {
            return Err(MaintenanceError::Invalid("part quantity must be positive".into()));
        }
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status::text FROM maintenance.maintenance_visits WHERE id=$1 AND (metadata->>'deleted_at') IS NULL")
            .bind(visit_id).fetch_optional(&self.pool).await?;
        // Parts may only be added while PLANNED. Once a visit is in_progress the set is FROZEN — that is the
        // state a crashed completion leaves behind, and a part added then would be replayed against the
        // line-agnostic idempotent inventory ack: physically consumed but never issued/costed/journaled
        // (maturity council 2026-07-10). So the line set can never widen after completion begins.
        match status.as_deref() {
            None => return Err(MaintenanceError::NotFound("visit")),
            Some("planned") => {}
            _ => return Err(MaintenanceError::InvalidState("visit is not planned — the part set is frozen")),
        }
        let id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO maintenance.maintenance_visit_parts (id, visit_id, item_id, quantity, unit_cost, amount)
               VALUES ($1,$2,$3,$4,0,0)"#,
        )
        .bind(id).bind(visit_id).bind(item_id).bind(quantity).execute(&self.pool).await?;
        Ok(id)
    }

    /// Start a planned visit (planned → in_progress).
    pub async fn start_visit(&self, visit_id: Uuid) -> Result<(), MaintenanceError> {
        let n = sqlx::query(
            r#"UPDATE maintenance.maintenance_visits SET status='in_progress'::visit_status
               WHERE id=$1 AND status='planned'::visit_status"#,
        )
        .bind(visit_id).execute(&self.pool).await?;
        if n.rows_affected() != 1 {
            return Err(MaintenanceError::InvalidState("visit is not planned"));
        }
        Ok(())
    }

    /// Complete a visit: issue its parts (valued by inventory), roll up the cost, post ONE balanced
    /// maintenance-cost journal, and gate `→ completed`. Posts at most once (idempotent on `source_id =
    /// visit_id`). A zero-cost visit completes without a posting. Emits `MaintenanceCompleted`.
    pub async fn complete_visit(
        &self,
        visit_id: Uuid,
        performed_date: NaiveDate,
        inventory: &dyn InventoryPort,
        sink: &dyn GlPostSink,
        events: &dyn MaintenanceEventSink,
    ) -> Result<CompleteOutcome, MaintenanceError> {
        let v = sqlx::query(
            r#"SELECT company_id, asset_id, status::text AS status, warehouse_id, labor_cost,
                      maintenance_expense_account_id, parts_inventory_account_id, labor_payable_account_id,
                      journal_id, total_cost, schedule_id, maintenance_type::text AS maintenance_type
               FROM maintenance.maintenance_visits WHERE id=$1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(visit_id).fetch_optional(&self.pool).await?
        .ok_or(MaintenanceError::NotFound("visit"))?;
        let status: String = v.get("status");
        if status == "completed" {
            return Ok(CompleteOutcome {
                visit_id, journal_id: v.get("journal_id"), total_cost: v.get("total_cost"), already: true,
            });
        }
        if status != "planned" && status != "in_progress" {
            return Err(MaintenanceError::InvalidState("visit is not open"));
        }
        // FREEZE the part set before any external effect: claim the visit to in_progress. `add_part` only
        // accepts a planned visit, so once claimed no line can be added — a crash-and-retry re-issues the
        // identical frozen set (maturity council 2026-07-10).
        sqlx::query(
            r#"UPDATE maintenance.maintenance_visits SET status='in_progress'::visit_status
               WHERE id=$1 AND status='planned'::visit_status"#,
        )
        .bind(visit_id).execute(&self.pool).await?;

        let company_id: Uuid = v.get("company_id");
        let asset_id: Uuid = v.get("asset_id");
        let labor_cost: Decimal = v.get("labor_cost");
        let schedule_id: Option<Uuid> = v.get("schedule_id");
        let maintenance_type: String = v.get("maintenance_type");

        // Issue the parts out of inventory (valued at moving-average). Idempotent per visit.
        let part_rows = sqlx::query(
            "SELECT id, item_id, quantity FROM maintenance.maintenance_visit_parts WHERE visit_id=$1")
            .bind(visit_id).fetch_all(&self.pool).await?;
        let mut parts_cost = Decimal::ZERO;
        if !part_rows.is_empty() {
            let warehouse_id: Uuid = v.get::<Option<Uuid>, _>("warehouse_id")
                .ok_or(MaintenanceError::Invalid("a visit with parts needs a warehouse".into()))?;
            let lines: Vec<IssueLine> = part_rows.iter().map(|r| IssueLine {
                item_id: r.get("item_id"), quantity: r.get("quantity"),
            }).collect();
            let ack = inventory.issue_parts(&PartsIssue {
                company_id, visit_id, warehouse_id, idempotency_key: format!("maintenance:{visit_id}"), lines,
            }).await.map_err(|e| MaintenanceError::InventoryRejected(e.code))?;
            parts_cost = money(ack.total_value);
            // Write back the valued unit_cost/amount per line.
            for lv in &ack.lines {
                sqlx::query(
                    r#"UPDATE maintenance.maintenance_visit_parts SET unit_cost=$3, amount=$4
                       WHERE visit_id=$1 AND item_id=$2"#,
                )
                .bind(visit_id).bind(lv.item_id).bind(money(lv.rate)).bind(money(lv.value))
                .execute(&self.pool).await?;
            }
        }
        let total_cost = labor_cost + parts_cost;

        // Post the balanced cost journal (skip a zero-cost visit).
        let mut journal_id: Option<Uuid> = None;
        let mut post_id: Option<Uuid> = None;
        if total_cost > Decimal::ZERO {
            let expense: Uuid = v.get::<Option<Uuid>, _>("maintenance_expense_account_id")
                .ok_or(MaintenanceError::Invalid("no maintenance expense account".into()))?;
            let parts_acct: Uuid = v.get::<Option<Uuid>, _>("parts_inventory_account_id")
                .ok_or(MaintenanceError::Invalid("no parts inventory account".into()))?;
            let labor_acct: Uuid = v.get::<Option<Uuid>, _>("labor_payable_account_id")
                .ok_or(MaintenanceError::Invalid("no labor payable account".into()))?;
            let mut lines = vec![GlPostLine::debit(expense, total_cost).with_description("Maintenance expense")];
            if parts_cost > Decimal::ZERO {
                lines.push(GlPostLine::credit(parts_acct, parts_cost).with_description("Parts issued"));
            }
            if labor_cost > Decimal::ZERO {
                lines.push(GlPostLine::credit(labor_acct, labor_cost).with_description("Maintenance labor"));
            }
            let env = AccountingPostEnvelope {
                idempotency_key: format!("maintenance:{visit_id}"),
                company_id, branch_id: None, source_type: "maintenance".into(), source_id: visit_id,
                source_reference: None, posting_date: performed_date, currency: "IDR".into(),
                posting_type: "original".into(), description: Some("Maintenance cost".into()), lines,
            };
            if !env.is_balanced() {
                return Err(MaintenanceError::Unbalanced);
            }
            let ack = sink.post(&env).await.map_err(|r| MaintenanceError::GlRejected(r.code))?;
            journal_id = Some(ack.journal_id);
            post_id = Some(ack.post_id);
        }

        let mut tx = self.pool.begin().await?;
        let moved = sqlx::query(
            r#"UPDATE maintenance.maintenance_visits
               SET status='completed'::visit_status, performed_date=$2, parts_cost=$3, total_cost=$4,
                   journal_id=$5, accounting_post_id=$6
               WHERE id=$1 AND status IN ('planned'::visit_status,'in_progress'::visit_status)"#,
        )
        .bind(visit_id).bind(performed_date).bind(parts_cost).bind(total_cost).bind(journal_id).bind(post_id)
        .execute(&mut *tx).await?;
        if moved.rows_affected() != 1 {
            tx.rollback().await?;
            // Raced — re-read the winner.
            let j: Option<Uuid> = sqlx::query_scalar("SELECT journal_id FROM maintenance.maintenance_visits WHERE id=$1")
                .bind(visit_id).fetch_one(&self.pool).await?;
            return Ok(CompleteOutcome { visit_id, journal_id: j, total_cost, already: true });
        }

        // Advance the PREVENTIVE schedule so the next visit becomes due at performed + interval — the
        // recurrence that makes a preventive plan preventive. Inline in the completion tx, so it runs at
        // most once (gated by the visit CAS above) (completeness council 2026-07-10).
        if maintenance_type == "preventive" {
            if let Some(sid) = schedule_id {
                sqlx::query(
                    r#"UPDATE maintenance.maintenance_schedules
                       SET next_due_date = ($2::date + make_interval(days => interval_days))::date
                       WHERE id=$1 AND is_active=true AND (metadata->>'deleted_at') IS NULL"#,
                )
                .bind(sid).bind(performed_date).execute(&mut *tx).await?;
            }
        }

        let event = MaintenanceEvent::MaintenanceCompleted(MaintenanceCompleted {
            visit_id, company_id, asset_id, journal_id, labor_cost, parts_cost, total_cost,
        });
        let record = backbone_outbox::OutboxRecord::new(
            "MaintenanceCompleted", "MaintenanceVisit", visit_id.to_string(),
            serde_json::to_value(&event).map_err(|e| MaintenanceError::Invalid(e.to_string()))?,
            Utc::now(),
        );
        backbone_outbox::outbox::stage(&mut *tx, "maintenance", &record)
            .await.map_err(|e| MaintenanceError::Invalid(format!("outbox stage: {e}")))?;
        tx.commit().await?;
        events.publish(&event);

        Ok(CompleteOutcome { visit_id, journal_id, total_cost, already: false })
    }

    /// Cancel a planned/in-progress visit.
    pub async fn cancel_visit(&self, visit_id: Uuid) -> Result<(), MaintenanceError> {
        let n = sqlx::query(
            r#"UPDATE maintenance.maintenance_visits SET status='cancelled'::visit_status
               WHERE id=$1 AND status IN ('planned'::visit_status,'in_progress'::visit_status)"#,
        )
        .bind(visit_id).execute(&self.pool).await?;
        if n.rows_affected() != 1 {
            return Err(MaintenanceError::InvalidState("visit is not open"));
        }
        Ok(())
    }
}
