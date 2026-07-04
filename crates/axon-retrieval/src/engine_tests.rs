use std::sync::Arc;

use crate::citation::Citation;
use crate::context::ContextBundle;
use crate::engine::{RetrievalAccess, RetrievalEngine, RetrievalEngineConfig};
use crate::query::RetrievalRequest;
use async_trait::async_trait;
use axon_api::source::{
    BatchId, ChunkId, ContentKind, DocumentId, EmbeddingInput, EmbeddingResult, EmbeddingVector,
    JobId, JobPriority, MetadataMap, ProviderCapability, ProviderId, ProviderUsage,
    SourceGenerationId, SourceId, SourceRange, SourceWarning, VectorPoint, VectorPointBatch,
    VectorPointId, Visibility,
};
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_embedding::provider::{EmbeddingProvider, Result as EmbeddingProviderResult};
use axon_vectors::store::{FakeVectorStore, VectorStore};
use axon_vectors::testing::test_collection_spec_hybrid;
use serde_json::json;
use uuid::Uuid;

fn request() -> RetrievalRequest {
    RetrievalRequest {
        query: "vector embedding split".to_string(),
        collection: "axon-test".to_string(),
        limit: 3,
        source_id: Some(SourceId::new("src-docs")),
        generation: Some(SourceGenerationId::new("7")),
        namespace_filters: vec!["docs".to_string(), "guides".to_string()],
        byte_budget: 80,
        token_budget: 20,
    }
}

#[test]
fn retrieval_plan_preserves_source_id_generation_visibility_and_namespace_filters() {
    let request = request();

    let plan = request.plan(RetrievalAccess::standard().allowed_visibility);

    assert_eq!(plan.collection, "axon-test");
    assert_eq!(plan.source_id, Some(SourceId::new("src-docs")));
    assert_eq!(plan.generation, Some(SourceGenerationId::new("7")));
    assert_eq!(
        plan.allowed_visibility,
        vec![
            Visibility::Public,
            Visibility::Internal,
            Visibility::Derived
        ]
    );
    assert_eq!(plan.namespace_filters, vec!["docs", "guides"]);
    assert_eq!(plan.limit, 3);
}

#[tokio::test]
async fn ranking_is_deterministic_with_fixed_fake_vector_search_results() {
    let store = Arc::new(FakeVectorStore::new("fake-vectors"));
    store
        .ensure_collection(test_collection_spec_hybrid(4))
        .await
        .unwrap();

    let provider = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 4));
    let query_vector = provider
        .embed(axon_api::source::EmbeddingBatch {
            batch_id: BatchId::new(Uuid::from_u128(10)),
            job_id: JobId::new(Uuid::from_u128(11)),
            provider_id: ProviderId::new("fake-embedding"),
            model: "fake-embedding".to_string(),
            items: vec![EmbeddingInput {
                chunk_id: ChunkId::new("query"),
                text: request().query.clone(),
                content_kind: ContentKind::PlainText,
                metadata: MetadataMap::new(),
            }],
            instruction: None,
            priority: JobPriority::Interactive,
            metadata: MetadataMap::new(),
        })
        .await
        .unwrap()
        .vectors
        .remove(0)
        .values;

    store
        .upsert(VectorPointBatch {
            batch_id: BatchId::new(Uuid::from_u128(12)),
            collection: "axon-test".to_string(),
            model: "fake-embedding".to_string(),
            dimensions: 4,
            sparse_vectors: None,
            payload_indexes: test_collection_spec_hybrid(4).payload_indexes,
            points: vec![
                point("point-a", "chunk-a", &query_vector, "Alpha chunk body"),
                point(
                    "point-b",
                    "chunk-b",
                    &[1.0, 0.0, 0.0, 0.0],
                    "Beta chunk body",
                ),
                point(
                    "point-c",
                    "chunk-c",
                    &[1.0, 0.0, 0.0, 0.0],
                    "Gamma chunk body",
                ),
            ],
        })
        .await
        .unwrap();

    let engine = RetrievalEngine::new(store.clone(), provider, retrieval_config());
    let result = engine.retrieve(request()).await.unwrap();
    let chunk_ids: Vec<_> = result
        .matches
        .iter()
        .map(|item| item.chunk_id.0.as_str())
        .collect();

    assert_eq!(chunk_ids, vec!["chunk-a", "chunk-b", "chunk-c"]);
}

