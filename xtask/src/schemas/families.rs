use std::path::Path;

use anyhow::Result;
use serde_json::{Value, json};

use super::SchemaFamily;
use super::artifact::SchemaArtifact;
use super::registry::CANONICAL_ENUMS;
use super::rel;
use super::schema_json::{json_string, schema_defs};
use super::source_input::{SourceInput, source_inputs};

pub trait FamilyGenerator {
    fn generate(&self, root: &Path) -> Result<Vec<SchemaArtifact>>;
}

pub fn all_families() -> Vec<SchemaFamily> {
    vec![
        SchemaFamily::Api,
        SchemaFamily::Cli,
        SchemaFamily::Openapi,
        SchemaFamily::Mcp,
        SchemaFamily::Config,
        SchemaFamily::Events,
        SchemaFamily::Errors,
        SchemaFamily::Database,
        SchemaFamily::Graph,
        SchemaFamily::VectorPayload,
        SchemaFamily::Providers,
    ]
}

pub fn generator_for(family: SchemaFamily) -> Box<dyn FamilyGenerator> {
    Box::new(Generator { family })
}

struct Generator {
    family: SchemaFamily,
}

impl FamilyGenerator for Generator {
    fn generate(&self, root: &Path) -> Result<Vec<SchemaArtifact>> {
        match self.family {
            SchemaFamily::Api => api_artifacts(root),
            SchemaFamily::Errors => error_artifacts(root),
            family => skeleton_artifacts(root, spec_for(family)),
        }
    }
}

struct FamilySpec {
    family: SchemaFamily,
    title: &'static str,
    owner_crates: &'static [&'static str],
    source_paths: &'static [&'static str],
    json_path: &'static str,
    extra_json: Option<ExtraJsonSpec>,
    markdown_path: &'static str,
    extra_markdown_path: Option<&'static str>,
}

struct ExtraJsonSpec {
    path: &'static str,
    title: &'static str,
    id: &'static str,
}

fn api_artifacts(root: &Path) -> Result<Vec<SchemaArtifact>> {
    let inputs = source_inputs(
        root,
        &[
            "crates/axon-api/src/source.rs",
            "crates/axon-api/src/source/document.rs",
            "crates/axon-api/src/source/lifecycle.rs",
            "crates/axon-api/src/source/enums.rs",
            "crates/axon-api/src/source/ids.rs",
            "crates/axon-api/src/source/stage.rs",
            "crates/axon-api/src/source/state.rs",
            "crates/axon-api/src/source/status.rs",
            "crates/axon-error/src/api_error.rs",
        ],
    )?;
    let defs = schema_defs(
        &[
            (
                "SourceRequest",
                schemars::schema_for!(axon_api::source::SourceRequest).into(),
            ),
            (
                "SourceResult",
                schemars::schema_for!(axon_api::source::SourceResult).into(),
            ),
            (
                "ResolvedSource",
                schemars::schema_for!(axon_api::source::ResolvedSource).into(),
            ),
            (
                "SourceGeneration",
                schemars::schema_for!(axon_api::source::SourceGeneration).into(),
            ),
            (
                "CleanupDebt",
                schemars::schema_for!(axon_api::source::CleanupDebt).into(),
            ),
            (
                "LeaseRequest",
                schemars::schema_for!(axon_api::source::LeaseRequest).into(),
            ),
            (
                "LeaseGuard",
                schemars::schema_for!(axon_api::source::LeaseGuard).into(),
            ),
            (
                "CleanupSelector",
                schemars::schema_for!(axon_api::source::CleanupSelector).into(),
            ),
            (
                "DocumentStatus",
                schemars::schema_for!(axon_api::source::DocumentStatus).into(),
            ),
            (
                "ApiError",
                schemars::schema_for!(axon_error::ApiError).into(),
            ),
        ],
        Some(enum_defs("axon-api")),
    );
    let schema = schema_bundle(
        "https://axon.local/schemas/api/schemas.schema.json",
        "AxonApiSchemas",
        "cargo xtask schemas api",
        &["axon-api", "axon-error"],
        &inputs,
        defs,
    );
    Ok(vec![
        SchemaArtifact::new(
            rel("docs/reference/api/schemas.json"),
            json_string(&schema)?,
        ),
        SchemaArtifact::new(rel("docs/reference/api/dto.md"), markdown("api", &inputs)),
        SchemaArtifact::new(rel("docs/reference/api/enums.md"), enum_markdown()),
    ])
}

