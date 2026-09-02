use httpmock::MockServer;

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
