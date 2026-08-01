use super::*;
use axon_vectors::testing::{
    test_collection_spec, test_embedding_result_for, test_prepared_document,
};

fn web_document_with_target_metadata() -> SourceDocument {
    let mut metadata = MetadataMap::new();
    metadata.insert("source_family".to_string(), serde_json::json!("web"));
    metadata.insert("source_kind".to_string(), serde_json::json!("web"));
    metadata.insert("source_adapter".to_string(), serde_json::json!("web"));
    metadata.insert("source_scope".to_string(), serde_json::json!("site"));
    metadata.insert(
        "item_canonical_uri".to_string(),
        serde_json::json!("https://example.com/docs/page"),
    );
    metadata.insert("visibility".to_string(), serde_json::json!("internal"));
    metadata.insert("redaction_status".to_string(), serde_json::json!("clean"));
    metadata.insert("web_title".to_string(), serde_json::json!("Target Fields"));
    metadata.insert("web_domain".to_string(), serde_json::json!("example.com"));
    metadata.insert("normalization_version".to_string(), serde_json::json!("v1"));
    metadata.insert(
        "web_url".to_string(),
        serde_json::json!("https://example.com/docs/page?utm=1"),
    );
    metadata.insert(
        "web_seed_url".to_string(),
        serde_json::json!("https://example.com/docs"),
    );
    metadata.insert(
        "web_origin".to_string(),
        serde_json::json!("https://example.com"),
    );
    metadata.insert("web_path".to_string(), serde_json::json!("/docs/page"));
    metadata.insert(
        "web_normalized_url".to_string(),
        serde_json::json!("https://example.com/docs/page"),
    );
    metadata.insert("web_fetch_method".to_string(), serde_json::json!("http"));
    metadata.insert(
        "structured_payload_omitted".to_string(),
        serde_json::json!(false),
    );
    metadata.insert("web_render_mode".to_string(), serde_json::json!("chrome"));

    SourceDocument {
        document_id: DocumentId::new("doc_web_target_metadata"),
        source_id: SourceId::new("src_web"),
        source_item_key: SourceItemKey::new("https://example.com/docs/page"),
        canonical_uri: "https://example.com/docs/page".to_string(),
        content_kind: ContentKind::Markdown,
        content: ContentRef::InlineText {
            text: "# Target Fields\n\nBody text.".to_string(),
        },
        metadata,
        title: Some("Target Fields".to_string()),
        language: None,
        path: Some("/docs/page".to_string()),
        mime_type: None,
        structured_payload: None,
        artifact_id: None,
        chunk_hints: Vec::new(),
        parser_hints: Vec::new(),
    }
}

#[test]
fn web_source_vectorize_preserves_target_web_metadata() {
    let prepared = prepare_source_documents(
        vec![web_document_with_target_metadata()],
        &SourceGenerationId::new("gen-1"),
    )
    .expect("prepare web document");
    let document = prepared.into_iter().next().expect("prepared document");

    for field in [
        "normalization_version",
        "web_url",
        "web_seed_url",
        "web_origin",
        "web_path",
        "web_normalized_url",
        "web_fetch_method",
        "structured_payload_omitted",
    ] {
        assert!(
            document.metadata.contains_key(field),
            "document metadata should keep {field}"
        );
        assert!(
            document
                .chunks
                .iter()
                .all(|chunk| chunk.metadata.contains_key(field)),
            "every chunk should keep {field}"
        );
    }
    assert!(
        !document.metadata.contains_key("web_render_mode"),
        "debug-only acquisition metadata stays out of vector payloads"
    );
}

#[test]
fn vector_point_counts_are_recorded_per_document_after_redaction_skips() {
    let mut redacted_document = test_prepared_document();
    redacted_document.chunks[0].content = "API_KEY=abc123".to_string();

    let mut clean_document = test_prepared_document();
    clean_document.document_id = DocumentId::new("doc-clean");
    clean_document.source_item_key = SourceItemKey::new("https://example.com/clean");
    clean_document.canonical_uri = "https://example.com/clean".to_string();
    for (index, chunk) in clean_document.chunks.iter_mut().enumerate() {
        chunk.chunk_id = ChunkId::new(format!("chunk-clean-{index}"));
    }

    let mut embeddings = test_embedding_result_for(&redacted_document, "text-embedding-test", 3);
    embeddings
        .vectors
        .extend(test_embedding_result_for(&clean_document, "text-embedding-test", 3).vectors);

    let built = vector_point_batch_for_documents(
        test_collection_spec(3),
        &[redacted_document.clone(), clean_document.clone()],
        &embeddings,
    )
    .expect("build vector points");

    assert_eq!(built.skipped_redaction, 1);
    assert_eq!(built.batch.points.len(), 3);
    assert_eq!(
        built.points_by_document.get(&redacted_document.document_id),
        Some(&1)
    );
    assert_eq!(
        built.points_by_document.get(&clean_document.document_id),
        Some(&2)
    );
    let redacted_status = vectorized_document_status(
        &redacted_document,
        &built.points_by_document,
        Timestamp("2026-07-31T00:00:00Z".to_string()),
    )
    .expect("build redacted document status");
    assert_eq!(redacted_status.chunk_count, 2);
    assert_eq!(redacted_status.vector_point_count, 1);
}
