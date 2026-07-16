use std::collections::BTreeSet;

use anyhow::{Result, bail};

use super::artifact::SchemaArtifact;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemovedRestRoute {
    pub method: &'static str,
    pub path: &'static str,
    pub replacement: &'static str,
    pub operation_id: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemovedDtoField {
    pub dto: &'static str,
    pub field: &'static str,
    pub replacement: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemovedOperation {
    pub name: &'static str,
    pub replacement: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemovedSurfaceRegistry {
    pub cli_commands: &'static [RemovedOperation],
    pub mcp_actions: &'static [RemovedOperation],
    pub rest_routes: &'static [RemovedRestRoute],
    pub config_keys: &'static [RemovedOperation],
    pub dto_fields: &'static [RemovedDtoField],
    pub generated_clients: &'static [&'static str],
    pub generated_client_operations: &'static [RemovedOperation],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedSurfaceFinding {
    pub artifact_path: String,
    pub category: &'static str,
    pub surface: String,
    pub replacement: &'static str,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemovedSurfaceReport {
    pub findings: Vec<RemovedSurfaceFinding>,
}

impl RemovedSurfaceReport {
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    pub fn warning_messages(&self) -> Vec<String> {
        self.findings
            .iter()
            .map(|finding| {
                format!(
                    "{} exposes removed {} {}; replacement: {}",
                    finding.artifact_path, finding.category, finding.surface, finding.replacement
                )
            })
            .collect()
    }
}

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

pub fn removed_surface_registry() -> RemovedSurfaceRegistry {
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

pub fn removed_surface_absence_report(artifacts: &[SchemaArtifact]) -> RemovedSurfaceReport {
    let registry = removed_surface_registry();
    let mut report = RemovedSurfaceReport::default();

    for artifact in artifacts {
        if artifact.path.to_string_lossy().ends_with(".json") {
            match serde_json::from_str::<serde_json::Value>(&artifact.content) {
                Ok(json) => check_json_artifact(&registry, artifact, &json, &mut report),
                Err(_) => continue,
            }
        }
    }

    report
}

pub fn assert_removed_surface_absent(report: &RemovedSurfaceReport) -> Result<()> {
    if report.is_clean() {
        return Ok(());
    }
    bail!("{}", report.warning_messages().join("\n"))
}

fn check_json_artifact(
    registry: &RemovedSurfaceRegistry,
    artifact: &SchemaArtifact,
    json: &serde_json::Value,
    report: &mut RemovedSurfaceReport,
) {
    check_cli_commands(registry, artifact, json, report);
    check_mcp_actions(registry, artifact, json, report);
    check_rest_routes(registry, artifact, json, report);
    check_config_keys(registry, artifact, json, report);
    check_api_dto_fields(registry, artifact, json, report);
    check_generated_client_operations(registry, artifact, json, report);
}

fn check_cli_commands(
    registry: &RemovedSurfaceRegistry,
    artifact: &SchemaArtifact,
    json: &serde_json::Value,
    report: &mut RemovedSurfaceReport,
) {
    let Some(commands) = collect_string_field(json.get("commands"), "name") else {
        return;
    };
    for removed in registry.cli_commands {
        if commands.contains(removed.name) {
            push_finding(
                report,
                artifact,
                "CLI command",
                removed.name,
                removed.replacement,
            );
        }
    }
}

fn check_mcp_actions(
    registry: &RemovedSurfaceRegistry,
    artifact: &SchemaArtifact,
    json: &serde_json::Value,
    report: &mut RemovedSurfaceReport,
) {
    let Some(actions) = collect_string_field(json.get("actions"), "action") else {
        return;
    };
    for removed in registry.mcp_actions {
        if actions.contains(removed.name) {
            push_finding(
                report,
                artifact,
                "MCP action",
                removed.name,
                removed.replacement,
            );
        }
    }
}

fn check_rest_routes(
    registry: &RemovedSurfaceRegistry,
    artifact: &SchemaArtifact,
    json: &serde_json::Value,
    report: &mut RemovedSurfaceReport,
) {
    let Some(routes) = json.get("routes").and_then(|routes| routes.as_array()) else {
        return;
    };
    let operation_ids =
        collect_string_field(json.get("routes"), "operation_id").unwrap_or_else(|| BTreeSet::new());
    for removed in registry.rest_routes {
        let route_present = routes.iter().any(|route| {
            route.get("method").and_then(|value| value.as_str()) == Some(removed.method)
                && route.get("path").and_then(|value| value.as_str()) == Some(removed.path)
        });
        if route_present {
            push_finding(
                report,
                artifact,
                "REST route",
                &format!("{} {}", removed.method, removed.path),
                removed.replacement,
            );
        }
        if operation_ids.contains(removed.operation_id) {
            push_finding(
                report,
                artifact,
                "REST operation",
                removed.operation_id,
                removed.replacement,
            );
        }
    }
}

fn check_config_keys(
    registry: &RemovedSurfaceRegistry,
    artifact: &SchemaArtifact,
    json: &serde_json::Value,
    report: &mut RemovedSurfaceReport,
) {
    let Some(config_keys) = json.get("config_keys").and_then(|keys| keys.as_array()) else {
        return;
    };
    for key in config_keys {
        for removed in registry.config_keys {
            let env_present =
                key.get("env_key").and_then(|value| value.as_str()) == Some(removed.name);
            let key_present = key.get("key").and_then(|value| value.as_str()) == Some(removed.name);
            if env_present || key_present {
                push_finding(
                    report,
                    artifact,
                    "config key",
                    removed.name,
                    removed.replacement,
                );
            }
        }
    }
}

fn check_api_dto_fields(
    registry: &RemovedSurfaceRegistry,
    artifact: &SchemaArtifact,
    json: &serde_json::Value,
    report: &mut RemovedSurfaceReport,
) {
    let Some(defs) = json.get("$defs").and_then(|defs| defs.as_object()) else {
        return;
    };
    let mut removed_dtos = BTreeSet::new();
    for removed in registry.dto_fields {
        removed_dtos.insert(removed.dto);
    }

    for dto in removed_dtos {
        if defs.contains_key(dto) {
            push_finding(
                report,
                artifact,
                "DTO schema",
                dto,
                "SourceRequest, QueryRequest, or PruneSelector replacement DTO",
            );
        }
    }

    for removed in registry.dto_fields {
        let Some(properties) = defs
            .get(removed.dto)
            .and_then(|schema| schema.get("properties"))
            .and_then(|properties| properties.as_object())
        else {
            continue;
        };
        if properties.contains_key(removed.field) {
            push_finding(
                report,
                artifact,
                "DTO field",
                &format!("{}.{}", removed.dto, removed.field),
                removed.replacement,
            );
        }
    }
}

fn check_generated_client_operations(
    registry: &RemovedSurfaceRegistry,
    artifact: &SchemaArtifact,
    json: &serde_json::Value,
    report: &mut RemovedSurfaceReport,
) {
    if !is_generated_client_artifact(artifact, json) {
        return;
    }

    let mut operations = BTreeSet::new();
    collect_operation_names(json, &mut operations);
    for removed in registry.generated_client_operations {
        if operations.contains(removed.name) {
            push_finding(
                report,
                artifact,
                "generated client operation",
                removed.name,
                removed.replacement,
            );
        }
    }
}

fn collect_operation_names(value: &serde_json::Value, names: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                collect_operation_names(value, names);
            }
        }
        serde_json::Value::Object(map) => {
            for key in ["operation_id", "operationId", "operation", "method", "name"] {
                if let Some(name) = map.get(key).and_then(|value| value.as_str()) {
                    names.insert(name.to_owned());
                }
            }
            for key in ["operations", "methods", "endpoints"] {
                if let Some(value) = map.get(key) {
                    collect_operation_names(value, names);
                }
            }
        }
        _ => {}
    }
}

fn collect_string_field<'a>(
    value: Option<&'a serde_json::Value>,
    field: &str,
) -> Option<BTreeSet<&'a str>> {
    let values = value?.as_array()?;
    Some(
        values
            .iter()
            .filter_map(|item| item.get(field).and_then(|value| value.as_str()))
            .collect(),
    )
}

fn is_generated_client_artifact(artifact: &SchemaArtifact, json: &serde_json::Value) -> bool {
    let path = artifact.path.to_string_lossy();
    path.contains("generated")
        || path.contains("client")
        || path.contains("apps/web")
        || json.get("client").is_some()
        || json.get("operations").is_some()
}

fn push_finding(
    report: &mut RemovedSurfaceReport,
    artifact: &SchemaArtifact,
    category: &'static str,
    surface: &str,
    replacement: &'static str,
) {
    report.findings.push(RemovedSurfaceFinding {
        artifact_path: artifact.path.display().to_string(),
        category,
        surface: surface.to_owned(),
        replacement,
    });
}
