use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use serde_json::{Map, Value, json};

use super::super::artifact::SchemaArtifact;
use super::super::rel;
use super::super::schema_json::{json_string, schema_defs};
use super::super::source_input::{SourceInput, source_inputs};
use axon_vectors::payload::{
    BARE_SECRET_TOKEN_PREFIXES, FORBIDDEN_FIELD_FRAGMENTS, FORBIDDEN_VALUE_FRAGMENTS,
    VECTOR_REDACTION_STATUS_VALUES, VECTOR_REQUIRED_FIELDS, VECTOR_SHARED_FIELDS,
    VECTOR_SOURCE_FAMILIES, VECTOR_SOURCE_FAMILY_FIELDS, VECTOR_VISIBILITY_VALUES,
};

mod vector_payload_markdown;

const VECTOR_API_DTOS: &[&str] = &[
    "EmbeddingBatch",
    "EmbeddingInput",
    "EmbeddingResult",
    "EmbeddingVector",
    "SparseVector",
    "VectorPointBatch",
    "VectorPoint",
    "PayloadIndexSpec",
    "CollectionSpec",
    "VectorDeleteSelector",
    "VectorStoreDeleteResult",
    "VectorSearchRequest",
    "VectorSearchResult",
    "VectorSearchMatch",
];

pub fn vector_payload_artifacts(root: &Path) -> Result<Vec<SchemaArtifact>> {
    let registry = StaticVectorPayloadContract::from_constants();
    let inputs = source_inputs(
        root,
        &[
            "crates/axon-vectors/src/schema_registry.rs",
            "crates/axon-vectors/src/payload.rs",
            "crates/axon-vectors/src/payload_families.rs",
            "crates/axon-vectors/src/point.rs",
            "crates/axon-api/src/source/vector.rs",
            "xtask/src/schemas/vector_payload_markdown.rs",
            "docs/pipeline-unification/sources/metadata-payload.md",
            "docs/pipeline-unification/sources/chunking-contract.md",
            "docs/pipeline-unification/schemas/vector-payload-schema.md",
        ],
    )?;
    let schema = schema_bundle(&inputs, &registry);

    Ok(vec![
        SchemaArtifact::new(
            rel("docs/reference/sources/vector-payload.schema.json"),
            json_string(&schema)?,
        ),
        SchemaArtifact::new(
            rel("docs/reference/sources/vector-payload.md"),
            vector_payload_markdown::markdown(&inputs, &registry),
        ),
    ])
}

struct StaticVectorPayloadContract {
    required_fields: &'static [&'static str],
    shared_fields: &'static [&'static str],
    source_families: &'static [(&'static str, &'static [&'static str])],
}

impl StaticVectorPayloadContract {
    fn from_constants() -> Self {
        Self {
            required_fields: VECTOR_REQUIRED_FIELDS,
            shared_fields: VECTOR_SHARED_FIELDS,
            source_families: VECTOR_SOURCE_FAMILY_FIELDS,
        }
    }
}

fn schema_bundle(inputs: &[SourceInput], registry: &StaticVectorPayloadContract) -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://axon.local/schemas/sources/vector-payload.schema.json",
        "title": "AxonVectorPayloadSchema",
        "description": "Generated vector payload metadata contract derived from the axon-vectors registry.",
        "type": "object",
        "required": registry.required_fields,
        "properties": schema_properties(registry),
        "additionalProperties": false,
        "allOf": source_family_conditionals(registry),
        "$defs": vector_payload_schema_defs(
            &[
                ("ChunkLocator", schemars::schema_for!(axon_api::source::ChunkLocator).into()),
                ("SourceRange", schemars::schema_for!(axon_api::source::SourceRange).into()),
            ],
        ),
        "x-axon": {
            "contract_version": axon_vectors::payload::VECTOR_PAYLOAD_CONTRACT_VERSION,
            "generated_by": "cargo xtask schemas vector-payload",
            "owner_crates": ["axon-vectors", "axon-api"],
            "source_inputs": inputs,
            "clean_break": true,
            "source_specific_families": registry_families_json(registry),
            "redaction_guardrails": redaction_guardrails(),
            "index_plan": index_plan(registry),
            "examples": payload_examples(registry),
            "api_dtos": VECTOR_API_DTOS
        }
    })
}

