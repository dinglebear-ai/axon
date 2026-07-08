//! Durable memory pipeline DTOs.
//!
//! These are transport-neutral data contracts for the `axon-memory` crate. The
//! closed `MemoryType`/`MemoryStatus` enums, decay profiles, and the request/
//! result/record structs are the single wire contract shared by CLI, MCP, REST,
//! and the memory store. Behavior (scoring, decay math, supersession) lives in
//! `axon-memory`; only the shapes live here.
//!
//! Contract: `docs/pipeline-unification/runtime/memory-contract.md`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::*;
use super::enums::Visibility;
use super::graph::*;
use super::ids::*;

/// Closed set of durable memory kinds.
///
/// Contract: "Memory Types" table — `decision`, `fact`, `preference`, `task`,
/// `bug`, `procedure`, `incident`, `entity`, `episode`, `working`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    /// Durable design/implementation decision (slow decay).
    Decision,
    /// Observed factual project/system state (normal decay).
    Fact,
    /// User preference or standing instruction (very slow decay).
    Preference,
    /// Work item or pending follow-up (normal decay).
    Task,
    /// Known defect or failure pattern (normal decay).
    Bug,
    /// Repeatable operational procedure (slow decay).
    Procedure,
    /// Specific outage/failure/investigation (normal decay).
    Incident,
    /// Stable entity profile such as repo/service/person/package (slow decay).
    Entity,
    /// Session or event summary (fast decay).
    Episode,
    /// Short-lived working context (very fast decay).
    Working,
}

impl MemoryType {
    /// Default decay profile for this memory type per the contract table.
    pub fn default_decay_profile(self) -> DecayProfile {
        match self {
            MemoryType::Decision | MemoryType::Procedure | MemoryType::Entity => DecayProfile::Slow,
            MemoryType::Fact | MemoryType::Task | MemoryType::Bug | MemoryType::Incident => {
                DecayProfile::Normal
            }
            MemoryType::Preference => DecayProfile::VerySlow,
            MemoryType::Episode => DecayProfile::Fast,
            MemoryType::Working => DecayProfile::VeryFast,
        }
    }
}

/// Closed set of memory recall statuses.
///
/// Contract: "Memory Status" table — `active`, `review`, `superseded`,
/// `contradicted`, `archived`, `forgotten`, `working`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    /// Eligible for recall.
    Active,
    /// Needs user/agent confirmation.
    Review,
    /// Replaced by another memory.
    Superseded,
    /// Conflicts with another memory and needs resolution.
    Contradicted,
    /// Hidden from normal recall but retained.
    Archived,
    /// Removed from recall and redacted/deleted according to policy.
    Forgotten,
    /// Temporary memory with short TTL.
    Working,
}

/// Decay curve applied to a memory's score over time.
///
/// Contract: "Decay profiles" table.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DecayProfile {
    /// half-life 1 day — working/session scratch.
    VeryFast,
    /// half-life 7 days — episode summaries/short-lived observations.
    Fast,
    /// half-life 30 days — facts, bugs, tasks, incidents.
    Normal,
    /// half-life 180 days — decisions, procedures, entity profiles.
    Slow,
    /// half-life 730 days — durable preferences/standing instructions.
    VerySlow,
    /// infinite — pinned/manual-retention memories.
    None,
}

impl DecayProfile {
    /// Effective half-life in days, or `None` for the `none` (infinite) profile.
    ///
    /// Contract: "Decay profiles" Half-Life Days column.
    pub fn half_life_days(self) -> Option<f64> {
        match self {
            DecayProfile::VeryFast => Some(1.0),
            DecayProfile::Fast => Some(7.0),
            DecayProfile::Normal => Some(30.0),
            DecayProfile::Slow => Some(180.0),
            DecayProfile::VerySlow => Some(730.0),
            DecayProfile::None => None,
        }
    }
}

/// Memory scope controlling recall and visibility.
///
/// Contract: "Scope Model" table (`global`, `project`, `repo`, `file`,
/// `source_id`, `graph_node_id`, `agent`, `user`, `environment`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryScope {
    pub kind: String,
    pub value: String,
}

