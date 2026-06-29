use super::*;

#[test]
fn cleanup_selector_v1_rejects_empty_scope() {
    let selector = CleanupSelectorV1::new("axon", "", 1, 2, "src/lib.rs");
    assert!(selector.is_err());

    let selector =
        CleanupSelectorV1::new("axon", "source-a", 1, 2, "src/lib.rs").expect("selector");
    let filter = selector.filter();
    let must = filter["must"].as_array().expect("must array");
    assert!(must.contains(&serde_json::json!({
        "key": "source_id",
        "match": {"value": "source-a"}
    })));
    assert!(must.contains(&serde_json::json!({
        "key": "source_item_key",
        "match": {"value": "src/lib.rs"}
    })));
}

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

#[test]
fn repo_file_filter_with_host_omits_owner_when_absent() {
    let body = repo_file_points_filter_with_host("git", "example.com", None, "repo");
    let must = body["must"].as_array().expect("must array");
    assert!(must.contains(&serde_json::json!({
        "key": "provider",
        "match": {"value": "git"}
    })));
    assert!(must.contains(&serde_json::json!({
        "key": "git_host",
        "match": {"value": "example.com"}
    })));
    assert!(must.contains(&serde_json::json!({
        "key": "git_repo",
        "match": {"value": "repo"}
    })));
    assert!(must.contains(&serde_json::json!({
        "key": "git_content_kind",
        "match": {"value": "file"}
    })));
    assert!(
        must.iter().all(|condition| condition["key"] != "git_owner"),
        "generic git targets without owners must not require git_owner"
    );
}

#[test]
fn repo_file_filter_with_host_scopes_subgroup_owner() {
    let body = repo_file_points_filter_with_host(
        "gitlab",
        "gitlab.com",
        Some("group/subgroup"),
        "project",
    );
    let must = body["must"].as_array().expect("must array");
    assert!(must.contains(&serde_json::json!({
        "key": "git_owner",
        "match": {"value": "group/subgroup"}
    })));
}

#[test]
fn repo_legacy_fragment_candidates_require_exact_current_url_prefix() {
    let current = HashSet::from([
        "https://gitlab.com/group/project/-/blob/main/src/lib.rs".to_string(),
        "https://gitlab.com/group/project/-/blob/main/src/lib.rsx".to_string(),
    ]);
    let stale = legacy_repo_fragment_urls(
        [
            "https://gitlab.com/group/project/-/blob/main/src/lib.rs#L1-L2".to_string(),
            "https://gitlab.com/group/project/-/blob/main/src/lib.rsx#L1-L2".to_string(),
            "https://gitlab.com/group/project/-/blob/main/src/lib.rs-old#L1-L2".to_string(),
            "https://gitlab.com/group/project/-/blob/main/src/main.rs#L1-L2".to_string(),
            "https://gitlab.com/group/project/-/blob/main/src/lib.rs".to_string(),
        ],
        &current,
    );
    assert_eq!(
        stale,
        vec![
            "https://gitlab.com/group/project/-/blob/main/src/lib.rs#L1-L2",
            "https://gitlab.com/group/project/-/blob/main/src/lib.rsx#L1-L2",
        ]
    );
}

#[test]
fn local_fragment_cleanup_is_scoped_to_local_embed_legacy_urls() {
    let scroll = local_legacy_fragment_scroll_filter();
    let must = scroll["must"].as_array().expect("must array");
    assert_eq!(must.len(), 1);
    assert!(
        must.iter().all(|condition| condition["key"] != "url"),
        "candidate scan must not depend on full-text URL matching; exact legacy prefix filtering happens in Rust"
    );
    assert!(must.contains(&serde_json::json!({
        "key": "source_type",
        "match": {"value": "embed"}
    })));

    let delete = local_legacy_fragment_delete_body(&["file:///tmp/a/src/lib.rs#L1-L2"]);
    let delete_must = delete["filter"]["must"].as_array().expect("must array");
    assert!(
        delete_must
            .iter()
            .all(|condition| condition["key"] != "code_file_path"),
        "cleanup must not match by non-unique path because that can delete git/provider points"
    );
    assert!(
        delete_must
            .iter()
            .all(|condition| condition["key"] != "domain"),
        "legacy local code fragments used parent-directory domains, not always domain=local"
    );
    assert!(delete_must.contains(&serde_json::json!({
        "key": "source_type",
        "match": {"value": "embed"}
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

#[test]
fn url_target_match_exact_url_or_seed_url() {
    assert!(point_matches_url_target(
        Some("https://docs.example.com/page"),
        None,
        "https://docs.example.com/page",
        false
    ));
    assert!(point_matches_url_target(
        Some("https://docs.example.com/page"),
        Some("https://docs.example.com/"),
        "https://docs.example.com/",
        false
    ));
    assert!(!point_matches_url_target(
        Some("https://docs.example.com/page"),
        None,
        "https://docs.example.com/",
        false
    ));
}

#[test]
fn url_target_match_prefix_respects_path_boundaries() {
    assert!(point_matches_url_target(
        Some("https://docs.example.com/guide/install"),
        None,
        "https://docs.example.com/guide",
        true
    ));
    assert!(point_matches_url_target(
        Some("https://docs.example.com/guide?x=1"),
        None,
        "https://docs.example.com/guide",
        true
    ));
    assert!(!point_matches_url_target(
        Some("https://docs.example.com/guide-old"),
        None,
        "https://docs.example.com/guide",
        true
    ));
}

#[test]
fn local_code_batch_delete_body_is_generation_fenced() {
    let body = local_code_batch_delete_body_for_test(
        "project-1",
        41,
        &["src/lib.rs".to_string(), "src/main.rs".to_string()],
    );
    let must = body["filter"]["must"].as_array().expect("must array");
    assert!(must.contains(&serde_json::json!({
        "key": "source_type",
        "match": {"value": "local_code"}
    })));
    assert!(must.contains(&serde_json::json!({
        "key": "local_project_key",
        "match": {"value": "project-1"}
    })));
    assert!(must.contains(&serde_json::json!({
        "key": "local_index_version",
        "match": {"value": axon_core::CODE_INDEX_VERSION}
    })));
    assert!(must.contains(&serde_json::json!({
        "key": "local_generation",
        "match": {"value": 41}
    })));
    let should = body["filter"]["should"].as_array().expect("should array");
    assert_eq!(should.len(), 2);
}
