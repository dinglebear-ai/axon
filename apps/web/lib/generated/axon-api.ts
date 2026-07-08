/**
 * This file was generated from apps/web/openapi/axon.json.
 * Do not edit by hand; run npm run openapi:generate.
 */

export type components = {
    schemas: {
        "AcceptedJob": {
            "job_id": string;
            "status": string;
            "status_url": string;
        };
        "AdapterOptions": {
            "values": components['schemas']['MetadataMap'];
        };
        "AdapterRef": {
            "name": string;
            "version": string;
        };
        "ApiError": {
            "chunk_id"?: string | null;
            "code": components['schemas']['ErrorCode'];
            "cooldown_until"?: string | null;
            "details": Record<string, unknown>;
            "document_id"?: string | null;
            "job_id"?: string | null;
            "message": string;
            "provider_id"?: string | null;
            "retry_after_ms"?: number | null;
            "retryable": boolean;
            "severity": components['schemas']['ErrorSeverity'];
            "source_id"?: string | null;
            "source_item_key"?: string | null;
            "stage": components['schemas']['ErrorStage'];
            "visibility": components['schemas']['ErrorVisibility'];
        };
        "ArtifactHandle": {
            "bytes": number;
            "display_path": string;
            "job_id"?: string | null;
            "kind": string;
            "line_count"?: number | null;
            "relative_path": string;
            "url"?: string | null;
        };
        "ArtifactId": string;
        "ArtifactKind": "raw_content" | "normalized_content" | "manifest" | "report" | "screenshot" | "warc" | "provider_trace";
        "ArtifactMode": "none" | "on_large_output" | "always";
        "ArtifactRef": {
            "artifact_id": components['schemas']['ArtifactId'];
            "artifact_kind": components['schemas']['ArtifactKind'];
            "content_hash"?: string | null;
            "created_at": components['schemas']['Timestamp'];
            "size_bytes"?: number | null;
            "uri": string;
        };
        "AuthorityEvidence": {
            "confidence": number;
            "evidence_kind": string;
            "value": string;
        };
        "AuthorityHint": {
            "authority": components['schemas']['AuthorityLevel'];
            "canonical_uri"?: string | null;
            "evidence": components['schemas']['AuthorityEvidence'][];
        };
        "AuthorityLevel": "official" | "verified" | "user_pinned" | "inferred" | "community" | "mirror" | "conflicting" | "unknown";
        "BatchId": string;
        "BrandColor": {
            "count": number;
            "hex": string;
            "usage": components['schemas']['ColorUsage'];
        };
        "BrandResult": {
            "colors": components['schemas']['BrandColor'][];
            "favicon_url"?: string | null;
            "fonts": string[];
            "logo_url"?: string | null;
            "logos": components['schemas']['LogoVariant'][];
            "name"?: string | null;
            "og_image"?: string | null;
            "url": string;
        };
        "CheckpointId": string;
        "ChunkId": string;
        "CleanupDebtId": string;
        "ColorUsage": "primary" | "secondary" | "background" | "text" | "accent" | "unknown";
        "ContentRef": {
            "kind": "inline_text";
            "text": string;
        } | {
            "bytes_base64": string;
            "kind": "inline_bytes";
            "mime_type": string;
        } | {
            "artifact_id": components['schemas']['ArtifactId'];
            "kind": "artifact";
        } | {
            "integrity"?: string | null;
            "kind": "external";
            "uri": string;
        };
        "DedupeRequest": {
            "collection"?: string | null;
        };
        "DeleteMobileSessionResponse": {
            "ok": boolean;
        };
        "DiffResult": {
            "links_added": components['schemas']['LinkEntry'][];
            "links_removed": components['schemas']['LinkEntry'][];
            "metadata_changes": components['schemas']['MetadataChange'][];
            "status": components['schemas']['DiffStatus'];
            "text_diff"?: string | null;
            "url_a": string;
            "url_b": string;
            "word_count_delta": number;
        };
        "DiffStatus": "same" | "changed";
        "DiscoveredEndpoint": {
            "first_party": boolean;
            "kind": components['schemas']['EndpointKind'];
            "normalized_url"?: string | null;
            "rpc_probe"?: null | components['schemas']['RpcProbeResult'];
            "source": components['schemas']['EndpointSourceKind'];
            "source_url"?: string | null;
            "value": string;
            "verified"?: null | components['schemas']['EndpointVerification'];
        };
        "DocumentId": string;
        "EndpointKind": "relative_path" | "absolute_url" | "graphql" | "websocket";
        "EndpointReport": {
            "bundles_fetched": number;
            "bundles_scanned": number;
            "elapsed_ms": number;
            "endpoints": components['schemas']['DiscoveredEndpoint'][];
            "hosts": string[];
            "mcp_candidates"?: components['schemas']['McpCandidateAttempt'][];
            "scripts_discovered": number;
            "truncated": boolean;
            "url": string;
            "warnings": string[];
        };
        "EndpointSourceKind": "inline_script" | "script_bundle" | "html_attribute" | "network_capture" | "synthesized_mcp";
        "EndpointVerification": {
            "attempted_url": string;
            "content_type"?: string | null;
            "error"?: string | null;
            "final_url"?: string | null;
            "method": string;
            "reachable": boolean;
            "redirect_count": number;
            "status"?: number | null;
        };
        "EndpointsRequest": {
            "capture_network"?: boolean | null;
            "first_party_only"?: boolean | null;
            "include_bundles"?: boolean | null;
            "max_scan_bytes"?: number | null;
            "max_scripts"?: number | null;
            "probe_rpc"?: boolean | null;
            "probe_rpc_subdomains"?: boolean | null;
            "unique_only"?: boolean | null;
            "url": string;
            "verify"?: boolean | null;
        };
        "ErrorBody": {
            "diagnostics"?: Record<string, unknown>;
            "kind": components['schemas']['ErrorKind'];
            "message": string;
        };
        "ErrorCode": string;
        "ErrorEnvelope": {
            "contract_version": string;
            "error": components['schemas']['ApiError'];
            "ok": boolean;
            "request_id": string;
            "trace": components['schemas']['TraceContext'];
        };
        "ErrorKind": "bad_gateway" | "bad_request" | "challenge_detected" | "forbidden" | "internal" | "invalid_path" | "ladder_exhausted" | "not_found" | "output_dir_error" | "path_error" | "path_escape" | "payload_too_large" | "rate_limited" | "read_error" | "structured_data_malformed" | "symlink_not_allowed" | "timeout" | "unauthorized" | "unsupported_media_type" | "upstream_unavailable" | "vertical_auth_invalid" | "vertical_auth_missing" | "vertical_blocked_antibot" | "vertical_rate_limited" | "vertical_target_not_found" | "vertical_target_unavailable" | "vertical_unsupported_url";
        "ErrorSeverity": "info" | "warning" | "degraded" | "failed" | "fatal";
        "ErrorStage": "parsing" | "validation" | "resolving" | "routing" | "authorizing" | "planning" | "leasing" | "discovering" | "diffing" | "fetching" | "rendering" | "normalizing" | "parsing_content" | "graphing" | "preparing" | "embedding" | "upserting" | "publishing" | "cleaning" | "retrieving" | "synthesizing" | "observing";
        "ErrorVisibility": "public" | "internal" | "sensitive";
        "ExecutionMode": "foreground" | "background" | "wait";
        "ExecutionPolicy": {
            "detached": boolean;
            "heartbeat_interval_secs": number;
            "mode": components['schemas']['ExecutionMode'];
            "priority": components['schemas']['JobPriority'];
            "wait_timeout_secs"?: number | null;
        };
        "GraphEdgeId": string;
        "GraphNodeId": string;
        "GraphWriteSummary": {
            "degraded": boolean;
            "edges_upserted": number;
            "evidence_records": number;
            "nodes_upserted": number;
        };
        "InlineSourceResult": {
            "content"?: null | components['schemas']['ContentRef'];
            "metadata": components['schemas']['MetadataMap'];
            "summary"?: string | null;
        };
        "JobArtifactListResult": {
            "artifacts": components['schemas']['ArtifactRef'][];
            "limit": number;
            "next_cursor"?: string | null;
        };
        "JobCancelRequest": {
            "force_after_ms"?: number | null;
            "reason"?: string | null;
        };
        "JobCancelResult": {
            "canceled_at"?: null | components['schemas']['Timestamp'];
            "job_id": components['schemas']['JobId'];
            "reason"?: string | null;
            "status": components['schemas']['LifecycleStatus'];
        };
        "JobCleanupRequest": {
            "dry_run": boolean;
            "kind"?: null | components['schemas']['JobKind'];
            "limit"?: number | null;
            "older_than"?: null | components['schemas']['Timestamp'];
            "status"?: null | components['schemas']['LifecycleStatus'];
        };
        "JobCleanupResult": {
            "artifacts_pruned": number;
            "attempts_pruned": number;
            "deleted": number;
            "dry_run": boolean;
            "events_pruned": number;
            "heartbeats_pruned": number;
            "jobs_pruned": number;
            "matched": number;
            "reservations_pruned": number;
            "stages_pruned": number;
            "warnings": components['schemas']['SourceWarning'][];
        };
        "JobClearRequest": {
            "confirm": boolean;
            "kind"?: null | components['schemas']['JobKind'];
            "older_than"?: null | components['schemas']['Timestamp'];
            "status"?: null | components['schemas']['LifecycleStatus'];
        };
        "JobClearResult": {
            "deleted": number;
            "status"?: null | components['schemas']['LifecycleStatus'];
            "warnings": components['schemas']['SourceWarning'][];
        };
        "JobDescriptor": {
            "cancel_url"?: string | null;
            "events_url": string;
            "id": components['schemas']['JobId'];
            "kind": components['schemas']['JobKind'];
            "poll_after_ms": number;
            "retry_url"?: string | null;
            "status_url": string;
            "stream_url": string;
        };
        "JobEvent": {
            "attempt"?: number;
            "details": components['schemas']['MetadataMap'];
            "event_id": string;
            "job_id": components['schemas']['JobId'];
            "message": string;
            "phase": components['schemas']['PipelinePhase'];
            "sequence": number;
            "severity": components['schemas']['Severity'];
            "stage_id"?: null | components['schemas']['StageId'];
            "status": components['schemas']['LifecycleStatus'];
            "timestamp": components['schemas']['Timestamp'];
            "visibility"?: components['schemas']['Visibility'];
        };
        "JobEventPage": {
            "events": components['schemas']['JobEvent'][];
            "last_sequence": number;
            "limit"?: number;
            "next_cursor"?: string | null;
        };
        "JobFamily": "embed" | "extract" | "ingest";
        "JobId": string;
        "JobKind": "source" | "watch" | "map" | "extract" | "research" | "ask" | "query" | "retrieve" | "memory" | "graph" | "prune" | "provider_probe" | "reset";
        "JobMetric": {
            "label": string;
            "value": string;
        };
        "JobPhase": "pending" | "running" | "done" | "failed" | "canceled";
        "JobPriority": "interactive" | "high" | "normal" | "background" | "maintenance";
        "JobProgress": {
            "error"?: string | null;
            "family": components['schemas']['JobFamily'];
            "metrics": components['schemas']['JobMetric'][];
            "percent"?: number | null;
            "phase": components['schemas']['JobPhase'];
        };
        "JobRecoveryRequest": {
            "kind"?: null | components['schemas']['JobKind'];
            "limit"?: number | null;
            "stale_before"?: null | components['schemas']['Timestamp'];
        };
        "JobRecoveryResult": {
            "job_ids": components['schemas']['JobId'][];
            "recovered": number;
            "warnings": components['schemas']['SourceWarning'][];
        };
        "JobRetryMode": "same_config" | "with_overrides";
        "JobRetryRequest": {
            "from_phase"?: null | components['schemas']['PipelinePhase'];
            "idempotency_key"?: string | null;
            "mode": components['schemas']['JobRetryMode'];
            "overrides"?: components['schemas']['MetadataMap'];
        };
        "JobRetryResult": {
            "attempt": number;
            "original_job_id": components['schemas']['JobId'];
            "retry_job": components['schemas']['JobDescriptor'];
        };
        "JobStatusResponse": {
            "job": unknown;
            "progress"?: null | components['schemas']['JobProgress'];
        };
        "JobSummary": {
            "counts"?: null | components['schemas']['StageCounts'];
            "created_at": components['schemas']['Timestamp'];
            "job_id": components['schemas']['JobId'];
            "kind": components['schemas']['JobKind'];
            "last_error"?: null | components['schemas']['SourceError'];
            "phase": components['schemas']['PipelinePhase'];
            "source_id"?: null | components['schemas']['SourceId'];
            "status": components['schemas']['LifecycleStatus'];
            "updated_at": components['schemas']['Timestamp'];
            "watch_id"?: null | components['schemas']['WatchId'];
        };
        "LedgerSummary": {
            "committed_generation"?: null | components['schemas']['SourceGenerationId'];
            "counts": components['schemas']['SourceCounts'];
            "generation": components['schemas']['SourceGenerationId'];
            "source_id": components['schemas']['SourceId'];
            "status": components['schemas']['LifecycleStatus'];
        };
        "LifecycleStatus": "queued" | "pending" | "running" | "waiting" | "blocked" | "canceling" | "completed" | "completed_degraded" | "failed" | "canceled" | "expired" | "skipped";
        "LinkEntry": {
            "href": string;
            "text": string;
        };
        "LogoVariant": {
            "kind": string;
            "url": string;
        };
        "McpCandidateAttempt": {
            "host_kind": components['schemas']['McpHostKind'];
            "outcome": components['schemas']['McpProbeOutcome'];
            "path": string;
            "rpc_probe"?: null | components['schemas']['RpcProbeResult'];
            "url": string;
        };
        "McpHostKind": "same_host" | "apex_subdomain";
        "McpProbeOutcome": "confirmed" | "unconfirmed" | "blocked";
        "MemoryId": string;
        "MetadataChange": {
            "field": string;
            "new"?: string | null;
            "old"?: string | null;
        };
        "MetadataMap": Record<string, unknown>;
        "MobileChatItem": {
            "kind": string;
            "payload"?: Record<string, unknown>;
            "text"?: string | null;
            "timestamp": number;
        };
        "MobileSession": {
            "created_at": number;
            "first_message_preview": string;
            "id": string;
            "injected_op_count": number;
            "items"?: components['schemas']['MobileChatItem'][];
            "pinned_at"?: number | null;
            "title": string;
            "turn_count": number;
            "updated_at": number;
        };
        "MobileSessionDetailResponse": {
            "session": components['schemas']['MobileSession'];
        };
        "MobileSessionListResponse": {
            "sessions": components['schemas']['MobileSessionSummary'][];
        };
        "MobileSessionSummary": {
            "created_at": number;
            "first_message_preview": string;
            "id": string;
            "injected_op_count": number;
            "pinned_at"?: number | null;
            "title": string;
            "turn_count": number;
            "updated_at": number;
        };
        "OutputPolicy": {
            "artifact_mode": components['schemas']['ArtifactMode'];
            "include_progress": boolean;
            "inline_limit_bytes": number;
            "json": boolean;
            "response_mode": components['schemas']['ResponseMode'];
        };
        "PanelCollectionsResponse": {
            "collections": string[];
        };
        "PipelinePhase": "queued" | "requested" | "resolving" | "routing" | "authorizing" | "planning" | "leasing" | "discovering" | "diffing" | "fetching" | "rendering" | "enriching" | "normalizing" | "parsing" | "graphing" | "preparing" | "batching" | "embedding" | "vectorizing" | "upserting" | "retrieving" | "synthesizing" | "evaluating" | "publishing" | "cleaning" | "complete" | "canceled";
        "ProgressCurrent": {
            "adapter"?: string | null;
            "chunk_id"?: null | components['schemas']['ChunkId'];
            "document_id"?: null | components['schemas']['DocumentId'];
            "message"?: string | null;
            "provider"?: null | components['schemas']['ProviderId'];
            "source_item_key"?: null | components['schemas']['SourceItemKey'];
        };
        "ProgressThroughput": {
            "bytes_per_second"?: number | null;
            "chunks_per_second"?: number | null;
            "items_per_second"?: number | null;
        };
        "ProgressTiming": {
            "elapsed_ms": number;
            "eta_ms"?: number | null;
            "started_at": components['schemas']['Timestamp'];
            "updated_at": components['schemas']['Timestamp'];
        };
        "ProviderId": string;
        "PruneCounts": {
            "artifacts": number;
            "cache_entries": number;
            "graph_edges": number;
            "graph_nodes": number;
            "jobs": number;
            "ledger_generations": number;
            "memory_records": number;
            "vector_points": number;
        };
        "PruneEstimate": {
            "artifacts": number;
            "cache_entries": number;
            "graph_edges": number;
            "graph_nodes": number;
            "jobs": number;
            "ledger_generations": number;
            "memory_records": number;
            "vector_points": number;
        };
        "PruneExecRequest": {
            "confirm"?: boolean;
            "generation"?: string | null;
            "target": string;
        };
        "PrunePlan": {
            "destructive": boolean;
            "estimated": components['schemas']['PruneEstimate'];
            "job_id": components['schemas']['JobId'];
            "requires_admin": boolean;
            "selector": components['schemas']['PruneSelector'];
            "steps": components['schemas']['PruneStep'][];
            "warnings": components['schemas']['SourceWarning'][];
        };
        "PrunePlanRequest": {
            "generation"?: string | null;
            "target": string;
        };
        "PruneResult": {
            "cleanup_debt_remaining": number;
            "deleted_counts": components['schemas']['PruneCounts'];
            "job_id": components['schemas']['JobId'];
            "status": components['schemas']['LifecycleStatus'];
            "steps": components['schemas']['PruneStepResult'][];
        };
        "PruneSelector": {
            "kind": "source";
            "source_id": components['schemas']['SourceId'];
        } | {
            "generation": components['schemas']['SourceGenerationId'];
            "kind": "generation";
            "source_id": components['schemas']['SourceId'];
        } | {
            "debt_id": components['schemas']['CleanupDebtId'];
            "kind": "cleanup_debt";
        } | {
            "collection": string;
            "kind": "collection";
        } | {
            "artifact_id": components['schemas']['ArtifactId'];
            "kind": "artifact";
        } | {
            "edge_id"?: null | components['schemas']['GraphEdgeId'];
            "kind": "graph";
            "node_id"?: null | components['schemas']['GraphNodeId'];
        } | {
            "kind": "memory";
            "memory_id"?: null | components['schemas']['MemoryId'];
        } | {
            "kind": "job_retention";
            "older_than_days": number;
        } | {
            "kind": "cache";
            "older_than_days": number;
        };
        "PruneStep": {
            "description": string;
            "estimated_deletes": number;
            "generation"?: null | components['schemas']['SourceGenerationId'];
            "source_id"?: null | components['schemas']['SourceId'];
            "target": components['schemas']['PruneTargetKind'];
            "vector_selector"?: null | components['schemas']['VectorDeleteSelector'];
        };
        "PruneStepResult": {
            "deleted": number;
            "generation"?: null | components['schemas']['SourceGenerationId'];
            "skipped_reason"?: string | null;
            "source_id"?: null | components['schemas']['SourceId'];
            "status": components['schemas']['LifecycleStatus'];
            "target": components['schemas']['PruneTargetKind'];
        };
        "PruneTargetKind": "vector" | "artifact" | "graph" | "memory" | "ledger" | "job_retention" | "cache";
        "PurgeRequest": {
            "collection"?: string | null;
            "dry_run"?: boolean | null;
            "prefix"?: boolean;
            "response_mode"?: null | components['schemas']['ResponseMode'];
            "target"?: string | null;
        };
        "PurgeResult": {
            "deleted_points": number;
            "dry_run": boolean;
            "matched_points": number;
            "matched_url_count": number;
            "prefix": boolean;
            "sample_urls": string[];
            "target": string;
        };
        "ReadinessBody": {
            "ok": boolean;
            "qdrant": string;
            "sqlite": string;
            "tei": string;
        };
        "RenderMode": "http" | "chrome" | "auto-switch";
        "ReservationId": string;
        "ResponseMode": "path" | "inline" | "both" | "auto_inline";
        "RestAskRequest": {
            "ask_authoritative_boost"?: number | null;
            "ask_authoritative_domains"?: string[] | null;
            "ask_backfill_chunks"?: number | null;
            "ask_candidate_limit"?: number | null;
            "ask_chunk_limit"?: number | null;
            "ask_doc_chunk_limit"?: number | null;
            "ask_doc_fetch_concurrency"?: number | null;
            "ask_full_docs"?: number | null;
            "ask_hybrid_candidates"?: number | null;
            "ask_max_context_chars"?: number | null;
            "ask_min_citations_nontrivial"?: number | null;
            "ask_min_relevance_score"?: number | null;
            "before"?: string | null;
            "collection"?: string | null;
            "diagnostics"?: boolean | null;
            "explain"?: boolean | null;
            "hybrid_search"?: boolean | null;
            "query": string;
            "since"?: string | null;
        };
        "RestBrandRequest": {
            "url": string;
        };
        "RestChatRequest": {
            "message": string;
        };
        "RestChatResponse": {
            "answer": string;
            "message": string;
            "model"?: string | null;
        };
        "RestDiffRequest": {
            "render_mode"?: null | components['schemas']['RenderMode'];
            "url_a": string;
            "url_b": string;
        };
        "RestEvaluateRequest": {
            "before"?: string | null;
            "collection"?: string | null;
            "diagnostics"?: boolean | null;
            "hybrid_search"?: boolean | null;
            "question": string;
            "retrieval_ab"?: boolean | null;
            "since"?: string | null;
        };
        "RestExtractMode": "auto";
        "RestExtractRequest": {
            "collection"?: string | null;
            "embed"?: boolean | null;
            "headers"?: string[];
            "max_pages"?: number | null;
            "mode"?: null | components['schemas']['RestExtractMode'];
            "prompt"?: string | null;
            "render_mode"?: null | components['schemas']['RenderMode'];
            "urls": string[];
        };
        "RestMapRequest": {
            "limit"?: number | null;
            "offset"?: number | null;
            "url": string;
        };
        "RestMemoryEdgeType": "relates_to" | "supersedes";
        "RestMemoryNodeType": "decision" | "fact" | "preference" | "task" | "bug";
        "RestMemoryRequest": {
            "amount"?: number | null;
            "archive_sources"?: boolean | null;
            "body"?: string | null;
            "confidence"?: number | null;
            "depth"?: number | null;
            "edge_type"?: null | components['schemas']['RestMemoryEdgeType'];
            "file"?: string | null;
            "id"?: string | null;
            "limit"?: number | null;
            "memory_ids"?: string[] | null;
            "memory_type"?: null | components['schemas']['RestMemoryNodeType'];
            "pinned"?: boolean | null;
            "project"?: string | null;
            "query"?: string | null;
            "reason"?: string | null;
            "repo"?: string | null;
            "source_id"?: string | null;
            "status"?: string | null;
            "strategy"?: string | null;
            "subaction"?: null | components['schemas']['RestMemorySubaction'];
            "target_id"?: string | null;
            "title"?: string | null;
            "token_budget"?: number | null;
        };
        "RestMemorySubaction": "remember" | "list" | "search" | "show" | "link" | "supersede" | "context" | "reinforce" | "contradict" | "pin" | "archive" | "forget" | "review" | "compact";
        "RestQueryRequest": {
            "before"?: string | null;
            "collection"?: string | null;
            "hybrid_search"?: boolean | null;
            "limit"?: number | null;
            "offset"?: number | null;
            "query": string;
            "since"?: string | null;
        };
        "RestResearchRequest": {
            "limit"?: number | null;
            "offset"?: number | null;
            "query": string;
            "time_range"?: string | null;
        };
        "RestRetrieveRequest": {
            "before"?: string | null;
            "collection"?: string | null;
            "cursor"?: string | null;
            "max_points"?: number | null;
            "since"?: string | null;
            "token_budget"?: number | null;
            "url": string;
        };
        "RestScreenshotRequest": {
            "full_page"?: boolean | null;
            "url": string;
            "viewport"?: string | null;
        };
        "RestSearchRequest": {
            "limit"?: number | null;
            "offset"?: number | null;
            "query": string;
            "time_range"?: string | null;
        };
        "RestSuggestRequest": {
            "collection"?: string | null;
            "focus"?: string | null;
        };
        "RestSummarizeRequest": {
            "exclude_selector"?: string | null;
            "headers"?: string[];
            "render_mode"?: null | components['schemas']['RenderMode'];
            "root_selector"?: string | null;
            "url"?: string | null;
            "urls"?: string[] | null;
        };
        "RetryState": {
            "attempt": number;
            "max_attempts"?: number | null;
            "next_retry_at"?: null | components['schemas']['Timestamp'];
            "reason": string;
        };
        "RpcProbeResult": {
            "methods"?: string[];
            "protocol"?: null | components['schemas']['RpcProtocol'];
            "server_name"?: string | null;
            "server_version"?: string | null;
            "tools"?: string[];
            "transport"?: null | components['schemas']['RpcTransport'];
        };
        "RpcProtocol": "jsonrpc2" | "openrpc" | "mcp";
        "RpcTransport": "http" | "sse";
        "ScreenshotResult": {
            "artifact_handle"?: null | components['schemas']['ArtifactHandle'];
            "path": string;
            "size_bytes": number;
            "url": string;
        };
        "ServerInfo": {
            "minimum_client_schema_version": string;
            "required_request_fields"?: string[];
            "schema_version": string;
            "supported_actions"?: string[];
            "supported_routes": string[];
            "version": string;
        };
        "Severity": "debug" | "info" | "warning" | "degraded" | "failed" | "fatal";
        "SourceCounts": {
            "bytes_total": number;
            "chunks_total": number;
            "documents_total": number;
            "items_changed": number;
            "items_total": number;
            "vector_points_total": number;
        };
        "SourceError": {
            "cause"?: string | null;
            "code": string;
            "message": string;
            "provider_id"?: null | components['schemas']['ProviderId'];
            "retryable": boolean;
            "severity": components['schemas']['Severity'];
            "source_item_key"?: null | components['schemas']['SourceItemKey'];
        };
        "SourceGenerationId": string;
        "SourceId": string;
        "SourceIntent": "acquire" | "refresh" | "watch" | "map";
        "SourceItemKey": string;
        "SourceKind": "web" | "local" | "git" | "registry" | "feed" | "reddit" | "youtube" | "session" | "cli_tool" | "mcp_tool" | "memory" | "upload";
        "SourceLimits": {
            "max_bytes_per_item"?: number | null;
            "max_chunks"?: number | null;
            "max_depth"?: number | null;
            "max_items"?: number | null;
            "max_pages"?: number | null;
            "max_total_bytes"?: number | null;
            "provider_timeout_ms"?: number | null;
        };
        "SourceProgressEvent": {
            "adapter"?: null | components['schemas']['AdapterRef'];
            "attempt"?: number;
            "batch_id"?: null | components['schemas']['BatchId'];
            "canonical_uri"?: string | null;
            "checkpoint_id"?: null | components['schemas']['CheckpointId'];
            "counts": components['schemas']['StageCounts'];
            "current"?: null | components['schemas']['ProgressCurrent'];
            "dedupe_key"?: string | null;
            "error"?: null | components['schemas']['ApiError'];
            "event_id": string;
            "generation"?: null | components['schemas']['SourceGenerationId'];
            "job_id": components['schemas']['JobId'];
            "message": string;
            "phase": components['schemas']['PipelinePhase'];
            "reservation_id"?: null | components['schemas']['ReservationId'];
            "retry"?: null | components['schemas']['RetryState'];
            "scope"?: null | components['schemas']['SourceScope'];
            "sequence": number;
            "severity": components['schemas']['Severity'];
            "source_id"?: null | components['schemas']['SourceId'];
            "stage_id"?: null | components['schemas']['StageId'];
            "status": components['schemas']['LifecycleStatus'];
            "throughput"?: null | components['schemas']['ProgressThroughput'];
            "timestamp": components['schemas']['Timestamp'];
            "timing"?: null | components['schemas']['ProgressTiming'];
            "visibility": components['schemas']['Visibility'];
            "warning"?: null | components['schemas']['SourceWarning'];
        };
        "SourceRefreshPolicy": "if_stale" | "force" | "never";
        "SourceRequest": {
            "adapter"?: string | null;
            "authority_hint"?: null | components['schemas']['AuthorityHint'];
            "collection"?: string | null;
            "embed"?: boolean;
            "execution"?: components['schemas']['ExecutionPolicy'];
            "idempotency_key"?: string | null;
            "intent"?: components['schemas']['SourceIntent'];
            "limits"?: components['schemas']['SourceLimits'];
            "metadata"?: components['schemas']['MetadataMap'];
            "options"?: components['schemas']['AdapterOptions'];
            "output"?: components['schemas']['OutputPolicy'];
            "refresh"?: components['schemas']['SourceRefreshPolicy'];
            "scope"?: null | components['schemas']['SourceScope'];
            "source": string;
            "watch"?: components['schemas']['SourceWatchPolicy'];
        };
        "SourceResult": {
            "adapter": components['schemas']['AdapterRef'];
            "canonical_uri": string;
            "counts": components['schemas']['SourceCounts'];
            "graph": components['schemas']['GraphWriteSummary'];
            "inline"?: null | components['schemas']['InlineSourceResult'];
            "job"?: null | components['schemas']['JobDescriptor'];
            "job_id": components['schemas']['JobId'];
            "ledger": components['schemas']['LedgerSummary'];
            "scope": components['schemas']['SourceScope'];
            "source_id": components['schemas']['SourceId'];
            "source_kind": components['schemas']['SourceKind'];
            "status": components['schemas']['LifecycleStatus'];
            "warnings": components['schemas']['SourceWarning'][];
            "watch"?: null | components['schemas']['WatchResult'];
        };
        "SourceResultRef": {
            "job_id": components['schemas']['JobId'];
            "source_id": components['schemas']['SourceId'];
            "status": components['schemas']['LifecycleStatus'];
        };
        "SourceScope": "page" | "site" | "docs" | "repo" | "workspace" | "branch" | "org" | "package" | "version" | "feed" | "subreddit" | "thread" | "comment" | "video" | "playlist" | "channel" | "issue" | "pull_request" | "merge_request" | "release" | "wiki" | "file" | "directory" | "map" | "tool" | "script" | "api";
        "SourceWarning": {
            "code": string;
            "message": string;
            "retryable": boolean;
            "severity": components['schemas']['Severity'];
            "source_item_key"?: null | components['schemas']['SourceItemKey'];
        };
        "SourceWatchPolicy": "disabled" | "ensure" | "enabled";
        "StageCounts": {
            "bytes_done": number;
            "bytes_total"?: number | null;
            "chunks_done": number;
            "chunks_total"?: number | null;
            "documents_done": number;
            "documents_total"?: number | null;
            "items_done": number;
            "items_total"?: number | null;
        };
        "StageId": string;
        "StreamEvent": {
            "event": components['schemas']['SourceProgressEvent'];
            "kind": "progress";
        } | {
            "kind": "result";
            "result": components['schemas']['SourceResultRef'];
        } | {
            "error": components['schemas']['ApiError'];
            "kind": "error";
        } | {
            "kind": "heartbeat";
            "timestamp": components['schemas']['Timestamp'];
        };
        "Timestamp": string;
        "TraceContext": {
            "attributes": components['schemas']['MetadataMap'];
            "parent_span_id"?: string | null;
            "sampled": boolean;
            "span_id"?: string | null;
            "trace_id": string;
        };
        "UpsertMobileSessionRequest": {
            "session": components['schemas']['MobileSession'];
        };
        "UpsertMobileSessionResponse": {
            "ok": boolean;
            "session": components['schemas']['MobileSession'];
        };
        "VectorDeleteSelector": {
            "collection": string;
            "generation"?: null | components['schemas']['SourceGenerationId'];
            "kind": "source";
            "source_id": components['schemas']['SourceId'];
        } | {
            "collection": string;
            "generation": components['schemas']['SourceGenerationId'];
            "kind": "generation";
            "source_id": components['schemas']['SourceId'];
        } | {
            "collection": string;
            "document_id": components['schemas']['DocumentId'];
            "generation"?: null | components['schemas']['SourceGenerationId'];
            "kind": "document";
        } | {
            "chunk_ids": components['schemas']['ChunkId'][];
            "collection": string;
            "kind": "chunks";
        } | {
            "collection": string;
            "kind": "points";
            "point_ids": components['schemas']['VectorPointId'][];
        } | {
            "canonical_uri": string;
            "collection": string;
            "kind": "canonical_uri";
            "match_prefix": boolean;
        } | {
            "collection": string;
            "filter": unknown;
            "kind": "filter";
        };
        "VectorPointId": string;
        "Visibility": "public" | "internal" | "sensitive" | "redacted" | "derived";
        "WatchDefCreateRequest": {
            "enabled"?: boolean | null;
            "every_seconds": number;
            "name": string;
            "next_run_at"?: string | null;
            "task_payload": unknown;
            "task_type": string;
        };
        "WatchId": string;
        "WatchResult": {
            "adapter": components['schemas']['AdapterRef'];
            "canonical_uri": string;
            "enabled": boolean;
            "job"?: null | components['schemas']['JobDescriptor'];
            "latest_job"?: null | components['schemas']['JobDescriptor'];
            "schedule": components['schemas']['WatchSchedule'];
            "scope": components['schemas']['SourceScope'];
            "source_id": components['schemas']['SourceId'];
            "warnings": components['schemas']['SourceWarning'][];
            "watch_id": components['schemas']['WatchId'];
        };
        "WatchSchedule": {
            "cron"?: string | null;
            "every_seconds": number;
            "timezone"?: string | null;
        };
    };
};

