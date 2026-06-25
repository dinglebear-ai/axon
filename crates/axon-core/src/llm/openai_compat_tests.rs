use super::*;
use crate::llm::{CompletionRequest, LlmBackendConfig, LlmBackendKind};
use httpmock::prelude::*;

fn backend(server: &MockServer, api_key: Option<&str>) -> LlmBackendConfig {
    LlmBackendConfig {
        kind: LlmBackendKind::OpenAiCompat,
        gemini_cmd: "gemini".to_string(),
        gemini_model: None,
        gemini_home: None,
        openai_base_url: Some(format!("{}/v1", server.base_url())),
        openai_api_key: api_key.map(ToString::to_string),
        openai_model: Some("gemma-4-e4b".to_string()),
        completion_concurrency: 1,
        completion_timeout_secs: 30,
        configured: true,
        ..LlmBackendConfig::default()
    }
}

#[tokio::test(flavor = "current_thread")]
async fn openai_compat_posts_chat_completions_to_base_url() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer local-key")
            .json_body_includes(
                r#"{"model":"gemma-4-e4b","stream":false,"messages":[{"role":"system","content":"system"},{"role":"user","content":"hello"}]}"#,
            );
        then.status(200)
            .header("content-type", "application/json")
            .body(r#"{"choices":[{"message":{"content":"hi from llama.cpp"}}],"usage":{"prompt_tokens":4,"completion_tokens":3,"total_tokens":7}}"#);
    });

    let mut req = CompletionRequest::new("hello").system_prompt("system");
    req.backend = backend(&server, Some("local-key"));

    let response = complete_text(req).await.expect("completion should succeed");

    mock.assert();
    assert_eq!(response.text, "hi from llama.cpp");
    let usage = response.usage.expect("usage should be parsed");
    assert_eq!(usage.prompt_tokens, 4);
    assert_eq!(usage.completion_tokens, 3);
    assert_eq!(usage.total_tokens, 7);
}

#[tokio::test(flavor = "current_thread")]
async fn openai_compat_streams_sse_deltas() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .json_body_includes(r#"{"model":"gemma-4-e4b","stream":true}"#);
        then.status(200)
            .header("content-type", "text/event-stream")
            .body("data: {\"choices\":[{\"delta\":{\"content\":\"hel\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\ndata: [DONE]\n\n");
    });

    let mut req = CompletionRequest::new("hello");
    req.backend = backend(&server, None);
    req.stream = true;
    let mut deltas = String::new();

    let response = complete_streaming(req, |delta| {
        deltas.push_str(delta);
        Ok(())
    })
    .await
    .expect("streaming completion should succeed");

    mock.assert();
    assert_eq!(deltas, "hello");
    assert_eq!(response.text, "hello");
}

#[tokio::test(flavor = "current_thread")]
async fn openai_compat_streams_sse_with_finish_reason_terminal() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .json_body_includes(r#"{"model":"gemma-4-e4b","stream":true}"#);
        then.status(200)
            .header("content-type", "text/event-stream")
            .body("data: {\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\n\ndata: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n");
    });

    let mut req = CompletionRequest::new("hello");
    req.backend = backend(&server, None);
    req.stream = true;

    let response = complete_streaming(req, |_| Ok(()))
        .await
        .expect("finish_reason should terminate stream");

    mock.assert();
    assert_eq!(response.text, "hello");
}

#[tokio::test(flavor = "current_thread")]
async fn openai_compat_rejects_partial_sse_without_terminal_marker() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .json_body_includes(r#"{"model":"gemma-4-e4b","stream":true}"#);
        then.status(200)
            .header("content-type", "text/event-stream")
            .body("data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n");
    });

    let mut req = CompletionRequest::new("hello");
    req.backend = backend(&server, None);
    req.stream = true;

    let err = complete_streaming(req, |_| Ok(()))
        .await
        .expect_err("partial stream should be rejected")
        .to_string();

    mock.assert();
    assert!(err.contains("ended before terminal marker"));
}

