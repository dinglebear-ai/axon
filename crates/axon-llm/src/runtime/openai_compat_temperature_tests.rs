use super::*;
use crate::runtime::{CompletionRequest, LlmBackendConfig, LlmBackendKind};
use httpmock::prelude::*;

fn backend(server: &MockServer, _key: Option<String>) -> LlmBackendConfig {
    LlmBackendConfig {
        kind: LlmBackendKind::OpenAiCompat,
        openai_base_url: Some(server.base_url() + "/v1"),
        openai_model: Some("fixture-model".into()),
        ..LlmBackendConfig::default()
    }
}

#[tokio::test]
async fn completion_temperature_is_omitted_by_default_and_forwarded_when_explicit() {
    for temperature in [None, Some(0.2)] {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/chat/completions")
                .is_true(move |request| {
                    let body: serde_json::Value =
                        serde_json::from_slice(request.body().as_ref()).unwrap();
                    match temperature {
                        Some(value) => body.get("temperature") == Some(&serde_json::json!(value)),
                        None => body.get("temperature").is_none(),
                    }
                });
            then.status(200)
                .json_body(serde_json::json!({"choices":[{"message":{"content":"answer"}}]}));
        });
        let mut request = CompletionRequest::new("question");
        request.backend = backend(&server, None);
        request.backend.openai_temperature = temperature;
        assert_eq!(complete_text(request).await.unwrap().text, "answer");
        mock.assert();
    }
}
