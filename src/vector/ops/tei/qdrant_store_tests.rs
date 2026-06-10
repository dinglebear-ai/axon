use super::*;

fn resolve_test_qdrant_url() -> Option<String> {
    let url = std::env::var("AXON_TEST_QDRANT_URL")
        .ok()
        .filter(|v| !v.trim().is_empty());
    if url.is_none() {
        if cfg!(feature = "live-qdrant") {
            panic!("AXON_TEST_QDRANT_URL must be set when running with feature=live-qdrant");
        }
        eprintln!("skipping live Qdrant test: AXON_TEST_QDRANT_URL is not set");
    }
    url
}

// -- detect_vector_mode (pure parsing logic) --

#[test]
fn detect_vector_mode_named_collection() {
    let body = serde_json::json!({
        "result": {"config": {"params": {"vectors": {
            "dense": {"size": 384, "distance": "Cosine"}
        }}}}
    });
    assert_eq!(detect_vector_mode(&body), VectorMode::Named);
}

#[test]
fn detect_vector_mode_unnamed_collection() {
    let body = serde_json::json!({
        "result": {"config": {"params": {"vectors": {"size": 384, "distance": "Cosine"}}}}
    });
    assert_eq!(detect_vector_mode(&body), VectorMode::Unnamed);
}

// -- VectorMode cache --
#[test]
fn cached_vector_mode_returns_none_for_unknown_collection() {
    assert!(cached_vector_mode("test_no_such_collection_xyz_999").is_none());
}

#[test]
fn cache_and_retrieve_named_mode() {
    cache_vector_mode("test_cache_named", VectorMode::Named);
    assert_eq!(
        cached_vector_mode("test_cache_named"),
        Some(VectorMode::Named)
    );
}

#[test]
fn cache_and_retrieve_unnamed_mode() {
    cache_vector_mode("test_cache_unnamed", VectorMode::Unnamed);
    assert_eq!(
        cached_vector_mode("test_cache_unnamed"),
        Some(VectorMode::Unnamed)
    );
}

#[test]
fn endpoint_collection_cache_keys_keep_same_collection_separate() {
    let mut cfg_a = Config::test_default();
    cfg_a.qdrant_url = "http://127.0.0.1:6333".to_string();
    cfg_a.collection = "shared_collection_name".to_string();
    let mut cfg_b = cfg_a.clone();
    cfg_b.qdrant_url = "http://127.0.0.1:7333/".to_string();

    let key_a = collection_mode_cache_key(&cfg_a);
    let key_b = collection_mode_cache_key(&cfg_b);
    assert_ne!(key_a, key_b);

    cache_vector_mode_key(&key_a, VectorMode::Named);
    cache_vector_mode_key(&key_b, VectorMode::Unnamed);

    assert_eq!(cached_vector_mode_key(&key_a), Some(VectorMode::Named));
    assert_eq!(cached_vector_mode_key(&key_b), Some(VectorMode::Unnamed));
}

#[test]
fn cached_unnamed_mode_is_not_authoritative_when_hybrid_enabled() {
    let mut cfg = Config::test_default();
    cfg.hybrid_search_enabled = true;

    assert!(!cached_vector_mode_is_authoritative(
        &cfg,
        "test_auth_key",
        VectorMode::Unnamed
    ));
    assert!(cached_vector_mode_is_authoritative(
        &cfg,
        "test_auth_key",
        VectorMode::Named
    ));

    cfg.hybrid_search_enabled = false;
    assert!(cached_vector_mode_is_authoritative(
        &cfg,
        "test_auth_key",
        VectorMode::Unnamed
    ));
}

// -- clear_collection_mode_cache --

#[test]
fn clear_cache_allows_mode_re_detection() {
    let name = "test_clear_cache_redetect";
    cache_vector_mode(name, VectorMode::Unnamed);
    assert_eq!(cached_vector_mode(name), Some(VectorMode::Unnamed));

    clear_collection_mode_cache(name);

    assert!(
        cached_vector_mode(name).is_none(),
        "cache entry must be removed after clear_collection_mode_cache"
    );
}

