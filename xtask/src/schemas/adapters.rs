use std::path::Path;

use anyhow::Result;
use axon_adapters::{
    ParserFamily, SourceAdapterSpec, SourceFamily, SourceScopeCapability, source_family_matrix,
};
use axon_route::{AdapterDefinition, AdapterRegistry};
use serde::Serialize;
use serde_json::{Value, json};

use crate::schemas::artifact::SchemaArtifact;
use crate::schemas::rel;
use crate::schemas::schema_json::json_string;
use crate::schemas::source_input::{SourceInput, source_inputs};

pub fn adapter_artifacts(root: &Path) -> Result<Vec<SchemaArtifact>> {
    let capability = generate_adapter_capability_artifact_for(root)?;
    let markdown = generate_adapter_capability_markdown_for(root)?;

    Ok(vec![capability, markdown])
}

pub fn generate_adapter_capability_artifact() -> Result<SchemaArtifact> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask manifest has workspace parent");
    generate_adapter_capability_artifact_for(root)
}

fn generate_adapter_capability_artifact_for(root: &Path) -> Result<SchemaArtifact> {
    let inputs = source_inputs(
        root,
        &[
            "crates/axon-route/src/capability.rs",
            "crates/axon-adapters/src/family_matrix.rs",
            "crates/axon-adapters/src/onboarding.rs",
            "crates/axon-adapters/src/spec.rs",
            "crates/axon-adapters/src/web.rs",
            "crates/axon-adapters/fixtures/provider-variant-exceptions.json",
            "docs/pipeline-unification/sources/adapter-scopes.md",
            "docs/pipeline-unification/sources/new-source-contract.md",
        ],
    )?;
    let registry = AdapterRegistry::target_defaults();
    let adapters = registry
        .definitions()
        .iter()
        .map(adapter_json)
        .collect::<Result<Vec<_>>>()?;
    let matrix = source_family_matrix()
        .iter()
        .map(matrix_json)
        .collect::<Result<Vec<_>>>()?;
    let schema = schema_bundle(&inputs, adapters, matrix);

    Ok(SchemaArtifact::new(
        rel("docs/reference/sources/adapter-scopes.json"),
        json_string(&schema)?,
    ))
}

fn generate_adapter_capability_markdown_for(root: &Path) -> Result<SchemaArtifact> {
    let inputs = source_inputs(
        root,
        &[
            "crates/axon-route/src/capability.rs",
            "crates/axon-adapters/src/family_matrix.rs",
            "crates/axon-adapters/src/onboarding.rs",
            "crates/axon-adapters/src/spec.rs",
            "crates/axon-adapters/src/web.rs",
            "crates/axon-adapters/fixtures/provider-variant-exceptions.json",
            "docs/pipeline-unification/sources/adapter-scopes.md",
            "docs/pipeline-unification/sources/new-source-contract.md",
        ],
    )?;
    let registry = AdapterRegistry::target_defaults();
    Ok(SchemaArtifact::new(
        rel("docs/reference/sources/adapter-scopes.md"),
        markdown(&inputs, registry.definitions(), source_family_matrix())?,
    ))
}

fn schema_bundle(inputs: &[SourceInput], adapters: Vec<Value>, matrix: Vec<Value>) -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://axon.local/schemas/sources/adapter-scopes.json",
        "title": "AxonAdapterScopeRegistry",
        "description": "Generated route-time adapter scope and capability registry.",
        "type": "object",
        "required": ["x-axon"],
        "properties": envelope_properties(),
        "additionalProperties": false,
        "$defs": adapter_defs(),
        "x-axon": {
            "contract_version": "2026-06-30",
            "generated_by": "cargo xtask schemas adapters",
            "owner_crates": ["axon-route", "axon-adapters"],
            "source_inputs": inputs,
            "clean_break": true,
            "registry_status": "route-time implemented subset",
            "adapters": adapters,
            "source_family_matrix": matrix
        }
    })
}

fn envelope_properties() -> Value {
    json!({
        "x-axon": {
            "type": "object",
            "required": [
                "contract_version",
                "generated_by",
                "owner_crates",
                "source_inputs",
                "clean_break",
                "registry_status",
                "adapters",
                "source_family_matrix"
            ],
            "properties": {
                "contract_version": { "type": "string" },
                "generated_by": { "const": "cargo xtask schemas adapters" },
                "owner_crates": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "source_inputs": {
                    "type": "array",
                    "items": { "type": "object" }
                },
                "clean_break": { "const": true },
                "registry_status": { "type": "string" },
                "adapters": {
                    "type": "array",
                    "items": { "$ref": "#/$defs/AdapterCapability" }
                },
                "source_family_matrix": {
                    "type": "array",
                    "items": { "$ref": "#/$defs/SourceFamilyCapability" }
                }
            },
            "additionalProperties": false
        }
    })
}

