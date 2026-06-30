# API Contract
Last Modified: 2026-06-30

## Contract

`axon-api` is the transport-neutral contract crate. It is not only the HTTP API.
It contains the shared DTOs used by CLI, MCP, REST, jobs, and watch config.
`axon-error` owns the error taxonomy and `axon-observe` owns event/span/metric
plumbing; `axon-api` owns their serializable transport-neutral projections and
envelopes.

## Current Implementation Snapshot

Implemented today:

- `axon-api` contains the current shared DTO foothold: MCP schema/request types,
  job DTOs, job status/progress projections, result types,
  diff/explain/ingest/purge DTOs, and service job contracts.
- `ServiceJob`, `JobStatus`, `JobProgress`, MCP request/response structs, and
  several query/retrieve/memory request structs are real implementation types.
- The crate comments still describe DTO migration as in progress.

Partially implemented:

- CLI, MCP, REST, and jobs share some DTOs, but many current routes still return
  route-specific structs or raw typed results.
- Provider capability data exists in places such as server capabilities and LLM
  configuration, but not as one full provider capability contract.

Planned by this contract:

- The DTO registry below is the desired complete catalog. Types such as
  `SourceRequest`, `SourceResult`, `SuccessEnvelope`, `ErrorEnvelope`,
  `CapabilityDocument`, source ledger DTOs, graph DTOs, upload DTOs, provider
  DTOs, and prune-plan DTOs are target contract types unless explicitly called
  out as implemented in code. `ApiError` is sourced from `axon-error`;
  `SourceProgressEvent` is sourced from `axon-observe` and projected here for
  transports.

## Required DTOs

```rust
pub struct SourceRequest;
pub struct SourceResult;
pub struct SourceProgressEvent; // projection of axon-observe event model
pub struct SourceStatus;
pub struct SourceError;
pub struct SourceWarning;
pub struct ServerInfo;
pub struct CapabilityDocument;
pub struct OpenApiDocument;
pub struct SuccessEnvelope<T>;
pub struct ErrorEnvelope;
pub struct ApiError; // projection of axon-error taxonomy
pub struct StreamEvent;
pub struct ProviderCapability;
pub struct ProviderSummary;
pub struct HealthReport;
pub struct StatusReport;
pub struct DoctorReport;
pub struct StatsReport;
pub struct StatsRequest;
pub struct SourceAdapterCapability;
pub struct SourceScopeCapability;
pub struct LlmCompletionRequest;
pub struct LlmCompletionResponse;
pub struct LlmDelta;
pub struct EmbeddingProviderCapability;
pub struct CollectionSpec;
pub struct VectorDeleteSelector;
pub struct VectorSearchRequest;
pub struct VectorSearchResult;
pub struct VectorStoreWriteResult;
pub struct VectorStoreDeleteResult;
pub struct LlmProviderCapability;
pub struct VectorStoreCapability;
pub struct LedgerStoreCapability;
pub struct GraphStoreCapability;
pub struct MemoryStoreCapability;
pub struct ArtifactStoreCapability;
pub struct SearchProviderCapability;
pub struct FetchProviderCapability;
pub struct RenderProviderCapability;
pub struct NetworkCaptureProviderCapability;
pub struct JobStoreCapability;
pub struct WatchStoreCapability;
pub struct MobileSessionStoreCapability;
pub struct ConfigStoreCapability;
pub struct RateLimiterCapability;
pub struct SecurityPolicyCapability;
pub struct CredentialProviderCapability;
pub struct DocumentCacheCapability;
pub struct HealthProbeCapability;
pub struct ResolvedSource;
pub struct PollDescriptor;
pub struct Page<T>;
pub struct JobDescriptor;
pub struct JobSummary;
pub struct JobEventPage;
pub struct JobCleanupRequest;
pub struct JobCleanupResult;
pub struct JobRecoverRequest;
pub struct JobRecoverResult;
pub struct JobClearRequest;
pub struct JobClearResult;
pub struct WatchRequest;
pub struct WatchResult;
pub struct WatchSummary;
pub struct WatchUpdateRequest;
pub struct WatchListRequest;
pub struct WatchExecRequest;
pub struct WatchHistoryResult;
pub struct DeleteResult;
pub struct DomainSummary;
pub struct SourceSummary;
pub struct SourceListRequest;
pub struct SourceItem;
pub struct SourceItemListRequest;
pub struct SourceItemDetail;
pub struct SourceGenerationListRequest;
pub struct SourceGenerationSummary;
pub struct SourceGenerationDetail;
pub struct DomainListRequest;
pub struct DocumentListRequest;
pub struct ChunkListRequest;
pub struct ChunkGetRequest;
pub struct SearchRequest;
pub struct SearchResult;
pub struct ResearchRequest;
pub struct ResearchResult;
pub struct SummarizeRequest;
pub struct SummarizeResult;
pub struct ChatRequest;
pub struct ChatResult;
pub struct EvaluationRequest;
pub struct EvaluationResult;
pub struct SuggestRequest;
pub struct SuggestResult;
pub struct EndpointDiscoveryRequest;
pub struct EndpointDiscoveryResult;
pub struct BrandRequest;
pub struct BrandResult;
pub struct DiffRequest;
pub struct DiffResult;
pub struct ScreenshotRequest;
pub struct ScreenshotResult;
pub struct PruneRequest;
pub struct PrunePlan;
pub struct PruneExecuteRequest;
pub struct PruneResult;
pub struct CleanupDebtRequest;
pub struct CleanupDebtResult;
pub struct ResetRequest;
pub struct ResetPlan;
pub struct ResetExecuteRequest;
pub struct ResetResult;
pub struct DedupeRequest;
pub struct DedupeResult;
pub struct JobListRequest;
pub struct JobEventListRequest;
pub struct WatchListRequest;
pub struct WatchExecRequest;
pub struct WatchHistoryRequest;
pub struct SourceEnrichment;
pub struct SourceDocument;
pub struct SourceParseFacts;
pub struct GraphCandidate;
pub struct PreparedDocument;
pub struct DocumentSummary;
pub struct DocumentDetail;
pub struct ChunkSummary;
pub struct ChunkDetail;
pub struct EmbeddingBatch;
pub struct VectorPointBatch;
pub struct DocumentStatus;
pub struct SourceGeneration;
pub struct CleanupDebt;
pub struct GraphNode;
pub struct GraphEdge;
pub struct GraphEvidence;
pub struct GraphKindDocument;
pub struct GraphResolveRequest;
pub struct GraphResolveResult;
pub struct GraphQueryRequest;
pub struct GraphQueryResult;
pub struct RetrievalRequest;
pub struct RetrievalResult;
pub struct QueryRequest;
pub struct QueryResult;
pub struct AskRequest;
pub struct AskResult;
pub struct ChatRequest;
pub struct ChatResult;
pub struct EvaluationRequest;
pub struct EvaluationResult;
pub struct SuggestRequest;
pub struct SuggestResult;
pub struct SearchRequest;
pub struct SearchResult;
pub struct ResearchRequest;
pub struct ResearchResult;
pub struct SummarizeRequest;
pub struct SummarizeResult;
pub struct EndpointDiscoveryRequest;
pub struct EndpointDiscoveryResult;
pub struct BrandRequest;
pub struct BrandResult;
pub struct DiffRequest;
pub struct DiffResult;
pub struct ScreenshotRequest;
pub struct ScreenshotResult;
pub struct ExtractRequest;
pub struct ExtractResult;
pub struct MemoryRequest;
pub struct MemoryResult;
pub struct MemoryRecord;
pub struct MemorySearchRequest;
pub struct MemorySearchResult;
pub struct MemoryContextRequest;
pub struct MemoryContextResult;
pub struct MemoryLinkRequest;
pub struct MemoryImportRequest;
pub struct MemoryImportResult;
pub struct MemoryExportRequest;
pub struct MemoryExportResult;
pub struct ArtifactSummary;
pub struct ArtifactDetail;
pub struct ArtifactContentDescriptor;
pub struct ArtifactListRequest;
pub struct ArtifactWriteRequest;
pub struct ArtifactHandle;
pub struct ArtifactReadResult;
pub struct FetchRequest;
pub struct FetchedResource;
pub struct RenderRequest;
pub struct RenderedResource;
pub struct NetworkCaptureRequest;
pub struct NetworkCaptureResult;
pub struct UploadCreateRequest;
pub struct UploadCreateResult;
pub struct UploadStatus;
pub struct UploadCompleteRequest;
pub struct UploadCompleteResult;
pub struct UploadAbortRequest;
pub struct UploadAbortResult;
pub struct PrunePlanRequest;
pub struct PrunePlan;
pub struct PruneExecRequest;
pub struct PruneJobStatus;
pub struct DedupeRequest;
pub struct DedupeResult;
pub struct PurgeRequest;
pub struct PurgeResult;
pub struct CollectionSummary;
pub struct CollectionDetail;
pub struct CollectionListRequest;
pub struct MobileSessionSummary;
pub struct MobileSessionDetail;
pub struct MobileSessionListRequest;
pub struct MobileSessionUpsertRequest;
pub struct MobileSessionUpsertResult;
pub struct MobileSessionDeleteResult;
pub struct PanelState;
pub struct PanelLoginRequest;
pub struct PanelLoginResult;
pub struct PanelConfigDocument;
pub struct PanelConfigSaveRequest;
pub struct PanelConfigSaveResult;
pub struct PanelCommandRequest;
pub struct PanelCommandResult;
pub struct PanelOpsReport;
pub struct PanelStackReport;
pub struct PanelSetupTargets;
pub struct CredentialRequest;
pub struct CredentialMaterial;
pub struct DocumentCacheKey;
pub struct CachedDocument;
pub struct DocumentCacheInvalidation;
pub struct HealthProbeResult;
pub struct RateLimitRequest;
pub struct RateLimitPermit;
pub struct RouteAuthRequest;
pub struct SecurityDecision;
```

