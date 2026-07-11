//! Deterministic LLM provider fake.

use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;
use tokio::sync::Mutex;

use crate::completion::{LlmCompletionRequest, LlmCompletionResponse};
use crate::provider::{LlmProvider, Result};
use crate::stream::{LlmDelta, LlmDeltaSink};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FakeLlmMode {
    Success,
    Timeout,
    RateLimited,
    Fatal,
    MalformedStructuredOutput,
}

#[derive(Debug, Clone)]
pub struct FakeLlmProvider {
    provider_id: ProviderId,
    health: HealthStatus,
    health_override: Option<HealthStatus>,
    mode: FakeLlmMode,
    cooldown_until_override: Option<Timestamp>,
    calls: Arc<Mutex<Vec<LlmCompletionRequest>>>,
}

impl FakeLlmProvider {
    pub fn new(provider_id: impl Into<String>) -> Self {
        Self {
            provider_id: ProviderId::new(provider_id),
            health: HealthStatus::Healthy,
            health_override: None,
            mode: FakeLlmMode::Success,
            cooldown_until_override: None,
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_health(mut self, health: HealthStatus) -> Self {
        self.health = health;
        self.health_override = Some(health);
        self
    }

    pub fn with_mode(mut self, mode: FakeLlmMode) -> Self {
        self.mode = mode;
        self
    }

    /// Override `capabilities().cooldown_until`, taking precedence over the
    /// fixed timestamp [`FakeLlmMode::RateLimited`] otherwise reports. Lets
    /// tests simulate a live, "now"-relative cooldown window instead of a
    /// mode-derived fixed instant.
    pub fn with_cooldown_until(mut self, cooldown_until: Timestamp) -> Self {
        self.cooldown_until_override = Some(cooldown_until);
        self
    }

    pub async fn calls(&self) -> Vec<LlmCompletionRequest> {
        self.calls.lock().await.clone()
    }

    fn mode_state(&self) -> FakeProviderModeState {
        match self.mode {
            FakeLlmMode::Success | FakeLlmMode::MalformedStructuredOutput => {
                FakeProviderModeState::Success
            }
            FakeLlmMode::Timeout => FakeProviderModeState::Timeout,
            FakeLlmMode::RateLimited => FakeProviderModeState::RateLimited,
            FakeLlmMode::Fatal => FakeProviderModeState::Fatal,
        }
    }

    fn error(&self, code: &str, message: &str) -> ApiError {
        let mut error = ApiError::new(code, axon_error::ErrorStage::Synthesizing, message)
            .with_provider_id(self.provider_id.0.clone());
        if self.mode == FakeLlmMode::Fatal {
            error.retryable = false;
        }
        error
    }

    fn capability_state(&self) -> FakeProviderCapabilityState {
        let mut state = fake_provider_capability_state(
            self.mode_state(),
            &self.provider_id.0,
            axon_error::ErrorStage::Synthesizing,
            "llm provider",
        );
        if let Some(health) = self.health_override.filter(|health| {
            self.mode_state() == FakeProviderModeState::Success || *health != HealthStatus::Healthy
        }) {
            state.health = health;
        }
        if let Some(cooldown_until) = self.cooldown_until_override.clone() {
            state.cooldown_until = Some(cooldown_until);
        }
        state
    }

    fn response(&self, request: &LlmCompletionRequest) -> LlmCompletionResponse {
        let prompt_parts = request
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>();
        let prompt = match request.system.as_deref() {
            Some(system) => std::iter::once(system)
                .chain(prompt_parts)
                .collect::<Vec<_>>()
                .join("\n"),
            None => prompt_parts.join("\n"),
        };
        let checksum = stable_checksum(&prompt);
        let text = format!("fake:{}:{checksum}", self.provider_id.0);
        let structured = match (self.mode, request.response_schema.as_ref()) {
            (FakeLlmMode::MalformedStructuredOutput, Some(_)) => {
                Some(serde_json::json!({"malformed": true}))
            }
            (_, Some(_)) => Some(serde_json::json!({
                "provider": self.provider_id.0,
                "checksum": checksum,
            })),
            _ => None,
        };
        LlmCompletionResponse {
            text,
            model: self.provider_id.0.clone(),
            finish_reason: "stop".to_string(),
            usage: ProviderUsage {
                input_tokens: Some(prompt.split_whitespace().count() as u64),
                output_tokens: Some(1),
                requests: 1,
                duration_ms: 0,
            },
            structured,
            tool_calls: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

#[async_trait]
impl LlmProvider for FakeLlmProvider {
    async fn complete(&self, request: LlmCompletionRequest) -> Result<LlmCompletionResponse> {
        self.calls.lock().await.push(request.clone());
        match self.mode {
            FakeLlmMode::Success | FakeLlmMode::MalformedStructuredOutput => {
                Ok(self.response(&request))
            }
            FakeLlmMode::Timeout => Err(self.error("provider.timeout", "llm provider timed out")),
            FakeLlmMode::RateLimited => {
                Err(self.error("provider.rate_limited", "llm provider rate limited"))
            }
            FakeLlmMode::Fatal => Err(self.error("provider.fatal", "llm provider failed fatally")),
        }
    }

    async fn complete_streaming(
        &self,
        request: LlmCompletionRequest,
        on_delta: LlmDeltaSink<'_>,
    ) -> Result<LlmCompletionResponse> {
        let response = self.complete(request).await?;
        for piece in response.text.split_inclusive(':') {
            on_delta(LlmDelta {
                text: piece.to_string(),
                tool_call: None,
            });
        }
        Ok(response)
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        let state = self.capability_state();
        Ok(ProviderCapability {
            provider_id: self.provider_id.clone(),
            provider_kind: ProviderKind::Llm,
            implementation: "fake".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            health: state.health,
            limits: ProviderLimits {
                max_concurrency: Some(1),
                timeout_ms: Some(30_000),
                interactive_reserved_concurrency: Some(1),
                ..ProviderLimits::default()
            },
            features: vec!["streaming".to_string(), "call_recording".to_string()],
            cooldown_until: state.cooldown_until,
            last_error: state.last_error,
            reservation_policy: ReservationPolicy {
                supports_reservations: true,
                queue_policy: QueuePolicy::Priority,
                interactive_reserve: 1,
                cooldown_after_failures: 1,
                cooldown_secs: 30,
                retry_backoff_ms: Some(100),
            },
            reservation_state: ReservationStateSnapshot {
                queued: 0,
                active: 0,
                available_units: 1,
                oldest_queued_ms: None,
                priority_breakdown: Default::default(),
                states: vec![ReservationState::Granted],
            },
            cost_class: ProviderCostClass::Internal,
            degraded_modes: Vec::new(),
            fake_overrides_supported: true,
            embedding: None,
            llm: Some(LlmProviderCapability {
                model_id: self.provider_id.0.clone(),
                context_window: 128_000,
                streaming: true,
                json_schema: true,
                tool_use: false,
                structured_output: true,
                max_output_tokens: 4096,
            }),
            vector_store: None,
            fetch: None,
            render: None,
            credential: None,
        })
    }
}

fn stable_checksum(input: &str) -> u32 {
    input.bytes().fold(0u32, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(u32::from(byte))
    })
}
