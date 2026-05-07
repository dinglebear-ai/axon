use std::error::Error as StdError;
use std::path::PathBuf;

use crate::core::config::Config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmBackendConfig {
    pub gemini_cmd: String,
    pub gemini_model: Option<String>,
    pub gemini_home: Option<PathBuf>,
    pub completion_concurrency: usize,
    pub completion_timeout_secs: u64,
    pub configured: bool,
}

impl Default for LlmBackendConfig {
    fn default() -> Self {
        Self {
            gemini_cmd: "gemini".to_string(),
            gemini_model: None,
            gemini_home: None,
            completion_concurrency: 4,
            completion_timeout_secs: 300,
            configured: false,
        }
    }
}

impl LlmBackendConfig {
    #[must_use]
    pub fn from_config(cfg: &Config) -> Self {
        Self {
            gemini_cmd: non_empty(cfg.headless_gemini_cmd.clone())
                .unwrap_or_else(|| "gemini".to_string()),
            gemini_model: non_empty(cfg.headless_gemini_model.clone())
                .or_else(|| gemini_compatible_model(&cfg.openai_model)),
            gemini_home: cfg.headless_gemini_home.clone(),
            completion_concurrency: cfg.llm_completion_concurrency.max(1),
            completion_timeout_secs: cfg.llm_completion_timeout_secs.max(1),
            configured: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionRequest {
    pub system_prompt: Option<String>,
    pub user_prompt: String,
    pub model: Option<String>,
    pub stream: bool,
    pub backend: LlmBackendConfig,
}

impl CompletionRequest {
    #[must_use]
    pub fn new(user_prompt: impl Into<String>) -> Self {
        Self {
            system_prompt: None,
            user_prompt: user_prompt.into(),
            model: None,
            stream: false,
            backend: LlmBackendConfig::default(),
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

    #[must_use]
    pub fn backend_from_config(mut self, cfg: &Config) -> Self {
        self.backend = LlmBackendConfig::from_config(cfg);
        if self.model.is_none() {
            self.model = self.backend.gemini_model.clone();
        }
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

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn gemini_compatible_model(model: &str) -> Option<String> {
    let model = model.trim();
    model.starts_with("gemini-").then(|| model.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_config_ignores_legacy_openai_model_names() {
        let cfg = Config {
            openai_model: "gpt-4o-mini".to_string(),
            ..Config::default()
        };
        let backend = LlmBackendConfig::from_config(&cfg);
        assert_eq!(backend.gemini_model, None);
    }

    #[test]
    fn backend_config_accepts_explicit_gemini_model() {
        let cfg = Config {
            openai_model: "gpt-4o-mini".to_string(),
            headless_gemini_model: "gemini-3.1-pro-preview".to_string(),
            ..Config::default()
        };
        let backend = LlmBackendConfig::from_config(&cfg);
        assert_eq!(
            backend.gemini_model.as_deref(),
            Some("gemini-3.1-pro-preview")
        );
    }
}
