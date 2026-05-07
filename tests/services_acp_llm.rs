use axon::core::config::{AskBackend, Config};
use axon::services::acp_llm::{
    AcpCompletionRequest, AcpCompletionResponse, AcpCompletionRunner, AcpCompletionTurnResult,
    AcpUsageSnapshot, complete_streaming_with_runner, complete_text, complete_text_with_runner,
};
use std::error::Error;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct FakeCompletionRunner {
    captured_requests: Arc<Mutex<Vec<AcpCompletionRequest>>>,
    result: AcpCompletionTurnResult,
    deltas: Vec<String>,
}

#[async_trait::async_trait(?Send)]
impl AcpCompletionRunner for FakeCompletionRunner {
    async fn complete_text(
        &self,
        req: AcpCompletionRequest,
    ) -> Result<AcpCompletionTurnResult, Box<dyn Error>> {
        self.captured_requests
            .lock()
            .expect("lock request capture")
            .push(req);
        Ok(self.result.clone())
    }

    async fn complete_streaming<F>(
        &self,
        req: AcpCompletionRequest,
        on_delta: &mut F,
    ) -> Result<AcpCompletionTurnResult, Box<dyn Error>>
    where
        F: FnMut(&str) -> Result<(), Box<dyn Error>> + Send,
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
    let captured_requests = Arc::new(Mutex::new(Vec::new()));
    let runner = FakeCompletionRunner {
        captured_requests: Arc::clone(&captured_requests),
        result: AcpCompletionTurnResult {
            text: "final answer".to_string(),
            usage: Some(AcpUsageSnapshot {
                prompt_tokens: 12,
                completion_tokens: 34,
                total_tokens: 46,
            }),
        },
        deltas: vec![],
    };

    let req = AcpCompletionRequest::new("user prompt")
        .system_prompt("system prompt")
        .model("llama3.1")
        .stream(true);

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

    let captured = captured_requests.lock().expect("request capture lock");
    assert_eq!(captured.len(), 1);
    let captured = &captured[0];
    assert_eq!(captured.system_prompt.as_deref(), Some("system prompt"));
    assert_eq!(captured.user_prompt, "user prompt");
    assert_eq!(captured.model.as_deref(), Some("llama3.1"));
    assert!(!captured.stream);
}

#[tokio::test]
async fn services_acp_llm_complete_streaming_with_runner_normalizes_stream_and_propagates_delta_errors()
 {
    let captured_requests = Arc::new(Mutex::new(Vec::new()));
    let runner = FakeCompletionRunner {
        captured_requests: Arc::clone(&captured_requests),
        result: AcpCompletionTurnResult {
            text: "final answer".to_string(),
            usage: None,
        },
        deltas: vec!["alpha".to_string(), "beta".to_string()],
    };

    let mut seen_deltas = Vec::new();
    let req = AcpCompletionRequest::new("user prompt")
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

#[tokio::test]
async fn services_acp_llm_complete_text_requires_adapter_config() {
    let cfg = Config {
        acp_adapter_cmd: None,
        ask_backend: AskBackend::Acp,
        ..Config::default()
    };
    let err = complete_text(&cfg, AcpCompletionRequest::new("user prompt"))
        .await
        .expect_err("missing adapter config should fail");

    assert!(err.to_string().contains("AXON_ACP_ADAPTER_CMD"));
}
