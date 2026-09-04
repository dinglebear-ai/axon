use super::*;

fn snapshot(embedding_model: &str) -> JobConfigSnapshot<'_> {
    JobConfigSnapshot {
        source_kind: "web",
        source_ref: "https://example.com",
        collection: "axon",
        embedding_provider_id: "tei",
        vector_provider_id: "qdrant",
        embedding_model,
        embedding_dimensions: 1024,
        embed: true,
        max_items: None,
    }
}

#[test]
fn same_inputs_produce_same_id() {
    let a = config_snapshot_id(&snapshot("qwen3"));
    let b = config_snapshot_id(&snapshot("qwen3"));
    assert_eq!(a, b);
}

#[test]
fn different_inputs_produce_different_ids() {
    let a = config_snapshot_id(&snapshot("qwen3"));
    let b = config_snapshot_id(&snapshot("other-model"));
    assert_ne!(a, b);
}

#[test]
fn id_has_expected_shape() {
    let id = config_snapshot_id(&snapshot("qwen3"));
    assert!(id.0.starts_with("cfg_"));
    assert_eq!(id.0.len(), "cfg_".len() + 12);
    assert!(id.0["cfg_".len()..].chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn from_json_is_deterministic() {
    let json = r#"{"collection":"axon","qdrant_url":"http://localhost:6333"}"#;
    let a = config_snapshot_id_from_json(json);
    let b = config_snapshot_id_from_json(json);
    assert_eq!(a, b);
    assert_ne!(a, config_snapshot_id_from_json(r#"{"collection":"other"}"#));
}
