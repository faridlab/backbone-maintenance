//! Golden cases — the maintenance visit oracle: complete rolls parts (valued by inventory) + labor into
//! the cost and posts ONE balanced journal; post-once idempotency; a zero-cost visit completes without a
//! post; parts are valued by inventory's number.

mod common;
use common::*;

use backbone_maintenance::application::service::maintenance_events::{LoggingSink, MaintenanceEvent};
use backbone_maintenance::application::service::maintenance_write_service::*;
use rust_decimal::Decimal;
use uuid::Uuid;

fn new_visit(company: Uuid, asset: Uuid, warehouse: Option<Uuid>, labor: &str, a: &MxAccounts) -> NewVisit {
    NewVisit {
        company_id: company, asset_id: asset, schedule_id: None, maintenance_type: "corrective".into(),
        scheduled_date: today(), warehouse_id: warehouse, warranty_claim_id: None, labor_cost: dec(labor),
        maintenance_expense_account_id: a.expense, parts_inventory_account_id: a.parts_inventory,
        labor_payable_account_id: a.labor_payable,
    }
}

// MGC-1 — a visit with parts + labor posts a balanced cost journal (Dr Expense total · Cr Parts · Cr Labor).
#[tokio::test]
async fn mgc1_complete_posts_balanced_cost() {
    let pool = pool().await;
    let company = Uuid::new_v4();
    let asset = Uuid::new_v4();
    let warehouse = Uuid::new_v4();
    let a = mx_accounts(&pool, company).await;
    let svc = MaintenanceWriteService::new(pool.clone());
    let inv = FakeInventory::new("5000"); // 5000/unit

    let visit = svc.plan_visit(new_visit(company, asset, Some(warehouse), "200000", &a)).await.unwrap();
    svc.add_part(visit, Uuid::new_v4(), dec("3")).await.unwrap(); // 3 × 5000 = 15,000
    let gl = CountingGl::new();
    let out = svc.complete_visit(visit, today(), &inv, &gl, &LoggingSink).await.unwrap();

    assert!(!out.already);
    assert_eq!(out.total_cost, dec("215000"), "labor 200,000 + parts 15,000");
    let env = gl.last();
    assert!(env.is_balanced());
    let dr: Decimal = env.lines.iter().filter(|l| l.account_id == a.expense).map(|l| l.debit).sum();
    assert_eq!(dr, dec("215000"));
    let parts_cr: Decimal = env.lines.iter().filter(|l| l.account_id == a.parts_inventory).map(|l| l.credit).sum();
    assert_eq!(parts_cr, dec("15000"));
    let labor_cr: Decimal = env.lines.iter().filter(|l| l.account_id == a.labor_payable).map(|l| l.credit).sum();
    assert_eq!(labor_cr, dec("200000"));
    assert_eq!(env.source_type, "maintenance");
}

// MGC-2 — completing a posted visit is idempotent (the ledger is hit once).
#[tokio::test]
async fn mgc2_complete_idempotent() {
    let pool = pool().await;
    let company = Uuid::new_v4();
    let a = mx_accounts(&pool, company).await;
    let svc = MaintenanceWriteService::new(pool.clone());
    let inv = FakeInventory::new("5000");
    let visit = svc.plan_visit(new_visit(company, Uuid::new_v4(), None, "100000", &a)).await.unwrap();
    let gl = CountingGl::new();

    let first = svc.complete_visit(visit, today(), &inv, &gl, &LoggingSink).await.unwrap();
    let second = svc.complete_visit(visit, today(), &inv, &gl, &LoggingSink).await.unwrap();
    assert!(!first.already);
    assert!(second.already);
    assert_eq!(gl.count(), 1, "the ledger is hit exactly once");
}

