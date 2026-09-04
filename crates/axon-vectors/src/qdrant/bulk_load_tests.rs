use httpmock::MockServer;
use std::path::Path;
use std::time::Duration;

use super::*;
use crate::qdrant::configure_bulk_load;

fn bulk_key(endpoint: impl Into<String>, collection: impl Into<String>) -> BulkLoadKey {
    BulkLoadKey {
        endpoint: endpoint.into(),
        collection: collection.into(),
    }
}

#[tokio::test]
async fn transition_worker_spawn_and_runtime_failures_are_typed() {
    let mut store = QdrantVectorStore::new("http://127.0.0.1:1", "qdrant-test");
    configure_bulk_load(&mut store, true, 10_485_760, 20_000);
    for (fault, expected) in [
        (
            TransitionWorkerFault::Spawn,
            "vector.qdrant.bulk_begin_spawn",
        ),
        (
            TransitionWorkerFault::Runtime,
            "vector.qdrant.bulk_begin_runtime",
        ),
    ] {
        let error = store
            .begin_bulk_load_inner_with_fault("fault", Some(fault))
            .await
            .expect_err("begin worker fault must surface");
        assert_eq!(error.code.0, expected);
    }
    for (fault, expected) in [
        (
            TransitionWorkerFault::Spawn,
            "vector.qdrant.bulk_finish_spawn",
        ),
        (
            TransitionWorkerFault::Runtime,
            "vector.qdrant.bulk_finish_runtime",
        ),
    ] {
        let error = store
            .finish_bulk_load_inner_with_fault("fault", Some(fault))
            .await
            .expect_err("finish worker fault must surface");
        assert_eq!(error.code.0, expected);
    }
}

#[test]
fn bulk_journal_process_writer() {
    let Some(directory) = std::env::var_os("AXON_BULK_JOURNAL_CHILD_DIR") else {
        return;
    };
    let prefix = std::env::var("AXON_BULK_JOURNAL_CHILD_PREFIX").expect("child prefix");
    let journal = BulkLoadJournal::open(Path::new(&directory)).expect("open child journal");
    for index in 0..100 {
        journal
            .record(
                &bulk_key("http://qdrant", format!("{prefix}-{index}")),
                20_000,
            )
            .expect("record child transition");
    }
}

#[test]
fn bulk_journal_serializes_read_modify_write_across_processes() {
    let directory = std::env::temp_dir().join(format!(
        "axon-bulk-journal-processes-{}",
        uuid::Uuid::new_v4()
    ));
    let executable = std::env::current_exe().expect("current test executable");
    let spawn = |prefix: &str| {
        std::process::Command::new(&executable)
            .args([
                "--exact",
                "qdrant::bulk_load::tests::bulk_journal_process_writer",
                "--nocapture",
            ])
            .env("AXON_BULK_JOURNAL_CHILD_DIR", &directory)
            .env("AXON_BULK_JOURNAL_CHILD_PREFIX", prefix)
            .spawn()
            .expect("spawn journal child")
    };
    let mut first = spawn("first");
    let mut second = spawn("second");
    assert!(first.wait().expect("wait first child").success());
    assert!(second.wait().expect("wait second child").success());

    let pending = BulkLoadJournal::open(&directory)
        .expect("reopen journal")
        .pending()
        .expect("read journal");
    assert_eq!(
        pending.len(),
        200,
        "neither process may lose the other's RMW"
    );
    std::fs::remove_dir_all(directory).expect("remove journal directory");
}

#[cfg(unix)]
#[test]
fn bulk_journal_is_private_and_rejects_symlink_root_and_file() {
    use std::os::unix::fs::MetadataExt as _;

    let parent = std::env::temp_dir().join(format!("axon-bulk-secure-{}", uuid::Uuid::new_v4()));
    let directory = parent.join("journal");
    let journal = BulkLoadJournal::open(&directory).expect("open journal");
    journal
        .record(&bulk_key("endpoint", "collection"), 1)
        .expect("record");
    assert_eq!(std::fs::metadata(&directory).unwrap().mode() & 0o777, 0o700);
    assert_eq!(
        std::fs::metadata(&journal.path).unwrap().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(journal.path.with_extension("lock"))
            .unwrap()
            .mode()
            & 0o777,
        0o600
    );

    let linked_root = parent.join("linked-root");
    std::os::unix::fs::symlink(&directory, &linked_root).expect("symlink root");
    assert!(BulkLoadJournal::open(&linked_root).is_err());
    std::fs::remove_file(&journal.path).expect("remove journal file");
    std::os::unix::fs::symlink(parent.join("victim"), &journal.path).expect("symlink file");
    assert!(
        journal.pending().is_err(),
        "O_NOFOLLOW must reject journal symlink"
    );
    std::fs::remove_dir_all(parent).expect("remove secure fixture");
}

