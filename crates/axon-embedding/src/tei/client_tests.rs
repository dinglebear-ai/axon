use super::*;
use httpmock::MockServer;
use std::time::{Duration, Instant};

#[tokio::test]
async fn embed_all_packs_similar_lengths_and_restores_input_order() {
    let server = MockServer::start_async().await;
    let short = server
        .mock_async(|when, then| {
            when.method("POST")
                .path("/embed")
                .json_body(serde_json::json!({"inputs": ["a", "cc"], "truncate": false}));
            then.status(200)
                .json_body(serde_json::json!([[1.0_f32], [2.0_f32]]));
        })
        .await;
    let long = server
        .mock_async(|when, then| {
            when.method("POST")
                .path("/embed")
                .json_body(serde_json::json!({"inputs": ["ddd", "bbbb"], "truncate": false}));
            then.status(200)
                .json_body(serde_json::json!([[3.0_f32], [4.0_f32]]));
        })
        .await;
    let client = TeiClient::new(TeiClientParams {
        endpoint: server.base_url(),
        provider_id: "tei".to_string(),
        max_batch_inputs: 2,
        max_concurrent_requests: 1,
        max_in_flight_inputs: 2,
        max_attempts: 1,
        request_timeout: Duration::from_secs(2),
        retry_backoff_base_ms: 1,
    })
    .expect("client");

    let outcome = client
        .embed_all(&["a".into(), "bbbb".into(), "cc".into(), "ddd".into()])
        .await
        .expect("length-aware embed");

    assert_eq!(
        outcome.vectors,
        vec![vec![1.0], vec![4.0], vec![2.0], vec![3.0]],
        "transport packing must not change the provider's input-order contract"
    );
    short.assert_calls_async(1).await;
    long.assert_calls_async(1).await;
}

#[tokio::test]
async fn embed_all_explicitly_disables_truncation_for_long_input() {
    let server = MockServer::start_async().await;
    let long_input = "important documentation ".repeat(700);
    let expected = long_input.clone();
    let endpoint = server
        .mock_async(move |when, then| {
            when.method("POST")
                .path("/embed")
                .json_body(serde_json::json!({"inputs": [expected], "truncate": false}));
            then.status(200)
                .json_body(serde_json::json!([[0.25_f32, 0.75_f32]]));
        })
        .await;
    let client = TeiClient::new(TeiClientParams {
        endpoint: server.base_url(),
        provider_id: "tei".to_string(),
        max_batch_inputs: 1,
        max_concurrent_requests: 1,
        max_in_flight_inputs: 1,
        max_attempts: 1,
        request_timeout: Duration::from_secs(2),
        retry_backoff_base_ms: 1,
    })
    .expect("client");

    let outcome = client
        .embed_all(&[long_input])
        .await
        .expect("lossless embed");
    assert_eq!(outcome.vectors, vec![vec![0.25, 0.75]]);
    endpoint.assert_calls_async(1).await;
}

#[tokio::test]
async fn embed_all_surfaces_single_input_413_without_retrying_or_truncating() {
    let server = MockServer::start_async().await;
    let endpoint = server
        .mock_async(|when, then| {
            when.method("POST")
                .path("/embed")
                .json_body(serde_json::json!({"inputs": ["oversized"], "truncate": false}));
            then.status(413);
        })
        .await;
    let client = TeiClient::new(TeiClientParams {
        endpoint: server.base_url(),
        provider_id: "tei".to_string(),
        max_batch_inputs: 1,
        max_concurrent_requests: 1,
        max_in_flight_inputs: 1,
        max_attempts: 3,
        request_timeout: Duration::from_secs(2),
        retry_backoff_base_ms: 1,
    })
    .expect("client");

    let error = client
        .embed_all(&["oversized".to_string()])
        .await
        .expect_err("a singleton cannot be split without losing content");

    assert_eq!(error.code.0, "embedding.tei.status");
    assert!(error.message.contains("413"));
    assert!(error.provider_cooling().is_none());
    endpoint.assert_calls_async(1).await;
}

