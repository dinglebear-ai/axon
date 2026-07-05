use axon_api::source::*;
use axon_embedding::fake::{FakeEmbeddingMode, FakeEmbeddingProvider};
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::{FakeVectorMode, FakeVectorStore};

use super::{FeedSourceIndexInput, index_feed_source, index_feed_source_with_job};

const RSS_TWO_ITEMS: &str = r#"<?xml version="1.0"?>
<rss version="2.0"><channel>
  <title>Example Feed</title>
  <link>https://example.com/</link>
  <item>
    <title>First Post</title>
    <link>https://example.com/a</link>
    <description>Hello world</description>
  </item>
  <item>
    <title>Second Post</title>
    <link>https://example.com/b</link>
    <description>Body two</description>
  </item>
</channel></rss>"#;

fn job_id() -> JobId {
    JobId::new(uuid::Uuid::from_u128(0x2222))
}

async fn fixture_feed_path(dir: &std::path::Path, contents: &str) -> std::path::PathBuf {
    let path = dir.join("feed.xml");
    tokio::fs::write(&path, contents).await.unwrap();
    path
}

fn input(feed_path: std::path::PathBuf) -> FeedSourceIndexInput {
    FeedSourceIndexInput {
        feed_path,
        collection: "axon-test".to_string(),
        owner_id: "test-owner".to_string(),
        job_id: job_id(),
        embedding_provider_id: ProviderId::new("fake-embedding"),
        vector_provider_id: ProviderId::new("fake-vector"),
        embedding_model: "fake-embedding".to_string(),
        embedding_dimensions: 8,
        embedding_reservations: None,
        vector_reservations: None,
    }
}

#[tokio::test]
async fn vector_failure_leaves_generation_uncommitted_and_releases_lease() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_feed_path(dir.path(), RSS_TWO_ITEMS).await;
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let failing_vectors =
        FakeVectorStore::new("fake-vector").with_mode(FakeVectorMode::Unavailable);

    let err = index_feed_source(input(path.clone()), &ledger, &embedder, &failing_vectors)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("vector store unavailable"),
        "unexpected error: {err:#}"
    );

    let source_id = super::feed_source_id(&tokio::fs::canonicalize(&path).await.unwrap());
    assert_eq!(ledger.committed_generation(&source_id).await, None);
    assert_eq!(ledger.generation_count().await, 0);
    assert_eq!(ledger.manifest_count().await, 0);

    let healthy_vectors = FakeVectorStore::new("fake-vector");
    let output = index_feed_source(input(path), &ledger, &embedder, &healthy_vectors)
        .await
        .unwrap();
    assert_eq!(
        ledger.committed_generation(&output.source_id).await,
        Some(output.generation)
    );
}

#[tokio::test]
async fn embedding_failure_keeps_generation_uncommitted() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_feed_path(dir.path(), RSS_TWO_ITEMS).await;
    let ledger = FakeLedgerStore::new();
    let embedder =
        FakeEmbeddingProvider::new("fake-embedding", 8).with_mode(FakeEmbeddingMode::Fatal);
    let vectors = FakeVectorStore::new("fake-vector");

    let err = index_feed_source(input(path.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("embedding provider failed fatally"),
        "unexpected error: {err:#}"
    );
    let source_id = super::feed_source_id(&tokio::fs::canonicalize(&path).await.unwrap());
    assert_eq!(ledger.committed_generation(&source_id).await, None);
    assert_eq!(ledger.generation_count().await, 0);
    assert_eq!(ledger.manifest_count().await, 0);
}

#[tokio::test]
async fn partial_vector_write_failure_rolls_back_generation_points() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_feed_path(dir.path(), RSS_TWO_ITEMS).await;
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector").with_mode(FakeVectorMode::PartialFailure);

    let err = index_feed_source(input(path.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("wrote"),
        "unexpected error: {err:#}"
    );

    let source_id = super::feed_source_id(&tokio::fs::canonicalize(&path).await.unwrap());
    assert_eq!(ledger.committed_generation(&source_id).await, None);
    assert_eq!(
        vectors.calls().await,
        vec!["ensure_collection", "upsert", "delete"]
    );
    assert!(vectors.points("axon-test").await.is_empty());
}

