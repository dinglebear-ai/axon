use axon_api::source::*;
use serde_json::json;

use crate::point::{VectorPointBatchBuildError, VectorPointBatchBuilder};
use crate::testing::{
    test_collection_spec, test_embedding_result_for, test_embedding_result_with_vectors,
    test_prepared_document, test_vector_build_context,
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

#[test]
fn prepared_document_and_embeddings_build_validated_points() {
    let document = test_prepared_document();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);
    let batch = builder(test_collection_spec(3), document, embeddings)
        .build()
        .unwrap();

    assert_eq!(batch.collection, "axon-test");
    assert_eq!(batch.model, "text-embedding-test");
    assert_eq!(batch.dimensions, 3);
    assert_eq!(batch.points.len(), 2);
    assert_eq!(batch.points[0].chunk_id, ChunkId::new("chunk-web-1"));
    assert_eq!(batch.points[0].vector, vec![1.0, 2.0, 3.0]);
    assert_eq!(batch.points[0].payload["collection"], "axon-test");
    assert_eq!(batch.points[0].payload["source_id"], "src-web");
    assert_eq!(
        batch.points[0].payload["source_item_key"],
        "https://example.com/docs"
    );
    // `source_generation` is integer-typed per the vector-payload contract;
    // `committed_generation` is null until a later publish step commits it
    // (see `axon_vectors::point`'s point builder).
    assert_eq!(batch.points[0].payload["source_generation"], json!(7));
    assert_eq!(
        batch.points[0].payload["committed_generation"],
        serde_json::Value::Null
    );
    assert_eq!(batch.points[0].payload["chunk_id"], "chunk-web-1");
    assert_eq!(
        batch.points[0].payload["job_id"],
        uuid::Uuid::from_u128(43).to_string()
    );
    assert_eq!(
        batch.points[0].payload["embedding_batch_id"],
        uuid::Uuid::from_u128(42).to_string()
    );
    assert_eq!(batch.points[0].payload["chunk_key"], "chunk-web-1");
    assert_eq!(batch.points[0].payload["content_hash"], "hash-0");
    assert_eq!(batch.points[0].payload["content_kind"], "markdown");
    assert_eq!(batch.points[0].payload["vector_namespace"], "dense");
    assert_eq!(batch.points[0].payload["chunk_text"], "chunk-web-1 content");
    assert!(batch.points[0].payload["source_range"].is_object());
    assert_eq!(batch.payload_indexes.len(), 3);
}

#[test]
fn absolute_local_chunk_locator_paths_skip_the_chunk() {
    let mut document = test_prepared_document();
    document.chunks[0].chunk_locator.path = Some("/home/jmagar/workspace/private.rs".to_string());
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);

    // A `ForbiddenValue` (absolute local path in the locator) is a per-chunk
    // concern: the secret-bearing chunk is skipped (not indexed), and the rest
    // of the document still builds. The whole source must not fail.
    let batch = builder(test_collection_spec(3), document, embeddings)
        .build()
        .unwrap();

    assert_eq!(batch.points.len(), 1);
    assert_eq!(batch.points[0].chunk_id, ChunkId::new("chunk-web-2"));
}

#[test]
fn preparer_internal_chunk_metadata_is_not_copied_into_vector_payload() {
    let mut document = test_prepared_document();
    // A stray per-chunk copy of these fields (as the preparer used to stamp
    // before the document-level `chunking_profile`/`chunking_method` were
    // exposed in the payload) must not leak through -- the payload's
    // authoritative values come from `PreparedDocument`, not `chunk.metadata`.
    document.chunks[0].metadata.insert(
        "chunking_profile".to_string(),
        json!("stray-per-chunk-value"),
    );
    document.chunks[0].metadata.insert(
        "chunking_method".to_string(),
        json!("stray-per-chunk-value"),
    );
    document.chunks[0]
        .metadata
        .insert("preparer_version".to_string(), json!("2026-07-01"));
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);
    let expected_profile = document.chunking_profile.clone();
    let expected_method = document.chunking_method.clone();

    let batch = builder(test_collection_spec(3), document, embeddings)
        .build()
        .unwrap();

    // Document-level `chunking_profile`/`chunking_method` DO appear in the
    // payload (S2-18/S2-27), distinct from `embedding_profile` -- but always
    // the document's authoritative values, never the stray chunk copy.
    assert_eq!(
        batch.points[0].payload["chunking_profile"],
        expected_profile
    );
    assert_eq!(batch.points[0].payload["chunking_method"], expected_method);
    assert_ne!(
        batch.points[0].payload["chunking_profile"],
        json!("stray-per-chunk-value")
    );
    assert!(!batch.points[0].payload.contains_key("preparer_version"));
}

