use super::*;
use crate::services::llm_backend::{CompletionRequest, LlmBackendConfig, LlmBackendKind};
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
    };

    let err = openai_chat_completions_url(&config).expect_err("suffix should be rejected");
    assert!(
        err.to_string()
            .contains("must not include /chat/completions")
    );
}
