use super::removed::{RemovedDtoField, RemovedOperation, RemovedRestRoute, RemovedSurfaceRegistry};

const CLI_COMMANDS: &[RemovedOperation] = &[
    op("embed", "axon <source>"),
    op("ingest", "axon <source>"),
    op("crawl", "axon <url> --scope site"),
    op(
        "code-search",
        "axon query <query> --content-kind code --freshness committed",
    ),
    op("code-search-watch", "axon watch <path>"),
    op("purge", "axon prune plan ... then axon prune exec ..."),
    op("dedupe", "axon prune plan ... then axon prune exec ..."),
    op("refresh", "axon <source>"),
    op("fresh", "axon watch ..."),
];

const MCP_ACTIONS: &[RemovedOperation] = &[
    op("embed", "source"),
    op("ingest", "source"),
    op("scrape", "source with scope=page"),
    op("crawl", "source with scope=site"),
    op(
        "code_search",
        "query with code filters and committed freshness",
    ),
    op("code_search_watch", "watch"),
    op("vertical_scrape", "adapter capabilities plus source"),
    op("purge", "prune"),
    op("dedupe", "prune"),
];

const REST_ROUTES: &[RemovedRestRoute] = &[
    route("POST", "/v1/embed", "embed", "POST /v1/sources"),
    route("POST", "/v1/ingest", "ingest", "POST /v1/sources"),
    route("POST", "/v1/scrape", "scrape", "POST /v1/sources"),
    route("POST", "/v1/crawl", "crawl", "POST /v1/sources"),
    route(
        "POST",
        "/v1/purge",
        "purge",
        "POST /v1/prune/plan then /v1/prune/exec",
    ),
    route(
        "POST",
        "/v1/dedupe",
        "dedupe",
        "POST /v1/prune/plan then /v1/prune/exec",
    ),
    route(
        "POST",
        "/v1/prune/purge",
        "prune_purge",
        "POST /v1/prune/plan then /v1/prune/exec",
    ),
    route(
        "POST",
        "/v1/prune/dedupe",
        "prune_dedupe",
        "POST /v1/prune/plan then /v1/prune/exec",
    ),
    route(
        "POST",
        "/v1/watch/{id}/run",
        "watch_run",
        "POST /v1/watches/{watch_id}/exec",
    ),
    route(
        "GET",
        "/v1/artifacts/{path}",
        "artifact_by_path",
        "GET /v1/artifacts/{artifact_id} or /v1/artifacts/{artifact_id}/content",
    ),
];

const CONFIG_KEYS: &[RemovedOperation] = &[
    op("AXON_MCP_HTTP_HOST", "AXON_HTTP_HOST"),
    op("AXON_MCP_HTTP_PORT", "AXON_HTTP_PORT"),
    op("AXON_MCP_HTTP_TOKEN", "AXON_HTTP_TOKEN"),
    op("AXON_MCP_AUTH_MODE", "AXON_AUTH_MODE"),
    op("AXON_MCP_PUBLIC_URL", "AXON_PUBLIC_URL"),
    op("AXON_MCP_GOOGLE_CLIENT_ID", "AXON_GOOGLE_CLIENT_ID"),
    op("AXON_MCP_GOOGLE_CLIENT_SECRET", "AXON_GOOGLE_CLIENT_SECRET"),
    op("AXON_MCP_AUTH_ADMIN_EMAIL", "AXON_AUTH_ADMIN_EMAIL"),
    op(
        "AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS",
        "AXON_ALLOWED_REDIRECT_URIS",
    ),
    op("AXON_MCP_ALLOWED_ORIGINS", "AXON_ALLOWED_ORIGINS"),
    op(
        "AXON_COLLECTION",
        "server.default_collection in config.toml",
    ),
    op(
        "AXON_HYBRID_CANDIDATES",
        "retrieval.hybrid_candidates in config.toml",
    ),
    op(
        "AXON_ASK_HYBRID_CANDIDATES",
        "ask.hybrid_candidates in config.toml",
    ),
    op("AXON_INGEST_LANES", "pipeline.ingest_lanes in config.toml"),
    op(
        "AXON_EMBED_DOC_TIMEOUT_SECS",
        "providers.embedding.doc_timeout_secs in config.toml",
    ),
    op("AXON_WATCH_TICK_SECS", "watch.tick_secs in config.toml"),
    op("AXON_WATCH_LEASE_SECS", "watch.lease_secs in config.toml"),
];

const DTO_FIELDS: &[RemovedDtoField] = &[
    dto("EmbedRequest", "input", "SourceRequest.source"),
    dto(
        "EmbedRequest",
        "source_type",
        "adapter-selected SourceKind / SourceScope",
    ),
    dto("IngestRequest", "target", "SourceRequest.source"),
    dto(
        "IngestRequest",
        "source_type",
        "adapter-selected SourceKind / SourceScope",
    ),
    dto(
        "IngestRequest",
        "include_source",
        "SourceRequest.options.include_source when supported",
    ),
    dto(
        "CrawlRequest",
        "urls",
        "SourceRequest.source plus multi-source submission",
    ),
    dto(
        "ScrapeRequest",
        "url",
        "SourceRequest.source with scope=page",
    ),
    dto("PurgeRequest", "target", "PruneSelector"),
    dto("PurgeRequest", "prefix", "PruneSelector scope/options"),
    dto(
        "CodeSearchRequest",
        "cwd",
        "QueryRequest.filters.source_id or local source filter",
    ),
    dto(
        "CodeSearchRequest",
        "path_prefix",
        "QueryRequest.filters.path_prefix",
    ),
    dto(
        "CodeSearchRequest",
        "no_freshness",
        "QueryRequest.freshness",
    ),
];

const GENERATED_CLIENTS: &[&str] = &["web", "palette", "android", "chrome-extension"];

const GENERATED_CLIENT_OPERATIONS: &[RemovedOperation] = &[
    op("embed", "create_source"),
    op("ingest", "create_source"),
    op("scrape", "create_source"),
    op("crawl", "create_source"),
    op("purge", "prune_plan then prune_exec"),
    op("dedupe", "prune_plan then prune_exec"),
    op("prune_purge", "prune_plan then prune_exec"),
    op("prune_dedupe", "prune_plan then prune_exec"),
    op("watch_run", "exec_watch"),
];

pub(super) fn removed_surface_registry() -> RemovedSurfaceRegistry {
    RemovedSurfaceRegistry {
        cli_commands: CLI_COMMANDS,
        mcp_actions: MCP_ACTIONS,
        rest_routes: REST_ROUTES,
        config_keys: CONFIG_KEYS,
        dto_fields: DTO_FIELDS,
        generated_clients: GENERATED_CLIENTS,
        generated_client_operations: GENERATED_CLIENT_OPERATIONS,
    }
}

const fn op(name: &'static str, replacement: &'static str) -> RemovedOperation {
    RemovedOperation { name, replacement }
}

const fn route(
    method: &'static str,
    path: &'static str,
    operation_id: &'static str,
    replacement: &'static str,
) -> RemovedRestRoute {
    RemovedRestRoute {
        method,
        path,
        replacement,
        operation_id,
    }
}

const fn dto(dto: &'static str, field: &'static str, replacement: &'static str) -> RemovedDtoField {
    RemovedDtoField {
        dto,
        field,
        replacement,
    }
}
