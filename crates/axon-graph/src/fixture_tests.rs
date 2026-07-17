use axon_api::source::{
    ChunkId, DocumentId, GraphCandidate, GraphCandidateProducer, GraphEdgeCandidate, GraphEvidence,
    GraphNodeCandidate, JobId, MetadataMap, SourceId, SourceItemKey, SourceRange,
};
use axon_vectors::payload::VectorPayload;
use uuid::Uuid;

use crate::candidate::validate_candidate;

#[test]
fn ledger_vector_graph_fixture_shares_source_generation_lineage() {
    let source_id = SourceId::from("src_local_repo");
    // Integer-typed per the vector-payload contract (source_generation /
    // committed_generation are `PayloadFieldSchema::Integer`).
    let generation: i64 = 7;
    let document_id = DocumentId::from("doc_Dockerfile");
    let chunk_id = ChunkId::from("chunk_Dockerfile_1");
    let job_id = JobId::new(Uuid::from_u128(7));

    let vector_payload = VectorPayload::try_from_metadata(metadata(serde_json::json!({
        "payload_contract_version": "2026-07-01",
        "collection": "axon",
        "vector_point_id": "point_1",
        "vector_namespace": "source",
        "source_family": "local",
        "source_kind": "local",
        "source_adapter": "local",
        "source_scope": "repo",
        "source_id": source_id.0,
        "source_canonical_uri": "file:///repo",
        "source_generation": generation,
        "committed_generation": generation,
        "source_item_key": "Dockerfile",
        "item_canonical_uri": "file:///repo/Dockerfile",
        "document_id": document_id.0,
        "chunk_id": chunk_id.0,
        "chunk_index": 0,
        "chunking_profile": "structured_records",
        "chunking_method": "structured_records",
        "content_kind": "structured",
        "chunk_content_kind": "structured",
        "content_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "chunk_hash": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "chunk_text": "Dockerfile image qdrant/qdrant:v1.13.1 exposes 6333",
        "chunk_locator": {
            "canonical_uri": "file:///repo/Dockerfile",
            "path": "Dockerfile",
            "heading_path": [],
            "symbol": null,
            "range": { "line_start": 1, "line_end": 2 }
        },
        "source_range": { "line_start": 1, "line_end": 2 },
        "redaction_status": "clean",
        "redaction_version": "2026-07-16",
        "redacted_field_count": 0,
        "dropped_field_count": 0,
        "detector_count": 0,
        "detector_names": [],
        "visibility": "public",
        "job_id": job_id.0.to_string(),
        "document_status": "published",
        "embedding_model": "fake",
        "embedding_dimensions": 8,
        "embedding_provider": "fake",
        "embedding_profile": "test",
        "embedded_at": "2026-07-04T00:00:00Z",
        "local_checkout": "local://src_local_repo"
    })))
    .expect("vector payload validates through production validator");

    assert_eq!(vector_payload.metadata()["source_id"], source_id.0);
    assert_eq!(vector_payload.metadata()["source_generation"], generation);
    assert_eq!(
        vector_payload.metadata()["committed_generation"],
        generation
    );
    assert_eq!(vector_payload.metadata()["document_id"], document_id.0);
    assert_eq!(vector_payload.metadata()["chunk_id"], chunk_id.0);

    let candidate = GraphCandidate {
        candidate_id: "cand_container_image".to_string(),
        job_id,
        source_id: source_id.clone(),
        source_item_key: SourceItemKey::from("Dockerfile"),
        item_canonical_uri: "file:///repo/Dockerfile".to_string(),
        document_id: Some(document_id.clone()),
        kind: "container_manifest".to_string(),
        merge_key: Some("container_manifest:file:///repo/Dockerfile:qdrant".to_string()),
        producer: GraphCandidateProducer {
            adapter: "axon-parse".to_string(),
            parser: Some("docker_manifest".to_string()),
            version: "test".to_string(),
        },
        nodes: vec![
            GraphNodeCandidate {
                node_kind: "local_checkout".to_string(),
                stable_key: "local://src_local_repo".to_string(),
                label: "src_local_repo".to_string(),
                properties: MetadataMap::new(),
            },
            GraphNodeCandidate {
                node_kind: "container_image_tag".to_string(),
                stable_key: "docker:qdrant/qdrant:v1.13.1".to_string(),
                label: "qdrant/qdrant:v1.13.1".to_string(),
                properties: MetadataMap::new(),
            },
        ],
        edges: vec![GraphEdgeCandidate {
            edge_kind: "repo_uses_container_image".to_string(),
            from_stable_key: "local://src_local_repo".to_string(),
            to_stable_key: "docker:qdrant/qdrant:v1.13.1".to_string(),
            evidence_ids: vec!["ev_1".to_string()],
            properties: MetadataMap::new(),
        }],
        evidence: vec![GraphEvidence {
            evidence_id: "ev_1".to_string(),
            evidence_kind: "container_manifest".to_string(),
            source_id,
            source_item_key: SourceItemKey::from("Dockerfile"),
            document_id: Some(document_id),
            chunk_id: Some(chunk_id),
            range: Some(SourceRange {
                line_start: Some(1),
                line_end: Some(1),
                byte_start: None,
                byte_end: None,
                char_start: None,
                char_end: None,
                time_start_ms: None,
                time_end_ms: None,
                dom_selector: None,
                json_pointer: None,
                yaml_path: None,
                xml_xpath: None,
                csv_row: None,
                session_turn_id: None,
                turn_start: None,
                turn_end: None,
            }),
            quote: Some("FROM qdrant/qdrant:v1.13.1".to_string()),
            confidence: 0.9,
            metadata: MetadataMap::new(),
        }],
        confidence: 0.9,
        metadata: MetadataMap::new(),
    };

    validate_candidate(&candidate).expect("fixture candidate validates");
}

fn metadata(value: serde_json::Value) -> MetadataMap {
    MetadataMap(
        value
            .as_object()
            .expect("fixture metadata object")
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    )
}