## DTO Field Catalog

This is the field-level contract for `axon-api`. Field names are snake_case in
Rust/JSON unless a transport explicitly maps them. Optional fields are nullable
or absent in JSON; required fields must be present. Request DTOs use
`serde(deny_unknown_fields)` unless a field is explicitly `options` or
`metadata`.

### Envelope and Common DTOs

| DTO | Required fields | Optional fields |
|---|---|---|
| `SuccessEnvelope<T>` | `ok=true`, `data`, `warnings`, `request_id`, `job` | none |
| `ErrorEnvelope` | `ok=false`, `error`, `request_id` | none |
| `ApiError` | `code`, `message`, `stage`, `retryable`, `severity`, `details` | `job_id`, `source_id`, `provider_id`, `retry_after_ms`, `cooldown_until` |
| `Page<T>` | `items`, `next_cursor`, `limit` | none |
| `PollDescriptor` | `kind`, `id`, `status_url`, `events_url`, `stream_url`, `poll_after_ms` | `cancel_url`, `retry_url` |
| `JobDescriptor` | `kind`, `id`, `status_url`, `events_url`, `stream_url`, `poll_after_ms` | `cancel_url`, `retry_url` |
| `StreamEvent` | `event_id`, `kind`, `sequence`, `timestamp`, `data` | `job_id`, `request_id`, `warning`, `error` |

### Capability and Health DTOs

| DTO | Required fields | Optional fields |
|---|---|---|
| `ServerInfo` | `name`, `version`, `contract_version`, `build`, `auth_mode`, `features` | `data_dir`, `public_url` |
| `CapabilityDocument` | `server`, `adapters`, `providers`, `limits`, `graph`, `memory`, `uploads`, `auth` | `schemas`, `warnings` |
| `OpenApiDocument` | OpenAPI `openapi`, `info`, `paths`, `components` | `security`, `tags`, `servers` |
| `ProviderCapability` | `provider_id`, `kind`, `status`, `capabilities`, `health` | `limits`, `cooling`, `message` |
| `ProviderSummary` | `total`, `ready`, `degraded`, `unavailable`, `disabled` | `cooling_down` |
| `HealthReport` | `status`, `checks`, `generated_at` | `warnings`, `remediation` |
| `StatusReport` | `status`, `jobs`, `watches`, `providers`, `cleanup`, `degraded`, `warnings` | `current_activity` |
| `DoctorReport` | `status`, `checks`, `remediation`, `warnings` | `deep_checks` |
| `StatsRequest` | none | `collection`, `source_id`, `include_graph`, `include_memory` |
| `StatsReport` | `sources`, `documents`, `chunks`, `vectors`, `graph`, `memory`, `jobs`, `storage` | `collections` |
| `SourceAdapterCapability` | `name`, `version`, `source_kinds`, `default_scope`, `scopes` | `option_schema`, `credential_requirements`, `limits`, `watch_supported` |
| `SourceScopeCapability` | `name`, `embeds_by_default`, `watch_supported`, `requires_credentials` | `description`, `options_schema`, `limits` |
| `LlmProviderCapability` | `model_id`, `context_window`, `streaming`, `json_schema`, `health` | `tools`, `reasoning`, `rate_limits` |
| `EmbeddingProviderCapability` | `model_id`, `dimensions`, `batch_limits`, `max_input_tokens`, `health` | `sparse_output`, `instruction_support` |
| `VectorStoreCapability` | `dense`, `sparse`, `hybrid`, `payload_filters`, `delete_by_filter`, `health` | `collection_aliases`, `transactional_semantics` |
| `LedgerStoreCapability` | `leases`, `heartbeats`, `transactions`, `cleanup_debt`, `health` | `advisory_locks`, `event_replay` |
| `GraphStoreCapability` | `node_kinds`, `edge_kinds`, `evidence_kinds`, `query_depth_limit`, `health` | `conflict_handling`, `merge_semantics` |
| `MemoryStoreCapability` | `memory_types`, `statuses`, `link_types`, `decay_modes`, `health` | `context_budget_limits`, `forget_semantics` |
| `ArtifactStoreCapability` | `max_object_size`, `content_addressing`, `retention`, `delete_semantics`, `health` | `signed_urls`, `warc_support` |
| `SearchProviderCapability` | `backend_id`, `result_limits`, `time_ranges`, `rate_limits`, `health` | `safe_search`, `languages` |
| `FetchProviderCapability` | `schemes`, `byte_limits`, `redirect_policy`, `header_policy`, `health` | `robots_policy`, `retry_policy` |
| `RenderProviderCapability` | `render_modes`, `browser_pool_limits`, `timeouts`, `health` | `script_support`, `screenshot_support` |
| `NetworkCaptureProviderCapability` | `capture_types`, `endpoint_extraction`, `redaction`, `health` | `browser_affinity` |
| `JobStoreCapability` | `leases`, `event_replay`, `cleanup`, `recover`, `retention`, `health` | `schema_version` |
| `WatchStoreCapability` | `schedule_kinds`, `lease_semantics`, `history_retention`, `health` | `filesystem_support`, `debounce_limits` |
| `MobileSessionStoreCapability` | `owner_scoping`, `conflict_detection`, `max_payload_size`, `health` | `retention` |
| `ConfigStoreCapability` | `readable_domains`, `writable_domains`, `secret_redaction`, `health` | `validation_support`, `restart_required_signaling` |
| `CredentialProviderCapability` | `auth_schemes`, `redaction_policy`, `health` | `refresh_support`, `scope_requirements` |
| `DocumentCacheCapability` | `max_entry_size`, `ttl`, `invalidation`, `health` | `generation_awareness` |
| `HealthProbeCapability` | `probe_cost`, `timeout`, `dependency_coverage`, `health` | `degraded_mapping` |
| `RateLimiterCapability` | `limit_keys`, `burst_limits`, `concurrency_limits`, `cooling_semantics`, `health` | `queue_behavior` |
| `SecurityPolicyCapability` | `auth_scopes`, `local_path_roots`, `ssrf_policy`, `header_forwarding_policy`, `health` | `tenant_policy` |

