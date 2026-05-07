use std::error::Error as StdError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionRequest {
    pub system_prompt: Option<String>,
    pub user_prompt: String,
    pub model: Option<String>,
    pub stream: bool,
}

impl CompletionRequest {
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
pub struct UsageSnapshot {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionResponse {
    pub text: String,
    pub usage: Option<UsageSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionTurnResult {
    pub text: String,
    pub usage: Option<UsageSnapshot>,
}

#[async_trait::async_trait]
pub trait CompletionRunner {
    async fn complete_text(
        &self,
        req: CompletionRequest,
    ) -> Result<CompletionTurnResult, Box<dyn StdError + Send + Sync>>;

    async fn complete_streaming<F>(
        &self,
        req: CompletionRequest,
        on_delta: &mut F,
    ) -> Result<CompletionTurnResult, Box<dyn StdError + Send + Sync>>
    where
        F: FnMut(&str) -> Result<(), Box<dyn StdError + Send + Sync>> + Send;
}

#[must_use]
pub fn extract_completion_result(turn_result: CompletionTurnResult) -> CompletionResponse {
    CompletionResponse {
        text: turn_result.text,
        usage: turn_result.usage,
    }
}

/// Ensure `req.stream` matches the expected mode.
pub fn normalize_stream_flag(mut req: CompletionRequest, stream: bool) -> CompletionRequest {
    req.stream = stream;
    req
}
