use axon_api::source::*;
use axon_embedding::fake::{FakeEmbeddingMode, FakeEmbeddingProvider};
use axon_jobs::boundary::FakeJobWatchStore;
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::{FakeVectorMode, FakeVectorStore};

use super::reddit_source_adapter::reddit_source_id;
use super::{RedditSourceIndexInput, index_reddit_source, index_reddit_source_with_job};

fn job_id() -> JobId {
    JobId::new(uuid::Uuid::from_u128(0x2222))
}

fn sample_dump() -> serde_json::Value {
    serde_json::json!([
        {
            "title": "Rust chunking",
            "selftext": "Post body",
            "permalink": "/r/rust/comments/abc123/rust_chunking/",
            "author": "alice",
            "score": 42,
            "subreddit": "rust",
            "domain": "self.rust",
            "num_comments": 2,
            "upvote_ratio": 0.97,
            "is_video": false,
            "distinguished": null,
            "gilded": 0,
            "link_flair_text": "Discussion",
            "created_utc": 1_767_225_600u64,
            "comments": []
        }
    ])
}

fn write_dump(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("dump.json");
    std::fs::write(&path, serde_json::to_vec(&sample_dump()).unwrap()).unwrap();
    path
}

fn input(dump_path: std::path::PathBuf) -> RedditSourceIndexInput {
    RedditSourceIndexInput {
        target: "r/rust".to_string(),
        dump_path,
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
    let dir = tempfile::tempdir().unwrap();
    let dump_path = write_dump(dir.path());
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let failing_vectors =
        FakeVectorStore::new("fake-vector").with_mode(FakeVectorMode::Unavailable);

    let err = index_reddit_source(
        input(dump_path.clone()),
        &ledger,
        &embedder,
        &failing_vectors,
    )
    .await
    .unwrap_err();
    assert!(
        err.to_string().contains("vector store unavailable"),
        "unexpected error: {err:#}"
    );

    let source_id = reddit_source_id("r/rust").unwrap();
    assert_eq!(ledger.committed_generation(&source_id).await, None);
    assert_eq!(ledger.generation_count().await, 0);
    assert_eq!(ledger.manifest_count().await, 0);

    let healthy_vectors = FakeVectorStore::new("fake-vector");
    let output = index_reddit_source(input(dump_path), &ledger, &embedder, &healthy_vectors)
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
    let dump_path = write_dump(dir.path());
    let ledger = FakeLedgerStore::new();
    let embedder =
        FakeEmbeddingProvider::new("fake-embedding", 8).with_mode(FakeEmbeddingMode::Fatal);
    let vectors = FakeVectorStore::new("fake-vector");

    let err = index_reddit_source(input(dump_path), &ledger, &embedder, &vectors)
        .await
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("embedding provider failed fatally"),
        "unexpected error: {err:#}"
    );
    let source_id = reddit_source_id("r/rust").unwrap();
    assert_eq!(ledger.committed_generation(&source_id).await, None);
    assert_eq!(ledger.generation_count().await, 0);
    assert_eq!(ledger.manifest_count().await, 0);
}

#[tokio::test]
async fn partial_vector_write_failure_rolls_back_generation_points() {
    let dir = tempfile::tempdir().unwrap();
    let dump_path = write_dump(dir.path());
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector").with_mode(FakeVectorMode::PartialFailure);

    let err = index_reddit_source(input(dump_path), &ledger, &embedder, &vectors)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("wrote"),
        "unexpected error: {err:#}"
    );

    let source_id = reddit_source_id("r/rust").unwrap();
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
    let dump_path = write_dump(dir.path());
    let jobs = FakeJobWatchStore::new();
    let ledger = FakeLedgerStore::new();
    let embedder =
        FakeEmbeddingProvider::new("fake-embedding", 8).with_mode(FakeEmbeddingMode::RateLimited);
    let vectors = FakeVectorStore::new("fake-vector");

    let err = index_reddit_source_with_job(input(dump_path), &jobs, &ledger, &embedder, &vectors)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("rate"),
        "unexpected error: {err:#}"
    );
}

#[tokio::test]
async fn invalid_target_fails_before_touching_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let dump_path = write_dump(dir.path());
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");
    let mut request = input(dump_path);
    request.target = "not a valid subreddit name!!".to_string();

    let err = index_reddit_source(request, &ledger, &embedder, &vectors)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("invalid reddit target"),
        "unexpected error: {err:#}"
    );
    assert_eq!(ledger.generation_count().await, 0);
}

#[tokio::test]
async fn missing_dump_file_fails_during_discovery() {
    let dir = tempfile::tempdir().unwrap();
    let missing_path = dir.path().join("does-not-exist.json");
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let err = index_reddit_source(input(missing_path), &ledger, &embedder, &vectors)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("dump_read_failed") || err.to_string().contains("reddit"),
        "unexpected error: {err:#}"
    );
    let source_id = reddit_source_id("r/rust").unwrap();
    assert_eq!(ledger.generation_count().await, 0);
    assert_eq!(ledger.committed_generation(&source_id).await, None);
}
