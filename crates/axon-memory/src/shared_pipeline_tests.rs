use axon_adapters::{SourceFamily, source_family_matrix};
use axon_retrieval::memory::{matches_memory_source_kind, memory_retrieval_filter};
use serde_json::Value;

use crate::graph::{MEMORY_GRAPH_OPTIONAL_FACTS, memory_graph_candidates};

const FIXTURE: &str = include_str!("../fixtures/shared-pipeline/memory-document.valid.json");
const VECTOR_PAYLOAD_FIXTURE: &str =
    include_str!("../../axon-vectors/tests/fixtures/payload/memory.valid.json");

#[test]
fn memory_matrix_row_is_a_canonical_source_adapter() {
    let matrix = source_family_matrix();
    let memory = matrix
        .iter()
        .find(|spec| spec.family == SourceFamily::Memory)
        .expect("memory source matrix row");

    assert_eq!(memory.adapter, "memory");
    assert!(memory.is_source_adapter);
    assert_eq!(memory.supported_schemes, &["memory"]);
    assert_eq!(memory.shorthand_patterns, &["memory://mem_<id>"]);
    assert_eq!(memory.vector_namespace, "dense");
    assert!(!memory.scopes.is_empty());
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
fn memory_fixture_flows_through_the_canonical_source_pipeline() {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("valid memory fixture");
    let vector_payload: Value =
        serde_json::from_str(VECTOR_PAYLOAD_FIXTURE).expect("valid memory vector payload fixture");

    assert_eq!(fixture["source_family"], "memory");
    assert_eq!(fixture["vector_namespace"], "dense");
    assert_eq!(vector_payload["source_family"], "memory");
    assert_eq!(vector_payload["vector_namespace"], "dense");

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
        fixture["retrieval_filter"]["source_kind"],
        retrieval_filter.source_kind
    );
    assert!(matches_memory_source_kind(
        fixture["retrieval_filter"]["source_kind"]
            .as_str()
            .expect("retrieval source kind")
    ));

    assert_eq!(fixture["source_adapter_row"], true);
    assert_eq!(fixture["source_ledger_generation"], 1);
}
