use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::common::*;
use super::enums::*;
use super::ids::*;
use super::status::ApiError;
use axon_error::ErrorStage;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CapabilityBase {
    pub name: String,
    pub version: String,
    pub owner_crate: String,
    pub health: HealthStatus,
    pub features: Vec<String>,
    pub limits: MetadataMap,
}

macro_rules! capability_newtype {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
        #[serde(transparent)]
        pub struct $name(pub CapabilityBase);

        impl From<CapabilityBase> for $name {
            fn from(value: CapabilityBase) -> Self {
                Self(value)
            }
        }
    };
}

capability_newtype!(SourceResolverCapability);
capability_newtype!(SourceRouterCapability);
capability_newtype!(SourceAdapterCapability);
capability_newtype!(SourceScopeCapability);
capability_newtype!(SourceEnricherCapability);
capability_newtype!(DocumentPreparerCapability);
capability_newtype!(ChunkProfileCapability);
capability_newtype!(ParserCapability);
capability_newtype!(RetrievalCapability);
capability_newtype!(LedgerStoreCapability);
capability_newtype!(GraphStoreCapability);
capability_newtype!(MemoryStoreCapability);
capability_newtype!(JobStoreCapability);
capability_newtype!(WatchStoreCapability);
capability_newtype!(ArtifactStoreCapability);
capability_newtype!(ConfigStoreCapability);
capability_newtype!(DocumentCacheCapability);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProviderCapability {
    pub provider_id: ProviderId,
    pub provider_kind: ProviderKind,
    pub implementation: String,
    pub version: String,
    pub health: HealthStatus,
    pub limits: ProviderLimits,
    pub features: Vec<String>,
    pub cooldown_until: Option<Timestamp>,
    pub last_error: Option<ApiError>,
    pub reservation_policy: ReservationPolicy,
    pub reservation_state: ReservationStateSnapshot,
    pub cost_class: ProviderCostClass,
    pub degraded_modes: Vec<DegradedMode>,
    pub fake_overrides_supported: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<EmbeddingProviderCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm: Option<LlmProviderCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vector_store: Option<VectorStoreCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetch: Option<FetchProviderCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render: Option<RenderProviderCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<CredentialProviderCapability>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FakeProviderModeState {
    Success,
    Timeout,
    RateLimited,
    Fatal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeProviderCapabilityState {
    pub health: HealthStatus,
    pub cooldown_until: Option<Timestamp>,
    pub last_error: Option<ApiError>,
}

pub fn fake_provider_capability_state(
    mode: FakeProviderModeState,
    provider_id: &str,
    stage: ErrorStage,
    label: &str,
) -> FakeProviderCapabilityState {
    FakeProviderCapabilityState {
        health: fake_provider_mode_health(mode),
        cooldown_until: (mode == FakeProviderModeState::RateLimited)
            .then(|| Timestamp("2026-07-01T00:00:30Z".to_string())),
        last_error: fake_provider_mode_error(mode, provider_id, stage, label),
    }
}

pub fn fake_provider_mode_health(mode: FakeProviderModeState) -> HealthStatus {
    match mode {
        FakeProviderModeState::Success => HealthStatus::Healthy,
        FakeProviderModeState::Timeout => HealthStatus::Degraded,
        FakeProviderModeState::RateLimited => HealthStatus::Cooling,
        FakeProviderModeState::Fatal => HealthStatus::Unavailable,
    }
}

pub fn fake_provider_mode_error(
    mode: FakeProviderModeState,
    provider_id: &str,
    stage: ErrorStage,
    label: &str,
) -> Option<ApiError> {
    let (code, message) = match mode {
        FakeProviderModeState::Success => return None,
        FakeProviderModeState::Timeout => ("provider.timeout", format!("{label} timed out")),
        FakeProviderModeState::RateLimited => {
            ("provider.rate_limited", format!("{label} rate limited"))
        }
        FakeProviderModeState::Fatal => ("provider.fatal", format!("{label} failed fatally")),
    };
    let mut error = ApiError::new(code, stage, message).with_provider_id(provider_id);
    if mode == FakeProviderModeState::Fatal {
        error.retryable = false;
    }
    Some(error)
}

#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(deny_unknown_fields)]
pub struct ProviderLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrency: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_batch_size: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_input_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit_per_minute: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_queue_depth: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interactive_reserved_concurrency: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_max_concurrency: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maintenance_max_concurrency: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReservationPolicy {
    pub supports_reservations: bool,
    pub queue_policy: QueuePolicy,
    pub interactive_reserve: u32,
    pub cooldown_after_failures: u32,
    pub cooldown_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_backoff_ms: Option<u64>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum QueuePolicy {
    Fifo,
    Priority,
    FairByJob,
    DropWhenFull,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReservationStateSnapshot {
    pub queued: u32,
    pub active: u32,
    pub available_units: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oldest_queued_ms: Option<u64>,
    pub priority_breakdown: BTreeMap<String, u32>,
    pub states: Vec<ReservationState>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ReservationState {
    Requested,
    Queued,
    Granted,
    Active,
    Released,
    Expired,
    Canceled,
    Failed,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCostClass {
    Free,
    Low,
    Standard,
    High,
    Premium,
    Internal,
    Unknown,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DegradedMode {
    ReadOnly,
    NoStreaming,
    NoStructuredOutput,
    NoToolUse,
    SnippetOnly,
    HttpOnly,
    NoJavascript,
    NoSparseVectors,
    NoHybridSearch,
    NoDeleteByFilter,
    LowerConcurrency,
    RetryOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingProviderCapability {
    pub model_id: String,
    pub dimensions: u32,
    pub max_input_tokens: u32,
    pub max_batch_tokens: u32,
    pub instruction_support: InstructionSupport,
    pub sparse_output: bool,
    pub batch_limits: BatchLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchLimits {
    pub max_items: u32,
    pub max_tokens: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum InstructionSupport {
    None,
    QueryOnly,
    DocumentOnly,
    QueryAndDocument,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LlmProviderCapability {
    pub model_id: String,
    pub context_window: u32,
    pub streaming: bool,
    pub json_schema: bool,
    pub tool_use: bool,
    pub structured_output: bool,
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VectorStoreCapability {
    pub dense: bool,
    pub sparse: bool,
    pub hybrid: bool,
    pub payload_filters: bool,
    pub payload_indexes: Vec<String>,
    pub delete_by_filter: bool,
    pub collection_aliases: bool,
    pub consistency: VectorConsistency,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum VectorConsistency {
    Strong,
    Eventual,
    Tunable,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FetchProviderCapability {
    pub schemes: Vec<String>,
    pub redirect_policy: RedirectPolicy,
    pub header_policy: HeaderPolicy,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RedirectPolicy {
    None,
    SameOrigin,
    SameSite,
    Any,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum HeaderPolicy {
    None,
    Allowlist,
    Passthrough,
    RedactedPassthrough,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderProviderCapability {
    pub render_modes: Vec<RenderMode>,
    pub browser_pool_limits: BrowserPoolLimits,
    pub script_support: bool,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RenderMode {
    Http,
    Chrome,
    AutoSwitch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserPoolLimits {
    pub max_browsers: u32,
    pub max_pages_per_browser: u32,
    pub max_page_lifetime_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CredentialProviderCapability {
    pub auth_schemes: Vec<String>,
    pub redaction_policy: RedactionPolicy,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RedactionPolicy {
    None,
    DisplaySafe,
    Strict,
    Opaque,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProviderSummary {
    pub provider_id: ProviderId,
    pub provider_kind: ProviderKind,
    pub health: HealthStatus,
    pub active_reservations: u32,
    pub queued_requests: u32,
    pub cooling_until: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CapabilityDocument {
    pub server: ServerInfo,
    pub generated_at: Timestamp,
    pub source_kinds: Vec<SourceKind>,
    pub source_scopes: Vec<SourceScope>,
    pub pipeline_phases: Vec<PipelinePhase>,
    pub adapters: Vec<SourceAdapterCapability>,
    pub providers: Vec<ProviderCapability>,
    pub stores: StoreCapabilities,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
    pub build: Option<String>,
    pub environment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StoreCapabilities {
    pub ledger: Option<LedgerStoreCapability>,
    pub graph: Option<GraphStoreCapability>,
    pub memory: Option<MemoryStoreCapability>,
    pub job: Option<JobStoreCapability>,
    pub watch: Option<WatchStoreCapability>,
    pub artifact: Option<ArtifactStoreCapability>,
    pub config: Option<ConfigStoreCapability>,
    pub document_cache: Option<DocumentCacheCapability>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HealthReport {
    pub status: HealthStatus,
    pub generated_at: Timestamp,
    pub providers: Vec<ProviderSummary>,
    pub warnings: Vec<SourceWarning>,
    pub metadata: MetadataMap,
}