fn source_family_conditionals(registry: &StaticVectorPayloadContract) -> Value {
    Value::Array(
        registry
            .source_families
            .iter()
            .map(|(family, allowed_fields)| {
                let forbidden = registry
                    .source_families
                    .iter()
                    .flat_map(|(_, fields)| fields.iter().copied())
                    .filter(|field| !allowed_fields.contains(field))
                    .map(|field| json!({ "required": [field] }))
                    .collect::<Vec<_>>();
                json!({
                    "if": {
                        "properties": { "source_family": { "const": family } },
                        "required": ["source_family"]
                    },
                    "then": {
                        "not": { "anyOf": forbidden }
                    }
                })
            })
            .collect(),
    )
}

fn vector_payload_schema_defs(schemas: &[(&str, Value)]) -> Value {
    let mut defs = schema_defs(schemas, None);
    require_source_range_anchor(&mut defs, "SourceRange");
    require_source_range_anchor(&mut defs, "ChunkLocator_SourceRange");
    defs
}

fn require_source_range_anchor(defs: &mut Value, name: &str) {
    let Some(schema) = defs.get_mut(name).and_then(Value::as_object_mut) else {
        return;
    };
    schema.insert(
        "anyOf".to_string(),
        Value::Array(
            source_range_anchor_fields()
                .map(|(field, schema)| {
                    json!({
                        "required": [field],
                        "properties": {
                            field: schema
                        }
                    })
                })
                .collect(),
        ),
    );
}

fn source_range_anchor_fields() -> impl Iterator<Item = (&'static str, Value)> {
    axon_vectors::payload::SOURCE_RANGE_ANCHOR_FIELDS
        .iter()
        .copied()
        .map(|field| {
            let schema = match field {
                "dom_selector" | "json_pointer" | "yaml_path" | "xml_xpath" | "session_turn_id"
                | "turn_start" | "turn_end" => non_empty_string_schema(),
                _ => non_null_schema(),
            };
            (field, schema)
        })
}

fn non_null_schema() -> Value {
    json!({ "not": { "type": "null" } })
}

fn non_empty_string_schema() -> Value {
    json!({ "type": "string", "minLength": 1 })
}

fn schema_properties(registry: &StaticVectorPayloadContract) -> Value {
    let mut properties = Map::new();
    for field in registry.shared_fields {
        properties.insert((*field).to_string(), shared_field_schema(field));
    }
    for (_, fields) in registry.source_families {
        for field in *fields {
            properties.insert((*field).to_string(), source_specific_field_schema(field));
        }
    }
    Value::Object(properties)
}

fn shared_field_schema(field: &str) -> Value {
    match field {
        "payload_contract_version" => json!({
            "type": "string",
            "const": axon_vectors::payload::VECTOR_PAYLOAD_CONTRACT_VERSION,
            "x-qdrant-index": "keyword"
        }),
        "collection" | "source_id" | "source_item_key" | "document_id" | "chunk_id"
        | "chunk_key" | "content_hash" | "job_id" | "embedding_batch_id" | "document_status"
        | "embedding_model" | "embedding_provider" | "embedding_profile" | "vector_namespace" => {
            json!({ "type": "string", "minLength": 1, "x-qdrant-index": "keyword" })
        }
        "chunk_text" => json!({ "type": "string", "minLength": 1 }),
        "source_family" => {
            json!({
                "type": "string",
                "minLength": 1,
                "enum": VECTOR_SOURCE_FAMILIES,
                "x-qdrant-index": "keyword"
            })
        }
        "source_generation" => {
            json!({ "type": "integer", "minimum": 0, "x-qdrant-index": "integer" })
        }
        "committed_generation" => json!({
            "anyOf": [
                { "type": "integer", "minimum": 0, "x-qdrant-index": "integer" },
                { "type": "null" }
            ],
            "x-qdrant-index": "integer"
        }),
        "embedding_dimensions" => {
            json!({ "type": "integer", "minimum": 1, "x-qdrant-index": "integer" })
        }
        // chunk_index is Integer-typed (require_non_negative_integer);
        // 0 is valid (atomic/first chunk), so minimum 0 not 1.
        "chunk_index" => {
            json!({ "type": "integer", "minimum": 0, "x-qdrant-index": "integer" })
        }
        "redacted_field_count" | "dropped_field_count" | "detector_count" => {
            json!({ "type": "integer", "minimum": 0 })
        }
        "detector_names" => json!({
            "type": "array",
            "items": { "type": "string", "minLength": 1 },
            "uniqueItems": true
        }),
        "chunk_locator" => json!({ "$ref": "#/$defs/ChunkLocator" }),
        "source_range" => json!({ "$ref": "#/$defs/SourceRange" }),
        "visibility" => json!({
            "type": "string",
            "minLength": 1,
            "enum": VECTOR_VISIBILITY_VALUES,
            "x-qdrant-index": "keyword"
        }),
        "redaction_status" => {
            json!({
                "type": "string",
                "minLength": 1,
                "enum": VECTOR_REDACTION_STATUS_VALUES,
                "x-qdrant-index": "keyword"
            })
        }
        "embedded_at" => json!({ "type": "string", "minLength": 1, "format": "date-time" }),
        _ => json!({ "type": "string", "minLength": 1 }),
    }
}

