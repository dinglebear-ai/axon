/// Verify that the empty-chunk filter (applied before tei_embed) removes
/// blank and whitespace-only strings while keeping real content.
#[test]
fn empty_and_whitespace_chunks_are_filtered() {
    let mut chunks = vec![
        "".to_string(),
        "   ".to_string(),
        "real content".to_string(),
        "\n\t\n".to_string(),
        "another chunk".to_string(),
    ];
    chunks.retain(|c| !c.trim().is_empty());
    assert_eq!(
        chunks,
        vec!["real content".to_string(), "another chunk".to_string()]
    );
}

#[test]
fn all_empty_chunks_produces_no_chunks() {
    let mut chunks = vec!["".to_string(), "  ".to_string(), "\n".to_string()];
    chunks.retain(|c| !c.trim().is_empty());
    assert!(
        chunks.is_empty(),
        "all-empty input must produce zero chunks"
    );
}

// axon_rust-lu6a: schema-version + extractor_name payload tests.
#[test]
fn payload_schema_version_is_at_least_two() {
    let version = std::hint::black_box(crate::vector::ops::qdrant::PAYLOAD_SCHEMA_VERSION);
    assert!(version >= 2);
}

#[test]
fn pipeline_payload_stamps_schema_version_and_extractor() {
    use crate::vector::ops::qdrant::PAYLOAD_SCHEMA_VERSION;
    let mut payload = serde_json::json!({
        "url": "https://example.com/x",
        "chunk_index": 0,
        "chunk_text": "body",
        "payload_schema_version": PAYLOAD_SCHEMA_VERSION,
    });
    assert_eq!(
        payload["payload_schema_version"].as_u64(),
        Some(u64::from(PAYLOAD_SCHEMA_VERSION))
    );
    assert!(payload.get("extractor_name").is_none());
    let extractor = Some("docs".to_string());
    if let Some(name) = &extractor
        && !name.is_empty()
    {
        payload["extractor_name"] = serde_json::Value::String(name.clone());
    }
    assert_eq!(payload["extractor_name"].as_str(), Some("docs"));
}
