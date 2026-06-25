use super::*;
use crate::ops::qdrant::utils::validate_collection_name;
use axon_core::config::Config;
use httpmock::prelude::*;

fn named_collection_body() -> serde_json::Value {
    serde_json::json!({
        "result": {
            "config": {
                "params": {
                    "vectors": {
                        "dense": {"size": 4, "distance": "Cosine"}
                    },
                    "sparse_vectors": {
                        "bm42": {"modifier": "idf"}
                    }
                }
            }
        }
    })
}

fn unnamed_collection_body() -> serde_json::Value {
    serde_json::json!({
        "result": {
            "config": {
                "params": {
                    "vectors": {"size": 4, "distance": "Cosine"}
                }
            }
        }
    })
}

fn query_response(url: &str, score: f64) -> serde_json::Value {
    serde_json::json!({
        "result": {
            "points": [
                {"id": "hit", "score": score, "payload": {"url": url, "chunk_text": "chunk"}}
            ]
        }
    })
}

fn search_response(url: &str, score: f64) -> serde_json::Value {
    serde_json::json!({
        "result": [
            {"id": "hit", "score": score, "payload": {"url": url, "chunk_text": "chunk"}}
        ]
    })
}

#[test]
fn collection_name_accepts_legal_values() {
    assert!(validate_collection_name("cortex").is_ok());
    assert!(validate_collection_name("axon_v2").is_ok());
    assert!(validate_collection_name("my-collection").is_ok());
    assert!(validate_collection_name("a.b.c").is_ok());
    assert!(validate_collection_name("a").is_ok());
}

#[test]
fn collection_name_rejects_path_traversal() {
    assert!(validate_collection_name("..").is_err());
    assert!(validate_collection_name("../etc/passwd").is_err());
    assert!(validate_collection_name("a/b").is_err());
    assert!(validate_collection_name("a..b").is_err());
    assert!(validate_collection_name(".hidden").is_err());
    assert!(validate_collection_name("trailing.").is_err());
}

#[test]
fn collection_name_rejects_url_delimiters() {
    assert!(validate_collection_name("a?x=1").is_err());
    assert!(validate_collection_name("a#frag").is_err());
    assert!(validate_collection_name("a b").is_err());
    assert!(validate_collection_name("a%2e%2e").is_err());
}

#[test]
fn collection_name_rejects_empty_and_oversize() {
    assert!(validate_collection_name("").is_err());
    let huge = "a".repeat(256);
    assert!(validate_collection_name(&huge).is_err());
}

#[tokio::test]
async fn dispatch_rejects_invalid_collection_name() {
    let mut cfg = Config::test_default();
    cfg.collection = "../etc/passwd".to_string();
    let vec = vec![0.0f32; 4];
    let err = dispatch_vector_search(&cfg, &vec, "ok", 5)
        .await
        .expect_err("path-traversal collection name must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("invalid collection name"),
        "error should mention invalid collection: {msg}"
    );
}

#[tokio::test]
async fn dispatch_rejects_query_over_max_len() {
    let cfg = Config::test_default();
    let huge = "a".repeat(MAX_QUERY_LEN_BYTES + 1);
    let vec = vec![0.0f32; 4];
    let err = dispatch_vector_search(&cfg, &vec, &huge, 5)
        .await
        .expect_err("query over cap must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("64-byte cap")
            || msg.contains("cap")
            || msg.contains(&MAX_QUERY_LEN_BYTES.to_string()),
        "error should mention the cap: {msg}"
    );
}

#[tokio::test]
async fn dispatch_accepts_query_at_max_len() {
    // We can't actually run the search without a Qdrant mock, but we can confirm
    // the length guard does not trip at the boundary.
    let cfg = Config::test_default();
    let at_cap = "a".repeat(MAX_QUERY_LEN_BYTES);
    let vec = vec![0.0f32; 4];
    let res = dispatch_vector_search(&cfg, &vec, &at_cap, 5).await;
    // Either succeeds (impossible without a mock) or fails for a downstream reason
    // (vector mode probe / network) — but the failure must NOT be the length cap.
    if let Err(e) = res {
        let msg = e.to_string();
        assert!(
            !msg.contains("cap"),
            "boundary-length query must not trip the length cap: {msg}"
        );
    }
}

