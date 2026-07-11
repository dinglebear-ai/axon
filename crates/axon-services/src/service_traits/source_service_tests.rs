use super::*;

/// Compile-level check: `SourceServiceImpl` satisfies `SourceService` and can
/// be held as a trait object. No live providers are touched — this only
/// proves the production impl type-checks against the trait.
#[allow(dead_code)]
fn assert_source_service_impl_is_object_safe(ctx: Arc<ServiceContext>) -> Arc<dyn SourceService> {
    Arc::new(SourceServiceImpl::new(ctx))
}

fn fake_service() -> Arc<dyn SourceService> {
    Arc::new(FakeSourceService::new())
}

#[tokio::test]
async fn fake_source_service_submit_returns_result() {
    let fake = fake_service();
    let request = SourceRequest::new("https://example.com/docs");
    let result = fake.submit(request).await.expect("submit should succeed");
    assert_eq!(result.canonical_uri, "https://example.com/docs");
}

#[tokio::test]
async fn fake_source_service_run_now_returns_result() {
    let fake = fake_service();
    let request = SourceRequest::new("https://example.com/docs");
    let result = fake.run_now(request).await.expect("run_now should succeed");
    assert_eq!(result.canonical_uri, "https://example.com/docs");
}

#[tokio::test]
async fn fake_source_service_resolve_returns_resolved_source() {
    let fake = fake_service();
    let request = SourceRequest::new("https://example.com/docs");
    let resolved = fake.resolve(request).await.expect("resolve should succeed");
    assert_eq!(resolved.canonical_uri, "https://example.com/docs");
}

#[tokio::test]
async fn fake_source_service_get_after_seed() {
    let fake = FakeSourceService::new();
    let request = SourceRequest::new("https://example.com/docs");
    let result = fake.submit(request).await.expect("submit should succeed");

    // The fake doesn't auto-populate `get` from `submit` (production
    // semantics differ), so seed explicitly to exercise `get`.
    let summary = SourceSummary {
        source_id: result.source_id.clone(),
        canonical_uri: result.canonical_uri.clone(),
        display_name: "Example Docs".to_string(),
        source_kind: result.source_kind,
        adapter: result.adapter.clone(),
        authority: axon_api::source::AuthorityLevel::Unknown,
        status: result.status,
        counts: result.counts.clone(),
        created_at: axon_api::source::Timestamp::from(chrono::Utc::now()),
        updated_at: axon_api::source::Timestamp::from(chrono::Utc::now()),
        watch_id: None,
        graph_node_ids: Vec::new(),
        last_job_id: None,
        last_refreshed_at: None,
        tags: Vec::new(),
        user_label: None,
    };
    fake.seed(summary);

    let got = fake
        .get(result.source_id.clone())
        .await
        .expect("get should find seeded source");
    assert_eq!(got.source_id, result.source_id);
}

#[tokio::test]
async fn fake_source_service_get_missing_errors() {
    let fake = fake_service();
    let err = fake
        .get(SourceId::new("missing"))
        .await
        .expect_err("get should error for unknown source id");
    assert!(err.to_string().contains("not found"));
}

#[tokio::test]
async fn fake_source_service_list_reflects_seeded_sources() {
    let fake = FakeSourceService::new();
    let now = axon_api::source::Timestamp::from(chrono::Utc::now());
    let summary = SourceSummary {
        source_id: SourceId::new("fake:one"),
        canonical_uri: "https://example.com/one".to_string(),
        display_name: "One".to_string(),
        source_kind: axon_api::source::SourceKind::Web,
        adapter: axon_api::source::AdapterRef {
            name: "fake".to_string(),
            version: "0".to_string(),
        },
        authority: axon_api::source::AuthorityLevel::Unknown,
        status: axon_api::source::LifecycleStatus::Completed,
        counts: axon_api::source::SourceCounts {
            items_total: 1,
            items_changed: 1,
            documents_total: 1,
            chunks_total: 0,
            vector_points_total: 0,
            bytes_total: 0,
        },
        created_at: now.clone(),
        updated_at: now,
        watch_id: None,
        graph_node_ids: Vec::new(),
        last_job_id: None,
        last_refreshed_at: None,
        tags: Vec::new(),
        user_label: None,
    };
    fake.seed(summary);

    let page = fake
        .list(empty_source_list_request())
        .await
        .expect("list should succeed");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.total, Some(1));
}

#[tokio::test]
async fn fake_source_service_items_returns_empty_page() {
    let fake = fake_service();
    let page = fake
        .items(SourceItemListRequest {
            source_id: SourceId::new("fake:one"),
            limit: None,
            cursor: None,
        })
        .await
        .expect("items should succeed");
    assert!(page.items.is_empty());
    assert_eq!(page.total, Some(0));
}

#[tokio::test]
async fn fake_source_service_generations_returns_empty_page() {
    let fake = fake_service();
    let page = fake
        .generations(SourceGenerationListRequest {
            source_id: SourceId::new("fake:one"),
            limit: None,
            cursor: None,
        })
        .await
        .expect("generations should succeed");
    assert!(page.items.is_empty());
    assert_eq!(page.total, Some(0));
}

fn empty_source_list_request() -> SourceListRequest {
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
