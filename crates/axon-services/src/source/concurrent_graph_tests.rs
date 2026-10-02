use super::*;
use axon_adapters::boundary::{FakeAdapterProviders, RenderProvider};
use axon_api::source::{
    EmbeddingBatch, EmbeddingResult, ProviderCapability, RenderRequest, RenderedResource,
    SourceRefreshPolicy,
};
use axon_embedding::{fake::FakeEmbeddingProvider, provider::EmbeddingProvider};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::Notify;

struct GatedEmbedding {
    inner: FakeEmbeddingProvider,
    entered: Notify,
    release: Notify,
    fail: bool,
    released: AtomicBool,
}

#[async_trait::async_trait]
impl EmbeddingProvider for GatedEmbedding {
    async fn embed(
        &self,
        batch: EmbeddingBatch,
    ) -> axon_embedding::provider::Result<EmbeddingResult> {
        self.entered.notify_one();
        loop {
            let released = self.release.notified();
            tokio::pin!(released);
            released.as_mut().enable();
            if self.released.load(Ordering::Acquire) {
                break;
            }
            released.await;
        }
        if self.fail {
            return Err(axon_api::source::ApiError::new(
                "provider.test_embedding_failure",
                axon_api::source::ErrorStage::Embedding,
                "injected embedding failure after graph stage progress",
            ));
        }
        self.inner.embed(batch).await
    }
    async fn capabilities(&self) -> axon_embedding::provider::Result<ProviderCapability> {
        self.inner.capabilities().await
    }
}

struct GraphContent(Arc<FakeAdapterProviders>);
#[async_trait::async_trait]
impl RenderProvider for GraphContent {
    async fn render(
        &self,
        request: RenderRequest,
    ) -> Result<RenderedResource, axon_api::source::ApiError> {
        let mut output = self.0.render(request).await?;
        output.markdown = format!(
            "# Concurrent publication\n\n{}\n## Durable graph staging\n\n{}",
            "This documentation describes source generations and searchable graph evidence. "
                .repeat(32),
            "Graph staging preserves the preceding committed source while embedding is pending. "
                .repeat(32)
        );
        output.text = Some(output.markdown.clone());
        Ok(output)
    }
    async fn capabilities(&self) -> Result<ProviderCapability, axon_api::source::ApiError> {
        RenderProvider::capabilities(self.0.as_ref()).await
    }
}

fn gated_context(ctx: &ServiceContext, gate: Arc<GatedEmbedding>) -> ServiceContext {
    let mut runtime = ctx.target_local_source_runtime().unwrap().clone();
    let donor = TargetLocalSourceRuntime::new(
        runtime.jobs.clone(),
        runtime.ledger.clone(),
        gate.clone(),
        runtime.vector_store.clone(),
        runtime.embedding_provider_id.clone(),
        runtime.embedding_model.clone(),
        runtime.embedding_dimensions,
    );
    runtime.embedding_provider = gate;
    runtime.verified_embedding = donor.verified_embedding;
    ctx.clone().with_target_local_source_runtime(runtime)
}

async fn wait_for_private_graph_progress(pool: &sqlx::SqlitePool) -> anyhow::Result<String> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let row: Option<(String, String)> = sqlx::query_as(
                "SELECT stage_id,path FROM graph_stages WHERE state='building' LIMIT 1",
            )
            .fetch_optional(pool)
            .await?;
            if let Some((id, path)) = row {
                let options = sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(path)
                    .read_only(true)
                    .busy_timeout(Duration::from_millis(100));
                if let Ok(stage) = sqlx::sqlite::SqlitePoolOptions::new()
                    .max_connections(1)
                    .connect_with(options)
                    .await
                {
                    let journal =
                        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM stage_journal")
                            .fetch_one(&stage)
                            .await;
                    let nodes = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM graph_nodes")
                        .fetch_one(&stage)
                        .await;
                    stage.close().await;
                    if journal.is_ok_and(|n| n > 0) && nodes.is_ok_and(|n| n > 0) {
                        return Ok::<_, anyhow::Error>(id);
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?
}

async fn assert_unpublished(pool: &sqlx::SqlitePool) {
    let nodes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_nodes")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(nodes, 0, "private stage rows cannot alter the live graph");
    let committed: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sources WHERE committed_generation IS NOT NULL")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(
        committed, 0,
        "embedding must settle before source publication"
    );
    let finished: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM jobs WHERE status IN ('completed','completed_degraded')",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        finished, 0,
        "blocked ingestion cannot complete its durable job"
    );
}

