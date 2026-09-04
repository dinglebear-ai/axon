use httpmock::MockServer;
use std::time::Duration;

use super::*;
use crate::qdrant::configure_bulk_load;

#[test]
fn bulk_load_journal_survives_reopen_until_restoration_completes() {
    let directory =
        std::env::temp_dir().join(format!("axon-bulk-journal-{}", uuid::Uuid::new_v4()));
    let journal = BulkLoadJournal::open(&directory).expect("open journal");
    journal
        .record("http://qdrant:6333", "docs", 20_000)
        .expect("record transition");

    let reopened = BulkLoadJournal::open(&directory).expect("reopen journal");
    assert_eq!(
        reopened.pending().expect("read pending transitions"),
        vec![PendingBulkLoad {
            endpoint: "http://qdrant:6333".to_string(),
            collection: "docs".to_string(),
            restore_threshold: 20_000,
        }]
    );
    reopened
        .complete("http://qdrant:6333", "docs")
        .expect("complete transition");
    assert!(reopened.pending().expect("read cleared journal").is_empty());
    std::fs::remove_dir_all(directory).expect("remove temporary journal directory");
}

#[test]
fn journal_failure_before_rename_preserves_last_durable_state() {
    let directory =
        std::env::temp_dir().join(format!("axon-bulk-journal-fault-{}", uuid::Uuid::new_v4()));
    let journal = BulkLoadJournal::open(&directory).expect("open journal");
    journal
        .record("http://qdrant:6333", "docs", 20_000)
        .expect("record durable transition");

    let error = journal
        .write_unlocked_with(&[], |boundary| {
            if boundary == JournalWriteBoundary::BeforeRename {
                return Err(std::io::Error::other("injected pre-rename failure"));
            }
            Ok(())
        })
        .expect_err("injected boundary must fail");
    assert_eq!(error.kind(), std::io::ErrorKind::Other);

    let reopened = BulkLoadJournal::open(&directory).expect("reopen journal");
    assert_eq!(reopened.pending().expect("read durable state").len(), 1);
    std::fs::remove_dir_all(directory).expect("remove temporary journal directory");
}

#[test]
fn journal_failure_after_rename_leaves_complete_recoverable_state() {
    let directory =
        std::env::temp_dir().join(format!("axon-bulk-journal-fault-{}", uuid::Uuid::new_v4()));
    let journal = BulkLoadJournal::open(&directory).expect("open journal");
    journal
        .record("http://qdrant:6333", "docs", 20_000)
        .expect("record durable transition");

    journal
        .write_unlocked_with(&[], |boundary| {
            if boundary == JournalWriteBoundary::BeforeParentSync {
                return Err(std::io::Error::other("injected parent-sync failure"));
            }
            Ok(())
        })
        .expect_err("injected boundary must fail");

    let reopened = BulkLoadJournal::open(&directory).expect("reopen journal");
    assert!(
        reopened
            .pending()
            .expect("read complete new state")
            .is_empty(),
        "rename must expose a complete journal file even when directory sync reports failure"
    );
    std::fs::remove_dir_all(directory).expect("remove temporary journal directory");
}

#[test]
fn journal_clear_is_durable_across_reopen() {
    let directory =
        std::env::temp_dir().join(format!("axon-bulk-journal-clear-{}", uuid::Uuid::new_v4()));
    let journal = BulkLoadJournal::open(&directory).expect("open journal");
    journal
        .record("http://qdrant:6333", "docs", 20_000)
        .expect("record transition");
    journal
        .complete("http://qdrant:6333", "docs")
        .expect("durably clear transition");

    drop(journal);
    let reopened = BulkLoadJournal::open(&directory).expect("reopen journal");
    assert!(reopened.pending().expect("read durable clear").is_empty());
    std::fs::remove_dir_all(directory).expect("remove temporary journal directory");
}

