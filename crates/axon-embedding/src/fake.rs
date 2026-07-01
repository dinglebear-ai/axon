//! Deterministic embedding provider fake.

use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;
use tokio::sync::Mutex;

use crate::provider::{EmbeddingProvider, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FakeEmbeddingMode {
    Success,
    Timeout,
    RateLimited,
    Fatal,
}

#[derive(Debug, Clone)]
pub struct FakeEmbeddingProvider {
    provider_id: ProviderId,
    dimensions: u32,
    health: HealthStatus,
    health_override: Option<HealthStatus>,
    mode: FakeEmbeddingMode,
    calls: Arc<Mutex<Vec<EmbeddingBatch>>>,
}

impl FakeEmbeddingProvider {
    pub fn new(provider_id: impl Into<String>, dimensions: u32) -> Self {
        Self {
            provider_id: ProviderId::new(provider_id),
            dimensions,
            health: HealthStatus::Healthy,
            health_override: None,
            mode: FakeEmbeddingMode::Success,
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_health(mut self, health: HealthStatus) -> Self {
        self.health = health;
        self.health_override = Some(health);
        self
    }

    pub fn with_mode(mut self, mode: FakeEmbeddingMode) -> Self {
        self.mode = mode;
        self
    }

    pub async fn calls(&self) -> Vec<EmbeddingBatch> {
        self.calls.lock().await.clone()
    }

    fn mode_state(&self) -> FakeProviderModeState {
        match self.mode {
            FakeEmbeddingMode::Success => FakeProviderModeState::Success,
            FakeEmbeddingMode::Timeout => FakeProviderModeState::Timeout,
            FakeEmbeddingMode::RateLimited => FakeProviderModeState::RateLimited,
            FakeEmbeddingMode::Fatal => FakeProviderModeState::Fatal,
        }
    }

    fn error(&self, code: &str, message: &str) -> ApiError {
        let mut error = ApiError::new(code, axon_error::ErrorStage::Embedding, message)
            .with_provider_id(self.provider_id.0.clone());
        if self.mode == FakeEmbeddingMode::Fatal {
            error.retryable = false;
        }
        error
    }

    fn capability_state(&self) -> FakeProviderCapabilityState {
        let mut state = fake_provider_capability_state(
            self.mode_state(),
            &self.provider_id.0,
            axon_error::ErrorStage::Embedding,
            "embedding provider",
        );
        if let Some(health) = self.health_override.filter(|health| {
            self.mode == FakeEmbeddingMode::Success || *health != HealthStatus::Healthy
        }) {
            state.health = health;
        }
        state
    }
}

#[async_trait]
impl EmbeddingProvider for FakeEmbeddingProvider {
    async fn embed(&self, batch: EmbeddingBatch) -> Result<EmbeddingResult> {
        self.calls.lock().await.push(batch.clone());
        match self.mode {
            FakeEmbeddingMode::Success => {}
            FakeEmbeddingMode::Timeout => {
                return Err(self.error("provider.timeout", "embedding provider timed out"));
            }
            FakeEmbeddingMode::RateLimited => {
                return Err(self.error("provider.rate_limited", "embedding provider rate limited"));
            }
            FakeEmbeddingMode::Fatal => {
                return Err(self.error("provider.fatal", "embedding provider failed fatally"));
            }
        }

        let vectors = batch
            .items
            .iter()
            .map(|item| EmbeddingVector {
                chunk_id: item.chunk_id.clone(),
                values: deterministic_vector(&item.chunk_id.0, self.dimensions),
            })
            .collect();

        Ok(EmbeddingResult {
            batch_id: batch.batch_id,
            model: "fake-embedding".to_string(),
            dimensions: self.dimensions,
            vectors,
            usage: ProviderUsage {
                input_tokens: Some(batch.items.len() as u64),
                output_tokens: None,
                requests: 1,
                duration_ms: 0,
            },
            warnings: Vec::new(),
        })
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        let state = self.capability_state();
        Ok(ProviderCapability {
            provider_id: self.provider_id.clone(),
            provider_kind: ProviderKind::Embedding,
            implementation: "fake".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            health: state.health,
            limits: ProviderLimits {
                max_concurrency: Some(2),
                max_batch_size: Some(128),
                interactive_reserved_concurrency: Some(1),
                background_max_concurrency: Some(1),
                ..ProviderLimits::default()
            },
            features: vec!["deterministic".to_string(), "call_recording".to_string()],
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
                available_units: 2,
                oldest_queued_ms: None,
                priority_breakdown: Default::default(),
                states: vec![ReservationState::Granted],
            },
            cost_class: ProviderCostClass::Internal,
            degraded_modes: Vec::new(),
            fake_overrides_supported: true,
            embedding: Some(EmbeddingProviderCapability {
                model_id: "fake-embedding".to_string(),
                dimensions: self.dimensions,
                max_input_tokens: 8192,
                max_batch_tokens: 65_536,
                instruction_support: InstructionSupport::QueryAndDocument,
                sparse_output: false,
                batch_limits: BatchLimits {
                    max_items: 128,
                    max_tokens: 65_536,
                    max_bytes: None,
                },
            }),
            llm: None,
            vector_store: None,
            fetch: None,
            render: None,
            credential: None,
        })
    }
}

fn deterministic_vector(seed: &str, dimensions: u32) -> Vec<f32> {
    let mut state = seed.bytes().fold(0x811c9dc5u32, |acc, byte| {
        acc.wrapping_mul(16_777_619) ^ u32::from(byte)
    });
    (0..dimensions)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state % 10_000) as f32 / 10_000.0
        })
        .collect()
}
