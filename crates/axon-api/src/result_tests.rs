use super::*;

fn citation() -> CanonicalCitation {
    CanonicalCitation {
        source_id: SourceId::new("source-1"),
        source_item_key: SourceItemKey::new("docs/guide"),
        generation: SourceGenerationId::new("7"),
        document_id: DocumentId::new("document-1"),
        chunk_id: ChunkId::new("chunk-1"),
        job_id: JobId::new(uuid::Uuid::from_u128(1)),
        canonical_uri: "https://example.com/docs/guide".to_string(),
        source_range: SourceRange {
            line_start: Some(10),
            line_end: Some(20),
            byte_start: Some(100),
            byte_end: Some(200),
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
        redaction: RedactionMetadata {
            redaction_status: crate::source::RedactionStatus::Clean,
            redaction_version: "2026-07-16".to_string(),
            visibility: crate::source::Visibility::Public,
            redacted_field_count: 0,
            dropped_field_count: 0,
            detector_count: 1,
            detector_names: vec!["secret-patterns-v1".to_string()],
        },
    }
}

#[test]
fn canonical_citation_round_trips_complete_lineage() {
    let expected = citation();
    let value = serde_json::to_value(&expected).expect("citation serializes");
    let actual: CanonicalCitation =
        serde_json::from_value(value.clone()).expect("citation deserializes");

    assert_eq!(actual, expected);
    assert_eq!(value["source_id"], "source-1");
    assert_eq!(value["source_item_key"], "docs/guide");
    assert_eq!(value["generation"], "7");
    assert_eq!(value["document_id"], "document-1");
    assert_eq!(value["chunk_id"], "chunk-1");
    assert_eq!(value["redaction"]["visibility"], "public");
    assert_eq!(value["redaction"]["redaction_status"], "clean");
}

#[test]
fn canonical_citation_rejects_compatibility_fields() {
    let mut value = serde_json::to_value(citation()).expect("citation serializes");
    value["url"] = serde_json::json!("https://legacy.invalid");

    let error = serde_json::from_value::<CanonicalCitation>(value)
        .expect_err("unknown compatibility field must fail");
    assert!(error.to_string().contains("unknown field"));
}