async fn exercise_blocked_embedding(fail: bool) {
    let providers = Arc::new(FakeAdapterProviders::new());
    let harness = Box::pin(crate::test_support::source_context_with_web_providers(
        providers.clone(),
        Arc::new(GraphContent(providers)),
    ))
    .await
    .unwrap();
    let gate = Arc::new(GatedEmbedding {
        inner: FakeEmbeddingProvider::new("fake-embedding", 8),
        entered: Notify::new(),
        release: Notify::new(),
        fail,
        released: AtomicBool::new(false),
    });
    let ctx = gated_context(harness.ctx(), gate.clone());
    let pool = ctx.sqlite_pool().unwrap();
    let mut request = SourceRequest::new("https://example.test/concurrent-graph");
    request.scope = Some(SourceScope::Page);
    request.refresh = SourceRefreshPolicy::Force;
    let ingestion = tokio::spawn(async move {
        Box::pin(index_source_with_auth(
            request,
            &ctx,
            Some(AuthSnapshot::trusted_system("concurrent-graph-test")),
        ))
        .await
    });
    let entered = tokio::time::timeout(Duration::from_secs(10), gate.entered.notified()).await;
    let progress = if entered.is_ok() {
        wait_for_private_graph_progress(pool.as_ref()).await
    } else {
        Err(anyhow::anyhow!("embedding never entered"))
    };
    if progress.is_ok() {
        assert_unpublished(pool.as_ref()).await;
    }
    let pending = !ingestion.is_finished();
    gate.released.store(true, Ordering::Release);
    gate.release.notify_waiters();
    let output = tokio::time::timeout(Duration::from_secs(10), ingestion)
        .await
        .unwrap()
        .unwrap();
    let stage_id = progress
        .expect("real private journal and node writes must progress while embedding is blocked");
    assert!(pending, "publication waits for blocked embedding");
    if fail {
        assert!(
            output.is_err(),
            "embedding failure must prevent publication"
        );
        assert_unpublished(pool.as_ref()).await;
    } else {
        let output = output.unwrap();
        assert_eq!(output.status, LifecycleStatus::Completed);
        assert!(output.counts.chunks_total > 0);
        let committed: Option<String> =
            sqlx::query_scalar("SELECT committed_generation FROM sources WHERE source_id=?")
                .bind(&output.source_id.0)
                .fetch_one(pool.as_ref())
                .await
                .unwrap();
        assert_eq!(
            committed.as_deref(),
            Some(output.ledger.generation.0.as_str())
        );
        let nodes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_nodes")
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
        assert!(nodes > 0, "publication activates staged graph rows");
        let receipt: (String, String) = sqlx::query_as(
            "SELECT source_id,generation_id FROM graph_stage_receipts WHERE stage_id=?",
        )
        .bind(stage_id)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
        assert_eq!(
            receipt,
            (output.source_id.0, output.ledger.generation.0),
            "atomic publication persists the receipt beyond stage disposal"
        );
    }
}

#[tokio::test]
async fn graph_stage_writes_progress_while_embedding_blocks_publication() {
    Box::pin(exercise_blocked_embedding(false)).await;
}

#[tokio::test]
async fn embedding_failure_after_graph_progress_preserves_unpublished_live_state() {
    Box::pin(exercise_blocked_embedding(true)).await;
}

#[tokio::test]
async fn map_inventory_publishes_only_source_container_graph_without_document_claims() {
    let harness = Box::pin(crate::test_support::source_context_with_fake_web())
        .await
        .unwrap();
    let urls = [
        "https://example.test/docs/intro",
        "https://example.test/docs/api",
    ];
    let mut request = SourceRequest::new("https://example.test/docs");
    request.scope = Some(SourceScope::Map);
    request.refresh = SourceRefreshPolicy::Force;
    request
        .options
        .values
        .insert("map_urls".to_string(), serde_json::json!(urls));
    let result = Box::pin(index_source_with_auth(
        request,
        harness.ctx(),
        Some(AuthSnapshot::trusted_system("map-graph-regression")),
    ))
    .await
    .unwrap();
    assert_eq!(result.status, LifecycleStatus::Completed);
    assert!(result.errors.is_empty());
    assert!(result.warnings.is_empty());
    assert!(!result.graph.degraded);
    assert_eq!(result.counts.items_total, 2);
    assert_eq!(result.counts.documents_total, 0);
    assert_eq!(result.counts.chunks_total, 0);
    let manifest = harness
        .ledger()
        .get_manifest(result.source_id.clone(), result.ledger.generation.clone())
        .await
        .unwrap()
        .unwrap();
    let mut retained_urls = manifest
        .items
        .iter()
        .map(|item| item.canonical_uri.as_str())
        .collect::<Vec<_>>();
    retained_urls.sort_unstable();
    let mut expected_urls = urls.to_vec();
    expected_urls.sort_unstable();
    assert_eq!(
        retained_urls, expected_urls,
        "discovery inventory retains both mapped URLs"
    );
    let pool = harness.ctx().sqlite_pool().unwrap();
    let summary = axon_graph::GraphStage::activation_summary(
        pool.as_ref(),
        &result.source_id,
        &result.ledger.generation,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(summary.nodes_upserted, 1);
    assert_eq!(summary.edges_upserted, 0);
    let nodes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_nodes")
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    let edges: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(
        (nodes, edges),
        (1, 0),
        "Map publishes its source container without discovered document graph rows"
    );
    let statuses: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM document_status WHERE source_id=?")
            .bind(&result.source_id.0)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
    assert_eq!(
        statuses, 0,
        "discovered URLs must not claim indexed document dispositions"
    );
    assert!(harness.embedder().calls().await.is_empty());
}