### Source, Ledger, and Document DTOs

| DTO | Required fields | Optional fields |
|---|---|---|
| `SourceRequest` | `source`, `intent`, `embed`, `refresh`, `watch`, `execution`, `output`, `limits`, `options` | `scope`, `collection`, `adapter`, `authority_hint`, `metadata` |
| `ResolvedSource` | `source`, `canonical_uri`, `source_kind`, `adapter`, `default_scope`, `available_scopes`, `authority`, `confidence`, `reason` | `graph`, `warnings` |
| `SourceResult` | `job_id`, `source_id`, `canonical_uri`, `source_kind`, `adapter`, `scope`, `status`, `ledger`, `graph`, `counts`, `warnings` | `inline`, `job`, `watch` |
| `SourceSummary` | `source_id`, `canonical_uri`, `display_name`, `source_kind`, `adapter`, `authority`, `status`, `counts`, `created_at`, `updated_at` | `watch_id`, `graph_node_ids`, `last_job_id`, `last_refreshed_at`, `tags`, `user_label` |
| `SourceItem` | `source_id`, `source_item_key`, `status`, `content_hash`, `generation` | `path`, `url`, `size_bytes`, `document_ids`, `graph_refs`, `last_error` |
| `SourceItemDetail` | all `SourceItem` fields, `manifest`, `statuses`, `errors` | `content_preview`, `metadata` |
| `SourceGeneration` | `source_id`, `generation`, `status`, `created_at` | `published_at`, `counts`, `cleanup_debt` |
| `CleanupDebt` | `debt_id`, `source_id`, `kind`, `target`, `status`, `created_at` | `job_id`, `generation`, `verified_at`, `warnings` |
| `SourceGenerationSummary` | `source_id`, `generation`, `status`, `counts` | `published_at`, `cleanup_status` |
| `SourceGenerationDetail` | all `SourceGenerationSummary` fields, `items`, `documents`, `chunks`, `cleanup_debt` | `errors`, `artifacts` |
| `SourceStatus` | `job_id`, `source_id`, `status`, `phase`, `heartbeat_at`, `counts` | `current`, `last_error`, `warnings`, `poll_after_ms` |
| `SourceProgressEvent` | `event_id`, `sequence`, `job_id`, `phase`, `status`, `severity`, `visibility`, `message`, `timestamp` | `source_id`, `canonical_uri`, `adapter`, `scope`, `generation`, `counts`, `timing`, `current`, `throughput`, `retry`, `warning`, `error` |
| `SourceError` | `code`, `message`, `stage`, `retryable`, `severity`, `details` | `source_item_key` |
| `SourceWarning` | `code`, `message`, `stage`, `visibility` | `details`, `source_item_key` |
| `DomainListRequest` | none | `domain`, `source_kind`, `limit`, `cursor` |
| `DomainSummary` | `domain`, `source_count`, `document_count`, `chunk_count` | `latest_refresh_at`, `top_sources` |
| `SourceEnrichment` | `job_id`, `source_id`, `source_item_key`, `enrichment_kind`, `status`, `metadata` | `parse_hints`, `chunk_hints`, `graph_candidates`, `artifacts`, `warnings` |
| `SourceDocument` | `document_id`, `source_id`, `source_item_key`, `canonical_uri`, `content_kind`, `content`, `metadata` | `title`, `language`, `path`, `structured_payload`, `chunk_hint` |
| `SourceParseFacts` | `document_id`, `parser`, `parser_version`, `facts` | `warnings`, `errors` |
| `GraphCandidate` | `kind`, `candidate_id`, `evidence`, `confidence` | `node`, `edge`, `merge_key`, `metadata` |
| `PreparedDocument` | `document_id`, `source_id`, `source_item_key`, `chunks`, `metadata` | `cleanup_keys`, `graph_refs` |
| `DocumentSummary` | `document_id`, `source_id`, `source_item_key`, `status`, `chunk_count`, `vector_point_count` | `content_kind`, `title`, `path`, `graph_refs` |
| `DocumentDetail` | all `DocumentSummary` fields, `generation`, `metadata`, `chunk_summary`, `vector_keys` | `chunks`, `source`, `graph` |
| `DocumentListRequest` | none | `source_id`, `status`, `generation`, `content_kind`, `limit`, `cursor` |
| `DocumentStatus` | `document_id`, `source_id`, `source_item_key`, `status`, `updated_at` | `generation`, `error`, `cleanup_status` |
| `ChunkSummary` | `chunk_id`, `document_id`, `chunk_index`, `chunk_locator`, `source_range`, `metadata` | `score`, `graph_refs`, `vector_refs` |
| `ChunkDetail` | all `ChunkSummary` fields, `content_hash` | `content`, `payload`, `embedding_metadata` |
| `ChunkListRequest` | `document_id` | `include_content`, `limit`, `cursor` |
| `ChunkGetRequest` | `document_id`, `chunk_id` | `include_content` |

### Job, Watch, Vector, and Provider Operation DTOs

