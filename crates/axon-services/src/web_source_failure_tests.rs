//! `index_web_source` failure/rollback tests.
//!
//! See the module doc on `web_source_tests.rs` for why these use
//! `SourceScope::Page` + `FakeAdapterProviders` instead of multi-page
//! `manifest.jsonl` fixtures (issue #298 Wave 1b retired the disk handoff).
//! The two multi-page tests this file used to carry
//! (`partial_unchanged_vector_copy_failure_keeps_previous_web_generation_visible`,
//! `web_vectorization_batches_more_than_changed_document_limit`) were removed
//! — they required a single `discover` call to enumerate more than one URL,
//! which only `Site`/`Docs` scope can do, and that scope now drives a real
//! `axon-crawl` engine crawl that cannot be exercised hermetically here.

use std::sync::Arc;

use axon_adapters::boundary::FakeAdapterProviders;
use axon_api::source::*;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_embedding::provider::EmbeddingProvider;
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::{FakeVectorMode, FakeVectorStore};

use super::{WebSourceIndexInput, index_web_source};

fn job_id() -> JobId {
    JobId::new(uuid::Uuid::from_u128(0x29812))
}

fn input() -> WebSourceIndexInput {
    let providers = Arc::new(FakeAdapterProviders::new());
    WebSourceIndexInput {
        source: "https://example.com/docs?utm_source=noise&token=secret".to_string(),
        scope: SourceScope::Page,
        map_urls: Vec::new(),
        crawl_options: MetadataMap::new(),
        output: OutputPolicy::default(),
        collection: "axon-web-test".to_string(),
        owner_id: "test-owner".to_string(),
        job_id: job_id(),
        auth_snapshot: None,
        embedding_provider_id: ProviderId::new("fake-embedding"),
        vector_provider_id: ProviderId::new("fake-vector"),
        embedding_model: "fake-embedding".to_string(),
        embedding_dimensions: 8,
        embed: true,
        fetch_provider: providers.clone(),
        render_provider: providers,
        artifact_store: Arc::new(axon_core::boundary::FakeCoreBoundaries::new()),
    }
}

#[tokio::test]
async fn partial_vector_write_failure_rolls_back_web_generation_vectors() {
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector").with_mode(FakeVectorMode::PartialFailure);

    let err = index_web_source(input(), &ledger, &embedder, &vectors)
        .await
        .unwrap_err();

    let rendered = format!("{err:#}");
    assert!(
        rendered.contains("partial_failure") || rendered.contains("partial.failure"),
        "unexpected error: {rendered}"
    );
    assert!(vectors.calls().await.contains(&"delete"));
    assert!(vectors.points("axon-web-test").await.is_empty());
    assert_eq!(ledger.generation_count().await, 1);
}

#[tokio::test]
async fn lost_lease_before_publish_rolls_back_web_generation_vectors() {
    let ledger = FakeLedgerStore::new().with_heartbeat_lost();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let err = index_web_source(input(), &ledger, &embedder, &vectors)
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("lost lease"),
        "unexpected error: {err:#}"
    );
    assert!(vectors.calls().await.contains(&"delete"));
    assert!(vectors.points("axon-web-test").await.is_empty());
    assert_eq!(ledger.generation_count().await, 1);
}

#[tokio::test]
async fn vector_commit_failure_rolls_back_web_generation_vectors() {
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector").with_mode(FakeVectorMode::CommitFailure);

    let err = index_web_source(input(), &ledger, &embedder, &vectors)
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("commit_failed"),
        "unexpected error: {err:#}"
    );
    assert!(vectors.calls().await.contains(&"delete"));
    assert!(vectors.points("axon-web-test").await.is_empty());
    assert_eq!(ledger.generation_count().await, 1);
}

#[tokio::test]
async fn missing_embedding_vector_aborts_without_publishing_web_generation() {
    let ledger = FakeLedgerStore::new();
    let embedder = MissingVectorEmbeddingProvider {
        inner: FakeEmbeddingProvider::new("fake-embedding", 8),
    };
    let vectors = FakeVectorStore::new("fake-vector");

    let err = index_web_source(input(), &ledger, &embedder, &vectors)
        .await
        .unwrap_err();

    let rendered = format!("{err:#}");
    assert!(
        rendered.contains("missing vector"),
        "unexpected error: {rendered}"
    );
    assert!(vectors.calls().await.contains(&"delete"));
    assert!(vectors.points("axon-web-test").await.is_empty());
    assert_eq!(ledger.generation_count().await, 1);
}

struct MissingVectorEmbeddingProvider {
    inner: FakeEmbeddingProvider,
}

#[async_trait::async_trait]
impl EmbeddingProvider for MissingVectorEmbeddingProvider {
    async fn embed(
        &self,
        batch: EmbeddingBatch,
    ) -> axon_embedding::provider::Result<EmbeddingResult> {
        let mut result = self.inner.embed(batch).await?;
        result.vectors.pop();
        Ok(result)
    }

    async fn capabilities(&self) -> axon_embedding::provider::Result<ProviderCapability> {
        self.inner.capabilities().await
    }
}