#[tokio::test]
async fn retrieval_applies_all_namespace_filters() {
    let store = Arc::new(FakeVectorStore::new("fake-vectors"));
    store
        .ensure_collection(test_collection_spec_hybrid(4))
        .await
        .unwrap();

    let provider = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 4));
    store
        .upsert(VectorPointBatch {
            batch_id: BatchId::new(Uuid::from_u128(12)),
            collection: "axon-test".to_string(),
            model: "fake-embedding".to_string(),
            dimensions: 4,
            sparse_vectors: None,
            payload_indexes: test_collection_spec_hybrid(4).payload_indexes,
            points: vec![
                point_in_namespace(
                    "point-docs",
                    "chunk-docs",
                    &[1.0, 0.0, 0.0, 0.0],
                    "Docs chunk body",
                    "docs",
                ),
                point_in_namespace(
                    "point-guides",
                    "chunk-guides",
                    &[1.0, 0.0, 0.0, 0.0],
                    "Guides chunk body",
                    "guides",
                ),
                point_in_namespace(
                    "point-summary",
                    "chunk-summary",
                    &[1.0, 0.0, 0.0, 0.0],
                    "Summary chunk body",
                    "summary",
                ),
                point_with_filters(
                    "point-other-source",
                    "chunk-other-source",
                    &[1.0, 0.0, 0.0, 0.0],
                    "Other source chunk body",
                    PointFilters {
                        source_id: "src-other",
                        generation: "7",
                        visibility: "internal",
                        namespace: "docs",
                    },
                ),
                point_with_filters(
                    "point-other-generation",
                    "chunk-other-generation",
                    &[1.0, 0.0, 0.0, 0.0],
                    "Other generation chunk body",
                    PointFilters {
                        source_id: "src-docs",
                        generation: "8",
                        visibility: "internal",
                        namespace: "docs",
                    },
                ),
                point_with_filters(
                    "point-other-visibility",
                    "chunk-other-visibility",
                    &[1.0, 0.0, 0.0, 0.0],
                    "Other visibility chunk body",
                    PointFilters {
                        source_id: "src-docs",
                        generation: "7",
                        visibility: "sensitive",
                        namespace: "docs",
                    },
                ),
            ],
        })
        .await
        .unwrap();

    let engine = RetrievalEngine::new(store.clone(), provider, retrieval_config());
    let result = engine.retrieve(request()).await.unwrap();
    let chunk_ids = result
        .matches
        .iter()
        .map(|item| item.chunk_id.0.as_str())
        .collect::<Vec<_>>();

    assert_eq!(chunk_ids, vec!["chunk-docs", "chunk-guides"]);
    assert!(result.context.text.contains("Docs chunk body"));
    assert!(result.context.text.contains("Guides chunk body"));
    assert!(!result.context.text.contains("Summary chunk body"));
}