#[tokio::test]
async fn source_job_terminal_failure_preserves_provider_retryability() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_feed_path(dir.path(), RSS_TWO_ITEMS).await;
    let jobs = FakeJobWatchStore::new();
    let ledger = FakeLedgerStore::new();
    let embedder =
        FakeEmbeddingProvider::new("fake-embedding", 8).with_mode(FakeEmbeddingMode::RateLimited);
    let vectors = FakeVectorStore::new("fake-vector");

    let err = index_feed_source_with_job(input(path), &jobs, &ledger, &embedder, &vectors)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("rate"),
        "unexpected error: {err:#}"
    );

    let summary = JobStore::list(
        &jobs,
        JobListRequest {
            status: None,
            kind: Some(JobKind::Source),
            source_id: None,
            watch_id: None,
            limit: Some(1),
            cursor: None,
        },
    )
    .await
    .unwrap()
    .items
    .pop()
    .expect("job summary");
    let last_error = summary.last_error.expect("last error");
    assert_eq!(
        last_error.provider_id,
        Some(ProviderId::new("fake-embedding"))
    );
    assert!(last_error.retryable);

    let events = JobStore::events(
        &jobs,
        JobEventListRequest {
            job_id: summary.job_id,
            after_sequence: None,
            phase: Some(PipelinePhase::Complete),
            severity: Some(Severity::Failed),
            visibility: Some(Visibility::Public),
            since_sequence: None,
            limit: Some(10),
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(events.events.len(), 1);
    assert_eq!(events.events[0].status, LifecycleStatus::Failed);
}

#[tokio::test]
async fn publish_generation_failure_leaves_vectors_uncommitted() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_feed_path(dir.path(), RSS_TWO_ITEMS).await;
    let ledger = FakeLedgerStore::new().with_publish_generation_failure();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let err = index_feed_source(input(path.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("failed to publish generation"),
        "unexpected error: {err:#}"
    );

    let source_id = super::feed_source_id(&tokio::fs::canonicalize(&path).await.unwrap());
    assert_eq!(ledger.committed_generation(&source_id).await, None);
    assert_eq!(
        vectors.calls().await,
        vec![
            "ensure_collection",
            "upsert",
            "mark_generation_committed",
            "delete"
        ]
    );
    assert!(vectors.points("axon-test").await.is_empty());
}

#[tokio::test]
async fn lost_lease_before_publish_leaves_generation_uncommitted() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_feed_path(dir.path(), RSS_TWO_ITEMS).await;
    let ledger = FakeLedgerStore::new().with_heartbeat_lost();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let err = index_feed_source(input(path.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("lost lease before publish"),
        "unexpected error: {err:#}"
    );

    let source_id = super::feed_source_id(&tokio::fs::canonicalize(&path).await.unwrap());
    assert_eq!(ledger.committed_generation(&source_id).await, None);
}

#[tokio::test]
async fn publish_generation_failure_reports_rollback_delete_failure() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_feed_path(dir.path(), RSS_TWO_ITEMS).await;
    let ledger = FakeLedgerStore::new().with_publish_generation_failure();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector").with_mode(FakeVectorMode::DeleteFailure);

    let err = index_feed_source(input(path.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap_err();

    assert!(
        err.to_string()
            .contains("also failed to rollback committed vector generation"),
        "unexpected error: {err:#}"
    );
    assert_eq!(
        vectors.calls().await,
        vec![
            "ensure_collection",
            "upsert",
            "mark_generation_committed",
            "delete"
        ]
    );
    let source_id = super::feed_source_id(&tokio::fs::canonicalize(&path).await.unwrap());
    assert_eq!(ledger.committed_generation(&source_id).await, None);
    assert_eq!(ledger.generation_count().await, 1);
    assert!(
        vectors
            .points("axon-test")
            .await
            .iter()
            .all(|point| point.payload["committed_generation"].as_str() != Some("uncommitted"))
    );
}

#[tokio::test]
async fn vector_commit_marker_failure_leaves_vectors_uncommitted() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_feed_path(dir.path(), RSS_TWO_ITEMS).await;
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector").with_mode(FakeVectorMode::CommitFailure);

    let err = index_feed_source(input(path.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("mark generation committed"),
        "unexpected error: {err:#}"
    );

    let source_id = super::feed_source_id(&tokio::fs::canonicalize(&path).await.unwrap());
    assert_eq!(ledger.committed_generation(&source_id).await, None);
    assert_eq!(ledger.generation_count().await, 1);
    assert!(
        vectors
            .points("axon-test")
            .await
            .iter()
            .all(|point| point.payload["committed_generation"].as_str() == Some("uncommitted"))
    );
}