#[tokio::test]
async fn journal_setup_failure_releases_owner_and_retry_lowers_threshold() {
    let server = MockServer::start_async().await;
    let lower = server
        .mock_async(|when, then| {
            when.method("PATCH")
                .path("/collections/journal-retry")
                .json_body(serde_json::json!({
                    "optimizers_config": {"indexing_threshold": 10_485_760}
                }));
            then.status(200);
        })
        .await;
    let mut store = QdrantVectorStore::new(server.base_url(), "qdrant-test");
    configure_bulk_load(&mut store, true, 10_485_760, 20_000);
    let key = store.bulk_load_key("journal-retry").unwrap();
    let setup_error = ApiError::new(
        "vector.qdrant.bulk_journal",
        ErrorStage::Upserting,
        "injected journal setup failure",
    );

    store
        .begin_bulk_load_transition_with_journal("journal-retry", Err(setup_error))
        .await
        .expect_err("journal setup must fail the begin");
    assert!(!BULK_LOAD_USERS.lock().await.contains_key(&key));

    store
        .begin_bulk_load_transition_with_journal("journal-retry", Ok(None))
        .await
        .expect("retry must establish bulk mode");
    lower.assert_calls_async(1).await;
    BULK_LOAD_USERS.lock().await.remove(&key);
}

#[tokio::test]
async fn failed_lower_and_compensation_retains_journal_for_restart_recovery() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method("PATCH")
                .path("/collections/ambiguous")
                .json_body(serde_json::json!({
                    "optimizers_config": {"indexing_threshold": 10_485_760}
                }));
            then.status(500);
        })
        .await;
    let failed_restore = server
        .mock_async(|when, then| {
            when.method("PATCH")
                .path("/collections/ambiguous")
                .json_body(serde_json::json!({
                    "optimizers_config": {"indexing_threshold": 20_000}
                }));
            then.status(500);
        })
        .await;
    let directory = std::env::temp_dir().join(format!(
        "axon-bulk-failed-compensation-{}",
        uuid::Uuid::new_v4()
    ));
    let journal = BulkLoadJournal::open(&directory).expect("open journal");
    let mut store = QdrantVectorStore::new(server.base_url(), "qdrant-test");
    configure_bulk_load(&mut store, true, 10_485_760, 20_000);

    store
        .begin_bulk_load_transition_with_journal("ambiguous", Ok(Some(&journal)))
        .await
        .expect_err("lowering and compensation must fail");
    assert_eq!(journal.pending().expect("pending recovery").len(), 1);

    failed_restore.delete_async().await;
    server
        .mock_async(|when, then| {
            when.method("PATCH").path("/collections/ambiguous");
            then.status(200);
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method("GET").path("/collections/ambiguous");
            then.status(200).json_body(serde_json::json!({
                "result": {"status": "green", "optimizer_status": "ok"}
            }));
        })
        .await;
    store
        .recover_bulk_load_transitions_from(&journal)
        .await
        .expect("restart recovery must restore normal indexing");
    assert!(journal.pending().expect("cleared recovery").is_empty());
    std::fs::remove_dir_all(directory).expect("remove journal directory");
}

#[test]
fn bulk_load_journal_survives_reopen_until_restoration_completes() {
    let directory =
        std::env::temp_dir().join(format!("axon-bulk-journal-{}", uuid::Uuid::new_v4()));
    let journal = BulkLoadJournal::open(&directory).expect("open journal");
    journal
        .record(&bulk_key("http://qdrant:6333", "docs"), 20_000)
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
        .complete(&bulk_key("http://qdrant:6333", "docs"))
        .expect("complete transition");
    assert!(reopened.pending().expect("read cleared journal").is_empty());
    std::fs::remove_dir_all(directory).expect("remove temporary journal directory");
}

#[cfg(windows)]
#[test]
fn windows_bulk_journal_replaces_existing_state_and_clears_it() {
    let directory = std::env::temp_dir().join(format!(
        "axon-bulk-journal-windows-replace-{}",
        uuid::Uuid::new_v4()
    ));
    let journal = BulkLoadJournal::open(&directory).expect("open journal");
    journal
        .record(&bulk_key("http://qdrant", "first"), 1)
        .expect("first record");
    journal
        .record(&bulk_key("http://qdrant", "second"), 2)
        .expect("replace record");
    journal
        .complete(&bulk_key("http://qdrant", "first"))
        .expect("replace on complete");
    journal
        .complete(&bulk_key("http://qdrant", "second"))
        .expect("clear journal");
    assert!(journal.pending().expect("read cleared journal").is_empty());
    std::fs::remove_dir_all(directory).expect("remove fixture");
}

