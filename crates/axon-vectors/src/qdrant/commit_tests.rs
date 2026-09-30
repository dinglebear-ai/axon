use super::*;
use httpmock::MockServer;

#[tokio::test]
async fn carry_forward_splits_requests_by_encoded_bytes_not_only_point_count() {
    let server = MockServer::start_async().await;
    let upsert = server
        .mock_async(|when, then| {
            when.method("PUT").path("/points");
            then.status(200)
                .json_body(serde_json::json!({"status":"ok","result":{"status":"completed"}}));
        })
        .await;
    let store = QdrantVectorStore::new(server.base_url(), "carry-byte-test");
    let http = store.http().unwrap();
    let points = (0..3).map(|id| serde_json::json!({
        "id": id, "vector":{"dense":[1.0,2.0],"bm42":{"indices":[1],"values":[0.5]}},
        "payload":{"source_generation":8,"committed_generation":8,"body":"x".repeat(6*1024*1024)}
    })).collect();
    let requests = upsert_carried_points(
        &store,
        &http,
        &format!("{}/points", server.base_url()),
        points,
        ErrorStage::Publishing,
    )
    .await
    .unwrap();
    assert_eq!(
        requests, 2,
        "18 MiB of carried points must not share a 16 MiB request"
    );
    upsert.assert_calls_async(2).await;
}

#[tokio::test]
async fn carry_forward_rejects_one_oversized_point_before_sending() {
    let server = MockServer::start_async().await;
    let upsert = server
        .mock_async(|when, then| {
            when.method("PUT").path("/points");
            then.status(200)
                .json_body(serde_json::json!({"status":"ok","result":{"status":"completed"}}));
        })
        .await;
    let store = QdrantVectorStore::new(server.base_url(), "carry-oversize-test");
    let http = store.http().unwrap();
    let point = serde_json::json!({"id":1,"payload":{"body":"x".repeat(17*1024*1024)}});
    let error = upsert_carried_points(
        &store,
        &http,
        &format!("{}/points", server.base_url()),
        vec![point],
        ErrorStage::Publishing,
    )
    .await
    .expect_err("one oversized point must fail");
    assert_eq!(error.code, "vector.qdrant.upsert_point_oversized".into());
    upsert.assert_calls_async(0).await;
}

#[test]
fn carry_forward_filter_contains_only_the_requested_key_batch() {
    let keys = vec!["a".to_string(), "b".to_string()];
    let filter = carry_forward_filter(&SourceId::new("source"), 7, &keys);
    assert_eq!(filter["must"][0]["match"]["value"], "source");
    assert_eq!(filter["must"][1]["match"]["value"], 7);
    assert_eq!(
        filter["must"][2]["match"]["any"],
        serde_json::json!(["a", "b"])
    );
}

#[test]
fn carry_forward_key_batches_bound_match_any_requests() {
    let keys = (0..257)
        .map(|index| format!("key-{index}"))
        .collect::<Vec<_>>();
    let batches = keys
        .chunks(CARRY_FORWARD_KEY_BATCH_SIZE)
        .collect::<Vec<_>>();
    assert_eq!(batches.len(), 2);
    assert_eq!(batches[0].len(), 256);
    assert_eq!(batches[1], ["key-256"]);
}

fn collection_spec(name: &str) -> CollectionSpec {
    CollectionSpec {
        collection: name.to_string(),
        dense: VectorConfig {
            name: "dense".to_string(),
            dimensions: 2,
            distance: VectorDistance::Cosine,
        },
        payload_indexes: Vec::new(),
        sparse: None,
        aliases: Vec::new(),
        distance: Some(VectorDistance::Cosine),
        metadata: MetadataMap::new(),
    }
}

fn scroll_body(filter: &serde_json::Value, offset: Option<&str>) -> serde_json::Value {
    let mut body = serde_json::json!({
        "filter": filter,
        "limit": SCROLL_PAGE_LIMIT,
        "with_payload": true,
        "with_vector": true,
    });
    if let Some(offset) = offset {
        body["offset"] = serde_json::Value::from(offset);
    }
    body
}

fn scroll_point(id: &str, key: &str, marker: u64) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "vector": { "dense": [marker as f64, 1.0] },
        "payload": {
            "source_id": "source",
            "source_item_key": key,
            "source_generation": 7,
            "committed_generation": 7,
            "document_status": "published",
            "vector_point_id": id,
            "marker": marker,
        },
    })
}

fn carried_body(
    points: &[(&str, &str, u64)],
    generation: &SourceGenerationId,
) -> serde_json::Value {
    serde_json::json!({
        "points": points.iter().map(|(id, key, marker)| {
            let new_id = carried_point_id(id, generation).0;
            serde_json::json!({
                "id": new_id,
                "vector": { "dense": [*marker as f64, 1.0] },
                "payload": {
                    "source_id": "source",
                    "source_item_key": key,
                    "source_generation": 8,
                    "committed_generation": 8,
                    "document_status": "published",
                    "vector_point_id": new_id,
                    "marker": marker,
                },
            })
        }).collect::<Vec<_>>()
    })
}