#[test]
fn clear_cache_noop_for_absent_entry() {
    // Clearing a non-existent entry must not panic or corrupt other entries.
    clear_collection_mode_cache("test_clear_cache_never_inserted");
    assert!(cached_vector_mode("test_clear_cache_never_inserted").is_none());
}

// -- validate_existing_dim --

#[test]
fn validate_dim_match_unnamed() {
    let b = serde_json::json!({"result":{"config":{"params":{"vectors":{"size":384}}}}});
    assert!(validate_existing_dim(&b, VectorMode::Unnamed, 384, "c").is_ok());
}

#[test]
fn validate_dim_match_named() {
    let b = serde_json::json!({"result":{"config":{"params":{"vectors":{"dense":{"size":384}}}}}});
    assert!(validate_existing_dim(&b, VectorMode::Named, 384, "c").is_ok());
}

#[test]
fn validate_dim_mismatch_unnamed() {
    let b = serde_json::json!({"result":{"config":{"params":{"vectors":{"size":768}}}}});
    let msg = validate_existing_dim(&b, VectorMode::Unnamed, 384, "col")
        .unwrap_err()
        .to_string();
    assert!(msg.contains("dim=768") && msg.contains("dim=384") && msg.contains("col"));
}

#[test]
fn validate_dim_mismatch_named() {
    let b = serde_json::json!({"result":{"config":{"params":{"vectors":{"dense":{"size":1024}}}}}});
    let msg = validate_existing_dim(&b, VectorMode::Named, 384, "my_col")
        .unwrap_err()
        .to_string();
    assert!(msg.contains("my_col") && msg.contains("dim=1024") && msg.contains("dim=384"));
    assert!(msg.contains("AXON_COLLECTION"));
}

#[test]
fn validate_dim_missing_is_ok() {
    let b = serde_json::json!({"result":{"config":{"params":{}}}});
    assert!(validate_existing_dim(&b, VectorMode::Unnamed, 384, "c").is_ok());
}

// -- ensure_collection (integration -- requires live Qdrant) --

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "integration test -- requires running Qdrant; run with cargo test -- --ignored"]
async fn ensure_collection_new_collection_returns_named_mode() -> Result<(), Box<dyn Error>> {
    let Some(qdrant_url) = resolve_test_qdrant_url() else {
        return Ok(());
    };
    let mut cfg = Config::test_default();
    cfg.qdrant_url = qdrant_url.clone();
    cfg.collection = format!("test_{}", uuid::Uuid::new_v4().simple());

    let mode = ensure_collection(&cfg, 4).await?;

    let _ = reqwest::Client::new()
        .delete(format!(
            "{}/collections/{}",
            qdrant_url.trim_end_matches('/'),
            cfg.collection
        ))
        .send()
        .await;

    assert_eq!(mode, VectorMode::Named, "new collection must be Named");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "integration test -- requires running Qdrant; run with cargo test -- --ignored"]
async fn ensure_collection_existing_unnamed_returns_unnamed_mode() -> Result<(), Box<dyn Error>> {
    let Some(qdrant_url) = resolve_test_qdrant_url() else {
        return Ok(());
    };
    let client = reqwest::Client::new();
    let base = qdrant_url.trim_end_matches('/').to_string();
    let collection = format!("test_{}", uuid::Uuid::new_v4().simple());

    client
        .put(format!("{base}/collections/{collection}"))
        .json(&serde_json::json!({"vectors": {"size": 4, "distance": "Cosine"}}))
        .send()
        .await?
        .error_for_status()?;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = qdrant_url;
    cfg.collection = collection.clone();

    let mode = ensure_collection(&cfg, 4).await?;

    let _ = client
        .delete(format!("{base}/collections/{collection}"))
        .send()
        .await;

    assert_eq!(
        mode,
        VectorMode::Unnamed,
        "existing unnamed must return Unnamed"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "integration test -- requires running Qdrant; run with cargo test -- --ignored"]
async fn ensure_collection_is_idempotent() -> Result<(), Box<dyn Error>> {
    let Some(qdrant_url) = resolve_test_qdrant_url() else {
        return Ok(());
    };
    let mut cfg = Config::test_default();
    cfg.qdrant_url = qdrant_url;
    cfg.collection = format!("test_{}", uuid::Uuid::new_v4().simple());

    ensure_collection(&cfg, 4).await?;
    ensure_collection(&cfg, 4).await?;

    let base = cfg.qdrant_url.trim_end_matches('/');
    let _ = reqwest::Client::new()
        .delete(format!("{}/collections/{}", base, cfg.collection))
        .send()
        .await;
    Ok(())
}

// -- get_or_fetch_vector_mode: probe failures must return Err and must NOT be cached --

#[tokio::test]
async fn get_or_fetch_mode_auth_failure_is_not_cached() {
    use httpmock::MockServer;

    let server = MockServer::start_async().await;

    server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET)
                .path_matches(regex::Regex::new("/collections/").unwrap());
            then.status(401);
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = "auth_test_col_401".to_string();

    let err = get_or_fetch_vector_mode(&cfg)
        .await
        .expect_err("401 mode probe must return Err");
    assert!(
        err.to_string().contains("returned 401"),
        "error should mention status code, got: {err}"
    );

    // Cache must NOT contain an entry for this collection (401 = don't cache)
    assert!(
        cached_vector_mode("auth_test_col_401").is_none(),
        "401 auth failure must not be cached permanently"
    );
}

