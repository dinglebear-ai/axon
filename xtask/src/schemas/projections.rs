use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use axon_api::source::{
    CodeSearchRequest, CrawlRequest, EmbedRequest, IngestRequest, PROJECTION_CONTRACT_VERSION,
    PROJECTION_OPERATIONS, ProjectionOperation, ScrapeRequest, project_code_search, project_crawl,
    project_embed, project_ingest, project_scrape,
};
use schemars::{JsonSchema, generate::SchemaSettings};
use serde_json::{Value, json};

use crate::schemas::artifact::SchemaArtifact;
use crate::schemas::schema_json::json_string;
use crate::schemas::source_input::source_inputs_with_rust_module_closure;

use super::rel;

#[cfg(test)]
#[path = "projections_tests.rs"]
mod tests;

pub(super) fn projection_artifacts(root: &Path) -> Result<Vec<SchemaArtifact>> {
    let contract = generate_projection_contract(root)?;
    Ok(vec![
        SchemaArtifact::new(
            rel("docs/reference/sources/projections.json"),
            json_string(&contract)?,
        ),
        SchemaArtifact::new(
            rel("docs/reference/sources/projections.md"),
            projection_markdown(&contract)?,
        ),
    ])
}

fn generate_projection_contract(root: &Path) -> Result<Value> {
    let inputs = source_inputs_with_rust_module_closure(
        root,
        &["tests/fixtures/source-projections"],
        &[
            "crates/axon-api/src/source/projection.rs",
            "crates/axon-api/src/source/projection_registry.rs",
            "xtask/src/schemas/projections.rs",
        ],
    )?;
    let fixtures = load_fixtures(root)?;
    validate_fixture_coverage(&fixtures)?;

    let operations = PROJECTION_OPERATIONS
        .iter()
        .map(|spec| {
            json!({
                "operation": spec.operation,
                "cli_name": spec.cli_name,
                "mcp_name": spec.mcp_name,
                "rest_path": spec.rest_path,
                "auth_scope": spec.auth_scope,
                "mutating": spec.mutating,
                "supports_batch": spec.supports_batch,
                "supports_idempotency": spec.supports_idempotency,
                "request_schema": spec.request_schema,
                "result_schema": spec.result_schema,
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://axon.local/schemas/sources/projections.schema.json",
        "title": "Axon Restored Source Projections",
        "type": "object",
        "required": ["contract_version", "operations", "fixtures"],
        "properties": {
            "contract_version": {"type": "string"},
            "operations": {"type": "array", "minItems": 5},
            "fixtures": {"type": "array", "minItems": 10}
        },
        "contract_version": PROJECTION_CONTRACT_VERSION,
        "generated_by": "cargo xtask schemas projections",
        "source_inputs": inputs,
        "operations": operations,
        "request_schemas": request_schemas()?,
        "fixtures": fixtures,
    }))
}

fn request_schemas() -> Result<BTreeMap<&'static str, Value>> {
    Ok(BTreeMap::from([
        ("CodeSearchRequest", inline_schema::<CodeSearchRequest>()?),
        ("CrawlRequest", inline_schema::<CrawlRequest>()?),
        ("EmbedRequest", inline_schema::<EmbedRequest>()?),
        ("IngestRequest", inline_schema::<IngestRequest>()?),
        ("ScrapeRequest", inline_schema::<ScrapeRequest>()?),
    ]))
}

fn inline_schema<T: JsonSchema>() -> Result<Value> {
    let settings = SchemaSettings::draft2020_12().with(|settings| {
        settings.meta_schema = None;
        settings.inline_subschemas = true;
    });
    Ok(serde_json::to_value(
        settings.into_generator().into_root_schema_for::<T>(),
    )?)
}