fn error_artifacts(root: &Path) -> Result<Vec<SchemaArtifact>> {
    let inputs = source_inputs(
        root,
        &[
            "crates/axon-error/src/lib.rs",
            "crates/axon-error/src/api_error.rs",
            "crates/axon-error/src/code.rs",
            "crates/axon-error/src/stage.rs",
            "docs/pipeline-unification/runtime/error-handling.md",
        ],
    )?;
    let defs = schema_defs(
        &[
            (
                "ApiError",
                schemars::schema_for!(axon_error::ApiError).into(),
            ),
            (
                "ErrorCode",
                schemars::schema_for!(axon_error::ErrorCode).into(),
            ),
            (
                "ErrorStage",
                schemars::schema_for!(axon_error::ErrorStage).into(),
            ),
            (
                "ErrorSeverity",
                schemars::schema_for!(axon_error::ErrorSeverity).into(),
            ),
        ],
        None,
    );
    let schema = schema_bundle(
        "https://axon.local/schemas/errors/errors.schema.json",
        "AxonErrorSchemas",
        "cargo xtask schemas errors",
        &["axon-error"],
        &inputs,
        defs,
    );
    Ok(vec![
        SchemaArtifact::new(
            rel("docs/reference/api/errors.schema.json"),
            json_string(&schema)?,
        ),
        SchemaArtifact::new(
            rel("docs/reference/api/errors.md"),
            markdown("errors", &inputs),
        ),
    ])
}

fn skeleton_artifacts(root: &Path, spec: FamilySpec) -> Result<Vec<SchemaArtifact>> {
    let inputs = source_inputs(root, spec.source_paths)?;
    let schema = schema_bundle(
        schema_id(spec.family),
        spec.title,
        &format!("cargo xtask schemas {}", spec.family.as_str()),
        spec.owner_crates,
        &inputs,
        skeleton_defs(spec.family),
    );
    let mut artifacts = vec![
        SchemaArtifact::new(rel(spec.json_path), json_string(&schema)?),
        SchemaArtifact::new(
            rel(spec.markdown_path),
            markdown(spec.family.as_str(), &inputs),
        ),
    ];
    if let Some(extra_json) = spec.extra_json {
        let extra_schema = schema_bundle(
            extra_json.id,
            extra_json.title,
            &format!("cargo xtask schemas {}", spec.family.as_str()),
            spec.owner_crates,
            &inputs,
            skeleton_defs(spec.family),
        );
        artifacts.push(SchemaArtifact::new(
            rel(extra_json.path),
            json_string(&extra_schema)?,
        ));
    }
    if let Some(path) = spec.extra_markdown_path {
        artifacts.push(SchemaArtifact::new(
            rel(path),
            markdown(spec.family.as_str(), &inputs),
        ));
    }
    Ok(artifacts)
}

fn schema_bundle(
    id: &str,
    title: &str,
    generated_by: &str,
    owner_crates: &[&str],
    inputs: &[SourceInput],
    defs: Value,
) -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": id,
        "title": title,
        "description": "Generated Axon schema contract artifact.",
        "type": "object",
        "additionalProperties": false,
        "$defs": defs,
        "x-axon": {
            "contract_version": "2026-06-30",
            "generated_by": generated_by,
            "owner_crates": owner_crates,
            "source_inputs": inputs,
            "clean_break": true,
        }
    })
}

fn enum_defs(owner_crate: &str) -> Value {
    let mut defs = serde_json::Map::new();
    for (name, values) in CANONICAL_ENUMS {
        defs.insert(
            (*name).to_string(),
            json!({
                "type": "string",
                "enum": values,
                "x-axon": {
                    "rust_enum": name,
                    "owner_crate": owner_crate,
                }
            }),
        );
    }
    Value::Object(defs)
}