fn adapter_defs() -> Value {
    json!({
        "AdapterCapability": {
            "type": "object",
            "required": [
                "name",
                "version",
                "source_kind",
                "default_scope",
                "supported_scopes",
                "watch_supported",
                "refresh_supported"
            ],
            "properties": adapter_properties(),
            "additionalProperties": true
        },
        "SourceFamilyCapability": {
            "type": "object",
            "required": [
                "family",
                "version",
                "source_kinds",
                "vector_namespace",
                "is_source_adapter",
                "may_execute_tools"
            ],
            "properties": source_family_properties(),
            "additionalProperties": true
        }
    })
}

fn adapter_properties() -> Value {
    json!({
        "name": { "type": "string" },
        "version": { "type": "string" },
        "source_kind": { "type": "string" },
        "default_scope": { "type": "string" },
        "supported_scopes": {
            "type": "array",
            "items": { "type": "string" }
        },
        "safety_class": { "type": "string" },
        "execution_affinity": { "type": "string" },
        "provider_requirements": {
            "type": "array",
            "items": { "type": "object" }
        },
        "credential_requirements": {
            "type": "array",
            "items": { "type": "object" }
        },
        "option_schema_id": { "type": "string" },
        "allowed_option_keys": {
            "type": "array",
            "items": { "type": "string" }
        },
        "chunking_hints": {
            "type": "array",
            "items": { "type": "object" }
        },
        "parser_hints": {
            "type": "array",
            "items": { "type": "object" }
        },
        "watch_supported": { "type": "boolean" },
        "refresh_supported": { "type": "boolean" }
    })
}

fn source_family_properties() -> Value {
    json!({
        "family": { "type": "string" },
        "adapter": { "type": "string" },
        "integration": { "type": "string" },
        "version": { "type": "string" },
        "source_kinds": {
            "type": "array",
            "items": { "type": "string" }
        },
        "vector_namespace": { "type": "string" },
        "supported_schemes": {
            "type": "array",
            "items": { "type": "string" }
        },
        "shorthand_patterns": {
            "type": "array",
            "items": { "type": "string" }
        },
        "default_scope": { "type": "string" },
        "scopes": {
            "type": "array",
            "items": { "type": "object" }
        },
        "credential_requirements": {
            "type": "array",
            "items": { "type": "object" }
        },
        "option_schema": { "type": "string" },
        "parser_families": {
            "type": "array",
            "items": { "type": "string" }
        },
        "metadata_families": {
            "type": "array",
            "items": { "type": "string" }
        },
        "watch_supported": { "type": "boolean" },
        "refresh_supported": { "type": "boolean" },
        "may_access_local_paths": { "type": "boolean" },
        "may_perform_network_fetches": { "type": "boolean" },
        "may_call_render_provider": { "type": "boolean" },
        "may_execute_tools": { "type": "boolean" },
        "is_source_adapter": { "type": "boolean" },
        "degraded_modes": {
            "type": "array",
            "items": { "type": "string" }
        },
        "required_graph_fact_kinds": {
            "type": "array",
            "items": { "type": "string" }
        },
        "optional_graph_fact_kinds": {
            "type": "array",
            "items": { "type": "string" }
        }
    })
}

fn adapter_json(adapter: &AdapterDefinition) -> Result<Value> {
    Ok(json!({
        "name": adapter.adapter.name,
        "version": adapter.adapter.version,
        "source_kind": wire(adapter.source_kind)?,
        "default_scope": wire(adapter.default_scope)?,
        "supported_scopes": wire_list(&adapter.supported_scopes)?,
        "safety_class": wire(adapter.safety_class)?,
        "execution_affinity": wire(adapter.execution_affinity)?,
        "provider_requirements": adapter.provider_requirements,
        "credential_requirements": adapter.credential_requirements,
        "option_schema_id": adapter.option_schema_id,
        "allowed_option_keys": adapter.allowed_option_keys,
        "chunking_hints": adapter.chunking_hints,
        "parser_hints": adapter.parser_hints,
        "watch_supported": adapter.watch_supported,
        "refresh_supported": adapter.refresh_supported,
    }))
}

fn matrix_json(spec: &SourceAdapterSpec) -> Result<Value> {
    let mut value = json!({
        "family": source_family_wire(spec.family),
        "version": spec.version,
        "source_kinds": wire_list(spec.source_kinds)?,
        "vector_namespace": spec.vector_namespace,
        "supported_schemes": spec.supported_schemes,
        "shorthand_patterns": spec.shorthand_patterns,
        "default_scope": wire(spec.default_scope)?,
        "scopes": spec
            .scopes
            .iter()
            .map(scope_json)
            .collect::<Result<Vec<_>>>()?,
        "credential_requirements": spec.credential_requirements,
        "option_schema": spec.option_schema,
        "parser_families": spec
            .parser_families
            .iter()
            .copied()
            .map(parser_family_wire)
            .collect::<Vec<_>>(),
        "metadata_families": spec.metadata_families,
        "watch_supported": spec.watch_supported,
        "refresh_supported": spec.refresh_supported,
        "may_access_local_paths": spec.may_access_local_paths,
        "may_perform_network_fetches": spec.may_perform_network_fetches,
        "may_call_render_provider": spec.may_call_render_provider,
        "may_execute_tools": spec.may_execute_tools,
        "is_source_adapter": spec.is_source_adapter,
        "degraded_modes": spec.degraded_modes,
        "required_graph_fact_kinds": spec.required_graph_fact_kinds,
        "optional_graph_fact_kinds": spec.optional_graph_fact_kinds,
    });
    let object = value.as_object_mut().expect("matrix record object");
    if spec.is_source_adapter {
        object.insert("adapter".to_string(), json!(spec.adapter));
    } else {
        object.insert("integration".to_string(), json!(spec.adapter));
    }
    Ok(value)
}

