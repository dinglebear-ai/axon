use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::*;
use super::enums::*;
use super::graph::GraphRef;
use super::ids::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceRequest {
    pub source: String,
    #[serde(default)]
    pub intent: SourceIntent,
    #[serde(default = "default_embed")]
    pub embed: bool,
    #[serde(default)]
    pub refresh: SourceRefreshPolicy,
    #[serde(default)]
    pub watch: SourceWatchPolicy,
    #[serde(default)]
    pub execution: ExecutionPolicy,
    #[serde(default)]
    pub output: OutputPolicy,
    #[serde(default)]
    pub limits: SourceLimits,
    #[serde(default)]
    pub options: AdapterOptions,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<SourceScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority_hint: Option<AuthorityHint>,
    #[serde(default)]
    pub metadata: MetadataMap,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

impl SourceRequest {
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            intent: SourceIntent::Acquire,
            embed: true,
            refresh: SourceRefreshPolicy::IfStale,
            watch: SourceWatchPolicy::Disabled,
            execution: ExecutionPolicy::default(),
            output: OutputPolicy::default(),
            limits: SourceLimits::default(),
            options: AdapterOptions::default(),
            scope: None,
            collection: None,
            adapter: None,
            authority_hint: None,
            metadata: MetadataMap::new(),
            idempotency_key: None,
        }
    }

    pub fn local_path(path: impl Into<String>, is_dir: bool) -> Self {
        let mut request = Self::new(path);
        request.scope = Some(if is_dir {
            SourceScope::Directory
        } else {
            SourceScope::File
        });
        request.adapter = Some("local".to_string());
        request
    }

    pub fn with_watch(mut self, watch: SourceWatchPolicy) -> Self {
        self.watch = watch;
        if watch != SourceWatchPolicy::Disabled {
            self.intent = SourceIntent::Watch;
        }
        self
    }

    pub fn with_refresh(mut self, refresh: SourceRefreshPolicy) -> Self {
        self.refresh = refresh;
        if refresh == SourceRefreshPolicy::Force {
            self.intent = SourceIntent::Refresh;
        }
        self
    }

    pub fn without_embedding(mut self) -> Self {
        self.embed = false;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolvedSource {
    pub source: String,
    pub canonical_uri: String,
    #[serde(skip)]
    pub source_id: SourceId,
    pub source_kind: SourceKind,
    pub adapter: AdapterRef,
    pub default_scope: SourceScope,
    pub available_scopes: Vec<SourceScope>,
    pub authority: AuthorityLevel,
    pub confidence: f32,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graph: Vec<GraphRef>,
    pub warnings: Vec<SourceWarning>,
    #[serde(skip)]
    pub metadata: MetadataMap,
}

impl ResolvedSource {
    #[allow(clippy::too_many_arguments)]
    pub fn resolved(
        source: impl Into<String>,
        canonical_uri: impl Into<String>,
        source_id: SourceId,
        source_kind: SourceKind,
        adapter: AdapterRef,
        scope: SourceScope,
        authority: AuthorityLevel,
        confidence: f32,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            source: source.into(),
            canonical_uri: canonical_uri.into(),
            source_id,
            source_kind,
            adapter,
            default_scope: scope,
            available_scopes: vec![scope],
            authority,
            confidence,
            reason: reason.into(),
            graph: Vec::new(),
            warnings: Vec::new(),
            metadata: MetadataMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RoutePlan {
    pub source: ResolvedSource,
    pub adapter: AdapterRef,
    pub scope: SourceScope,
    pub provider_requirements: Vec<ProviderRequirement>,
    pub credential_requirements: Vec<CredentialRequirement>,
    pub execution_affinity: ExecutionAffinity,
    pub safety_class: SafetyClass,
    pub option_schema_id: String,
    pub validated_options: AdapterOptions,
    pub chunking_hints: Vec<ChunkHint>,
    pub parser_hints: Vec<ParserHint>,
    pub graph_fact_kinds: Vec<String>,
    pub watch_supported: bool,
    pub refresh_supported: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourcePlan {
    pub job_id: JobId,
    pub request: SourceRequest,
    pub route: RoutePlan,
    pub stage_plan: Vec<JobStagePlan>,
    pub limits: EffectiveLimits,
    pub config_snapshot_id: ConfigSnapshotId,
    pub provider_reservations: Vec<ProviderReservationRequest>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EffectiveLimits {
    pub request: SourceLimits,
    pub adapter_defaults: SourceLimits,
    pub config_defaults: SourceLimits,
    pub effective: SourceLimits,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProviderReservationRequest {
    pub provider_kind: ProviderKind,
    pub priority: JobPriority,
    pub units: u32,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceResult {
    pub job_id: JobId,
    pub source_id: SourceId,
    pub canonical_uri: String,
    pub source_kind: SourceKind,
    pub adapter: AdapterRef,
    pub scope: SourceScope,
    pub status: LifecycleStatus,
    pub ledger: LedgerSummary,
    pub graph: GraphWriteSummary,
    pub counts: SourceCounts,
    pub warnings: Vec<SourceWarning>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline: Option<InlineSourceResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<JobDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch: Option<WatchResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<SourceError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct InlineSourceResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<ContentRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JobDescriptor {
    pub kind: JobKind,
    pub id: JobId,
    pub status_url: String,
    pub events_url: String,
    pub stream_url: String,
    pub poll_after_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_url: Option<String>,
    #[serde(skip)]
    pub job_id: JobId,
    #[serde(skip, default = "default_descriptor_status")]
    pub status: LifecycleStatus,
    #[serde(skip)]
    pub poll: Option<PollDescriptor>,
    #[serde(skip)]
    pub created_at: Option<Timestamp>,
    #[serde(skip)]
    pub updated_at: Option<Timestamp>,
}

fn default_descriptor_status() -> LifecycleStatus {
    LifecycleStatus::Queued
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PollDescriptor {
    pub kind: JobKind,
    pub id: JobId,
    pub status_url: String,
    pub events_url: String,
    pub stream_url: String,
    pub poll_after_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchResult {
    pub watch_id: WatchId,
    pub source_id: SourceId,
    pub canonical_uri: String,
    pub adapter: AdapterRef,
    pub scope: SourceScope,
    pub enabled: bool,
    pub schedule: WatchSchedule,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<JobDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_job: Option<JobDescriptor>,
    pub warnings: Vec<SourceWarning>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchSchedule {
    pub every_seconds: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cron: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LedgerSummary {
    pub source_id: SourceId,
    pub generation: SourceGenerationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_generation: Option<SourceGenerationId>,
    pub status: LifecycleStatus,
    pub counts: SourceCounts,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphWriteSummary {
    pub nodes_upserted: u64,
    pub edges_upserted: u64,
    pub evidence_records: u64,
    pub degraded: bool,
}

fn default_embed() -> bool {
    true
}