#[test]
fn openai_compat_rejects_chat_completions_suffix() {
    let config = LlmBackendConfig {
        kind: LlmBackendKind::OpenAiCompat,
        gemini_cmd: "gemini".to_string(),
        gemini_model: None,
        gemini_home: None,
        openai_base_url: Some("http://127.0.0.1:8080/v1/chat/completions".to_string()),
        openai_api_key: None,
        openai_model: Some("gemma-4-e4b".to_string()),
        completion_concurrency: 1,
        completion_timeout_secs: 30,
        configured: true,
        ..LlmBackendConfig::default()
    };

    let err = openai_chat_completions_url(&config).expect_err("suffix should be rejected");
    assert!(
        err.to_string()
            .contains("must not include /chat/completions")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn openai_compat_error_body_is_bounded_and_redacted() {
    let server = MockServer::start();
    let secret = "sk-live-abcdefghijklmnopqrstuvwxyz123456";
    let prompt = "user prompt: include private customer identifier";
    let body = format!(
        "{{\"error\":\"bad auth\",\"api_key\":\"{secret}\",\"prompt\":\"{prompt}\",\"padding\":\"{}\"}}",
        "x".repeat(1200)
    );
    let _mock = server.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(500)
            .header("content-type", "application/json")
            .body(body);
    });
    let mut req = CompletionRequest::new("hello");
    req.backend = backend(&server, Some(secret));

    let err = complete_text(req)
        .await
        .expect_err("non-2xx response should be an error")
        .to_string();

    assert!(err.contains("HTTP 500"));
    assert!(!err.contains(secret), "error leaked API key: {err}");
    assert!(!err.contains(prompt), "error leaked prompt: {err}");
    assert!(
        err.len() < 700,
        "error body should be bounded: {}",
        err.len()
    );
}

#[test]
fn openai_compat_plain_error_truncates_on_utf8_boundary() {
    // NB: redaction runs before truncation. The `x` padding is zero-entropy, so
    // `core::redact`'s low-entropy carve-out leaves it at full length and the
    // body still exceeds the 512-char truncation point. If that carve-out is
    // ever weakened, the padding would collapse to `[REDACTED]` and this test
    // would fail for a non-obvious reason.
    let body = format!("{}{}", "x".repeat(511), "é".repeat(20));

    let sanitized = sanitize_openai_error_body(&body);

    assert!(sanitized.ends_with("...[truncated]"));
    assert!(sanitized.is_char_boundary(512));
}

#[test]
fn openai_compat_json_error_truncates_on_utf8_boundary() {
    let body = serde_json::json!({
        "error": "backend failed",
        "detail": format!("{}{}", "x".repeat(480), "é".repeat(40)),
    })
    .to_string();

    let sanitized = sanitize_openai_error_body(&body);

    assert!(sanitized.ends_with("...[truncated]"));
    assert!(sanitized.is_char_boundary(512));
}

#[test]
fn openai_compat_json_error_preserves_provider_message_but_redacts_request_echoes() {
    let body = serde_json::json!({
        "error": {
            "message": "model not found",
            "type": "invalid_request_error"
        },
        "messages": [
            {"role": "user", "content": "private prompt"}
        ],
        "authorization": "Bearer sk-live-secret",
        "detail": "upstream mentioned token=abc123 and sk-live-abcdefghijklmnopqrstuvwxyz"
    })
    .to_string();

    let sanitized = sanitize_openai_error_body(&body);

    assert!(
        sanitized.contains("model not found"),
        "provider diagnostic should be preserved: {sanitized}"
    );
    assert!(!sanitized.contains("private prompt"));
    assert!(!sanitized.contains("sk-live-secret"));
    assert!(!sanitized.contains("token=abc123"));
    assert!(!sanitized.contains("sk-live-abcdefghijklmnopqrstuvwxyz"));
    assert!(sanitized.contains("[redacted]"));
}