#[tokio::test]
async fn embed_all_overlaps_independent_client_batches() {
    let server = MockServer::start_async().await;
    let endpoint = server
        .mock_async(|when, then| {
            when.method("POST").path("/embed");
            then.status(200)
                .delay(Duration::from_secs(2))
                .json_body(serde_json::json!([[0.1_f32, 0.2_f32]]));
        })
        .await;
    let client = TeiClient::new(TeiClientParams {
        endpoint: server.base_url(),
        provider_id: "tei".to_string(),
        max_batch_inputs: 1,
        max_concurrent_requests: 4,
        max_in_flight_inputs: 4,
        max_attempts: 1,
        request_timeout: Duration::from_secs(5),
        retry_backoff_base_ms: 1,
    })
    .expect("client");

    let client = Arc::new(client);
    let task_client = Arc::clone(&client);
    let task = tokio::spawn(async move {
        task_client
            .embed_all(&["a".into(), "b".into(), "c".into(), "d".into()])
            .await
    });

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if endpoint.calls_async().await == 4 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("all four requests should be admitted before the first delayed response completes");

    let outcome = task.await.expect("embed task").expect("embed batches");
    assert_eq!(outcome.vectors.len(), 4);
    assert_eq!(outcome.requests, 4);
    endpoint.assert_calls_async(4).await;
}

