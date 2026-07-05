use axon_api::source::*;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use axon_vectors::store::FakeVectorStore;

use super::{FeedSourceIndexInput, index_feed_source};

const RSS_INITIAL: &str = r#"<?xml version="1.0"?>
<rss version="2.0"><channel>
  <title>Example Feed</title>
  <link>https://example.com/</link>
  <item>
    <title>Old Post</title>
    <link>https://example.com/old</link>
    <description>Old body</description>
  </item>
  <item>
    <title>Stable Post</title>
    <link>https://example.com/stable</link>
    <description>Stable body</description>
  </item>
</channel></rss>"#;

const RSS_UPDATED: &str = r#"<?xml version="1.0"?>
<rss version="2.0"><channel>
  <title>Example Feed</title>
  <link>https://example.com/</link>
  <item>
    <title>Stable Post</title>
    <link>https://example.com/stable</link>
    <description>Stable body</description>
  </item>
  <item>
    <title>New Post</title>
    <link>https://example.com/new</link>
    <description>New body</description>
  </item>
</channel></rss>"#;

fn job_id() -> JobId {
    JobId::new(uuid::Uuid::from_u128(0x2222))
}

fn input(feed_path: std::path::PathBuf) -> FeedSourceIndexInput {
    FeedSourceIndexInput {
        feed_path,
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
    }
}

#[tokio::test]
async fn refresh_vectorizes_new_entries_and_debts_removed_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("feed.xml");
    tokio::fs::write(&path, RSS_INITIAL).await.unwrap();
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let first = index_feed_source(input(path.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap();
    assert_eq!(first.documents_prepared, 2);

    tokio::fs::write(&path, RSS_UPDATED).await.unwrap();

    let second = index_feed_source(input(path.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap();

    assert_ne!(second.generation, first.generation);
    assert_eq!(second.documents_prepared, 1);
    assert_eq!(second.removed_entries, 1);
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
    let stable_points = vectors
        .points("axon-test")
        .await
        .into_iter()
        .filter(|point| {
            point
                .payload
                .get("item_canonical_uri")
                .and_then(|value| value.as_str())
                == Some("https://example.com/stable")
        })
        .collect::<Vec<_>>();
    assert!(!stable_points.is_empty());
    assert!(
        stable_points
            .iter()
            .all(|point| { point.payload["document_status"].as_str() == Some("published") })
    );
    assert_eq!(
        ledger.committed_generation(&second.source_id).await,
        Some(second.generation.clone())
    );
    let source = ledger
        .get_source(second.source_id.clone())
        .await
        .unwrap()
        .expect("source summary after incremental refresh");
    assert_eq!(source.counts.items_total, 2);
    assert_eq!(source.counts.documents_total, 2);
}
