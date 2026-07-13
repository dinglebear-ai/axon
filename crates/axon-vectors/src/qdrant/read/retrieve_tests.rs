use super::*;
use serde_json::json;

fn point(id: &str, payload: serde_json::Value) -> QdrantScrolledPoint {
    QdrantScrolledPoint {
        id: json!(id),
        payload,
    }
}

#[test]
fn canonical_candidates_dedupes_and_orders_normalized_first() {
    let candidates = canonical_first_url_candidates("https://Example.com/docs/");
    assert!(!candidates.is_empty());
    // Normalized form comes first; the raw input is not duplicated if it
    // already matches an earlier variant.
    assert!(candidates.iter().all(|c| !c.is_empty()));
    let mut seen = std::collections::HashSet::new();
    for candidate in &candidates {
        assert!(
            seen.insert(candidate.clone()),
            "duplicate candidate: {candidate}"
        );
    }
}

#[test]
fn canonical_candidates_includes_raw_input_when_distinct() {
    // A schemeless target gets normalized to `https://...`, but the raw
    // (schemeless) input is still tried as a final fallback candidate.
    let candidates = canonical_first_url_candidates("example.com/docs");
    assert!(candidates.contains(&"https://example.com/docs".to_string()));
    assert!(candidates.contains(&"example.com/docs".to_string()));
}

#[test]
fn retrieve_max_points_clamps_to_ceiling() {
    assert_eq!(retrieve_max_points(None), RETRIEVE_MAX_POINTS_CEILING);
    assert_eq!(retrieve_max_points(Some(10)), 10);
    assert_eq!(
        retrieve_max_points(Some(10_000)),
        RETRIEVE_MAX_POINTS_CEILING
    );
}

#[test]
fn url_match_filter_shape() {
    let filter = url_match_filter("https://x/y");
    let should = filter["should"]
        .as_array()
        .expect("canonical should filter");
    let keys = should
        .iter()
        .filter_map(|condition| condition["key"].as_str())
        .collect::<Vec<_>>();
    assert!(keys.contains(&"item_canonical_uri"));
    assert!(keys.contains(&"source_canonical_uri"));
    assert!(keys.contains(&"source_item_key"));
    assert!(keys.contains(&"chunk_locator.canonical_uri"));
    assert!(!keys.contains(&"url"));
}

#[test]
fn retrieve_visibility_filter_adds_must_not() {
    let filter = retrieve_visibility_filter(url_match_filter("https://x/y"));
    assert!(filter["should"].is_array());
    assert_eq!(
        filter["must_not"],
        json!([
            { "is_null": { "key": "committed_generation" } },
            { "key": "visibility", "match": { "value": "redacted" } },
            { "key": "redaction_status", "match": { "value": "failed" } }
        ])
    );
}

#[test]
fn render_full_doc_orders_by_chunk_index() {
    let points = vec![
        point("c-1", json!({"chunk_index": 1, "chunk_text": "second"})),
        point("c-0", json!({"chunk_index": 0, "chunk_text": "first"})),
    ];
    assert_eq!(render_full_doc_from_points(&points), "first\nsecond");
}

#[test]
fn render_full_doc_falls_back_to_text_field() {
    let points = vec![point("c-0", json!({"chunk_index": 0, "text": "fallback"}))];
    assert_eq!(render_full_doc_from_points(&points), "fallback");
}

#[test]
fn render_full_doc_skips_empty_chunks() {
    let points = vec![
        point("c-0", json!({"chunk_index": 0, "chunk_text": ""})),
        point("c-1", json!({"chunk_index": 1, "chunk_text": "kept"})),
    ];
    assert_eq!(render_full_doc_from_points(&points), "kept");
}

#[test]
fn render_full_doc_missing_chunk_index_sorts_last() {
    let points = vec![
        point("c-none", json!({"chunk_text": "no-index"})),
        point("c-0", json!({"chunk_index": 0, "chunk_text": "first"})),
    ];
    assert_eq!(render_full_doc_from_points(&points), "first\nno-index");
}

#[test]
fn render_full_doc_empty_input_is_empty_string() {
    assert_eq!(render_full_doc_from_points(&[]), "");
}
