use axon_api::source::*;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use axon_vectors::store::FakeVectorStore;

use crate::test_support::source_generation_payload;

use super::{RedditSourceIndexInput, index_reddit_source};

fn job_id() -> JobId {
    JobId::new(uuid::Uuid::from_u128(0x2222))
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

fn post(permalink: &str, title: &str) -> serde_json::Value {
    serde_json::json!({
        "title": title,
        "selftext": "body",
        "permalink": permalink,
        "author": "alice",
        "score": 1,
        "subreddit": "rust",
        "domain": "self.rust",
        "num_comments": 0,
        "upvote_ratio": 1.0,
        "is_video": false,
        "distinguished": null,
        "gilded": 0,
        "link_flair_text": null,
        "created_utc": 1_767_225_600u64,
        "comments": []
    })
}

fn write_dump(path: &std::path::Path, posts: Vec<serde_json::Value>) {
    std::fs::write(
        path,
        serde_json::to_vec(&serde_json::Value::Array(posts)).unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn unchanged_refresh_reuses_committed_generation_without_vector_work() {
    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("dump.json");
    write_dump(
        &dump_path,
        vec![post("/r/rust/comments/abc123/keep/", "Keep")],
    );
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let first = index_reddit_source(input(dump_path.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap();
    let embedding_calls = embedder.calls().await.len();
    let vector_calls = vectors.calls().await;

    let second = index_reddit_source(input(dump_path), &ledger, &embedder, &vectors)
        .await
        .unwrap();

    let source = ledger
        .get_source(first.source_id.clone())
        .await
        .unwrap()
        .expect("source summary after no-op refresh");
    assert_eq!(source.status, LifecycleStatus::Completed);
    assert_eq!(source.counts.items_total, 1);
    assert_eq!(source.counts.items_changed, 0);
    assert_eq!(source.counts.documents_total, first.documents_prepared);
    assert_eq!(source.counts.chunks_total, first.chunks_prepared);
    assert_eq!(
        source.counts.vector_points_total,
        first.vector_points_written
    );
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
async fn refresh_vectorizes_added_and_modified_posts_and_debts_removed_items() {
    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("dump.json");
    write_dump(
        &dump_path,
        vec![
            post("/r/rust/comments/old111/old/", "Old"),
            post("/r/rust/comments/keep222/keep/", "Keep"),
            post("/r/rust/comments/stable333/stable/", "Stable"),
        ],
    );
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let first = index_reddit_source(input(dump_path.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap();
    assert_eq!(first.documents_prepared, 3);

    // Remove "old", modify "keep" (new title => different content hash), add "new".
    write_dump(
        &dump_path,
        vec![
            post("/r/rust/comments/keep222/keep/", "Keep updated"),
            post("/r/rust/comments/stable333/stable/", "Stable"),
            post("/r/rust/comments/new444/new/", "New"),
        ],
    );

    let second = index_reddit_source(input(dump_path.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap();

    assert_ne!(second.generation, first.generation);
    assert_eq!(second.documents_prepared, 2);
    assert_eq!(second.removed_items, 1);
    assert!(
        second.vector_points_written >= second.chunks_prepared,
        "carried-forward unchanged vectors should be counted with new vectors"
    );
    assert_eq!(embedder.calls().await.len(), 2);
    assert_eq!(
        vectors
            .calls()
            .await
            .into_iter()
            .filter(|call| *call == "upsert")
            .count(),
        2
    );
    assert_eq!(
        vectors
            .calls()
            .await
            .into_iter()
            .filter(|call| *call == "delete")
            .count(),
        0
    );
    assert!(
        vectors
            .points("axon-test")
            .await
            .iter()
            .any(|point| point.payload["source_generation"]
                == source_generation_payload(&first.generation))
    );
    let stable_points = vectors
        .points("axon-test")
        .await
        .into_iter()
        .filter(|point| {
            point
                .payload
                .get("source_item_key")
                .and_then(|value| value.as_str())
                == Some("r/rust/comments/stable333/stable")
        })
        .collect::<Vec<_>>();
    assert!(!stable_points.is_empty());
    assert!(
        stable_points
            .iter()
            .all(|point| { point.payload["document_status"].as_str() == Some("published") })
    );
    assert_eq!(
        vectors
            .calls()
            .await
            .into_iter()
            .filter(|call| *call == "mark_unchanged_items_committed")
            .count(),
        1
    );
    // 2 VectorDelete debts (old removed, keep modified) plus 1 auto-emitted
    // GraphPrune debt for the genuinely-removed "old" post (a modified item
    // like "keep" keeps its stable key, so it never gets a GraphPrune debt —
    // see `record_graph_prune_cleanup_debt`).
    assert_eq!(ledger.cleanup_debt_count().await, 3);
    assert_eq!(
        ledger.committed_generation(&second.source_id).await,
        Some(second.generation.clone())
    );
    let source = ledger
        .get_source(second.source_id.clone())
        .await
        .unwrap()
        .expect("source summary after incremental refresh");
    assert_eq!(source.counts.items_total, 3);
    assert_eq!(source.counts.items_changed, 3);
    assert_eq!(source.counts.documents_total, 3);
    assert_eq!(
        source.counts.vector_points_total,
        second.vector_points_written
    );

    let third = index_reddit_source(input(dump_path), &ledger, &embedder, &vectors)
        .await
        .unwrap();
    assert_eq!(third.generation, second.generation);
    assert_eq!(third.documents_prepared, 0);
}
