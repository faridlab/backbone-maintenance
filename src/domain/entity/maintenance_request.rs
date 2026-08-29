use chrono::{DateTime, Utc, NaiveDate};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use rust_decimal::Decimal;

use super::RequestKanbanState;
use super::RequestPriority;
use super::MaintenanceType;
use super::RepeatUnit;
use super::RepeatType;
use super::AuditMetadata;

/// Strongly-typed ID for MaintenanceRequest
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MaintenanceRequestId(pub Uuid);

impl MaintenanceRequestId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MaintenanceRequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MaintenanceRequestId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MaintenanceRequestId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MaintenanceRequestId> for Uuid {
    fn from(id: MaintenanceRequestId) -> Self { id.0 }
}

impl AsRef<Uuid> for MaintenanceRequestId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MaintenanceRequestId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MaintenanceRequest {
    pub id: Uuid,
    pub company_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub request_date: NaiveDate,
    pub schedule_date: Option<DateTime<Utc>>,
    pub schedule_end: Option<DateTime<Utc>>,
    pub close_date: Option<NaiveDate>,
    pub duration: Decimal,
    pub owner_user_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub asset_id: Option<Uuid>,
    pub stage_id: Uuid,
    pub kanban_state: RequestKanbanState,
    pub priority: RequestPriority,
    pub maintenance_type: MaintenanceType,
    pub recurring: bool,
    pub repeat_interval: i32,
    pub repeat_unit: RepeatUnit,
    pub repeat_type: RepeatType,
    pub repeat_until: Option<NaiveDate>,
    pub successor_request_id: Option<Uuid>,
    pub successor_of_request_id: Option<Uuid>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl MaintenanceRequest {
    /// Create a builder for MaintenanceRequest
    pub fn builder() -> MaintenanceRequestBuilder {
        <MaintenanceRequestBuilder as Default>::default()
    }

    /// Create a new MaintenanceRequest with required fields
    pub fn new(company_id: Uuid, name: String, request_date: NaiveDate, duration: Decimal, stage_id: Uuid, kanban_state: RequestKanbanState, priority: RequestPriority, maintenance_type: MaintenanceType, recurring: bool, repeat_interval: i32, repeat_unit: RepeatUnit, repeat_type: RepeatType) -> Self {
        Self {
            id: Uuid::new_v4(),
            company_id,
            name,
            description: None,
            request_date,
            schedule_date: None,
            schedule_end: None,
            close_date: None,
            duration,
            owner_user_id: None,
            user_id: None,
            asset_id: None,
            stage_id,
            kanban_state,
            priority,
            maintenance_type,
            recurring,
            repeat_interval,
            repeat_unit,
            repeat_type,
            repeat_until: None,
            successor_request_id: None,
            successor_of_request_id: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MaintenanceRequestId {
        MaintenanceRequestId(self.id)
    }

    /// Get when this entity was created
    pub fn created_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.created_at.as_ref()
    }

    /// Get when this entity was last updated
    pub fn updated_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.updated_at.as_ref()
    }

    /// Check if this entity is soft deleted
    pub fn is_deleted(&self) -> bool {
        self.metadata.deleted_at.is_some()
    }

    /// Check if this entity is active (not deleted)
    pub fn is_active(&self) -> bool {
        self.metadata.deleted_at.is_none()
    }

    /// Get when this entity was deleted
    pub fn deleted_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.deleted_at.as_ref()
    }

    /// Get who created this entity
    pub fn created_by(&self) -> Option<&Uuid> {
        self.metadata.created_by.as_ref()
    }

    /// Get who last updated this entity
    pub fn updated_by(&self) -> Option<&Uuid> {
        self.metadata.updated_by.as_ref()
    }

    /// Get who deleted this entity
    pub fn deleted_by(&self) -> Option<&Uuid> {
        self.metadata.deleted_by.as_ref()
    }


    // ==========================================================
    // Fluent Setters (with_* for optional fields)
    // ==========================================================

    /// Set the description field (chainable)
    pub fn with_description(mut self, value: String) -> Self {
        self.description = Some(value);
        self
    }

