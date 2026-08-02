//! Regression: the guarded route composer (`MaintenanceModule::read_only_routes`) must NOT expose
//! generic mutable CRUD on the engine-owned `MaintenanceVisit` / `MaintenanceVisitPart` tables —
//! closing the invariant-bypass found by the maturity council.
//!
//! See `docs/council/2026-08-02-module-backbone-maintenance-maturity.md` (recommendation #1).
//! Routing is decided before any handler runs, so these assertions need no database — the module is
//! built against a *lazy* pool that never opens a connection.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use backbone_maintenance::MaintenanceModule;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

/// Build the module against a lazy pool. `connect_lazy` parses options without connecting, and the
/// guarded composer's rejected methods never reach a handler, so this test runs without a database.
fn module_with_lazy_pool() -> MaintenanceModule {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://nobody:nobody@localhost:5432/_")
        .expect("lazy pool options parse");
    MaintenanceModule::builder()
        .with_database(pool)
        .build()
        .expect("module builds")
}

const VISIT_ID: &str = "00000000-0000-0000-0000-000000000000";

#[tokio::test]
async fn read_only_routes_rejects_writes_on_visit() {
    let router = module_with_lazy_pool().read_only_routes();

    // Item-level writes on the visit must not be routed.
    for method in [Method::PUT, Method::PATCH, Method::DELETE] {
        let resp = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(method.clone())
                    .uri(format!("/maintenance_visits/{VISIT_ID}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            matches!(resp.status(), StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED),
            "guarded composer must not route {method} on MaintenanceVisit (got {})",
            resp.status()
        );
    }

    // Collection-level create must not be routed either.
    let resp = router
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/maintenance_visits")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        matches!(resp.status(), StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED),
        "guarded composer must not route POST on the MaintenanceVisit collection (got {})",
        resp.status()
    );
}

#[tokio::test]
async fn read_only_routes_rejects_writes_on_visit_part() {
    let router = module_with_lazy_pool().read_only_routes();
    let resp = router
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/maintenance_visit_parts/{VISIT_ID}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        matches!(resp.status(), StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED),
        "guarded composer must not route PATCH on MaintenanceVisitPart (got {})",
        resp.status()
    );
}

#[tokio::test]
async fn all_crud_routes_keeps_the_write_as_explicit_admin_surface() {
    // Contrast: the generated `all_crud_routes()` still mounts the write — it remains the explicit
    // admin/seeding surface (the closure is in the composer, not a global disappearance). It must
    // match the route (not 404/405); we don't assert success because a lazy pool can't serve it.
    let router = module_with_lazy_pool().all_crud_routes();
    let resp = router
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/maintenance_visits/{VISIT_ID}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        !matches!(resp.status(), StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED),
        "all_crud_routes must keep the visit write route as the admin surface (got {})",
        resp.status()
    );
}

#[tokio::test]
async fn query_service_delivers_the_published_read_contract() {
    // Council rec #2: the published `MaintenanceQueryService` trait is now realized and reachable
    // through the module (it was an unbacked promise before). Compile guarantees all 9 methods are
    // implemented; this proves the builder wires and delivers the contract object at runtime.
    let module = module_with_lazy_pool();
    let _svc: std::sync::Arc<dyn backbone_maintenance::exports::MaintenanceQueryService> =
        module.query_service();
}
