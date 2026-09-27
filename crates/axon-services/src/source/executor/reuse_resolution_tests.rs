use super::super::{ResolvedAcquisition, SourcePipelineInput, resolve_acquisition};
use super::*;
use axon_adapters::{FakeSourceAdapter, ReusePolicy, SourceAdapter};
use axon_ledger::store::LedgerStore;
use std::sync::atomic::{AtomicUsize, Ordering};

struct ConditionalAdapter {
    inner: FakeSourceAdapter,
    calls: AtomicUsize,
}
#[async_trait::async_trait]
impl SourceAdapter for ConditionalAdapter {
    fn name(&self) -> &'static str {
        "web"
    }
    fn version(&self) -> &'static str {
        "test"
    }
    fn reuse_policy(&self) -> ReusePolicy {
        ReusePolicy::ConditionalRequest
    }
    async fn capabilities(&self) -> axon_adapters::adapter::Result<SourceAdapterCapability> {
        self.inner.capabilities().await
    }
    async fn discover(&self, plan: &SourcePlan) -> axon_adapters::adapter::Result<SourceManifest> {
        self.inner.discover(plan).await
    }
    async fn normalize(
        &self,
        plan: &SourcePlan,
        acquisition: SourceAcquisition,
    ) -> axon_adapters::adapter::Result<StageExecutionResult<Vec<SourceDocument>>> {
        self.inner.normalize(plan, acquisition).await
    }
    async fn acquire(
        &self,
        plan: &SourcePlan,
        _: &SourceManifestDiff,
    ) -> axon_adapters::adapter::Result<SourceAcquisition> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(
            plan.route.validated_options.values.get("etag_conditional"),
            Some(&serde_json::json!(false))
        );
        let mut result = acquisition_fixture(
            Vec::new(),
            Vec::new(),
            ContentRef::InlineText {
                text: "fresh body".into(),
            },
        );
        result.fetched_items[0].manifest_item.content_hash = Some("fresh-hash".into());
        result.fetched_items[0]
            .manifest_item
            .metadata
            .insert("web_etag".into(), "fresh-etag".into());
        Ok(result)
    }
}

async fn resolve_case(
    compatible: bool,
    failed_sibling: bool,
    response_304: bool,
    cache: Option<ContentRef>,
) -> (ResolvedAcquisition, usize) {
    let adapter = ConditionalAdapter {
        inner: FakeSourceAdapter::new(AdapterRef {
            name: "web".into(),
            version: "test".into(),
        }),
        calls: AtomicUsize::new(0),
    };
    let ledger = Arc::new(FakeLedgerStore::new());
    let mut runtime = TargetLocalSourceRuntime::new(
        Arc::new(FakeJobWatchStore::new()),
        ledger.clone(),
        Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8)),
        Arc::new(FakeVectorStore::new("fake-vector")),
        ProviderId::new("fake-embedding"),
        "fake-embedding",
        8,
    );
    runtime.document_cache = Arc::new(FakeCoreBoundaries::new());
    let request = SourceRequest::new("https://example.test/page");
    let routed = crate::source::routing::resolve_source_route(&request).unwrap();
    let mut plan = crate::source::dispatch::family_source_plan(
        &request.source,
        &routed.route,
        true,
        None,
        None,
    );
    plan.route.source.source_id = SourceId::new("src_reuse");
    let execution = crate::source::SourceExecutionContext::inline(request, None);
    let input = SourcePipelineInput {
        adapter: &adapter,
        plan,
        collection: "test",
        owner_id: "test",
        auth_snapshot: None,
        execution: &execution,
    };
    let content = if response_304 {
        ContentRef::External {
            uri: "reuse://src_reuse/item".into(),
            integrity: None,
        }
    } else {
        ContentRef::InlineText {
            text: "same body".into(),
        }
    };
    let mut acquired = acquisition_fixture(Vec::new(), Vec::new(), content);
    acquired.fetched_items[0].manifest_item.content_hash = Some("same-hash".into());
    let mut prior = acquired.manifest.clone();
    prior.generation = SourceGenerationId::new("gen_previous");
    prior.items[0].content_hash = Some("same-hash".into());
    let identity = if compatible {
        super::super::super::retention::processing_identity(&runtime, &input)
    } else {
        ConfigSnapshotId::new("old-policy")
    };
    prior.metadata.insert(
        super::super::super::PUBLICATION_CONFIG_KEY.into(),
        identity.0.into(),
    );
    seed_source(&ledger, &prior).await;
    ledger.put_manifest(prior.clone()).await.unwrap();
    let mut diff = ledger.diff_manifest(prior.clone()).await.unwrap();
    diff.added.clear();
    diff.modified = prior.items.clone();
    diff.previous_generation = Some(prior.generation.clone());
    diff.next_generation = SourceGenerationId::new("gen_next");
    diff.counts.added = 0;
    diff.counts.modified = 1;
    seed_statuses(&ledger, &prior, failed_sibling).await;
    if let Some(content) = cache {
        let normalized = adapter
            .normalize(
                &input.plan,
                acquisition_fixture(Vec::new(), Vec::new(), content),
            )
            .await
            .unwrap();
        runtime
            .document_cache
            .put(
                DocumentCacheKey {
                    source_id: prior.source_id.clone(),
                    source_item_key: prior.items[0].source_item_key.clone(),
                    generation: Some(prior.generation),
                },
                CachedDocument {
                    document: normalized.data[0].clone(),
                    cached_at: prior.created_at,
                },
            )
            .await
            .unwrap();
    }
    let result = resolve_acquisition(&runtime, &input, &diff, acquired)
        .await
        .unwrap();
    assert_cached_enrichment(&input, &result).await;
    (result, adapter.calls.load(Ordering::SeqCst))
}

