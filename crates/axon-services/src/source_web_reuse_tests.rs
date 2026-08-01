use std::sync::Arc;

use async_trait::async_trait;
use axon_adapters::boundary::{
    FakeAdapterProviders, FetchProvider, RenderProvider, Result as AdapterResult,
};
use axon_api::source::*;
use axon_core::boundary::DocumentCache;
use tokio::sync::Mutex;

#[derive(Debug)]
struct ConditionalState {
    body: String,
    etag: String,
    conditional_304: bool,
    conditional_fetches: usize,
    full_fetches: usize,
}

#[derive(Clone)]
struct ConditionalFetchProvider {
    state: Arc<Mutex<ConditionalState>>,
    capabilities: Arc<FakeAdapterProviders>,
}

impl ConditionalFetchProvider {
    fn new(body: &str, etag: &str) -> Self {
        Self {
            state: Arc::new(Mutex::new(ConditionalState {
                body: body.to_string(),
                etag: etag.to_string(),
                conditional_304: false,
                conditional_fetches: 0,
                full_fetches: 0,
            })),
            capabilities: Arc::new(FakeAdapterProviders::new()),
        }
    }

    async fn set_body(&self, body: &str) {
        self.state.lock().await.body = body.to_string();
    }

    async fn set_conditional_304(&self, enabled: bool) {
        self.state.lock().await.conditional_304 = enabled;
    }

    async fn conditional_fetches(&self) -> usize {
        self.state.lock().await.conditional_fetches
    }

    async fn full_fetches(&self) -> usize {
        self.state.lock().await.full_fetches
    }
}

#[async_trait]
impl FetchProvider for ConditionalFetchProvider {
    async fn fetch(&self, request: FetchRequest) -> AdapterResult<FetchedResource> {
        let mut state = self.state.lock().await;
        let conditional = request
            .headers
            .headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case("If-None-Match"))
            .map(|header| header.value.clone());
        let status = if conditional.is_some() {
            state.conditional_fetches += 1;
            if state.conditional_304 && conditional.as_deref() == Some(state.etag.as_str()) {
                304
            } else {
                200
            }
        } else {
            state.full_fetches += 1;
            200
        };
        let body = if status == 304 {
            String::new()
        } else {
            state.body.clone()
        };
        Ok(FetchedResource {
            uri: request.uri.clone(),
            final_uri: request.uri,
            status,
            content: ContentRef::InlineText { text: body.clone() },
            headers: RedactedHeaders {
                headers: Vec::new(),
            },
            fetched_at: Timestamp("2026-08-01T00:00:00Z".to_string()),
            etag: Some(state.etag.clone()),
            redirect_chain: Vec::new(),
            bytes: Some(body.len() as u64),
            metadata: MetadataMap::new(),
        })
    }

    async fn capabilities(&self) -> AdapterResult<ProviderCapability> {
        FetchProvider::capabilities(self.capabilities.as_ref()).await
    }
}

#[async_trait]
impl RenderProvider for ConditionalFetchProvider {
    async fn render(&self, request: RenderRequest) -> AdapterResult<RenderedResource> {
        self.capabilities.render(request).await
    }

    async fn capabilities(&self) -> AdapterResult<ProviderCapability> {
        RenderProvider::capabilities(self.capabilities.as_ref()).await
    }
}

fn page_request() -> SourceRequest {
    let mut request = SourceRequest::new("https://docs.example.test/intro");
    request.scope = Some(SourceScope::Page);
    request
        .options
        .values
        .insert("render_mode".to_string(), serde_json::json!("http"));
    request
        .options
        .values
        .insert("etag_conditional".to_string(), serde_json::json!(true));
    request
        .options
        .values
        .insert("cache_policy".to_string(), serde_json::json!("revalidate"));
    request
}

