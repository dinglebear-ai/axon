use std::error::Error as StdError;
use std::path::PathBuf;

use crate::core::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmModelPurpose {
    Synthesis,
    Chat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmBackendKind {
    GeminiHeadless,
    OpenAiCompat,
    CodexAppServer,
}

impl LlmBackendKind {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim() {
            "" | "gemini-headless" | "gemini" | "headless" => Ok(Self::GeminiHeadless),
            "openai-compat" | "openai_compat" => Ok(Self::OpenAiCompat),
            "codex-app-server" | "codex_app_server" | "codex" => Ok(Self::CodexAppServer),
            other => Err(format!(
                "AXON_LLM_BACKEND must be 'gemini-headless', 'openai-compat', or 'codex-app-server' (got '{other}')"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmBackendConfig {
    pub kind: LlmBackendKind,
    pub gemini_cmd: String,
    pub gemini_model: Option<String>,
    pub gemini_home: Option<PathBuf>,
    pub openai_base_url: Option<String>,
    pub openai_api_key: Option<String>,
    pub openai_model: Option<String>,
    pub codex_cmd: String,
    pub codex_model: Option<String>,
    pub codex_home: Option<PathBuf>,
    pub completion_concurrency: usize,
    pub completion_timeout_secs: u64,
    pub configured: bool,
}

impl Default for LlmBackendConfig {
    fn default() -> Self {
        Self {
            kind: LlmBackendKind::GeminiHeadless,
            gemini_cmd: "gemini".to_string(),
            gemini_model: None,
            gemini_home: None,
            openai_base_url: None,
            openai_api_key: None,
            openai_model: None,
            codex_cmd: "codex".to_string(),
            codex_model: None,
            codex_home: None,
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
            kind: cfg.llm_backend,
            gemini_cmd: non_empty(cfg.headless_gemini_cmd.clone())
                .unwrap_or_else(|| "gemini".to_string()),
            gemini_model: non_empty(cfg.headless_gemini_model.clone()),
            gemini_home: cfg.headless_gemini_home.clone(),
            openai_base_url: non_empty(cfg.openai_base_url.clone()),
            openai_api_key: non_empty(cfg.openai_api_key.clone()),
            openai_model: non_empty(cfg.openai_model.clone()),
            codex_cmd: non_empty(cfg.codex_cmd.clone()).unwrap_or_else(|| "codex".to_string()),
            codex_model: non_empty(cfg.codex_model.clone()),
            codex_home: cfg.codex_home.clone(),
            completion_concurrency: match cfg.llm_backend {
                LlmBackendKind::CodexAppServer => cfg
                    .codex_completion_concurrency
                    .clamp(1, tokio::sync::Semaphore::MAX_PERMITS),
                _ => cfg
                    .llm_completion_concurrency
                    .clamp(1, tokio::sync::Semaphore::MAX_PERMITS),
            },
            completion_timeout_secs: cfg.llm_completion_timeout_secs.max(1),
            configured: true,
        }
    }

    #[must_use]
    pub fn completion_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.completion_timeout_secs.max(1))
    }
}

#[must_use]
pub fn configured_model_from_config(cfg: &Config) -> Option<String> {
    configured_model_for_config(cfg, LlmModelPurpose::Synthesis)
}

#[must_use]
pub fn configured_chat_model_from_config(cfg: &Config) -> Option<String> {
    configured_model_for_config(cfg, LlmModelPurpose::Chat)
}

#[must_use]
pub fn configured_model_for_config(cfg: &Config, purpose: LlmModelPurpose) -> Option<String> {
    match cfg.llm_backend {
        LlmBackendKind::GeminiHeadless => match purpose {
            LlmModelPurpose::Synthesis => non_empty(cfg.headless_gemini_model.clone()),
            LlmModelPurpose::Chat => non_empty(cfg.headless_gemini_chat_model.clone())
                .or_else(|| non_empty(cfg.headless_gemini_model.clone())),
        },
        LlmBackendKind::OpenAiCompat => match purpose {
            LlmModelPurpose::Synthesis => non_empty(cfg.openai_model.clone()),
            LlmModelPurpose::Chat => non_empty(cfg.openai_chat_model.clone())
                .or_else(|| non_empty(cfg.openai_model.clone())),
        },
        LlmBackendKind::CodexAppServer => match purpose {
            LlmModelPurpose::Synthesis | LlmModelPurpose::Chat => {
                non_empty(cfg.codex_model.clone())
            }
        },
    }
}

/// Context-window size class of the configured synthesis model. This is shared
/// by ask tuning, RAG context assembly, and research-source preservation so the
/// model-capability policy does not drift between call sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SynthesisModelTier {
    /// ~1M-token windows — Gemini, Claude.
    Large,
    /// ~400k-token window — GPT/Codex.
    Medium,
    /// Local Gemma on the 12 GB llama.cpp path.
    LocalGemma,
    /// Unknown model — assume a < 50k-token window.
    Small,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SynthesisModelProfile {
    model: String,
    is_gemini_backend: bool,
}

impl SynthesisModelProfile {
    #[must_use]
    pub fn from_config(cfg: &Config) -> Self {
        Self {
            model: configured_model_from_config(cfg)
                .unwrap_or_default()
                .to_ascii_lowercase(),
            is_gemini_backend: matches!(cfg.llm_backend, LlmBackendKind::GeminiHeadless),
        }
    }

    #[must_use]
    pub fn tier(&self) -> SynthesisModelTier {
        if self.is_gemini() || self.model.contains("claude") {
            SynthesisModelTier::Large
        } else if self.model.contains("gemma") {
            SynthesisModelTier::LocalGemma
        } else if self.is_gpt_or_codex() {
            SynthesisModelTier::Medium
        } else {
            SynthesisModelTier::Small
        }
    }

    #[must_use]
    pub fn high_context_full_docs(&self) -> bool {
        matches!(
            self.tier(),
            SynthesisModelTier::Large | SynthesisModelTier::Medium
        )
    }

    #[must_use]
    pub fn preserve_full_research_sources(&self) -> bool {
        self.is_gemini() || self.model.contains("opus") || self.is_gpt_or_codex()
    }

    fn is_gemini(&self) -> bool {
        self.is_gemini_backend || self.model.contains("gemini")
    }

    fn is_gpt_or_codex(&self) -> bool {
        self.model.contains("codex")
            || self.model.starts_with("gpt-")
            || self.model.contains("/gpt-")
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
            self.model = configured_model_from_config(cfg);
        }
        self
    }

    #[must_use]
    pub fn backend_from_config_for(mut self, cfg: &Config, purpose: LlmModelPurpose) -> Self {
        self.backend = LlmBackendConfig::from_config(cfg);
        if self.model.is_none() {
            self.model = configured_model_for_config(cfg, purpose);
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

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
