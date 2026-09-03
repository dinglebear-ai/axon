use httpmock::MockServer;
use std::time::Duration;

use super::*;
use crate::qdrant::configure_bulk_load;

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
    let workers = std::sync::Mutex::new(TransitionWorkers::default());
    let existing_finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let finished = std::sync::Arc::clone(&existing_finished);
    track_transition_worker_in(
        &workers,
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(40));
            finished.store(true, std::sync::atomic::Ordering::Release);
        }),
    );

    drain_bulk_load_transition_workers_in(&workers);
    assert!(existing_finished.load(std::sync::atomic::Ordering::Acquire));

    let late_finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let finished = std::sync::Arc::clone(&late_finished);
    track_transition_worker_in(
        &workers,
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(40));
            finished.store(true, std::sync::atomic::Ordering::Release);
        }),
    );
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
    tokio::time::timeout(Duration::from_secs(2), async {
        while restore.calls_async().await == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cancelled begin must compensate indexing threshold");

    restore.assert_calls_async(1).await;
    tokio::time::timeout(Duration::from_secs(2), async {
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