#[tokio::test]
async fn get_or_fetch_mode_403_is_not_cached() {
    use httpmock::MockServer;

    let server = MockServer::start_async().await;

    server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET)
                .path_matches(regex::Regex::new("/collections/").unwrap());
            then.status(403);
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = "auth_test_col_403".to_string();

    let err = get_or_fetch_vector_mode(&cfg)
        .await
        .expect_err("403 mode probe must return Err");
    assert!(
        err.to_string().contains("returned 403"),
        "error should mention status code, got: {err}"
    );

    assert!(
        cached_vector_mode("auth_test_col_403").is_none(),
        "403 auth failure must not be cached permanently"
    );
}

#[tokio::test]
async fn get_or_fetch_mode_500_is_not_cached() {
    use httpmock::MockServer;

    let server = MockServer::start_async().await;

    server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET)
                .path_matches(regex::Regex::new("/collections/").unwrap());
            then.status(500);
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = "auth_test_col_500".to_string();

    let err = get_or_fetch_vector_mode(&cfg)
        .await
        .expect_err("500 mode probe must return Err");
    assert!(
        err.to_string().contains("returned 500"),
        "error should mention status code, got: {err}"
    );

    // 500 must NOT be cached -- transient server errors should not permanently
    // downgrade the collection mode for the entire process lifetime.
    assert!(
        cached_vector_mode("auth_test_col_500").is_none(),
        "500 server error must not be cached (transient failure)"
    );
}

#[tokio::test]
async fn get_or_fetch_mode_404_returns_error_and_is_not_cached() {
    use httpmock::MockServer;

    let server = MockServer::start_async().await;

    let mock = server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET)
                .path_matches(regex::Regex::new("/collections/").unwrap());
            then.status(404);
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = "auth_test_col_404".to_string();

    let err = get_or_fetch_vector_mode(&cfg)
        .await
        .expect_err("404 mode probe must return Err");
    let msg = err.to_string();
    assert!(
        msg.contains("returned 404") && msg.contains("collection not found"),
        "404 error should include not-found context, got: {msg}"
    );
    assert_eq!(
        mock.calls_async().await,
        1,
        "404 should fail immediately without retries"
    );
    assert!(
        cached_vector_mode("auth_test_col_404").is_none(),
        "404 mode probe failure must not be cached"
    );
}

#[tokio::test]
async fn get_or_fetch_mode_500_retries_three_times() {
    use httpmock::MockServer;

    let server = MockServer::start_async().await;

    let mock = server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET)
                .path_matches(regex::Regex::new("/collections/").unwrap());
            then.status(500);
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = "retry_test_col_500".to_string();

    let err = get_or_fetch_vector_mode(&cfg)
        .await
        .expect_err("500 mode probe must return Err after retries");
    let msg = err.to_string();
    assert!(
        msg.contains("returned 500"),
        "error should include final 500 status, got: {msg}"
    );
    assert_eq!(
        mock.calls_async().await,
        3,
        "500 status should trigger exactly 3 probe attempts"
    );
}

