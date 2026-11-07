use super::*;
use axon_adapters::{
    FakeSourceAdapter, ReusePolicy, boundary::FakeAdapterProviders, web::WebSourceAdapter,
};
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_jobs::boundary::FakeJobWatchStore;
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::FakeVectorStore;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

struct WebInventory {
    discovery: FakeSourceAdapter,
    web: WebSourceAdapter,
    omit_second: AtomicBool,
}

#[async_trait::async_trait]
impl SourceAdapter for WebInventory {
    fn name(&self) -> &'static str {
        "web"
    }
    fn version(&self) -> &'static str {
        "1"
    }
    fn reuse_policy(&self) -> ReusePolicy {
        self.web.reuse_policy()
    }
    async fn capabilities(&self) -> Result<SourceAdapterCapability, ApiError> {
        self.web.capabilities().await
    }
    async fn discover(&self, plan: &SourcePlan) -> Result<SourceManifest, ApiError> {
        let mut manifest = self.discovery.discover(plan).await?;
        if self.omit_second.load(Ordering::Relaxed) {
            manifest.items.truncate(1);
        }
        manifest.set_inventory_completeness(InventoryCompleteness::Partial);
        Ok(manifest)
    }
    async fn acquire(
        &self,
        plan: &SourcePlan,
        diff: &SourceManifestDiff,
    ) -> Result<SourceAcquisition, ApiError> {
        self.web.acquire(plan, diff).await
    }
    async fn normalize(
        &self,
        plan: &SourcePlan,
        acquisition: SourceAcquisition,
    ) -> Result<StageExecutionResult<Vec<SourceDocument>>, ApiError> {
        self.web.normalize(plan, acquisition).await
    }
}

struct Fixture {
    adapter: Arc<WebInventory>,
    fetch: Arc<FakeAdapterProviders>,
    ledger: Arc<FakeLedgerStore>,
    vectors: Arc<FakeVectorStore>,
    runtime: TargetLocalSourceRuntime,
}

impl Fixture {
    fn new() -> Self {
        let route = crate::source::routing::resolve_source_route(&SourceRequest::new(
            "https://example.test/docs",
        ))
        .unwrap()
        .route;
        let fetch = Arc::new(FakeAdapterProviders::new().with_fetch_text(
            "This documentation explains how source inventory, safe publication, and complete content refresh preserve useful searchable information. ".repeat(4)
        ));
        let adapter = Arc::new(WebInventory {
            discovery: FakeSourceAdapter::new(route.adapter)
                .with_scope(SourceScope::Site)
                .with_item("a", ContentKind::Markdown, "fixture")
                .with_item("b", ContentKind::Markdown, "fixture"),
            web: WebSourceAdapter::new(fetch.clone(), Arc::new(FakeAdapterProviders::new())),
            omit_second: AtomicBool::new(false),
        });
        let ledger = Arc::new(FakeLedgerStore::new());
        let vectors = Arc::new(FakeVectorStore::new("vectors"));
        let runtime = TargetLocalSourceRuntime::new(
            Arc::new(FakeJobWatchStore::new()),
            ledger.clone(),
            Arc::new(FakeEmbeddingProvider::new("embedding", 8)),
            vectors.clone(),
            ProviderId::new("embedding"),
            "fake-embedding",
            8,
        );
        Self {
            adapter,
            fetch,
            ledger,
            vectors,
            runtime,
        }
    }
    async fn run(
        &self,
        refresh: SourceRefreshPolicy,
        max_items: Option<u64>,
    ) -> anyhow::Result<IndexCounts> {
        self.run_render_mode(refresh, max_items, axon_core::config::RenderMode::Http)
            .await
    }

    async fn run_render_mode(
        &self,
        refresh: SourceRefreshPolicy,
        max_items: Option<u64>,
        render_mode: axon_core::config::RenderMode,
    ) -> anyhow::Result<IndexCounts> {
        let source = "https://example.test/docs";
        let mut request = SourceRequest::new(source);
        request.refresh = refresh;
        let route = crate::source::routing::resolve_source_route(&request)?.route;
        let cfg = axon_core::config::Config {
            render_mode,
            ..Default::default()
        };
        crate::source::family_dispatch::dispatch_web_kind(
            self.adapter.clone(),
            &cfg,
            &self.runtime,
            source,
            "refresh",
            "owner",
            SourceScope::Site,
            None,
            true,
            &OutputPolicy::default(),
            &SourceLimits {
                max_items,
                ..Default::default()
            },
            &route,
            &SourceExecutionContext::inline(request, None),
        )
        .await
    }
    async fn legacy_inventory(&self) -> IndexCounts {
        let initial = self.run(SourceRefreshPolicy::IfStale, None).await.unwrap();
        let mut manifest = initial.published_manifest.clone().unwrap();
        manifest
            .metadata
            .insert(PUBLICATION_CONFIG_KEY.into(), "legacy-policy".into());
        self.ledger.put_manifest(manifest).await.unwrap();
        self.adapter.omit_second.store(true, Ordering::Relaxed);
        initial
    }
}

struct GoneFetch(u16);

#[async_trait::async_trait]
impl axon_adapters::boundary::RenderProvider for GoneFetch {
    async fn render(&self, request: RenderRequest) -> Result<RenderedResource, ApiError> {
        let mut rendered =
            axon_adapters::boundary::RenderProvider::render(&FakeAdapterProviders::new(), request)
                .await?;
        rendered
            .metadata
            .insert("web_status".into(), serde_json::json!(self.0));
        Ok(rendered)
    }
    async fn capabilities(&self) -> Result<ProviderCapability, ApiError> {
        axon_adapters::boundary::RenderProvider::capabilities(&FakeAdapterProviders::new()).await
    }
}

