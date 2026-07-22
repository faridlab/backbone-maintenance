use chrono::{DateTime, Utc, NaiveDate};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use rust_decimal::Decimal;

use super::MaintenanceType;
use super::VisitStatus;
use super::AuditMetadata;

/// Strongly-typed ID for MaintenanceVisit
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MaintenanceVisitId(pub Uuid);

impl MaintenanceVisitId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MaintenanceVisitId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MaintenanceVisitId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MaintenanceVisitId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MaintenanceVisitId> for Uuid {
    fn from(id: MaintenanceVisitId) -> Self { id.0 }
}

impl AsRef<Uuid> for MaintenanceVisitId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MaintenanceVisitId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MaintenanceVisit {
    pub id: Uuid,
    pub company_id: Uuid,
    pub asset_id: Uuid,
    pub schedule_id: Option<Uuid>,
    pub maintenance_type: MaintenanceType,
    pub status: VisitStatus,
    pub warehouse_id: Option<Uuid>,
    pub warranty_claim_id: Option<Uuid>,
    pub scheduled_date: NaiveDate,
    pub performed_date: Option<NaiveDate>,
    pub labor_cost: Decimal,
    pub parts_cost: Decimal,
    pub total_cost: Decimal,
    pub maintenance_expense_account_id: Option<Uuid>,
    pub parts_inventory_account_id: Option<Uuid>,
    pub labor_payable_account_id: Option<Uuid>,
    pub journal_id: Option<Uuid>,
    pub accounting_post_id: Option<Uuid>,
    pub notes: Option<String>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl MaintenanceVisit {
    /// Create a builder for MaintenanceVisit
    pub fn builder() -> MaintenanceVisitBuilder {
        MaintenanceVisitBuilder::default()
    }

    /// Create a new MaintenanceVisit with required fields
    pub fn new(company_id: Uuid, asset_id: Uuid, maintenance_type: MaintenanceType, status: VisitStatus, scheduled_date: NaiveDate, labor_cost: Decimal, parts_cost: Decimal, total_cost: Decimal) -> Self {
        Self {
            id: Uuid::new_v4(),
            company_id,
            asset_id,
            schedule_id: None,
            maintenance_type,
            status,
            warehouse_id: None,
            warranty_claim_id: None,
            scheduled_date,
            performed_date: None,
            labor_cost,
            parts_cost,
            total_cost,
            maintenance_expense_account_id: None,
            parts_inventory_account_id: None,
            labor_payable_account_id: None,
            journal_id: None,
            accounting_post_id: None,
            notes: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MaintenanceVisitId {
        MaintenanceVisitId(self.id)
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

    /// Get the current status
    pub fn status(&self) -> &VisitStatus {
        &self.status
    }


    // ==========================================================
    // Fluent Setters (with_* for optional fields)
    // ==========================================================

    /// Set the schedule_id field (chainable)
    pub fn with_schedule_id(mut self, value: Uuid) -> Self {
        self.schedule_id = Some(value);
        self
    }

    /// Set the warehouse_id field (chainable)
    pub fn with_warehouse_id(mut self, value: Uuid) -> Self {
        self.warehouse_id = Some(value);
        self
    }

    /// Set the warranty_claim_id field (chainable)
    pub fn with_warranty_claim_id(mut self, value: Uuid) -> Self {
        self.warranty_claim_id = Some(value);
        self
    }

    /// Set the performed_date field (chainable)
    pub fn with_performed_date(mut self, value: NaiveDate) -> Self {
        self.performed_date = Some(value);
        self
    }

    /// Set the maintenance_expense_account_id field (chainable)
    pub fn with_maintenance_expense_account_id(mut self, value: Uuid) -> Self {
        self.maintenance_expense_account_id = Some(value);
        self
    }

    /// Set the parts_inventory_account_id field (chainable)
    pub fn with_parts_inventory_account_id(mut self, value: Uuid) -> Self {
        self.parts_inventory_account_id = Some(value);
        self
    }

    /// Set the labor_payable_account_id field (chainable)
    pub fn with_labor_payable_account_id(mut self, value: Uuid) -> Self {
        self.labor_payable_account_id = Some(value);
        self
    }

    /// Set the journal_id field (chainable)
    pub fn with_journal_id(mut self, value: Uuid) -> Self {
        self.journal_id = Some(value);
        self
    }

    /// Set the accounting_post_id field (chainable)
    pub fn with_accounting_post_id(mut self, value: Uuid) -> Self {
        self.accounting_post_id = Some(value);
        self
    }

    /// Set the notes field (chainable)
    pub fn with_notes(mut self, value: String) -> Self {
        self.notes = Some(value);
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
                "asset_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.asset_id = v; }
                }
                "schedule_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.schedule_id = v; }
                }
                "maintenance_type" => {
                    if let Ok(v) = serde_json::from_value(value) { self.maintenance_type = v; }
                }
                "status" => {
                    if let Ok(v) = serde_json::from_value(value) { self.status = v; }
                }
                "warehouse_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.warehouse_id = v; }
                }
                "warranty_claim_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.warranty_claim_id = v; }
                }
                "scheduled_date" => {
                    if let Ok(v) = serde_json::from_value(value) { self.scheduled_date = v; }
                }
                "performed_date" => {
                    if let Ok(v) = serde_json::from_value(value) { self.performed_date = v; }
                }
                "labor_cost" => {
                    if let Ok(v) = serde_json::from_value(value) { self.labor_cost = v; }
                }
                "parts_cost" => {
                    if let Ok(v) = serde_json::from_value(value) { self.parts_cost = v; }
                }
                "total_cost" => {
                    if let Ok(v) = serde_json::from_value(value) { self.total_cost = v; }
                }
                "maintenance_expense_account_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.maintenance_expense_account_id = v; }
                }
                "parts_inventory_account_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.parts_inventory_account_id = v; }
                }
                "labor_payable_account_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.labor_payable_account_id = v; }
                }
                "journal_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.journal_id = v; }
                }
                "accounting_post_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.accounting_post_id = v; }
                }
                "notes" => {
                    if let Ok(v) = serde_json::from_value(value) { self.notes = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for MaintenanceVisit {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "MaintenanceVisit"
    }
}

impl backbone_core::PersistentEntity for MaintenanceVisit {
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

impl backbone_orm::EntityRepoMeta for MaintenanceVisit {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("company_id".to_string(), "uuid".to_string());
        m.insert("asset_id".to_string(), "uuid".to_string());
        m.insert("schedule_id".to_string(), "uuid".to_string());
        m.insert("warehouse_id".to_string(), "uuid".to_string());
        m.insert("warranty_claim_id".to_string(), "uuid".to_string());
        m.insert("maintenance_expense_account_id".to_string(), "uuid".to_string());
        m.insert("parts_inventory_account_id".to_string(), "uuid".to_string());
        m.insert("labor_payable_account_id".to_string(), "uuid".to_string());
        m.insert("journal_id".to_string(), "uuid".to_string());
        m.insert("accounting_post_id".to_string(), "uuid".to_string());
        m.insert("maintenance_type".to_string(), "maintenance_type".to_string());
        m.insert("status".to_string(), "visit_status".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
    fn company_field() -> Option<&'static str> {
        Some("company_id")
    }
}

/// Builder for MaintenanceVisit entity
///
/// Provides a fluent API for constructing MaintenanceVisit instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MaintenanceVisitBuilder {
    company_id: Option<Uuid>,
    asset_id: Option<Uuid>,
    schedule_id: Option<Uuid>,
    maintenance_type: Option<MaintenanceType>,
    status: Option<VisitStatus>,
    warehouse_id: Option<Uuid>,
    warranty_claim_id: Option<Uuid>,
    scheduled_date: Option<NaiveDate>,
    performed_date: Option<NaiveDate>,
    labor_cost: Option<Decimal>,
    parts_cost: Option<Decimal>,
    total_cost: Option<Decimal>,
    maintenance_expense_account_id: Option<Uuid>,
    parts_inventory_account_id: Option<Uuid>,
    labor_payable_account_id: Option<Uuid>,
    journal_id: Option<Uuid>,
    accounting_post_id: Option<Uuid>,
    notes: Option<String>,
}

impl MaintenanceVisitBuilder {
    /// Set the company_id field (required)
    pub fn company_id(mut self, value: Uuid) -> Self {
        self.company_id = Some(value);
        self
    }

    /// Set the asset_id field (required)
    pub fn asset_id(mut self, value: Uuid) -> Self {
        self.asset_id = Some(value);
        self
    }

    /// Set the schedule_id field (optional)
    pub fn schedule_id(mut self, value: Uuid) -> Self {
        self.schedule_id = Some(value);
        self
    }

    /// Set the maintenance_type field (default: `MaintenanceType::default()`)
    pub fn maintenance_type(mut self, value: MaintenanceType) -> Self {
        self.maintenance_type = Some(value);
        self
    }

    /// Set the status field (default: `VisitStatus::default()`)
    pub fn status(mut self, value: VisitStatus) -> Self {
        self.status = Some(value);
        self
    }

    /// Set the warehouse_id field (optional)
    pub fn warehouse_id(mut self, value: Uuid) -> Self {
        self.warehouse_id = Some(value);
        self
    }

    /// Set the warranty_claim_id field (optional)
    pub fn warranty_claim_id(mut self, value: Uuid) -> Self {
        self.warranty_claim_id = Some(value);
        self
    }

    /// Set the scheduled_date field (required)
    pub fn scheduled_date(mut self, value: NaiveDate) -> Self {
        self.scheduled_date = Some(value);
        self
    }

    /// Set the performed_date field (optional)
    pub fn performed_date(mut self, value: NaiveDate) -> Self {
        self.performed_date = Some(value);
        self
    }

    /// Set the labor_cost field (default: `Decimal::from(0)`)
    pub fn labor_cost(mut self, value: Decimal) -> Self {
        self.labor_cost = Some(value);
        self
    }

    /// Set the parts_cost field (default: `Decimal::from(0)`)
    pub fn parts_cost(mut self, value: Decimal) -> Self {
        self.parts_cost = Some(value);
        self
    }

    /// Set the total_cost field (default: `Decimal::from(0)`)
    pub fn total_cost(mut self, value: Decimal) -> Self {
        self.total_cost = Some(value);
        self
    }

    /// Set the maintenance_expense_account_id field (optional)
    pub fn maintenance_expense_account_id(mut self, value: Uuid) -> Self {
        self.maintenance_expense_account_id = Some(value);
        self
    }

    /// Set the parts_inventory_account_id field (optional)
    pub fn parts_inventory_account_id(mut self, value: Uuid) -> Self {
        self.parts_inventory_account_id = Some(value);
        self
    }

    /// Set the labor_payable_account_id field (optional)
    pub fn labor_payable_account_id(mut self, value: Uuid) -> Self {
        self.labor_payable_account_id = Some(value);
        self
    }

    /// Set the journal_id field (optional)
    pub fn journal_id(mut self, value: Uuid) -> Self {
        self.journal_id = Some(value);
        self
    }

    /// Set the accounting_post_id field (optional)
    pub fn accounting_post_id(mut self, value: Uuid) -> Self {
        self.accounting_post_id = Some(value);
        self
    }

    /// Set the notes field (optional)
    pub fn notes(mut self, value: String) -> Self {
        self.notes = Some(value);
        self
    }

    /// Build the MaintenanceVisit entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<MaintenanceVisit, String> {
        let company_id = self.company_id.ok_or_else(|| "company_id is required".to_string())?;
        let asset_id = self.asset_id.ok_or_else(|| "asset_id is required".to_string())?;
        let scheduled_date = self.scheduled_date.ok_or_else(|| "scheduled_date is required".to_string())?;

        Ok(MaintenanceVisit {
            id: Uuid::new_v4(),
            company_id,
            asset_id,
            schedule_id: self.schedule_id,
            maintenance_type: self.maintenance_type.unwrap_or(MaintenanceType::default()),
            status: self.status.unwrap_or(VisitStatus::default()),
            warehouse_id: self.warehouse_id,
            warranty_claim_id: self.warranty_claim_id,
            scheduled_date,
            performed_date: self.performed_date,
            labor_cost: self.labor_cost.unwrap_or(Decimal::from(0)),
            parts_cost: self.parts_cost.unwrap_or(Decimal::from(0)),
            total_cost: self.total_cost.unwrap_or(Decimal::from(0)),
            maintenance_expense_account_id: self.maintenance_expense_account_id,
            parts_inventory_account_id: self.parts_inventory_account_id,
            labor_payable_account_id: self.labor_payable_account_id,
            journal_id: self.journal_id,
            accounting_post_id: self.accounting_post_id,
            notes: self.notes,
            metadata: AuditMetadata::default(),
        })
    }
}
