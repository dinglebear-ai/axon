use axon::crates::services::acp_llm::{
    AcpCompletionRequest, AcpCompletionResponse, AcpCompletionRunner, AcpCompletionTurnResult,
    AcpUsageSnapshot, complete_text_with_runner,
};
use std::error::Error;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct FakeCompletionRunner {
    captured_request: Arc<Mutex<Option<AcpCompletionRequest>>>,
    result: AcpCompletionTurnResult,
}

#[async_trait::async_trait(?Send)]
impl AcpCompletionRunner for FakeCompletionRunner {
    async fn complete_text(
        &self,
        req: AcpCompletionRequest,
    ) -> Result<AcpCompletionTurnResult, Box<dyn Error>> {
        *self.captured_request.lock().expect("lock request capture") = Some(req);
        Ok(self.result.clone())
    }

    async fn complete_streaming<F>(
        &self,
        req: AcpCompletionRequest,
        _on_delta: &mut F,
    ) -> Result<AcpCompletionTurnResult, Box<dyn Error>>
    where
        F: FnMut(&str) -> Result<(), Box<dyn Error>> + Send,
    {
        *self.captured_request.lock().expect("lock request capture") = Some(req);
        Ok(self.result.clone())
    }
}

#[test]
fn services_acp_llm_request_shape_supports_system_user_model_and_stream_toggle() {
    let req = AcpCompletionRequest::new("user prompt")
        .system_prompt("system prompt")
        .model("llama3.1")
        .stream(true);

    assert_eq!(req.system_prompt.as_deref(), Some("system prompt"));
    assert_eq!(req.user_prompt, "user prompt");
    assert_eq!(req.model.as_deref(), Some("llama3.1"));
    assert!(req.stream);
}

#[tokio::test]
async fn services_acp_llm_complete_text_with_runner_extracts_turn_result_text() {
    let captured_request = Arc::new(Mutex::new(None));
    let runner = FakeCompletionRunner {
        captured_request: Arc::clone(&captured_request),
        result: AcpCompletionTurnResult {
            text: "final answer".to_string(),
            usage: Some(AcpUsageSnapshot {
                prompt_tokens: 12,
                completion_tokens: 34,
                total_tokens: 46,
            }),
        },
    };

    let req = AcpCompletionRequest::new("user prompt")
        .system_prompt("system prompt")
        .model("llama3.1");

    let response = complete_text_with_runner(&runner, req)
        .await
        .expect("runner result should be mapped");

    assert_eq!(
        response,
        AcpCompletionResponse {
            text: "final answer".to_string(),
            usage: Some(AcpUsageSnapshot {
                prompt_tokens: 12,
                completion_tokens: 34,
                total_tokens: 46,
            }),
        }
    );

    let captured = captured_request
        .lock()
        .expect("request capture lock")
        .clone()
        .expect("request should be captured");
    assert_eq!(captured.system_prompt.as_deref(), Some("system prompt"));
    assert_eq!(captured.user_prompt, "user prompt");
    assert_eq!(captured.model.as_deref(), Some("llama3.1"));
    assert!(!captured.stream);
}
