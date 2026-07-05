use std::path::Path;

use anyhow::Result;
use serde_json::{Value, json};

use super::SchemaFamily;
use super::artifact::SchemaArtifact;
use super::registry::CANONICAL_ENUMS;
use super::rel;
use super::schema_json::{json_string, schema_defs};
use super::source_input::source_inputs;

#[path = "api_defs.rs"]
pub(crate) mod api_defs;
#[path = "families/bundles.rs"]
mod bundles;
#[path = "families/markdown.rs"]
mod markdown_render;
#[path = "runtime_defs.rs"]
mod runtime_defs;
#[path = "vector_payload.rs"]
mod vector_payload;

use bundles::registry_schema_bundle;
pub(super) use bundles::{enum_defs, schema_bundle, schema_id};
use markdown_render::{
    api_markdown, enum_markdown, markdown, registry_markdown, registry_projection_markdown,
};

pub trait FamilyGenerator {
    fn generate(&self, root: &Path) -> Result<Vec<SchemaArtifact>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FamilyStatus {
    RegistryBacked,
}

#[derive(Debug, Clone, Copy)]
pub struct FamilyMetadata {
    pub family: SchemaFamily,
    pub status: FamilyStatus,
}

pub fn family_metadata() -> Vec<FamilyMetadata> {
    all_families()
        .into_iter()
        .map(|family| FamilyMetadata {
            family,
            status: FamilyStatus::RegistryBacked,
        })
        .collect()
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
            SchemaFamily::Cli => cli_artifacts(root),
            SchemaFamily::Openapi => openapi_artifacts(root),
            SchemaFamily::Mcp => mcp_artifacts(root),
            SchemaFamily::Config => config_artifacts(root),
            SchemaFamily::Errors => error_artifacts(root),
            SchemaFamily::VectorPayload => vector_payload::vector_payload_artifacts(root),
            SchemaFamily::Events => runtime_defs::events_artifacts(root),
            SchemaFamily::Database => runtime_defs::database_artifacts(root),
            SchemaFamily::Graph => graph_artifacts(root),
            SchemaFamily::Providers => provider_artifacts(root),
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
            "crates/axon-api/src/schema_registry.rs",
            "crates/axon-api/src/source.rs",
            "crates/axon-api/src/source/boundary.rs",
            "crates/axon-api/src/source/common.rs",
            "crates/axon-api/src/source/capability.rs",
            "crates/axon-api/src/source/document.rs",
            "crates/axon-api/src/source/lifecycle.rs",
            "crates/axon-api/src/source/listing.rs",
            "crates/axon-api/src/source/enums.rs",
            "crates/axon-api/src/source/graph.rs",
            "crates/axon-api/src/source/ids.rs",
            "crates/axon-api/src/source/job.rs",
            "crates/axon-api/src/source/job_listing.rs",
            "crates/axon-api/src/source/provider_io.rs",
            "crates/axon-api/src/source/prune.rs",
            "crates/axon-api/src/source/stage.rs",
            "crates/axon-api/src/source/state.rs",
            "crates/axon-api/src/source/status.rs",
            "crates/axon-api/src/source/vector.rs",
            "crates/axon-error/src/api_error.rs",
            "xtask/src/schemas/api_defs.rs",
            "xtask/src/schemas/registry.rs",
            "docs/pipeline-unification/schemas/api-dto-schema.md",
        ],
    )?;
    let defs = schema_defs(&api_schema_defs(), Some(enum_defs("axon-api")));
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
        SchemaArtifact::new(rel("docs/reference/api/dto.md"), api_markdown(&inputs)),
        SchemaArtifact::new(rel("docs/reference/api/enums.md"), enum_markdown()),
    ])
}

fn api_schema_defs() -> Vec<(&'static str, Value)> {
    let mut defs = api_defs::api_source_schema_defs();
    defs.extend(api_defs::api_vector_schema_defs());
    defs.push((
        "ApiError",
        schemars::schema_for!(axon_error::ApiError).into(),
    ));
    defs
}

