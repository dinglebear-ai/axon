use axon::crates::services::acp_llm::{
    AcpCompletionRequest, AcpCompletionResponse, AcpUsageSnapshot, extract_completion_result,
};

#[test]
fn services_acp_llm_extract_completion_result_returns_text_and_usage_snapshot() {
    let response = extract_completion_result(
        "final answer",
        Some(AcpUsageSnapshot {
            prompt_tokens: 12,
            completion_tokens: 34,
            total_tokens: 46,
        }),
    );

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
