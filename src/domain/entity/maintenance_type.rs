use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "maintenance_type", rename_all = "snake_case")]
pub enum MaintenanceType {
    Preventive,
    Corrective,
}

impl std::fmt::Display for MaintenanceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Preventive => write!(f, "preventive"),
            Self::Corrective => write!(f, "corrective"),
        }
    }
}

impl FromStr for MaintenanceType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "preventive" => Ok(Self::Preventive),
            "corrective" => Ok(Self::Corrective),
            _ => Err(format!("Unknown MaintenanceType variant: {}", s)),
        }
    }
}

impl Default for MaintenanceType {
    fn default() -> Self {
        Self::Preventive
    }
}