#[tokio::test]
async fn standard_retrieval_access_excludes_sensitive_and_redacted_chunks() {
    let store = Arc::new(FakeVectorStore::new("fake-vectors"));
    store
        .ensure_collection(test_collection_spec_hybrid(4))
        .await
        .unwrap();
    let provider = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 4));
    store
        .upsert(VectorPointBatch {
            batch_id: BatchId::new(Uuid::from_u128(12)),
            collection: "axon-test".to_string(),
            model: "fake-embedding".to_string(),
            dimensions: 4,
            sparse_vectors: None,
            payload_indexes: test_collection_spec_hybrid(4).payload_indexes,
            points: vec![
                point_with_filters(
                    "point-internal",
                    "chunk-internal",
                    &[1.0, 0.0, 0.0, 0.0],
                    "Internal chunk body",
                    PointFilters {
                        source_id: "src-docs",
                        generation: "7",
                        visibility: "internal",
                        namespace: "docs",
                    },
                ),
                point_with_filters(
                    "point-sensitive",
                    "chunk-sensitive",
                    &[1.0, 0.0, 0.0, 0.0],
                    "Sensitive chunk body",
                    PointFilters {
                        source_id: "src-docs",
                        generation: "7",
                        visibility: "sensitive",
                        namespace: "docs",
                    },
                ),
                point_with_filters(
                    "point-redacted",
                    "chunk-redacted",
                    &[1.0, 0.0, 0.0, 0.0],
                    "Redacted chunk body",
                    PointFilters {
                        source_id: "src-docs",
                        generation: "7",
                        visibility: "redacted",
                        namespace: "docs",
                    },
                ),
            ],
        })
        .await
        .unwrap();

    let engine = RetrievalEngine::new(store, provider, retrieval_config());
    let result = engine.retrieve(request()).await.unwrap();

    assert!(result.context.text.contains("Internal chunk body"));
    assert!(!result.context.text.contains("Sensitive chunk body"));
    assert!(!result.context.text.contains("Redacted chunk body"));
}

#[tokio::test]
async fn standard_retrieval_access_excludes_non_clean_redaction_status() {
    let store = Arc::new(FakeVectorStore::new("fake-vectors"));
    store
        .ensure_collection(test_collection_spec_hybrid(4))
        .await
        .unwrap();
    let provider = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 4));
    let mut redacted = point_in_namespace(
        "point-redaction-status",
        "chunk-redaction-status",
        &[1.0, 0.0, 0.0, 0.0],
        "Redaction status body",
        "docs",
    );
    redacted
        .payload
        .insert("redaction_status".to_string(), json!("redacted"));
    store
        .upsert(VectorPointBatch {
            batch_id: BatchId::new(Uuid::from_u128(12)),
            collection: "axon-test".to_string(),
            model: "fake-embedding".to_string(),
            dimensions: 4,
            sparse_vectors: None,
            payload_indexes: test_collection_spec_hybrid(4).payload_indexes,
            points: vec![
                point_in_namespace(
                    "point-clean",
                    "chunk-clean",
                    &[1.0, 0.0, 0.0, 0.0],
                    "Clean chunk body",
                    "docs",
                ),
                redacted,
            ],
        })
        .await
        .unwrap();

    let engine = RetrievalEngine::new(store, provider, retrieval_config());
    let result = engine.retrieve(request()).await.unwrap();

    assert!(result.context.text.contains("Clean chunk body"));
    assert!(!result.context.text.contains("Redaction status body"));
}

#[tokio::test]
async fn retrieval_rejects_missing_query_vector_from_embedding_provider() {
    let store = Arc::new(FakeVectorStore::new("fake-vectors"));
    store
        .ensure_collection(test_collection_spec_hybrid(4))
        .await
        .unwrap();
    let provider = Arc::new(MalformedEmbeddingProvider::empty_vectors());
    let engine = RetrievalEngine::new(store, provider, retrieval_config());

    let err = engine.retrieve(request()).await.unwrap_err();

    assert_eq!(err.code.to_string(), "retrieval.missing_query_vector");
}

#[tokio::test]
async fn retrieval_rejects_embedding_dimension_mismatch_before_search() {
    let store = Arc::new(FakeVectorStore::new("fake-vectors"));
    store
        .ensure_collection(test_collection_spec_hybrid(4))
        .await
        .unwrap();
    let provider = Arc::new(MalformedEmbeddingProvider::dimension_mismatch());
    let engine = RetrievalEngine::new(store.clone(), provider, retrieval_config());

    let err = engine.retrieve(request()).await.unwrap_err();

    assert_eq!(
        err.code.to_string(),
        "retrieval.embedding_dimension_mismatch"
    );
    assert_eq!(store.calls().await, vec!["ensure_collection"]);
}