| DTO | Required fields | Optional fields |
|---|---|---|
| `JobSummary` | `job_id`, `kind`, `status`, `phase`, `created_at`, `updated_at` | `source_id`, `watch_id`, `counts`, `last_error` |
| `JobListRequest` | none | `status`, `kind`, `source_id`, `watch_id`, `limit`, `cursor` |
| `JobEventListRequest` | `job_id` | `after_sequence`, `limit`, `severity`, `visibility` |
| `JobEventPage` | `events`, `next_cursor`, `last_sequence` | none |
| `JobCleanupRequest` | `dry_run` | `kind`, `older_than`, `status`, `limit` |
| `JobCleanupResult` | `matched`, `deleted`, `dry_run` | `warnings` |
| `JobRecoverRequest` | none | `kind`, `stale_before`, `limit` |
| `JobRecoverResult` | `recovered`, `job_ids` | `warnings` |
| `JobClearRequest` | `status`, `confirm` | `kind`, `older_than` |
| `JobClearResult` | `deleted`, `status` | `warnings` |
| `WatchRequest` | `source`, `schedule`, `embed`, `options` | `scope`, `collection`, `enabled` |
| `WatchResult` | `watch_id`, `source_id`, `canonical_uri`, `adapter`, `scope`, `enabled`, `schedule` | `job`, `latest_job`, `warnings` |
| `WatchSummary` | `watch_id`, `source_id`, `enabled`, `schedule`, `next_run_at` | `last_job_id`, `last_status` |
| `WatchUpdateRequest` | none | `enabled`, `schedule`, `options`, `embed`, `collection` |
| `WatchListRequest` | none | `enabled`, `source_id`, `adapter`, `limit`, `cursor` |
| `WatchExecRequest` | none | `reason`, `refresh`, `wait` |
| `WatchHistoryRequest` | `watch_id` | `limit`, `cursor`, `status` |
| `WatchHistoryResult` | `watch_id`, `jobs`, `next_cursor` | none |
| `EmbeddingBatch` | `batch_id`, `model`, `items` | `instruction`, `metadata` |
| `VectorPointBatch` | `collection`, `points`, `model`, `dimensions` | `sparse_vectors`, `payload_indexes` |
| `CollectionSpec` | `collection`, `dense`, `payload_indexes` | `sparse`, `aliases`, `distance` |
| `VectorDeleteSelector` | `collection`, `kind` | `source_id`, `generation`, `document_id`, `chunk_ids`, `filter` |
| `VectorSearchRequest` | `collection`, `query`, `limit` | `filters`, `hybrid`, `generation`, `graph_refs` |
| `VectorSearchResult` | `results`, `limit` | `next_cursor`, `warnings` |
| `VectorStoreWriteResult` | `collection`, `points_written` | `warnings` |
| `VectorStoreDeleteResult` | `collection`, `points_deleted` | `dry_run`, `warnings` |
| `LlmCompletionRequest` | `messages`, `model` | `system`, `temperature`, `response_schema`, `stream`, `tools`, `metadata` |
| `LlmCompletionResponse` | `message`, `model` | `usage`, `finish_reason`, `tool_calls`, `metadata` |
| `LlmDelta` | `sequence`, `text` | `tool_call_delta`, `finish_reason` |
| `FetchRequest` | `uri`, `method`, `headers` | `body`, `timeout_ms`, `byte_limit`, `credentials_ref` |
| `FetchedResource` | `uri`, `status`, `headers`, `body`, `fetched_at` | `etag`, `last_modified`, `redirect_chain` |
| `RenderRequest` | `uri`, `render_mode` | `viewport`, `automation`, `timeout_ms`, `headers` |
| `RenderedResource` | `uri`, `html`, `text`, `render_mode`, `captured_at` | `screenshot_artifact_id`, `console`, `network` |
| `NetworkCaptureRequest` | `uri`, `capture_types` | `render_mode`, `timeout_ms`, `headers` |
| `NetworkCaptureResult` | `uri`, `requests`, `responses`, `captured_at` | `endpoint_candidates`, `redactions` |
| `CredentialRequest` | `provider`, `scope` | `source_id`, `adapter`, `auth_scheme` |
| `CredentialMaterial` | `kind`, `redacted_label` | `secret_ref`, `expires_at`, `headers` |
| `DocumentCacheKey` | `kind`, `id` | `generation`, `variant` |
| `CachedDocument` | `key`, `content`, `created_at` | `expires_at`, `metadata` |
| `DocumentCacheInvalidation` | `kind` | `source_id`, `generation`, `document_id`, `prefix` |
| `HealthProbeResult` | `status`, `checked_at`, `latency_ms` | `message`, `details` |
| `RateLimitRequest` | `key`, `cost` | `timeout_ms`, `priority` |
| `RateLimitPermit` | `key`, `granted`, `expires_at` | `retry_after_ms` |
| `RouteAuthRequest` | `route`, `method`, `scope` | `actor`, `source_request` |
| `SecurityDecision` | `allowed`, `scope`, `reason` | `redactions`, `warnings` |

### Graph, Retrieval, Exploration, and Extraction DTOs

| DTO | Required fields | Optional fields |
|---|---|---|
| `GraphNode` | `node_id`, `kind`, `canonical_uri`, `display_name`, `authority`, `confidence`, `metadata` | `source_ids`, `created_at`, `updated_at` |
| `GraphEdge` | `edge_id`, `kind`, `from_node_id`, `to_node_id`, `authority`, `confidence`, `evidence` | `metadata` |
| `GraphEvidence` | `kind`, `source_id`, `locator`, `confidence` | `job_id`, `document_id`, `chunk_id`, `excerpt` |
| `GraphKindDocument` | `node_kinds`, `edge_kinds`, `evidence_kinds`, `authority_levels` | none |
| `GraphResolveRequest` | `identifiers` | `include_edges` |
| `GraphResolveResult` | `resolved`, `misses` | `warnings` |
| `GraphQueryRequest` | `start`, `direction`, `depth`, `limit` | `edges`, `filters`, `cursor` |
| `GraphQueryResult` | `nodes`, `edges`, `evidence`, `next_cursor` | `warnings` |
| `QueryRequest` | `query`, `limit` | `source_id`, `graph_node_id`, `filters`, `generation`, `include_graph` |
| `QueryResult` | `results` | `graph`, `warnings`, `next_cursor` |
| `RetrievalRequest` | one of `source`, `source_id`, `document_id`, `url`, `chunk_id` | `include_content`, `limit`, `token_budget` |
| `RetrievalResult` | `documents`, `chunks` | `next_cursor`, `warnings` |
| `AskRequest` | `question` | `filters`, `retrieval`, `synthesis`, `include_trace` |
| `AskResult` | `answer`, `citations`, `retrieval`, `model` | `graph`, `warnings`, `trace` |
| `ChatRequest` | `message` | `system`, `model`, `temperature`, `history` |
| `ChatResult` | `message`, `model` | `usage`, `warnings` |
| `EvaluationRequest` | `question` | `expected`, `filters`, `judge`, `limit` |
| `EvaluationResult` | `answer`, `scores`, `verdict` | `baseline`, `citations`, `warnings` |
| `SuggestRequest` | none | `focus`, `source_id`, `limit`, `constraints` |
| `SuggestResult` | `suggestions` | `warnings` |
| `SearchRequest` | `query` | `limit`, `offset`, `time_range`, `auto_source` |
| `SearchResult` | `results` | `jobs`, `warnings` |
| `ResearchRequest` | `query` | `limit`, `depth`, `full_content`, `auto_source` |
| `ResearchResult` | `answer`, `sources`, `citations` | `jobs`, `warnings` |
| `SummarizeRequest` | one of `source`, `url`, `urls` | `instructions`, `format`, `headers` |
| `SummarizeResult` | `summary`, `sources` | `citations`, `artifact_refs`, `warnings` |
| `EndpointDiscoveryRequest` | `source` or `url` | `render_mode`, `capture`, `limit` |
| `EndpointDiscoveryResult` | `endpoints`, `report` | `artifact_id`, `warnings` |
| `BrandRequest` | `source` or `url` | `render_mode`, `include_screenshot` |
| `BrandResult` | `colors`, `fonts`, `assets` | `screenshot_artifact_id`, `warnings` |
| `DiffRequest` | `source_a`, `source_b` | `mode`, `headers` |
| `DiffResult` | `changed`, `summary`, `diffs` | `artifacts`, `warnings` |
| `ScreenshotRequest` | `source` or `url` | `viewport`, `full_page`, `render_mode`, `wait_for` |
| `ScreenshotResult` | `artifact_id`, `width`, `height`, `captured_at` | `warnings` |
| `ExtractRequest` | `source`, `schema` | `instructions`, `persist_artifact`, `trusted_graph_write` |
| `ExtractResult` | `result`, `schema` | `artifact_id`, `graph_candidates`, `warnings` |

### Memory, Artifact, Upload, Prune, Collection, Mobile, and Panel DTOs

