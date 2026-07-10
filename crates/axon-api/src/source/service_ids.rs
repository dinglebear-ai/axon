//! Minimal DTO additions for the `axon-services` service-trait seam
//! (docs/pipeline-unification/foundation/types/service-contract.md).
//!
//! These are intentionally small: `ResetId` reuses the existing
//! `string_id!`-style newtype shape, `DeleteResult` is a generic delete
//! acknowledgement, and `SourceItemListRequest`/`SourceGenerationListRequest`
//! mirror the existing `ChunkListRequest`/`ChunkGetRequest` pagination shape
//! in `listing.rs` for the two `SourceService` listing methods that don't yet
//! have a backing free function.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::ids::SourceId;

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Default,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    utoipa::ToSchema,
)]
#[schema(value_type = String)]
pub struct ResetId(pub String);

impl ResetId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl From<String> for ResetId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// Generic acknowledgement for a single-entity delete operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteResult {
    pub deleted: bool,
    pub id: String,
}

/// Paginated listing request for the items within one source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceItemListRequest {
    pub source_id: SourceId,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

/// Paginated listing request for the generations of one source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceGenerationListRequest {
    pub source_id: SourceId,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}