fn skeleton_defs(family: SchemaFamily) -> Value {
    json!({
        "SchemaFamilyContract": {
            "type": "object",
            "required": ["family", "status", "owner_crates"],
            "properties": {
                "family": { "const": family.as_str() },
                "status": { "const": "skeleton" },
                "owner_crates": { "type": "array", "items": { "type": "string" } },
                "future_registry": { "type": "string" }
            },
            "additionalProperties": false,
            "x-axon": {
                "skeleton": true,
                "replaced_by": "crate-owned registry generator in the matching implementation PR"
            }
        }
    })
}

fn enum_markdown() -> String {
    let mut out = generated_header("api-enums");
    out.push_str("| Enum | Values |\n|---|---|\n");
    for (name, values) in CANONICAL_ENUMS {
        out.push_str(&format!("| `{name}` | `{}` |\n", values.join("`, `")));
    }
    out
}

fn markdown(family: &str, inputs: &[SourceInput]) -> String {
    let mut out = generated_header(family);
    out.push_str("## Source Inputs\n\n| Path | SHA-256 |\n|---|---|\n");
    for input in inputs {
        out.push_str(&format!("| `{}` | `{}` |\n", input.path, input.checksum));
    }
    out
}

fn generated_header(family: &str) -> String {
    format!("# {family} Schema Reference\n\nGenerated by `cargo xtask schemas {family}`.\n\n")
}

fn schema_id(family: SchemaFamily) -> &'static str {
    match family {
        SchemaFamily::Cli => "https://axon.local/schemas/cli/commands.schema.json",
        SchemaFamily::Openapi => "https://axon.local/schemas/rest/openapi.schema.json",
        SchemaFamily::Mcp => "https://axon.local/schemas/mcp/tool.schema.json",
        SchemaFamily::Config => "https://axon.local/schemas/config/config.schema.json",
        SchemaFamily::Events => "https://axon.local/schemas/runtime/events.schema.json",
        SchemaFamily::Database => "https://axon.local/schemas/runtime/database.schema.json",
        SchemaFamily::Graph => "https://axon.local/schemas/sources/graph.schema.json",
        SchemaFamily::VectorPayload => {
            "https://axon.local/schemas/sources/vector-payload.schema.json"
        }
        SchemaFamily::Providers => {
            "https://axon.local/schemas/runtime/provider-capabilities.schema.json"
        }
        SchemaFamily::Api | SchemaFamily::Errors => {
            unreachable!("real generators use explicit ids")
        }
    }
}

fn spec_for(family: SchemaFamily) -> FamilySpec {
    match family {
        SchemaFamily::Cli => cli_spec(),
        SchemaFamily::Openapi => openapi_spec(),
        SchemaFamily::Mcp => mcp_spec(),
        SchemaFamily::Config => config_spec(),
        SchemaFamily::Events => events_spec(),
        SchemaFamily::Database => database_spec(),
        SchemaFamily::Graph => graph_spec(),
        SchemaFamily::VectorPayload => vector_payload_spec(),
        SchemaFamily::Providers => providers_spec(),
        SchemaFamily::Api | SchemaFamily::Errors => unreachable!("real generator"),
    }
}

fn cli_spec() -> FamilySpec {
    FamilySpec {
        family: SchemaFamily::Cli,
        title: "AxonCliSchema",
        owner_crates: &["axon-cli", "axon-api"],
        source_paths: &[
            "crates/axon-cli/src",
            "docs/pipeline-unification/surfaces/command-contract.md",
        ],
        json_path: "docs/reference/cli/commands.json",
        extra_json: None,
        markdown_path: "docs/reference/cli/commands.md",
        extra_markdown_path: Some("docs/reference/cli/axon-help.md"),
    }
}

fn openapi_spec() -> FamilySpec {
    FamilySpec {
        family: SchemaFamily::Openapi,
        title: "AxonOpenApiSchema",
        owner_crates: &["axon-web", "axon-api"],
        source_paths: &[
            "crates/axon-web/src",
            "docs/pipeline-unification/surfaces/rest-contract.md",
        ],
        json_path: "docs/reference/rest/openapi.json",
        extra_json: None,
        markdown_path: "docs/reference/rest/openapi.md",
        extra_markdown_path: Some("docs/reference/rest/schemas.md"),
    }
}

