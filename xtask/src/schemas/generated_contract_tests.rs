use std::path::Path;

use axon_vectors::payload::{
    BARE_SECRET_TOKEN_PREFIXES, FORBIDDEN_FIELD_FRAGMENTS, FORBIDDEN_VALUE_FRAGMENTS,
    VECTOR_PAYLOAD_CONTRACT_VERSION, VECTOR_REQUIRED_FIELDS, VECTOR_VISIBILITY_VALUES,
};
use jsonschema::validator_for;

use super::{fixture_repo, generate};

fn generated_json(root: &Path, path: &str) -> serde_json::Value {
    let content = std::fs::read_to_string(root.join(path)).unwrap();
    serde_json::from_str(&content).unwrap()
}

fn payload_required_fields_from_source() -> Vec<String> {
    VECTOR_REQUIRED_FIELDS
        .iter()
        .map(|field| (*field).to_string())
        .collect()
}

#[test]
fn adapter_schema_family_emits_source_family_matrix_artifact() {
    let tmp = fixture_repo();
    generate(tmp.path()).unwrap();

    let value = generated_json(tmp.path(), "docs/reference/sources/adapter-scopes.json");
    let matrix = value["x-axon"]["source_family_matrix"]
        .as_array()
        .expect("source family matrix records");

    assert!(
        tmp.path()
            .join("docs/reference/sources/adapter-scopes.md")
            .exists()
    );
    assert!(
        matrix
            .iter()
            .any(|record| record["adapter"] == "cli_tool" && record["may_execute_tools"] == true)
    );
    assert!(
        matrix
            .iter()
            .any(|record| record["adapter"] == "mcp_tool" && record["may_execute_tools"] == true)
    );
    assert!(matrix.iter().any(|record| record["adapter"] == "memory"
        && record["vector_namespace"] == "dense"
        && record["is_source_adapter"] == true));
}

#[test]
fn generated_adapter_capability_artifact_contains_tool_and_memory_contracts() {
    let artifact = super::families::adapters::generate_adapter_capability_artifact().unwrap();
    let value: serde_json::Value = serde_json::from_str(&artifact.content).unwrap();
    let matrix = value["x-axon"]["source_family_matrix"]
        .as_array()
        .expect("source family matrix records");

    assert!(matrix.iter().any(|record| record["adapter"] == "cli_tool"));
    assert!(matrix.iter().any(|record| record["adapter"] == "mcp_tool"));
    assert!(matrix.iter().any(|record| record["adapter"] == "memory"));
    assert!(
        matrix
            .iter()
            .any(|record| record["may_execute_tools"] == true)
    );
}

#[test]
fn generated_json_contains_source_input_checksums_and_canonical_enums() {
    let tmp = fixture_repo();
    generate(tmp.path()).unwrap();
    let value = generated_json(tmp.path(), "docs/reference/api/schemas.json");

    let inputs = value["x-axon"]["source_inputs"].as_array().unwrap();
    assert!(inputs.iter().any(|input| {
        input["path"] == "crates/axon-api/src/source.rs"
            && input["kind"] == "rust_module"
            && input["checksum"]
                .as_str()
                .unwrap()
                .strip_prefix("sha256:")
                .unwrap()
                .len()
                == 64
    }));
    for path in [
        "crates/axon-api/src/source/document.rs",
        "crates/axon-api/src/source/boundary.rs",
        "crates/axon-api/src/source/graph.rs",
        "crates/axon-api/src/source/ids.rs",
        "crates/axon-api/src/source/listing.rs",
        "crates/axon-api/src/source/job_listing.rs",
        "crates/axon-api/src/source/state.rs",
        "crates/axon-api/src/source/status.rs",
        "crates/axon-api/src/source/vector.rs",
    ] {
        assert!(
            inputs.iter().any(|input| input["path"] == path),
            "{path} should be tracked as an API source input"
        );
    }
    for def in [
        "SourceGeneration",
        "PublishGenerationRequest",
        "CleanupDebt",
        "LeaseRequest",
        "LeaseGuard",
        "CleanupSelector",
        "DocumentStatus",
        "SourceDocument",
        "PreparedDocument",
        "PreparedChunk",
        "ChunkLocator",
        "SourceParseFacts",
        "GraphCandidate",
        "GraphEvidence",
        "EmbeddingBatch",
        "EmbeddingInput",
        "JobCreateRequest",
        "JobDescriptor",
        "JobSummary",
        "SourceJobStatus",
        "JobEvent",
        "JobHeartbeat",
        "JobCancelRequest",
        "JobRetryRequest",
        "JobRecoveryRequest",
        "JobCleanupRequest",
        "JobArtifactListResult",
    ] {
        assert!(
            value["$defs"].get(def).is_some(),
            "{def} should be emitted in the API schema bundle"
        );
    }
    assert_eq!(
        value["$defs"]["enums"]["SourceKind"]["enum"][0],
        serde_json::json!("web")
    );
    assert_eq!(
        value["$defs"]["enums"]["PipelinePhase"]["enum"][1],
        serde_json::json!("requested")
    );
}