#[tokio::test]
async fn retrieval_rejects_embedding_provider_and_model_mismatches_before_search() {
    let store = Arc::new(FakeVectorStore::new("fake-vectors"));
    store
        .ensure_collection(test_collection_spec_hybrid(4))
        .await
        .unwrap();

    let provider = Arc::new(MalformedEmbeddingProvider::provider_mismatch());
    let engine = RetrievalEngine::new(store.clone(), provider, retrieval_config());
    let err = engine.retrieve(request()).await.unwrap_err();
    assert_eq!(
        err.code.to_string(),
        "retrieval.embedding_provider_mismatch"
    );

    let provider = Arc::new(MalformedEmbeddingProvider::model_mismatch());
    let engine = RetrievalEngine::new(store.clone(), provider, retrieval_config());
    let err = engine.retrieve(request()).await.unwrap_err();
    assert_eq!(err.code.to_string(), "retrieval.embedding_model_mismatch");

    assert_eq!(store.calls().await, vec!["ensure_collection"]);
}

#[tokio::test]
async fn retrieval_rejects_context_budgets_that_admit_no_chunks() {
    let store = Arc::new(FakeVectorStore::new("fake-vectors"));
    store
        .ensure_collection(test_collection_spec_hybrid(4))
        .await
        .unwrap();
    let provider = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 4));
    store
        .upsert(VectorPointBatch {
            batch_id: BatchId::new(Uuid::from_u128(12)),
            collection: "axon-test".to_string(),
            model: "fake-embedding".to_string(),
            dimensions: 4,
            sparse_vectors: None,
            payload_indexes: test_collection_spec_hybrid(4).payload_indexes,
            points: vec![point_in_namespace(
                "point-budget",
                "chunk-budget",
                &[1.0, 0.0, 0.0, 0.0],
                "Body too large for the tiny budget",
                "docs",
            )],
        })
        .await
        .unwrap();

    let mut request = request();
    request.byte_budget = 1;
    request.token_budget = 1;
    let engine = RetrievalEngine::new(store, provider, retrieval_config());

    let err = engine.retrieve(request).await.unwrap_err();

    assert_eq!(err.code.to_string(), "retrieval.context_budget_too_small");
}

#[test]
fn vector_match_text_falls_back_to_chunk_text_payload() {
    let mut payload = MetadataMap::new();
    payload.insert("chunk_text".to_string(), json!("payload body"));
    payload.insert("redaction_status".to_string(), json!("clean"));
    payload.insert(
        "chunk_locator".to_string(),
        json!({
            "canonical_uri": "https://example.com/docs",
            "range": { "line_start": 1, "line_end": 2 }
        }),
    );
    payload.insert(
        "source_range".to_string(),
        json!({ "line_start": 1, "line_end": 2 }),
    );

    let item = axon_api::source::VectorSearchMatch {
        point_id: VectorPointId::new("point"),
        score: 1.0,
        chunk_id: Some(ChunkId::new("chunk")),
        document_id: Some(DocumentId::new("doc")),
        source_id: Some(SourceId::new("src")),
        source_item_key: None,
        text: None,
        payload,
    };

    let matched = super::engine::match_from_vector(&item).unwrap();

    assert_eq!(matched.text, "payload body");
}

#[test]
fn vector_match_rejects_malformed_source_ranges() {
    for (field, expected_code, payload) in [
        (
            "source_range",
            "retrieval.invalid_source_range",
            json!({
                "chunk_locator": {
                    "canonical_uri": "https://example.com/docs",
                    "range": { "line_start": 1, "line_end": 2 }
                },
                "source_range": "not-a-range"
            }),
        ),
        (
            "chunk_locator.range",
            "retrieval.invalid_chunk_locator_range",
            json!({
                "chunk_locator": {
                    "canonical_uri": "https://example.com/docs",
                    "range": "not-a-range"
                },
                "source_range": { "line_start": 1, "line_end": 2 }
            }),
        ),
    ] {
        let mut metadata = MetadataMap::new();
        metadata.insert("chunk_text".to_string(), json!("payload body"));
        metadata.insert("redaction_status".to_string(), json!("clean"));
        metadata.extend(
            payload
                .as_object()
                .unwrap()
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );

        let err = super::engine::match_from_vector(&axon_api::source::VectorSearchMatch {
            point_id: VectorPointId::new(format!("point-{field}")),
            score: 1.0,
            chunk_id: Some(ChunkId::new("chunk")),
            document_id: Some(DocumentId::new("doc")),
            source_id: Some(SourceId::new("src")),
            source_item_key: None,
            text: None,
            payload: metadata,
        })
        .unwrap_err();

        assert_eq!(err.code.to_string(), expected_code, "{field}");
    }
}

