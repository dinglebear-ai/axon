use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::*;
use super::document::CleanupSelector;
use super::enums::*;
use super::ids::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceGeneration {
    pub source_id: SourceId,
    pub generation: SourceGenerationId,
    pub status: LifecycleStatus,
    pub created_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<Timestamp>,
    pub item_counts: ItemCounts,
    pub document_counts: DocumentCounts,
    pub cleanup_debt: Vec<CleanupDebtId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_generation: Option<SourceGenerationId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ItemCounts {
    pub added: u64,
    pub modified: u64,
    pub removed: u64,
    pub unchanged: u64,
    pub failed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DocumentCounts {
    pub discovered: u64,
    pub prepared: u64,
    pub embedded: u64,
    pub published: u64,
    pub failed: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CleanupDebt {
    pub debt_id: CleanupDebtId,
    pub job_id: JobId,
    pub source_id: SourceId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<SourceGenerationId>,
    pub kind: CleanupDebtKind,
    pub selector: CleanupSelector,
    pub status: LifecycleStatus,
    pub created_at: Timestamp,
    pub attempts: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<SourceError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_retry_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LeaseRequest {
    pub lease_key: String,
    pub owner_id: String,
    pub ttl_seconds: u64,
    pub acquired_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<JobId>,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LeaseGuard {
    pub lease_id: LeaseId,
    pub lease_key: String,
    pub owner_id: String,
    pub acquired_at: Timestamp,
    pub expires_at: Timestamp,
    pub heartbeat_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<JobId>,
    pub metadata: MetadataMap,
}
