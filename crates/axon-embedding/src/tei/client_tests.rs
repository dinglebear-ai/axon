use super::*;
use httpmock::MockServer;
use std::time::{Duration, Instant};

#[test]
fn batch_plan_contains_only_indices_into_caller_owned_inputs() {
    let inputs = vec!["longer".to_string(), "x".to_string(), "mid".to_string()];
    let batches = pack_batches(
        &inputs,
        BatchLimits {
            max_inputs: 2,
            max_input_tokens: 8192,
            max_batch_tokens: 65536,
            max_batch_bytes: MAX_BATCH_BYTES,
        },
    )
    .expect("valid batch plan");
    assert_eq!(
        batches
            .iter()
            .map(|(indices, _)| indices.clone())
            .collect::<Vec<_>>(),
        vec![vec![1, 2], vec![0]]
    );
    for (indices, texts) in batches {
        for (index, text) in indices.into_iter().zip(texts) {
            assert_eq!(
                text.as_ptr(),
                inputs[index].as_ptr(),
                "batch text must remain borrowed"
            );
        }
    }
}

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
        max_input_tokens: 8_192,
        max_batch_tokens: 131_072,
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
        max_input_tokens: 32_768,
        max_batch_tokens: 131_072,
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
        max_input_tokens: 8_192,
        max_batch_tokens: 131_072,
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
        max_input_tokens: 8_192,
        max_batch_tokens: 131_072,
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
async fn embed_all_sends_a_long_singleton_for_provider_tokenization() {
    let server = MockServer::start_async().await;
    let endpoint = server
        .mock_async(|when, then| {
            when.method("POST").path("/embed");
            then.status(200).json_body(serde_json::json!([[1.0_f32]]));
        })
        .await;
    let client = TeiClient::new(TeiClientParams {
        endpoint: server.base_url(),
        provider_id: "tei".to_string(),
        max_batch_inputs: 8,
        max_input_tokens: 1,
        max_batch_tokens: 8,
        max_concurrent_requests: 1,
        max_in_flight_inputs: 8,
        max_attempts: 1,
        request_timeout: Duration::from_secs(2),
        retry_backoff_base_ms: 1,
    })
    .expect("client");

    let outcome = client
        .embed_all(&["12345".to_string()])
        .await
        .expect("a conservative estimate must not reject a valid singleton");
    assert_eq!(outcome.vectors.len(), 1);
    endpoint.assert_calls_async(1).await;
}

#[tokio::test]
async fn embed_all_splits_batches_at_the_configured_token_boundary() {
    let server = MockServer::start_async().await;
    let endpoint = server
        .mock_async(|when, then| {
            when.method("POST").path("/embed");
            then.status(200).json_body(serde_json::json!([[1.0_f32]]));
        })
        .await;
    let client = TeiClient::new(TeiClientParams {
        endpoint: server.base_url(),
        provider_id: "tei".to_string(),
        max_batch_inputs: 8,
        max_input_tokens: 8,
        max_batch_tokens: 1,
        max_concurrent_requests: 2,
        max_in_flight_inputs: 8,
        max_attempts: 1,
        request_timeout: Duration::from_secs(2),
        retry_backoff_base_ms: 1,
    })
    .expect("client");

    let outcome = client
        .embed_all(&["aaaa".to_string(), "bbbb".to_string()])
        .await
        .expect("two one-token inputs should be packed separately");
    assert_eq!(outcome.vectors.len(), 2);
    assert_eq!(outcome.requests, 2);
    endpoint.assert_calls_async(2).await;
}

#[test]
fn token_estimate_batches_ascii_efficiently_and_non_ascii_conservatively() {
    assert_eq!(estimated_tokens("abcdefgh"), 4);
    assert_eq!(estimated_tokens("fn main()"), 5);
    assert_eq!(estimated_tokens("漢字"), 6);
    assert_eq!(estimated_tokens("😀"), 4);

    let inputs = ["abcdefgh".into(), "ijklmnop".into(), "漢字".into()];
    let batches = pack_batches(
        &inputs,
        BatchLimits {
            max_inputs: 8,
            max_input_tokens: 32,
            max_batch_tokens: 8,
            max_batch_bytes: MAX_BATCH_BYTES,
        },
    )
    .expect("representative inputs fit the payload ceiling");

    assert_eq!(
        batches
            .iter()
            .map(|(_, texts)| texts.clone())
            .collect::<Vec<_>>(),
        vec![vec!["abcdefgh", "ijklmnop"], vec!["漢字"]]
    );
}