#[test]
fn vector_match_rejects_non_clean_redaction_status() {
    let mut payload = MetadataMap::new();
    payload.insert("chunk_text".to_string(), json!("payload body"));
    payload.insert("redaction_status".to_string(), json!("redacted"));
    payload.insert(
        "chunk_locator".to_string(),
        json!({
            "canonical_uri": "https://example.com/docs",
            "range": { "line_start": 1, "line_end": 2 }
        }),
    );
    payload.insert(
        "source_range".to_string(),
        json!({ "line_start": 1, "line_end": 2 }),
    );

    let err = super::engine::match_from_vector(&axon_api::source::VectorSearchMatch {
        point_id: VectorPointId::new("point-redacted"),
        score: 1.0,
        chunk_id: Some(ChunkId::new("chunk")),
        document_id: Some(DocumentId::new("doc")),
        source_id: Some(SourceId::new("src")),
        source_item_key: None,
        text: None,
        payload,
    })
    .unwrap_err();

    assert_eq!(err.code.to_string(), "retrieval.redaction_status_not_clean");
}

#[test]
fn vector_match_rejects_missing_redaction_status() {
    let mut payload = MetadataMap::new();
    payload.insert("chunk_text".to_string(), json!("payload body"));
    payload.insert(
        "chunk_locator".to_string(),
        json!({
            "canonical_uri": "https://example.com/docs",
            "range": { "line_start": 1, "line_end": 2 }
        }),
    );
    payload.insert(
        "source_range".to_string(),
        json!({ "line_start": 1, "line_end": 2 }),
    );

    let err = super::engine::match_from_vector(&axon_api::source::VectorSearchMatch {
        point_id: VectorPointId::new("point-missing-redaction"),
        score: 1.0,
        chunk_id: Some(ChunkId::new("chunk")),
        document_id: Some(DocumentId::new("doc")),
        source_id: Some(SourceId::new("src")),
        source_item_key: None,
        text: None,
        payload,
    })
    .unwrap_err();

    assert_eq!(err.code.to_string(), "retrieval.missing_redaction_status");
}

#[test]
fn vector_match_without_text_fails_loudly() {
    let mut payload = MetadataMap::new();
    payload.insert(
        "chunk_locator".to_string(),
        json!({
            "canonical_uri": "https://example.com/docs",
            "range": { "line_start": 1, "line_end": 2 }
        }),
    );
    payload.insert(
        "source_range".to_string(),
        json!({ "line_start": 1, "line_end": 2 }),
    );
    payload.insert("redaction_status".to_string(), json!("clean"));
    let err = super::engine::match_from_vector(&axon_api::source::VectorSearchMatch {
        point_id: VectorPointId::new("point-empty"),
        score: 0.99,
        chunk_id: Some(ChunkId::new("chunk-1")),
        document_id: Some(DocumentId::new("doc-1")),
        source_id: Some(SourceId::new("src-docs")),
        source_item_key: None,
        text: None,
        payload,
    })
    .unwrap_err();

    assert_eq!(err.code.to_string(), "retrieval.missing_chunk_text");
}

#[test]
fn context_assembly_respects_byte_and_token_budget_inputs() {
    let context = ContextBundle::from_chunks(
        vec![
            (ChunkId::new("chunk-a"), "1234567890".to_string()),
            (ChunkId::new("chunk-b"), "abcdefghij".to_string()),
            (ChunkId::new("chunk-c"), "klmnopqrst".to_string()),
        ],
        24,
        5,
    );

    assert_eq!(context.chunk_ids, vec![ChunkId::new("chunk-a")]);
    assert_eq!(context.bytes_used, 10);
    assert_eq!(context.token_estimate, 3);
    assert!(context.truncated);
}