#[test]
fn vector_payload_carries_chunk_index_and_a_distinct_embedding_profile() {
    let document = test_prepared_document();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);

    let batch = builder(test_collection_spec(3), document.clone(), embeddings)
        .build()
        .unwrap();

    assert_eq!(batch.points[0].payload["chunk_index"], json!(0));
    assert_eq!(
        batch.points[0].payload["chunking_profile"],
        document.chunking_profile
    );
    // `embedding_profile` is a distinct identity, not a copy of the chunking
    // profile.
    assert_ne!(
        batch.points[0].payload["embedding_profile"],
        json!(document.chunking_profile)
    );
}

#[test]
fn embedding_chunk_mismatch_fails_without_partial_batch() {
    let document = test_prepared_document();
    let embeddings = test_embedding_result_with_vectors(
        "text-embedding-test",
        3,
        vec![
            ("chunk-web-1", vec![1.0, 2.0, 3.0]),
            ("chunk-web-missing", vec![4.0, 5.0, 6.0]),
        ],
    );

    let err = builder(test_collection_spec(3), document, embeddings)
        .build()
        .unwrap_err();
    assert_eq!(
        err,
        VectorPointBatchBuildError::UnexpectedEmbeddingChunk {
            chunk_id: ChunkId::new("chunk-web-missing")
        }
    );
}

#[test]
fn duplicate_chunk_ids_fail() {
    let mut document = test_prepared_document();
    document.chunks[1].chunk_id = document.chunks[0].chunk_id.clone();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);

    let err = builder(test_collection_spec(3), document, embeddings)
        .build()
        .unwrap_err();
    assert_eq!(
        err,
        VectorPointBatchBuildError::DuplicateChunkId {
            chunk_id: ChunkId::new("chunk-web-1")
        }
    );
}

#[test]
fn embedding_result_model_must_match_document_embedding_provenance() {
    let document = test_prepared_document();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);
    let first = builder(test_collection_spec(3), document.clone(), embeddings)
        .build()
        .unwrap();

    let same_model = test_embedding_result_for(&document, "text-embedding-test", 3);
    let second = builder(test_collection_spec(3), document.clone(), same_model)
        .build()
        .unwrap();

    let mut other_model_document = document.clone();
    other_model_document.metadata.insert(
        "embedding_model".to_string(),
        json!("other-embedding-model"),
    );
    let other_model = test_embedding_result_for(&other_model_document, "other-embedding-model", 3);
    let changed = builder(test_collection_spec(3), document, other_model)
        .build()
        .unwrap_err();

    assert_eq!(first.points[0].point_id, second.points[0].point_id);
    assert!(matches!(
        changed,
        VectorPointBatchBuildError::EmbeddingModelMismatch { .. }
    ));
}

#[test]
fn point_ids_stay_stable_across_embedding_model_when_generation_is_same() {
    let document = test_prepared_document();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);
    let first = builder(test_collection_spec(3), document.clone(), embeddings)
        .build()
        .unwrap();

    let mut other_model_document = document.clone();
    other_model_document.metadata.insert(
        "embedding_model".to_string(),
        json!("other-embedding-model"),
    );
    let other_model = test_embedding_result_for(&other_model_document, "other-embedding-model", 3);
    let second = builder(test_collection_spec(3), other_model_document, other_model)
        .build()
        .unwrap();

    assert_eq!(first.points[0].point_id, second.points[0].point_id);
    assert_ne!(
        first.points[0].payload["embedding_model"],
        second.points[0].payload["embedding_model"]
    );
}

