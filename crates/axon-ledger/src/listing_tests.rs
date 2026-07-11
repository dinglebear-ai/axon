use super::*;

fn ts() -> Timestamp {
    Timestamp("2026-07-01T00:00:00Z".to_string())
}

fn summary(id: &str, kind: SourceKind, uri: &str, tags: Vec<&str>) -> SourceSummary {
    SourceSummary {
        source_id: SourceId::new(id),
        canonical_uri: uri.to_string(),
        display_name: format!("{id}-display"),
        source_kind: kind,
        adapter: AdapterRef {
            name: "web".to_string(),
            version: "test".to_string(),
        },
        authority: AuthorityLevel::Verified,
        status: LifecycleStatus::Completed,
        counts: SourceCounts {
            items_total: 1,
            items_changed: 0,
            documents_total: 1,
            chunks_total: 3,
            vector_points_total: 3,
            bytes_total: 100,
        },
        created_at: ts(),
        updated_at: ts(),
        watch_id: None,
        graph_node_ids: Vec::new(),
        last_job_id: None,
        last_refreshed_at: None,
        tags: tags.into_iter().map(str::to_string).collect(),
        user_label: None,
    }
}

fn empty_request() -> SourceListRequest {
    SourceListRequest {
        source_kind: None,
        adapter: None,
        status: None,
        authority: None,
        watch_enabled: None,
        tag: None,
        query: None,
        limit: None,
        cursor: None,
    }
}

#[test]
fn resolve_limit_defaults_and_clamps() {
    assert_eq!(resolve_limit(None), DEFAULT_LIST_SOURCES_LIMIT);
    assert_eq!(resolve_limit(Some(0)), 1);
    assert_eq!(resolve_limit(Some(10_000)), MAX_LIST_SOURCES_LIMIT);
    assert_eq!(resolve_limit(Some(25)), 25);
}

#[test]
fn matches_request_filters_by_kind_and_query() {
    let source = summary(
        "src_a",
        SourceKind::Web,
        "https://docs.example.com",
        vec!["docs"],
    );

    let mut request = empty_request();
    assert!(matches_request(&source, &request));

    request.source_kind = Some(SourceKind::Git);
    assert!(!matches_request(&source, &request));

    request.source_kind = Some(SourceKind::Web);
    request.query = Some("example.com".to_string());
    assert!(matches_request(&source, &request));

    request.query = Some("nomatch".to_string());
    assert!(!matches_request(&source, &request));
}

#[test]
fn matches_request_filters_by_tag_and_watch_enabled() {
    let mut source = summary(
        "src_a",
        SourceKind::Web,
        "https://a.example.com",
        vec!["blog"],
    );
    let mut request = empty_request();
    request.tag = Some("BLOG".to_string()); // case-insensitive
    assert!(matches_request(&source, &request));

    request.tag = Some("news".to_string());
    assert!(!matches_request(&source, &request));

    request.tag = None;
    request.watch_enabled = Some(true);
    assert!(!matches_request(&source, &request));

    source.watch_id = Some(WatchId::new("watch_1"));
    assert!(matches_request(&source, &request));
}

#[test]
fn list_page_filters_sorts_and_paginates() {
    let sources = vec![
        summary("src_c", SourceKind::Web, "https://c.example.com", vec![]),
        summary("src_a", SourceKind::Web, "https://a.example.com", vec![]),
        summary("src_b", SourceKind::Git, "owner/repo", vec![]),
    ];

    let mut request = empty_request();
    request.source_kind = Some(SourceKind::Web);
    let page = list_page(sources.clone(), &request);
    assert_eq!(page.total, Some(2));
    assert_eq!(
        page.items
            .iter()
            .map(|s| s.source_id.0.clone())
            .collect::<Vec<_>>(),
        vec!["src_a".to_string(), "src_c".to_string()]
    );
    assert_eq!(page.next_cursor, None);

    let mut request = empty_request();
    request.limit = Some(1);
    let first_page = list_page(sources.clone(), &request);
    assert_eq!(first_page.total, Some(3));
    assert_eq!(first_page.items.len(), 1);
    assert_eq!(first_page.items[0].source_id.0, "src_a");
    let cursor = first_page.next_cursor.expect("more pages remain");

    request.cursor = Some(cursor);
    let second_page = list_page(sources.clone(), &request);
    assert_eq!(second_page.items.len(), 1);
    assert_eq!(second_page.items[0].source_id.0, "src_b");
    assert!(second_page.next_cursor.is_some());

    request.cursor = second_page.next_cursor;
    let third_page = list_page(sources, &request);
    assert_eq!(third_page.items.len(), 1);
    assert_eq!(third_page.items[0].source_id.0, "src_c");
    assert_eq!(third_page.next_cursor, None);
}

#[test]
fn list_page_empty_input_returns_empty_page() {
    let page = list_page(Vec::new(), &empty_request());
    assert!(page.items.is_empty());
    assert_eq!(page.total, Some(0));
    assert_eq!(page.next_cursor, None);
}
