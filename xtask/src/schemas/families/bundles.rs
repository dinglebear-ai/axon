use serde_json::{Value, json};

use crate::schemas::SchemaFamily;
use crate::schemas::registry::CANONICAL_ENUMS;
use crate::schemas::source_input::SourceInput;

pub(super) fn registry_schema_bundle(
    id: &str,
    title: &str,
    generated_by: &str,
    owner_crates: &[&str],
    inputs: &[SourceInput],
    registry_key: &str,
    records: Vec<Value>,
    removed: &[&str],
) -> Value {
    let item_schema = registry_item_schema(registry_key);
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": id,
        "title": title,
        "type": "object",
        "additionalProperties": false,
        "required": [registry_key],
        "properties": {
            registry_key: {
                "type": "array",
                "items": item_schema
            }
        },
        registry_key: records,
        "x-axon": {
            "contract_version": "2026-06-30",
            "generated_by": generated_by,
            "owner_crates": owner_crates,
            "source_inputs": inputs,
            "status": "RegistryBacked",
            "removed": removed
        }
    })
}

fn registry_item_schema(registry_key: &str) -> Value {
    let required = match registry_key {
        "commands" => vec!["name", "maps_to_dto", "requires_auth_scope"],
        "actions" => vec!["action", "request_dto", "result_dto", "requires_auth_scope"],
        "routes" => vec![
            "method",
            "path",
            "operation_id",
            "result_dto",
            "requires_auth_scope",
            "responses",
        ],
        "config_keys" => vec!["key", "section", "env_key", "secret"],
        "graph_kinds" => vec!["kind", "type", "requires_evidence"],
        "providers" => vec![
            "provider_kind",
            "health",
            "limits",
            "reservation_policy",
            "degraded_modes",
            "capabilities",
        ],
        _ => Vec::new(),
    };
    json!({
        "type": "object",
        "required": required,
        "additionalProperties": true
    })
}

pub(crate) fn schema_bundle(
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

pub(crate) fn enum_defs(owner_crate: &str) -> Value {
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

pub(crate) fn schema_id(family: SchemaFamily) -> &'static str {
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