#[tokio::test]
async fn canonical_web_304_reuses_cache_and_cache_miss_refetches() {
    let provider = Arc::new(ConditionalFetchProvider::new(
        "<html><body>version one</body></html>",
        "etag-v1",
    ));
    let harness =
        crate::test_support::source_context_with_web_providers(provider.clone(), provider.clone())
            .await
            .expect("canonical web harness");

    let first = crate::source::index_source_with_auth(
        page_request(),
        harness.ctx(),
        Some(AuthSnapshot::trusted_system("canonical-304-test")),
    )
    .await
    .expect("seed committed generation");
    let embed_calls_after_first = harness.embedder().calls().await.len();
    let vector_calls_after_first = harness.vectors().calls().await;
    assert!(embed_calls_after_first > 0);
    assert!(
        vector_calls_after_first
            .iter()
            .any(|call| *call == "upsert")
    );

    provider
        .set_body("<html><body>transient discovery change</body></html>")
        .await;
    provider.set_conditional_304(true).await;
    let second = crate::source::index_source_with_auth(
        page_request(),
        harness.ctx(),
        Some(AuthSnapshot::trusted_system("canonical-304-test")),
    )
    .await
    .expect("304 reuse should publish");

    assert_ne!(second.ledger.generation, first.ledger.generation);
    assert_eq!(
        harness.embedder().calls().await.len(),
        embed_calls_after_first
    );
    assert_eq!(
        harness
            .vectors()
            .calls()
            .await
            .iter()
            .filter(|call| **call == "upsert")
            .count(),
        vector_calls_after_first
            .iter()
            .filter(|call| **call == "upsert")
            .count(),
        "cache-hit 304 must not embed or upsert changed content"
    );
    assert_eq!(provider.conditional_fetches().await, 1);

    harness
        .core()
        .invalidate(DocumentCacheInvalidation::Generation {
            generation: second.ledger.generation.clone(),
        })
        .await
        .expect("evict reused cache generation");
    provider
        .set_body("<html><body>another discovery change</body></html>")
        .await;
    let embeds_before_refetch = harness.embedder().calls().await.len();
    let full_fetches_before_refetch = provider.full_fetches().await;
    let third = crate::source::index_source_with_auth(
        page_request(),
        harness.ctx(),
        Some(AuthSnapshot::trusted_system("canonical-304-test")),
    )
    .await
    .expect("cache miss should refetch and publish");

    assert_ne!(third.ledger.generation, second.ledger.generation);
    assert_eq!(provider.conditional_fetches().await, 2);
    assert_eq!(
        provider.full_fetches().await,
        full_fetches_before_refetch + 2,
        "third run should perform discovery plus one unconditional cache-miss refetch"
    );
    assert!(harness.embedder().calls().await.len() > embeds_before_refetch);
    assert!(
        third
            .warnings
            .iter()
            .any(|warning| warning.code == "source.reuse.cache_miss_refetch")
    );

    let committed = harness
        .ledger()
        .committed_generation(third.source_id.clone())
        .await
        .expect("committed generation lookup");
    assert_eq!(committed, Some(third.ledger.generation));
}

#[tokio::test]
async fn canonical_vector_commit_failure_rolls_back_generation() {
    use axon_vectors::store::{FakeVectorMode, FakeVectorStore};

    let provider = Arc::new(ConditionalFetchProvider::new(
        "<html><body>rollback fixture</body></html>",
        "etag-rollback",
    ));
    let vectors =
        Arc::new(FakeVectorStore::new("fake-vector").with_mode(FakeVectorMode::CommitFailure));
    let harness = crate::test_support::source_context_with_web_providers_and_vectors(
        provider.clone(),
        provider,
        vectors.clone(),
    )
    .await
    .expect("canonical rollback harness");
    let request = page_request();
    let source_id = crate::source::routing::resolve_source_route(&request)
        .expect("web route")
        .route
        .source
        .source_id;

    let error = crate::source::index_source_with_auth(
        request,
        harness.ctx(),
        Some(AuthSnapshot::trusted_system("canonical-rollback-test")),
    )
    .await
    .expect_err("vector commit failure must fail the source operation");

    assert!(format!("{error:#}").contains("commit_failed"), "{error:#}");
    let calls = vectors.calls().await;
    assert!(calls.iter().any(|call| *call == "upsert"));
    assert!(
        calls
            .iter()
            .any(|call| *call == "mark_generation_committed")
    );
    assert!(calls.iter().any(|call| *call == "delete"));
    assert_eq!(
        harness
            .ledger()
            .committed_generation(source_id)
            .await
            .expect("committed generation lookup"),
        None,
        "ledger watermark must not advance when vector commit fails"
    );
}
