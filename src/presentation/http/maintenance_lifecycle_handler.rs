//! The validated maintenance lifecycle write surface (hand-authored, user-owned).
//!
//! These five verbs are the ONLY way a maintenance visit's state may change — generic CRUD on the
//! visit/part tables is read-only by default (see [`crate::MaintenanceModule::read_only_routes`]).
//! Each handler delegates to [`MaintenanceWriteService`]; on completion the engine issues parts
//! through the injected [`InventoryPort`], posts the balanced cost journal through the [`GlPostSink`],
//! and publishes `MaintenanceCompleted` through the [`MaintenanceEventSink`] — so inventory, the
//! books, and the GL can never diverge. Completion is idempotent per visit.
//!
//! Mounted via [`crate::MaintenanceModule::lifecycle_routes`]; the production surface composes as
//! `module.read_only_routes().merge(module.lifecycle_routes())` (reads + validated writes). Do NOT
//! merge with [`crate::MaintenanceModule::all_crud_routes`] — both mount `POST /maintenance_visits`.
//!
//! NOTE: `plan_visit` reads `company_id` from the request body. A composing service that wires
//! `company_auth` middleware should instead take it from the authenticated tenant and reject
//! body-supplied values. Every later verb self-scopes from the visit's own company, so only
//! `plan_visit` needs it.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::application::service::{
    GlPostSink, InventoryPort, MaintenanceError as WriteError, MaintenanceEventSink,
    MaintenanceWriteService, NewVisit,
};

/// Shared state for the lifecycle routes: the write service plus the three ports the completion verb
/// issues/posts/publishes through. `Clone` (everything is behind an `Arc`), so axum can hand a copy to
/// each request. Only `complete_visit` touches the ports; the other four verbs self-scope from the
/// visit's own company and need none — but the ports are required to mount the router so a missing
/// adapter fails at startup, not on the first completion.
#[derive(Clone)]
pub struct MaintenanceLifecycleState {
    pub write_svc: Arc<MaintenanceWriteService>,
    pub inventory: Arc<dyn InventoryPort>,
    pub gl: Arc<dyn GlPostSink>,
    pub event_sink: Arc<dyn MaintenanceEventSink>,
}

/// Local wrapper around the engine `MaintenanceError` so we can impl axum's `IntoResponse` (foreign
/// trait + foreign type → orphan rule). Maps each domain failure to a stable HTTP code + contract
/// error string.
pub struct MaintenanceApiError(pub WriteError);

impl IntoResponse for MaintenanceApiError {
    fn into_response(self) -> axum::response::Response {
        let msg = self.0.to_string();
        let (status, code) = match self.0 {
            WriteError::NotFound(_) => (StatusCode::NOT_FOUND, "MAINTENANCE_NOT_FOUND"),
            WriteError::InvalidState(_) => (StatusCode::CONFLICT, "MAINTENANCE_INVALID_STATE"),
            WriteError::Invalid(_) => (StatusCode::BAD_REQUEST, "MAINTENANCE_INVALID_INPUT"),
            // The engine's balanced-post guard failed, or the GL/inventory rejected the posting —
            // the visit did not move (completion is atomic + idempotent).
            WriteError::Unbalanced
            | WriteError::GlRejected(_)
            | WriteError::InventoryRejected(_) => {
                (StatusCode::FAILED_DEPENDENCY, "MAINTENANCE_POST_REJECTED")
            }
            WriteError::Db(_) => (StatusCode::INTERNAL_SERVER_ERROR, "MAINTENANCE_DATABASE_ERROR"),
        };
        (status, Json(json!({ "success": false, "error": code, "message": msg }))).into_response()
    }
}

// ---------------------------------------------------------------------------
// Request bodies
// ---------------------------------------------------------------------------