#[test]
fn point_ids_include_collection_namespace_document_chunk_model_and_generation() {
    let document = test_prepared_document();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);
    let baseline = builder(
        test_collection_spec(3),
        document.clone(),
        embeddings.clone(),
    )
    .build()
    .unwrap()
    .points
    .remove(0)
    .point_id;

    let mut other_collection = test_collection_spec(3);
    other_collection.collection = "other-collection".to_string();
    assert_ne!(
        baseline,
        builder(other_collection, document.clone(), embeddings.clone())
            .build()
            .unwrap()
            .points[0]
            .point_id
    );

    let mut other_namespace = test_collection_spec(3);
    other_namespace.dense.name = "dense-code".to_string();
    assert_ne!(
        baseline,
        builder(other_namespace, document.clone(), embeddings.clone())
            .build()
            .unwrap()
            .points[0]
            .point_id
    );

    let mut other_document = document.clone();
    other_document.document_id = DocumentId::new("doc-other");
    assert_ne!(
        baseline,
        builder(test_collection_spec(3), other_document, embeddings.clone())
            .build()
            .unwrap()
            .points[0]
            .point_id
    );

    let mut other_chunk = document.clone();
    other_chunk.chunks[0].chunk_id = ChunkId::new("chunk-other");
    let other_chunk_embeddings = test_embedding_result_for(&other_chunk, "text-embedding-test", 3);
    assert_ne!(
        baseline,
        builder(test_collection_spec(3), other_chunk, other_chunk_embeddings)
            .build()
            .unwrap()
            .points[0]
            .point_id
    );

    let mut other_generation = document;
    other_generation.generation = SourceGenerationId::new("8");
    let other_generation_embeddings =
        test_embedding_result_for(&other_generation, "text-embedding-test", 3);
    assert_ne!(
        baseline,
        builder(
            test_collection_spec(3),
            other_generation,
            other_generation_embeddings
        )
        .build()
        .unwrap()
        .points[0]
            .point_id
    );
}

#[test]
fn dimensions_mismatch_fails() {
    let document = test_prepared_document();
    let embeddings = test_embedding_result_with_vectors(
        "text-embedding-test",
        3,
        vec![
            ("chunk-web-1", vec![1.0, 2.0]),
            ("chunk-web-2", vec![4.0, 5.0, 6.0]),
        ],
    );

    let err = builder(test_collection_spec(3), document, embeddings)
        .build()
        .unwrap_err();
    assert_eq!(
        err,
        VectorPointBatchBuildError::DimensionMismatch {
            chunk_id: Some(ChunkId::new("chunk-web-1")),
            expected: 3,
            actual: 2
        }
    );
}

#[test]
fn non_finite_dense_vectors_fail_before_payload_build() {
    let document = test_prepared_document();
    let embeddings = test_embedding_result_with_vectors(
        "text-embedding-test",
        3,
        vec![
            ("chunk-web-1", vec![1.0, f32::NAN, 3.0]),
            ("chunk-web-2", vec![4.0, 5.0, 6.0]),
        ],
    );

    let err = builder(test_collection_spec(3), document, embeddings)
        .build()
        .unwrap_err();

    assert_eq!(
        err,
        VectorPointBatchBuildError::InvalidDenseVector {
            chunk_id: ChunkId::new("chunk-web-1")
        }
    );
}

#[test]
fn source_generation_builds_integer_payload_fields() {
    let mut document = test_prepared_document();
    document.generation = SourceGenerationId::new("gen_0001");
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);

    let batch = builder(test_collection_spec(3), document, embeddings)
        .build()
        .unwrap();

    assert_eq!(batch.points[0].payload["source_generation"], 1);
    assert!(batch.points[0].payload["committed_generation"].is_null());
}

