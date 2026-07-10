use axon_api::source::*;
use axon_embedding::fake::{FakeEmbeddingMode, FakeEmbeddingProvider};
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::{FakeVectorMode, FakeVectorStore};

use super::{YoutubeSourceIndexInput, index_youtube_source};

const TARGET_URL: &str = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";

const DUMP_ONE_VIDEO: &str = r#"{
  "videos": [
    {
      "video_id": "dQw4w9WgXcQ",
      "title": "Never Gonna Give You Up",
      "channel": "Rick Astley",
      "transcript": "Never gonna give you up, never gonna let you down"
    }
  ]
}"#;

fn job_id() -> JobId {
    JobId::new(uuid::Uuid::from_u128(0x2981))
}

fn dump_file(contents: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "axon-youtube-src-failure-test-{}",
        uuid::Uuid::new_v4()
    ));
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

#[tokio::test]
async fn vector_failure_leaves_generation_uncommitted_and_releases_lease() {
    let dump = dump_file(DUMP_ONE_VIDEO);
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let failing_vectors =
        FakeVectorStore::new("fake-vector").with_mode(FakeVectorMode::Unavailable);

    let err = index_youtube_source(input(dump.clone()), &ledger, &embedder, &failing_vectors)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("vector store unavailable"),
        "unexpected error: {err:#}"
    );

    let source_id = super::youtube_source_id(TARGET_URL);
    assert_eq!(ledger.committed_generation(&source_id).await, None);
    assert_eq!(ledger.generation_count().await, 0);
    assert_eq!(ledger.manifest_count().await, 0);

    let healthy_vectors = FakeVectorStore::new("fake-vector");
    let output = index_youtube_source(input(dump.clone()), &ledger, &embedder, &healthy_vectors)
        .await
        .unwrap();
    assert_eq!(
        ledger.committed_generation(&output.source_id).await,
        Some(output.generation)
    );

    std::fs::remove_dir_all(dump.parent().unwrap()).ok();
}

#[tokio::test]
async fn embedding_failure_keeps_generation_uncommitted() {
    let dump = dump_file(DUMP_ONE_VIDEO);
    let ledger = FakeLedgerStore::new();
    let embedder =
        FakeEmbeddingProvider::new("fake-embedding", 8).with_mode(FakeEmbeddingMode::Fatal);
    let vectors = FakeVectorStore::new("fake-vector");

    let err = index_youtube_source(input(dump.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("embedding provider failed fatally"),
        "unexpected error: {err:#}"
    );
    let source_id = super::youtube_source_id(TARGET_URL);
    assert_eq!(ledger.committed_generation(&source_id).await, None);
    assert_eq!(ledger.generation_count().await, 0);
    assert_eq!(ledger.manifest_count().await, 0);

    std::fs::remove_dir_all(dump.parent().unwrap()).ok();
}

#[tokio::test]
async fn partial_vector_write_failure_rolls_back_generation_points() {
    let dump = dump_file(DUMP_ONE_VIDEO);
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector").with_mode(FakeVectorMode::PartialFailure);

    let err = index_youtube_source(input(dump.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("wrote"),
        "unexpected error: {err:#}"
    );

    let source_id = super::youtube_source_id(TARGET_URL);
    assert_eq!(ledger.committed_generation(&source_id).await, None);
    assert_eq!(
        vectors.calls().await,
        vec!["ensure_collection", "upsert", "delete"]
    );
    assert!(vectors.points("axon-test").await.is_empty());

    std::fs::remove_dir_all(dump.parent().unwrap()).ok();
}

#[tokio::test]
async fn publish_generation_failure_leaves_vectors_uncommitted() {
    let dump = dump_file(DUMP_ONE_VIDEO);
    let ledger = FakeLedgerStore::new().with_publish_generation_failure();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let err = index_youtube_source(input(dump.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("failed to publish generation"),
        "unexpected error: {err:#}"
    );

    let source_id = super::youtube_source_id(TARGET_URL);
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

    std::fs::remove_dir_all(dump.parent().unwrap()).ok();
}

#[tokio::test]
async fn lost_lease_before_publish_leaves_generation_uncommitted() {
    let dump = dump_file(DUMP_ONE_VIDEO);
    let ledger = FakeLedgerStore::new().with_heartbeat_lost();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let err = index_youtube_source(input(dump.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("lost lease before publish"),
        "unexpected error: {err:#}"
    );

    let source_id = super::youtube_source_id(TARGET_URL);
    assert_eq!(ledger.committed_generation(&source_id).await, None);

    std::fs::remove_dir_all(dump.parent().unwrap()).ok();
}