/// Plan a new visit. `company_id` is the owning tenant (see the module NOTE on auth).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanVisitRequest {
    pub company_id: Uuid,
    pub asset_id: Uuid,
    #[serde(default)]
    pub schedule_id: Option<Uuid>,
    /// `preventive` (references a schedule) | `corrective` (ad-hoc).
    pub maintenance_type: String,
    pub scheduled_date: NaiveDate,
    #[serde(default)]
    pub warehouse_id: Option<Uuid>,
    #[serde(default)]
    pub warranty_claim_id: Option<Uuid>,
    #[serde(default)]
    pub labor_cost: Decimal,
    pub maintenance_expense_account_id: Uuid,
    pub parts_inventory_account_id: Uuid,
    pub labor_payable_account_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddPartRequest {
    pub item_id: Uuid,
    pub quantity: Decimal,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteVisitRequest {
    pub performed_date: NaiveDate,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Plan a maintenance visit (status = planned). Preventive visits reference a schedule; corrective don't.
pub async fn plan_visit(
    State(st): State<MaintenanceLifecycleState>,
    Json(req): Json<PlanVisitRequest>,
) -> Result<(StatusCode, Json<Value>), MaintenanceApiError> {
    let id = st
        .write_svc
        .plan_visit(NewVisit {
            company_id: req.company_id,
            asset_id: req.asset_id,
            schedule_id: req.schedule_id,
            maintenance_type: req.maintenance_type,
            scheduled_date: req.scheduled_date,
            warehouse_id: req.warehouse_id,
            warranty_claim_id: req.warranty_claim_id,
            labor_cost: req.labor_cost,
            maintenance_expense_account_id: req.maintenance_expense_account_id,
            parts_inventory_account_id: req.parts_inventory_account_id,
            labor_payable_account_id: req.labor_payable_account_id,
        })
        .await
        .map_err(MaintenanceApiError)?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "success": true, "data": { "id": id } })),
    ))
}

/// Start a planned visit (planned → in_progress).
pub async fn start_visit(
    State(st): State<MaintenanceLifecycleState>,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<Value>), MaintenanceApiError> {
    st.write_svc.start_visit(id).await.map_err(MaintenanceApiError)?;
    Ok((StatusCode::OK, Json(json!({ "success": true }))))
}

/// Add a part line to a planned visit (the part set freezes once the visit is in_progress).
pub async fn add_part(
    State(st): State<MaintenanceLifecycleState>,
    Path(id): Path<Uuid>,
    Json(req): Json<AddPartRequest>,
) -> Result<(StatusCode, Json<Value>), MaintenanceApiError> {
    let part_id = st
        .write_svc
        .add_part(id, req.item_id, req.quantity)
        .await
        .map_err(MaintenanceApiError)?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "success": true, "data": { "id": part_id } })),
    ))
}

/// Complete a visit: issue its parts (valued by inventory), roll up the cost, post ONE balanced
/// maintenance-cost journal, advance a preventive schedule, and emit `MaintenanceCompleted`.
/// Idempotent per visit — re-completion returns the original outcome with `already: true`.
pub async fn complete_visit(
    State(st): State<MaintenanceLifecycleState>,
    Path(id): Path<Uuid>,
    Json(req): Json<CompleteVisitRequest>,
) -> Result<(StatusCode, Json<Value>), MaintenanceApiError> {
    let outcome = st
        .write_svc
        .complete_visit(
            id,
            req.performed_date,
            st.inventory.as_ref(),
            st.gl.as_ref(),
            st.event_sink.as_ref(),
        )
        .await
        .map_err(MaintenanceApiError)?;
    Ok((
        StatusCode::OK,
        Json(json!({
            "success": true,
            "data": {
                "visit_id": outcome.visit_id,
                "journal_id": outcome.journal_id,
                "total_cost": outcome.total_cost,
                "already": outcome.already,
            }
        })),
    ))
}

/// Cancel a planned/in_progress visit.
pub async fn cancel_visit(
    State(st): State<MaintenanceLifecycleState>,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<Value>), MaintenanceApiError> {
    st.write_svc.cancel_visit(id).await.map_err(MaintenanceApiError)?;
    Ok((StatusCode::OK, Json(json!({ "success": true }))))
}

/// Mount the five lifecycle verbs. Returns a stateless `Router<()>` ready to merge with
/// [`crate::MaintenanceModule::read_only_routes`].
pub fn create_maintenance_lifecycle_routes(state: MaintenanceLifecycleState) -> Router {
    Router::<MaintenanceLifecycleState>::new()
        .route("/maintenance_visits", post(plan_visit))
        .route("/maintenance_visits/:id/start", post(start_visit))
        .route("/maintenance_visits/:id/parts", post(add_part))
        .route("/maintenance_visits/:id/complete", post(complete_visit))
        .route("/maintenance_visits/:id/cancel", post(cancel_visit))
        .with_state(state)
}
