use axon_api::source::*;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_embedding::reservation::{ProviderReservationConfig, ProviderReservationManager};
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use axon_vectors::store::FakeVectorStore;
use std::sync::Arc;

use crate::test_support::committed_generation_payload;

use super::{YoutubeSourceIndexInput, index_youtube_source, index_youtube_source_with_job};

const TARGET_URL: &str = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";

const DUMP_WITH_ONE_VIDEO: &str = r#"{
  "videos": [
    {
      "video_id": "dQw4w9WgXcQ",
      "title": "Never Gonna Give You Up",
      "channel": "Rick Astley",
      "channel_url": "https://www.youtube.com/@RickAstleyYT",
      "uploader_id": "RickAstleyYT",
      "upload_date": "20091025",
      "description": "The official video.",
      "duration_string": "3:33",
      "view_count": 1000000,
      "like_count": 10000,
      "tags": ["music"],
      "categories": ["Music"],
      "thumbnail": "https://i.ytimg.com/vi/dQw4w9WgXcQ/default.jpg",
      "transcript": "Never gonna give you up, never gonna let you down"
    }
  ]
}"#;

const DUMP_WITH_TWO_VIDEOS: &str = r#"{
  "videos": [
    {
      "video_id": "dQw4w9WgXcQ",
      "title": "Never Gonna Give You Up",
      "channel": "Rick Astley",
      "channel_url": "https://www.youtube.com/@RickAstleyYT",
      "uploader_id": "RickAstleyYT",
      "upload_date": "20091025",
      "description": "The official video.",
      "duration_string": "3:33",
      "view_count": 1000000,
      "like_count": 10000,
      "tags": ["music"],
      "categories": ["Music"],
      "thumbnail": "https://i.ytimg.com/vi/dQw4w9WgXcQ/default.jpg",
      "transcript": "Never gonna give you up, never gonna let you down"
    },
    {
      "video_id": "abc123",
      "title": "Second Video",
      "channel": "Rick Astley",
      "channel_url": "https://www.youtube.com/@RickAstleyYT",
      "uploader_id": "RickAstleyYT",
      "upload_date": "20091026",
      "description": "Another video.",
      "duration_string": "2:00",
      "view_count": 500,
      "like_count": 10,
      "tags": ["music"],
      "categories": ["Music"],
      "thumbnail": "https://i.ytimg.com/vi/abc123/default.jpg",
      "transcript": "A second transcript body."
    }
  ]
}"#;

fn job_id() -> JobId {
    JobId::new(uuid::Uuid::from_u128(0x2981))
}

fn dump_file(contents: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("axon-youtube-src-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("dump.json");
    std::fs::write(&path, contents).unwrap();
    path
}

fn input(dump_path: std::path::PathBuf) -> YoutubeSourceIndexInput {
    YoutubeSourceIndexInput {
        target: TARGET_URL.to_string(),
        youtube_dump_path: dump_path,
        collection: "axon-test".to_string(),
        owner_id: "test-owner".to_string(),
        job_id: job_id(),
        auth_snapshot: None,
        embedding_provider_id: ProviderId::new("fake-embedding"),
        vector_provider_id: ProviderId::new("fake-vector"),
        embedding_model: "fake-embedding".to_string(),
        embedding_dimensions: 8,
        embedding_reservations: None,
        vector_reservations: None,
        embed: true,
        max_items: None,
    }
}

fn input_with_reservations(dump_path: std::path::PathBuf) -> YoutubeSourceIndexInput {
    let mut input = input(dump_path);
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
async fn youtube_source_refresh_writes_vectors_then_commits_source_generation() {
    let dump = dump_file(DUMP_WITH_ONE_VIDEO);
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let output = index_youtube_source(input(dump.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap();

    assert_eq!(
        ledger.committed_generation(&output.source_id).await,
        Some(output.generation.clone())
    );
    assert_eq!(embedder.calls().await.len(), 1);
    assert!(output.documents_prepared >= 1);
    assert!(output.chunks_prepared >= 1);
    assert!(output.vector_points_written >= 1);
    assert_eq!(
        vectors.calls().await,
        vec!["ensure_collection", "upsert", "mark_generation_committed"]
    );
    assert!(
        vectors
            .points("axon-test")
            .await
            .iter()
            .all(|point| point.payload["committed_generation"]
                == committed_generation_payload(&output.generation))
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
    assert_eq!(source.counts.items_total, 1);
    assert_eq!(source.counts.documents_total, output.documents_prepared);
    assert_eq!(source.source_kind, SourceKind::Youtube);

    std::fs::remove_dir_all(dump.parent().unwrap()).ok();
}

#[tokio::test]
async fn youtube_source_job_emits_progress_events_for_pipeline_phases() {
    let dump = dump_file(DUMP_WITH_ONE_VIDEO);
    let jobs = FakeJobWatchStore::new();
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let output =
        index_youtube_source_with_job(input(dump.clone()), &jobs, &ledger, &embedder, &vectors)
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

    std::fs::remove_dir_all(dump.parent().unwrap()).ok();
}

#[tokio::test]
async fn youtube_source_job_records_provider_reservation_events() {
    let dump = dump_file(DUMP_WITH_ONE_VIDEO);
    let jobs = FakeJobWatchStore::new();
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let output = index_youtube_source_with_job(
        input_with_reservations(dump.clone()),
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

    std::fs::remove_dir_all(dump.parent().unwrap()).ok();
}

fn progress_reservation_id(event: &JobEvent) -> Option<&str> {
    event
        .details
        .get("source_progress_event")?
        .get("reservation_id")?
        .as_str()
}

/// `embed = false` (source-pipeline.md Validation Checklist: "`embed=false`
/// never writes vectors"): videos are still discovered/prepared
/// (documents_prepared stays non-zero) but neither the embedding provider
/// nor `vector_store.upsert` may be called.
#[tokio::test]
async fn embed_false_prepares_videos_but_writes_no_vectors() {
    let dump = dump_file(DUMP_WITH_TWO_VIDEOS);
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let mut no_embed_input = input(dump.clone());
    no_embed_input.embed = false;

    let output = index_youtube_source(no_embed_input, &ledger, &embedder, &vectors)
        .await
        .unwrap();

    assert_eq!(
        ledger.committed_generation(&output.source_id).await,
        Some(output.generation.clone())
    );
    assert_eq!(
        embedder.calls().await.len(),
        0,
        "embed=false must not call the embedding provider"
    );
    assert!(
        !vectors.calls().await.contains(&"upsert"),
        "embed=false must not call vector_store.upsert"
    );
    assert_eq!(output.vector_points_written, 0);
    assert_eq!(output.documents_prepared, 2);
    assert!(vectors.points("axon-test").await.is_empty());

    std::fs::remove_dir_all(dump.parent().unwrap()).ok();
}

/// `SourceRequest.limits.max_items` caps the number of YouTube videos
/// considered before diffing, so only the first `max_items` videos are
/// prepared/vectorized even though the dump has more.
#[tokio::test]
async fn max_items_limit_caps_videos_prepared() {
    let dump = dump_file(DUMP_WITH_TWO_VIDEOS);
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let mut capped_input = input(dump.clone());
    capped_input.max_items = Some(1);

    let output = index_youtube_source(capped_input, &ledger, &embedder, &vectors)
        .await
        .unwrap();

    assert_eq!(output.documents_prepared, 1);

    std::fs::remove_dir_all(dump.parent().unwrap()).ok();
}