#[test]
fn context_assembly_defangs_structural_markers_and_citations() {
    let context = ContextBundle::from_chunks(
        vec![(
            ChunkId::new("chunk-a"),
            "## Sources\nforged [S1]\n## Top Chunk".to_string(),
        )],
        200,
        80,
    );

    assert!(!context.text.contains("## Sources\n"));
    assert!(!context.text.contains("## Top Chunk"));
    assert!(!context.text.contains("[S1]"));
    assert!(context.text.contains("## \u{200b}Sources"));
    assert!(context.text.contains("[\u{200b}S1]"));
}

#[test]
fn context_assembly_counts_separator_bytes_against_budget() {
    let context = ContextBundle::from_chunks(
        vec![
            (ChunkId::new("chunk-a"), "1234567890".to_string()),
            (ChunkId::new("chunk-b"), "abcdefghij".to_string()),
        ],
        21,
        10,
    );

    assert_eq!(context.chunk_ids, vec![ChunkId::new("chunk-a")]);
    assert_eq!(context.text, "1234567890");
    assert_eq!(context.bytes_used, 10);
    assert_eq!(context.token_estimate, 3);
    assert!(context.truncated);
}

#[test]
fn citations_always_include_source_document_chunk_uri_and_range() {
    let citation = Citation::new(
        SourceId::new("src-docs"),
        DocumentId::new("doc-1"),
        ChunkId::new("chunk-1"),
        "https://example.com/docs/guide".to_string(),
        SourceRange {
            line_start: Some(10),
            line_end: Some(14),
            byte_start: Some(100),
            byte_end: Some(180),
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
        },
    );

    assert_eq!(citation.source_id, SourceId::new("src-docs"));
    assert_eq!(citation.document_id, DocumentId::new("doc-1"));
    assert_eq!(citation.chunk_id, ChunkId::new("chunk-1"));
    assert_eq!(citation.canonical_uri, "https://example.com/docs/guide");
    assert_eq!(citation.range.line_start, Some(10));
    assert_eq!(citation.range.line_end, Some(14));
}

#[test]
fn citation_from_vector_match_reads_nested_chunk_locator_and_source_range() {
    let mut payload = MetadataMap::new();
    payload.insert(
        "chunk_locator".to_string(),
        json!({
            "canonical_uri": "https://example.com/docs/guide",
            "path": "docs/guide.md",
            "range": {
                "line_start": 4,
                "line_end": 8,
            }
        }),
    );
    payload.insert(
        "source_range".to_string(),
        json!({
            "line_start": 10,
            "line_end": 14,
            "byte_start": 100,
            "byte_end": 180,
        }),
    );

    let citation = Citation::from_vector_match(&axon_api::source::VectorSearchMatch {
        point_id: VectorPointId::new("point-nested"),
        score: 0.99,
        chunk_id: Some(ChunkId::new("chunk-1")),
        document_id: Some(DocumentId::new("doc-1")),
        source_id: Some(SourceId::new("src-docs")),
        source_item_key: None,
        text: Some("hello".to_string()),
        payload,
    })
    .expect("nested vector payloads should parse");

    assert_eq!(citation.canonical_uri, "https://example.com/docs/guide");
    assert_eq!(citation.range.line_start, Some(10));
    assert_eq!(citation.range.line_end, Some(14));
    assert_eq!(citation.range.byte_start, Some(100));
    assert_eq!(citation.range.byte_end, Some(180));
}

