use super::*;

#[test]
fn projection_preflight_is_all_or_nothing_and_clamps_scrape() {
    let policy = ProjectionBatchPolicy {
        max_input_bytes: 20,
        ..ProjectionBatchPolicy::default()
    };
    let requests = vec![
        SourceRequest::new("https://example.com"),
        SourceRequest::new("https://example.com/a/very/long/path"),
    ];
    assert!(
        preflight_source_batch(
            ProjectionOperation::Scrape,
            requests,
            None,
            &policy,
            &SourceAccessPolicy::default(),
        )
        .is_err()
    );
}

#[test]
fn projection_preflight_code_search_has_no_refresh_and_clamps_window() {
    let policy = ProjectionBatchPolicy {
        max_query_window: 10,
        ..ProjectionBatchPolicy::default()
    };
    let plan = CodeSearchPlan {
        query: "needle".to_string(),
        content_kind: "code".to_string(),
        collection: None,
        limit: 20,
        offset: 8,
        path_prefix: None,
        language: None,
        source: None,
    };
    let result = preflight_code_search_batch(vec![plan], &policy).unwrap();
    assert_eq!(result.items[0].plan.limit, 2);
}

#[test]
fn projection_preflight_rejects_local_escape_before_execution() {
    let request = SourceRequest::new("/etc/passwd");
    let access = SourceAccessPolicy {
        allowed_roots: Some(vec![std::env::temp_dir()]),
        ..SourceAccessPolicy::default()
    };
    assert!(
        preflight_source_batch(
            ProjectionOperation::Ingest,
            vec![request],
            Some(&AuthSnapshot::default()),
            &ProjectionBatchPolicy::default(),
            &access,
        )
        .is_err()
    );
}

#[test]
fn projection_preflight_bounds_normalized_request_bytes() {
    let policy = ProjectionBatchPolicy {
        max_request_bytes: 16,
        ..ProjectionBatchPolicy::default()
    };
    let error = preflight_source_batch(
        ProjectionOperation::Scrape,
        vec![SourceRequest::new("https://example.com")],
        None,
        &policy,
        &SourceAccessPolicy::default(),
    )
    .unwrap_err();
    assert_eq!(error.code.0, "projection.request_too_large");
}
