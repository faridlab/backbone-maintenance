//! Maintenance's inventory seam (hand-authored, user-owned).
//!
//! Maintenance owns no stock. A visit's parts are issued out of `backbone-inventory` on completion, and
//! inventory VALUES them at its moving-average — the parts cost is inventory's number, not a made-up one.
//! Maintenance holds only the `InventoryPort` trait; a composing service (and the seam test) wires it over
//! the real inventory write path. **Zero normal Cargo edge** to inventory.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A request to issue a visit's parts out of a warehouse.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PartsIssue {
    pub company_id: Uuid,
    pub visit_id: Uuid,
    pub warehouse_id: Uuid,
    /// Stable dedup key. A retry of the SAME issue (a crash/error mid-completion) must NOT move stock
    /// twice — the inventory implementation returns the prior result for a repeated key.
    pub idempotency_key: String,
    pub lines: Vec<IssueLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IssueLine {
    pub item_id: Uuid,
    pub quantity: Decimal,
}

/// The valued result of an issue — inventory reports the rate + value it removed for each line.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IssueAck {
    pub total_value: Decimal,
    pub lines: Vec<IssuedLineValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IssuedLineValue {
    pub item_id: Uuid,
    pub quantity: Decimal,
    pub rate: Decimal,
    pub value: Decimal,
}

/// Inventory's rejection of an issue (e.g. insufficient stock).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InventoryRejected {
    pub code: String,
    pub message: String,
}

/// The inventory seam maintenance drives.
#[async_trait::async_trait]
pub trait InventoryPort: Send + Sync {
    /// Remove a visit's parts from stock and return what they were worth.
    async fn issue_parts(&self, req: &PartsIssue) -> Result<IssueAck, InventoryRejected>;
}