    /// Set the schedule_date field (chainable)
    pub fn with_schedule_date(mut self, value: DateTime<Utc>) -> Self {
        self.schedule_date = Some(value);
        self
    }

    /// Set the schedule_end field (chainable)
    pub fn with_schedule_end(mut self, value: DateTime<Utc>) -> Self {
        self.schedule_end = Some(value);
        self
    }

    /// Set the close_date field (chainable)
    pub fn with_close_date(mut self, value: NaiveDate) -> Self {
        self.close_date = Some(value);
        self
    }

    /// Set the owner_user_id field (chainable)
    pub fn with_owner_user_id(mut self, value: Uuid) -> Self {
        self.owner_user_id = Some(value);
        self
    }

    /// Set the user_id field (chainable)
    pub fn with_user_id(mut self, value: Uuid) -> Self {
        self.user_id = Some(value);
        self
    }

    /// Set the asset_id field (chainable)
    pub fn with_asset_id(mut self, value: Uuid) -> Self {
        self.asset_id = Some(value);
        self
    }

    /// Set the repeat_until field (chainable)
    pub fn with_repeat_until(mut self, value: NaiveDate) -> Self {
        self.repeat_until = Some(value);
        self
    }

    /// Set the successor_request_id field (chainable)
    pub fn with_successor_request_id(mut self, value: Uuid) -> Self {
        self.successor_request_id = Some(value);
        self
    }

    /// Set the successor_of_request_id field (chainable)
    pub fn with_successor_of_request_id(mut self, value: Uuid) -> Self {
        self.successor_of_request_id = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "company_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.company_id = v; }
                }
                "name" => {
                    if let Ok(v) = serde_json::from_value(value) { self.name = v; }
                }
                "description" => {
                    if let Ok(v) = serde_json::from_value(value) { self.description = v; }
                }
                "request_date" => {
                    if let Ok(v) = serde_json::from_value(value) { self.request_date = v; }
                }
                "schedule_date" => {
                    if let Ok(v) = serde_json::from_value(value) { self.schedule_date = v; }
                }
                "schedule_end" => {
                    if let Ok(v) = serde_json::from_value(value) { self.schedule_end = v; }
                }
                "close_date" => {
                    if let Ok(v) = serde_json::from_value(value) { self.close_date = v; }
                }
                "duration" => {
                    if let Ok(v) = serde_json::from_value(value) { self.duration = v; }
                }
                "owner_user_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.owner_user_id = v; }
                }
                "user_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.user_id = v; }
                }
                "asset_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.asset_id = v; }
                }
                "stage_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.stage_id = v; }
                }
                "kanban_state" => {
                    if let Ok(v) = serde_json::from_value(value) { self.kanban_state = v; }
                }
                "priority" => {
                    if let Ok(v) = serde_json::from_value(value) { self.priority = v; }
                }
                "maintenance_type" => {
                    if let Ok(v) = serde_json::from_value(value) { self.maintenance_type = v; }
                }
                "recurring" => {
                    if let Ok(v) = serde_json::from_value(value) { self.recurring = v; }
                }
                "repeat_interval" => {
                    if let Ok(v) = serde_json::from_value(value) { self.repeat_interval = v; }
                }
                "repeat_unit" => {
                    if let Ok(v) = serde_json::from_value(value) { self.repeat_unit = v; }
                }
                "repeat_type" => {
                    if let Ok(v) = serde_json::from_value(value) { self.repeat_type = v; }
                }
                "repeat_until" => {
                    if let Ok(v) = serde_json::from_value(value) { self.repeat_until = v; }
                }
                "successor_request_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.successor_request_id = v; }
                }
                "successor_of_request_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.successor_of_request_id = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for MaintenanceRequest {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "MaintenanceRequest"
    }
}

