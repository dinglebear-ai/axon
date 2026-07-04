use axon_api::source::*;
use serde_json::json;

use crate::payload::{VectorPayload, VectorPayloadValidationError};
use crate::point::VectorPointBatchBuilder;
use crate::testing::{
    test_collection_spec, test_embedding_result_for, test_prepared_document,
    test_vector_build_context,
};

fn builder(
    collection: CollectionSpec,
    document: PreparedDocument,
    embeddings: EmbeddingResult,
) -> VectorPointBatchBuilder {
    VectorPointBatchBuilder::new(
        collection,
        document,
        embeddings,
        test_vector_build_context(),
    )
}

fn local_document() -> PreparedDocument {
    let mut document = test_prepared_document();
    let canonical_uri = "file://local/fnv1a64:7d0f9a22/docs/README.md";

    document.document_id = DocumentId::new("doc_local_readme");
    document.source_id = SourceId::new("src_local_workspace");
    document.source_item_key = SourceItemKey::new("docs/README.md");
    document.generation = SourceGenerationId::new("gen_local_0001");
    document.canonical_uri = canonical_uri.to_string();
    document.metadata.remove("web_title");
    document.metadata.remove("web_domain");
    document.metadata.remove("web_status_code");
    document.metadata.remove("web_depth");
    document
        .metadata
        .insert("source_family".into(), json!("code"));
    document
        .metadata
        .insert("source_kind".into(), json!("local"));
    document
        .metadata
        .insert("source_adapter".into(), json!("local"));
    document
        .metadata
        .insert("source_scope".into(), json!("directory"));
    document
        .metadata
        .insert("code_file_type".into(), json!("markdown"));

    for chunk in &mut document.chunks {
        chunk.document_id = document.document_id.clone();
        chunk.chunk_locator.canonical_uri = canonical_uri.to_string();
        chunk.chunk_locator.path = Some("docs/README.md".to_string());
    }

    document
}

#[test]
fn local_payload_includes_target_source_lineage_fields() {
    let document = local_document();
    let expected_canonical_uri = document.canonical_uri.clone();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);

    let batch = builder(test_collection_spec(3), document, embeddings)
        .build()
        .unwrap();
    let payload = &batch.points[0].payload;

    assert_eq!(payload["source_id"], "src_local_workspace");
    assert_eq!(payload["source_kind"], "local");
    assert_eq!(payload["source_adapter"], "local");
    assert_eq!(payload["source_scope"], "directory");
    assert_eq!(payload["source_generation"], "gen_local_0001");
    assert_eq!(payload["committed_generation"], "uncommitted");
    assert_eq!(payload["source_item_key"], "docs/README.md");
    assert_eq!(payload["item_canonical_uri"], expected_canonical_uri);
    assert_eq!(
        payload["chunk_locator"]["canonical_uri"],
        expected_canonical_uri
    );
    assert_eq!(payload["document_id"], "doc_local_readme");
    assert_eq!(payload["chunk_id"], "chunk-web-1");
    assert_eq!(payload["job_id"], uuid::Uuid::from_u128(43).to_string());
}

#[test]
fn local_payload_requires_shared_local_lineage_fields() {
    let document = local_document();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);
    let batch = builder(test_collection_spec(3), document, embeddings)
        .build()
        .unwrap();
    let valid_payload = batch.points[0].payload.clone();

    for field in [
        "source_kind",
        "source_adapter",
        "source_scope",
        "source_item_key",
        "item_canonical_uri",
    ] {
        let mut payload = valid_payload.clone();
        payload.remove(field);

        let err = VectorPayload::try_from_metadata(payload).unwrap_err();

        assert!(matches!(
            err,
            VectorPayloadValidationError::MissingRequiredField { field: missing } if missing == field
        ));
    }
}

#[test]
fn point_build_never_marks_generation_committed_before_publish() {
    let mut document = local_document();
    document
        .metadata
        .insert("committed_generation".to_string(), json!("gen_local_0001"));
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);

    let batch = builder(test_collection_spec(3), document, embeddings)
        .build()
        .unwrap();

    assert_eq!(
        batch.points[0].payload["committed_generation"],
        "uncommitted"
    );
}

#[test]
fn local_payload_does_not_leak_absolute_home_paths() {
    let document = local_document();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);

    let batch = builder(test_collection_spec(3), document, embeddings)
        .build()
        .unwrap();
    let payload_json = serde_json::to_string(&batch.points[0].payload).unwrap();

    assert!(!payload_json.contains("/home/jmagar"));
    if let Ok(home) = std::env::var("HOME") {
        assert!(!home.is_empty());
        assert!(!payload_json.contains(&home));
    }
}
