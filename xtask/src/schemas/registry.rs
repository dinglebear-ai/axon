use std::path::Path;

use anyhow::{Result, bail};

use super::artifact::SchemaArtifact;

pub struct RemovedSurfaceRule {
    pub token: &'static str,
    pub path_contains: &'static [&'static str],
}

pub const REMOVED_SURFACE_RULES: &[RemovedSurfaceRule] = &[
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
];

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

mod canonical_enums;
pub use canonical_enums::CANONICAL_ENUMS;

pub fn check_removed_surface_drift(artifacts: &[SchemaArtifact]) -> Result<()> {
    for artifact in artifacts {
        let artifact_path = artifact.path.to_string_lossy();
        if artifact.path == Path::new("docs/reference/api/schemas.json") {
            check_removed_api_dto_shapes(artifact)?;
        }
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

fn check_removed_api_dto_shapes(artifact: &SchemaArtifact) -> Result<()> {
    let doc: serde_json::Value = serde_json::from_str(&artifact.content)?;
    let Some(defs) = doc.get("$defs").and_then(|value| value.as_object()) else {
        return Ok(());
    };

    for removed_def in axon_api::schema_registry::removed_dto_names() {
        if defs.contains_key(*removed_def) {
            bail!(
                "{} contains removed API DTO definition {removed_def}",
                artifact.path.display()
            );
        }
    }

    if let Some(purge) = defs.get("PurgeRequest") {
        reject_legacy_properties(artifact, "PurgeRequest", purge, &["target", "prefix"])?;
    }
    if let Some(dedupe) = defs.get("DedupeRequest") {
        reject_legacy_properties(artifact, "DedupeRequest", dedupe, &["target", "prefix"])?;
    }

    Ok(())
}

fn reject_legacy_properties(
    artifact: &SchemaArtifact,
    def_name: &str,
    schema: &serde_json::Value,
    legacy_properties: &[&str],
) -> Result<()> {
    let Some(properties) = schema.get("properties").and_then(|value| value.as_object()) else {
        return Ok(());
    };
    for property in legacy_properties {
        if properties.contains_key(*property) {
            bail!(
                "{} contains removed API DTO property {def_name}.{property}",
                artifact.path.display()
            );
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
        // Bidirectional: the generated enum must not carry values beyond the
        // canonical contract set either. A subset-only check lets a stray
        // non-canonical variant pass silently.
        for enum_value in enum_values {
            let Some(enum_value) = enum_value.as_str() else {
                continue;
            };
            if !values.contains(&enum_value) {
                bail!("api schema enum {name} has non-canonical value {enum_value}");
            }
        }
    }
    Ok(())
}
