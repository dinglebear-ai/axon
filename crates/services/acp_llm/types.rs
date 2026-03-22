use std::error::Error as StdError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpCompletionRequest {
    pub system_prompt: Option<String>,
    pub user_prompt: String,
    pub model: Option<String>,
    pub stream: bool,
}

impl AcpCompletionRequest {
    #[must_use]
    pub fn new(user_prompt: impl Into<String>) -> Self {
        Self {
            system_prompt: None,
            user_prompt: user_prompt.into(),
            model: None,
            stream: false,
        }
    }

    #[must_use]
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    #[must_use]
    pub fn stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpUsageSnapshot {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

impl From<agent_client_protocol::Usage> for AcpUsageSnapshot {
    fn from(value: agent_client_protocol::Usage) -> Self {
        Self {
            prompt_tokens: value.input_tokens,
            completion_tokens: value.output_tokens,
            total_tokens: value.total_tokens,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpCompletionResponse {
    pub text: String,
    pub usage: Option<AcpUsageSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpCompletionTurnResult {
    pub text: String,
    pub usage: Option<AcpUsageSnapshot>,
}

#[async_trait::async_trait(?Send)]
pub trait AcpCompletionRunner {
    async fn complete_text(
        &self,
        req: AcpCompletionRequest,
    ) -> Result<AcpCompletionTurnResult, Box<dyn StdError>>;

    async fn complete_streaming<F>(
        &self,
        req: AcpCompletionRequest,
        on_delta: &mut F,
    ) -> Result<AcpCompletionTurnResult, Box<dyn StdError>>
    where
        F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send;
}

#[must_use]
pub fn extract_completion_result(turn_result: AcpCompletionTurnResult) -> AcpCompletionResponse {
    AcpCompletionResponse {
        text: turn_result.text,
        usage: turn_result.usage,
    }
}

/// Ensure `req.stream` matches the expected mode.
pub fn normalize_stream_flag(mut req: AcpCompletionRequest, stream: bool) -> AcpCompletionRequest {
    req.stream = stream;
    req
}
