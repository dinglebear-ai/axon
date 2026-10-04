use super::super::{BatchLimits, TeiClientParams};
use super::*;
use httpmock::MockServer;

fn client(endpoint: String) -> TeiClient {
    TeiClient::new(TeiClientParams {
        endpoint,
        provider_id: "tokens".into(),
        max_batch_inputs: 8,
        max_input_tokens: 8,
        max_batch_tokens: 4,
        max_concurrent_requests: 1,
        max_in_flight_inputs: 8,
        max_attempts: 1,
        request_timeout: Duration::from_secs(1),
        retry_backoff_base_ms: 1,
    })
    .unwrap()
}

#[tokio::test]
async fn server_tokens_improve_packing_and_preserve_order() {
    let server = MockServer::start_async().await;
    let tokenize = server
        .mock_async(|when, then| {
            when.method("POST")
                .path("/tokenize")
                .json_body(serde_json::json!({
                    "inputs": ["long ASCII sentence", "漢字"], "add_special_tokens": true
                }));
            then.status(200)
                .json_body(serde_json::json!([[{"id":1},{"id":2}], [{"id":3},{"id":4}]]));
        })
        .await;
    let embed = server
        .mock_async(|when, then| {
            when.method("POST")
                .path("/embed")
                .json_body(serde_json::json!({
                    "inputs": ["漢字", "long ASCII sentence"], "truncate": false
                }));
            then.status(200)
                .json_body(serde_json::json!([[0.2], [0.1]]));
        })
        .await;
    let result = client(server.base_url())
        .embed_all(&["long ASCII sentence".into(), "漢字".into()])
        .await
        .unwrap();
    assert_eq!(result.vectors, vec![vec![0.1], vec![0.2]]);
    assert_eq!(result.input_tokens, Some(4));
    assert_eq!(result.tokenization_fallback, None);
    tokenize.assert_calls_async(1).await;
    embed.assert_calls_async(1).await;
}

#[tokio::test]
async fn unsupported_capability_is_shared_and_fallback_is_visible() {
    let server = MockServer::start_async().await;
    let unsupported = server
        .mock_async(|when, then| {
            when.method("POST").path("/tokenize");
            then.status(404);
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method("POST").path("/embed");
            then.status(200).json_body(serde_json::json!([[0.1]]));
        })
        .await;
    let first = client(server.base_url());
    let second = client(server.base_url()).with_tokenization(first.tokenization.clone());
    for transport in [&first, &second] {
        let result = transport.embed_all(&["a".into()]).await.unwrap();
        assert_eq!(result.input_tokens, None);
        assert_eq!(result.tokenization_fallback, Some("unsupported_endpoint"));
    }
    unsupported.assert_calls_async(1).await;
}

#[tokio::test]
async fn malformed_and_denied_counts_never_relax_limits() {
    for (status, body) in [(200, "[[{}]]"), (200, "[]"), (403, "denied")] {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method("POST").path("/tokenize");
                then.status(status).body(body);
            })
            .await;
        let embed = server
            .mock_async(|when, then| {
                when.method("POST").path("/embed");
                then.status(200).json_body(serde_json::json!([[0.1]]));
            })
            .await;
        let error = client(server.base_url())
            .embed_all(&["too many bytes".into()])
            .await
            .unwrap_err();
        assert_eq!(error.code, "embedding.tei.input_too_large".into());
        embed.assert_calls_async(0).await;
    }
}

#[test]
fn counted_packing_enforces_escaped_payload_and_single_batch_token_limits() {
    let inputs = vec!["\u{0}".repeat(10)];
    assert!(
        policy::pack_batches_counted(
            &inputs,
            &[1],
            BatchLimits {
                max_inputs: 8,
                max_input_tokens: 8,
                max_batch_tokens: 4,
                max_batch_bytes: 64,
            }
        )
        .is_err()
    );
    assert!(
        policy::pack_batches_counted(
            &["a".into()],
            &[5],
            BatchLimits {
                max_inputs: 8,
                max_input_tokens: 8,
                max_batch_tokens: 4,
                max_batch_bytes: 1024,
            }
        )
        .is_err()
    );
}

#[tokio::test]
async fn tokenization_admission_timeout_releases_every_permit() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method("POST").path("/tokenize");
            then.status(200)
                .delay(Duration::from_millis(200))
                .json_body(serde_json::json!([[{"id":1}]]));
        })
        .await;
    let mut transport = client(server.base_url());
    transport.request_timeout = Duration::from_millis(20);
    let (_, reason) = transport.token_counts(&["a".into()]).await;
    assert!(matches!(
        reason,
        Some("tokenization_timeout" | "tokenization_transport")
    ));
    assert_eq!(transport.request_slots.available_permits(), 1);
    assert_eq!(transport.input_slots.available_permits(), 8);
    assert_eq!(transport.profile_input_slots.available_permits(), 8);
    assert!(!transport.tokenization.unsupported.load(Ordering::Acquire));
}

#[tokio::test]
async fn tokenizer_windows_share_configured_request_and_input_limits() {
    let server = MockServer::start_async().await;
    let tokenize = server
        .mock_async(|when, then| {
            when.method("POST")
                .path("/tokenize")
                .json_body(serde_json::json!({
                    "inputs": ["a", "a"], "add_special_tokens": true
                }));
            then.status(200)
                .json_body(serde_json::json!([[{"id":1}], [{"id":1}]]));
        })
        .await;
    let mut transport = client(server.base_url());
    transport.max_batch_inputs = 2;
    let (counts, fallback) = transport
        .token_counts(&["a".into(), "a".into(), "a".into(), "a".into()])
        .await;
    assert_eq!(counts, vec![1, 1, 1, 1]);
    assert_eq!(fallback, None);
    tokenize.assert_calls_async(2).await;
}