#[test]
fn citation_from_vector_match_falls_back_to_chunk_locator_range() {
    let mut payload = MetadataMap::new();
    payload.insert(
        "chunk_locator".to_string(),
        json!({
            "canonical_uri": "https://example.com/docs/guide",
            "range": {
                "line_start": 21,
                "line_end": 24,
                "char_start": 500,
                "char_end": 620,
            }
        }),
    );

    let citation = Citation::from_vector_match(&axon_api::source::VectorSearchMatch {
        point_id: VectorPointId::new("point-locator-range"),
        score: 0.99,
        chunk_id: Some(ChunkId::new("chunk-1")),
        document_id: Some(DocumentId::new("doc-1")),
        source_id: Some(SourceId::new("src-docs")),
        source_item_key: None,
        text: Some("hello".to_string()),
        payload,
    })
    .expect("chunk locator range should be accepted when source_range is absent");

    assert_eq!(citation.canonical_uri, "https://example.com/docs/guide");
    assert_eq!(citation.range.line_start, Some(21));
    assert_eq!(citation.range.line_end, Some(24));
    assert_eq!(citation.range.char_start, Some(500));
    assert_eq!(citation.range.char_end, Some(620));
}

#[test]
fn citation_from_vector_match_rejects_missing_range_locator() {
    let mut payload = MetadataMap::new();
    payload.insert(
        "chunk_locator".to_string(),
        json!({
            "canonical_uri": "https://example.com/docs/guide",
        }),
    );

    let err = Citation::from_vector_match(&axon_api::source::VectorSearchMatch {
        point_id: VectorPointId::new("point-no-range"),
        score: 0.99,
        chunk_id: Some(ChunkId::new("chunk-1")),
        document_id: Some(DocumentId::new("doc-1")),
        source_id: Some(SourceId::new("src-docs")),
        source_item_key: None,
        text: Some("hello".to_string()),
        payload,
    })
    .expect_err("locator-less vector matches should be rejected");

    assert_eq!(err.code.to_string(), "retrieval.missing_source_range");
}

fn point(point_id: &str, chunk_id: &str, vector: &[f32], text: &str) -> VectorPoint {
    point_in_namespace(point_id, chunk_id, vector, text, "docs")
}

fn retrieval_config() -> RetrievalEngineConfig {
    RetrievalEngineConfig::new(
        ProviderId::new("fake-embedding"),
        "fake-embedding",
        4,
        RetrievalAccess::standard(),
    )
}

fn point_in_namespace(
    point_id: &str,
    chunk_id: &str,
    vector: &[f32],
    text: &str,
    namespace: &str,
) -> VectorPoint {
    point_with_filters(
        point_id,
        chunk_id,
        vector,
        text,
        PointFilters {
            source_id: "src-docs",
            generation: "7",
            visibility: "internal",
            namespace,
        },
    )
}

struct PointFilters<'a> {
    source_id: &'a str,
    generation: &'a str,
    visibility: &'a str,
    namespace: &'a str,
}