#[tokio::test]
async fn embed_all_replenishes_concurrency_before_a_straggler_finishes() {
    let server = MockServer::start_async().await;
    let slow = server
        .mock_async(|when, then| {
            when.method("POST")
                .path("/embed")
                .json_body(serde_json::json!({"inputs": ["a"], "truncate": false}));
            then.status(200)
                .delay(Duration::from_millis(800))
                .json_body(serde_json::json!([[1.0_f32]]));
        })
        .await;
    let fast = server
        .mock_async(|when, then| {
            when.method("POST")
                .path("/embed")
                .json_body(serde_json::json!({"inputs": ["bb"], "truncate": false}));
            then.status(200)
                .delay(Duration::from_millis(20))
                .json_body(serde_json::json!([[2.0_f32]]));
        })
        .await;
    let replenished = server
        .mock_async(|when, then| {
            when.method("POST")
                .path("/embed")
                .json_body(serde_json::json!({"inputs": ["ccc"], "truncate": false}));
            then.status(200)
                .delay(Duration::from_millis(20))
                .json_body(serde_json::json!([[3.0_f32]]));
        })
        .await;
    let client = Arc::new(
        TeiClient::new(TeiClientParams {
            endpoint: server.base_url(),
            provider_id: "tei".to_string(),
            max_batch_inputs: 1,
            max_concurrent_requests: 2,
            max_in_flight_inputs: 2,
            max_attempts: 1,
            request_timeout: Duration::from_secs(2),
            retry_backoff_base_ms: 1,
        })
        .expect("client"),
    );

    let task_client = Arc::clone(&client);
    let task = tokio::spawn(async move {
        task_client
            .embed_all(&["a".into(), "bb".into(), "ccc".into()])
            .await
    });
    let admitted_while_slow_was_running = tokio::time::timeout(Duration::from_millis(300), async {
        loop {
            if replenished.calls_async().await == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .is_ok();

    let outcome = task.await.expect("embed task").expect("embed batches");
    assert!(
        admitted_while_slow_was_running,
        "a completed request must replenish the concurrency window without waiting for a sibling straggler"
    );
    assert_eq!(outcome.vectors, vec![vec![1.0], vec![2.0], vec![3.0]]);
    slow.assert_calls_async(1).await;
    fast.assert_calls_async(1).await;
    replenished.assert_calls_async(1).await;
}

#[test]
fn is_retryable_status_covers_429_and_5xx_only() {
    assert!(is_retryable_status(StatusCode::TOO_MANY_REQUESTS));
    assert!(is_retryable_status(StatusCode::INTERNAL_SERVER_ERROR));
    assert!(is_retryable_status(StatusCode::SERVICE_UNAVAILABLE));
    assert!(is_retryable_status(StatusCode::GATEWAY_TIMEOUT));
    assert!(!is_retryable_status(StatusCode::OK));
    assert!(!is_retryable_status(StatusCode::BAD_REQUEST));
    // 413 drives the batch-split path, not the generic retry path.
    assert!(!is_retryable_status(StatusCode::PAYLOAD_TOO_LARGE));
}

#[test]
fn is_batch_too_large_is_413_only() {
    assert!(is_batch_too_large(StatusCode::PAYLOAD_TOO_LARGE));
    assert!(!is_batch_too_large(StatusCode::UNPROCESSABLE_ENTITY));
    assert!(!is_batch_too_large(StatusCode::OK));
}

#[test]
fn retry_delay_grows_exponentially_and_caps() {
    let now = Instant::now();
    assert!(retry_delay(1, now, 1000).as_millis() >= 1000);
    assert!(retry_delay(2, now, 1000).as_millis() >= 2000);
    assert!(retry_delay(3, now, 1000).as_millis() >= 4000);
    // Capped at 60_000 + <500ms jitter.
    assert!(retry_delay(100, now, 1000).as_millis() <= 60_500);
}

#[test]
fn retry_delay_attempt_zero_does_not_panic() {
    // saturating_sub(1) clamps to 0 → base_ms unchanged, no u32 underflow.
    assert!(retry_delay(0, Instant::now(), 1000).as_millis() >= 1000);
}

#[test]
fn retry_delay_scales_with_configured_base_ms() {
    // Proves `base_ms` (config-driven, was a hardcoded 1000 literal) actually
    // controls the backoff rather than being ignored.
    let now = Instant::now();
    assert!(retry_delay(1, now, 500).as_millis() >= 500);
    assert!(retry_delay(1, now, 500).as_millis() < 1000);
    assert!(retry_delay(2, now, 500).as_millis() >= 1000);
}

#[test]
fn resolve_batch_size_clamps_to_valid_range() {
    // Env var is not set in this test, so config value is used and clamped.
    assert_eq!(resolve_batch_size(64), 64);
    assert_eq!(resolve_batch_size(0), 1);
    assert_eq!(resolve_batch_size(10_000), 4096);
}

#[test]
fn request_concurrency_respects_aggregate_input_budget() {
    assert_eq!(effective_request_concurrency(8, 320, 96), 3);
    assert_eq!(effective_request_concurrency(8, 32, 96), 1);
    assert_eq!(effective_request_concurrency(0, 0, 0), 1);
}

#[test]
fn tei_client_new_reuses_the_shared_client_across_many_constructions() {
    let before = shared_client_build_count();
    for i in 0..5 {
        TeiClient::new(TeiClientParams {
            endpoint: "http://127.0.0.1:1".to_string(),
            provider_id: format!("tei-{i}"),
            max_batch_inputs: 8,
            max_concurrent_requests: 1,
            max_in_flight_inputs: 8,
            max_attempts: 1,
            request_timeout: Duration::from_millis(10),
            retry_backoff_base_ms: 500,
        })
        .expect("client construction performs no I/O");
    }
    let after = shared_client_build_count();
    assert!(
        after == before || after == before + 1,
        "the shared client may initialize once, never once per TeiClient::new call"
    );
    for i in 5..10 {
        TeiClient::new(TeiClientParams {
            endpoint: "http://127.0.0.1:1".to_string(),
            provider_id: format!("tei-{i}"),
            max_batch_inputs: 8,
            max_concurrent_requests: 1,
            max_in_flight_inputs: 8,
            max_attempts: 1,
            request_timeout: Duration::from_millis(10),
            retry_backoff_base_ms: 500,
        })
        .expect("client construction performs no I/O");
    }
    assert_eq!(
        shared_client_build_count(),
        after,
        "later TeiClient::new calls must keep reusing the same client"
    );
}

#[test]
fn exhausted_cooling_attaches_provider_cooling_metadata_and_marks_retryable() {
    let client = TeiClient::new(TeiClientParams {
        endpoint: "http://127.0.0.1:1".to_string(),
        provider_id: "tei".to_string(),
        max_batch_inputs: 8,
        max_concurrent_requests: 1,
        max_in_flight_inputs: 8,
        max_attempts: 1,
        request_timeout: Duration::from_millis(10),
        retry_backoff_base_ms: 500,
    })
    .expect("client construction performs no I/O");

    let before = Utc::now();
    let err = client.status_error(StatusCode::SERVICE_UNAVAILABLE);
    let cooled = client.with_exhausted_cooling(err);

    assert!(
        cooled.retryable,
        "an exhausted retryable error stays retryable"
    );
    let cooling = cooled
        .provider_cooling()
        .expect("retry-exhausted errors must carry ProviderCooling metadata");
    assert_eq!(cooling.provider_id.as_deref(), Some("tei"));
    assert!(cooling.cooldown_until > before);
    assert_eq!(cooled.cooldown_until, Some(cooling.cooldown_until));
}
