use super::*;

#[test]
fn repo_code_delete_body_is_scoped_to_one_repo_file_points() {
    let body = repo_code_points_delete_body("github", "owner-a", "repo-a");
    let must = body["filter"]["must"]
        .as_array()
        .expect("canonical must array");
    assert_eq!(must.len(), 4);
    assert!(must.contains(&serde_json::json!({
        "key": "provider",
        "match": {"value": "github"}
    })));
    assert!(must.contains(&serde_json::json!({
        "key": "git_owner",
        "match": {"value": "owner-a"}
    })));
    assert!(must.contains(&serde_json::json!({
        "key": "git_repo",
        "match": {"value": "repo-a"}
    })));
    assert!(must.contains(&serde_json::json!({
        "key": "git_content_kind",
        "match": {"value": "file"}
    })));
}

// T-H3: stale-tail filter shape tests ----------------------------------------

#[test]
fn stale_tail_filter_contains_url_match_and_chunk_index_gte() {
    let body = stale_tail_filter_body("https://example.com/page", 5);
    let must = body["filter"]["must"].as_array().expect("must array");
    assert_eq!(must.len(), 2, "filter must have exactly 2 conditions");
    assert!(
        must.contains(
            &serde_json::json!({"key": "url", "match": {"value": "https://example.com/page"}})
        ),
        "url match condition missing"
    );
    assert!(
        must.iter()
            .any(|c| c["key"] == "chunk_index" && c["range"]["gte"] == 5),
        "chunk_index gte condition missing or wrong threshold"
    );
}

#[test]
fn stale_tail_filter_gte_threshold_matches_new_chunk_count() {
    // The threshold must equal new_chunk_count so that only chunk_index >= new_chunk_count
    // is deleted — chunk_index == new_chunk_count - 1 (the last valid chunk) is preserved.
    for count in [0usize, 1, 10, 100] {
        let body = stale_tail_filter_body("https://example.com/doc", count);
        let must = body["filter"]["must"].as_array().expect("must array");
        let gte_condition = must
            .iter()
            .find(|c| c["key"] == "chunk_index")
            .expect("chunk_index condition present");
        assert_eq!(
            gte_condition["range"]["gte"].as_u64(),
            Some(count as u64),
            "gte threshold must equal new_chunk_count={count}"
        );
    }
}

#[test]
fn stale_tail_filter_count_1_is_noop_for_single_chunk_docs() {
    // Regression guard for P-H1: a single-chunk document (new_chunk_count == 1)
    // sets gte=1, which matches chunk_index >= 1 — i.e., only orphaned extra chunks
    // are deleted, never chunk_index=0 (the sole surviving chunk). This verifies
    // that the filter is a no-op on a collection containing only the canonical chunk.
    let body = stale_tail_filter_body("https://example.com/single", 1);
    let must = body["filter"]["must"].as_array().expect("must array");
    let gte_condition = must
        .iter()
        .find(|c| c["key"] == "chunk_index")
        .expect("chunk_index condition present");
    assert_eq!(
        gte_condition["range"]["gte"].as_u64(),
        Some(1),
        "gte must be 1 for a single-chunk doc — chunk_index=0 must not be deleted"
    );
}

#[test]
fn stale_tail_filter_count_0_would_delete_all_chunks() {
    // Safety documentation: new_chunk_count == 0 sets gte=0, meaning every chunk
    // (including chunk_index=0) would be deleted. Callers must never pass 0 unless
    // they intend to wipe all chunks for the URL. This test documents the behavior
    // rather than asserting it should not happen.
    let body = stale_tail_filter_body("https://example.com/empty", 0);
    let must = body["filter"]["must"].as_array().expect("must array");
    let gte_condition = must
        .iter()
        .find(|c| c["key"] == "chunk_index")
        .expect("chunk_index condition present");
    assert_eq!(
        gte_condition["range"]["gte"].as_u64(),
        Some(0),
        "gte=0 deletes all chunks including chunk_index=0"
    );
}