#[test]
fn generated_vector_payload_schema_includes_registered_required_fields() {
    let tmp = fixture_repo();
    generate(tmp.path()).unwrap();

    let value = generated_json(
        tmp.path(),
        "docs/reference/sources/vector-payload.schema.json",
    );
    let required = value["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    let expected = payload_required_fields_from_source();

    assert_eq!(required, expected);
    assert_eq!(
        value["properties"]["visibility"]["enum"],
        serde_json::json!(VECTOR_VISIBILITY_VALUES)
    );
    assert_eq!(
        value["properties"]["payload_contract_version"]["const"],
        serde_json::json!(VECTOR_PAYLOAD_CONTRACT_VERSION)
    );
    assert_eq!(
        value["properties"]["source_generation"]["type"],
        serde_json::json!("integer")
    );
    assert_eq!(
        value["properties"]["source_generation"]["x-qdrant-index"],
        serde_json::json!("integer")
    );
    assert!(
        value["properties"]["committed_generation"]["anyOf"]
            .as_array()
            .unwrap()
            .iter()
            .any(|schema| schema["type"] == "null")
    );
    assert_eq!(
        value["properties"]["embedding_batch_id"]["x-qdrant-index"],
        serde_json::json!("keyword")
    );
    assert!(value["x-axon"]["redaction_guardrails"].is_object());
    assert!(value["$defs"]["SourceRange"].get("anyOf").is_some());
    assert!(
        value["allOf"]
            .as_array()
            .unwrap()
            .iter()
            .any(|conditional| conditional["if"]["properties"]["source_family"]["const"] == "web")
    );
    let api_dtos = value["x-axon"]["api_dtos"].as_array().unwrap();
    for dto in [
        "EmbeddingResult",
        "EmbeddingVector",
        "SparseVector",
        "VectorDeleteSelector",
        "VectorStoreDeleteResult",
        "VectorSearchMatch",
    ] {
        assert!(
            api_dtos.iter().any(|value| value.as_str() == Some(dto)),
            "{dto} should be listed in vector payload DTO coverage"
        );
    }
}

#[test]
fn generated_vector_payload_schema_rejects_runtime_invalid_family_and_range_shapes() {
    let tmp = fixture_repo();
    generate(tmp.path()).unwrap();

    let value = generated_json(
        tmp.path(),
        "docs/reference/sources/vector-payload.schema.json",
    );
    let validator = validator_for(&value).unwrap();
    let mut payload = value["x-axon"]["examples"][0].clone();
    payload["payload_contract_version"] = serde_json::json!(20260701);
    assert!(validator.validate(&payload).is_err());

    let mut payload = value["x-axon"]["examples"][0].clone();
    payload["payload_contract_version"] = serde_json::json!("2026-06-01");
    assert!(validator.validate(&payload).is_err());

    let mut payload = value["x-axon"]["examples"][0].clone();
    payload["embedding_dimensions"] = serde_json::json!(0);
    assert!(validator.validate(&payload).is_err());

    let mut payload = value["x-axon"]["examples"][0].clone();
    payload["collection"] = serde_json::json!("");
    assert!(validator.validate(&payload).is_err());

    let mut payload = value["x-axon"]["examples"][0].clone();
    payload["source_range"] = serde_json::json!({});
    assert!(validator.validate(&payload).is_err());

    let mut payload = value["x-axon"]["examples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|example| example["source_family"] == "web")
        .unwrap()
        .clone();
    payload["code_language"] = serde_json::json!("rust");
    assert!(validator.validate(&payload).is_err());
}

#[test]
fn generated_vector_payload_index_plan_references_only_schema_fields() {
    let tmp = fixture_repo();
    generate(tmp.path()).unwrap();

    let value = generated_json(
        tmp.path(),
        "docs/reference/sources/vector-payload.schema.json",
    );
    let properties = value["properties"].as_object().unwrap();
    let indexes = value["x-axon"]["index_plan"]["indexes"].as_array().unwrap();

    assert!(!indexes.is_empty(), "index plan should not be empty");
    for index in indexes {
        let field_name = index["field_name"].as_str().unwrap();
        assert!(
            properties.contains_key(field_name),
            "index field {field_name} must exist in schema properties"
        );
    }
}

#[test]
fn generated_vector_payload_index_plan_matches_schema_index_annotations() {
    let tmp = fixture_repo();
    generate(tmp.path()).unwrap();

    let value = generated_json(
        tmp.path(),
        "docs/reference/sources/vector-payload.schema.json",
    );
    let properties = value["properties"].as_object().unwrap();
    let mut annotated = properties
        .iter()
        .filter_map(|(field_name, schema)| {
            schema
                .get("x-qdrant-index")
                .and_then(serde_json::Value::as_str)
                .map(|field_schema| {
                    serde_json::json!({
                        "field_name": field_name,
                        "field_schema": field_schema,
                    })
                })
        })
        .collect::<Vec<_>>();
    let mut planned = value["x-axon"]["index_plan"]["indexes"]
        .as_array()
        .unwrap()
        .clone();

    annotated.sort_by(|left, right| {
        left["field_name"]
            .as_str()
            .cmp(&right["field_name"].as_str())
    });
    planned.sort_by(|left, right| {
        left["field_name"]
            .as_str()
            .cmp(&right["field_name"].as_str())
    });

    assert_eq!(planned, annotated);
    assert!(
        planned
            .iter()
            .any(|index| index["field_name"] == "redaction_status")
    );
    assert!(
        planned
            .iter()
            .any(|index| index["field_name"] == "vector_namespace")
    );
}

#[test]
fn generated_vector_payload_redaction_guardrails_match_runtime_constants() {
    let tmp = fixture_repo();
    generate(tmp.path()).unwrap();

    let value = generated_json(
        tmp.path(),
        "docs/reference/sources/vector-payload.schema.json",
    );
    let guardrails = &value["x-axon"]["redaction_guardrails"];

    assert_eq!(
        guardrails["forbidden_field_fragments"],
        serde_json::json!(FORBIDDEN_FIELD_FRAGMENTS)
    );
    assert_eq!(
        guardrails["forbidden_value_fragments"],
        serde_json::json!(FORBIDDEN_VALUE_FRAGMENTS)
    );
    assert_eq!(
        guardrails["bare_secret_token_prefixes"],
        serde_json::json!(BARE_SECRET_TOKEN_PREFIXES)
    );
}

#[test]
fn generated_vector_payload_examples_validate_against_the_builder_registry() {
    let tmp = fixture_repo();
    generate(tmp.path()).unwrap();

    let value = generated_json(
        tmp.path(),
        "docs/reference/sources/vector-payload.schema.json",
    );
    let required = payload_required_fields_from_source();
    let examples = value["x-axon"]["examples"].as_array().unwrap();

    assert!(!examples.is_empty(), "payload examples should be emitted");
    for example in examples {
        let family = example["source_family"].as_str().unwrap();
        let object = example.as_object().unwrap();
        for field in &required {
            assert!(
                object.contains_key(field),
                "example for {family} missing required field {field}"
            );
        }
        let metadata = axon_api::source::MetadataMap(object.clone().into_iter().collect());
        axon_vectors::payload::VectorPayload::try_from_metadata(metadata)
            .unwrap_or_else(|err| panic!("example for {family} must validate: {err}"));
    }
}

#[test]
fn generated_vector_payload_source_inputs_cover_builder_contract_and_api_vector_dtos() {
    let tmp = fixture_repo();
    generate(tmp.path()).unwrap();

    let value = generated_json(
        tmp.path(),
        "docs/reference/sources/vector-payload.schema.json",
    );
    let inputs = value["x-axon"]["source_inputs"].as_array().unwrap();

    for path in [
        "crates/axon-vectors/src/payload.rs",
        "crates/axon-vectors/src/payload_families.rs",
        "crates/axon-vectors/src/point.rs",
        "crates/axon-api/src/source/vector.rs",
        "docs/pipeline-unification/sources/metadata-payload.md",
        "docs/pipeline-unification/sources/chunking-contract.md",
        "docs/pipeline-unification/schemas/vector-payload-schema.md",
    ] {
        assert!(
            inputs.iter().any(|input| input["path"] == path),
            "{path} should be tracked as a vector payload source input"
        );
    }
}

#[test]
fn generated_api_schema_and_docs_include_vector_store_dtos() {
    let tmp = fixture_repo();
    generate(tmp.path()).unwrap();

    let schema = generated_json(tmp.path(), "docs/reference/api/schemas.json");
    for def in [
        "VectorPointBatch",
        "VectorPoint",
        "PayloadIndexSpec",
        "CollectionSpec",
        "VectorConfig",
        "SparseVector",
        "SparseVectorConfig",
        "VectorStoreDeleteResult",
        "VectorSearchRequest",
        "VectorSearchResult",
        "VectorSearchMatch",
    ] {
        assert!(
            schema["$defs"].get(def).is_some(),
            "{def} should be emitted in the API schema bundle"
        );
    }

    let markdown = std::fs::read_to_string(tmp.path().join("docs/reference/api/dto.md")).unwrap();
    assert!(markdown.contains("VectorPointBatch"));
    assert!(markdown.contains("CollectionSpec"));
}