fn point_with_filters(
    point_id: &str,
    chunk_id: &str,
    vector: &[f32],
    text: &str,
    filters: PointFilters<'_>,
) -> VectorPoint {
    let mut payload = MetadataMap::new();
    payload.insert("payload_contract_version".to_string(), json!("2026-07-01"));
    payload.insert("collection".to_string(), json!("axon-test"));
    payload.insert("source_family".to_string(), json!("web"));
    payload.insert("source_kind".to_string(), json!("web"));
    payload.insert("source_adapter".to_string(), json!("web"));
    payload.insert("source_scope".to_string(), json!("page"));
    payload.insert(
        "item_canonical_uri".to_string(),
        json!(format!("https://example.com/{chunk_id}")),
    );
    payload.insert(
        "source_item_key".to_string(),
        json!(format!("https://example.com/{chunk_id}")),
    );
    payload.insert("source_id".to_string(), json!(filters.source_id));
    payload.insert("source_generation".to_string(), json!(filters.generation));
    payload.insert(
        "committed_generation".to_string(),
        json!(filters.generation),
    );
    payload.insert("document_id".to_string(), json!(format!("doc-{chunk_id}")));
    payload.insert("chunk_id".to_string(), json!(chunk_id));
    payload.insert(
        "chunk_locator".to_string(),
        json!({
            "canonical_uri": format!("https://example.com/{chunk_id}"),
            "path": format!("/{chunk_id}"),
            "heading_path": [],
            "symbol": null,
            "range": {
                "line_start": 1,
                "line_end": 3,
            }
        }),
    );
    payload.insert(
        "source_range".to_string(),
        json!({
            "line_start": 1,
            "line_end": 3,
        }),
    );
    payload.insert("visibility".to_string(), json!(filters.visibility));
    payload.insert("redaction_status".to_string(), json!("clean"));
    payload.insert(
        "job_id".to_string(),
        json!("00000000-0000-0000-0000-000000000099"),
    );
    payload.insert(
        "embedding_batch_id".to_string(),
        json!("00000000-0000-0000-0000-00000000000c"),
    );
    payload.insert("document_status".to_string(), json!("prepared"));
    payload.insert("embedding_model".to_string(), json!("fake-embedding"));
    payload.insert("embedding_dimensions".to_string(), json!(4));
    payload.insert("embedding_provider".to_string(), json!("fake-embedding"));
    payload.insert("embedding_profile".to_string(), json!("test"));
    payload.insert("embedded_at".to_string(), json!("2026-07-01T00:00:00Z"));
    payload.insert("vector_namespace".to_string(), json!(filters.namespace));
    payload.insert("content_kind".to_string(), json!("markdown"));
    payload.insert("chunk_text".to_string(), json!(text));

    VectorPoint {
        point_id: VectorPointId::new(point_id),
        chunk_id: ChunkId::new(chunk_id),
        vector: vector.to_vec(),
        sparse_vector: None,
        payload,
    }
}

#[derive(Debug, Clone)]
struct MalformedEmbeddingProvider {
    vectors: Vec<EmbeddingVector>,
    dimensions: u32,
    provider_id: ProviderId,
    model: String,
}

impl MalformedEmbeddingProvider {
    fn empty_vectors() -> Self {
        Self {
            vectors: Vec::new(),
            dimensions: 4,
            provider_id: ProviderId::new("fake-embedding"),
            model: "fake-embedding".to_string(),
        }
    }

    fn dimension_mismatch() -> Self {
        Self {
            vectors: vec![EmbeddingVector {
                chunk_id: ChunkId::new("query"),
                values: vec![1.0, 2.0, 3.0],
            }],
            dimensions: 3,
            provider_id: ProviderId::new("fake-embedding"),
            model: "fake-embedding".to_string(),
        }
    }

    fn provider_mismatch() -> Self {
        Self {
            vectors: vec![EmbeddingVector {
                chunk_id: ChunkId::new("query"),
                values: vec![1.0, 2.0, 3.0, 4.0],
            }],
            dimensions: 4,
            provider_id: ProviderId::new("other-provider"),
            model: "fake-embedding".to_string(),
        }
    }

    fn model_mismatch() -> Self {
        Self {
            vectors: vec![EmbeddingVector {
                chunk_id: ChunkId::new("query"),
                values: vec![1.0, 2.0, 3.0, 4.0],
            }],
            dimensions: 4,
            provider_id: ProviderId::new("fake-embedding"),
            model: "other-model".to_string(),
        }
    }
}

#[async_trait]
impl EmbeddingProvider for MalformedEmbeddingProvider {
    async fn embed(
        &self,
        batch: axon_api::source::EmbeddingBatch,
    ) -> EmbeddingProviderResult<EmbeddingResult> {
        Ok(EmbeddingResult {
            batch_id: batch.batch_id,
            job_id: batch.job_id,
            provider_id: self.provider_id.clone(),
            model: self.model.clone(),
            dimensions: self.dimensions,
            vectors: self.vectors.clone(),
            usage: ProviderUsage {
                input_tokens: Some(1),
                output_tokens: None,
                requests: 1,
                duration_ms: 0,
            },
            warnings: Vec::<SourceWarning>::new(),
        })
    }

    async fn capabilities(&self) -> EmbeddingProviderResult<ProviderCapability> {
        FakeEmbeddingProvider::new("fake-embedding", self.dimensions)
            .capabilities()
            .await
    }
}
