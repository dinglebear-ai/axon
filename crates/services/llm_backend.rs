pub mod headless;

pub use crate::crates::services::acp_llm::{
    AcpCompletionRequest, AcpCompletionResponse, AcpCompletionRunner, AcpCompletionTurnResult,
    AcpUsageSnapshot, WarmAcpSession, WarmAcpSessionOrigin, complete_streaming,
    complete_streaming_with_runner, complete_text, complete_text_with_runner,
    extract_completion_result, init_warm_pool, normalize_stream_flag, pool_size, warm_session,
};

#[cfg(test)]
mod tests {
    use super::headless::HeadlessAgent;

    #[test]
    fn headless_agent_names_are_stable() {
        assert_eq!(HeadlessAgent::Claude.as_str(), "claude");
        assert_eq!(HeadlessAgent::Codex.as_str(), "codex");
        assert_eq!(HeadlessAgent::Gemini.as_str(), "gemini");
    }
}