#[test]
fn batch_packer_isolates_oversize_input_from_following_normal_input() {
    let inputs = ["oversized".into(), "ok".into()];
    let batches = pack_batches(
        &inputs,
        BatchLimits {
            max_inputs: 8,
            max_input_tokens: 2,
            max_batch_tokens: 64,
            max_batch_bytes: MAX_BATCH_BYTES,
        },
    )
    .expect("provider-authoritative token overflow is not a client error");

    assert_eq!(batches.len(), 2);
    assert!(batches.iter().all(|(_, texts)| texts.len() == 1));
    assert!(
        batches
            .iter()
            .any(|(_, texts)| texts == &["oversized".to_string()])
    );
    assert!(
        batches
            .iter()
            .any(|(_, texts)| texts == &["ok".to_string()])
    );
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
            max_input_tokens: 8192,
            max_batch_tokens: 65536,
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

#[tokio::test]
async fn embed_all_preserves_order_when_a_concurrent_batch_splits_after_413() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method("POST")
                .path("/embed")
                .json_body(serde_json::json!({
                    "inputs": ["a", "bb", "ccc", "dddd"], "truncate": false
                }));
            then.status(413);
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method("POST")
                .path("/embed")
                .json_body(serde_json::json!({
                    "inputs": ["a", "bb"], "truncate": false
                }));
            then.status(200)
                .delay(Duration::from_millis(100))
                .json_body(serde_json::json!([[1.0_f32], [2.0_f32]]));
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method("POST")
                .path("/embed")
                .json_body(serde_json::json!({
                    "inputs": ["ccc", "dddd"], "truncate": false
                }));
            then.status(200)
                .json_body(serde_json::json!([[3.0_f32], [4.0_f32]]));
        })
        .await;
    let client = TeiClient::new(TeiClientParams {
        endpoint: server.base_url(),
        provider_id: "tei".into(),
        max_batch_inputs: 4,
        max_input_tokens: 8192,
        max_batch_tokens: 65536,
        max_concurrent_requests: 2,
        max_in_flight_inputs: 8,
        max_attempts: 1,
        request_timeout: Duration::from_secs(2),
        retry_backoff_base_ms: 1,
    })
    .expect("client");

    let outcome = client
        .embed_all(&["a".into(), "bb".into(), "ccc".into(), "dddd".into()])
        .await
        .expect("split batch");

    assert_eq!(
        outcome.vectors,
        vec![vec![1.0], vec![2.0], vec![3.0], vec![4.0]]
    );
    assert_eq!(outcome.requests, 3);
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
    assert_eq!(resolve_batch_size(64), 64);
    assert_eq!(resolve_batch_size(0), 1);
    assert_eq!(resolve_batch_size(10_000), 4096);
}

#[test]
fn retry_after_delta_seconds_is_honored() {
    let value = reqwest::header::HeaderValue::from_static("7");
    assert_eq!(parse_retry_after(&value), Some(Duration::from_secs(7)));
    assert_eq!(
        parse_retry_after(&reqwest::header::HeaderValue::from_static("later")),
        None
    );
}

#[test]
fn invalid_tei_endpoint_is_rejected_without_echoing_it() {
    let mut params = TeiClientParams {
        endpoint: "not a url".to_string(),
        provider_id: "tei".to_string(),
        max_batch_inputs: 8,
        max_input_tokens: 8_192,
        max_batch_tokens: 131_072,
        max_concurrent_requests: 1,
        max_in_flight_inputs: 8,
        max_attempts: 1,
        request_timeout: Duration::from_millis(10),
        retry_backoff_base_ms: 1,
    };
    let error = TeiClient::new(params.clone()).expect_err("invalid endpoint");
    assert_eq!(error.code.0, "embedding.tei.invalid_endpoint");
    assert!(!error.to_string().contains("not a url"));
    params.endpoint = "mailto:tei@example.test".to_string();
    assert_eq!(
        TeiClient::new(params)
            .expect_err("unsupported scheme")
            .code
            .0,
        "embedding.tei.invalid_endpoint"
    );
}

#[test]
fn tei_credentials_require_tls_except_on_loopback() {
    assert!(!credential_transport_is_safe(
        &url::Url::parse("http://tei.internal:80").unwrap(),
        true
    ));
    assert!(credential_transport_is_safe(
        &url::Url::parse("https://tei.internal").unwrap(),
        true
    ));
    assert!(credential_transport_is_safe(
        &url::Url::parse("http://127.0.0.1:52000").unwrap(),
        true
    ));
    assert!(credential_transport_is_safe(
        &url::Url::parse("http://127.0.0.2:52000").unwrap(),
        true
    ));
}

#[tokio::test]
async fn request_count_is_local_to_each_invocation() {
    let server = MockServer::start_async().await;
    let endpoint = server
        .mock_async(|when, then| {
            when.method("POST").path("/embed");
            then.status(200).json_body(serde_json::json!([[1.0_f32]]));
        })
        .await;
    let client = TeiClient::new(TeiClientParams {
        endpoint: server.base_url(),
        provider_id: "tei".to_string(),
        max_batch_inputs: 1,
        max_input_tokens: 8_192,
        max_batch_tokens: 131_072,
        max_concurrent_requests: 2,
        max_in_flight_inputs: 2,
        max_attempts: 1,
        request_timeout: Duration::from_secs(2),
        retry_backoff_base_ms: 1,
    })
    .expect("client");

    let first = client.embed_all(&["one".into()]).await.expect("first");
    let second = client.embed_all(&["two".into()]).await.expect("second");
    assert_eq!(first.requests, 1);
    assert_eq!(second.requests, 1);
    endpoint.assert_calls_async(2).await;
}

#[test]
fn tei_client_new_reuses_the_shared_client_across_many_constructions() {
    let before = shared_client_build_count();
    for i in 0..5 {
        TeiClient::new(TeiClientParams {
            endpoint: "http://127.0.0.1:1".to_string(),
            provider_id: format!("tei-{i}"),
            max_batch_inputs: 8,
            max_input_tokens: 8_192,
            max_batch_tokens: 131_072,
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
            max_input_tokens: 8_192,
            max_batch_tokens: 131_072,
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
        max_input_tokens: 8_192,
        max_batch_tokens: 131_072,
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