#[tokio::test]
async fn carry_forward_batches_scroll_pages_and_rewrites_committed_points() {
    let server = MockServer::start_async().await;
    let collection = "axon-test";
    let source_id = SourceId::new("source");
    let previous_generation = SourceGenerationId::new("7");
    let committed_generation = SourceGenerationId::new("8");
    let keys = (0..257)
        .map(|index| SourceItemKey::new(format!("key-{index}")))
        .collect::<Vec<_>>();
    let sorted_keys = (0..257)
        .map(|index| format!("key-{index}"))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let first_keys = sorted_keys[..256].to_vec();
    let second_keys = sorted_keys[256..].to_vec();
    let first_key = first_keys[0].clone();
    let next_key = first_keys[1].clone();
    let continued_key = first_keys[255].clone();
    let reset_key = second_keys[0].clone();
    let first_filter = carry_forward_filter(&source_id, 7, &first_keys);
    let second_filter = carry_forward_filter(&source_id, 7, &second_keys);

    let first_scroll = server
        .mock_async(|when, then| {
            when.method("POST")
                .path("/collections/axon-test/points/scroll")
                .json_body(scroll_body(&first_filter, None));
            then.status(200).json_body(serde_json::json!({
                "result": {
                    "points": [
                        scroll_point("point-a", &first_key, 1),
                        scroll_point("point-b", &next_key, 2),
                    ],
                    "next_page_offset": "page-2",
                }
            }));
        })
        .await;
    let continued_scroll = server
        .mock_async(|when, then| {
            when.method("POST")
                .path("/collections/axon-test/points/scroll")
                .json_body(scroll_body(&first_filter, Some("page-2")));
            then.status(200).json_body(serde_json::json!({
                "result": {
                    "points": [scroll_point("point-c", &continued_key, 3)],
                    "next_page_offset": null,
                }
            }));
        })
        .await;
    let reset_scroll = server
        .mock_async(|when, then| {
            when.method("POST")
                .path("/collections/axon-test/points/scroll")
                .json_body(scroll_body(&second_filter, None));
            then.status(200).json_body(serde_json::json!({
                "result": {
                    "points": [scroll_point("point-d", &reset_key, 4)],
                    "next_page_offset": null,
                }
            }));
        })
        .await;

    let first_upsert_body = carried_body(
        &[
            ("point-a", first_key.as_str(), 1),
            ("point-b", next_key.as_str(), 2),
        ],
        &committed_generation,
    );
    let continued_upsert_body = carried_body(
        &[("point-c", continued_key.as_str(), 3)],
        &committed_generation,
    );
    let second_upsert_body =
        carried_body(&[("point-d", reset_key.as_str(), 4)], &committed_generation);
    let first_upsert = server
        .mock_async(|when, then| {
            when.method("PUT")
                .path("/collections/axon-test/points")
                .json_body(first_upsert_body);
            then.status(200);
        })
        .await;
    let continued_upsert = server
        .mock_async(|when, then| {
            when.method("PUT")
                .path("/collections/axon-test/points")
                .json_body(continued_upsert_body);
            then.status(200);
        })
        .await;
    let second_upsert = server
        .mock_async(|when, then| {
            when.method("PUT")
                .path("/collections/axon-test/points")
                .json_body(second_upsert_body);
            then.status(200);
        })
        .await;

    let store = QdrantVectorStore::new(server.base_url(), "qdrant-test");
    store
        .cache_collection_spec(collection_spec(collection))
        .await;
    let http = store.http().expect("qdrant HTTP wrapper");
    let result = mark_unchanged_items_committed_rest(
        &store,
        &http,
        collection.to_string(),
        source_id,
        previous_generation,
        committed_generation,
        keys,
    )
    .await
    .expect("carry forward points");

    assert_eq!(result.points_attempted, 4);
    assert_eq!(result.points_written, 4);
    assert_eq!(result.usage.requests, 6);
    first_scroll.assert_calls_async(1).await;
    continued_scroll.assert_calls_async(1).await;
    reset_scroll.assert_calls_async(1).await;
    first_upsert.assert_calls_async(1).await;
    continued_upsert.assert_calls_async(1).await;
    second_upsert.assert_calls_async(1).await;
}

