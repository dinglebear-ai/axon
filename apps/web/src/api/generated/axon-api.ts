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
        "ChunkHint": {
            "options": components['schemas']['MetadataMap'];
            "profile": components['schemas']['ChunkProfile'];
            "reason": string;
        };
        "ChunkId": string;
        "ChunkProfile": "code_symbol" | "code_manifest" | "markdown_sections" | "html_article" | "plain_text_windows" | "transcript_segments" | "structured_records" | "api_schema" | "tool_output" | "session_turns" | "atomic_metadata";
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
        "CredentialKind": "api_key" | "o_auth_token" | "bearer_token" | "basic_auth" | "cookie" | "ssh_key" | "local_config";
        "CredentialRequirement": {
            "credential_kind": components['schemas']['CredentialKind'];
            "reason": string;
            "required": boolean;
            "secret_ref"?: null | components['schemas']['SecretRef'];
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
        "DocumentBackend": "qdrant" | "stored_source" | "live_scrape";
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
        "ErrorStage": "parsing" | "validation" | "resolving" | "routing" | "authorizing" | "planning" | "leasing" | "discovering" | "diffing" | "fetching" | "rendering" | "enriching" | "normalizing" | "parsing_content" | "graphing" | "preparing" | "batching" | "embedding" | "vectorizing" | "upserting" | "publishing" | "cleaning" | "retrieving" | "synthesizing" | "evaluating" | "observing" | "storage" | "provider" | "transport" | "internal";
        "ErrorVisibility": "public" | "internal" | "sensitive";
        "ExecutionAffinity": "inline" | "worker" | "scheduler" | "provider_bound";
        "ExecutionMode": "foreground" | "background" | "wait";
        "ExecutionPolicy": {
            "detached": boolean;
            "heartbeat_interval_secs": number;
            "mode": components['schemas']['ExecutionMode'];
            "priority": components['schemas']['JobPriority'];
            "wait_timeout_secs"?: number | null;
        };
        "GraphDirection": "in" | "out" | "both";
        "GraphEdge": {
            "authority": components['schemas']['AuthorityLevel'];
            "confidence": number;
            "edge_id": components['schemas']['GraphEdgeId'];
            "evidence": components['schemas']['GraphEvidence'][];
            "from_node_id": components['schemas']['GraphNodeId'];
            "kind": string;
            "metadata": components['schemas']['MetadataMap'];
            "to_node_id": components['schemas']['GraphNodeId'];
        };
        "GraphEdgeId": string;
        "GraphEvidence": {
            "chunk_id"?: null | components['schemas']['ChunkId'];
            "confidence": number;
            "document_id"?: null | components['schemas']['DocumentId'];
            "evidence_id": string;
            "evidence_kind": string;
            "metadata": components['schemas']['MetadataMap'];
            "quote"?: string | null;
            "range"?: null | components['schemas']['SourceRange'];
            "source_id": components['schemas']['SourceId'];
            "source_item_key": components['schemas']['SourceItemKey'];
        };
        "GraphIdentifier": {
            "canonical_uri"?: string | null;
            "kind": string;
            "metadata": components['schemas']['MetadataMap'];
            "node_id"?: null | components['schemas']['GraphNodeId'];
            "source_id"?: null | components['schemas']['SourceId'];
            "source_item_key"?: null | components['schemas']['SourceItemKey'];
            "value"?: string | null;
        };
        "GraphKindDocument": {
            "authority_levels": components['schemas']['AuthorityLevel'][];
            "edge_kinds": string[];
            "evidence_kinds": string[];
            "node_kinds": string[];
        };
        "GraphNode": {
            "authority": components['schemas']['AuthorityLevel'];
            "canonical_uri": string;
            "confidence": number;
            "created_at"?: null | components['schemas']['Timestamp'];
            "display_name": string;
            "kind": string;
            "metadata": components['schemas']['MetadataMap'];
            "node_id": components['schemas']['GraphNodeId'];
            "source_ids"?: components['schemas']['SourceId'][];
            "updated_at"?: null | components['schemas']['Timestamp'];
        };
        "GraphNodeId": string;
        "GraphQueryFilters": {
            "metadata": components['schemas']['MetadataMap'];
            "min_confidence"?: number | null;
            "node_kinds"?: string[];
            "source_ids"?: components['schemas']['SourceId'][];
        };
        "GraphQueryRequest": {
            "cursor"?: string | null;
            "depth": number;
            "direction": components['schemas']['GraphDirection'];
            "edges"?: string[];
            "filters"?: null | components['schemas']['GraphQueryFilters'];
            "limit": number;
            "start": components['schemas']['GraphIdentifier'];
        };
        "GraphQueryResult": {
            "edges": components['schemas']['GraphEdge'][];
            "evidence": components['schemas']['GraphEvidence'][];
            "next_cursor"?: string | null;
            "nodes": components['schemas']['GraphNode'][];
            "warnings": components['schemas']['SourceWarning'][];
        };
        "GraphRef": {
            "candidate_id"?: string | null;
            "edge_id"?: null | components['schemas']['GraphEdgeId'];
            "node_id"?: null | components['schemas']['GraphNodeId'];
        };
        "GraphResolveMatch": {
            "confidence": number;
            "edges"?: components['schemas']['GraphEdge'][];
            "evidence": components['schemas']['GraphEvidence'][];
            "identifier": components['schemas']['GraphIdentifier'];
            "node": components['schemas']['GraphNode'];
        };
        "GraphResolveMiss": {
            "identifier": components['schemas']['GraphIdentifier'];
            "reason": string;
        };
        "GraphResolveRequest": {
            "identifiers": components['schemas']['GraphIdentifier'][];
            "include_edges"?: boolean;
        };
        "GraphResolveResult": {
            "misses": components['schemas']['GraphResolveMiss'][];
            "resolved": components['schemas']['GraphResolveMatch'][];
            "warnings": components['schemas']['SourceWarning'][];
        };
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
            "actor"?: string | null;
            "force_after_ms"?: number | null;
            "reason"?: string | null;
        };
        "JobCancelResult": {
            "canceled_at"?: null | components['schemas']['Timestamp'];
            "canceled_by"?: string | null;
            "cleanup_debt_ids"?: string[];
            "job_id": components['schemas']['JobId'];
            "last_safe_stage"?: null | components['schemas']['PipelinePhase'];
            "reason"?: string | null;
            "side_effects"?: string[];
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
        "JobKind": "source" | "watch" | "map" | "extract" | "research" | "ask" | "query" | "retrieve" | "memory" | "graph" | "prune" | "provider_probe" | "reset" | "embed" | "crawl" | "ingest";
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
        "MemoryDecayPolicy": {
            "expires_at"?: null | components['schemas']['Timestamp'];
            "half_life_days"?: number | null;
            "last_reinforced_at"?: null | components['schemas']['Timestamp'];
            "pinned"?: boolean;
            "profile": string;
            "reinforcement_count"?: number;
            "review_after"?: null | components['schemas']['Timestamp'];
        };
        "MemoryExportRequest": {
            "include_archived"?: boolean;
            "include_working"?: boolean;
            "scope"?: null | components['schemas']['MemoryScope'];
        };
        "MemoryExportResult": {
            "artifact"?: null | components['schemas']['ArtifactRef'];
            "count": number;
            "records": components['schemas']['MemoryRecord'][];
        };
        "MemoryHistoryEvent": {
            "message": string;
            "status": components['schemas']['MemoryStatus'];
            "timestamp": components['schemas']['Timestamp'];
        };
        "MemoryId": string;
        "MemoryImportMode": "merge" | "replace_scope";
        "MemoryImportRequest": {
            "dry_run"?: boolean;
            "mode": components['schemas']['MemoryImportMode'];
            "records": components['schemas']['MemoryRecord'][];
        };
        "MemoryImportResult": {
            "created": number;
            "created_ids"?: components['schemas']['MemoryId'][];
            "dry_run": boolean;
            "skipped": number;
            "updated": number;
            "warnings"?: components['schemas']['SourceWarning'][];
        };
        "MemoryLink": {
            "confidence": number;
            "evidence": components['schemas']['GraphEvidence'][];
            "link_type": string;
            "target": string;
        };
        "MemoryRecord": {
            "body": string;
            "confidence": number;
            "contradicts"?: null | components['schemas']['MemoryId'];
            "decay"?: null | components['schemas']['MemoryDecayPolicy'];
            "embedding_refs"?: components['schemas']['VectorPointId'][];
            "history": components['schemas']['MemoryHistoryEvent'][];
            "links"?: components['schemas']['MemoryLink'][];
            "memory_id": components['schemas']['MemoryId'];
            "memory_type": components['schemas']['MemoryType'];
            "salience": number;
            "scope": components['schemas']['MemoryScope'];
            "status": components['schemas']['MemoryStatus'];
            "superseded_by"?: null | components['schemas']['MemoryId'];
            "title"?: string | null;
            "visibility"?: components['schemas']['Visibility'];
        };
        "MemoryScope": {
            "kind": string;
            "value": string;
        };
        "MemoryStatus": "active" | "review" | "superseded" | "contradicted" | "archived" | "forgotten" | "working";
        "MemoryType": "decision" | "fact" | "preference" | "task" | "bug" | "procedure" | "incident" | "entity" | "episode" | "working";
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
            "draft"?: string | null;
            "first_message_preview": string;
            "id": string;
            "injected_op_count": number;
            "items"?: components['schemas']['MobileChatItem'][];
            "pinned_at"?: number | null;
            "source_refs"?: string[];
            "status"?: components['schemas']['MobileSessionStatus'];
            "sync_version"?: number;
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
        "MobileSessionStatus": "active" | "archived" | "deleted" | "sync_conflict";
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
        "PageInfo": {
            "limit": number;
            "next_cursor"?: string | null;
            "total"?: number | null;
        };
        "PanelCollectionsResponse": {
            "collections": string[];
        };
        "ParserHint": {
            "options": components['schemas']['MetadataMap'];
            "parser_id": string;
            "reason": string;
        };
        "PipelinePhase": "queued" | "requested" | "resolving" | "routing" | "authorizing" | "planning" | "leasing" | "discovering" | "diffing" | "fetching" | "rendering" | "enriching" | "normalizing" | "parsing" | "graphing" | "preparing" | "batching" | "embedding" | "vectorizing" | "upserting" | "retrieving" | "synthesizing" | "evaluating" | "publishing" | "cleaning" | "complete" | "canceled";
        "ProviderId": string;
        "ProviderKind": "llm" | "embedding" | "vector" | "search" | "fetch" | "render" | "network_capture" | "artifact" | "ledger" | "graph" | "memory" | "job" | "watch" | "config" | "credential" | "cache" | "security" | "rate_limiter" | "health_probe";
        "ProviderListResponse": {
            "providers": components['schemas']['ProviderSummary'][];
        };
        "ProviderRequirement": {
            "capability": string;
            "provider_kind": components['schemas']['ProviderKind'];
            "reason": string;
            "required": boolean;
        };
        "ProviderSummary": {
            "detail": unknown;
            "id": string;
            "ok": boolean;
        };
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
            "graph_edge_ids"?: components['schemas']['GraphEdgeId'][] | null;
            "graph_stable_keys"?: string[] | null;
            "memory_ids"?: components['schemas']['MemoryId'][] | null;
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
        "QueryHit": {
            "chunk_index"?: number | null;
            "chunking_method"?: string | null;
            "content_kind"?: string | null;
            "end_line"?: number | null;
            "file_path"?: string | null;
            "file_type"?: string | null;
            "kind"?: string | null;
            "language"?: string | null;
            "provider"?: string | null;
            "rank": number;
            "rerank_score": number;
            "score": number;
            "snippet": string;
            "source": string;
            "start_line"?: number | null;
            "symbol"?: string | null;
            "symbol_extraction_status"?: string | null;
            "url": string;
        };
        "QueryResult": {
            "results": components['schemas']['QueryHit'][];
        };
        "ReadinessBody": {
            "ok": boolean;
            "qdrant": string;
            "sqlite": string;
            "tei": string;
        };
        "RenderMode": "http" | "chrome" | "auto-switch";
        "ResolvedSource": {
            "adapter": components['schemas']['AdapterRef'];
            "authority": components['schemas']['AuthorityLevel'];
            "available_scopes": components['schemas']['SourceScope'][];
            "canonical_uri": string;
            "confidence": number;
            "default_scope": components['schemas']['SourceScope'];
            "graph"?: components['schemas']['GraphRef'][];
            "reason": string;
            "source": string;
            "source_kind": components['schemas']['SourceKind'];
            "warnings": components['schemas']['SourceWarning'][];
        };
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
        "RetrieveResult": {
            "backend"?: null | components['schemas']['DocumentBackend'];
            "chunk_count": number;
            "content": string;
            "matched_url"?: string | null;
            "next_cursor"?: string | null;
            "refresh_status"?: string | null;
            "remaining_tokens_estimate"?: number | null;
            "requested_url"?: string | null;
            "token_estimate"?: number | null;
            "truncated"?: boolean;
            "variant_errors"?: components['schemas']['ServiceRetrieveVariantError'][];
            "warnings"?: string[];
        };
        "RoutePlan": {
            "adapter": components['schemas']['AdapterRef'];
            "chunking_hints": components['schemas']['ChunkHint'][];
            "credential_requirements": components['schemas']['CredentialRequirement'][];
            "execution_affinity": components['schemas']['ExecutionAffinity'];
            "graph_fact_kinds": string[];
            "option_schema_id": string;
            "parser_hints": components['schemas']['ParserHint'][];
            "provider_requirements": components['schemas']['ProviderRequirement'][];
            "refresh_supported": boolean;
            "safety_class": components['schemas']['SafetyClass'];
            "scope": components['schemas']['SourceScope'];
            "source": components['schemas']['ResolvedSource'];
            "validated_options": components['schemas']['AdapterOptions'];
            "watch_supported": boolean;
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
        "SafetyClass": "public_network" | "authenticated_network" | "local_filesystem" | "tool_execution";
        "ScreenshotResult": {
            "artifact_handle"?: null | components['schemas']['ArtifactHandle'];
            "path": string;
            "size_bytes": number;
            "url": string;
        };
        "SecretRef": {
            "key": string;
            "label": string;
            "provider": string;
        };
        "ServerInfo": {
            "minimum_client_schema_version": string;
            "required_request_fields"?: string[];
            "schema_version": string;
            "supported_actions"?: string[];
            "supported_routes": string[];
            "version": string;
        };
        "ServiceRetrieveVariantError": {
            "error": string;
            "url": string;
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
        "SourceRange": {
            "byte_end"?: number | null;
            "byte_start"?: number | null;
            "char_end"?: number | null;
            "char_start"?: number | null;
            "csv_row"?: number | null;
            "dom_selector"?: string | null;
            "json_pointer"?: string | null;
            "line_end"?: number | null;
            "line_start"?: number | null;
            "session_turn_id"?: string | null;
            "time_end_ms"?: number | null;
            "time_start_ms"?: number | null;
            "turn_end"?: string | null;
            "turn_start"?: string | null;
            "xml_xpath"?: string | null;
            "yaml_path"?: string | null;
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
        "SourceScope": "page" | "site" | "docs" | "repo" | "workspace" | "branch" | "org" | "package" | "version" | "feed" | "subreddit" | "thread" | "comment" | "video" | "playlist" | "channel" | "issue" | "pull_request" | "merge_request" | "release" | "wiki" | "file" | "directory" | "map" | "tool" | "script" | "api";
        "SourceSummary": {
            "adapter": components['schemas']['AdapterRef'];
            "authority": components['schemas']['AuthorityLevel'];
            "canonical_uri": string;
            "counts": components['schemas']['SourceCounts'];
            "created_at": components['schemas']['Timestamp'];
            "display_name": string;
            "graph_node_ids"?: components['schemas']['GraphNodeId'][];
            "last_job_id"?: null | components['schemas']['JobId'];
            "last_refreshed_at"?: null | components['schemas']['Timestamp'];
            "source_id": components['schemas']['SourceId'];
            "source_kind": components['schemas']['SourceKind'];
            "status": components['schemas']['LifecycleStatus'];
            "tags"?: string[];
            "updated_at": components['schemas']['Timestamp'];
            "user_label"?: string | null;
            "watch_id"?: null | components['schemas']['WatchId'];
        };
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
            "data"?: unknown;
            "error"?: null | components['schemas']['ApiError'];
            "event_id": string;
            "job_id"?: null | components['schemas']['JobId'];
            "kind": components['schemas']['StreamKind'];
            "request_id"?: string | null;
            "sequence": number;
            "timestamp": components['schemas']['Timestamp'];
            "warning"?: null | components['schemas']['SourceWarning'];
        };
        "StreamKind": "progress" | "token" | "citation" | "artifact" | "warning" | "error" | "final";
        "SuccessEnvelope_QueryResult": {
            "artifacts": components['schemas']['ArtifactRef'][];
            "contract_version": string;
            "data": {
            "results": components['schemas']['QueryHit'][];
        };
            "job"?: null | components['schemas']['JobDescriptor'];
            "ok": boolean;
            "pagination"?: null | components['schemas']['PageInfo'];
            "request_id": string;
            "trace": components['schemas']['TraceContext'];
            "warnings": components['schemas']['SourceWarning'][];
        };
        "SuccessEnvelope_RetrieveResult": {
            "artifacts": components['schemas']['ArtifactRef'][];
            "contract_version": string;
            "data": {
            "backend"?: null | components['schemas']['DocumentBackend'];
            "chunk_count": number;
            "content": string;
            "matched_url"?: string | null;
            "next_cursor"?: string | null;
            "refresh_status"?: string | null;
            "remaining_tokens_estimate"?: number | null;
            "requested_url"?: string | null;
            "token_estimate"?: number | null;
            "truncated"?: boolean;
            "variant_errors"?: components['schemas']['ServiceRetrieveVariantError'][];
            "warnings"?: string[];
        };
            "job"?: null | components['schemas']['JobDescriptor'];
            "ok": boolean;
            "pagination"?: null | components['schemas']['PageInfo'];
            "request_id": string;
            "trace": components['schemas']['TraceContext'];
            "warnings": components['schemas']['SourceWarning'][];
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
        "WatchRequest": {
            "collection"?: string | null;
            "embed": boolean;
            "enabled"?: boolean | null;
            "options": components['schemas']['AdapterOptions'];
            "schedule": components['schemas']['WatchSchedule'];
            "scope"?: null | components['schemas']['SourceScope'];
            "source": string;
        };
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
        "WatchUpdateRequest": {
            "collection"?: string | null;
            "embed"?: boolean | null;
            "enabled"?: boolean | null;
            "options"?: null | components['schemas']['AdapterOptions'];
            "schedule"?: null | components['schemas']['WatchSchedule'];
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
    "/v1/graph/edges/{edge_id}": { get: operations["get_edge"] };
    "/v1/graph/kinds": { get: operations["kinds"] };
    "/v1/graph/nodes/{node_id}": { get: operations["get_node"] };
    "/v1/graph/nodes/{node_id}/edges": { get: operations["get_node_edges"] };
    "/v1/graph/query": { post: operations["graph_query"] };
    "/v1/graph/resolve": { post: operations["resolve"] };
    "/v1/graph/sources/{source_id}": { get: operations["get_source_subgraph"] };
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
    "/v1/memories": { post: operations["remember_memory"] };
    "/v1/memories/compact": { post: operations["compact_memories"] };
    "/v1/memories/context": { post: operations["memory_context"] };
    "/v1/memories/export": { post: operations["export_memories"] };
    "/v1/memories/import": { post: operations["import_memories"] };
    "/v1/memories/review": { post: operations["review_memories"] };
    "/v1/memories/search": { post: operations["search_memories"] };
    "/v1/memories/{memory_id}": { get: operations["show_memory"]; delete: operations["forget_memory"] };
    "/v1/memories/{memory_id}/archive": { post: operations["archive_memory"] };
    "/v1/memories/{memory_id}/compact": { post: operations["compact_one_memory"] };
    "/v1/memories/{memory_id}/contradict": { post: operations["contradict_memory"] };
    "/v1/memories/{memory_id}/link": { post: operations["link_memory"] };
    "/v1/memories/{memory_id}/pin": { post: operations["pin_memory"] };
    "/v1/memories/{memory_id}/reinforce": { post: operations["reinforce_memory"] };
    "/v1/memories/{memory_id}/supersede": { post: operations["supersede_memory"] };
    "/v1/memory": { post: operations["memory"] };
    "/v1/mobile/sessions": { get: operations["list_mobile_sessions"] };
    "/v1/mobile/sessions/{id}": { get: operations["get_mobile_session"]; put: operations["upsert_mobile_session"]; delete: operations["delete_mobile_session"] };
    "/v1/providers": { get: operations["list_providers"] };
    "/v1/providers/{provider}": { get: operations["get_provider"] };
    "/v1/prune/dedupe": { post: operations["dedupe"] };
    "/v1/prune/exec": { post: operations["prune_exec"] };
    "/v1/prune/plan": { post: operations["prune_plan"] };
    "/v1/prune/purge": { post: operations["purge"] };
    "/v1/query": { post: operations["query"] };
    "/v1/research": { post: operations["research"] };
    "/v1/research/stream": { post: operations["research_stream"] };
    "/v1/resolve": { post: operations["resolve_source"] };
    "/v1/retrieve": { post: operations["retrieve"] };
    "/v1/screenshot": { post: operations["screenshot"] };
    "/v1/search": { post: operations["search"] };
    "/v1/sources": { get: operations["sources"]; post: operations["index_source"] };
    "/v1/sources/{source_id}": { get: operations["get_source"] };
    "/v1/stats": { get: operations["stats"] };
    "/v1/status": { get: operations["status"] };
    "/v1/suggest": { post: operations["suggest"] };
    "/v1/summarize": { post: operations["summarize"] };
    "/v1/summarize/stream": { post: operations["summarize_stream"] };
    "/v1/watch": { get: operations["list_watch"]; post: operations["create_watch"] };
    "/v1/watch/{id}/run": { post: operations["run_watch"] };
    "/v1/watches": { get: operations["list_watches"]; post: operations["create_watch"] };
    "/v1/watches/{watch_id}": { get: operations["get_watch"]; patch: operations["update_watch"]; delete: operations["delete_watch"] };
    "/v1/watches/{watch_id}/pause": { post: operations["pause_watch"] };
    "/v1/watches/{watch_id}/resume": { post: operations["resume_watch"] };
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
    "get_edge": { method: "get"; path: "/v1/graph/edges/{edge_id}"; operationId: "get_edge"; parameters: { query: Record<string, never>; path: { "edge_id": string } }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "404": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "kinds": { method: "get"; path: "/v1/graph/kinds"; operationId: "kinds"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: never; responses: { "200": components['schemas']['GraphKindDocument']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "get_node": { method: "get"; path: "/v1/graph/nodes/{node_id}"; operationId: "get_node"; parameters: { query: { "include_edges"?: boolean }; path: { "node_id": string } }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "404": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "get_node_edges": { method: "get"; path: "/v1/graph/nodes/{node_id}/edges"; operationId: "get_node_edges"; parameters: { query: Record<string, never>; path: { "node_id": string } }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "404": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "graph_query": { method: "post"; path: "/v1/graph/query"; operationId: "graph_query"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['GraphQueryRequest']; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "resolve": { method: "post"; path: "/v1/graph/resolve"; operationId: "resolve"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['GraphResolveRequest']; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "get_source_subgraph": { method: "get"; path: "/v1/graph/sources/{source_id}"; operationId: "get_source_subgraph"; parameters: { query: { "depth"?: number | null; "edge_kind"?: string | null; "limit"?: number | null }; path: { "source_id": string } }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
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
    "remember_memory": { method: "post"; path: "/v1/memories"; operationId: "remember_memory"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestMemoryRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "compact_memories": { method: "post"; path: "/v1/memories/compact"; operationId: "compact_memories"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestMemoryRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "memory_context": { method: "post"; path: "/v1/memories/context"; operationId: "memory_context"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestMemoryRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "export_memories": { method: "post"; path: "/v1/memories/export"; operationId: "export_memories"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['MemoryExportRequest']; responses: { "200": components['schemas']['MemoryExportResult']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "413": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "import_memories": { method: "post"; path: "/v1/memories/import"; operationId: "import_memories"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['MemoryImportRequest']; responses: { "200": components['schemas']['MemoryImportResult']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "413": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "review_memories": { method: "post"; path: "/v1/memories/review"; operationId: "review_memories"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestMemoryRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "search_memories": { method: "post"; path: "/v1/memories/search"; operationId: "search_memories"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestMemoryRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "show_memory": { method: "get"; path: "/v1/memories/{memory_id}"; operationId: "show_memory"; parameters: { query: Record<string, never>; path: { "memory_id": string } }; requestBody: never; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "forget_memory": { method: "delete"; path: "/v1/memories/{memory_id}"; operationId: "forget_memory"; parameters: { query: Record<string, never>; path: { "memory_id": string } }; requestBody: never; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "archive_memory": { method: "post"; path: "/v1/memories/{memory_id}/archive"; operationId: "archive_memory"; parameters: { query: Record<string, never>; path: { "memory_id": string } }; requestBody: components['schemas']['RestMemoryRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "compact_one_memory": { method: "post"; path: "/v1/memories/{memory_id}/compact"; operationId: "compact_one_memory"; parameters: { query: Record<string, never>; path: { "memory_id": string } }; requestBody: components['schemas']['RestMemoryRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "contradict_memory": { method: "post"; path: "/v1/memories/{memory_id}/contradict"; operationId: "contradict_memory"; parameters: { query: Record<string, never>; path: { "memory_id": string } }; requestBody: components['schemas']['RestMemoryRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "link_memory": { method: "post"; path: "/v1/memories/{memory_id}/link"; operationId: "link_memory"; parameters: { query: Record<string, never>; path: { "memory_id": string } }; requestBody: components['schemas']['RestMemoryRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "pin_memory": { method: "post"; path: "/v1/memories/{memory_id}/pin"; operationId: "pin_memory"; parameters: { query: Record<string, never>; path: { "memory_id": string } }; requestBody: components['schemas']['RestMemoryRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "reinforce_memory": { method: "post"; path: "/v1/memories/{memory_id}/reinforce"; operationId: "reinforce_memory"; parameters: { query: Record<string, never>; path: { "memory_id": string } }; requestBody: components['schemas']['RestMemoryRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "supersede_memory": { method: "post"; path: "/v1/memories/{memory_id}/supersede"; operationId: "supersede_memory"; parameters: { query: Record<string, never>; path: { "memory_id": string } }; requestBody: components['schemas']['RestMemoryRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "memory": { method: "post"; path: "/v1/memory"; operationId: "memory"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestMemoryRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "list_mobile_sessions": { method: "get"; path: "/v1/mobile/sessions"; operationId: "list_mobile_sessions"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: never; responses: { "200": components['schemas']['MobileSessionListResponse']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "500": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "get_mobile_session": { method: "get"; path: "/v1/mobile/sessions/{id}"; operationId: "get_mobile_session"; parameters: { query: Record<string, never>; path: { "id": string } }; requestBody: never; responses: { "200": components['schemas']['MobileSessionDetailResponse']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "404": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "upsert_mobile_session": { method: "put"; path: "/v1/mobile/sessions/{id}"; operationId: "upsert_mobile_session"; parameters: { query: Record<string, never>; path: { "id": string } }; requestBody: components['schemas']['UpsertMobileSessionRequest']; responses: { "200": components['schemas']['UpsertMobileSessionResponse']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "409": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "delete_mobile_session": { method: "delete"; path: "/v1/mobile/sessions/{id}"; operationId: "delete_mobile_session"; parameters: { query: Record<string, never>; path: { "id": string } }; requestBody: never; responses: { "200": components['schemas']['DeleteMobileSessionResponse']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "500": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "list_providers": { method: "get"; path: "/v1/providers"; operationId: "list_providers"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: never; responses: { "200": components['schemas']['ProviderListResponse']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "get_provider": { method: "get"; path: "/v1/providers/{provider}"; operationId: "get_provider"; parameters: { query: Record<string, never>; path: { "provider": string } }; requestBody: never; responses: { "200": components['schemas']['ProviderSummary']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "404": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "dedupe": { method: "post"; path: "/v1/prune/dedupe"; operationId: "dedupe"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: null | components['schemas']['DedupeRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "415": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "prune_exec": { method: "post"; path: "/v1/prune/exec"; operationId: "prune_exec"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['PruneExecRequest']; responses: { "200": components['schemas']['PruneResult']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "prune_plan": { method: "post"; path: "/v1/prune/plan"; operationId: "prune_plan"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['PrunePlanRequest']; responses: { "200": components['schemas']['PrunePlan']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "purge": { method: "post"; path: "/v1/prune/purge"; operationId: "purge"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['PurgeRequest']; responses: { "200": components['schemas']['PurgeResult']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "query": { method: "post"; path: "/v1/query"; operationId: "query"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestQueryRequest']; responses: { "200": components['schemas']['SuccessEnvelope_QueryResult']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "research": { method: "post"; path: "/v1/research"; operationId: "research"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestResearchRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "504": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "research_stream": { method: "post"; path: "/v1/research/stream"; operationId: "research_stream"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestResearchRequest']; responses: { "200": string; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "resolve_source": { method: "post"; path: "/v1/resolve"; operationId: "resolve_source"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['SourceRequest']; responses: { "200": components['schemas']['RoutePlan']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "422": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "retrieve": { method: "post"; path: "/v1/retrieve"; operationId: "retrieve"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestRetrieveRequest']; responses: { "200": components['schemas']['SuccessEnvelope_RetrieveResult']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "screenshot": { method: "post"; path: "/v1/screenshot"; operationId: "screenshot"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestScreenshotRequest']; responses: { "200": components['schemas']['ScreenshotResult']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "search": { method: "post"; path: "/v1/search"; operationId: "search"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestSearchRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "sources": { method: "get"; path: "/v1/sources"; operationId: "sources"; parameters: { query: { "limit"?: number | null; "offset"?: number | null; "domain"?: string | null; "cursor"?: string | null }; path: Record<string, never> }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "index_source": { method: "post"; path: "/v1/sources"; operationId: "index_source"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['SourceRequest']; responses: { "200": components['schemas']['SourceResult']; "202": components['schemas']['SourceResult']; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "get_source": { method: "get"; path: "/v1/sources/{source_id}"; operationId: "get_source"; parameters: { query: Record<string, never>; path: { "source_id": string } }; requestBody: never; responses: { "200": components['schemas']['SourceSummary']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "404": components['schemas']['ErrorBody']; "503": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "stats": { method: "get"; path: "/v1/stats"; operationId: "stats"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "status": { method: "get"; path: "/v1/status"; operationId: "status"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "suggest": { method: "post"; path: "/v1/suggest"; operationId: "suggest"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestSuggestRequest']; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "429": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "summarize": { method: "post"; path: "/v1/summarize"; operationId: "summarize"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestSummarizeRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "summarize_stream": { method: "post"; path: "/v1/summarize/stream"; operationId: "summarize_stream"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['RestSummarizeRequest']; responses: { "200": string; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "list_watch": { method: "get"; path: "/v1/watch"; operationId: "list_watch"; parameters: { query: { "limit"?: number | null }; path: Record<string, never> }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "create_watch": { method: "post"; path: "/v1/watch"; operationId: "create_watch"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['WatchDefCreateRequest']; responses: { "200": unknown; "400": components['schemas']['ErrorBody']; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "run_watch": { method: "post"; path: "/v1/watch/{id}/run"; operationId: "run_watch"; parameters: { query: Record<string, never>; path: { "id": string } }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "404": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "list_watches": { method: "get"; path: "/v1/watches"; operationId: "list_watches"; parameters: { query: { "enabled"?: boolean | null; "source_id"?: string | null; "adapter"?: string | null; "limit"?: number | null; "cursor"?: string | null }; path: Record<string, never> }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "create_watch": { method: "post"; path: "/v1/watches"; operationId: "create_watch"; parameters: { query: Record<string, never>; path: Record<string, never> }; requestBody: components['schemas']['WatchRequest']; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "get_watch": { method: "get"; path: "/v1/watches/{watch_id}"; operationId: "get_watch"; parameters: { query: Record<string, never>; path: { "watch_id": string } }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "404": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "update_watch": { method: "patch"; path: "/v1/watches/{watch_id}"; operationId: "update_watch"; parameters: { query: Record<string, never>; path: { "watch_id": string } }; requestBody: components['schemas']['WatchUpdateRequest']; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "404": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "delete_watch": { method: "delete"; path: "/v1/watches/{watch_id}"; operationId: "delete_watch"; parameters: { query: Record<string, never>; path: { "watch_id": string } }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "404": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "pause_watch": { method: "post"; path: "/v1/watches/{watch_id}/pause"; operationId: "pause_watch"; parameters: { query: Record<string, never>; path: { "watch_id": string } }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "404": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
    "resume_watch": { method: "post"; path: "/v1/watches/{watch_id}/resume"; operationId: "resume_watch"; parameters: { query: Record<string, never>; path: { "watch_id": string } }; requestBody: never; responses: { "200": unknown; "401": components['schemas']['ErrorBody']; "403": components['schemas']['ErrorBody']; "404": components['schemas']['ErrorBody']; "502": components['schemas']['ErrorBody'] }; security: "bearerAuth" | "oauth2" };
};