#[tokio::test]
async fn dispatch_routes_named_with_sparse_to_hybrid() {
    let server = MockServer::start_async().await;
    let collection = "dispatch_named_sparse_hybrid";

    server
        .mock_async(|when, then| {
            when.method(GET).path(format!("/collections/{collection}"));
            then.status(200).json_body(named_collection_body());
        })
        .await;
    let hybrid = server
        .mock_async(|when, then| {
            when.method(POST)
                .path(format!("/collections/{collection}/points/query"))
                .json_body_includes(r#"{"query":{"fusion":"rrf"}}"#);
            then.status(200)
                .json_body(query_response("https://example.com/hybrid", 0.666));
        })
        .await;
    let legacy = server
        .mock_async(|when, then| {
            when.method(POST)
                .path(format!("/collections/{collection}/points/search"));
            then.status(500);
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = collection.to_string();
    cfg.hybrid_search_enabled = true;

    let vector = vec![0.1, 0.2, 0.3, 0.4];
    let request = VectorSearchRequest::from_query(&cfg, &vector, "hybrid retrieval pipeline", 5)
        .expect("request builds");
    let hits = dispatch_vector_search_request(&cfg, &request)
        .await
        .expect("hybrid dispatch succeeds");

    assert_eq!(hits[0].payload.url, "https://example.com/hybrid");
    assert_eq!(hybrid.calls_async().await, 1);
    assert_eq!(legacy.calls_async().await, 0);
}

#[tokio::test]
async fn dispatch_routes_named_empty_sparse_to_named_dense() {
    let server = MockServer::start_async().await;
    let collection = "dispatch_named_empty_sparse";

    server
        .mock_async(|when, then| {
            when.method(GET).path(format!("/collections/{collection}"));
            then.status(200).json_body(named_collection_body());
        })
        .await;
    let named_dense = server
        .mock_async(|when, then| {
            when.method(POST)
                .path(format!("/collections/{collection}/points/query"))
                .json_body_includes(r#"{"using":"dense"}"#);
            then.status(200)
                .json_body(query_response("https://example.com/named-dense", 0.88));
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = collection.to_string();
    cfg.hybrid_search_enabled = true;

    let vector = vec![0.1, 0.2, 0.3, 0.4];
    let request =
        VectorSearchRequest::from_query(&cfg, &vector, "of to in", 5).expect("request builds");
    assert!(request.sparse.as_ref().is_none_or(|sv| sv.is_empty()));

    let hits = dispatch_vector_search_request(&cfg, &request)
        .await
        .expect("named dense dispatch succeeds");

    assert_eq!(hits[0].payload.url, "https://example.com/named-dense");
    assert_eq!(named_dense.calls_async().await, 1);
}

#[tokio::test]
async fn dispatch_routes_unnamed_to_legacy_search() {
    let server = MockServer::start_async().await;
    let collection = "dispatch_unnamed_legacy";

    server
        .mock_async(|when, then| {
            when.method(GET).path(format!("/collections/{collection}"));
            then.status(200).json_body(unnamed_collection_body());
        })
        .await;
    let legacy = server
        .mock_async(|when, then| {
            when.method(POST)
                .path(format!("/collections/{collection}/points/search"));
            then.status(200)
                .json_body(search_response("https://example.com/legacy", 0.77));
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = collection.to_string();
    cfg.hybrid_search_enabled = true;

    let vector = vec![0.1, 0.2, 0.3, 0.4];
    let request = VectorSearchRequest::from_query(&cfg, &vector, "hybrid retrieval pipeline", 5)
        .expect("request builds");
    let hits = dispatch_vector_search_request(&cfg, &request)
        .await
        .expect("legacy dispatch succeeds");

    assert_eq!(hits[0].payload.url, "https://example.com/legacy");
    assert_eq!(legacy.calls_async().await, 1);
}
