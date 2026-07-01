use std::path::Path;

use anyhow::{Result, bail};

use super::artifact::SchemaArtifact;

pub struct RemovedSurfaceRule {
    pub token: &'static str,
    pub path_contains: &'static [&'static str],
}

pub const REMOVED_SURFACE_RULES: &[RemovedSurfaceRule] = &[
    global("\"EmbedRequest\""),
    global("\"IngestRequest\""),
    global("\"CrawlRequest\""),
    global("\"ScrapeRequest\""),
    global("\"PurgeRequest\""),
    global("\"CodeSearchRequest\""),
    cli("\"embed\""),
    cli("\"ingest\""),
    cli("\"scrape\""),
    cli("\"crawl\""),
    cli("\"code-search\""),
    cli("\"code-search-watch\""),
    cli("\"purge\""),
    cli("\"dedupe\""),
    cli("\"axon refresh\""),
    cli("\"fresh\""),
    mcp("\"embed\""),
    mcp("\"ingest\""),
    mcp("\"scrape\""),
    mcp("\"crawl\""),
    mcp("\"code_search\""),
    mcp("\"vertical_scrape\""),
    mcp("\"purge\""),
    mcp("\"dedupe\""),
    rest("\"/v1/embed\""),
    rest("\"/v1/ingest\""),
    rest("\"/v1/scrape\""),
    rest("\"/v1/crawl\""),
    rest("\"/v1/purge\""),
    rest("\"/v1/dedupe\""),
    rest("\"/v1/watch/{id}/run\""),
    config("\"AXON_MCP_HTTP_HOST\""),
    config("\"AXON_MCP_HTTP_PORT\""),
    config("\"AXON_MCP_HTTP_TOKEN\""),
    config("\"AXON_MCP_AUTH_MODE\""),
    config("\"AXON_MCP_PUBLIC_URL\""),
    config("\"AXON_MCP_GOOGLE_CLIENT_ID\""),
    config("\"AXON_MCP_GOOGLE_CLIENT_SECRET\""),
    config("\"AXON_MCP_AUTH_ADMIN_EMAIL\""),
    config("\"AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS\""),
    config("\"AXON_MCP_ALLOWED_ORIGINS\""),
    config("\"AXON_COLLECTION\""),
    config("\"AXON_HYBRID_CANDIDATES\""),
    config("\"AXON_ASK_HYBRID_CANDIDATES\""),
    config("\"AXON_INGEST_LANES\""),
    config("\"AXON_EMBED_DOC_TIMEOUT_SECS\""),
    config("\"AXON_WATCH_TICK_SECS\""),
    config("\"AXON_WATCH_LEASE_SECS\""),
    dto("\"input\""),
    dto("\"source_type\""),
    dto("\"target\""),
    dto("\"include_source\""),
    dto("\"urls\""),
    dto("\"url\""),
    dto("\"prefix\""),
    dto("\"cwd\""),
    dto("\"path_prefix\""),
    dto("\"no_freshness\""),
];

const fn global(token: &'static str) -> RemovedSurfaceRule {
    RemovedSurfaceRule {
        token,
        path_contains: &[],
    }
}

const fn cli(token: &'static str) -> RemovedSurfaceRule {
    RemovedSurfaceRule {
        token,
        path_contains: &["docs/reference/cli/"],
    }
}

const fn mcp(token: &'static str) -> RemovedSurfaceRule {
    RemovedSurfaceRule {
        token,
        path_contains: &["docs/reference/mcp/", "crates/axon-mcp/tests/golden/"],
    }
}

const fn rest(token: &'static str) -> RemovedSurfaceRule {
    RemovedSurfaceRule {
        token,
        path_contains: &["docs/reference/rest/"],
    }
}

const fn config(token: &'static str) -> RemovedSurfaceRule {
    RemovedSurfaceRule {
        token,
        path_contains: &["docs/reference/config/"],
    }
}

const fn dto(token: &'static str) -> RemovedSurfaceRule {
    RemovedSurfaceRule {
        token,
        path_contains: &["docs/reference/api/schemas.json"],
    }
}

pub const CANONICAL_ENUMS: &[(&str, &[&str])] = &[
    ("SourceIntent", &["acquire", "refresh", "watch", "map"]),
    (
        "SourceKind",
        &[
            "web", "local", "git", "registry", "feed", "reddit", "youtube", "session", "cli_tool",
            "mcp_tool", "memory", "upload",
        ],
    ),
    (
        "PipelinePhase",
        &[
            "queued",
            "requested",
            "resolving",
            "routing",
            "authorizing",
            "planning",
            "leasing",
            "discovering",
            "diffing",
            "fetching",
            "rendering",
            "enriching",
            "normalizing",
            "parsing",
            "graphing",
            "preparing",
            "batching",
            "embedding",
            "vectorizing",
            "upserting",
            "retrieving",
            "synthesizing",
            "evaluating",
            "publishing",
            "cleaning",
            "complete",
            "canceled",
        ],
    ),
    (
        "JobKind",
        &[
            "source",
            "watch",
            "map",
            "extract",
            "research",
            "ask",
            "query",
            "retrieve",
            "memory",
            "graph",
            "prune",
            "provider_probe",
            "reset",
        ],
    ),
    (
        "LifecycleStatus",
        &[
            "queued",
            "pending",
            "running",
            "waiting",
            "blocked",
            "canceling",
            "completed",
            "completed_degraded",
            "failed",
            "canceled",
            "expired",
            "skipped",
        ],
    ),
    (
        "PublishState",
        &[
            "planning",
            "writing",
            "publishing",
            "committed",
            "cleanup_pending",
            "cleaning",
            "cleaned",
        ],
    ),
];

pub fn check_removed_surface_drift(artifacts: &[SchemaArtifact]) -> Result<()> {
    for artifact in artifacts {
        let artifact_path = artifact.path.to_string_lossy();
        for rule in REMOVED_SURFACE_RULES {
            if rule_applies(&artifact.path, &artifact_path, rule)
                && artifact.content.contains(rule.token)
            {
                bail!(
                    "{} contains removed public surface token {}",
                    artifact.path.display(),
                    rule.token
                );
            }
        }
    }
    Ok(())
}

fn rule_applies(path: &Path, path_string: &str, rule: &RemovedSurfaceRule) -> bool {
    rule.path_contains.is_empty()
        || rule
            .path_contains
            .iter()
            .any(|needle| path_string.contains(needle) || path == Path::new(needle))
}

pub fn check_enum_projection_drift(artifacts: &[SchemaArtifact]) -> Result<()> {
    let Some(api) = artifacts
        .iter()
        .find(|artifact| artifact.path == std::path::Path::new("docs/reference/api/schemas.json"))
    else {
        return Ok(());
    };
    let doc: serde_json::Value = serde_json::from_str(&api.content)?;

    for (name, values) in CANONICAL_ENUMS {
        let enum_values = doc
            .pointer(&format!("/$defs/enums/{name}/enum"))
            .and_then(|value| value.as_array())
            .ok_or_else(|| anyhow::anyhow!("api schema is missing canonical enum {name}"))?;
        for value in *values {
            if !enum_values
                .iter()
                .any(|enum_value| enum_value.as_str() == Some(value))
            {
                bail!("api schema enum {name} is missing value {value}");
            }
        }
    }
    Ok(())
}
