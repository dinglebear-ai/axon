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
    mode: FakeLlmMode,
    calls: Arc<Mutex<Vec<LlmCompletionRequest>>>,
}

impl FakeLlmProvider {
    pub fn new(provider_id: impl Into<String>) -> Self {
        Self {
            provider_id: ProviderId::new(provider_id),
            health: HealthStatus::Healthy,
            mode: FakeLlmMode::Success,
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_health(mut self, health: HealthStatus) -> Self {
        self.health = health;
        self
    }

    pub fn with_mode(mut self, mode: FakeLlmMode) -> Self {
        self.mode = mode;
        self
    }

    pub async fn calls(&self) -> Vec<LlmCompletionRequest> {
        self.calls.lock().await.clone()
    }

    fn error(&self, code: &str, message: &str) -> ApiError {
        ApiError::new(code, axon_error::ErrorStage::Synthesizing, message)
            .with_provider_id(self.provider_id.0.clone())
    }

    fn response(&self, request: &LlmCompletionRequest) -> LlmCompletionResponse {
        let prompt = request
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
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
        Ok(ProviderCapability {
            provider_id: self.provider_id.clone(),
            provider_kind: ProviderKind::Llm,
            implementation: "fake".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            health: self.health,
            limits: ProviderLimits {
                max_concurrency: Some(1),
                timeout_ms: Some(30_000),
                interactive_reserved_concurrency: Some(1),
                ..ProviderLimits::default()
            },
            features: vec!["streaming".to_string(), "call_recording".to_string()],
            cooldown_until: None,
            last_error: None,
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