#[test]
fn payload_validation_runs_before_returning_batch() {
    let mut document = test_prepared_document();
    document.chunks[0].metadata.insert(
        "raw_auth_headers".to_string(),
        json!("Authorization: Bearer secret"),
    );
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);

    let err = builder(test_collection_spec(3), document, embeddings)
        .build()
        .unwrap_err();
    assert!(matches!(
        err,
        VectorPointBatchBuildError::Payload {
            chunk_id,
            source: crate::payload::VectorPayloadValidationError::ForbiddenField { field }
        } if chunk_id == ChunkId::new("chunk-web-1") && field == "raw_auth_headers"
    ));
}

#[test]
fn document_body_examples_do_not_trigger_metadata_redaction_guardrails() {
    let mut document = test_prepared_document();
    document.chunks[0].content =
        "Use /tmp/axon in examples, or render <html> snippets.".to_string();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);

    let batch = builder(test_collection_spec(3), document, embeddings)
        .build()
        .unwrap();

    assert!(
        batch.points[0].payload["chunk_text"]
            .as_str()
            .unwrap()
            .contains("/tmp/axon")
    );
}

#[test]
fn clean_document_stamps_redaction_status_clean() {
    let document = test_prepared_document();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);

    let batch = builder(test_collection_spec(3), document, embeddings)
        .build()
        .unwrap();

    // No secret in metadata or body → the Redactor pass changed nothing, so the
    // status is `clean` (derived from the real pass, not hardcoded).
    assert_eq!(batch.points[0].payload["redaction_status"], "clean");
}

#[test]
fn secret_metadata_value_is_redacted_and_status_reflects_it() {
    let mut document = test_prepared_document();
    // A secret-shaped value in an allowed source-family metadata field (not a
    // forbidden field name, not the retrievable body) is scrubbed rather than
    // skipped, and the payload records `redaction_status = redacted`.
    document.chunks[0].metadata.insert(
        "web_title".to_string(),
        json!("authorization: bearer abcdef0123456789abcdef0123"),
    );
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);

    let batch = builder(test_collection_spec(3), document, embeddings)
        .build()
        .unwrap();

    assert_eq!(batch.points[0].payload["web_title"], "[REDACTED]");
    assert_eq!(batch.points[0].payload["redaction_status"], "redacted");
    // The other chunk carried no secret and stays clean.
    assert_eq!(batch.points[1].payload["redaction_status"], "clean");
}

#[test]
fn document_body_secret_examples_skip_the_chunk() {
    let mut document = test_prepared_document();
    document.chunks[0].content = "TOKEN=value".to_string();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);

    // A secret in the retrievable body (`chunk_text`) trips the `ForbiddenValue`
    // validator. The Redactor deliberately does NOT mask the body, so the chunk
    // is skipped (not indexed) rather than laundered — the sibling chunk still
    // builds. This is the secret-skip guarantee the redaction work preserves.
    let batch = builder(test_collection_spec(3), document, embeddings)
        .build()
        .unwrap();

    assert_eq!(batch.points.len(), 1);
    assert_eq!(batch.points[0].chunk_id, ChunkId::new("chunk-web-2"));
}

#[test]
fn embedding_result_must_match_document_embedding_provenance() {
    let document = test_prepared_document();
    let mut embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);
    embeddings.provider_id = ProviderId::new("other-provider");

    let err = builder(test_collection_spec(3), document, embeddings)
        .build()
        .unwrap_err();

    assert!(matches!(
        err,
        VectorPointBatchBuildError::EmbeddingProviderMismatch { .. }
    ));
}

#[test]
fn embedding_provider_provenance_is_checked_without_batch_id() {
    let mut document = test_prepared_document();
    document.metadata.remove("embedding_batch_id");
    let mut embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);
    embeddings.provider_id = ProviderId::new("other-provider");

    let err = builder(test_collection_spec(3), document, embeddings)
        .build()
        .unwrap_err();

    assert!(matches!(
        err,
        VectorPointBatchBuildError::EmbeddingProviderMismatch { .. }
    ));
}