fn scope_json(scope: &SourceScopeCapability) -> Result<Value> {
    Ok(json!({
        "scope": wire(scope.scope)?,
        "required": scope.required,
        "notes": scope.notes,
    }))
}

fn markdown(
    inputs: &[SourceInput],
    adapters: &[AdapterDefinition],
    matrix: &[SourceAdapterSpec],
) -> Result<String> {
    let mut out = String::from(
        "# adapters Schema Reference\n\nGenerated by `cargo xtask schemas adapters`.\n\n",
    );
    out.push_str("## Route-Time Adapter Registry\n\n");
    out.push_str("| Adapter | Source Kind | Default Scope | Supported Scopes | Watch | Refresh | Credentials |\n");
    out.push_str("|---|---|---|---|---:|---:|---|\n");
    for adapter in adapters {
        let scopes = wire_list(&adapter.supported_scopes)?.join("`, `");
        let credentials = adapter
            .credential_requirements
            .iter()
            .map(|requirement| wire(requirement.credential_kind))
            .collect::<Result<Vec<_>>>()?
            .join("`, `");
        out.push_str(&format!(
            "| `{}` | `{}` | `{}` | `{}` | {} | {} | {} |\n",
            adapter.adapter.name,
            wire(adapter.source_kind)?,
            wire(adapter.default_scope)?,
            scopes,
            adapter.watch_supported,
            adapter.refresh_supported,
            if credentials.is_empty() {
                "none".to_string()
            } else {
                format!("`{credentials}`")
            }
        ));
    }

    out.push_str("\n## Source-Family Matrix\n\n");
    out.push_str("| Family | Adapter/Integration | Namespace | Source Adapter | Schemes | May Execute Tools | Graph Facts |\n");
    out.push_str("|---|---|---|---:|---|---:|---|\n");
    for spec in matrix {
        out.push_str(&format!(
            "| `{}` | `{}` | `{}` | {} | {} | {} | `{}` |\n",
            source_family_wire(spec.family),
            spec.adapter,
            spec.vector_namespace,
            spec.is_source_adapter,
            if spec.supported_schemes.is_empty() {
                "none".to_string()
            } else {
                format!("`{}`", spec.supported_schemes.join("`, `"))
            },
            spec.may_execute_tools,
            spec.required_graph_fact_kinds.join("`, `")
        ));
    }

    out.push_str("\n## Source Inputs\n\n| Path | SHA-256 |\n|---|---|\n");
    for input in inputs {
        out.push_str(&format!("| `{}` | `{}` |\n", input.path, input.checksum));
    }
    Ok(out)
}

fn wire<T: Serialize>(value: T) -> Result<String> {
    Ok(serde_json::to_value(value)?
        .as_str()
        .unwrap_or("unknown")
        .to_string())
}

fn wire_list<T: Serialize + Copy>(values: &[T]) -> Result<Vec<String>> {
    values.iter().copied().map(wire).collect()
}

fn source_family_wire(family: SourceFamily) -> &'static str {
    match family {
        SourceFamily::Local => "local",
        SourceFamily::Git => "git",
        SourceFamily::Web => "web",
        SourceFamily::Feed => "feed",
        SourceFamily::Youtube => "youtube",
        SourceFamily::Reddit => "reddit",
        SourceFamily::Sessions => "sessions",
        SourceFamily::Registry => "registry",
        SourceFamily::CliTool => "cli_tool",
        SourceFamily::McpTool => "mcp_tool",
        SourceFamily::MemoryIntegration => "memory",
    }
}

fn parser_family_wire(family: ParserFamily) -> &'static str {
    match family {
        ParserFamily::None => "none",
        ParserFamily::Markdown => "markdown",
        ParserFamily::Html => "html",
        ParserFamily::Code => "code",
        ParserFamily::Manifest => "manifest",
        ParserFamily::Feed => "feed",
        ParserFamily::Transcript => "transcript",
        ParserFamily::Session => "session",
        ParserFamily::PackageMetadata => "package_metadata",
        ParserFamily::ToolOutput => "tool_output",
        ParserFamily::ApiSchema => "api_schema",
        ParserFamily::Memory => "memory",
    }
}
