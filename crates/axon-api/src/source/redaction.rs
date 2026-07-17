//! Transport-neutral redaction report contracts.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Visibility;

/// Outcome recorded for every public payload write.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RedactionStatus {
    Clean,
    Redacted,
    Failed,
}

impl RedactionStatus {
    /// Stable wire value used by persistence projections.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Redacted => "redacted",
            Self::Failed => "failed",
        }
    }
}

/// Bounded redaction provenance carried beside a public write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RedactionMetadata {
    pub redaction_status: RedactionStatus,
    pub redaction_version: String,
    pub visibility: Visibility,
    pub redacted_field_count: u32,
    pub dropped_field_count: u32,
    pub detector_count: u32,
    pub detector_names: Vec<String>,
}

/// Canonical envelope for a payload that crossed the public-write redaction gate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RedactedPublicWrite<T> {
    pub payload: T,
    pub redaction: RedactionMetadata,
}