export type paths = {
    "/healthz": { get: operations["healthz"] };
    "/readyz": { get: operations["readyz"] };
    "/v1/artifacts": { get: operations["serve_artifact_query"] };
    "/v1/ask": { post: operations["v1_ask"] };
    "/v1/ask/stream": { post: operations["v1_ask_stream"] };
    "/v1/brand": { post: operations["brand"] };
    "/v1/capabilities": { get: operations["v1_capabilities"] };
    "/v1/chat": { post: operations["v1_chat"] };
    "/v1/chat/stream": { post: operations["v1_chat_stream"] };
    "/v1/collections": { get: operations["collections_openapi_marker"] };
    "/v1/dedupe": { post: operations["dedupe"] };
    "/v1/diff": { post: operations["diff"] };
    "/v1/doctor": { get: operations["doctor"] };
    "/v1/domains": { get: operations["domains"] };
    "/v1/endpoints": { post: operations["endpoints"] };
    "/v1/evaluate": { post: operations["evaluate"] };
    "/v1/extract": { get: operations["list_extract_jobs"]; post: operations["start_extract"]; delete: operations["clear_extract_jobs"] };
    "/v1/extract/cleanup": { post: operations["cleanup_extract_jobs"] };
    "/v1/extract/recover": { post: operations["recover_extract_jobs"] };
    "/v1/extract/{id}": { get: operations["extract_job_status"] };
    "/v1/extract/{id}/cancel": { post: operations["cancel_extract_job"] };
    "/v1/jobs": { get: operations["list_unified_jobs"]; delete: operations["clear_unified_jobs"] };
    "/v1/jobs/cleanup": { post: operations["cleanup_unified_jobs"] };
    "/v1/jobs/recover": { post: operations["recover_unified_jobs"] };
    "/v1/jobs/{id}": { get: operations["unified_job_status"] };
    "/v1/jobs/{id}/artifacts": { get: operations["unified_job_artifacts"] };
    "/v1/jobs/{id}/cancel": { post: operations["cancel_unified_job"] };
    "/v1/jobs/{id}/events": { get: operations["unified_job_events"] };
    "/v1/jobs/{id}/retry": { post: operations["retry_unified_job"] };
    "/v1/jobs/{id}/stream": { get: operations["unified_job_stream"] };
    "/v1/map": { post: operations["map"] };
    "/v1/memory": { post: operations["memory"] };
    "/v1/mobile/sessions": { get: operations["list_mobile_sessions"] };
    "/v1/mobile/sessions/{id}": { get: operations["get_mobile_session"]; put: operations["upsert_mobile_session"]; delete: operations["delete_mobile_session"] };
    "/v1/prune/exec": { post: operations["prune_exec"] };
    "/v1/prune/plan": { post: operations["prune_plan"] };
    "/v1/purge": { post: operations["purge"] };
    "/v1/query": { post: operations["query"] };
    "/v1/research": { post: operations["research"] };
    "/v1/research/stream": { post: operations["research_stream"] };
    "/v1/retrieve": { post: operations["retrieve"] };
    "/v1/screenshot": { post: operations["screenshot"] };
    "/v1/search": { post: operations["search"] };
    "/v1/sources": { get: operations["sources"]; post: operations["index_source"] };
    "/v1/stats": { get: operations["stats"] };
    "/v1/status": { get: operations["status"] };
    "/v1/suggest": { post: operations["suggest"] };
    "/v1/summarize": { post: operations["summarize"] };
    "/v1/summarize/stream": { post: operations["summarize_stream"] };
    "/v1/watch": { get: operations["list_watch"]; post: operations["create_watch"] };
    "/v1/watch/{id}/run": { post: operations["run_watch"] };
};