impl backbone_core::PersistentEntity for MaintenanceRequest {
    fn entity_id(&self) -> String {
        self.id.to_string()
    }
    fn set_entity_id(&mut self, id: String) {
        if let Ok(uuid) = uuid::Uuid::parse_str(&id) {
            self.id = uuid;
        }
    }
    fn created_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.created_at
    }
    fn set_created_at(&mut self, ts: chrono::DateTime<chrono::Utc>) {
        self.metadata.created_at = Some(ts);
    }
    fn updated_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.updated_at
    }
    fn set_updated_at(&mut self, ts: chrono::DateTime<chrono::Utc>) {
        self.metadata.updated_at = Some(ts);
    }
    fn deleted_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.deleted_at
    }
    fn set_deleted_at(&mut self, ts: Option<chrono::DateTime<chrono::Utc>>) {
        self.metadata.deleted_at = ts;
    }
}

impl backbone_orm::EntityRepoMeta for MaintenanceRequest {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("company_id".to_string(), "uuid".to_string());
        m.insert("owner_user_id".to_string(), "uuid".to_string());
        m.insert("user_id".to_string(), "uuid".to_string());
        m.insert("asset_id".to_string(), "uuid".to_string());
        m.insert("stage_id".to_string(), "uuid".to_string());
        m.insert("successor_request_id".to_string(), "uuid".to_string());
        m.insert("successor_of_request_id".to_string(), "uuid".to_string());
        m.insert("kanban_state".to_string(), "request_kanban_state".to_string());
        m.insert("priority".to_string(), "request_priority".to_string());
        m.insert("maintenance_type".to_string(), "maintenance_type".to_string());
        m.insert("repeat_unit".to_string(), "repeat_unit".to_string());
        m.insert("repeat_type".to_string(), "repeat_type".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["name"]
    }
    fn company_field() -> Option<&'static str> {
        Some("company_id")
    }
}

/// Builder for MaintenanceRequest entity
///
/// Provides a fluent API for constructing MaintenanceRequest instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MaintenanceRequestBuilder {
    company_id: Option<Uuid>,
    name: Option<String>,
    description: Option<String>,
    request_date: Option<NaiveDate>,
    schedule_date: Option<DateTime<Utc>>,
    schedule_end: Option<DateTime<Utc>>,
    close_date: Option<NaiveDate>,
    duration: Option<Decimal>,
    owner_user_id: Option<Uuid>,
    user_id: Option<Uuid>,
    asset_id: Option<Uuid>,
    stage_id: Option<Uuid>,
    kanban_state: Option<RequestKanbanState>,
    priority: Option<RequestPriority>,
    maintenance_type: Option<MaintenanceType>,
    recurring: Option<bool>,
    repeat_interval: Option<i32>,
    repeat_unit: Option<RepeatUnit>,
    repeat_type: Option<RepeatType>,
    repeat_until: Option<NaiveDate>,
    successor_request_id: Option<Uuid>,
    successor_of_request_id: Option<Uuid>,
}

impl MaintenanceRequestBuilder {
    /// Set the company_id field (required)
    pub fn company_id(mut self, value: Uuid) -> Self {
        self.company_id = Some(value);
        self
    }

    /// Set the name field (required)
    pub fn name(mut self, value: String) -> Self {
        self.name = Some(value);
        self
    }

    /// Set the description field (optional)
    pub fn description(mut self, value: String) -> Self {
        self.description = Some(value);
        self
    }

    /// Set the request_date field (default: `Default::default()`)
    pub fn request_date(mut self, value: NaiveDate) -> Self {
        self.request_date = Some(value);
        self
    }

    /// Set the schedule_date field (optional)
    pub fn schedule_date(mut self, value: DateTime<Utc>) -> Self {
        self.schedule_date = Some(value);
        self
    }

    /// Set the schedule_end field (optional)
    pub fn schedule_end(mut self, value: DateTime<Utc>) -> Self {
        self.schedule_end = Some(value);
        self
    }

    /// Set the close_date field (optional)
    pub fn close_date(mut self, value: NaiveDate) -> Self {
        self.close_date = Some(value);
        self
    }

    /// Set the duration field (default: `Decimal::from(0)`)
    pub fn duration(mut self, value: Decimal) -> Self {
        self.duration = Some(value);
        self
    }