/// Explicit, inspectable decay configuration.
///
/// Contract: "Decay Contract" required fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryDecayPolicy {
    pub profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub half_life_days: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_reinforced_at: Option<Timestamp>,
    #[serde(default)]
    pub reinforcement_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_after: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<Timestamp>,
    #[serde(default)]
    pub pinned: bool,
}

/// Evidence-backed link from a memory to a source/entity/decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryLink {
    pub link_type: String,
    pub target: String,
    pub confidence: f32,
    pub evidence: Vec<GraphEvidence>,
}

/// Append-only history event recorded on every status/scoring change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryHistoryEvent {
    pub status: MemoryStatus,
    pub message: String,
    pub timestamp: Timestamp,
}

/// Create a durable memory. Contract: "Memory DTOs" — `MemoryRequest`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryRequest {
    pub memory_type: MemoryType,
    pub body: String,
    pub confidence: f32,
    pub salience: f32,
    pub scope: MemoryScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<MemoryLink>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decay: Option<MemoryDecayPolicy>,
    #[serde(default)]
    pub embed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Visibility>,
}

/// Result of a memory mutation. Contract: "Memory DTOs" — `MemoryResult`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryResult {
    pub memory_id: MemoryId,
    pub memory_type: MemoryType,
    pub status: MemoryStatus,
    pub memory_score: f32,
    pub confidence: f32,
    pub salience: f32,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_node_id: Option<GraphNodeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<DocumentId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vector_point_ids: Vec<VectorPointId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<SourceWarning>,
}

/// Full stored memory record. Contract: "Memory DTOs" — `MemoryRecord`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryRecord {
    pub memory_id: MemoryId,
    pub memory_type: MemoryType,
    pub status: MemoryStatus,
    pub body: String,
    pub confidence: f32,
    pub salience: f32,
    pub scope: MemoryScope,
    pub history: Vec<MemoryHistoryEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<MemoryLink>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decay: Option<MemoryDecayPolicy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embedding_refs: Vec<VectorPointId>,
    /// Memory this record was replaced by (`superseded` status only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<MemoryId>,
    /// Memory this record conflicts with (`contradicted` status only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contradicts: Option<MemoryId>,
}

/// Semantic search request. Contract: "Memory DTOs" — `MemorySearchRequest`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemorySearchRequest {
    pub query: String,
    pub limit: u32,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub filters: MetadataMap,
    #[serde(default)]
    pub include_graph: bool,
    #[serde(default)]
    pub include_archived: bool,
    #[serde(default)]
    pub reinforce: bool,
}

/// One scored search hit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemorySearchMatch {
    pub record: MemoryRecord,
    pub score: f32,
}

/// Search result set. Contract: "Memory DTOs" — `MemorySearchResult`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemorySearchResult {
    pub results: Vec<MemorySearchMatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_embedding_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<GraphQueryResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<SourceWarning>,
}

/// Bounded context assembly request. Contract: "Memory DTOs" —
/// `MemoryContextRequest`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryContextRequest {
    pub token_budget: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<SourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_node_id: Option<GraphNodeId>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub filters: MetadataMap,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
    #[serde(default)]
    pub include_working: bool,
}

/// Assembled, cited context. Contract: "Context Assembly" rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryContextResult {
    pub context: String,
    pub memories: Vec<MemoryRecord>,
    pub exclusions: Vec<String>,
    pub token_estimate: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<SourceWarning>,
}

/// Attach an evidence-backed link to a memory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryLinkRequest {
    pub memory_id: MemoryId,
    pub link: MemoryLink,
}

/// Positive-use reinforcement signal. Contract: "Scoring and Recall".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryReinforcement {
    pub amount: f32,
    pub reason: String,
    pub timestamp: Timestamp,
}

/// Replace one memory with another. Contract: supersede lifecycle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemorySupersedeRequest {
    /// The memory being replaced.
    pub memory_id: MemoryId,
    /// The replacement memory.
    pub replacement_id: MemoryId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub timestamp: Timestamp,
}

/// Flag two memories as conflicting. Contract: contradiction handling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryContradictRequest {
    pub memory_id: MemoryId,
    pub conflicting_id: MemoryId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub timestamp: Timestamp,
}

/// Transition a memory to a new status (archive/forget/pin/unpin/review).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryStatusRequest {
    pub memory_id: MemoryId,
    pub status: MemoryStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub timestamp: Timestamp,
}