// MGC-3 — a zero-cost visit (no parts, no labor) completes WITHOUT a posting.
#[tokio::test]
async fn mgc3_zero_cost_visit_no_post() {
    let pool = pool().await;
    let company = Uuid::new_v4();
    let a = mx_accounts(&pool, company).await;
    let svc = MaintenanceWriteService::new(pool.clone());
    let visit = svc.plan_visit(new_visit(company, Uuid::new_v4(), None, "0", &a)).await.unwrap();
    let gl = CountingGl::new();

    let out = svc.complete_visit(visit, today(), &FakeInventory::new("5000"), &gl, &LoggingSink).await.unwrap();
    assert_eq!(out.total_cost, Decimal::ZERO);
    assert!(out.journal_id.is_none(), "no journal for a zero-cost visit");
    assert_eq!(gl.count(), 0, "the ledger is not touched");
}

// MGC-4 — parts are valued by inventory's number, and the event carries the rolled-up costs.
#[tokio::test]
async fn mgc4_parts_valued_by_inventory() {
    let pool = pool().await;
    let company = Uuid::new_v4();
    let warehouse = Uuid::new_v4();
    let a = mx_accounts(&pool, company).await;
    let svc = MaintenanceWriteService::new(pool.clone());
    let inv = FakeInventory::new("2500");
    let sink = CapturingSink::new();

    let visit = svc.plan_visit(new_visit(company, Uuid::new_v4(), Some(warehouse), "0", &a)).await.unwrap();
    svc.add_part(visit, Uuid::new_v4(), dec("4")).await.unwrap(); // 4 × 2500 = 10,000
    svc.complete_visit(visit, today(), &inv, &CountingGl::new(), &sink).await.unwrap();

    let parts: Decimal = sqlx::query_scalar("SELECT parts_cost FROM maintenance.maintenance_visits WHERE id=$1")
        .bind(visit).fetch_one(&pool).await.unwrap();
    assert_eq!(parts, dec("10000"), "parts valued at inventory's rate");
    let last = sink.events.lock().unwrap().last().cloned().unwrap();
    match last {
        MaintenanceEvent::MaintenanceCompleted(c) => assert_eq!(c.parts_cost, dec("10000")),
        // The visit engine never emits the request-family events; listed so this match stays
        // exhaustive as the event enum grows.
        MaintenanceEvent::MaintenanceRequestStageChanged(_) | MaintenanceEvent::SuccessorSpawned(_) => {
            panic!("visit completion must not emit request-family events")
        }
    }
}

// MGC-5 — completing a preventive visit advances its schedule so the next one becomes due (completeness
// council 2026-07-10). Without it a preventive schedule is frozen overdue forever.
#[tokio::test]
async fn mgc5_preventive_completion_advances_schedule() {
    use chrono::NaiveDate;
    let pool = pool().await;
    let company = Uuid::new_v4();
    let asset = Uuid::new_v4();
    let a = mx_accounts(&pool, company).await;
    let svc = MaintenanceWriteService::new(pool.clone());

    let due = NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
    let schedule = svc.create_schedule(NewSchedule {
        company_id: company, asset_id: asset, name: "90-day service".into(),
        interval_days: 90, next_due_date: due,
    }).await.unwrap();

    // A preventive visit fulfilling the schedule, completed on the due date.
    let visit = svc.plan_visit(NewVisit {
        company_id: company, asset_id: asset, schedule_id: Some(schedule), maintenance_type: "preventive".into(),
        scheduled_date: due, warehouse_id: None, warranty_claim_id: None, labor_cost: dec("100000"),
        maintenance_expense_account_id: a.expense, parts_inventory_account_id: a.parts_inventory,
        labor_payable_account_id: a.labor_payable,
    }).await.unwrap();
    svc.complete_visit(visit, due, &FakeInventory::new("5000"), &CountingGl::new(), &LoggingSink).await.unwrap();

    let next: NaiveDate = sqlx::query_scalar("SELECT next_due_date FROM maintenance.maintenance_schedules WHERE id=$1")
        .bind(schedule).fetch_one(&pool).await.unwrap();
    assert_eq!(next, NaiveDate::from_ymd_opt(2026, 9, 29).unwrap(), "next due = performed + 90 days");
}