#[tokio::test]
async fn get_or_fetch_mode_429_retries_three_times() {
    use httpmock::MockServer;

    let server = MockServer::start_async().await;

    let mock = server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET)
                .path_matches(regex::Regex::new("/collections/").unwrap());
            then.status(429);
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = "retry_test_col_429".to_string();

    let err = get_or_fetch_vector_mode(&cfg)
        .await
        .expect_err("429 mode probe must return Err after retries");
    let msg = err.to_string();
    assert!(
        msg.contains("returned 429"),
        "error should include final 429 status, got: {msg}"
    );
    assert_eq!(
        mock.calls_async().await,
        3,
        "429 status should trigger exactly 3 probe attempts"
    );
}

#[tokio::test]
async fn get_or_fetch_revalidates_cached_unnamed_when_hybrid_enabled() {
    use httpmock::prelude::*;

    let server = MockServer::start_async().await;
    let collection = "revalidate_cached_unnamed_query";

    let get_mock = server
        .mock_async(|when, then| {
            when.method(GET).path(format!("/collections/{collection}"));
            then.status(200).json_body(serde_json::json!({
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
            }));
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = collection.to_string();
    cfg.hybrid_search_enabled = true;

    let key = collection_mode_cache_key(&cfg);
    cache_vector_mode_key(&key, VectorMode::Unnamed);

    let mode = get_or_fetch_vector_mode(&cfg)
        .await
        .expect("cached unnamed mode must be re-probed successfully");

    assert_eq!(mode, VectorMode::Named);
    assert_eq!(cached_vector_mode_key(&key), Some(VectorMode::Named));
    assert_eq!(
        get_mock.calls_async().await,
        1,
        "cached Unnamed must trigger a live schema probe under hybrid mode"
    );
}

#[tokio::test]
async fn collection_init_revalidates_cached_unnamed_when_hybrid_enabled() {
    use httpmock::prelude::*;

    let server = MockServer::start_async().await;
    let collection = "revalidate_cached_unnamed_embed";

    let get_mock = server
        .mock_async(|when, then| {
            when.method(GET).path(format!("/collections/{collection}"));
            then.status(200).json_body(serde_json::json!({
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
            }));
        })
        .await;

    server
        .mock_async(|when, then| {
            when.method(PUT).path_matches(
                regex::Regex::new(&format!("/collections/{collection}/index")).unwrap(),
            );
            then.status(200)
                .json_body(serde_json::json!({"result": true, "status": "ok", "time": 0.0}));
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = collection.to_string();
    cfg.hybrid_search_enabled = true;

    let key = collection_mode_cache_key(&cfg);
    cache_vector_mode_key(&key, VectorMode::Unnamed);

    let mode = collection_init_or_cached(&cfg, 4)
        .await
        .expect("cached unnamed mode must be re-probed successfully");

    assert_eq!(mode, VectorMode::Named);
    assert_eq!(cached_vector_mode_key(&key), Some(VectorMode::Named));
    assert_eq!(
        get_mock.calls_async().await,
        1,
        "cached Unnamed must trigger live ensure_collection under hybrid mode"
    );
}

// -- Fix 5: ensure_collection sends hnsw_config on create --

#[tokio::test]
async fn ensure_collection_sends_hnsw_config_on_create() {
    use httpmock::prelude::*;

    let server = MockServer::start_async().await;

    // GET → 404 (collection does not exist) triggers the creation path.
    server
        .mock_async(|when, then| {
            when.method(GET).path("/collections/hnsw_test_col");
            then.status(404);
        })
        .await;

    // PUT → expect hnsw_config in the body; respond 200 (success).
    let put_mock = server
        .mock_async(|when, then| {
            when.method(PUT)
                .path("/collections/hnsw_test_col")
                .json_body_includes(r#"{"hnsw_config":{"m":32,"ef_construct":256}}"#);
            then.status(200)
                .json_body(serde_json::json!({"result": true, "status": "ok", "time": 0.0}));
        })
        .await;

    // PUT → payload indexes (idempotent, any body accepted).
    server
        .mock_async(|when, then| {
            when.method(PUT)
                .path_matches(regex::Regex::new("/collections/hnsw_test_col/index").unwrap());
            then.status(200)
                .json_body(serde_json::json!({"result": true, "status": "ok", "time": 0.0}));
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = "hnsw_test_col".to_string();

    let result = ensure_collection(&cfg, 4).await;

    put_mock.assert_async().await;
    assert!(
        result.is_ok(),
        "ensure_collection must succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn ensure_collection_does_not_put_on_existing_named_collection() {
    use httpmock::prelude::*;

    let server = MockServer::start_async().await;

    // GET → 200 with a Named-mode collection body — triggers the early-return path.
    server
        .mock_async(|when, then| {
            when.method(GET).path("/collections/existing_named_col");
            then.status(200).json_body(serde_json::json!({
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
            }));
        })
        .await;

    // PUT → payload indexes (idempotent). Accept any body.
    server
        .mock_async(|when, then| {
            when.method(PUT)
                .path_matches(regex::Regex::new("/collections/existing_named_col/index").unwrap());
            then.status(200)
                .json_body(serde_json::json!({"result": true, "status": "ok", "time": 0.0}));
        })
        .await;

    // Explicitly reject any PUT to the collection URL itself (creation must NOT fire).
    let unexpected_create = server
        .mock_async(|when, then| {
            when.method(PUT).path("/collections/existing_named_col");
            then.status(200);
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = "existing_named_col".to_string();

    let result = ensure_collection(&cfg, 4).await;

    assert!(
        result.is_ok(),
        "must succeed on existing collection: {:?}",
        result.err()
    );
    assert_eq!(
        unexpected_create.calls_async().await,
        0,
        "collection PUT must NOT be called for an existing Named collection"
    );
}

// -- Fix 6: ensure_collection sends quantization_config on create --

#[tokio::test]
async fn ensure_collection_sends_quantization_config_on_create() {
    use httpmock::prelude::*;

    let server = MockServer::start_async().await;

    server
        .mock_async(|when, then| {
            when.method(GET).path("/collections/quant_test_col");
            then.status(404);
        })
        .await;

    let put_mock = server
        .mock_async(|when, then| {
            when.method(PUT)
                .path("/collections/quant_test_col")
                .json_body_includes(r#"{"quantization_config":{"scalar":{"type":"int8","quantile":0.99,"always_ram":true}}}"#);
            then.status(200)
                .json_body(serde_json::json!({"result": true, "status": "ok", "time": 0.0}));
        })
        .await;

    server
        .mock_async(|when, then| {
            when.method(PUT)
                .path_matches(regex::Regex::new("/collections/quant_test_col/index").unwrap());
            then.status(200)
                .json_body(serde_json::json!({"result": true, "status": "ok", "time": 0.0}));
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = "quant_test_col".to_string();

    let result = ensure_collection(&cfg, 4).await;

    put_mock.assert_async().await;
    assert!(
        result.is_ok(),
        "ensure_collection must succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn ensure_collection_sends_full_create_body_with_hnsw_and_quantization() {
    use httpmock::prelude::*;

    let server = MockServer::start_async().await;

    server
        .mock_async(|when, then| {
            when.method(GET).path("/collections/full_body_col");
            then.status(404);
        })
        .await;

    // Assert that both hnsw_config AND quantization_config appear in the same PUT body.
    let put_mock = server
        .mock_async(|when, then| {
            when.method(PUT)
                .path("/collections/full_body_col")
                .json_body_includes(r#"{"hnsw_config":{"m":32,"ef_construct":256}}"#)
                .json_body_includes(r#"{"quantization_config":{"scalar":{"type":"int8","quantile":0.99,"always_ram":true}}}"#);
            then.status(200)
                .json_body(serde_json::json!({"result": true, "status": "ok", "time": 0.0}));
        })
        .await;

    server
        .mock_async(|when, then| {
            when.method(PUT)
                .path_matches(regex::Regex::new("/collections/full_body_col/index").unwrap());
            then.status(200)
                .json_body(serde_json::json!({"result": true, "status": "ok", "time": 0.0}));
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.qdrant_url = server.base_url();
    cfg.collection = "full_body_col".to_string();

    let result = ensure_collection(&cfg, 4).await;

    put_mock.assert_async().await;
    assert!(
        result.is_ok(),
        "full body test must succeed: {:?}",
        result.err()
    );
}