/// Review-queue request. Contract: "Memory DTOs" — `MemoryReviewRequest`.
#[derive(
    Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(deny_unknown_fields)]
pub struct MemoryReviewRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_type: Option<MemoryType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<MemoryScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Review queue result: memories needing user/agent attention.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryReviewResult {
    pub memories: Vec<MemoryRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<SourceWarning>,
}

/// Edit a memory's editable fields in place. Contract: "Memory DTOs" —
/// `MemoryUpdateRequest`; REST `PATCH /v1/memories/{memory_id}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryUpdateRequest {
    pub memory_id: MemoryId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_type: Option<MemoryType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub salience: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<MemoryScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub timestamp: Timestamp,
}

/// Pin or unpin a memory (exempts it from decay while pinned). Contract:
/// "Memory Service" — `pin`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryPinRequest {
    pub memory_id: MemoryId,
    pub pinned: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub timestamp: Timestamp,
}

/// Archive a memory. Contract: "Memory Service" — `archive`. Thin request
/// shape over the shared status transition (`MemoryStatusRequest`) so
/// transports get a dedicated, self-documenting action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryArchiveRequest {
    pub memory_id: MemoryId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub timestamp: Timestamp,
}

/// Forget (soft-delete) a memory. Contract: "Memory Service" — `forget`.
/// Forgotten memories are never recalled again, but history is preserved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryForgetRequest {
    pub memory_id: MemoryId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub timestamp: Timestamp,
}

/// Merge `memory_ids` into one new memory. Contract: "Memory DTOs" —
/// `MemoryCompactRequest`; REST `POST /v1/memories/compact`.
///
/// `strategy` selects how the compacted `body` is produced. Only
/// `"concatenate"` (deterministic, no LLM) is implemented; other values
/// (e.g. `"semantic_summary"`, which the REST contract example shows) are
/// reserved for a future LLM-backed synthesis strategy and currently reject
/// with `memory.unsupported_strategy`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryCompactRequest {
    pub memory_ids: Vec<MemoryId>,
    pub strategy: String,
    pub result_type: MemoryType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub scope: MemoryScope,
    #[serde(default)]
    pub archive_sources: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    pub timestamp: Timestamp,
}

/// How an import reconciles with existing memories for the same content.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MemoryImportMode {
    /// Add records not already present (deduped by content hash/scope/type);
    /// leave existing memories untouched.
    Merge,
    /// Archive every existing memory in the target scope before importing.
    /// Requires `axon:admin` at the transport boundary.
    ReplaceScope,
}

/// Bulk-import memory records. Contract: "Memory DTOs" —
/// `MemoryImportRequest`; REST `POST /v1/memories/import`.
///
/// Carries records directly rather than an artifact/upload bundle
/// reference — no bundle serialization format is specified anywhere in the
/// pipeline-unification docs. A transport that wants to accept an uploaded
/// bundle can deserialize it into `records` itself; axon-memory does not own
/// artifact/upload storage (out of this crate's boundary).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryImportRequest {
    pub records: Vec<MemoryRecord>,
    pub mode: MemoryImportMode,
    #[serde(default)]
    pub dry_run: bool,
}

/// Result of a memory import (or dry-run plan).
#[derive(
    Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(deny_unknown_fields)]
pub struct MemoryImportResult {
    pub created: u32,
    pub updated: u32,
    pub skipped: u32,
    pub dry_run: bool,
    /// Memory ids actually inserted (empty on a dry run). Lets a decorator
    /// (e.g. vector embedding) act on exactly what was created without a
    /// second lookup.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub created_ids: Vec<MemoryId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<SourceWarning>,
}

/// Export memory records matching a scope. Contract: "Memory DTOs" —
/// `MemoryExportRequest`; REST `GET /v1/memories/export`.
#[derive(
    Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(deny_unknown_fields)]
pub struct MemoryExportRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<MemoryScope>,
    #[serde(default)]
    pub include_archived: bool,
}

/// Exported memory records.
#[derive(
    Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(deny_unknown_fields)]
pub struct MemoryExportResult {
    pub records: Vec<MemoryRecord>,
    pub count: u32,
}