#[tokio::test]
async fn compensated_begin_preserves_primary_error_when_journal_clear_fails() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method("PATCH")
                .path("/collections/cleanup-fails")
                .json_body(
                    serde_json::json!({"optimizers_config": {"indexing_threshold": 10_485_760}}),
                );
            then.status(500);
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method("PATCH")
                .path("/collections/cleanup-fails")
                .json_body(
                    serde_json::json!({"optimizers_config": {"indexing_threshold": 20_000}}),
                );
            then.status(200);
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method("GET").path("/collections/cleanup-fails");
            then.status(200).json_body(
                serde_json::json!({"result": {"status": "green", "optimizer_status": "ok"}}),
            );
        })
        .await;
    let directory =
        std::env::temp_dir().join(format!("axon-bulk-cleanup-fault-{}", uuid::Uuid::new_v4()));
    let journal = BulkLoadJournal::open(&directory).unwrap();
    journal.inject_next_complete_failure();
    let mut store = QdrantVectorStore::new(server.base_url(), "qdrant-test");
    configure_bulk_load(&mut store, true, 10_485_760, 20_000);

    let error = store
        .begin_bulk_load_transition_with_journal("cleanup-fails", Ok(Some(&journal)))
        .await
        .expect_err("lowering remains the primary failure");
    assert_ne!(error.code.0, "vector.qdrant.bulk_journal");
    assert!(
        error
            .details
            .get("journal_cleanup_error")
            .is_some_and(|value| value.contains("injected"))
    );
    assert_eq!(
        journal.pending().unwrap().len(),
        1,
        "failed clear retains recovery intent"
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn journal_failure_before_rename_preserves_last_durable_state() {
    let directory =
        std::env::temp_dir().join(format!("axon-bulk-journal-fault-{}", uuid::Uuid::new_v4()));
    let journal = BulkLoadJournal::open(&directory).expect("open journal");
    journal
        .record(&bulk_key("http://qdrant:6333", "docs"), 20_000)
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
        .record(&bulk_key("http://qdrant:6333", "docs"), 20_000)
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
        .record(&bulk_key("http://qdrant:6333", "docs"), 20_000)
        .expect("record transition");
    journal
        .complete(&bulk_key("http://qdrant:6333", "docs"))
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
        .record(&bulk_key(server.base_url(), "crashed"), 20_000)
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
async fn credential_and_query_aliases_share_bulk_load_ownership() {
    let server = MockServer::start_async().await;
    let parsed = url::Url::parse(&server.base_url()).unwrap();
    let host = parsed.host_str().unwrap();
    let port = parsed.port().unwrap();
    let first_url = format!("http://first:secret@{host}:{port}/?api_key=ignored");
    let second_url = format!("http://second:secret@{host}:{port}?api_key=other#fragment");
    let lower = server
        .mock_async(|when, then| {
            when.method("PATCH").path("/collections/aliased").json_body(
                serde_json::json!({"optimizers_config": {"indexing_threshold": 10_485_760}}),
            );
            then.status(200);
        })
        .await;
    let restore = server
        .mock_async(|when, then| {
            when.method("PATCH").path("/collections/aliased").json_body(
                serde_json::json!({"optimizers_config": {"indexing_threshold": 20_000}}),
            );
            then.status(200);
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method("GET").path("/collections/aliased");
            then.status(200).json_body(
                serde_json::json!({"result": {"status": "green", "optimizer_status": "ok"}}),
            );
        })
        .await;
    let mut first = QdrantVectorStore::new(first_url, "first");
    let mut second = QdrantVectorStore::new(second_url, "second");
    configure_bulk_load(&mut first, true, 10_485_760, 20_000);
    configure_bulk_load(&mut second, true, 9_000_000, 30_000);

    first.begin_bulk_load_inner("aliased").await.unwrap();
    second.begin_bulk_load_inner("aliased").await.unwrap();
    first.finish_bulk_load_inner("aliased").await.unwrap();
    lower.assert_calls_async(1).await;
    restore.assert_calls_async(0).await;
    second.finish_bulk_load_inner("aliased").await.unwrap();
    restore.assert_calls_async(1).await;
}

#[tokio::test]
async fn credential_aliases_restore_first_owner_threshold_in_reverse_finish_order() {
    let server = MockServer::start_async().await;
    let parsed = url::Url::parse(&server.base_url()).unwrap();
    let origin = format!("{}:{}", parsed.host_str().unwrap(), parsed.port().unwrap());
    let mut first = QdrantVectorStore::new(format!("http://first:key@{origin}"), "first");
    let mut second = QdrantVectorStore::new(
        format!("http://second:key@{origin}?api_key=other"),
        "second",
    );
    configure_bulk_load(&mut first, true, 10_485_760, 20_000);
    configure_bulk_load(&mut second, true, 9_000_000, 30_000);
    let lower = server
        .mock_async(|when, then| {
            when.method("PATCH").path("/collections/reverse").json_body(
                serde_json::json!({"optimizers_config": {"indexing_threshold": 10_485_760}}),
            );
            then.status(200);
        })
        .await;
    let restore = server
        .mock_async(|when, then| {
            when.method("PATCH").path("/collections/reverse").json_body(
                serde_json::json!({"optimizers_config": {"indexing_threshold": 20_000}}),
            );
            then.status(200);
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method("GET").path("/collections/reverse");
            then.status(200).json_body(
                serde_json::json!({"result": {"status": "green", "optimizer_status": "ok"}}),
            );
        })
        .await;

    first.begin_bulk_load_inner("reverse").await.unwrap();
    second.begin_bulk_load_inner("reverse").await.unwrap();
    second.finish_bulk_load_inner("reverse").await.unwrap();
    lower.assert_calls_async(1).await;
    restore.assert_calls_async(0).await;
    first.finish_bulk_load_inner("reverse").await.unwrap();
    restore.assert_calls_async(1).await;
}

#[tokio::test]
async fn failed_finish_journal_clear_evicts_owner_and_next_begin_captures_fresh_baseline() {
    let server = MockServer::start_async().await;
    for threshold in [10_485_760_u64, 9_000_000, 20_000, 30_000] {
        server
            .mock_async(move |when, then| {
                when.method("PATCH")
                    .path("/collections/fresh-baseline")
                    .json_body(
                        serde_json::json!({"optimizers_config": {"indexing_threshold": threshold}}),
                    );
                then.status(200);
            })
            .await;
    }
    server
        .mock_async(|when, then| {
            when.method("GET").path("/collections/fresh-baseline");
            then.status(200).json_body(
                serde_json::json!({"result": {"status": "green", "optimizer_status": "ok"}}),
            );
        })
        .await;
    let directory = std::env::temp_dir().join(format!(
        "axon-bulk-finish-clear-fault-{}",
        uuid::Uuid::new_v4()
    ));
    let journal = BulkLoadJournal::open(&directory).unwrap();
    let mut first = QdrantVectorStore::new(server.base_url(), "first");
    configure_bulk_load(&mut first, true, 10_485_760, 20_000);
    let key = first.bulk_load_key("fresh-baseline").unwrap();
    first
        .begin_bulk_load_transition_with_journal("fresh-baseline", Ok(Some(&journal)))
        .await
        .unwrap();
    journal.inject_next_complete_failure();
    let error = first
        .finish_bulk_load_transition_with_journal("fresh-baseline", Ok(Some(&journal)))
        .await
        .expect_err("journal clear failure must surface");
    assert_eq!(error.code.0, "vector.qdrant.bulk_journal");
    assert!(!BULK_LOAD_USERS.lock().await.contains_key(&key));
    assert_eq!(journal.pending().unwrap()[0].restore_threshold, 20_000);

    let mut second = QdrantVectorStore::new(server.base_url(), "second");
    configure_bulk_load(&mut second, true, 9_000_000, 30_000);
    second
        .begin_bulk_load_transition_with_journal("fresh-baseline", Ok(Some(&journal)))
        .await
        .unwrap();
    assert_eq!(journal.pending().unwrap()[0].restore_threshold, 30_000);
    second
        .finish_bulk_load_transition_with_journal("fresh-baseline", Ok(Some(&journal)))
        .await
        .unwrap();
    assert!(journal.pending().unwrap().is_empty());
    assert!(!BULK_LOAD_USERS.lock().await.contains_key(&key));
    std::fs::remove_dir_all(directory).unwrap();
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
    let registry = tokio::time::timeout(Duration::from_millis(50), BULK_LOAD_USERS.lock())
        .await
        .expect("provider I/O must not retain the global registry lock");
    drop(registry);
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
    let key = store.bulk_load_key("cleanup-entry").unwrap();

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
    let key = store.bulk_load_key("cancel-begin").unwrap();

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
    let key = store.bulk_load_key("cancel-finish").unwrap();
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
