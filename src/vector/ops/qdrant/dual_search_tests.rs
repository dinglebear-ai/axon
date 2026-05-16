use super::*;
use crate::core::config::Config;
use crate::vector::ops::sparse::compute_sparse_vector;
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

fn batch_response(arms: &[Vec<(&str, f64)>]) -> serde_json::Value {
    let result: Vec<serde_json::Value> = arms
        .iter()
        .map(|arm| {
            let points: Vec<serde_json::Value> = arm
                .iter()
                .map(|(url, score)| {
                    serde_json::json!({
                        "id": "test-id",
                        "score": score,
                        "payload": {"url": url, "chunk_text": "chunk"}
                    })
                })
                .collect();
            serde_json::json!({"points": points})
        })
        .collect();
    serde_json::json!({"result": result})
}

#[tokio::test]
async fn qdrant_dual_search_named_hybrid_returns_two_result_arrays_in_order() {
    let server = MockServer::start_async().await;
    let collection = "dual_named_hybrid";

    server
        .mock_async(|when, then| {
            when.method(GET).path(format!("/collections/{collection}"));
            then.status(200).json_body(named_collection_body());
        })
        .await;
    let batch = server
        .mock_async(|when, then| {
            when.method(POST)
                .path(format!("/collections/{collection}/points/query/batch"))
                .json_body_includes(r#"{"searches":[{"query":{"fusion":"rrf"}}]}"#);
            then.status(200).json_body(batch_response(&[
                vec![("https://example.com/primary", 0.9)],
                vec![("https://example.com/secondary", 0.8)],
            ]));
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = collection.to_string();
    cfg.hybrid_search_enabled = true;

    let dense_p = vec![0.1f32, 0.2, 0.3, 0.4];
    let dense_s = vec![0.5f32, 0.6, 0.7, 0.8];
    let sparse_p = compute_sparse_vector("primary natural language");
    let sparse_s = compute_sparse_vector("primary keywords");

    let res = qdrant_dual_search(
        &cfg,
        DualSearchArm {
            dense: &dense_p,
            sparse: &sparse_p,
            filter: None,
        },
        DualSearchArm {
            dense: &dense_s,
            sparse: &sparse_s,
            filter: None,
        },
        5,
        None,
    )
    .await
    .expect("dual search succeeds");

    batch.assert_async().await;
    assert_eq!(res.primary.len(), 1);
    assert_eq!(res.secondary.len(), 1);
    assert_eq!(res.primary[0].payload.url, "https://example.com/primary");
    assert_eq!(
        res.secondary[0].payload.url,
        "https://example.com/secondary"
    );
}

#[tokio::test]
async fn qdrant_dual_search_named_dense_only_when_sparse_empty() {
    let server = MockServer::start_async().await;
    let collection = "dual_named_dense_only";

    server
        .mock_async(|when, then| {
            when.method(GET).path(format!("/collections/{collection}"));
            then.status(200).json_body(named_collection_body());
        })
        .await;
    let batch = server
        .mock_async(|when, then| {
            // Both arms should fall back to dense-only with `using: "dense"`
            // and no `prefetch` block when sparse is empty.
            when.method(POST)
                .path(format!("/collections/{collection}/points/query/batch"))
                .json_body_includes(r#"{"searches":[{"using":"dense"}]}"#);
            then.status(200).json_body(batch_response(&[
                vec![("https://example.com/p", 0.7)],
                vec![("https://example.com/s", 0.6)],
            ]));
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = collection.to_string();
    cfg.hybrid_search_enabled = true;

    let dense = vec![0.1f32, 0.2, 0.3, 0.4];
    let empty = SparseVector::default();

    let res = qdrant_dual_search(
        &cfg,
        DualSearchArm {
            dense: &dense,
            sparse: &empty,
            filter: None,
        },
        DualSearchArm {
            dense: &dense,
            sparse: &empty,
            filter: None,
        },
        5,
        None,
    )
    .await
    .expect("dual search succeeds with empty sparse");

    batch.assert_async().await;
    assert_eq!(res.primary.len(), 1);
    assert_eq!(res.secondary.len(), 1);
}

#[tokio::test]
async fn qdrant_dual_search_returns_err_on_batch_5xx() {
    let server = MockServer::start_async().await;
    let collection = "dual_batch_5xx";

    server
        .mock_async(|when, then| {
            when.method(GET).path(format!("/collections/{collection}"));
            then.status(200).json_body(named_collection_body());
        })
        .await;
    // Persistent 5xx: qdrant_post_json_with_retry will retry then surface Err.
    server
        .mock_async(|when, then| {
            when.method(POST)
                .path(format!("/collections/{collection}/points/query/batch"));
            then.status(500).body("internal server error");
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = collection.to_string();
    cfg.hybrid_search_enabled = true;

    let dense = vec![0.1f32, 0.2, 0.3, 0.4];
    let sparse = compute_sparse_vector("anything");
    let res = qdrant_dual_search(
        &cfg,
        DualSearchArm {
            dense: &dense,
            sparse: &sparse,
            filter: None,
        },
        DualSearchArm {
            dense: &dense,
            sparse: &sparse,
            filter: None,
        },
        5,
        None,
    )
    .await;

    assert!(
        res.is_err(),
        "persistent 5xx must surface as Err so caller can fall back"
    );
}

#[tokio::test]
async fn qdrant_dual_search_unnamed_mode_returns_explicit_unsupported_error() {
    let server = MockServer::start_async().await;
    let collection = "dual_unnamed_unsupported";

    server
        .mock_async(|when, then| {
            when.method(GET).path(format!("/collections/{collection}"));
            then.status(200).json_body(unnamed_collection_body());
        })
        .await;
    // No batch mock: the Unnamed-mode guard must fire BEFORE any HTTP call.

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = collection.to_string();
    cfg.hybrid_search_enabled = true;

    let dense = vec![0.1f32, 0.2, 0.3, 0.4];
    let sparse = compute_sparse_vector("doesn't matter");
    let err = qdrant_dual_search(
        &cfg,
        DualSearchArm {
            dense: &dense,
            sparse: &sparse,
            filter: None,
        },
        DualSearchArm {
            dense: &dense,
            sparse: &sparse,
            filter: None,
        },
        5,
        None,
    )
    .await
    .expect_err("unnamed-mode must be rejected");

    let msg = err.to_string();
    assert!(
        msg.contains("unnamed-mode") || msg.contains("not supported"),
        "error must explain the unsupported mode for retrieval fallback: {msg}"
    );
}
