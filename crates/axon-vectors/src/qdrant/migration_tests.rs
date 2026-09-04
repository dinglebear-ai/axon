use super::*;
use httpmock::MockServer;
use std::sync::Arc;
use std::time::Duration;

#[test]
fn migration_rejects_malformed_points_instead_of_reporting_partial_success() {
    let missing_text = serde_json::json!({"id": "point-1", "vector": [1.0], "payload": {}});
    let error = transform_point(&missing_text).expect_err("missing text must fail closed");
    assert_eq!(error.code.to_string(), "vector.migration.invalid_point");

    let malformed_vector =
        serde_json::json!({"id": "point-2", "vector": ["bad"], "payload": {"text": "ok"}});
    assert!(transform_point(&malformed_vector).is_err());
}

#[tokio::test]
async fn migration_overlaps_next_scroll_with_current_upsert_under_backpressure() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method("GET").path("/collections/source");
            then.status(200).json_body(serde_json::json!({
                "result": {"config": {"params": {"vectors": {"size": 1}}}}
            }));
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method("GET").path("/collections/destination");
            then.status(404);
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method("PUT").path("/collections/destination");
            then.status(200);
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method("POST")
                .path("/collections/source/points/scroll")
                .json_body(serde_json::json!({
                    "limit": 1, "with_payload": true, "with_vector": true
                }));
            then.status(200).json_body(serde_json::json!({
                "result": {"points": [{"id": "one", "vector": [1.0], "payload": {"text": "one"}}], "next_page_offset": "next"}
            }));
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method("POST")
                .path("/collections/source/points/scroll")
                .json_body(serde_json::json!({
                    "limit": 1,
                    "with_payload": true,
                    "with_vector": true,
                    "offset": "next"
                }));
            then.status(200).json_body(serde_json::json!({
                    "result": {"points": [{"id": "two", "vector": [1.0], "payload": {"text": "two"}}], "next_page_offset": null}
                }));
        })
        .await;
    let upserts = server
        .mock_async(|when, then| {
            when.method("PUT").path("/collections/destination/points");
            then.status(200);
        })
        .await;

    let receipt = migrate_unnamed_collection(
        server.base_url(),
        "migration-test",
        "source",
        "destination",
        1,
    )
    .await
    .expect("migration");

    assert_eq!(receipt.points_migrated, 2);
    assert_eq!(receipt.pages_processed, 2);
    upserts.assert_calls_async(2).await;
}

#[tokio::test]
async fn migration_page_pipeline_polls_write_and_next_scroll_concurrently() {
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let write_barrier = Arc::clone(&barrier);
    let fetch_barrier = Arc::clone(&barrier);

    let joined = tokio::time::timeout(
        Duration::from_secs(10),
        overlap_write_and_fetch(
            async move {
                write_barrier.wait().await;
                "write"
            },
            async move {
                fetch_barrier.wait().await;
                "fetch"
            },
        ),
    )
    .await
    .expect("write and next-page fetch must both reach the barrier");

    assert_eq!(joined, ("write", "fetch"));
}