fn redaction_guardrails() -> Value {
    json!({
        "scope": "metadata_and_locator_fields",
        "body_text_field_policy": {
            "field": "chunk_text",
            "allowed_without_metadata_rejection": [
                "ordinary document HTML snippets",
                "ordinary local path examples"
            ],
            "still_rejected": [
                "auth headers",
                "cookies",
                "dotenv-style assignments",
                "bare secret tokens",
                "adapter response markers"
            ]
        },
        "forbidden_field_fragments": FORBIDDEN_FIELD_FRAGMENTS,
        "forbidden_value_fragments": FORBIDDEN_VALUE_FRAGMENTS,
        "bare_secret_token_prefixes": BARE_SECRET_TOKEN_PREFIXES,
        "also_rejects": [
            "dotenv-style assignments",
            "absolute local paths",
            "raw HTML blobs in metadata",
            "adapter response blobs"
        ]
    })
}

fn source_specific_field_schema(field: &str) -> Value {
    match field {
        "web_status_code" | "web_depth" | "session_turn_index" | "memory_importance" => {
            json!({ "type": "integer" })
        }
        "graph_confidence" => json!({ "type": "number" }),
        "manifest" => json!({ "type": "boolean" }),
        "graph_node_ids" | "graph_edge_ids" => {
            json!({ "type": "array", "items": { "type": "string" } })
        }
        _ => json!({ "type": "string" }),
    }
}

fn registry_families_json(registry: &StaticVectorPayloadContract) -> Vec<Value> {
    registry
        .source_families
        .iter()
        .map(|(family, fields)| {
            json!({
                "source_family": family,
                "fields": fields,
            })
        })
        .collect()
}

fn index_plan(registry: &StaticVectorPayloadContract) -> Value {
    let properties = schema_properties(registry);
    let mut indexes = properties
        .as_object()
        .expect("schema_properties must return an object")
        .iter()
        .filter_map(|(field_name, schema)| {
            schema
                .get("x-qdrant-index")
                .and_then(Value::as_str)
                .map(|field_schema| {
                    json!({
                        "field_name": field_name,
                        "field_schema": field_schema,
                    })
                })
        })
        .collect::<Vec<_>>();
    indexes.sort_by(|left, right| {
        left["field_name"]
            .as_str()
            .cmp(&right["field_name"].as_str())
    });
    json!({
        "collection": "axon",
        "indexes": indexes,
    })
}

fn payload_examples(registry: &StaticVectorPayloadContract) -> Vec<Value> {
    registry
        .source_families
        .iter()
        .map(|(family, fields)| {
            let mut payload = BTreeMap::<String, Value>::new();
            for field in registry.required_fields {
                payload.insert((*field).to_string(), required_example_value(field, family));
            }
            payload.insert("source_family".to_string(), json!(family));
            payload.insert(
                "source_item_key".to_string(),
                json!(format!("{family}-item")),
            );
            payload.insert(
                "chunk_key".to_string(),
                json!(format!("{family}-chunk-key")),
            );
            payload.insert(
                "content_hash".to_string(),
                json!(format!("sha256:{family}hash")),
            );
            for field in *fields {
                payload.insert(
                    (*field).to_string(),
                    source_specific_example_value(field, family),
                );
            }
            serde_json::to_value(payload).expect("example payload should serialize")
        })
        .collect()
}

