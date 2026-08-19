use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::{AdapterRef, SourceWarning};
use super::enums::{AuthorityLevel, SourceKind};
use super::ids::{
    ArtifactCandidateId, JobId, MetadataMap, SourceGenerationId, SourceId, SourceItemKey, Timestamp,
};

/// Stable Axon -> ArtifactCandidate sink wire-contract version.
pub const ARTIFACT_CANDIDATE_CONTRACT_VERSION: &str = "1";

/// Media type used by serialized ArtifactCandidate sink implementations.
pub const ARTIFACT_CANDIDATE_MEDIA_TYPE: &str =
    "application/vnd.dinglebear.axon.artifact-candidates+json;version=1";

/// Artifact-family hints inferred by discovery/enrichment.
///
/// These are evidence only. Depot remains responsible for authoritative artifact
/// classification and publication state.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactCandidateKindHint {
    Skill,
    Prompt,
    AgentDefinition,
    AgentRuntime,
    McpServer,
    McpConfig,
    AgentPlugin,
    ApmPackage,
    ArdResource,
    Workflow,
    Unknown,
}

/// Rights state observed by Axon for a candidate source/revision.
///
/// Unknown, Restricted, MetadataOnly, and CacheForIndex all fail closed for
/// public byte mirroring. Discovery is never treated as a redistribution grant.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactRedistributionClass {
    MetadataOnly,
    CacheForIndex,
    Redistributable,
    Forkable,
    Restricted,
    #[default]
    Unknown,
}

impl ArtifactRedistributionClass {
    /// Whether evidence currently permits a public sink to mirror source bytes.
    /// Depot may apply stricter policy, but Axon never upgrades unknown or
    /// index-only evidence into redistribution rights.
    pub const fn permits_public_byte_mirroring(self) -> bool {
        matches!(self, Self::Redistributable | Self::Forkable)
    }
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactModificationClass {
    Allowed,
    Restricted,
    Prohibited,
    ReviewRequired,
    #[default]
    Unknown,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactEvidenceKind {
    Discovery,
    Popularity,
    Duplicate,
    Audit,
    Integrity,
    License,
    Notice,
    Attribution,
    Semantic,
    Graph,
    Compatibility,
}

/// One attributable evidence record attached to an artifact candidate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactEvidenceRecord {
    pub kind: ArtifactEvidenceKind,
    pub provider: String,
    pub authority: AuthorityLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    pub values: MetadataMap,
    pub observed_at: Timestamp,
}

/// Provenance resolved from the discovery pointer to a canonical source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactProvenanceEvidence {
    /// Discovery/source provider that produced this observation.
    pub provider: String,
    pub source_kind: SourceKind,
    pub observed_uri: String,
    pub canonical_source_uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_digest: Option<String>,
    pub adapter: AdapterRef,
    pub observed_at: Timestamp,
    pub metadata: MetadataMap,
}

/// License/NOTICE evidence and the conservative redistribution classification
/// derived from it at observation time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactLicenseEvidence {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_expression: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detected_expression: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detection_confidence: Option<f32>,
    pub redistribution: ArtifactRedistributionClass,
    pub modification: ArtifactModificationClass,
    pub evidence: Vec<ArtifactEvidenceRecord>,
    pub notice_refs: Vec<String>,
    pub attribution_refs: Vec<String>,
    pub observed_at: Timestamp,
}

impl ArtifactLicenseEvidence {
    pub const fn permits_public_byte_mirroring(&self) -> bool {
        self.redistribution.permits_public_byte_mirroring()
    }
}

/// Stable and content-aware keys used for candidate de-duplication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactCandidateDedupe {
    /// Stable for the canonical source/ref/path identity even when bytes change.
    pub identity_key: String,
    /// Stable for identical bytes at the same canonical source/ref/path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

/// Axon's typed discovery/enrichment observation for Depot intake.
///
/// The candidate is deliberately byte-free and non-authoritative. It can point
/// at source bytes and carry content hashes/evidence, but it cannot represent a
/// published Artifact revision or grant redistribution/authorization rights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactCandidate {
    pub contract_version: String,
    pub candidate_id: ArtifactCandidateId,
    pub job_id: JobId,
    pub source_id: SourceId,
    pub generation: SourceGenerationId,
    pub source_item_key: SourceItemKey,
    pub canonical_observed_uri: String,
    pub canonical_source_uri: String,
    pub kind_hints: Vec<ArtifactCandidateKindHint>,
    pub dedupe: ArtifactCandidateDedupe,
    pub provenance: ArtifactProvenanceEvidence,
    pub license: ArtifactLicenseEvidence,
    pub discovery_evidence: Vec<ArtifactEvidenceRecord>,
    pub enrichment_evidence: Vec<ArtifactEvidenceRecord>,
    pub observed_metadata: MetadataMap,
    pub observed_at: Timestamp,
    pub warnings: Vec<SourceWarning>,
}

/// Versioned delivery envelope sent to a candidate sink such as Depot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactCandidateBatch {
    pub contract_version: String,
    pub delivery_id: String,
    pub idempotency_key: String,
    pub job_id: JobId,
    pub source_id: SourceId,
    pub generation: SourceGenerationId,
    pub produced_at: Timestamp,
    pub candidates: Vec<ArtifactCandidate>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactCandidateSinkStatus {
    Disabled,
    Accepted,
    Partial,
    Rejected,
}

/// Transport-neutral receipt returned by an ArtifactCandidate sink.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactCandidateSinkResult {
    pub status: ArtifactCandidateSinkStatus,
    pub attempted: u64,
    pub accepted: u64,
    pub rejected: u64,
    pub warnings: Vec<SourceWarning>,
}

/// Sink capability contract. A sink advertises supported serialized contract
/// versions so Axon and Depot can evolve independently.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactCandidateSinkCapability {
    pub name: String,
    pub version: String,
    pub contract_versions: Vec<String>,
    pub max_batch_size: u32,
    pub supports_idempotency: bool,
}