| DTO | Required fields | Optional fields |
|---|---|---|
| `MemoryRequest` | `type`, `body`, `confidence`, `salience`, `scope` | `title`, `project`, `repo`, `file`, `decay`, `tags`, `links`, `embed` |
| `MemoryResult` | `memory_id`, `status`, `memory_score`, `confidence`, `salience`, `created_at` | `graph_node_id`, `document_id`, `vector_point_ids`, `decay`, `warnings` |
| `MemoryRecord` | `memory_id`, `type`, `status`, `body`, `confidence`, `salience`, `scope` | `title`, `links`, `history`, `decay` |
| `MemorySearchRequest` | `query`, `limit` | `filters`, `include_graph`, `include_archived` |
| `MemorySearchResult` | `results` | `graph`, `warnings` |
| `MemoryContextRequest` | `token_budget` | `query`, `source_id`, `graph_node_id`, `filters`, `depth` |
| `MemoryContextResult` | `context`, `memories`, `exclusions` | `warnings` |
| `MemoryLinkRequest` | `memory_id`, `type`, `target`, `confidence` | `evidence` |
| `MemoryImportRequest` | `format`, `mode`, `dry_run` | `bundle`, `artifact_id`, `upload_id` |
| `MemoryImportResult` | `created`, `updated`, `skipped`, `dry_run` | `warnings` |
| `MemoryExportRequest` | `format` | `project`, `repo`, `include_archived` |
| `MemoryExportResult` | `artifact_id` | `download_url`, `count` |
| `ArtifactSummary` | `artifact_id`, `kind`, `created_at`, `size_bytes` | `source_id`, `job_id`, `content_type`, `label` |
| `ArtifactDetail` | all `ArtifactSummary` fields, `retention`, `producer_refs` | `content_url`, `metadata` |
| `ArtifactContentDescriptor` | `artifact_id`, `content_type`, `disposition` | `bytes`, `stream`, `path` |
| `ArtifactListRequest` | none | `kind`, `source_id`, `job_id`, `limit`, `cursor` |
| `ArtifactWriteRequest` | `kind`, `content_type`, `content` | `source_id`, `job_id`, `metadata`, `retention` |
| `ArtifactHandle` | `artifact_id`, `kind` | `path`, `content_url` |
| `ArtifactReadResult` | `handle`, `content_type` | `bytes`, `stream`, `metadata` |
| `UploadCreateRequest` | `filename`, `content_type`, `size_bytes`, `purpose` | `sha256`, `source_hint` |
| `UploadCreateResult` | `upload_id`, `put_url`, `expires_at` | none |
| `UploadStatus` | `upload_id`, `status`, `bytes_received`, `expires_at` | `artifact_id`, `source_ref`, `sha256` |
| `UploadCompleteRequest` | none | `sha256`, `source_options` |
| `UploadCompleteResult` | `upload_id`, `artifact_id`, `source_ref` | `warnings` |
| `UploadAbortRequest` | none | `reason` |
| `UploadAbortResult` | `upload_id`, `deleted` | none |
| `PrunePlanRequest` | `targets`, `include`, `mode` | `retention`, `filters` |
| `PrunePlan` | `prune_plan_id`, `summary`, `requires_confirmation`, `expires_at` | `risk_flags`, `warnings` |
| `PruneExecRequest` | `confirm` | `prune_plan_id`, `inline_plan` |
| `PruneJobStatus` | `job_id`, `status`, `delete_counts`, `verification_state` | `warnings` |
| `DedupeRequest` | `dry_run` | `collection`, `threshold`, `source_id` |
| `DedupeResult` | `matched`, `deduped`, `dry_run` | `job`, `warnings` |
| `PurgeRequest` | `dry_run` | `source_id`, `url`, `prefix`, `filters`, `confirm` |
| `PurgeResult` | `matched`, `purged`, `dry_run` | `prune_plan`, `job`, `warnings` |
| `CollectionListRequest` | none | `include_stats`, `limit`, `cursor` |
| `CollectionSummary` | `collection`, `vectors`, `points` | `payload_indexes`, `status` |
| `CollectionDetail` | all `CollectionSummary` fields, `schema`, `health` | `segments`, `source_counts` |
| `MobileSessionListRequest` | none | `limit`, `cursor` |
| `MobileSessionSummary` | `session_id`, `owner`, `updated_at` | `title`, `message_count`, `revision` |
| `MobileSessionDetail` | all `MobileSessionSummary` fields, `messages` | `metadata` |
| `MobileSessionUpsertRequest` | `session_id`, `messages`, `revision` | `title`, `metadata` |
| `MobileSessionUpsertResult` | `session_id`, `revision`, `updated_at` | none |
| `MobileSessionDeleteResult` | `session_id`, `deleted` | none |
| `PanelState` | `authenticated`, `setup_required`, `config_path` | `user`, `features` |
| `PanelLoginRequest` | `password` | none |
| `PanelLoginResult` | `ok`, `authenticated` | `message` |
| `PanelConfigDocument` | `path`, `raw_text`, `restart_required` | `redactions` |
| `PanelConfigSaveRequest` | `raw_text` | none |
| `PanelConfigSaveResult` | `ok`, `restart_required`, `message` | none |
| `PanelCommandRequest` | `command` | `args`, `timeout_ms` |
| `PanelCommandResult` | `exit_code`, `stdout`, `stderr` | `duration_ms` |
| `PanelOpsReport` | `qdrant_url`, `tei_url`, `collection`, `mcp_http_url` | `warnings` |
| `PanelStackReport` | `services`, `status` | `warnings` |
| `PanelSetupTargets` | `targets` | `warnings` |

## Required Enums

The authoritative enum registry lives in
[types/enum-contract.md](types/enum-contract.md). `axon-api` exports those exact
Rust enums and JSON names; this file does not maintain a second enum list.

Required enum families:

- source intent/refresh/watch/execution/response policies
- source kind/scope/item/content kind
- pipeline phase
- job kind and lifecycle status
- document lifecycle status
- enrichment/diff/cleanup/provider kinds
- health/visibility/severity/priority
- authority/execution-affinity/safety/credential/artifact/cache/chunk profile

Schema generation fails when an enum is exposed by `axon-api` but missing from
`types/enum-contract.md`, or when an enum in that contract is not exported by
`axon-api`.

## Provider Boundaries

The source pipeline has primary data/AI provider boundaries plus supporting
provider boundaries for artifacts, credentials/secrets, cache, and health.

