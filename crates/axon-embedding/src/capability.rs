//! Provider capability constructors for embedding providers.

use axon_api::source::*;

#[derive(Debug, Clone)]
pub struct EmbeddingCapabilityConfig {
    pub model_id: String,
    pub dimensions: u32,
    pub max_input_tokens: u32,
    pub max_batch_tokens: u32,
    pub instruction_support: InstructionSupport,
    pub sparse_output: bool,
    pub max_batch_items: u32,
    pub max_batch_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ProviderCapabilityConfig {
    pub provider_id: ProviderId,
    pub implementation: String,
    pub health: HealthStatus,
    pub limits: ProviderLimits,
    pub features: Vec<String>,
    pub cooldown_until: Option<Timestamp>,
    pub last_error: Option<ApiError>,
    pub reservation_policy: ReservationPolicy,
    pub reservation_state: ReservationStateSnapshot,
    pub cost_class: ProviderCostClass,
    pub degraded_modes: Vec<DegradedMode>,
    pub fake_overrides_supported: bool,
    pub embedding: EmbeddingProviderCapability,
}

pub fn embedding_capability(config: EmbeddingCapabilityConfig) -> EmbeddingProviderCapability {
    EmbeddingProviderCapability {
        model_id: config.model_id,
        dimensions: config.dimensions,
        max_input_tokens: config.max_input_tokens,
        max_batch_tokens: config.max_batch_tokens,
        instruction_support: config.instruction_support,
        sparse_output: config.sparse_output,
        batch_limits: BatchLimits {
            max_items: config.max_batch_items,
            max_tokens: config.max_batch_tokens,
            max_bytes: config.max_batch_bytes,
        },
    }
}

pub fn embedding_provider_capability(config: ProviderCapabilityConfig) -> ProviderCapability {
    ProviderCapability {
        provider_id: config.provider_id,
        provider_kind: ProviderKind::Embedding,
        implementation: config.implementation,
        version: env!("CARGO_PKG_VERSION").to_string(),
        health: config.health,
        limits: config.limits,
        features: config.features,
        cooldown_until: config.cooldown_until,
        last_error: config.last_error,
        reservation_policy: config.reservation_policy,
        reservation_state: config.reservation_state,
        cost_class: config.cost_class,
        degraded_modes: config.degraded_modes,
        fake_overrides_supported: config.fake_overrides_supported,
        embedding: Some(config.embedding),
        llm: None,
        vector_store: None,
        fetch: None,
        render: None,
        credential: None,
    }
}

pub fn unavailable_embedding_provider_capability(
    provider_id: ProviderId,
    implementation: impl Into<String>,
    limits: ProviderLimits,
    features: Vec<String>,
    last_error: ApiError,
    cost_class: ProviderCostClass,
    embedding: EmbeddingCapabilityConfig,
) -> ProviderCapability {
    embedding_provider_capability(ProviderCapabilityConfig {
        provider_id,
        implementation: implementation.into(),
        health: HealthStatus::Unavailable,
        limits,
        features,
        cooldown_until: None,
        last_error: Some(last_error),
        reservation_policy: embedding_reservation_policy(false, QueuePolicy::Fifo, 0),
        reservation_state: embedding_reservation_state(0),
        cost_class,
        degraded_modes: Vec::new(),
        fake_overrides_supported: false,
        embedding: embedding_capability(embedding),
    })
}

/// Build an `Available`/`Healthy` embedding capability for a wired provider.
///
/// Mirrors [`unavailable_embedding_provider_capability`] but reports the
/// provider as reachable: `Healthy` health, no `last_error`, and a reservation
/// state with `capacity` granted units. No network call is performed — the
/// capability reflects static config so it stays unit-testable.
#[allow(clippy::too_many_arguments)]
pub fn available_embedding_provider_capability(
    provider_id: ProviderId,
    implementation: impl Into<String>,
    limits: ProviderLimits,
    features: Vec<String>,
    cost_class: ProviderCostClass,
    capacity: u32,
    embedding: EmbeddingCapabilityConfig,
) -> ProviderCapability {
    embedding_provider_capability(ProviderCapabilityConfig {
        provider_id,
        implementation: implementation.into(),
        health: HealthStatus::Healthy,
        limits,
        features,
        cooldown_until: None,
        last_error: None,
        reservation_policy: embedding_reservation_policy(true, QueuePolicy::Fifo, 0),
        reservation_state: embedding_reservation_state(capacity),
        cost_class,
        degraded_modes: Vec::new(),
        fake_overrides_supported: false,
        embedding: embedding_capability(embedding),
    })
}

pub fn embedding_reservation_policy(
    supports_reservations: bool,
    queue_policy: QueuePolicy,
    interactive_reserve: u32,
) -> ReservationPolicy {
    ReservationPolicy {
        supports_reservations,
        queue_policy,
        interactive_reserve,
        cooldown_after_failures: 1,
        cooldown_secs: 30,
        retry_backoff_ms: Some(100),
    }
}

pub fn embedding_reservation_state(capacity: u32) -> ReservationStateSnapshot {
    ReservationStateSnapshot {
        queued: 0,
        active: 0,
        available_units: capacity,
        oldest_queued_ms: None,
        priority_breakdown: Default::default(),
        states: if capacity == 0 {
            vec![ReservationState::Failed]
        } else {
            vec![ReservationState::Granted]
        },
    }
}
