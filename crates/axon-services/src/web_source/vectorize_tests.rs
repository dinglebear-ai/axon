use super::*;

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