#[async_trait::async_trait]
impl axon_adapters::boundary::FetchProvider for GoneFetch {
    async fn fetch(&self, request: FetchRequest) -> Result<FetchedResource, ApiError> {
        let mut fetched = axon_adapters::boundary::FetchProvider::fetch(
            &FakeAdapterProviders::new()
                .with_fetch_text("This missing-page error is not documentation."),
            request,
        )
        .await?;
        fetched.status = self.0;
        Ok(fetched)
    }
    async fn capabilities(&self) -> Result<ProviderCapability, ApiError> {
        axon_adapters::boundary::FetchProvider::capabilities(&FakeAdapterProviders::new()).await
    }
}

#[tokio::test]
async fn web_inventory_rebuild_retires_confirmed_missing_pages() {
    for mode in [
        axon_core::config::RenderMode::Http,
        axon_core::config::RenderMode::AutoSwitch,
        axon_core::config::RenderMode::Chrome,
    ] {
        for status in [404, 410] {
            let mut fixture = Fixture::new();
            let initial = fixture.legacy_inventory().await;
            assert!(!fixture.vectors.points("refresh").await.is_empty());
            Arc::get_mut(&mut fixture.adapter).unwrap().web =
                WebSourceAdapter::new(Arc::new(GoneFetch(status)), Arc::new(GoneFetch(status)));
            let refreshed = fixture
                .run_render_mode(SourceRefreshPolicy::Force, None, mode)
                .await
                .unwrap();
            assert_eq!(refreshed.chunks_prepared, 0);
            assert_ne!(refreshed.generation, initial.generation);
            let manifest = refreshed.published_manifest.unwrap();
            let statuses = fixture
                .ledger
                .document_statuses_for_items(
                    refreshed.source_id.clone(),
                    manifest
                        .items
                        .iter()
                        .map(|item| item.source_item_key.clone())
                        .collect(),
                )
                .await
                .unwrap();
            assert_eq!(statuses.len(), 2);
            assert!(
                statuses
                    .iter()
                    .all(|row| row.status == DocumentLifecycleStatus::Skipped
                        && row.generation.as_ref() == Some(&refreshed.generation))
            );
            let points = fixture.vectors.points("refresh").await;
            assert!(!points.is_empty());
            assert!(points.iter().all(|point| {
                point
                    .payload
                    .get("retired_epoch")
                    .and_then(serde_json::Value::as_i64)
                    .is_some_and(|epoch| epoch > 0)
            }));
            assert!(fixture.vectors.calls().await.contains(&"retire_generation"));
        }
    }
}

#[tokio::test]
async fn web_config_change_reprocesses_retained_pages_instead_of_blocking() {
    for refresh in [SourceRefreshPolicy::IfStale, SourceRefreshPolicy::Force] {
        let fixture = Fixture::new();
        let initial = fixture.legacy_inventory().await;
        let refreshed = fixture.run(refresh, None).await.unwrap();
        assert_eq!(refreshed.documents_prepared, 2);
        assert_eq!(refreshed.removed, 0);
        assert_ne!(refreshed.generation, initial.generation);
        let manifest = refreshed.published_manifest.unwrap();
        assert_eq!(manifest.items.len(), 2);
        assert_eq!(
            manifest.inventory_completeness(),
            InventoryCompleteness::Partial
        );
        let statuses = fixture
            .ledger
            .document_statuses_for_items(
                refreshed.source_id,
                manifest
                    .items
                    .iter()
                    .map(|item| item.source_item_key.clone())
                    .collect(),
            )
            .await
            .unwrap();
        assert_eq!(statuses.len(), 2);
        assert!(
            statuses
                .iter()
                .all(|row| row.generation.as_ref() == Some(&refreshed.generation))
        );
        assert_eq!(
            fixture.fetch.calls().await.len(),
            4,
            "both known pages must be fetched again"
        );
    }
}

#[tokio::test]
async fn web_config_change_cannot_expand_past_requested_inventory_limit() {
    let fixture = Fixture::new();
    let initial = fixture.legacy_inventory().await;
    let error = fixture
        .run(SourceRefreshPolicy::Force, Some(1))
        .await
        .unwrap_err();
    let api = error
        .downcast_ref::<ApiError>()
        .expect("actionable refresh limit error");
    assert_eq!(api.code.to_string(), "source.refresh.inventory_limit");
    assert_eq!(
        fixture
            .ledger
            .committed_generation(&initial.source_id)
            .await,
        Some(initial.generation)
    );
    assert_eq!(fixture.fetch.calls().await.len(), 2);
}

#[tokio::test]
async fn web_inventory_rebuild_fetch_failure_preserves_committed_generation() {
    let mut fixture = Fixture::new();
    let initial = fixture.legacy_inventory().await;
    Arc::get_mut(&mut fixture.adapter).unwrap().web = WebSourceAdapter::new(
        Arc::new(
            FakeAdapterProviders::new().with_mode(axon_adapters::boundary::FakeAdapterMode::Fatal),
        ),
        Arc::new(FakeAdapterProviders::new()),
    );
    let error = fixture
        .run(SourceRefreshPolicy::Force, None)
        .await
        .unwrap_err();
    let api = error
        .downcast_ref::<ApiError>()
        .expect("actionable incomplete refresh error");
    assert_eq!(api.code.to_string(), "source.refresh.inventory_incomplete");
    assert_eq!(
        fixture
            .ledger
            .committed_generation(&initial.source_id)
            .await,
        Some(initial.generation)
    );
}