fn load_fixtures(root: &Path) -> Result<Vec<Value>> {
    let directory = root.join("tests/fixtures/source-projections");
    let mut paths = std::fs::read_dir(&directory)
        .with_context(|| format!("failed to read {}", directory.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|path| {
        path.extension()
            .is_some_and(|extension| extension == "json")
    });
    paths.sort();

    paths
        .into_iter()
        .map(|path| {
            let bytes = std::fs::read(&path)?;
            let mut fixture: Value = serde_json::from_slice(&bytes)
                .with_context(|| format!("invalid projection fixture {}", path.display()))?;
            normalize_fixture(&mut fixture)
                .with_context(|| format!("projection fixture {}", path.display()))?;
            Ok(fixture)
        })
        .collect()
}

fn normalize_fixture(fixture: &mut Value) -> Result<()> {
    let operation: ProjectionOperation = serde_json::from_value(
        fixture
            .get("operation")
            .cloned()
            .context("missing operation")?,
    )?;
    let transport_input = fixture
        .get("transport_input")
        .cloned()
        .context("missing transport_input")?;
    fixture
        .get("canonical_requests")
        .context("missing canonical_requests")?;
    fixture
        .get("expected_result")
        .context("missing expected_result")?;

    let actual = match operation {
        ProjectionOperation::Scrape => {
            serde_json::to_value(project_scrape(&serde_json::from_value(transport_input)?)?)?
        }
        ProjectionOperation::Crawl => {
            serde_json::to_value(project_crawl(&serde_json::from_value(transport_input)?)?)?
        }
        ProjectionOperation::Embed => {
            serde_json::to_value(project_embed(&serde_json::from_value(transport_input)?)?)?
        }
        ProjectionOperation::Ingest => {
            serde_json::to_value(project_ingest(&serde_json::from_value(transport_input)?)?)?
        }
        ProjectionOperation::CodeSearch => serde_json::to_value(project_code_search(
            &serde_json::from_value(transport_input)?,
        )?)?,
    };
    let expected = &fixture["canonical_requests"];
    if expected == "generated" {
        fixture["canonical_requests"] = actual;
    } else if &actual != expected {
        bail!("canonical_requests do not match the declared projection");
    }
    Ok(())
}

fn validate_fixture_coverage(fixtures: &[Value]) -> Result<()> {
    for operation in PROJECTION_OPERATIONS {
        let count = fixtures
            .iter()
            .filter(|fixture| fixture["operation"] == json!(operation.operation))
            .count();
        if count < 2 {
            bail!(
                "projection {} requires minimal and boundary fixtures",
                operation.cli_name
            );
        }
    }
    Ok(())
}

fn projection_markdown(contract: &Value) -> Result<String> {
    let operations = contract["operations"]
        .as_array()
        .context("projection operations must be an array")?;
    let mut output = format!(
        "# Restored Source Projection Contract\n\nContract version: `{}`. Generated by `cargo xtask schemas projections`; do not edit manually.\n\n| Operation | CLI | MCP | REST | Scope | Mutating | Idempotency |\n|---|---|---|---|---|---:|---:|\n",
        contract["contract_version"].as_str().unwrap_or_default()
    );
    for operation in operations {
        output.push_str(&format!(
            "| {} | `{}` | `{}` | `{}` | `{}` | {} | {} |\n",
            operation["operation"].as_str().unwrap_or_default(),
            operation["cli_name"].as_str().unwrap_or_default(),
            operation["mcp_name"].as_str().unwrap_or_default(),
            operation["rest_path"].as_str().unwrap_or_default(),
            operation["auth_scope"].as_str().unwrap_or_default(),
            operation["mutating"].as_bool().unwrap_or_default(),
            operation["supports_idempotency"]
                .as_bool()
                .unwrap_or_default(),
        ));
    }
    output.push_str(&format!(
        "\n## Request shape\n\nAll five surfaces accept an ordered `inputs` array and one shared `options` object. Source items use `{{\"input\": \"...\", \"idempotency_key\": \"...\"}}`; code-search items use `{{\"input\": \"...\"}}`. The CLI accepts positional inputs, repeatable `--item` JSON, or `--request-file` JSON—exactly one input form per call. MCP and REST accept the same typed request bodies.\n\n```json\n{{\n  \"inputs\": [{{\"input\": \"https://example.com/docs\", \"idempotency_key\": \"docs-v1\"}}],\n  \"options\": {{\"execution\": {{\"mode\": \"background\", \"detached\": true}}}}\n}}\n```\n\n## Execution and results\n\nEvery mutating item is admitted to the canonical durable job store before execution. Background or detached work returns an ordered `queued` outcome and HTTP `202`; foreground/wait work returns completed, failed, or canceled outcomes and HTTP `200`. Raw source inputs are omitted from queued responses. `code-search` is read-only committed-state retrieval and never refreshes an index.\n\nThe batch envelope contains `batch_id`, aggregate `status`, ordered `items`, and a `summary`. Each item has exactly one tagged outcome: `completed`, `queued`, `failed`, or `canceled`. Per-item source idempotency keys are scoped by operation and opaque principal; identical semantic requests reuse the retained canonical job, while a different request under the same key is a typed conflict.\n\nServer policy clamps caller limits downward. The universal `source` and `query` surfaces remain supported; these names are focused ergonomic projections over the same services, not alternate pipelines or compatibility aliases.\n\n## Fixtures\n\n{} canonical semantic fixtures are embedded in the JSON contract. Transport-specific caller, cwd, authentication, and runtime context are intentionally excluded.\n",
        contract["fixtures"].as_array().map_or(0, Vec::len)
    ));
    Ok(output)
}
