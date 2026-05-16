use super::*;

#[test]
fn collection_from_config_extracts_collection() {
    let json = serde_json::json!({"collection": "cortex"});
    assert_eq!(collection_from_config(&json), Some("cortex"));
}

#[test]
fn collection_from_config_returns_none_for_missing() {
    let json = serde_json::json!({});
    assert_eq!(collection_from_config(&json), None);
}

#[test]
fn collection_from_config_returns_none_for_non_string() {
    let json = serde_json::json!({"collection": 42});
    assert_eq!(collection_from_config(&json), None);
}

#[test]
fn collection_from_config_handles_null() {
    let json = serde_json::json!(null);
    assert_eq!(collection_from_config(&json), None);
}

#[test]
fn display_embed_input_uses_crawl_url_for_domain_output_path() {
    let crawl_id = uuid::Uuid::parse_str("2313c2c5-29b8-46a6-a98d-2338f6b09a9d")
        .expect("test UUID should parse");
    let mut crawl_url_map = std::collections::HashMap::new();
    crawl_url_map.insert(crawl_id, "https://mem0.ai/");

    let label = display_embed_input(
        ".cache/axon-rust/output/domains/mem0.ai/2313c2c5-29b8-46a6-a98d-2338f6b09a9d/markdown",
        &crawl_url_map,
    );

    assert_eq!(label, "https://mem0.ai/");
}

#[test]
fn display_embed_input_preserves_path_when_crawl_url_is_unknown() {
    let crawl_url_map = std::collections::HashMap::new();

    let label = display_embed_input(
        ".cache/axon-rust/output/domains/mem0.ai/2313c2c5-29b8-46a6-a98d-2338f6b09a9d/markdown",
        &crawl_url_map,
    );

    assert_eq!(label, "2313c2c5-29b8-46a6-a98d-2338f6b09a9d/markdown");
}
