//! Integrity probes — the visit engine's invariants: schedule interval positive, transition gates, parts
//! only on an open visit, issue-once idempotency, and the durable completion event.

mod common;
use common::*;

use backbone_maintenance::application::service::maintenance_events::LoggingSink;
use backbone_maintenance::application::service::maintenance_write_service::*;
use uuid::Uuid;

fn visit_dto(company: Uuid, warehouse: Option<Uuid>, a: &MxAccounts) -> NewVisit {
    NewVisit {
        company_id: company, asset_id: Uuid::new_v4(), schedule_id: None, maintenance_type: "corrective".into(),
        scheduled_date: today(), warehouse_id: warehouse, warranty_claim_id: None, labor_cost: dec("50000"),
        maintenance_expense_account_id: a.expense, parts_inventory_account_id: a.parts_inventory,
        labor_payable_account_id: a.labor_payable,
    }
}

// MIP-1 — a schedule interval must be positive.
#[tokio::test]
async fn mip1_interval_positive() {
    let pool = pool().await;
    let svc = MaintenanceWriteService::new(pool.clone());
    let r = svc.create_schedule(NewSchedule {
        company_id: Uuid::new_v4(), asset_id: Uuid::new_v4(), name: "bad".into(),
        interval_days: 0, next_due_date: today(),
    }).await;
    assert!(matches!(r, Err(MaintenanceError::Invalid(_))));
}

// MIP-2 — a completed visit cannot be re-completed to a new journal (idempotent short-circuit); a
// cancelled visit cannot complete.
#[tokio::test]
async fn mip2_transition_gates() {
    let pool = pool().await;
    let company = Uuid::new_v4();
    let a = mx_accounts(&pool, company).await;
    let svc = MaintenanceWriteService::new(pool.clone());

    let cancelled = svc.plan_visit(visit_dto(company, None, &a)).await.unwrap();
    svc.cancel_visit(cancelled).await.unwrap();
    let r = svc.complete_visit(cancelled, today(), &FakeInventory::new("5000"), &CountingGl::new(), &LoggingSink).await;
    assert!(matches!(r, Err(MaintenanceError::InvalidState(_))), "a cancelled visit cannot complete");
}

// MIP-3 — parts can only be added to an open (planned/in_progress) visit.
#[tokio::test]
async fn mip3_parts_only_when_open() {
    let pool = pool().await;
    let company = Uuid::new_v4();
    let a = mx_accounts(&pool, company).await;
    let svc = MaintenanceWriteService::new(pool.clone());
    let visit = svc.plan_visit(visit_dto(company, None, &a)).await.unwrap();
    svc.complete_visit(visit, today(), &FakeInventory::new("5000"), &CountingGl::new(), &LoggingSink).await.unwrap();
    let r = svc.add_part(visit, Uuid::new_v4(), dec("1")).await;
    assert!(matches!(r, Err(MaintenanceError::InvalidState(_))), "no parts on a completed visit");
}

// MIP-4 — parts are issued out of inventory at most once even across a re-complete (the second complete
// short-circuits before touching inventory).
#[tokio::test]
async fn mip4_parts_issued_once() {
    let pool = pool().await;
    let company = Uuid::new_v4();
    let warehouse = Uuid::new_v4();
    let a = mx_accounts(&pool, company).await;
    let svc = MaintenanceWriteService::new(pool.clone());
    let inv = FakeInventory::new("5000");
    let visit = svc.plan_visit(visit_dto(company, Some(warehouse), &a)).await.unwrap();
    svc.add_part(visit, Uuid::new_v4(), dec("2")).await.unwrap();

    svc.complete_visit(visit, today(), &inv, &CountingGl::new(), &LoggingSink).await.unwrap();
    svc.complete_visit(visit, today(), &inv, &CountingGl::new(), &LoggingSink).await.unwrap();
    assert_eq!(inv.issue_count(), 1, "stock issued exactly once");
}

// MIP-5 — the completion event is durable: with the in-proc publish lost (dropping sink), it is still
// staged in the outbox for the relay.
#[tokio::test]
async fn mip5_completion_event_durable() {
    let pool = pool().await;
    let company = Uuid::new_v4();
    let a = mx_accounts(&pool, company).await;
    let svc = MaintenanceWriteService::new(pool.clone());
    let visit = svc.plan_visit(visit_dto(company, None, &a)).await.unwrap();
    svc.complete_visit(visit, today(), &FakeInventory::new("5000"), &CountingGl::new(), &DroppingSink).await.unwrap();
    let staged: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM maintenance.outbox_events WHERE aggregate_id=$1 AND event_type='MaintenanceCompleted'")
        .bind(visit.to_string()).fetch_one(&pool).await.unwrap();
    assert_eq!(staged, 1, "MaintenanceCompleted durably staged despite the lost publish");
}

// MIP-6 — the part set is FROZEN once completion begins (maturity council 2026-07-10). `add_part` only
// accepts a planned visit; once in_progress (the state a crashed completion leaves behind) a new part is
// refused — so a crash-and-retry can't replay a stale idempotent inventory ack against a widened set,
// which would consume a part physically without costing/journaling it.
#[tokio::test]
async fn mip6_part_set_frozen_after_start() {
    let pool = pool().await;
    let company = Uuid::new_v4();
    let a = mx_accounts(&pool, company).await;
    let svc = MaintenanceWriteService::new(pool.clone());
    let warehouse = Uuid::new_v4();
    let visit = svc.plan_visit(visit_dto(company, Some(warehouse), &a)).await.unwrap();
    svc.add_part(visit, Uuid::new_v4(), dec("5")).await.unwrap();

    // Completion has begun (in_progress) — the set is frozen.
    svc.start_visit(visit).await.unwrap();
    let r = svc.add_part(visit, Uuid::new_v4(), dec("3")).await;
    assert!(matches!(r, Err(MaintenanceError::InvalidState(_))), "no part can be added after the visit starts");
}
