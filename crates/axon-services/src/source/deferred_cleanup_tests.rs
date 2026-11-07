use super::*;
use axon_api::source::SourceRefreshPolicy;

#[tokio::test]
async fn refreshed_source_finishes_without_physically_deleting_retired_vectors() {
    let provider = Arc::new(axon_adapters::boundary::FakeAdapterProviders::new());
    let h = crate::test_support::source_context_with_web_providers(
        provider.clone(),
        Arc::new(LongRender(provider, std::sync::atomic::AtomicUsize::new(0))),
    )
    .await
    .unwrap();
    let mut request = SourceRequest::new("https://example.test/docs");
    request.scope = Some(SourceScope::Page);
    request.refresh = SourceRefreshPolicy::Force;
    let first = index_source_with_auth(
        request.clone(),
        h.ctx(),
        Some(AuthSnapshot::trusted_system("cleanup-test")),
    )
    .await
    .unwrap();
    assert_eq!(first.status, LifecycleStatus::Completed);
    assert!(
        first.counts.chunks_total > 0,
        "fixture must actually embed content"
    );
    // A refresh must not synchronously drain durable debt from an earlier publication.
    h.ledger()
        .record_cleanup_debt(axon_api::source::CleanupDebt {
            debt_id: axon_api::source::CleanupDebtId::new("prior-vector-cleanup"),
            job_id: first.job_id,
            origin_attempt: 0,
            source_id: first.source_id.clone(),
            generation: Some(first.ledger.generation.clone()),
            kind: axon_api::source::CleanupDebtKind::VectorDelete,
            selector: axon_api::source::CleanupSelector::Generation {
                source_id: first.source_id.clone(),
                generation: first.ledger.generation.clone(),
            },
            vector_collection: None,
            status: LifecycleStatus::Pending,
            created_at: axon_api::source::Timestamp::from(chrono::Utc::now()),
            attempts: 0,
            last_error: None,
            next_retry_at: None,
            completed_at: None,
        })
        .await
        .unwrap();
    let second = index_source_with_auth(
        request,
        h.ctx(),
        Some(AuthSnapshot::trusted_system("cleanup-test")),
    )
    .await
    .unwrap();
    assert_eq!(second.status, LifecycleStatus::Completed);
    assert!(
        !h.vectors().calls().await.contains(&"delete"),
        "physical cleanup must not run in source completion"
    );
    let pending = h
        .ledger()
        .list_pending_cleanup_debt(second.source_id.clone())
        .await
        .unwrap();
    assert!(
        pending
            .iter()
            .any(|d| d.kind == axon_api::source::CleanupDebtKind::VectorDelete),
        "durable debt must survive job completion"
    );
    assert!(
        pending
            .iter()
            .filter(|d| d.kind == axon_api::source::CleanupDebtKind::VectorDelete)
            .all(|d| d.vector_collection.as_deref() == Some(h.ctx().cfg().collection.as_str())),
        "worker cleanup must retain exact collection identity"
    );
}

struct LongRender(
    Arc<axon_adapters::boundary::FakeAdapterProviders>,
    std::sync::atomic::AtomicUsize,
);
#[async_trait::async_trait]
impl axon_adapters::boundary::RenderProvider for LongRender {
    async fn render(
        &self,
        request: axon_api::source::RenderRequest,
    ) -> Result<axon_api::source::RenderedResource, axon_api::source::ApiError> {
        let mut result =
            axon_adapters::boundary::RenderProvider::render(self.0.as_ref(), request).await?;
        result.markdown = "This useful documentation explains generation publication and durable cleanup retries with sufficient searchable source context. ".repeat(32);
        result.markdown.push_str(&format!(
            "\nVersion {}\n",
            self.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        result.text = Some(result.markdown.clone());
        Ok(result)
    }
    async fn capabilities(
        &self,
    ) -> Result<axon_api::source::ProviderCapability, axon_api::source::ApiError> {
        axon_adapters::boundary::RenderProvider::capabilities(self.0.as_ref()).await
    }
}

#[tokio::test]
async fn generation_graph_is_staged_before_source_publication() {
    let provider = Arc::new(axon_adapters::boundary::FakeAdapterProviders::new());
    let h = crate::test_support::source_context_with_web_providers(
        provider.clone(),
        Arc::new(LongRender(provider, std::sync::atomic::AtomicUsize::new(0))),
    )
    .await
    .unwrap();
    let mut request = SourceRequest::new("https://example.test/staging");
    request.scope = Some(SourceScope::Page);
    request.refresh = SourceRefreshPolicy::Force;
    let output = index_source_with_auth(
        request,
        h.ctx(),
        Some(AuthSnapshot::trusted_system("stage-test")),
    )
    .await
    .unwrap();
    let pool = h.ctx().sqlite_pool().unwrap();
    let messages: Vec<String> =
        sqlx::query_scalar("SELECT message FROM job_events WHERE job_id = ? ORDER BY sequence")
            .bind(output.job_id.0.to_string())
            .fetch_all(pool.as_ref())
            .await
            .unwrap();
    let staged = messages
        .iter()
        .position(|m| m == "generation graph staging complete")
        .expect("graph must finish staging before publication");
    let published = messages
        .iter()
        .position(|m| m == "published source generation")
        .unwrap();
    assert!(staged < published);
    assert!(!output.graph.degraded);
}