async fn assert_cached_enrichment(input: &SourcePipelineInput<'_>, result: &ResolvedAcquisition) {
    let enricher = Arc::new(
        axon_adapters::FakeSourceEnricher::new().with_graph_candidate_kind("doc_reference"),
    );
    let outputs =
        super::super::super::helpers::enrich(enricher, &input.plan, &result.cached_items_to_enrich)
            .await
            .unwrap();
    assert_eq!(outputs.len(), result.cached_documents_to_prepare.len());
    assert!(
        outputs
            .values()
            .all(|output| !output.graph_candidates.is_empty())
    );
}

async fn seed_statuses(ledger: &FakeLedgerStore, prior: &SourceManifest, failed_sibling: bool) {
    let status = DocumentStatus {
        document_id: DocumentId::new("doc"),
        source_id: prior.source_id.clone(),
        source_item_key: prior.items[0].source_item_key.clone(),
        generation: Some(prior.generation.clone()),
        status: DocumentLifecycleStatus::Published,
        updated_at: prior.created_at.clone(),
        chunk_count: 1,
        vector_point_count: 1,
        error: None,
        cleanup_status: None,
    };
    ledger.update_document_status(status.clone()).await.unwrap();
    if failed_sibling {
        let mut sibling = status;
        sibling.document_id = DocumentId::new("failed-sibling");
        sibling.generation = Some(SourceGenerationId::new("failed-attempt"));
        ledger.update_document_status(sibling).await.unwrap();
    }
}

async fn seed_source(ledger: &FakeLedgerStore, manifest: &SourceManifest) {
    ledger
        .upsert_source(SourceSummary {
            source_id: manifest.source_id.clone(),
            canonical_uri: "https://example.test/page".into(),
            display_name: "reuse".into(),
            source_kind: SourceKind::Web,
            adapter: manifest.adapter.clone(),
            authority: AuthorityLevel::UserPinned,
            status: LifecycleStatus::Running,
            counts: super::super::super::helpers::empty_source_counts(),
            created_at: manifest.created_at.clone(),
            updated_at: manifest.created_at.clone(),
            tags: Vec::new(),
            watch_id: None,
            graph_node_ids: Vec::new(),
            last_job_id: None,
            last_refreshed_at: None,
            user_label: None,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn changed_policy_304_reprepares_cached_body_without_refetch() {
    let (result, calls) = resolve_case(
        false,
        false,
        true,
        Some(ContentRef::InlineText {
            text: "cached body".into(),
        }),
    )
    .await;
    assert_eq!(calls, 0);
    assert!(result.reused_item_keys.is_empty());
    assert!(result.acquisition.fetched_items.is_empty());
    assert_eq!(result.cached_documents_to_prepare.len(), 1);
    assert_eq!(result.cached_items_to_enrich.len(), 1);
    assert!(matches!(
        result.cached_items_to_enrich[0].content_ref,
        ContentRef::InlineText { .. }
    ));
}

#[tokio::test]
async fn same_body_200_requires_policy_and_every_sibling_provenance() {
    for (compatible, failed, expected) in [(false, false, 0), (true, true, 0), (true, false, 1)] {
        let (result, calls) = resolve_case(
            compatible,
            failed,
            false,
            Some(ContentRef::InlineText {
                text: "cached body".into(),
            }),
        )
        .await;
        assert_eq!(calls, 0);
        assert_eq!(result.reused_item_keys.len(), expected);
        assert_eq!(result.acquisition.fetched_items.len(), 1 - expected);
    }
}

#[tokio::test]
async fn failed_sibling_304_reprepares_but_healthy_304_reuses() {
    for (failed, reused) in [(true, 0), (false, 1)] {
        let (result, calls) = resolve_case(
            true,
            failed,
            true,
            Some(ContentRef::InlineText {
                text: "cached body".into(),
            }),
        )
        .await;
        assert_eq!(calls, 0);
        assert_eq!(result.reused_item_keys.len(), reused);
        assert_eq!(result.cached_documents_to_prepare.len(), 1 - reused);
    }
}

#[tokio::test]
async fn missing_or_unresolved_cache_refetch_keeps_new_manifest_metadata() {
    for cache in [
        None,
        Some(ContentRef::External {
            uri: "artifact://missing".into(),
            integrity: None,
        }),
    ] {
        let (result, calls) = resolve_case(false, false, true, cache).await;
        assert_eq!(calls, 1);
        assert_eq!(
            result.acquisition.manifest.items[0].content_hash.as_deref(),
            Some("fresh-hash")
        );
        assert_eq!(
            result.acquisition.manifest.items[0]
                .metadata
                .get("web_etag"),
            Some(&serde_json::json!("fresh-etag"))
        );
        assert!(result.reused_item_keys.is_empty());
    }
}
