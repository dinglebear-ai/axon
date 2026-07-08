use axon_core::llm::{
    CompletionRequest, CompletionResponse, CompletionRunner, CompletionTurnResult, UsageSnapshot,
};
use axon_llm::runtime::{complete_streaming_with_runner, complete_text_with_runner};
use std::error::Error;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct FakeCompletionRunner {
    captured_requests: Arc<Mutex<Vec<CompletionRequest>>>,
    result: CompletionTurnResult,
    deltas: Vec<String>,
}

#[async_trait::async_trait]
impl CompletionRunner for FakeCompletionRunner {
    async fn complete_text(
        &self,
        req: CompletionRequest,
    ) -> Result<CompletionTurnResult, Box<dyn Error + Send + Sync>> {
        self.captured_requests
            .lock()
            .expect("lock request capture")
            .push(req);
        Ok(self.result.clone())
    }

    async fn complete_streaming<F>(
        &self,
        req: CompletionRequest,
        on_delta: &mut F,
    ) -> Result<CompletionTurnResult, Box<dyn Error + Send + Sync>>
    where
        F: FnMut(&str) -> Result<(), Box<dyn Error + Send + Sync>> + Send,
    {
        self.captured_requests
            .lock()
            .expect("lock request capture")
            .push(req);
        for delta in &self.deltas {
            on_delta(delta)?;
        }
        Ok(self.result.clone())
    }
}

#[test]
fn services_llm_backend_request_shape_supports_system_user_model_and_stream_toggle() {
    let req = CompletionRequest::new("user prompt")
        .system_prompt("system prompt")
        .model("llama3.1")
        .stream(true);

    assert_eq!(req.system_prompt.as_deref(), Some("system prompt"));
    assert_eq!(req.user_prompt, "user prompt");
    assert_eq!(req.model.as_deref(), Some("llama3.1"));
    assert!(req.stream);
}

#[tokio::test]
async fn services_llm_backend_complete_text_with_runner_extracts_turn_result_text() {
    let captured_requests = Arc::new(Mutex::new(Vec::new()));
    let runner = FakeCompletionRunner {
        captured_requests: Arc::clone(&captured_requests),
        result: CompletionTurnResult {
            text: "final answer".to_string(),
            usage: Some(UsageSnapshot {
                prompt_tokens: 12,
                completion_tokens: 34,
                total_tokens: 46,
            }),
        },
        deltas: vec![],
    };

    let req = CompletionRequest::new("user prompt")
        .system_prompt("system prompt")
        .model("llama3.1")
        .stream(true);

    let response = complete_text_with_runner(&runner, req)
        .await
        .expect("runner result should be mapped");

    assert_eq!(
        response,
        CompletionResponse {
            text: "final answer".to_string(),
            usage: Some(UsageSnapshot {
                prompt_tokens: 12,
                completion_tokens: 34,
                total_tokens: 46,
            }),
        }
    );

    let captured = captured_requests.lock().expect("request capture lock");
    assert_eq!(captured.len(), 1);
    let captured = &captured[0];
    assert_eq!(captured.system_prompt.as_deref(), Some("system prompt"));
    assert_eq!(captured.user_prompt, "user prompt");
    assert_eq!(captured.model.as_deref(), Some("llama3.1"));
    assert!(!captured.stream);
}

#[tokio::test]
async fn services_llm_backend_complete_streaming_with_runner_normalizes_stream_and_propagates_delta_errors()
 {
    let captured_requests = Arc::new(Mutex::new(Vec::new()));
    let runner = FakeCompletionRunner {
        captured_requests: Arc::clone(&captured_requests),
        result: CompletionTurnResult {
            text: "final answer".to_string(),
            usage: None,
        },
        deltas: vec!["alpha".to_string(), "beta".to_string()],
    };

    let mut seen_deltas = Vec::new();
    let req = CompletionRequest::new("user prompt")
        .system_prompt("system prompt")
        .model("llama3.1")
        .stream(false);

    let err = complete_streaming_with_runner(&runner, req, |delta| {
        seen_deltas.push(delta.to_string());
        if delta == "beta" {
            return Err(std::io::Error::other("delta handler failed").into());
        }
        Ok(())
    })
    .await
    .expect_err("delta handler error should propagate");

    assert!(err.to_string().contains("delta handler failed"));
    assert_eq!(seen_deltas, vec!["alpha".to_string(), "beta".to_string()]);

    let captured = captured_requests.lock().expect("request capture lock");
    assert_eq!(captured.len(), 1);
    assert!(captured[0].stream);
}
