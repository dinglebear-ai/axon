use super::*;
use serde_json::json;

fn point() -> VectorPoint {
    VectorPoint {
        point_id: VectorPointId::new("point"),
        chunk_id: ChunkId::new("chunk"),
        vector: vec![],
        sparse_vector: None,
        payload: MetadataMap(
            [
                ("source_item_key".into(), json!("canonical/file.md")),
                (
                    "source_item_aliases".into(),
                    json!(["plugins/copied/file.md"]),
                ),
                (
                    "source_path_prefixes".into(),
                    json!([
                        "/",
                        "canonical",
                        "canonical/file.md",
                        "plugins",
                        "plugins/copied",
                        "plugins/copied/file.md"
                    ]),
                ),
                (
                    "item_canonical_uri_aliases".into(),
                    json!(["https://example.com/plugins/copied/file.md"]),
                ),
            ]
            .into_iter()
            .collect(),
        ),
    }
}

#[test]
fn alias_paths_match_prefix_without_matching_sibling_names() {
    let point = point();
    assert!(payload_matches_path_prefix(
        &point.payload,
        &json!("plugins/copied")
    ));
    assert!(!payload_matches_path_prefix(
        &point.payload,
        &json!("plugins/copy")
    ));
    assert!(!payload_matches_path_prefix(
        &point.payload,
        &json!("removed")
    ));
}

#[test]
fn alias_paths_match_search_but_cannot_delete_shared_canonical_uri() {
    let point = point();
    let request = VectorSearchRequest {
        collection: "test".into(),
        query: "test".into(),
        limit: 1,
        dense_vector: None,
        sparse_vector: None,
        filters: MetadataMap(
            [("source_item_key".into(), json!("plugins/copied/file.md"))]
                .into_iter()
                .collect(),
        ),
        hybrid: None,
        generation: None,
        graph_refs: vec![],
        metadata: MetadataMap::new(),
    };
    assert!(matches_search_filters(&point, &request));
    assert!(!payload_url_matches(
        &point.payload,
        "https://example.com/plugins/copied/file.md",
        false
    ));
    let mut removed = point;
    removed.payload.remove("source_item_aliases");
    removed.payload.insert(
        "source_path_prefixes".into(),
        json!(["/", "canonical", "canonical/file.md"]),
    );
    removed.payload.remove("item_canonical_uri_aliases");
    assert!(!matches_search_filters(&removed, &request));
    assert!(!payload_url_matches(
        &removed.payload,
        "https://example.com/plugins/copied/file.md",
        false
    ));
}

#[test]
fn prefix_filter_requires_a_string_and_normalizes_trailing_slashes() {
    assert!(validate_filter_value(PATH_PREFIX, &json!(true), ErrorStage::Retrieving).is_err());
    assert!(
        validate_filter_value(PATH_PREFIX, &json!(["plugins"]), ErrorStage::Retrieving).is_err()
    );
    assert!(payload_matches_path_prefix(
        &point().payload,
        &json!("plugins/copied/")
    ));
    assert!(payload_matches_path_prefix(&point().payload, &json!("/")));
}