fn required_example_value(field: &str, family: &str) -> Value {
    match field {
        "payload_contract_version" => json!("2026-07-01"),
        "collection" => json!("axon"),
        "source_id" => json!(format!("src-{family}")),
        // Integer-typed per the vector-payload contract
        // (`PayloadFieldSchema::Integer`); never a string.
        "source_generation" => json!(7),
        // Null until a publisher commits the generation (never the string
        // "uncommitted" -- see `axon_vectors::payload::validate_generations`).
        "committed_generation" => Value::Null,
        "document_id" => json!(format!("doc-{family}")),
        "chunk_id" => json!(format!("chunk-{family}-0")),
        "chunk_locator" => json!({
            "canonical_uri": format!("https://example.com/{family}"),
            "path": format!("/{family}"),
            "heading_path": [format!("{family} heading")],
            "symbol": Value::Null,
            "range": source_range_example(),
        }),
        "source_range" => source_range_example(),
        "visibility" => json!("internal"),
        "redaction_status" => json!("clean"),
        "redaction_version" => json!(axon_core::redact::REDACTION_VERSION),
        "redacted_field_count" | "dropped_field_count" | "detector_count" => json!(0),
        "detector_names" => json!([]),
        "job_id" => json!(format!("job-{family}")),
        "document_status" => json!("prepared"),
        "embedding_model" => json!("text-embedding-test"),
        "embedding_dimensions" => json!(768),
        "embedding_provider" => json!("tei"),
        "embedding_profile" => json!("default"),
        "embedded_at" => json!("2026-06-30T00:00:00Z"),
        // Chunk position + chunking descriptors required since the chunking
        // cluster (S2-18/27). chunk_index is Integer-typed (never a string);
        // profile/method are free strings.
        "chunk_index" => json!(0),
        "chunking_profile" => json!("markdown_sections"),
        "chunking_method" => json!("heading_sections"),
        _ => json!(field),
    }
}

fn source_specific_example_value(field: &str, family: &str) -> Value {
    match field {
        "web_title" => json!("Example Docs"),
        "web_domain" => json!("example.com"),
        "web_status_code" => json!(200),
        "web_depth" => json!(1),
        "code_language" => json!("rust"),
        "code_symbol_name" => json!("VectorPayload"),
        "code_symbol_kind" => json!("struct"),
        "code_file_type" => json!("source"),
        "package_ecosystem" => json!("cargo"),
        "package_name" => json!("axon"),
        "package_version" => json!("6.2.1"),
        "session_provider" => json!("codex"),
        "session_id" => json!(format!("session-{family}")),
        "session_turn_index" => json!(3),
        "session_tool_name" => json!("schemas"),
        "session_skill_name" => json!("test-driven-development"),
        "graph_node_ids" => json!(["node-a", "node-b"]),
        "graph_edge_ids" => json!(["edge-a"]),
        "graph_confidence" => json!(0.93),
        "manifest" => json!(true),
        "memory_id" => json!(format!("memory-{family}")),
        "memory_importance" => json!(5),
        "memory_status" => json!("active"),
        _ => json!(field),
    }
}

fn source_range_example() -> Value {
    json!({
        "line_start": 1,
        "line_end": 4,
        "byte_start": Value::Null,
        "byte_end": Value::Null,
        "char_start": Value::Null,
        "char_end": Value::Null,
        "time_start_ms": Value::Null,
        "time_end_ms": Value::Null,
        "dom_selector": Value::Null,
        "json_pointer": Value::Null,
        "yaml_path": Value::Null,
        "xml_xpath": Value::Null,
        "csv_row": Value::Null,
        "session_turn_id": Value::Null,
        "turn_start": Value::Null,
        "turn_end": Value::Null,
    })
}