```rust
pub trait LlmProvider {
    async fn complete(&self, request: LlmCompletionRequest) -> Result<LlmCompletionResponse>;
    async fn complete_streaming(
        &self,
        request: LlmCompletionRequest,
        on_delta: LlmDeltaSink,
    ) -> Result<LlmCompletionResponse>;
    async fn capabilities(&self) -> Result<LlmProviderCapability>;
}

pub trait EmbeddingProvider {
    async fn embed(&self, batch: EmbeddingBatch) -> Result<EmbeddingResult>;
    async fn capabilities(&self) -> Result<EmbeddingProviderCapability>;
}

pub trait VectorStore {
    async fn ensure_collection(&self, spec: CollectionSpec) -> Result<()>;
    async fn upsert(&self, batch: VectorPointBatch) -> Result<VectorStoreWriteResult>;
    async fn delete(&self, selector: VectorDeleteSelector) -> Result<VectorStoreDeleteResult>;
    async fn search(&self, request: VectorSearchRequest) -> Result<VectorSearchResult>;
    async fn capabilities(&self) -> Result<VectorStoreCapability>;
}

pub trait LedgerStore {
    async fn create_job(&self, request: SourceRequest) -> Result<SourceJob>;
    async fn update_job_status(&self, status: SourceStatus) -> Result<()>;
    async fn update_document_status(&self, status: DocumentStatus) -> Result<()>;
    async fn publish_generation(&self, generation: SourceGeneration) -> Result<()>;
    async fn record_cleanup_debt(&self, debt: CleanupDebt) -> Result<()>;
    async fn capabilities(&self) -> Result<LedgerStoreCapability>;
}

pub trait GraphStore {
    async fn upsert_candidates(&self, candidates: Vec<GraphCandidate>) -> Result<GraphWriteResult>;
    async fn get_node(&self, node_id: GraphNodeId) -> Result<Option<GraphNode>>;
    async fn get_edge(&self, edge_id: GraphEdgeId) -> Result<Option<GraphEdge>>;
    async fn query(&self, request: GraphQueryRequest) -> Result<GraphQueryResult>;
    async fn resolve(&self, request: GraphResolveRequest) -> Result<GraphResolveResult>;
    async fn capabilities(&self) -> Result<GraphStoreCapability>;
}

pub trait MemoryStore {
    async fn remember(&self, request: MemoryRequest) -> Result<MemoryResult>;
    async fn get(&self, memory_id: MemoryId) -> Result<Option<MemoryRecord>>;
    async fn search(&self, request: MemorySearchRequest) -> Result<MemorySearchResult>;
    async fn context(&self, request: MemoryContextRequest) -> Result<MemoryContextResult>;
    async fn link(&self, request: MemoryLinkRequest) -> Result<MemoryResult>;
    async fn update_status(&self, memory_id: MemoryId, status: MemoryStatus) -> Result<MemoryResult>;
    async fn reinforce(&self, memory_id: MemoryId, signal: MemoryReinforcement) -> Result<MemoryResult>;
    async fn capabilities(&self) -> Result<MemoryStoreCapability>;
}

pub trait ArtifactStore {
    async fn put(&self, artifact: ArtifactWriteRequest) -> Result<ArtifactHandle>;
    async fn get(&self, handle: ArtifactHandle) -> Result<ArtifactReadResult>;
    async fn delete(&self, handle: ArtifactHandle) -> Result<()>;
    async fn capabilities(&self) -> Result<ArtifactStoreCapability>;
}

pub trait SearchProvider {
    async fn search(&self, request: SearchRequest) -> Result<SearchResult>;
    async fn capabilities(&self) -> Result<SearchProviderCapability>;
}

pub trait FetchProvider {
    async fn fetch(&self, request: FetchRequest) -> Result<FetchedResource>;
    async fn capabilities(&self) -> Result<FetchProviderCapability>;
}

pub trait RenderProvider {
    async fn render(&self, request: RenderRequest) -> Result<RenderedResource>;
    async fn capabilities(&self) -> Result<RenderProviderCapability>;
}

pub trait NetworkCaptureProvider {
    async fn capture(&self, request: NetworkCaptureRequest) -> Result<NetworkCaptureResult>;
    async fn capabilities(&self) -> Result<NetworkCaptureProviderCapability>;
}

pub trait JobStore {
    async fn list(&self, request: JobListRequest) -> Result<Page<JobSummary>>;
    async fn events(&self, request: JobEventListRequest) -> Result<JobEventPage>;
    async fn cleanup(&self, request: JobCleanupRequest) -> Result<JobCleanupResult>;
    async fn recover(&self, request: JobRecoverRequest) -> Result<JobRecoverResult>;
    async fn capabilities(&self) -> Result<JobStoreCapability>;
}

pub trait WatchStore {
    async fn create(&self, request: WatchRequest) -> Result<WatchResult>;
    async fn update(&self, watch_id: WatchId, request: WatchUpdateRequest) -> Result<WatchResult>;
    async fn delete(&self, watch_id: WatchId) -> Result<DeleteResult>;
    async fn history(&self, request: WatchHistoryRequest) -> Result<WatchHistoryResult>;
    async fn capabilities(&self) -> Result<WatchStoreCapability>;
}

pub trait MobileSessionStore {
    async fn list(&self, request: MobileSessionListRequest) -> Result<Page<MobileSessionSummary>>;
    async fn get(&self, session_id: MobileSessionId) -> Result<MobileSessionDetail>;
    async fn upsert(&self, session_id: MobileSessionId, request: MobileSessionUpsertRequest) -> Result<MobileSessionUpsertResult>;
    async fn delete(&self, session_id: MobileSessionId) -> Result<MobileSessionDeleteResult>;
    async fn capabilities(&self) -> Result<MobileSessionStoreCapability>;
}

pub trait ConfigStore {
    async fn read_config(&self) -> Result<PanelConfigDocument>;
    async fn write_config(&self, request: PanelConfigSaveRequest) -> Result<PanelConfigSaveResult>;
    async fn capabilities(&self) -> Result<ConfigStoreCapability>;
}

pub trait CredentialProvider {
    async fn credentials_for(&self, request: CredentialRequest) -> Result<CredentialMaterial>;
    async fn capabilities(&self) -> Result<CredentialProviderCapability>;
}

pub trait DocumentCache {
    async fn get(&self, key: DocumentCacheKey) -> Result<Option<CachedDocument>>;
    async fn put(&self, key: DocumentCacheKey, value: CachedDocument) -> Result<()>;
    async fn invalidate(&self, selector: DocumentCacheInvalidation) -> Result<()>;
    async fn capabilities(&self) -> Result<DocumentCacheCapability>;
}

pub trait HealthProbe {
    async fn probe(&self) -> Result<HealthProbeResult>;
    async fn capabilities(&self) -> Result<HealthProbeCapability>;
}

pub trait RateLimiter {
    async fn acquire(&self, request: RateLimitRequest) -> Result<RateLimitPermit>;
    async fn capabilities(&self) -> Result<RateLimiterCapability>;
}

pub trait SecurityPolicy {
    async fn authorize_source(&self, request: SourceRequest) -> Result<SecurityDecision>;
    async fn authorize_route(&self, request: RouteAuthRequest) -> Result<SecurityDecision>;
    async fn capabilities(&self) -> Result<SecurityPolicyCapability>;
}
```

Current implementations:

| Boundary | Current implementation | Examples of future implementations |
|---|---|---|
| `LlmProvider` | Gemini headless, OpenAI-compatible chat completions, Codex app-server | ACP if completion-capable support is added, local agent runtimes, hosted LLM APIs, fake test provider |
| `EmbeddingProvider` | TEI | OpenAI-compatible embeddings, Ollama, local model runtime, fake test embedding provider |
| `VectorStore` | Qdrant | pgvector, LanceDB, Weaviate, Milvus, in-memory test store |
| `LedgerStore` | SQLite | Postgres, libSQL, durable remote SQLite, in-memory test store |
| `GraphStore` | SQLite | Neo4j, Kuzu, Postgres graph tables, remote graph service, in-memory test store |
| `MemoryStore` | SQLite metadata plus VectorStore/GraphStore links | remote memory service, encrypted local store, in-memory test store |
| `ArtifactStore` | local `~/.axon/artifacts` | S3-compatible storage, content-addressed local store, fake test store |
| `SearchProvider` | SearXNG/Tavily | Brave, Kagi, local search proxy, fake test provider |
| `FetchProvider` | reqwest/HTTP + filesystem/git/package fetchers | remote fetch service, cached fetcher, fake test fetcher |
| `RenderProvider` | HTTP/Chrome render path | Playwright/CDP provider, remote browser service, null/fake renderer |
| `NetworkCaptureProvider` | Chrome/CDP capture | browser proxy capture, disabled/null capture, fake capture |
| `JobStore` | SQLite job tables | Postgres/libSQL, remote job service, in-memory test store |
| `WatchStore` | SQLite watch tables | Postgres/libSQL, platform watcher service, in-memory test store |
| `MobileSessionStore` | local durable session store | remote/mobile sync store, encrypted local store, in-memory test store |
| `ConfigStore` | `~/.axon/config.toml` + `~/.axon/.env` | managed config service, read-only config, fake test store |
| `CredentialProvider` | env/config/request context | keyring, lab/vault, OAuth token store, per-adapter providers |
| `DocumentCache` | in-process ask document cache | disabled/null cache, external cache, fake test cache |
| `HealthProbe` | doctor/service-specific checks | provider-specific probes, synthetic probes, fake test probes |
| `RateLimiter` | semaphores/provider cooling | distributed limiter, provider-specific limiter, fake test limiter |
| `SecurityPolicy` | SSRF/local-path/auth policy | stricter deployment policy, tenant policy, fake test policy |

These boundaries must stay separate. Embedding failures, vector storage
failures, LLM completion failures, ledger persistence failures, graph-store
failures, memory-store failures, artifact-store failures, search/fetch/render
failures, job/watch-store failures, mobile/config-store failures, credential
failures, cache failures, security-policy failures, rate-limit cooling, and
health-probe failures have different retry, scaling, security, and observability
behavior.