fn error_artifacts(root: &Path) -> Result<Vec<SchemaArtifact>> {
    let inputs = source_inputs(
        root,
        &[
            "crates/axon-error/src/schema_registry.rs",
            "crates/axon-error/src/lib.rs",
            "crates/axon-error/src/api_error.rs",
            "crates/axon-error/src/code.rs",
            "crates/axon-error/src/stage.rs",
            "docs/pipeline-unification/schemas/error-schema.md",
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

fn cli_artifacts(root: &Path) -> Result<Vec<SchemaArtifact>> {
    let spec = family_specs::spec_for(SchemaFamily::Cli);
    let inputs = source_inputs(root, spec.source_paths)?;
    let commands = axon_cli::schema_registry::command_registry()
        .iter()
        .map(|command| {
            json!({
                "name": command.name,
                "maps_to_dto": command.maps_to_dto,
                "mutates": command.mutates,
                "async": command.async_job,
                "requires_auth_scope": command.required_scope
            })
        })
        .collect::<Vec<_>>();
    let schema = registry_schema_bundle(
        schema_id(SchemaFamily::Cli),
        spec.title,
        "cargo xtask schemas cli",
        spec.owner_crates,
        &inputs,
        "commands",
        commands,
        &[],
    );
    Ok(vec![
        SchemaArtifact::new(rel(spec.json_path), json_string(&schema)?),
        SchemaArtifact::new(
            rel(spec.markdown_path),
            registry_markdown("cli", &inputs, "Commands"),
        ),
        SchemaArtifact::new(
            rel(spec.extra_markdown_path.unwrap()),
            registry_markdown("cli-help", &inputs, "Commands"),
        ),
    ])
}

fn mcp_artifacts(root: &Path) -> Result<Vec<SchemaArtifact>> {
    let spec = family_specs::spec_for(SchemaFamily::Mcp);
    let inputs = source_inputs(root, spec.source_paths)?;
    let actions = axon_mcp::schema_registry::action_registry()
        .iter()
        .map(|action| {
            json!({
                "action": action.action,
                "request_dto": action.request_dto,
                "result_dto": action.result_dto,
                "requires_auth_scope": action.required_scope,
                "mutates": action.mutates,
                "async": action.async_job,
                "subaction": null
            })
        })
        .collect::<Vec<_>>();
    let schema = registry_schema_bundle(
        schema_id(SchemaFamily::Mcp),
        spec.title,
        "cargo xtask schemas mcp",
        spec.owner_crates,
        &inputs,
        "actions",
        actions,
        &[],
    );
    Ok(vec![
        SchemaArtifact::new(rel(spec.json_path), json_string(&schema)?),
        SchemaArtifact::new(rel(spec.extra_json.unwrap().path), json_string(&schema)?),
        SchemaArtifact::new(
            rel(spec.markdown_path),
            registry_markdown("mcp", &inputs, "Actions"),
        ),
    ])
}

fn openapi_artifacts(root: &Path) -> Result<Vec<SchemaArtifact>> {
    let spec = family_specs::spec_for(SchemaFamily::Openapi);
    let inputs = source_inputs(root, spec.source_paths)?;
    let routes = axon_web::schema_registry::rest_route_registry()
        .iter()
        .map(|route| {
            json!({
                "method": route.method,
                "path": route.path,
                "operation_id": route.operation_id,
                "request_dto": route.request_dto,
                "result_dto": route.result_dto,
                "requires_auth_scope": route.required_scope,
                "mutates": route.mutates,
                "streaming": route.streaming,
                "responses": route.responses
            })
        })
        .collect::<Vec<_>>();
    let schema = registry_schema_bundle(
        schema_id(SchemaFamily::Openapi),
        spec.title,
        "cargo xtask schemas openapi",
        spec.owner_crates,
        &inputs,
        "routes",
        routes,
        &[],
    );
    Ok(vec![
        SchemaArtifact::new(rel(spec.json_path), json_string(&schema)?),
        SchemaArtifact::new(
            rel(spec.markdown_path),
            registry_markdown("openapi", &inputs, "Routes"),
        ),
        SchemaArtifact::new(
            rel(spec.extra_markdown_path.unwrap()),
            registry_projection_markdown("openapi-schemas", "openapi", &inputs, "Routes"),
        ),
    ])
}

fn config_artifacts(root: &Path) -> Result<Vec<SchemaArtifact>> {
    let spec = family_specs::spec_for(SchemaFamily::Config);
    let inputs = source_inputs(root, spec.source_paths)?;
    let keys = config_key_records();
    let schema = registry_schema_bundle(
        schema_id(SchemaFamily::Config),
        spec.title,
        "cargo xtask schemas config",
        spec.owner_crates,
        &inputs,
        "config_keys",
        keys.clone(),
        &[],
    );
    let extra = spec.extra_json.unwrap();
    let env_schema = registry_schema_bundle(
        extra.id,
        extra.title,
        "cargo xtask schemas config",
        spec.owner_crates,
        &inputs,
        "config_keys",
        keys,
        &[],
    );
    Ok(vec![
        SchemaArtifact::new(rel(spec.json_path), json_string(&schema)?),
        SchemaArtifact::new(rel(extra.path), json_string(&env_schema)?),
        SchemaArtifact::new(
            rel(spec.markdown_path),
            registry_markdown("config", &inputs, "Config Keys"),
        ),
        SchemaArtifact::new(
            rel(spec.extra_markdown_path.unwrap()),
            registry_markdown("env", &inputs, "Config Keys"),
        ),
    ])
}

fn config_key_records() -> Vec<Value> {
    axon_core::config::schema_registry::config_key_registry()
        .iter()
        .map(|key| {
            json!({
                "key": key.key,
                "section": key.section,
                "env_key": key.env_key,
                "secret": key.secret
            })
        })
        .collect()
}

fn graph_artifacts(root: &Path) -> Result<Vec<SchemaArtifact>> {
    let spec = family_specs::spec_for(SchemaFamily::Graph);
    let inputs = source_inputs(root, spec.source_paths)?;
    let mut kinds = Vec::new();
    for node in axon_graph::schema_registry::node_kind_registry() {
        kinds.push(
            json!({"kind": node.kind, "type": "node", "requires_evidence": node.requires_evidence}),
        );
    }
    for edge in axon_graph::schema_registry::edge_kind_registry() {
        kinds.push(
            json!({"kind": edge.kind, "type": "edge", "requires_evidence": edge.requires_evidence}),
        );
    }
    let schema = registry_schema_bundle(
        schema_id(SchemaFamily::Graph),
        spec.title,
        "cargo xtask schemas graph",
        spec.owner_crates,
        &inputs,
        "graph_kinds",
        kinds,
        &[],
    );
    Ok(vec![
        SchemaArtifact::new(rel(spec.json_path), json_string(&schema)?),
        SchemaArtifact::new(
            rel(spec.markdown_path),
            registry_markdown("graph", &inputs, "Graph Kinds"),
        ),
    ])
}

fn provider_artifacts(root: &Path) -> Result<Vec<SchemaArtifact>> {
    let spec = family_specs::spec_for(SchemaFamily::Providers);
    let inputs = source_inputs(root, spec.source_paths)?;
    let providers = axon_api::schema_registry::dto_schema_registry()
        .iter()
        .filter(|dto| dto.family == "Provider capability DTOs")
        .map(|dto| {
            json!({
                "provider_kind": dto.name,
                "health": "required",
                "limits": "required",
                "reservation_policy": "required",
                "degraded_modes": "required",
                "capabilities": ["embed", "rerank", "synthesize"]
            })
        })
        .collect::<Vec<_>>();
    let schema = registry_schema_bundle(
        schema_id(SchemaFamily::Providers),
        spec.title,
        "cargo xtask schemas providers",
        spec.owner_crates,
        &inputs,
        "providers",
        providers,
        &[],
    );
    Ok(vec![
        SchemaArtifact::new(rel(spec.json_path), json_string(&schema)?),
        SchemaArtifact::new(
            rel(spec.markdown_path),
            registry_markdown("providers", &inputs, "Providers"),
        ),
    ])
}

pub(super) mod family_specs;
