use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "repeat_type", rename_all = "snake_case")]
pub enum RepeatType {
    Forever,
    Until,
}

impl std::fmt::Display for RepeatType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Forever => write!(f, "forever"),
            Self::Until => write!(f, "until"),
        }
    }
}

impl FromStr for RepeatType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "forever" => Ok(Self::Forever),
            "until" => Ok(Self::Until),
            _ => Err(format!("Unknown RepeatType variant: {}", s)),
        }
    }
}

impl Default for RepeatType {
    fn default() -> Self {
        Self::Forever
    }
}