#[tokio::test]
async fn mark_unchanged_items_committed_rejects_upsert_conflict() {
    let server = MockServer::start_async().await;
    let source_id = SourceId::new("source");
    let previous_generation = SourceGenerationId::new("7");
    let committed_generation = SourceGenerationId::new("8");
    let keys = vec![SourceItemKey::new("key-a")];
    let filter = carry_forward_filter(&source_id, 7, &["key-a".to_string()]);
    let scroll = server
        .mock_async(|when, then| {
            when.method("POST")
                .path("/collections/axon-test/points/scroll")
                .json_body(scroll_body(&filter, None));
            then.status(200).json_body(serde_json::json!({
                "result": {
                    "points": [scroll_point("point-a", "key-a", 1)],
                    "next_page_offset": null
                }
            }));
        })
        .await;
    let conflict = server
        .mock_async(|when, then| {
            when.method("PUT").path("/collections/axon-test/points");
            then.status(409);
        })
        .await;
    let store = QdrantVectorStore::new(server.base_url(), "qdrant-test");
    store
        .cache_collection_spec(collection_spec("axon-test"))
        .await;
    let http = store.http().expect("qdrant HTTP wrapper");

    let error = mark_unchanged_items_committed_rest(
        &store,
        &http,
        "axon-test".to_string(),
        source_id,
        previous_generation,
        committed_generation,
        keys,
    )
    .await
    .expect_err("carry-forward upsert conflict must fail");

    assert!(error.to_string().contains("409"));
    scroll.assert_calls_async(1).await;
    conflict.assert_calls_async(1).await;
}

#[tokio::test]
async fn generation_publish_waits_for_async_qdrant_payload_update() {
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock Qdrant");
    let address = listener.local_addr().expect("mock address");
    let server = tokio::spawn(async move {
        let responses = [
            ("/collections/axon-test/points/count", 2),
            ("/collections/axon-test/points/payload?wait=false", 0),
            ("/collections/axon-test/points/count", 0),
            ("/collections/axon-test/points/count", 2),
        ];
        for (expected_path, count) in responses {
            let (mut stream, _) = listener.accept().await.expect("accept request");
            let mut request = [0_u8; 8192];
            let read = stream.read(&mut request).await.expect("read request");
            let request_line = String::from_utf8_lossy(&request[..read]);
            let actual_path = request_line
                .split_whitespace()
                .nth(1)
                .expect("request path");
            let (status, body) = if actual_path == expected_path {
                if expected_path.contains("/payload") {
                    (
                        200,
                        r#"{"result":{"operation_id":1,"status":"acknowledged"}}"#.to_string(),
                    )
                } else {
                    (200, format!(r#"{{"result":{{"count":{count}}}}}"#))
                }
            } else {
                (
                    400,
                    format!(r#"{{"error":"unexpected path: {actual_path}"}}"#),
                )
            };
            let response = format!(
                "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("write response");
        }
    });

    let store = QdrantVectorStore::new(format!("http://{address}"), "qdrant-test");
    store
        .cache_collection_spec(collection_spec("axon-test"))
        .await;
    let http = store.http().expect("Qdrant transport");
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        mark_generation_committed_rest(
            &store,
            &http,
            "axon-test".to_string(),
            SourceId::new("src-test"),
            SourceGenerationId::new("gen_1"),
        ),
    )
    .await
    .expect("publish did not hang")
    .expect("publish succeeded");
    assert_eq!(result.points_written, 2);
    server.await.expect("mock Qdrant finished");
}

#[tokio::test]
async fn generation_retirement_waits_for_async_qdrant_payload_update() {
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock Qdrant");
    let address = listener.local_addr().expect("mock address");
    let server = tokio::spawn(async move {
        let responses = [
            ("/collections/axon-test/points/count", 2),
            ("/collections/axon-test/points/payload?wait=false", 0),
            ("/collections/axon-test/points/count", 2),
            ("/collections/axon-test/points/count", 0),
        ];
        for (expected_path, count) in responses {
            let (mut stream, _) = listener.accept().await.expect("accept request");
            let mut request = [0_u8; 8192];
            let read = stream.read(&mut request).await.expect("read request");
            let request_line = String::from_utf8_lossy(&request[..read]);
            let actual_path = request_line
                .split_whitespace()
                .nth(1)
                .expect("request path");
            let (status, body) = if actual_path == expected_path {
                if expected_path.contains("/payload") {
                    (
                        200,
                        r#"{"result":{"operation_id":1,"status":"acknowledged"}}"#.to_string(),
                    )
                } else {
                    (200, format!(r#"{{"result":{{"count":{count}}}}}"#))
                }
            } else {
                (
                    400,
                    format!(r#"{{"error":"unexpected path: {actual_path}"}}"#),
                )
            };
            let response = format!(
                "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("write response");
        }
    });

    let store = QdrantVectorStore::new(format!("http://{address}"), "qdrant-test");
    store
        .cache_collection_spec(collection_spec("axon-test"))
        .await;
    let http = store.http().expect("Qdrant transport");
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        retire_generation_rest(
            &store,
            &http,
            "axon-test".to_string(),
            SourceId::new("src-test"),
            SourceGenerationId::new("gen_1"),
            SourceGenerationId::new("gen_2"),
        ),
    )
    .await
    .expect("retirement did not hang")
    .expect("retirement succeeded");
    assert_eq!(result.points_written, 2);
    server.await.expect("mock Qdrant finished");
}