    /// Set the owner_user_id field (optional)
    pub fn owner_user_id(mut self, value: Uuid) -> Self {
        self.owner_user_id = Some(value);
        self
    }

    /// Set the user_id field (optional)
    pub fn user_id(mut self, value: Uuid) -> Self {
        self.user_id = Some(value);
        self
    }

    /// Set the asset_id field (optional)
    pub fn asset_id(mut self, value: Uuid) -> Self {
        self.asset_id = Some(value);
        self
    }

    /// Set the stage_id field (required)
    pub fn stage_id(mut self, value: Uuid) -> Self {
        self.stage_id = Some(value);
        self
    }

    /// Set the kanban_state field (default: `RequestKanbanState::default()`)
    pub fn kanban_state(mut self, value: RequestKanbanState) -> Self {
        self.kanban_state = Some(value);
        self
    }

    /// Set the priority field (required)
    pub fn priority(mut self, value: RequestPriority) -> Self {
        self.priority = Some(value);
        self
    }

    /// Set the maintenance_type field (default: `MaintenanceType::default()`)
    pub fn maintenance_type(mut self, value: MaintenanceType) -> Self {
        self.maintenance_type = Some(value);
        self
    }

    /// Set the recurring field (default: `false`)
    pub fn recurring(mut self, value: bool) -> Self {
        self.recurring = Some(value);
        self
    }

    /// Set the repeat_interval field (default: `1`)
    pub fn repeat_interval(mut self, value: i32) -> Self {
        self.repeat_interval = Some(value);
        self
    }

    /// Set the repeat_unit field (default: `RepeatUnit::default()`)
    pub fn repeat_unit(mut self, value: RepeatUnit) -> Self {
        self.repeat_unit = Some(value);
        self
    }

    /// Set the repeat_type field (default: `RepeatType::default()`)
    pub fn repeat_type(mut self, value: RepeatType) -> Self {
        self.repeat_type = Some(value);
        self
    }

    /// Set the repeat_until field (optional)
    pub fn repeat_until(mut self, value: NaiveDate) -> Self {
        self.repeat_until = Some(value);
        self
    }

    /// Set the successor_request_id field (optional)
    pub fn successor_request_id(mut self, value: Uuid) -> Self {
        self.successor_request_id = Some(value);
        self
    }

    /// Set the successor_of_request_id field (optional)
    pub fn successor_of_request_id(mut self, value: Uuid) -> Self {
        self.successor_of_request_id = Some(value);
        self
    }

    /// Build the MaintenanceRequest entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<MaintenanceRequest, String> {
        let company_id = self.company_id.ok_or_else(|| "company_id is required".to_string())?;
        let name = self.name.ok_or_else(|| "name is required".to_string())?;
        let stage_id = self.stage_id.ok_or_else(|| "stage_id is required".to_string())?;
        let priority = self.priority.ok_or_else(|| "priority is required".to_string())?;

        Ok(MaintenanceRequest {
            id: Uuid::new_v4(),
            company_id,
            name,
            description: self.description,
            request_date: self.request_date.unwrap_or_default(),
            schedule_date: self.schedule_date,
            schedule_end: self.schedule_end,
            close_date: self.close_date,
            duration: self.duration.unwrap_or(Decimal::from(0)),
            owner_user_id: self.owner_user_id,
            user_id: self.user_id,
            asset_id: self.asset_id,
            stage_id,
            kanban_state: self.kanban_state.unwrap_or_default(),
            priority,
            maintenance_type: self.maintenance_type.unwrap_or_default(),
            recurring: self.recurring.unwrap_or(false),
            repeat_interval: self.repeat_interval.unwrap_or(1),
            repeat_unit: self.repeat_unit.unwrap_or_default(),
            repeat_type: self.repeat_type.unwrap_or_default(),
            repeat_until: self.repeat_until,
            successor_request_id: self.successor_request_id,
            successor_of_request_id: self.successor_of_request_id,
            metadata: AuditMetadata::default(),
        })
    }
}
