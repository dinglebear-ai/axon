use axon_adapters::{SourceFamily, source_family_matrix};
use axon_retrieval::memory::{
    MEMORY_VECTOR_NAMESPACE, matches_memory_namespace, memory_retrieval_filter,
};
use serde_json::Value;

use crate::graph::{MEMORY_GRAPH_OPTIONAL_FACTS, memory_graph_candidates};

const FIXTURE: &str = include_str!("../fixtures/shared-pipeline/memory-document.valid.json");
const VECTOR_PAYLOAD_FIXTURE: &str =
    include_str!("../../axon-vectors/tests/fixtures/payload/memory.valid.json");

#[test]
fn memory_integration_matrix_row_stays_distinct_from_source_adapters() {
    let matrix = source_family_matrix();
    let memory = matrix
        .iter()
        .find(|spec| spec.family == SourceFamily::MemoryIntegration)
        .expect("memory integration matrix row");

    assert_eq!(memory.adapter, "memory");
    assert!(!memory.is_source_adapter);
    assert_eq!(memory.supported_schemes, &[] as &[&str]);
    assert_eq!(memory.shorthand_patterns, &[] as &[&str]);
    assert_eq!(memory.vector_namespace, MEMORY_VECTOR_NAMESPACE);
    assert!(memory.scopes.is_empty());
    assert!(memory.credential_requirements.is_empty());
    assert!(memory.metadata_families.contains(&"memory"));
    assert!(
        memory
            .required_graph_fact_kinds
            .contains(&"memory_document")
    );
    assert_eq!(
        memory.optional_graph_fact_kinds,
        MEMORY_GRAPH_OPTIONAL_FACTS
    );
    assert!(!memory.may_execute_tools);
}

#[test]
fn memory_fixture_flows_through_shared_pipeline_contract_without_source_ledger() {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("valid memory fixture");
    let vector_payload: Value =
        serde_json::from_str(VECTOR_PAYLOAD_FIXTURE).expect("valid memory vector payload fixture");

    assert_eq!(fixture["source_family"], "memory");
    assert_eq!(fixture["vector_namespace"], MEMORY_VECTOR_NAMESPACE);
    assert_eq!(vector_payload["source_family"], "memory");
    assert_eq!(vector_payload["vector_namespace"], MEMORY_VECTOR_NAMESPACE);

    let chunks = fixture["prepared_chunks"]
        .as_array()
        .expect("prepared chunks");
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0]["content_kind"], "markdown");

    let graph_candidates =
        memory_graph_candidates(fixture["memory_id"].as_str().expect("memory id"));
    assert_eq!(graph_candidates.len(), 1);
    assert_eq!(graph_candidates[0].fact_kind, "memory_document");
    assert_eq!(
        fixture["graph"]["required_fact_kinds"][0],
        graph_candidates[0].fact_kind
    );

    let retrieval_filter = memory_retrieval_filter();
    assert_eq!(
        fixture["retrieval_filter"]["namespace"],
        retrieval_filter.vector_namespace
    );
    assert!(matches_memory_namespace(
        fixture["retrieval_filter"]["namespace"]
            .as_str()
            .expect("retrieval namespace")
    ));

    assert_eq!(fixture["source_adapter_row"], false);
    assert!(fixture["source_ledger_generation"].is_null());
}