fn mcp_spec() -> FamilySpec {
    FamilySpec {
        family: SchemaFamily::Mcp,
        title: "AxonMcpToolSchema",
        owner_crates: &["axon-mcp", "axon-api"],
        source_paths: &[
            "crates/axon-mcp/src",
            "docs/pipeline-unification/surfaces/tool-contract.md",
        ],
        json_path: "docs/reference/mcp/tool-schema.json",
        extra_json: Some(ExtraJsonSpec {
            path: "crates/axon-mcp/tests/golden/tool-schema.json",
            title: "AxonMcpToolSchema",
            id: "https://axon.local/schemas/mcp/tool.schema.json",
        }),
        markdown_path: "docs/reference/mcp/pipeline-tool-schema.md",
        extra_markdown_path: None,
    }
}

fn config_spec() -> FamilySpec {
    FamilySpec {
        family: SchemaFamily::Config,
        title: "AxonConfigSchema",
        owner_crates: &["axon-core"],
        source_paths: &[
            "crates/axon-core/src/config",
            "docs/pipeline-unification/configuration/config-contract.md",
        ],
        json_path: "docs/reference/config/config.schema.json",
        extra_json: Some(ExtraJsonSpec {
            path: "docs/reference/config/env.schema.json",
            title: "AxonEnvSchema",
            id: "https://axon.local/schemas/config/env.schema.json",
        }),
        markdown_path: "docs/reference/config/config-toml.md",
        extra_markdown_path: Some("docs/reference/config/env.md"),
    }
}

fn events_spec() -> FamilySpec {
    FamilySpec {
        family: SchemaFamily::Events,
        title: "AxonEventSchema",
        owner_crates: &["axon-observe"],
        source_paths: &[
            "crates/axon-observe/src",
            "docs/pipeline-unification/runtime/observability-contract.md",
        ],
        json_path: "docs/reference/runtime/events.schema.json",
        extra_json: None,
        markdown_path: "docs/reference/runtime/events.md",
        extra_markdown_path: None,
    }
}

fn database_spec() -> FamilySpec {
    FamilySpec {
        family: SchemaFamily::Database,
        title: "AxonDatabaseSchema",
        owner_crates: &["axon-jobs", "axon-ledger"],
        source_paths: &[
            "crates/axon-jobs/src/migrations",
            "docs/pipeline-unification/runtime/schema-contract.md",
        ],
        json_path: "docs/reference/runtime/database-schema.json",
        extra_json: None,
        markdown_path: "docs/reference/runtime/database-schema.md",
        extra_markdown_path: None,
    }
}

fn graph_spec() -> FamilySpec {
    FamilySpec {
        family: SchemaFamily::Graph,
        title: "AxonGraphSchema",
        owner_crates: &["axon-graph", "axon-parse"],
        source_paths: &[
            "crates/axon-graph/src/lib.rs",
            "docs/pipeline-unification/sources/source-graph.md",
        ],
        json_path: "docs/reference/sources/graph.schema.json",
        extra_json: None,
        markdown_path: "docs/reference/sources/graph.md",
        extra_markdown_path: None,
    }
}

fn vector_payload_spec() -> FamilySpec {
    FamilySpec {
        family: SchemaFamily::VectorPayload,
        title: "AxonVectorPayloadSchema",
        owner_crates: &["axon-vectors", "axon-api"],
        source_paths: &[
            "crates/axon-vectors/src/lib.rs",
            "docs/pipeline-unification/schemas/vector-payload-schema.md",
        ],
        json_path: "docs/reference/sources/vector-payload.schema.json",
        extra_json: None,
        markdown_path: "docs/reference/sources/vector-payload.md",
        extra_markdown_path: None,
    }
}

fn providers_spec() -> FamilySpec {
    FamilySpec {
        family: SchemaFamily::Providers,
        title: "AxonProviderCapabilitySchema",
        owner_crates: &["axon-api", "axon-embedding", "axon-llm"],
        source_paths: &[
            "crates/axon-api/src/source/capability.rs",
            "docs/pipeline-unification/runtime/provider-contract.md",
        ],
        json_path: "docs/reference/runtime/provider-capabilities.schema.json",
        extra_json: None,
        markdown_path: "docs/reference/runtime/provider-capabilities.md",
        extra_markdown_path: None,
    }
}
