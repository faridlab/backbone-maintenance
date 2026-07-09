//! Shared test helpers: a live pool, a real-accounting GL adapter, account seeding + ledger balances, a
//! valuing fake inventory, a counting GL sink, and a capturing event sink.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use backbone_accounting::application::service::posting_service::{PostingLine, PostingRequest, PostingService};
use backbone_maintenance::application::service::maintenance_events::{MaintenanceEvent, MaintenanceEventSink};
use backbone_maintenance::application::service::maintenance_gl::{
    AccountingPostEnvelope, GlPostAck, GlPostRejected, GlPostSink,
};
use backbone_maintenance::application::service::maintenance_ports::*;
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

pub fn dburl() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5433/backbone_maintenance".into())
}
pub async fn pool() -> PgPool {
    PgPool::connect(&dburl()).await.expect("connect")
}
pub fn dec(s: &str) -> Decimal {
    s.parse().unwrap()
}
pub fn today() -> chrono::NaiveDate {
    chrono::Utc::now().date_naive()
}

pub async fn account(pool: &PgPool, company: Uuid, code: &str, atype: &str, subtype: &str, normal: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO accounting.accounts
             (id, company_id, account_number, account_code, name, account_type, account_subtype,
              normal_balance, is_header, is_detail, status)
           VALUES ($1,$2,$3,$4,$5,$6::account_type,$7::account_subtype,$8::normal_balance,
                   false,true,'active'::account_status)"#,
    )
    .bind(id).bind(company).bind(code).bind(code).bind(code).bind(atype).bind(subtype).bind(normal)
    .execute(pool).await.expect("seed account");
    id
}

pub async fn balance(pool: &PgPool, account: Uuid) -> Decimal {
    sqlx::query_scalar(
        "SELECT COALESCE(SUM(debit_amount),0) - COALESCE(SUM(credit_amount),0) FROM accounting.ledgers WHERE account_id=$1")
        .bind(account).fetch_one(pool).await.expect("balance")
}

pub struct MxAccounts {
    pub expense: Uuid,
    pub parts_inventory: Uuid,
    pub labor_payable: Uuid,
}
pub async fn mx_accounts(pool: &PgPool, company: Uuid) -> MxAccounts {
    MxAccounts {
        expense: account(pool, company, "6200-MNT", "expense", "operating_expense", "debit").await,
        parts_inventory: account(pool, company, "1400-INV", "asset", "inventory", "debit").await,
        labor_payable: account(pool, company, "2200-LAB", "liability", "current_liability", "credit").await,
    }
}

/// ACL: maintenance's envelope → accounting's PostingRequest against the REAL ledger.
pub struct GlAdapter {
    pub svc: PostingService,
}
impl GlAdapter {
    pub fn new(pool: PgPool) -> Self {
        Self { svc: PostingService::new(pool) }
    }
}
#[async_trait::async_trait]
impl GlPostSink for GlAdapter {
    async fn post(&self, e: &AccountingPostEnvelope) -> Result<GlPostAck, GlPostRejected> {
        let mut r = PostingRequest::original(e.company_id, &e.source_type, e.source_id, e.posting_date);
        r.source_reference = e.source_reference.clone();
        r.posting_type = e.posting_type.clone();
        r.lines = e.lines.iter().map(|l| PostingLine {
            account_id: l.account_id, debit: l.debit, credit: l.credit,
            party_type: l.party_type.clone(), party_id: l.party_id,
            cost_center_id: None, project_id: None, department_id: None, description: l.description.clone(),
        }).collect();
        match self.svc.post(r, None).await {
            Ok(x) => Ok(GlPostAck { post_id: x.post_id, journal_id: x.journal_id, idempotent_reuse: x.idempotent_reuse }),
            Err(x) => Err(GlPostRejected { code: x.code().to_string(), message: x.to_string() }),
        }
    }
}

/// A valuing fake inventory: values each part at `rate` per unit, and records how many issues it saw so a
/// test can assert issue-once idempotency. Deduplicates on the idempotency key.
#[derive(Clone)]
pub struct FakeInventory {
    pub rate: Decimal,
    pub issued_keys: Arc<Mutex<Vec<String>>>,
}
impl FakeInventory {
    pub fn new(rate: &str) -> Self {
        Self { rate: dec(rate), issued_keys: Arc::new(Mutex::new(Vec::new())) }
    }
    pub fn issue_count(&self) -> usize {
        self.issued_keys.lock().unwrap().len()
    }
}
#[async_trait::async_trait]
impl InventoryPort for FakeInventory {
    async fn issue_parts(&self, req: &PartsIssue) -> Result<IssueAck, InventoryRejected> {
        // Idempotent: a repeated key returns the prior valuation without recording a new issue.
        let mut keys = self.issued_keys.lock().unwrap();
        if !keys.contains(&req.idempotency_key) {
            keys.push(req.idempotency_key.clone());
        }
        let lines: Vec<IssuedLineValue> = req.lines.iter().map(|l| IssuedLineValue {
            item_id: l.item_id, quantity: l.quantity, rate: self.rate, value: l.quantity * self.rate,
        }).collect();
        let total_value = lines.iter().map(|l| l.value).sum();
        Ok(IssueAck { total_value, lines })
    }
}

/// A counting GL sink — records each post envelope so tests can assert count + shape without a real ledger.
#[derive(Clone, Default)]
pub struct CountingGl {
    pub posts: Arc<Mutex<Vec<AccountingPostEnvelope>>>,
}
impl CountingGl {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn count(&self) -> usize {
        self.posts.lock().unwrap().len()
    }
    pub fn last(&self) -> AccountingPostEnvelope {
        self.posts.lock().unwrap().last().cloned().expect("a post")
    }
}
#[async_trait::async_trait]
impl GlPostSink for CountingGl {
    async fn post(&self, e: &AccountingPostEnvelope) -> Result<GlPostAck, GlPostRejected> {
        self.posts.lock().unwrap().push(e.clone());
        Ok(GlPostAck { post_id: Uuid::new_v4(), journal_id: Uuid::new_v4(), idempotent_reuse: false })
    }
}

#[derive(Clone, Default)]
pub struct CapturingSink {
    pub events: Arc<Mutex<Vec<MaintenanceEvent>>>,
}
impl CapturingSink {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn completed(&self) -> usize {
        self.events.lock().unwrap().len()
    }
}
impl MaintenanceEventSink for CapturingSink {
    fn publish(&self, event: &MaintenanceEvent) {
        self.events.lock().unwrap().push(event.clone());
    }
}

pub struct DroppingSink;
impl MaintenanceEventSink for DroppingSink {
    fn publish(&self, _e: &MaintenanceEvent) {}
}