#[tokio::test]
async fn stale_bulk_load_journal_is_restored_and_cleared_idempotently() {
    let server = MockServer::start_async().await;
    let restore = server
        .mock_async(|when, then| {
            when.method("PATCH").path("/collections/crashed").json_body(
                serde_json::json!({"optimizers_config": {"indexing_threshold": 20_000}}),
            );
            then.status(200);
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method("GET").path("/collections/crashed");
            then.status(200).json_body(serde_json::json!({
                "result": {"status": "green", "optimizer_status": "ok"}
            }));
        })
        .await;
    let directory =
        std::env::temp_dir().join(format!("axon-bulk-recovery-{}", uuid::Uuid::new_v4()));
    let journal = BulkLoadJournal::open(&directory).expect("open journal");
    journal
        .record(&server.base_url(), "crashed", 20_000)
        .expect("record crashed transition");
    let store = QdrantVectorStore::new(server.base_url(), "qdrant-test");

    store
        .recover_bulk_load_transitions_from(&journal)
        .await
        .expect("recover stale transition");
    store
        .recover_bulk_load_transitions_from(&journal)
        .await
        .expect("empty recovery is idempotent");

    restore.assert_calls_async(1).await;
    assert!(journal.pending().expect("journal cleared").is_empty());
    std::fs::remove_dir_all(directory).expect("remove temporary journal directory");
}

#[tokio::test]
async fn bulk_load_restores_threshold_and_waits_for_green_optimizer() {
    let server = MockServer::start_async().await;
    let patch = server
        .mock_async(|when, then| {
            when.method("PATCH").path("/collections/axon-test");
            then.status(200);
        })
        .await;
    let status = server
        .mock_async(|when, then| {
            when.method("GET").path("/collections/axon-test");
            then.status(200).json_body(serde_json::json!({
                "result": {"status": "green", "optimizer_status": "ok"}
            }));
        })
        .await;
    let mut store = QdrantVectorStore::new(server.base_url(), "qdrant-test");
    configure_bulk_load(&mut store, true, 10_485_760, 20_000);

    store.begin_bulk_load_inner("axon-test").await.unwrap();
    store.finish_bulk_load_inner("axon-test").await.unwrap();

    patch.assert_calls_async(2).await;
    status.assert_calls_async(1).await;
}

#[tokio::test]
async fn overlapping_bulk_loads_restore_only_after_last_user() {
    let server = MockServer::start_async().await;
    let patch = server
        .mock_async(|when, then| {
            when.method("PATCH").path("/collections/shared");
            then.status(200);
        })
        .await;
    let status = server
        .mock_async(|when, then| {
            when.method("GET").path("/collections/shared");
            then.status(200).json_body(serde_json::json!({
                "result": {"status": "green", "optimizer_status": "ok"}
            }));
        })
        .await;
    let mut store = QdrantVectorStore::new(server.base_url(), "qdrant-test");
    configure_bulk_load(&mut store, true, 10_485_760, 20_000);

    store.begin_bulk_load_inner("shared").await.unwrap();
    store.begin_bulk_load_inner("shared").await.unwrap();
    store.finish_bulk_load_inner("shared").await.unwrap();
    patch.assert_calls_async(1).await;
    store.finish_bulk_load_inner("shared").await.unwrap();

    patch.assert_calls_async(2).await;
    status.assert_calls_async(1).await;
}

#[tokio::test]
async fn unrelated_collections_do_not_hold_the_registry_during_provider_io() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method("PATCH").path("/collections/slow-a");
            then.status(200).delay(Duration::from_millis(200));
        })
        .await;
    let mut store = QdrantVectorStore::new(server.base_url(), "qdrant-test");
    configure_bulk_load(&mut store, true, 10_485_760, 20_000);

    let pending = tokio::spawn(async move { store.begin_bulk_load_inner("slow-a").await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    let _registry = tokio::time::timeout(Duration::from_millis(50), BULK_LOAD_USERS.lock())
        .await
        .expect("provider I/O must not retain the global registry lock");
    pending.await.unwrap().unwrap();
}

#[tokio::test]
async fn completed_bulk_load_removes_its_idle_registry_entry() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method("PATCH").path("/collections/cleanup-entry");
            then.status(200);
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method("GET").path("/collections/cleanup-entry");
            then.status(200).json_body(serde_json::json!({
                "result": {"status": "green", "optimizer_status": "ok"}
            }));
        })
        .await;
    let mut store = QdrantVectorStore::new(server.base_url(), "qdrant-test");
    configure_bulk_load(&mut store, true, 10_485_760, 20_000);
    let key = format!("{}\0cleanup-entry", server.base_url());

    store.begin_bulk_load_inner("cleanup-entry").await.unwrap();
    store.finish_bulk_load_inner("cleanup-entry").await.unwrap();

    assert!(!BULK_LOAD_USERS.lock().await.contains_key(&key));
}

