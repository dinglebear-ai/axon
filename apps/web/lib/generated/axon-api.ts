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
        "DedupeRequest": {
            "collection"?: string | null;
        };
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
            "kind": string;
            "message": string;
        };
        "IngestSessionsPreparedRequest": {
            "collection"?: string | null;
            "docs": components['schemas']['PreparedSessionDoc'][];
            "project"?: string | null;
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
        "PreparedSessionDoc": {
            "extra"?: unknown;
            "session_date"?: string | null;
            "session_file": string;
            "session_platform": string;
            "session_project"?: string | null;
            "session_turn_count"?: number | null;
            "text": string;
            "title"?: string | null;
            "url": string;
        };
        "RenderMode": "http" | "chrome" | "auto-switch";
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
        "RestCrawlRequest": {
            "collection"?: string | null;
            "delay_ms"?: number | null;
            "discover_sitemaps"?: boolean | null;
            "headers"?: string[];
            "include_subdomains"?: boolean | null;
            "max_depth"?: number | null;
            "max_pages"?: number | null;
            "max_sitemaps"?: number | null;
            "render_mode"?: null | components['schemas']['RenderMode'];
            "respect_robots"?: boolean | null;
            "sitemap_since_days"?: number | null;
            "urls": string[];
        };
        "RestEmbedRequest": {
            "collection"?: string | null;
            "input": string;
            "source_type"?: string | null;
        };
        "RestEvaluateRequest": {
            "collection"?: string | null;
            "question": string;
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
        "RestIngestRequest": {
            "include_source"?: boolean | null;
            "sessions"?: null | components['schemas']['RestSessionsIngestOptions'];
            "source_type": components['schemas']['RestIngestSourceType'];
            "target"?: string | null;
        };
        "RestIngestSourceType": "github" | "gitlab" | "gitea" | "git" | "reddit" | "youtube" | "sessions";
        "RestMapRequest": {
            "limit"?: number | null;
            "offset"?: number | null;
            "url": string;
        };
        "RestQueryRequest": {
            "collection"?: string | null;
            "limit"?: number | null;
            "offset"?: number | null;
            "query": string;
        };
        "RestResearchRequest": {
            "limit"?: number | null;
            "offset"?: number | null;
            "query": string;
            "time_range"?: string | null;
        };
        "RestRetrieveRequest": {
            "collection"?: string | null;
            "cursor"?: string | null;
            "max_points"?: number | null;
            "token_budget"?: number | null;
            "url": string;
        };
        "RestScrapeRequest": {
            "collection"?: string | null;
            "embed"?: boolean | null;
            "exclude_selector"?: string | null;
            "format"?: null | components['schemas']['ScrapeFormat'];
            "headers"?: string[];
            "render_mode"?: null | components['schemas']['RenderMode'];
            "root_selector"?: string | null;
            "url"?: string | null;
            "urls"?: string[] | null;
        };
        "RestSearchRequest": {
            "limit"?: number | null;
            "offset"?: number | null;
            "query": string;
            "time_range"?: string | null;
        };
        "RestSessionsIngestOptions": {
            "claude"?: boolean | null;
            "codex"?: boolean | null;
            "gemini"?: boolean | null;
            "project"?: string | null;
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
        "ScrapeFormat": "markdown" | "html" | "rawHtml" | "json" | "llm";
        "WatchCreateRequest": {
            "enabled"?: boolean | null;
            "every_seconds": number;
            "name": string;
            "next_run_at"?: string | null;
            "task_payload": unknown;
            "task_type": string;
        };
    };
};