Provider capabilities are part of the contract. The service layer must ask each
provider what it supports before selecting behavior:

- `LlmProvider`: model id, context window, streaming support, tool/JSON/schema
  support, reasoning controls, rate limits, concurrency limits, timeout model,
  isolation/security mode, health.
- `EmbeddingProvider`: dimensions, model id, batch limits, max input tokens, sparse output
  support, instruction/prefix support, health.
- `VectorStore`: dense vectors, sparse vectors, hybrid search, payload filters,
  payload indexes, delete-by-filter, collection aliases, transactional semantics,
  health.
- `LedgerStore`: leases, heartbeats, transactions, advisory locks,
  cleanup-debt queries, event replay, schema version support, health.
- `GraphStore`: node/edge/evidence kinds, merge/upsert semantics, conflict
  handling, query depth limits, source/job/evidence filters, health.
- `MemoryStore`: memory types/statuses/link types, decay modes, context budgets,
  contradiction handling, reinforcement semantics, forget behavior,
  health.
- `ArtifactStore`: max object size, content addressing, signed/served URL
  support, retention, delete semantics, WARC support, health.
- `SearchProvider`: backend id, result limits, time-range support, rate limits,
  safe-search/language support, health.
- `FetchProvider`: schemes, byte limits, redirect policy, header policy,
  retries, robots/SSRF behavior, health.
- `RenderProvider`: render modes, browser pool limits, script support,
  screenshot support, timeout model, health.
- `NetworkCaptureProvider`: capture types, endpoint extraction support, privacy
  redaction, browser affinity, health.
- `JobStore`: leases, event replay, cleanup/recover support, retention limits,
  schema version support, health.
- `WatchStore`: schedule kinds, filesystem support, debounce limits, lease
  semantics, history retention, health.
- `MobileSessionStore`: owner scoping, conflict detection, max payload size,
  retention, health.
- `ConfigStore`: readable/writable config domains, restart-required signaling,
  secret redaction, validation support, health.
- `CredentialProvider`: supported auth schemes, secret redaction policy,
  refresh support, user/admin scope requirements, health.
- `DocumentCache`: max entry size, TTL, invalidation, generation awareness,
  security/core-dump requirements, health.
- `HealthProbe`: probe cost, timeout, dependency coverage, degraded/failed
  mapping, health.
- `RateLimiter`: limit keys, burst/concurrency limits, cooling semantics,
  queue/reject behavior, health.
- `SecurityPolicy`: auth scopes, local-path roots, SSRF policy, header
  forwarding policy, route/source authorization, health.

## Strictness Rules

- Use `serde(deny_unknown_fields)` on external request structs.
- Keep request unions explicit; do not silently drop old fields.
- Removed routes/commands/actions are deleted from public DTOs and schemas.
  Do not add compatibility parsing, hidden aliases, or remap DTOs.
- Preserve unknown source-specific options under an adapter-owned object only
  after adapter selection.

## REST Alignment

Every canonical REST route in [../surfaces/rest-contract.md](../surfaces/rest-contract.md)
must map to one of these `axon-api` request/result families. REST may add
HTTP-only path, query, header, cookie, SSE, and raw-byte wrappers, but it must
not invent domain DTOs outside this crate.

| REST family | API request/result family |
|---|---|
| `/healthz`, `/readyz`, `/metrics`, `/v1/server`, `/v1/capabilities`, `/v1/providers`, `/v1/status`, `/v1/doctor`, `/v1/stats` | `ServerInfo`, `CapabilityDocument`, `ProviderCapability`, `HealthReport`, `StatusReport`, `DoctorReport`, `StatsRequest`, `StatsReport` |
| `/v1/resolve`, `/v1/sources`, `/v1/domains` | `SourceRequest`, `SourceResult`, `ResolvedSource`, `SourceSummary`, `SourceItem`, `SourceGeneration*`, `DomainSummary` |
| `/v1/documents`, `/v1/documents/*/chunks` | `DocumentSummary`, `DocumentDetail`, `ChunkSummary`, `ChunkDetail`, `DocumentStatus` |
| `/v1/jobs`, `/v1/jobs/*` | `JobDescriptor`, `JobSummary`, `SourceStatus`, `SourceProgressEvent`, `JobEventPage`, cleanup/recover/clear DTOs |
| `/v1/watches`, `/v1/watches/*` | `WatchRequest`, `WatchResult`, `WatchSummary`, `WatchUpdateRequest`, `WatchHistoryResult` |
| `/v1/graph/*` | `GraphKindDocument`, `GraphResolveRequest`, `GraphResolveResult`, `GraphQueryRequest`, `GraphQueryResult`, `GraphNode`, `GraphEdge`, `GraphEvidence` |
| `/v1/query`, `/v1/retrieve`, `/v1/ask`, `/v1/chat`, `/v1/evaluate`, `/v1/suggest` | `QueryRequest/Result`, `RetrievalRequest/Result`, `AskRequest/Result`, `ChatRequest/Result`, `EvaluationRequest/Result`, `SuggestRequest/Result` |
| `/v1/search`, `/v1/research`, `/v1/summarize`, `/v1/endpoints`, `/v1/brand`, `/v1/diff`, `/v1/screenshot`, `/v1/extract` | `Search*`, `Research*`, `Summarize*`, `EndpointDiscovery*`, `Brand*`, `Diff*`, `Screenshot*`, `Extract*` |
| `/v1/memories/*` | `Memory*` DTOs and `MemoryStore` lifecycle enums |
| `/v1/artifacts`, `/v1/uploads`, `/v1/prune`, `/v1/collections` | `Artifact*`, `Upload*`, `Prune*`, `Dedupe*`, `Purge*`, `Collection*` DTOs |
| `/v1/mobile/sessions/*` | `MobileSession*` DTOs |
| `/api/panel/*` | `Panel*` DTOs; panel auth/session transport remains web-owned |
| streaming routes | `StreamEvent` plus route-specific final result DTO |

## Service Entry Points

`axon-services` should expose:

```rust
pub async fn resolve_source(ctx: &ServiceContext, request: &SourceRequest) -> Result<ResolvedSource>;
pub async fn run_source(ctx: &ServiceContext, request: SourceRequest) -> Result<SourceResult>;
pub async fn get_source_status(ctx: &ServiceContext, job_id: Uuid) -> Result<SourceStatus>;
pub async fn cancel_source_job(ctx: &ServiceContext, job_id: Uuid) -> Result<SourceStatus>;
pub async fn list_source_capabilities(ctx: &ServiceContext) -> Result<Vec<SourceAdapterCapability>>;
pub async fn get_capabilities(ctx: &ServiceContext) -> Result<CapabilityDocument>;
pub async fn get_status(ctx: &ServiceContext) -> Result<StatusReport>;
pub async fn run_doctor(ctx: &ServiceContext) -> Result<DoctorReport>;
pub async fn get_stats(ctx: &ServiceContext, request: StatsRequest) -> Result<StatsReport>;
pub async fn list_providers(ctx: &ServiceContext) -> Result<Vec<ProviderCapability>>;
pub async fn get_provider(ctx: &ServiceContext, provider: ProviderId) -> Result<ProviderCapability>;
pub async fn list_domains(ctx: &ServiceContext, request: DomainListRequest) -> Result<Page<DomainSummary>>;
pub async fn list_documents(ctx: &ServiceContext, request: DocumentListRequest) -> Result<Page<DocumentSummary>>;
pub async fn get_document(ctx: &ServiceContext, document_id: DocumentId) -> Result<DocumentDetail>;
pub async fn list_chunks(ctx: &ServiceContext, request: ChunkListRequest) -> Result<Page<ChunkSummary>>;
pub async fn get_chunk(ctx: &ServiceContext, request: ChunkGetRequest) -> Result<ChunkDetail>;
pub async fn list_jobs(ctx: &ServiceContext, request: JobListRequest) -> Result<Page<JobSummary>>;
pub async fn get_job(ctx: &ServiceContext, job_id: Uuid) -> Result<SourceStatus>;
pub async fn list_job_events(ctx: &ServiceContext, request: JobEventListRequest) -> Result<JobEventPage>;
pub async fn recover_jobs(ctx: &ServiceContext, request: JobRecoverRequest) -> Result<JobRecoverResult>;
pub async fn cleanup_jobs(ctx: &ServiceContext, request: JobCleanupRequest) -> Result<JobCleanupResult>;
pub async fn clear_jobs(ctx: &ServiceContext, request: JobClearRequest) -> Result<JobClearResult>;
pub async fn create_watch(ctx: &ServiceContext, request: WatchRequest) -> Result<WatchResult>;
pub async fn update_watch(ctx: &ServiceContext, watch_id: WatchId, request: WatchUpdateRequest) -> Result<WatchResult>;
pub async fn run_watch(ctx: &ServiceContext, watch_id: WatchId, request: WatchExecRequest) -> Result<JobDescriptor>;
pub async fn list_watches(ctx: &ServiceContext, request: WatchListRequest) -> Result<Page<WatchSummary>>;
pub async fn get_watch(ctx: &ServiceContext, watch_id: WatchId) -> Result<WatchResult>;
pub async fn delete_watch(ctx: &ServiceContext, watch_id: WatchId) -> Result<DeleteResult>;
pub async fn watch_history(ctx: &ServiceContext, request: WatchHistoryRequest) -> Result<WatchHistoryResult>;
pub async fn graph_kinds(ctx: &ServiceContext) -> Result<GraphKindDocument>;
pub async fn graph_resolve(ctx: &ServiceContext, request: GraphResolveRequest) -> Result<GraphResolveResult>;
pub async fn graph_query(ctx: &ServiceContext, request: GraphQueryRequest) -> Result<GraphQueryResult>;
pub async fn graph_node(ctx: &ServiceContext, node_id: GraphNodeId) -> Result<GraphNode>;
pub async fn graph_edge(ctx: &ServiceContext, edge_id: GraphEdgeId) -> Result<GraphEdge>;
pub async fn query(ctx: &ServiceContext, request: QueryRequest) -> Result<QueryResult>;
pub async fn retrieve(ctx: &ServiceContext, request: RetrievalRequest) -> Result<RetrievalResult>;
pub async fn ask(ctx: &ServiceContext, request: AskRequest) -> Result<AskResult>;
pub async fn chat(ctx: &ServiceContext, request: ChatRequest) -> Result<ChatResult>;
pub async fn evaluate(ctx: &ServiceContext, request: EvaluationRequest) -> Result<EvaluationResult>;
pub async fn suggest(ctx: &ServiceContext, request: SuggestRequest) -> Result<SuggestResult>;
pub async fn search(ctx: &ServiceContext, request: SearchRequest) -> Result<SearchResult>;
pub async fn research(ctx: &ServiceContext, request: ResearchRequest) -> Result<ResearchResult>;
pub async fn summarize(ctx: &ServiceContext, request: SummarizeRequest) -> Result<SummarizeResult>;
pub async fn discover_endpoints(ctx: &ServiceContext, request: EndpointDiscoveryRequest) -> Result<EndpointDiscoveryResult>;
pub async fn brand(ctx: &ServiceContext, request: BrandRequest) -> Result<BrandResult>;
pub async fn diff(ctx: &ServiceContext, request: DiffRequest) -> Result<DiffResult>;
pub async fn screenshot(ctx: &ServiceContext, request: ScreenshotRequest) -> Result<ScreenshotResult>;
pub async fn extract(ctx: &ServiceContext, request: ExtractRequest) -> Result<ExtractResult>;
pub async fn remember(ctx: &ServiceContext, request: MemoryRequest) -> Result<MemoryResult>;
pub async fn search_memory(ctx: &ServiceContext, request: MemorySearchRequest) -> Result<MemorySearchResult>;
pub async fn build_memory_context(ctx: &ServiceContext, request: MemoryContextRequest) -> Result<MemoryContextResult>;
pub async fn link_memory(ctx: &ServiceContext, request: MemoryLinkRequest) -> Result<MemoryResult>;
pub async fn import_memory(ctx: &ServiceContext, request: MemoryImportRequest) -> Result<MemoryImportResult>;
pub async fn export_memory(ctx: &ServiceContext, request: MemoryExportRequest) -> Result<MemoryExportResult>;
pub async fn list_artifacts(ctx: &ServiceContext, request: ArtifactListRequest) -> Result<Page<ArtifactSummary>>;
pub async fn get_artifact(ctx: &ServiceContext, artifact_id: ArtifactId) -> Result<ArtifactDetail>;
pub async fn artifact_content(ctx: &ServiceContext, artifact_id: ArtifactId) -> Result<ArtifactContentDescriptor>;
pub async fn create_upload(ctx: &ServiceContext, request: UploadCreateRequest) -> Result<UploadCreateResult>;
pub async fn get_upload(ctx: &ServiceContext, upload_id: UploadId) -> Result<UploadStatus>;
pub async fn put_upload_content(ctx: &ServiceContext, upload_id: UploadId, body: ByteStream) -> Result<UploadStatus>;
pub async fn complete_upload(ctx: &ServiceContext, upload_id: UploadId, request: UploadCompleteRequest) -> Result<UploadCompleteResult>;
pub async fn abort_upload(ctx: &ServiceContext, upload_id: UploadId, request: UploadAbortRequest) -> Result<UploadAbortResult>;
pub async fn plan_prune(ctx: &ServiceContext, request: PrunePlanRequest) -> Result<PrunePlan>;
pub async fn exec_prune(ctx: &ServiceContext, request: PruneExecRequest) -> Result<JobDescriptor>;
pub async fn dedupe(ctx: &ServiceContext, request: DedupeRequest) -> Result<DedupeResult>;
pub async fn purge(ctx: &ServiceContext, request: PurgeRequest) -> Result<PurgeResult>;
pub async fn list_collections(ctx: &ServiceContext, request: CollectionListRequest) -> Result<Page<CollectionSummary>>;
pub async fn get_collection(ctx: &ServiceContext, collection: CollectionName) -> Result<CollectionDetail>;
pub async fn list_mobile_sessions(ctx: &ServiceContext, request: MobileSessionListRequest) -> Result<Page<MobileSessionSummary>>;
pub async fn get_mobile_session(ctx: &ServiceContext, session_id: MobileSessionId) -> Result<MobileSessionDetail>;
pub async fn upsert_mobile_session(ctx: &ServiceContext, session_id: MobileSessionId, request: MobileSessionUpsertRequest) -> Result<MobileSessionUpsertResult>;
pub async fn delete_mobile_session(ctx: &ServiceContext, session_id: MobileSessionId) -> Result<MobileSessionDeleteResult>;
```

Transports call these functions. They do not call crawl/embed/ingest internals
directly for happy-path acquisition. Memory transports likewise call the shared
memory service entry points rather than writing graph/vector/SQLite state
directly.

Panel routes are REST surfaces, but their DTOs still live in `axon-api` so the
web app, server handlers, and tests share one schema. Panel services may live in
`axon-web` or `axon-services` depending on whether they are pure UI control
plane behavior, but the request/response types are not ad hoc handler-local
JSON.

## Execution Affinity

Execution affinity is caller-supplied and validated in services:

- CLI local execution may access host paths directly.
- MCP/REST local path execution must pass configured allowed-root validation.
- Detached local watches require an explicit watch/admin path.
- A resolver result must not imply permission to read local files.
