//! The validated maintenance request lifecycle write surface (hand-authored, user-owned).
//!
//! Three verbs mirror the write service — create, recurrence-field update, and the stage
//! `transition` (the only sanctioned door for stage changes; closing a preventive recurring request
//! through it spawns the successor inside the same transaction). Requests deliberately get NO
//! generic write surface in the production composer (see
//! [`crate::MaintenanceModule::read_only_routes`]): a bare PATCH could flip `stage_id` outside the
//! verb — the G-MT5 trigger backstops even that path, but the verb is the sanctioned door.
//!
//! Mounted via [`crate::MaintenanceModule::lifecycle_routes`]; the production surface composes as
//! `module.read_only_routes().merge(module.lifecycle_routes())` (reads + validated writes).
//!
//! NOTE: `create_request` reads `company_id` from the request body. A composing service that wires
//! `company_auth` middleware should instead take it from the authenticated tenant and reject
//! body-supplied values. Every later verb self-scopes from the request's own company.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::application::service::{
    MaintenanceEventSink, MaintenanceRequestUpdate, MaintenanceRequestWriteService,
    NewMaintenanceRequest, RequestWriteError as WriteError,
};
use crate::domain::entity::{MaintenanceType, RequestKanbanState, RequestPriority, RepeatType, RepeatUnit};

/// Shared state for the request lifecycle routes: the write service plus the event sink the
/// transition verb publishes through. `Clone` (everything is behind an `Arc`), so axum can hand a
/// copy to each request.
#[derive(Clone)]
pub struct MaintenanceRequestLifecycleState {
    pub write_svc: Arc<MaintenanceRequestWriteService>,
    pub event_sink: Arc<dyn MaintenanceEventSink>,
}

/// Local wrapper around the engine `RequestWriteError` so we can impl axum's `IntoResponse` (foreign
/// trait + foreign type → orphan rule). Maps each domain failure to a stable HTTP code + contract
/// error string; every guard has a typed 4xx.
pub struct MaintenanceRequestApiError(pub WriteError);

impl IntoResponse for MaintenanceRequestApiError {
    fn into_response(self) -> axum::response::Response {
        let msg = self.0.to_string();
        let (status, code) = match self.0 {
            WriteError::NotFound(_) => (StatusCode::NOT_FOUND, "MAINTENANCE_REQUEST_NOT_FOUND"),
            // G-MT2
            WriteError::ScheduleEndBeforeStart => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "MAINTENANCE_REQUEST_SCHEDULE_END_BEFORE_START",
            ),
            // G-MT3
            WriteError::RepeatIntervalBelowOne => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "MAINTENANCE_REQUEST_REPEAT_INTERVAL_BELOW_ONE",
            ),
            // G-MT7
            WriteError::RepeatUntilMissing => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "MAINTENANCE_REQUEST_REPEAT_UNTIL_MISSING",
            ),
            // G-MT8
            WriteError::CorrectiveCannotRecur => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "MAINTENANCE_REQUEST_CORRECTIVE_CANNOT_RECUR",
            ),
            WriteError::InvalidState(_) => (StatusCode::CONFLICT, "MAINTENANCE_REQUEST_INVALID_STATE"),
            WriteError::Invalid(_) => (StatusCode::BAD_REQUEST, "MAINTENANCE_REQUEST_INVALID_INPUT"),
            WriteError::Db(_) => (StatusCode::INTERNAL_SERVER_ERROR, "MAINTENANCE_REQUEST_DATABASE_ERROR"),
        };
        (status, Json(json!({ "success": false, "error": code, "message": msg }))).into_response()
    }
}

// ---------------------------------------------------------------------------
// Request bodies
// ---------------------------------------------------------------------------

/// File a request. `company_id` is the owning tenant (see the module NOTE on auth). `stageId` is
/// optional — omitted means the first visible stage.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMaintenanceRequestBody {
    pub company_id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub schedule_date: Option<DateTime<Utc>>,
    #[serde(default)]
    pub schedule_end: Option<DateTime<Utc>>,
    #[serde(default)]
    pub duration: Decimal,
    #[serde(default)]
    pub owner_user_id: Option<Uuid>,
    #[serde(default)]
    pub user_id: Option<Uuid>,
    #[serde(default)]
    pub asset_id: Option<Uuid>,
    #[serde(default)]
    pub stage_id: Option<Uuid>,
    #[serde(default)]
    pub kanban_state: RequestKanbanState,
    #[serde(default)]
    pub priority: RequestPriority,
    #[serde(default = "default_maintenance_type")]
    pub maintenance_type: MaintenanceType,
    #[serde(default)]
    pub recurring: bool,
    #[serde(default = "default_repeat_interval")]
    pub repeat_interval: i32,
    #[serde(default)]
    pub repeat_unit: RepeatUnit,
    #[serde(default)]
    pub repeat_type: RepeatType,
    #[serde(default)]
    pub repeat_until: Option<NaiveDate>,
}

fn default_maintenance_type() -> MaintenanceType {
    MaintenanceType::Corrective
}

fn default_repeat_interval() -> i32 {
    1
}

