use axon_api::source::*;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_embedding::reservation::{ProviderReservationConfig, ProviderReservationManager};
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use axon_vectors::store::FakeVectorStore;
use std::sync::Arc;

use super::{FeedSourceIndexInput, index_feed_source, index_feed_source_with_job};

const RSS_TWO_ITEMS: &str = r#"<?xml version="1.0"?>
<rss version="2.0"><channel>
  <title>Example Feed</title>
  <link>https://example.com/</link>
  <item>
    <title>First Post</title>
    <link>https://example.com/a</link>
    <description>Hello &lt;b&gt;world&lt;/b&gt;</description>
    <pubDate>Mon, 01 Jan 2024 00:00:00 GMT</pubDate>
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

fn input_with_reservations(feed_path: std::path::PathBuf) -> FeedSourceIndexInput {
    let mut input = input(feed_path);
    input.embedding_reservations = Some(Arc::new(ProviderReservationManager::new(
        ProviderReservationConfig {
            provider_id: input.embedding_provider_id.clone(),
            provider_kind: ProviderKind::Embedding,
            capacity: 2,
            interactive_reserve: 1,
            cooldown_after_failures: 1,
            cooldown_secs: 30,
        },
    )));
    input.vector_reservations = Some(Arc::new(ProviderReservationManager::new(
        ProviderReservationConfig {
            provider_id: input.vector_provider_id.clone(),
            provider_kind: ProviderKind::Vector,
            capacity: 2,
            interactive_reserve: 1,
            cooldown_after_failures: 1,
            cooldown_secs: 30,
        },
    )));
    input
}

#[tokio::test]
async fn feed_refresh_writes_vectors_then_commits_source_generation() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_feed_path(dir.path(), RSS_TWO_ITEMS).await;
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let output = index_feed_source(input(path), &ledger, &embedder, &vectors)
        .await
        .unwrap();

    assert_eq!(
        ledger.committed_generation(&output.source_id).await,
        Some(output.generation.clone())
    );
    assert_eq!(embedder.calls().await.len(), 1);
    assert_eq!(output.documents_prepared, 2);
    assert!(output.chunks_prepared >= 2);
    assert!(output.vector_points_written >= 2);
    assert_eq!(
        vectors.calls().await,
        vec!["ensure_collection", "upsert", "mark_generation_committed"]
    );
    assert!(
        vectors
            .points("axon-test")
            .await
            .iter()
            .all(|point| point.payload["committed_generation"].as_str()
                == Some(output.generation.0.as_str()))
    );
    assert!(
        vectors
            .points("axon-test")
            .await
            .iter()
            .all(|point| point.payload["document_status"].as_str() == Some("published"))
    );
    for point in vectors.points("axon-test").await {
        let status = ledger
            .document_status(&DocumentId::new(
                point.payload["document_id"].as_str().unwrap(),
            ))
            .await
            .expect("ledger document status");
        assert_eq!(status.status, DocumentLifecycleStatus::Published);
    }
    let source = ledger
        .get_source(output.source_id.clone())
        .await
        .unwrap()
        .expect("source summary");
    assert_eq!(source.status, LifecycleStatus::Completed);
    assert_eq!(source.counts.items_total, 2);
    assert_eq!(source.counts.documents_total, output.documents_prepared);
}

#[tokio::test]
async fn unchanged_feed_refresh_reuses_committed_generation_without_vector_work() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_feed_path(dir.path(), RSS_TWO_ITEMS).await;
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let first = index_feed_source(input(path.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap();
    let embedding_calls = embedder.calls().await.len();
    let vector_calls = vectors.calls().await;

    let second = index_feed_source(input(path), &ledger, &embedder, &vectors)
        .await
        .unwrap();

    let source = ledger
        .get_source(first.source_id.clone())
        .await
        .unwrap()
        .expect("source summary after no-op refresh");
    assert_eq!(source.status, LifecycleStatus::Completed);
    assert_eq!(source.counts.items_total, 2);
    assert_eq!(source.counts.items_changed, 0);
    assert_eq!(second.generation, first.generation);
    assert_eq!(
        ledger.committed_generation(&first.source_id).await,
        Some(first.generation)
    );
    assert_eq!(second.documents_prepared, 0);
    assert_eq!(second.chunks_prepared, 0);
    assert_eq!(second.vector_points_written, 0);
    assert_eq!(embedder.calls().await.len(), embedding_calls);
    assert_eq!(vectors.calls().await, vector_calls);
}

#[tokio::test]
async fn feed_source_job_emits_progress_events_for_pipeline_phases() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_feed_path(dir.path(), RSS_TWO_ITEMS).await;
    let jobs = FakeJobWatchStore::new();
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let output = index_feed_source_with_job(input(path), &jobs, &ledger, &embedder, &vectors)
        .await
        .unwrap();

    let summary = JobStore::get(&jobs, output.job_id)
        .await
        .unwrap()
        .expect("job summary");
    assert_eq!(summary.kind, JobKind::Source);
    assert_eq!(summary.status, LifecycleStatus::Completed);
    assert_eq!(summary.source_id, Some(output.source_id.clone()));

    let events = JobStore::events(
        &jobs,
        JobEventListRequest {
            job_id: output.job_id,
            after_sequence: None,
            phase: None,
            severity: None,
            visibility: Some(Visibility::Public),
            since_sequence: None,
            limit: Some(20),
            cursor: None,
        },
    )
    .await
    .unwrap();
    let phases = events
        .events
        .iter()
        .map(|event| event.phase)
        .collect::<Vec<_>>();

    assert_eq!(
        phases,
        vec![
            PipelinePhase::Discovering,
            PipelinePhase::Diffing,
            PipelinePhase::Preparing,
            PipelinePhase::Embedding,
            PipelinePhase::Vectorizing,
            PipelinePhase::Publishing,
            PipelinePhase::Cleaning,
            PipelinePhase::Complete,
        ]
    );
    assert!(
        events
            .events
            .iter()
            .all(|event| event.job_id == output.job_id)
    );
}

#[tokio::test]
async fn feed_source_job_records_provider_reservation_events() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_feed_path(dir.path(), RSS_TWO_ITEMS).await;
    let jobs = FakeJobWatchStore::new();
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let output = index_feed_source_with_job(
        input_with_reservations(path),
        &jobs,
        &ledger,
        &embedder,
        &vectors,
    )
    .await
    .unwrap();

    let events = JobStore::events(
        &jobs,
        JobEventListRequest {
            job_id: output.job_id,
            after_sequence: None,
            phase: None,
            severity: None,
            visibility: Some(Visibility::Public),
            since_sequence: None,
            limit: Some(20),
            cursor: None,
        },
    )
    .await
    .unwrap();
    let embedding_event = events
        .events
        .iter()
        .find(|event| event.phase == PipelinePhase::Embedding)
        .expect("embedding event");
    assert!(
        progress_reservation_id(embedding_event).is_some(),
        "embedding phase should expose reservation evidence"
    );
    let vectorizing_event = events
        .events
        .iter()
        .find(|event| event.phase == PipelinePhase::Vectorizing)
        .expect("vectorizing event");
    assert!(
        progress_reservation_id(vectorizing_event).is_some(),
        "vectorizing phase should expose reservation evidence"
    );
}

fn progress_reservation_id(event: &JobEvent) -> Option<&str> {
    event
        .details
        .get("source_progress_event")?
        .get("reservation_id")?
        .as_str()
}
