//! The GL-posting seam against the REAL backbone-accounting ledger — maintenance is the 9th GL producer.
//! Proves the maintenance-cost journal lands balanced, accounting accepts `source_type='maintenance'`, and
//! re-completing reuses the one journal. ZERO normal Cargo edge — the envelope is the wire contract.

mod common;
use common::*;

use backbone_maintenance::application::service::maintenance_events::LoggingSink;
use backbone_maintenance::application::service::maintenance_write_service::*;
use rust_decimal::Decimal;
use uuid::Uuid;

async fn planned(pool: &sqlx::PgPool, svc: &MaintenanceWriteService, a: &MxAccounts, company: Uuid, warehouse: Uuid) -> Uuid {
    let visit = svc.plan_visit(NewVisit {
        company_id: company, asset_id: Uuid::new_v4(), schedule_id: None, maintenance_type: "corrective".into(),
        scheduled_date: today(), warehouse_id: Some(warehouse), warranty_claim_id: None, labor_cost: dec("300000"),
        maintenance_expense_account_id: a.expense, parts_inventory_account_id: a.parts_inventory,
        labor_payable_account_id: a.labor_payable,
    }).await.unwrap();
    svc.add_part(visit, Uuid::new_v4(), dec("2")).await.unwrap(); // 2 × 10,000 = 20,000
    let _ = pool;
    visit
}

// MSEAM-1 — the cost journal lands in the REAL ledger, balanced, accepted as 'maintenance'.
// Dr Maintenance Expense 320,000 · Cr Parts Inventory 20,000 · Cr Labor Payable 300,000.
#[tokio::test]
async fn mseam1_cost_journal_lands_balanced() {
    let pool = pool().await;
    let company = Uuid::new_v4();
    let warehouse = Uuid::new_v4();
    let a = mx_accounts(&pool, company).await;
    let svc = MaintenanceWriteService::new(pool.clone());
    let gl = GlAdapter::new(pool.clone());
    let inv = FakeInventory::new("10000");

    let visit = planned(&pool, &svc, &a, company, warehouse).await;
    let out = svc.complete_visit(visit, today(), &inv, &gl, &LoggingSink).await.expect("real accounting accepts maintenance post");
    assert!(!out.already);

    assert_eq!(balance(&pool, a.expense).await, dec("320000"));
    assert_eq!(balance(&pool, a.parts_inventory).await, dec("-20000")); // credit shows negative
    assert_eq!(balance(&pool, a.labor_payable).await, dec("-300000"));
    let net = balance(&pool, a.expense).await + balance(&pool, a.parts_inventory).await + balance(&pool, a.labor_payable).await;
    assert_eq!(net, Decimal::ZERO, "double-entry: Σ debits = Σ credits");
}

// MSEAM-2 — re-completing reuses the one journal; the ledger is not doubled.
#[tokio::test]
async fn mseam2_recomplete_reuses_journal() {
    let pool = pool().await;
    let company = Uuid::new_v4();
    let warehouse = Uuid::new_v4();
    let a = mx_accounts(&pool, company).await;
    let svc = MaintenanceWriteService::new(pool.clone());
    let gl = GlAdapter::new(pool.clone());
    let inv = FakeInventory::new("10000");

    let visit = planned(&pool, &svc, &a, company, warehouse).await;
    let first = svc.complete_visit(visit, today(), &inv, &gl, &LoggingSink).await.unwrap();
    let second = svc.complete_visit(visit, today(), &inv, &gl, &LoggingSink).await.unwrap();
    assert!(second.already);
    assert_eq!(first.journal_id, second.journal_id);
    assert_eq!(balance(&pool, a.expense).await, dec("320000"), "expense not doubled");
}