/// Move a request to another stage. `kanbanState` optionally sets the per-stage sub-state in the
/// same statement; omitted, it auto-resets to `normal` on the stage change.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionRequestBody {
    pub target_stage_id: Uuid,
    #[serde(default)]
    pub kanban_state: Option<RequestKanbanState>,
}

/// Change a request's descriptive/recurrence fields. `None` (absent) leaves a field unchanged; the
/// lifecycle columns are not reachable through this body.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMaintenanceRequestBody {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub schedule_date: Option<DateTime<Utc>>,
    #[serde(default)]
    pub schedule_end: Option<DateTime<Utc>>,
    #[serde(default)]
    pub duration: Option<Decimal>,
    #[serde(default)]
    pub owner_user_id: Option<Uuid>,
    #[serde(default)]
    pub user_id: Option<Uuid>,
    #[serde(default)]
    pub asset_id: Option<Uuid>,
    #[serde(default)]
    pub priority: Option<RequestPriority>,
    #[serde(default)]
    pub maintenance_type: Option<MaintenanceType>,
    #[serde(default)]
    pub recurring: Option<bool>,
    #[serde(default)]
    pub repeat_interval: Option<i32>,
    #[serde(default)]
    pub repeat_unit: Option<RepeatUnit>,
    #[serde(default)]
    pub repeat_type: Option<RepeatType>,
    #[serde(default)]
    pub repeat_until: Option<NaiveDate>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// File a maintenance request (stage defaults to the first visible stage).
pub async fn create_request(
    State(st): State<MaintenanceRequestLifecycleState>,
    Json(req): Json<CreateMaintenanceRequestBody>,
) -> Result<(StatusCode, Json<Value>), MaintenanceRequestApiError> {
    let id = st
        .write_svc
        .create_request(NewMaintenanceRequest {
            company_id: req.company_id,
            name: req.name,
            description: req.description,
            schedule_date: req.schedule_date,
            schedule_end: req.schedule_end,
            duration: req.duration,
            owner_user_id: req.owner_user_id,
            user_id: req.user_id,
            asset_id: req.asset_id,
            stage_id: req.stage_id,
            kanban_state: req.kanban_state,
            priority: req.priority,
            maintenance_type: req.maintenance_type,
            recurring: req.recurring,
            repeat_interval: req.repeat_interval,
            repeat_unit: req.repeat_unit,
            repeat_type: req.repeat_type,
            repeat_until: req.repeat_until,
        })
        .await
        .map_err(MaintenanceRequestApiError)?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "success": true, "data": { "id": id } })),
    ))
}

/// Move a request to another stage — the one sanctioned door for stage changes. Closing a
/// preventive recurring request through this verb spawns its successor in the same transaction.
pub async fn transition_request(
    State(st): State<MaintenanceRequestLifecycleState>,
    Path(id): Path<Uuid>,
    Json(req): Json<TransitionRequestBody>,
) -> Result<(StatusCode, Json<Value>), MaintenanceRequestApiError> {
    let outcome = st
        .write_svc
        .transition_request(id, req.target_stage_id, req.kanban_state, st.event_sink.as_ref())
        .await
        .map_err(MaintenanceRequestApiError)?;
    Ok((
        StatusCode::OK,
        Json(json!({
            "success": true,
            "data": {
                "request_id": outcome.request_id,
                "from_stage_id": outcome.from_stage_id,
                "to_stage_id": outcome.to_stage_id,
                "close_date": outcome.close_date,
                "spawned_successor_id": outcome.spawned_successor_id,
                "already": outcome.already,
            }
        })),
    ))
}

/// Change a request's descriptive/recurrence fields (the widened G-MT2 check runs on the merged
/// old-row + update view; the table CHECK backstops raw writers).
pub async fn update_request(
    State(st): State<MaintenanceRequestLifecycleState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateMaintenanceRequestBody>,
) -> Result<(StatusCode, Json<Value>), MaintenanceRequestApiError> {
    st.write_svc
        .update_request(
            id,
            MaintenanceRequestUpdate {
                name: req.name,
                description: req.description,
                schedule_date: req.schedule_date,
                schedule_end: req.schedule_end,
                duration: req.duration,
                owner_user_id: req.owner_user_id,
                user_id: req.user_id,
                asset_id: req.asset_id,
                priority: req.priority,
                maintenance_type: req.maintenance_type,
                recurring: req.recurring,
                repeat_interval: req.repeat_interval,
                repeat_unit: req.repeat_unit,
                repeat_type: req.repeat_type,
                repeat_until: req.repeat_until,
            },
        )
        .await
        .map_err(MaintenanceRequestApiError)?;
    Ok((StatusCode::OK, Json(json!({ "success": true }))))
}

/// Mount the three request verbs. Returns a stateless `Router<()>` ready to merge with
/// [`crate::MaintenanceModule::read_only_routes`].
pub fn create_maintenance_request_lifecycle_routes(state: MaintenanceRequestLifecycleState) -> Router {
    Router::<MaintenanceRequestLifecycleState>::new()
        .route("/maintenance_requests", post(create_request))
        .route("/maintenance_requests/:id/transition", post(transition_request))
        .route("/maintenance_requests/:id", post(update_request))
        .with_state(state)
}