export type operations = {
    "healthz": { method: "get"; path: "/healthz"; operationId: "healthz"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: never; responses: { "200": string }; security: never };
    "readyz": { method: "get"; path: "/readyz"; operationId: "readyz"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: never; responses: { "200": components['schemas']['ReadinessBody']; "503": components['schemas']['ReadinessBody'] }; security: never };
    "serve_artifact_query": { method: "get"; path: "/v1/artifacts"; operationId: "serve_artifact_query"; parameters: { query: { "path": string }; path: Record<string, never> }; requestBody: never; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "404": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "v1_ask": { method: "post"; path: "/v1/ask"; operationId: "v1_ask"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestAskRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "413": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody']; "504": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "v1_ask_stream": { method: "post"; path: "/v1/ask/stream"; operationId: "v1_ask_stream"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestAskRequest']; responses: { "200": string; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "413": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "brand": { method: "post"; path: "/v1/brand"; operationId: "brand"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestBrandRequest']; responses: { "200": components['schemas']['BrandResult']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "v1_capabilities": { method: "get"; path: "/v1/capabilities"; operationId: "v1_capabilities"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: never; responses: { "200": components['schemas']['ServerInfo']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "v1_chat": { method: "post"; path: "/v1/chat"; operationId: "v1_chat"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestChatRequest']; responses: { "200": components['schemas']['RestChatResponse']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "413": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "v1_chat_stream": { method: "post"; path: "/v1/chat/stream"; operationId: "v1_chat_stream"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestChatRequest']; responses: { "200": string; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "413": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "collections_openapi_marker": { method: "get"; path: "/v1/collections"; operationId: "collections_openapi_marker"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: never; responses: { "200": components['schemas']['PanelCollectionsResponse']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "dedupe": { method: "post"; path: "/v1/dedupe"; operationId: "dedupe"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: null | components['schemas']['DedupeRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "415": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "diff": { method: "post"; path: "/v1/diff"; operationId: "diff"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestDiffRequest']; responses: { "200": components['schemas']['DiffResult']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "doctor": { method: "get"; path: "/v1/doctor"; operationId: "doctor"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "domains": { method: "get"; path: "/v1/domains"; operationId: "domains"; parameters: { query: { "limit"?: number | null; "offset"?: number | null; "domain"?: string | null; "cursor"?: string | null }; path: Record<string, never> }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "endpoints": { method: "post"; path: "/v1/endpoints"; operationId: "endpoints"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['EndpointsRequest']; responses: { "200": components['schemas']['EndpointReport']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "evaluate": { method: "post"; path: "/v1/evaluate"; operationId: "evaluate"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestEvaluateRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "list_extract_jobs": { method: "get"; path: "/v1/extract"; operationId: "list_extract_jobs"; parameters: { query: { "limit"?: number; "offset"?: number }; path: Record<string, never> }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "start_extract": { method: "post"; path: "/v1/extract"; operationId: "start_extract"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestExtractRequest']; responses: { "202": components['schemas']['AcceptedJob']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "clear_extract_jobs": { method: "delete"; path: "/v1/extract"; operationId: "clear_extract_jobs"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "cleanup_extract_jobs": { method: "post"; path: "/v1/extract/cleanup"; operationId: "cleanup_extract_jobs"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "recover_extract_jobs": { method: "post"; path: "/v1/extract/recover"; operationId: "recover_extract_jobs"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "extract_job_status": { method: "get"; path: "/v1/extract/{id}"; operationId: "extract_job_status"; parameters: { query: Record<string, never>; path: { "id": string } }; requestBody: never; responses: { "200": components['schemas']['JobStatusResponse']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "404": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "cancel_extract_job": { method: "post"; path: "/v1/extract/{id}/cancel"; operationId: "cancel_extract_job"; parameters: { query: Record<string, never>; path: { "id": string } }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "list_unified_jobs": { method: "get"; path: "/v1/jobs"; operationId: "list_unified_jobs"; parameters: { query: { "status"?: components['schemas']['LifecycleStatus']; "kind"?: components['schemas']['JobKind']; "limit"?: number; "cursor"?: string }; path: Record<string, never> }; requestBody: never; responses: { "200": components['schemas']['JobSummary']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "clear_unified_jobs": { method: "delete"; path: "/v1/jobs"; operationId: "clear_unified_jobs"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['JobClearRequest']; responses: { "200": components['schemas']['JobClearResult']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "cleanup_unified_jobs": { method: "post"; path: "/v1/jobs/cleanup"; operationId: "cleanup_unified_jobs"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['JobCleanupRequest']; responses: { "200": components['schemas']['JobCleanupResult']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "recover_unified_jobs": { method: "post"; path: "/v1/jobs/recover"; operationId: "recover_unified_jobs"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['JobRecoveryRequest']; responses: { "200": components['schemas']['JobRecoveryResult']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "unified_job_status": { method: "get"; path: "/v1/jobs/{id}"; operationId: "unified_job_status"; parameters: { query: Record<string, never>; path: { "id": string } }; requestBody: never; responses: { "200": components['schemas']['JobSummary']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "404": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "unified_job_artifacts": { method: "get"; path: "/v1/jobs/{id}/artifacts"; operationId: "unified_job_artifacts"; parameters: { query: Record<string, never>; path: { "id": string } }; requestBody: never; responses: { "200": components['schemas']['JobArtifactListResult']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "cancel_unified_job": { method: "post"; path: "/v1/jobs/{id}/cancel"; operationId: "cancel_unified_job"; parameters: { query: Record<string, never>; path: { "id": string } }; requestBody: components['schemas']['JobCancelRequest']; responses: { "200": components['schemas']['JobCancelResult']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "unified_job_events": { method: "get"; path: "/v1/jobs/{id}/events"; operationId: "unified_job_events"; parameters: { query: { "after_sequence"?: number; "since_sequence"?: number; "limit"?: number; "severity"?: components['schemas']['Severity']; "visibility"?: components['schemas']['Visibility']; "cursor"?: string }; path: { "id": string } }; requestBody: never; responses: { "200": components['schemas']['JobEventPage']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "retry_unified_job": { method: "post"; path: "/v1/jobs/{id}/retry"; operationId: "retry_unified_job"; parameters: { query: Record<string, never>; path: { "id": string } }; requestBody: components['schemas']['JobRetryRequest']; responses: { "200": components['schemas']['JobRetryResult']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "unified_job_stream": { method: "get"; path: "/v1/jobs/{id}/stream"; operationId: "unified_job_stream"; parameters: { query: Record<string, never>; path: { "id": string } }; requestBody: never; responses: { "200": components['schemas']['StreamEvent']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "map": { method: "post"; path: "/v1/map"; operationId: "map"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestMapRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "memory": { method: "post"; path: "/v1/memory"; operationId: "memory"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestMemoryRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "list_mobile_sessions": { method: "get"; path: "/v1/mobile/sessions"; operationId: "list_mobile_sessions"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: never; responses: { "200": components['schemas']['MobileSessionListResponse']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "500": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "get_mobile_session": { method: "get"; path: "/v1/mobile/sessions/{id}"; operationId: "get_mobile_session"; parameters: { query: Record<string, never>; path: { "id": string } }; requestBody: never; responses: { "200": components['schemas']['MobileSessionDetailResponse']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "404": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "upsert_mobile_session": { method: "put"; path: "/v1/mobile/sessions/{id}"; operationId: "upsert_mobile_session"; parameters: { query: Record<string, never>; path: { "id": string } }; requestBody: components['schemas']['UpsertMobileSessionRequest']; responses: { "200": components['schemas']['UpsertMobileSessionResponse']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "409": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "delete_mobile_session": { method: "delete"; path: "/v1/mobile/sessions/{id}"; operationId: "delete_mobile_session"; parameters: { query: Record<string, never>; path: { "id": string } }; requestBody: never; responses: { "200": components['schemas']['DeleteMobileSessionResponse']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "500": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "prune_exec": { method: "post"; path: "/v1/prune/exec"; operationId: "prune_exec"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['PruneExecRequest']; responses: { "200": components['schemas']['PruneResult']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "prune_plan": { method: "post"; path: "/v1/prune/plan"; operationId: "prune_plan"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['PrunePlanRequest']; responses: { "200": components['schemas']['PrunePlan']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "purge": { method: "post"; path: "/v1/purge"; operationId: "purge"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['PurgeRequest']; responses: { "200": components['schemas']['PurgeResult']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "query": { method: "post"; path: "/v1/query"; operationId: "query"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestQueryRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "research": { method: "post"; path: "/v1/research"; operationId: "research"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestResearchRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "504": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "research_stream": { method: "post"; path: "/v1/research/stream"; operationId: "research_stream"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestResearchRequest']; responses: { "200": string; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "retrieve": { method: "post"; path: "/v1/retrieve"; operationId: "retrieve"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestRetrieveRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "screenshot": { method: "post"; path: "/v1/screenshot"; operationId: "screenshot"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestScreenshotRequest']; responses: { "200": components['schemas']['ScreenshotResult']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "search": { method: "post"; path: "/v1/search"; operationId: "search"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestSearchRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "sources": { method: "get"; path: "/v1/sources"; operationId: "sources"; parameters: { query: { "limit"?: number | null; "offset"?: number | null; "domain"?: string | null; "cursor"?: string | null }; path: Record<string, never> }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "index_source": { method: "post"; path: "/v1/sources"; operationId: "index_source"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['SourceRequest']; responses: { "200": components['schemas']['SourceResult']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "stats": { method: "get"; path: "/v1/stats"; operationId: "stats"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "status": { method: "get"; path: "/v1/status"; operationId: "status"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "suggest": { method: "post"; path: "/v1/suggest"; operationId: "suggest"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestSuggestRequest']; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "429": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "summarize": { method: "post"; path: "/v1/summarize"; operationId: "summarize"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestSummarizeRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "summarize_stream": { method: "post"; path: "/v1/summarize/stream"; operationId: "summarize_stream"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestSummarizeRequest']; responses: { "200": string; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "list_watch": { method: "get"; path: "/v1/watch"; operationId: "list_watch"; parameters: { query: { "limit"?: number | null }; path: Record<string, never> }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "create_watch": { method: "post"; path: "/v1/watch"; operationId: "create_watch"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['WatchDefCreateRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "run_watch": { method: "post"; path: "/v1/watch/{id}/run"; operationId: "run_watch"; parameters: { query: Record<string, never>; path: { "id": string } }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "404": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
};
