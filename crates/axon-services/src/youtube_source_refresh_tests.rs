use axon_api::source::*;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use axon_vectors::store::FakeVectorStore;

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

const DUMP_TWO_VIDEOS: &str = r#"{
  "videos": [
    {
      "video_id": "dQw4w9WgXcQ",
      "title": "Never Gonna Give You Up (updated)",
      "channel": "Rick Astley",
      "transcript": "Never gonna give you up, never gonna let you down, updated transcript text"
    },
    {
      "video_id": "secondvid01",
      "title": "Second Video",
      "channel": "Rick Astley",
      "transcript": "This is the second transcript"
    }
  ]
}"#;

fn job_id() -> JobId {
    JobId::new(uuid::Uuid::from_u128(0x2981))
}

fn dump_file(contents: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "axon-youtube-src-refresh-test-{}",
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
async fn unchanged_refresh_reuses_committed_generation_without_vector_work() {
    let dump = dump_file(DUMP_ONE_VIDEO);
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let first = index_youtube_source(input(dump.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap();
    let embedding_calls = embedder.calls().await.len();
    let vector_calls = vectors.calls().await;

    let second = index_youtube_source(input(dump.clone()), &ledger, &embedder, &vectors)
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

    std::fs::remove_dir_all(dump.parent().unwrap()).ok();
}

#[tokio::test]
async fn refresh_vectorizes_newly_added_video() {
    // The youtube adapter's discover pass does not stamp `content_hash` /
    // `version` / `mtime` on manifest items (see `youtube.rs::discover_sync`),
    // so the shared ledger's manifest diff cannot detect an in-place
    // transcript edit for an unchanged `video_id` — only added/removed video
    // IDs are detected as changes. This mirrors the adapter's own tests
    // (`youtube_tests.rs`), which only assert add/remove behavior.
    let dump = dump_file(DUMP_ONE_VIDEO);
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let first = index_youtube_source(input(dump.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap();
    assert_eq!(first.documents_prepared, 1);

    std::fs::write(&dump, DUMP_TWO_VIDEOS).unwrap();

    let second = index_youtube_source(input(dump.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap();

    assert_ne!(second.generation, first.generation);
    // Only the newly added `secondvid01` video is vectorized this
    // generation; the existing `dQw4w9WgXcQ` item is carried forward as
    // unchanged even though its transcript text differs in the dump.
    assert_eq!(second.documents_prepared, 1);
    assert_eq!(embedder.calls().await.len(), 2);
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
    assert_eq!(source.counts.items_changed, 1);
    assert_eq!(source.counts.documents_total, 2);

    std::fs::remove_dir_all(dump.parent().unwrap()).ok();
}
