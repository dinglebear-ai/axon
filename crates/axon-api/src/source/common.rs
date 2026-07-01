use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::enums::*;
use super::ids::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPolicy {
    pub mode: ExecutionMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_timeout_secs: Option<u64>,
    pub priority: JobPriority,
    pub detached: bool,
    pub heartbeat_interval_secs: u64,
}

impl Default for ExecutionPolicy {
    fn default() -> Self {
        Self {
            mode: ExecutionMode::Background,
            wait_timeout_secs: None,
            priority: JobPriority::Normal,
            detached: false,
            heartbeat_interval_secs: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputPolicy {
    pub json: bool,
    pub response_mode: ResponseMode,
    pub inline_limit_bytes: u64,
    pub artifact_mode: ArtifactMode,
    pub include_progress: bool,
}

impl Default for OutputPolicy {
    fn default() -> Self {
        Self {
            json: false,
            response_mode: ResponseMode::Auto,
            inline_limit_bytes: 64 * 1024,
            artifact_mode: ArtifactMode::OnLargeOutput,
            include_progress: false,
        }
    }
}

#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(deny_unknown_fields)]
pub struct SourceLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_items: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_pages: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes_per_item: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_total_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_chunks: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_timeout_ms: Option<u64>,
}

#[derive(
    Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(deny_unknown_fields)]
pub struct AdapterOptions {
    pub values: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AdapterRef {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContentRef {
    InlineText {
        text: String,
    },
    InlineBytes {
        bytes_base64: String,
        mime_type: String,
    },
    Artifact {
        artifact_id: ArtifactId,
    },
    External {
        uri: String,
        integrity: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceWarning {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_item_key: Option<SourceItemKey>,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceError {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_item_key: Option<SourceItemKey>,
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<ProviderId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AuthorityHint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_uri: Option<String>,
    pub authority: AuthorityLevel,
    pub evidence: Vec<AuthorityEvidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AuthorityEvidence {
    pub evidence_kind: String,
    pub value: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AdapterCandidate {
    pub adapter: AdapterRef,
    pub supported_scopes: Vec<SourceScope>,
    pub confidence: f32,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProviderRequirement {
    pub provider_kind: ProviderKind,
    pub capability: String,
    pub required: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CredentialRequirement {
    pub credential_kind: CredentialKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_ref: Option<SecretRef>,
    pub required: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SecretRef {
    pub provider: String,
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ChunkHint {
    pub profile: ChunkProfile,
    pub reason: String,
    pub options: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ParserHint {
    pub parser_id: String,
    pub reason: String,
    pub options: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChunkProfile {
    CodeAst,
    Markdown,
    Html,
    PlainText,
    Transcript,
    Structured,
    Session,
    BinaryMetadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobStagePlan {
    pub phase: PipelinePhase,
    pub required: bool,
    pub provider_requirements: Vec<ProviderRequirement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_items: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRef {
    pub artifact_id: ArtifactId,
    pub artifact_kind: ArtifactKind,
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    pub created_at: Timestamp,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    RawContent,
    NormalizedContent,
    Manifest,
    Report,
    Screenshot,
    Warc,
    ProviderTrace,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FetchPlan {
    pub uri: String,
    pub method: String,
    pub headers: RedactedHeaders,
    pub render_required: bool,
    pub cache_policy: CachePolicy,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CachePolicy {
    Bypass,
    Use,
    Revalidate,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RedactedHeaders {
    pub headers: Vec<RedactedHeader>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RedactedHeader {
    pub name: String,
    pub value: String,
    pub redacted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceRange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_start: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_end: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_start: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_end: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_start_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_end_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dom_selector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json_pointer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yaml_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xml_xpath: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub csv_row: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_turn_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceCounts {
    pub items_total: u64,
    pub items_changed: u64,
    pub documents_total: u64,
    pub chunks_total: u64,
    pub vector_points_total: u64,
    pub bytes_total: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SecurityDecision {
    pub allowed: bool,
    pub scope: String,
    pub reason: String,
    pub redactions: Vec<String>,
    pub warnings: Vec<SourceWarning>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CallerContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    pub transport: TransportKind,
    pub scopes: Vec<String>,
    pub visibility_ceiling: Visibility,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    Cli,
    Rest,
    Mcp,
    Watch,
    Worker,
    System,
}
