use super::*;

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
    use crate::crates::jobs::common::resolve_test_qdrant_url;
    let Some(qdrant_url) = resolve_test_qdrant_url() else {
        return Ok(());
    };
    let mut cfg = crate::crates::jobs::common::test_config("postgresql://dummy@127.0.0.1:1/dummy");
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
    use crate::crates::jobs::common::resolve_test_qdrant_url;
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

    let mut cfg = crate::crates::jobs::common::test_config("postgresql://dummy@127.0.0.1:1/dummy");
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
    use crate::crates::jobs::common::resolve_test_qdrant_url;
    let Some(qdrant_url) = resolve_test_qdrant_url() else {
        return Ok(());
    };
    let mut cfg = crate::crates::jobs::common::test_config("postgresql://dummy@127.0.0.1:1/dummy");
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

// -- get_or_fetch_vector_mode: 401/403 must NOT be cached --

#[tokio::test]
async fn get_or_fetch_mode_auth_failure_is_not_cached() {
    use crate::crates::jobs::common::test_config;
    use httpmock::MockServer;

    let server = MockServer::start_async().await;

    server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET)
                .path_matches(regex::Regex::new("/collections/").unwrap());
            then.status(401);
        })
        .await;

    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = server.base_url();
    cfg.collection = "auth_test_col_401".to_string();

    // Should degrade to Unnamed (not error) despite 401
    let mode = get_or_fetch_vector_mode(&cfg).await.unwrap();
    assert_eq!(mode, VectorMode::Unnamed);

    // Cache must NOT contain an entry for this collection (401 = don't cache)
    assert!(
        cached_vector_mode("auth_test_col_401").is_none(),
        "401 auth failure must not be cached permanently"
    );
}

#[tokio::test]
async fn get_or_fetch_mode_403_is_not_cached() {
    use crate::crates::jobs::common::test_config;
    use httpmock::MockServer;

    let server = MockServer::start_async().await;

    server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET)
                .path_matches(regex::Regex::new("/collections/").unwrap());
            then.status(403);
        })
        .await;

    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = server.base_url();
    cfg.collection = "auth_test_col_403".to_string();

    let mode = get_or_fetch_vector_mode(&cfg).await.unwrap();
    assert_eq!(mode, VectorMode::Unnamed);

    assert!(
        cached_vector_mode("auth_test_col_403").is_none(),
        "403 auth failure must not be cached permanently"
    );
}

#[tokio::test]
async fn get_or_fetch_mode_500_is_not_cached() {
    use crate::crates::jobs::common::test_config;
    use httpmock::MockServer;

    let server = MockServer::start_async().await;

    server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET)
                .path_matches(regex::Regex::new("/collections/").unwrap());
            then.status(500);
        })
        .await;

    let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
    cfg.qdrant_url = server.base_url();
    cfg.collection = "auth_test_col_500".to_string();

    let mode = get_or_fetch_vector_mode(&cfg).await.unwrap();
    assert_eq!(mode, VectorMode::Unnamed);

    // 500 must NOT be cached -- transient server errors should not permanently
    // downgrade the collection mode for the entire process lifetime.
    assert!(
        cached_vector_mode("auth_test_col_500").is_none(),
        "500 server error must not be cached (transient failure)"
    );
}
