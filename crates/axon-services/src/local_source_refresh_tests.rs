use axon_api::source::*;
use axon_embedding::fake::{FakeEmbeddingMode, FakeEmbeddingProvider};
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use axon_vectors::payload::generation_payload_i64;
use axon_vectors::store::FakeVectorStore;

use super::{
    LocalSourceIndexInput, LocalSourceSelectionPolicy, index_local_source, local_source_id,
};

fn job_id() -> JobId {
    JobId::new(uuid::Uuid::from_u128(0x1111))
}

fn input(root: std::path::PathBuf) -> LocalSourceIndexInput {
    LocalSourceIndexInput {
        root,
        collection: "axon-test".to_string(),
        owner_id: "test-owner".to_string(),
        job_id: job_id(),
        auth_snapshot: None,
        embedding_provider_id: ProviderId::new("fake-embedding"),
        vector_provider_id: ProviderId::new("fake-vector"),
        embedding_model: "fake-embedding".to_string(),
        embedding_dimensions: 8,
        selection_policy: LocalSourceSelectionPolicy::Permissive,
        embedding_reservations: None,
        vector_reservations: None,
    }
}

#[tokio::test]
async fn unchanged_refresh_reuses_committed_generation_without_vector_work() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lib.rs");
    tokio::fs::write(&path, "pub fn answer() -> i32 {\n    42\n}\n")
        .await
        .unwrap();
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let first = index_local_source(input(path.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap();
    let embedding_calls = embedder.calls().await.len();
    let vector_calls = vectors.calls().await;

    let second = index_local_source(input(path), &ledger, &embedder, &vectors)
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
async fn refresh_vectorizes_added_and_modified_docs_and_debts_removed_and_replaced_items() {
    let dir = tempfile::tempdir().unwrap();
    let old_path = dir.path().join("old.rs");
    let keep_path = dir.path().join("keep.rs");
    let stable_path = dir.path().join("stable.rs");
    let new_path = dir.path().join("new.rs");
    tokio::fs::write(&old_path, "pub fn old() -> i32 { 1 }\n")
        .await
        .unwrap();
    tokio::fs::write(&keep_path, "pub fn keep() -> i32 { 1 }\n")
        .await
        .unwrap();
    tokio::fs::write(&stable_path, "pub fn stable() -> i32 { 1 }\n")
        .await
        .unwrap();
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let first = index_local_source(
        input(dir.path().to_path_buf()),
        &ledger,
        &embedder,
        &vectors,
    )
    .await
    .unwrap();
    assert_eq!(first.documents_prepared, 3);

    tokio::fs::remove_file(old_path).await.unwrap();
    tokio::fs::write(&keep_path, "pub fn keep() -> i32 { 2 }\n")
        .await
        .unwrap();
    tokio::fs::write(&new_path, "pub fn new() -> i32 { 3 }\n")
        .await
        .unwrap();

    let second = index_local_source(
        input(dir.path().to_path_buf()),
        &ledger,
        &embedder,
        &vectors,
    )
    .await
    .unwrap();

    assert_ne!(second.generation, first.generation);
    assert_eq!(second.documents_prepared, 2);
    assert_eq!(second.removed_files, 1);
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
    assert!(vectors.points("axon-test").await.iter().any(
        |point| point.payload["source_generation"]
            == serde_json::json!(generation_payload_i64(
                    &first.generation,
                    "source_generation"
                )
                .expect("numeric generation"))
    ));
    let stable_points = vectors
        .points("axon-test")
        .await
        .into_iter()
        .filter(|point| {
            point
                .payload
                .get("source_item_key")
                .and_then(|value| value.as_str())
                == Some("stable.rs")
        })
        .collect::<Vec<_>>();
    assert!(!stable_points.is_empty());
    assert!(
        stable_points
            .iter()
            .all(|point| { point.payload["document_status"].as_str() == Some("published") })
    );
    assert!(stable_points.iter().any(|point| {
        point.payload["source_generation"]
            == serde_json::json!(
                generation_payload_i64(&first.generation, "source_generation")
                    .expect("numeric generation")
            )
            && point.payload["committed_generation"]
                == serde_json::json!(
                    generation_payload_i64(&first.generation, "committed_generation")
                        .expect("numeric generation")
                )
    }));
    assert!(stable_points.iter().any(|point| {
        point.payload["source_generation"]
            == serde_json::json!(
                generation_payload_i64(&second.generation, "source_generation")
                    .expect("numeric generation")
            )
            && point.payload["committed_generation"]
                == serde_json::json!(
                    generation_payload_i64(&second.generation, "committed_generation")
                        .expect("numeric generation")
                )
    }));
    assert_eq!(
        vectors
            .calls()
            .await
            .into_iter()
            .filter(|call| *call == "mark_unchanged_items_committed")
            .count(),
        1
    );
    assert_eq!(ledger.cleanup_debt_count().await, 2);
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

    let third = index_local_source(
        input(dir.path().to_path_buf()),
        &ledger,
        &embedder,
        &vectors,
    )
    .await
    .unwrap();
    assert_eq!(third.generation, second.generation);
    assert_eq!(third.documents_prepared, 0);
    let stable_points = vectors
        .points("axon-test")
        .await
        .into_iter()
        .filter(|point| {
            point
                .payload
                .get("source_item_key")
                .and_then(|value| value.as_str())
                == Some("stable.rs")
        })
        .collect::<Vec<_>>();
    assert!(!stable_points.is_empty());
    assert!(
        stable_points
            .iter()
            .all(|point| { point.payload["document_status"].as_str() == Some("published") })
    );
    assert!(stable_points.iter().any(|point| {
        point.payload["committed_generation"]
            == serde_json::json!(
                generation_payload_i64(&first.generation, "committed_generation")
                    .expect("numeric generation")
            )
    }));
    assert!(stable_points.iter().any(|point| {
        point.payload["committed_generation"]
            == serde_json::json!(
                generation_payload_i64(&second.generation, "committed_generation")
                    .expect("numeric generation")
            )
    }));
}

#[tokio::test]
async fn code_search_selection_skips_lockfiles_and_pruned_dirs() {
    let dir = tempfile::tempdir().unwrap();
    tokio::fs::create_dir_all(dir.path().join("src"))
        .await
        .unwrap();
    tokio::fs::create_dir_all(dir.path().join("target/debug"))
        .await
        .unwrap();
    tokio::fs::create_dir_all(dir.path().join(".cache"))
        .await
        .unwrap();
    tokio::fs::write(dir.path().join("src/lib.rs"), "pub fn x() {}\n")
        .await
        .unwrap();
    tokio::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\n",
    )
    .await
    .unwrap();
    tokio::fs::write(dir.path().join("README.md"), "# demo\n")
        .await
        .unwrap();
    tokio::fs::write(
        dir.path().join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\n",
    )
    .await
    .unwrap();
    tokio::fs::write(
        dir.path().join("target/debug/generated.rs"),
        "pub fn generated() {}\n",
    )
    .await
    .unwrap();
    tokio::fs::write(dir.path().join(".cache/script.sh"), "echo cache\n")
        .await
        .unwrap();
    tokio::fs::write(dir.path().join(".env"), "AXON_TOKEN=secret\n")
        .await
        .unwrap();
    tokio::fs::write(dir.path().join("api-token.txt"), "secret\n")
        .await
        .unwrap();
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");
    let mut request = input(dir.path().to_path_buf());
    request.selection_policy = LocalSourceSelectionPolicy::CodeSearch;

    let output = index_local_source(request, &ledger, &embedder, &vectors)
        .await
        .unwrap();

    assert_eq!(output.documents_prepared, 3);
    assert_eq!(embedder.calls().await.len(), 1);
    assert_eq!(
        ledger.committed_generation(&output.source_id).await,
        Some(output.generation)
    );
}

#[tokio::test]
async fn provider_unavailable_does_not_churn_first_generation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lib.rs");
    tokio::fs::write(&path, "pub fn answer() -> i32 { 42 }\n")
        .await
        .unwrap();
    let ledger = FakeLedgerStore::new();
    let embedder =
        FakeEmbeddingProvider::new("fake-embedding", 8).with_mode(FakeEmbeddingMode::Fatal);
    let vectors = FakeVectorStore::new("fake-vector");
    let source_id = local_source_id(&path);

    let result = index_local_source(input(path), &ledger, &embedder, &vectors).await;

    assert!(result.is_err());
    assert_eq!(ledger.committed_generation(&source_id).await, None);
    assert!(ledger.generation_count().await <= 1);
    assert!(vectors.calls().await.iter().all(|call| *call != "upsert"));
}
