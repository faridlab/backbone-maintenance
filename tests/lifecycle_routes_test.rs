//! Wiring smoke test for the maintenance lifecycle surface (rec #3). Proves the five validated visit
//! verbs are mounted and dispatch through `MaintenanceWriteService` + the injected ports. The engine's
//! behavior is covered by `maintenance_golden_cases` (DB-backed); this covers only the HTTP wiring.
//!
//! Runs without a database: each verb reaches the engine, hits the (dead) lazy pool, and returns a
//! `MaintenanceApiError` whose body carries the `MAINTENANCE_` contract prefix — proving route +
//! state injection + handler + error mapping all wire end-to-end.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Method, Request};
use backbone_maintenance::application::service::{
    AccountingPostEnvelope, GlPostAck, GlPostRejected, GlPostSink, InventoryPort, InventoryRejected,
    IssueAck, MaintenanceEvent, MaintenanceEventSink, PartsIssue,
};
use backbone_maintenance::MaintenanceModule;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

// Stubs that satisfy the port traits. Never called: every verb hits the dead pool before reaching them.
struct StubInventory;
#[async_trait]
impl InventoryPort for StubInventory {
    async fn issue_parts(&self, _req: &PartsIssue) -> Result<IssueAck, InventoryRejected> {
        unreachable!("complete_visit fails at the pool before reaching inventory")
    }
}
struct StubGl;
#[async_trait]
impl GlPostSink for StubGl {
    async fn post(&self, _e: &AccountingPostEnvelope) -> Result<GlPostAck, GlPostRejected> {
        unreachable!("complete_visit fails at the pool before reaching the GL")
    }
}
struct StubSink;
impl MaintenanceEventSink for StubSink {
    fn publish(&self, _e: &MaintenanceEvent) {}
}

fn lifecycle_router() -> axum::Router {
    let pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_secs(1))
        .connect_lazy("postgres://nobody:nobody@localhost:5432/_")
        .expect("lazy pool options parse");
    MaintenanceModule::builder()
        .with_database(pool)
        .with_gl_sink(Arc::new(StubGl))
        .with_inventory_port(Arc::new(StubInventory))
        .with_event_sink(Arc::new(StubSink))
        .build()
        .expect("module builds")
        .lifecycle_routes()
}

async fn assert_verb_dispatches(uri: &str, body: Option<&str>) {
    let router = lifecycle_router();
    let mut builder = Request::builder().method(Method::POST).uri(uri);
    let req = match body {
        Some(b) => {
            builder = builder.header("content-type", "application/json");
            builder.body(Body::from(b.to_string())).unwrap()
        }
        None => builder.body(Body::empty()).unwrap(),
    };
    let resp = router.oneshot(req).await.unwrap();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(
        text.contains("MAINTENANCE_"),
        "lifecycle verb {uri} must dispatch through the engine (expected a MAINTENANCE_ error body, got: {text})"
    );
}

const ID: &str = "00000000-0000-0000-0000-000000000000";
const PLAN_BODY: &str = r#"{"companyId":"00000000-0000-0000-0000-000000000000","assetId":"00000000-0000-0000-0000-000000000000","maintenanceType":"corrective","scheduledDate":"2026-01-01","maintenanceExpenseAccountId":"00000000-0000-0000-0000-000000000000","partsInventoryAccountId":"00000000-0000-0000-0000-000000000000","laborPayableAccountId":"00000000-0000-0000-0000-000000000000"}"#;

#[tokio::test]
async fn lifecycle_verbs_dispatch_through_the_engine() {
    assert_verb_dispatches("/maintenance_visits", Some(PLAN_BODY)).await;
    assert_verb_dispatches(&format!("/maintenance_visits/{ID}/start"), None).await;
    assert_verb_dispatches(
        &format!("/maintenance_visits/{ID}/parts"),
        Some(r#"{"itemId":"00000000-0000-0000-0000-000000000000","quantity":1}"#),
    )
    .await;
    assert_verb_dispatches(
        &format!("/maintenance_visits/{ID}/complete"),
        Some(r#"{"performedDate":"2026-01-01"}"#),
    )
    .await;
    assert_verb_dispatches(&format!("/maintenance_visits/{ID}/cancel"), None).await;
}