#[test]
fn transition_worker_drain_joins_existing_and_late_workers() {
    let workers = DetachedWorkerRegistry::default();
    let existing_finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let finished = std::sync::Arc::clone(&existing_finished);
    workers.track(std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(40));
        finished.store(true, std::sync::atomic::Ordering::Release);
    }));

    workers.drain();
    assert!(existing_finished.load(std::sync::atomic::Ordering::Acquire));

    let late_finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let finished = std::sync::Arc::clone(&late_finished);
    workers.track(std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(40));
        finished.store(true, std::sync::atomic::Ordering::Release);
    }));
    assert!(late_finished.load(std::sync::atomic::Ordering::Acquire));
}

#[tokio::test]
async fn cancelled_begin_compensates_and_cleans_registry_state() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method("PATCH")
                .path("/collections/cancel-begin")
                .json_body(
                    serde_json::json!({"optimizers_config": {"indexing_threshold": 10_485_760}}),
                );
            then.status(200).delay(Duration::from_millis(200));
        })
        .await;
    let restore = server
        .mock_async(|when, then| {
            when.method("PATCH")
                .path("/collections/cancel-begin")
                .json_body(
                    serde_json::json!({"optimizers_config": {"indexing_threshold": 20_000}}),
                );
            then.status(200);
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method("GET").path("/collections/cancel-begin");
            then.status(200).json_body(serde_json::json!({
                "result": {"status": "green", "optimizer_status": "ok"}
            }));
        })
        .await;
    let mut store = QdrantVectorStore::new(server.base_url(), "qdrant-test");
    configure_bulk_load(&mut store, true, 10_485_760, 20_000);
    let key = format!("{}\0cancel-begin", server.base_url());

    let pending = tokio::spawn(async move { store.begin_bulk_load_inner("cancel-begin").await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    pending.abort();
    tokio::time::timeout(Duration::from_secs(30), async {
        while restore.calls_async().await == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cancelled begin must compensate indexing threshold");

    restore.assert_calls_async(1).await;
    tokio::time::timeout(Duration::from_secs(30), async {
        while BULK_LOAD_USERS.lock().await.contains_key(&key) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cancelled begin must clean its registry entry");
}

#[tokio::test]
async fn cancelled_finish_completes_restore_and_cleans_registry_state() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method("PATCH")
                .path("/collections/cancel-finish")
                .json_body(
                    serde_json::json!({"optimizers_config": {"indexing_threshold": 10_485_760}}),
                );
            then.status(200);
        })
        .await;
    let restore = server
        .mock_async(|when, then| {
            when.method("PATCH")
                .path("/collections/cancel-finish")
                .json_body(
                    serde_json::json!({"optimizers_config": {"indexing_threshold": 20_000}}),
                );
            then.status(200).delay(Duration::from_millis(200));
        })
        .await;
    let ready = server
        .mock_async(|when, then| {
            when.method("GET").path("/collections/cancel-finish");
            then.status(200).json_body(serde_json::json!({
                "result": {"status": "green", "optimizer_status": "ok"}
            }));
        })
        .await;
    let mut store = QdrantVectorStore::new(server.base_url(), "qdrant-test");
    configure_bulk_load(&mut store, true, 10_485_760, 20_000);
    let key = format!("{}\0cancel-finish", server.base_url());
    store.begin_bulk_load_inner("cancel-finish").await.unwrap();

    let pending = tokio::spawn(async move { store.finish_bulk_load_inner("cancel-finish").await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while restore.calls_async().await == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("finish must enter the delayed restore before cancellation");
    pending.abort();
    let _ = pending.await;

    tokio::time::timeout(Duration::from_secs(2), async {
        while ready.calls_async().await == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cancelled finish must complete optimizer restoration");
    assert!(restore.calls_async().await >= 1);
    ready.assert_calls_async(1).await;
    tokio::time::timeout(Duration::from_secs(2), async {
        while BULK_LOAD_USERS.lock().await.contains_key(&key) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cancelled finish must clean its registry entry");
}
