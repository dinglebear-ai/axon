//! TEI embedding provider — real reqwest-backed `/embed` client.
//!
//! Ports request/response shape, 413 batch-split, and 429/5xx retry behaviour
//! from the legacy `axon-vector` TEI client. The HTTP transport lives in
//! [`client`]; this module owns batch validation, instruction prefixing, order
//! preservation, dimension validation, and capability reporting.

mod client;

use std::time::{Duration, Instant};

use async_trait::async_trait;
use axon_api::source::*;
use axon_error::ErrorStage;

use crate::batch::validate_batch;
use crate::capability::{
    EmbeddingCapabilityConfig, ProviderCapabilityConfig, embedding_capability,
    embedding_provider_capability, embedding_reservation_policy, embedding_reservation_state,
};
use crate::provider::{EmbeddingProvider, Result};
use crate::reservation::{ProviderReservationConfig, ProviderReservationManager};
use client::{TeiClient, TeiClientParams};

/// Provider-derived embedding identity: the `model_id` reported by TEI `/info`
/// and the output dimensionality measured with a probe embed.
///
/// Returned by [`TeiEmbeddingProvider::derive_embedding_identity`]. Callers use
/// this to stamp the vector-payload `embedding_model`/`embedding_dimensions`
/// fields and size the Qdrant collection from the live provider rather than a
/// hardcoded constant, falling back to config only when the provider is
/// unreachable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedEmbeddingIdentity {
    pub model: String,
    pub dimensions: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeiEmbeddingConfig {
    pub endpoint: String,
    pub model: String,
    pub dimensions: u32,
    pub timeout: Duration,
    pub max_batch_inputs: u32,
    pub max_input_tokens: u32,
    pub max_batch_tokens: u32,
    pub instruction_support: InstructionSupport,
    /// Base backoff (ms), before exponential growth + jitter, between retried
    /// `/embed` requests on 429/5xx. Config: `[providers.embedding].retry-backoff-ms`.
    pub retry_backoff_ms: u64,
    /// Total attempts per request = `Config::tei_max_retries + 1` (caller
    /// computes this — see `axon-services::context::target_runtime`).
    /// Config: `[providers.embedding].max-retries`, env `TEI_MAX_RETRIES`.
    /// Clamped to at least 1 by [`TeiEmbeddingProvider::new`].
    pub max_attempts: usize,
}

/// Deterministic single-token input used to measure the provider's output
/// dimensionality (TEI `/info` does not expose it). A stable input keeps the
/// measured length reproducible across probes.
const DIMENSION_PROBE_INPUT: &str = "axon";

/// Self-tracked health/cooldown capacity, independent of any scheduler-side
/// reservation pool the caller may layer on top. Sized generously (well above
/// any realistic in-flight batch count) — it exists purely to fold live
/// `record_success`/`record_failure` outcomes into `capabilities()`, not to
/// gate concurrency.
///
/// The trip threshold is intentionally hardcoded at 1 (not driven by
/// `[providers.embedding].cooldown-after-failures`): a single `embed()` call
/// already exhausts the full configured `max_attempts` retry budget
/// internally before `record_failure` is invoked once, so by the time this
/// tracker sees a failure at all, the provider has already proven itself
/// unhealthy across several real HTTP attempts — see the contract test
/// `embed_retry_exhaustion_cools_the_provider_and_capabilities_report_it_live`
/// (provider-contract F5-10..13/V01/V03), which requires cooling on that
/// first recorded failure. `cooldown-after-failures`/`cooldown-secs` DO drive
/// the separate scheduler-facing reservation pool in
/// `axon-services::context::target_runtime::embedding_reservation_config`,
/// which is the "avoid bottlenecks" admission-control knob the config
/// contract describes; this tracker is a health *report*, not a scheduling
/// gate, so it keeps its own fixed invariant.
const HEALTH_TRACKER_CAPACITY: u32 = 1_000_000;
const HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES: u32 = 1;
const HEALTH_TRACKER_COOLDOWN_SECS: u64 = 30;

#[derive(Debug, Clone)]
pub struct TeiEmbeddingProvider {
    config: TeiEmbeddingConfig,
    health: ProviderReservationManager,
    max_attempts: usize,
}

impl TeiEmbeddingProvider {
    pub fn new(config: TeiEmbeddingConfig) -> Self {
        let health = ProviderReservationManager::new(ProviderReservationConfig {
            provider_id: ProviderId::new("tei"),
            provider_kind: ProviderKind::Embedding,
            capacity: HEALTH_TRACKER_CAPACITY,
            interactive_reserve: 0,
            cooldown_after_failures: HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES,
            cooldown_secs: HEALTH_TRACKER_COOLDOWN_SECS,
        });
        // At least 1 attempt regardless of a misconfigured/zero `max_attempts`.
        let max_attempts = config.max_attempts.max(1);
        Self {
            config,
            health,
            max_attempts,
        }
    }

    /// Override the retry-attempt budget (production threads
    /// `TeiEmbeddingConfig::max_attempts`, computed by the caller from
    /// `cfg.tei_max_retries + 1`). Lets tests exercise retry-exhaustion/cooling
    /// deterministically without waiting out the real exponential backoff —
    /// see `tei_client_tests.rs`.
    #[cfg(test)]
    pub(crate) fn with_max_attempts(mut self, max_attempts: usize) -> Self {
        self.max_attempts = max_attempts;
        self
    }

    pub fn config(&self) -> &TeiEmbeddingConfig {
        &self.config
    }

    /// Derive the embedding model + dimensions from the live TEI endpoint.
    ///
    /// Fetches `/info` for `model_id` and issues a single probe `/embed` to
    /// measure the true output dimensionality (TEI does not report dimensions in
    /// `/info`). Any transport failure is surfaced to the caller, which falls
    /// back to the configured model/dimensions. Deterministic probe text keeps
    /// the measured length stable.
    pub async fn derive_embedding_identity(&self) -> Result<DerivedEmbeddingIdentity> {
        let client = self.build_client()?;
        let info = client.fetch_info().await?;
        let dimensions = client.probe_dimensions(DIMENSION_PROBE_INPUT).await?;
        let model = info
            .model_id
            .filter(|id| !id.trim().is_empty())
            .unwrap_or_else(|| self.config.model.clone());
        Ok(DerivedEmbeddingIdentity { model, dimensions })
    }

    fn build_client(&self) -> Result<TeiClient> {
        TeiClient::new(TeiClientParams {
            endpoint: self.config.endpoint.clone(),
            provider_id: "tei".to_string(),
            max_batch_inputs: self.config.max_batch_inputs.max(1) as usize,
            max_attempts: self.max_attempts,
            request_timeout: self.config.timeout,
            retry_backoff_base_ms: self.config.retry_backoff_ms,
        })
    }

    fn error(&self, code: &str, message: &str) -> ApiError {
        ApiError::new(code, ErrorStage::Embedding, message.to_string()).with_provider_id("tei")
    }

    /// Whether a batch-level instruction should be applied, given the provider's
    /// configured instruction support. `None` support never applies it.
    fn instruction_enabled(&self) -> bool {
        self.config.instruction_support != InstructionSupport::None
    }

    /// Build the ordered list of request texts, prepending the instruction to
    /// each input when one is present and instruction support permits it.
    fn request_texts(&self, batch: &EmbeddingBatch) -> Vec<String> {
        match &batch.instruction {
            Some(instruction) if self.instruction_enabled() && !instruction.is_empty() => batch
                .items
                .iter()
                .map(|item| format!("{instruction}{}", item.text))
                .collect(),
            _ => batch.items.iter().map(|item| item.text.clone()).collect(),
        }
    }
}

#[async_trait]
impl EmbeddingProvider for TeiEmbeddingProvider {
    async fn embed(&self, batch: EmbeddingBatch) -> Result<EmbeddingResult> {
        validate_batch(&batch)?;

        let dimensions = self.config.dimensions;
        if dimensions == 0 {
            return Err(self.error(
                "provider.invalid_dimensions",
                "TEI provider dimensions must be greater than zero",
            ));
        }

        let texts = self.request_texts(&batch);
        let client = self.build_client()?;

        let started = Instant::now();
        let outcome = match client.embed_all(&texts).await {
            Ok(outcome) => {
                self.health.record_success().await;
                outcome
            }
            Err(err) => {
                self.health
                    .record_failure(err.code.0.clone(), err.retryable)
                    .await;
                return Err(err);
            }
        };
        let duration_ms = started.elapsed().as_millis() as u64;
        let raw = outcome.vectors;
        let requests = outcome.requests;

        if raw.len() != batch.items.len() {
            return Err(self.error(
                "embedding.tei.count_mismatch",
                &format!(
                    "TEI returned {} vectors for {} inputs",
                    raw.len(),
                    batch.items.len()
                ),
            ));
        }

        // Map response vectors back to items by request order — TEI returns
        // embeddings in the order they were submitted.
        let mut vectors = Vec::with_capacity(raw.len());
        let mut warnings = Vec::new();
        for (item, values) in batch.items.iter().zip(raw) {
            if values.len() as u32 != dimensions {
                warnings.push(SourceWarning {
                    code: "embedding.tei.dimension_mismatch".to_string(),
                    severity: Severity::Warning,
                    message: format!(
                        "TEI vector length {} does not match configured dimensions {}",
                        values.len(),
                        dimensions
                    ),
                    source_item_key: None,
                    retryable: false,
                });
            }
            vectors.push(EmbeddingVector {
                chunk_id: item.chunk_id.clone(),
                values,
            });
        }

        Ok(EmbeddingResult {
            batch_id: batch.batch_id,
            job_id: batch.job_id,
            provider_id: ProviderId::new("tei"),
            model: self.config.model.clone(),
            dimensions,
            vectors,
            usage: ProviderUsage {
                input_tokens: None,
                output_tokens: None,
                requests,
                duration_ms,
            },
            warnings,
        })
    }

    /// Reports the provider's **live** health/cooldown, folded in from every
    /// [`embed`](Self::embed) call's `record_success`/`record_failure`
    /// outcome — not a static always-healthy snapshot. A provider mid-cooldown
    /// (see [`client::TeiClient::send_chunk_with_retries`]) reports
    /// `HealthStatus::Cooling` with a populated `cooldown_until` here until the
    /// window elapses or a subsequent call succeeds.
    async fn capabilities(&self) -> Result<ProviderCapability> {
        let health = self.health.health().await;
        let cooldown_until = self.health.cooldown_until().await;
        let last_error = self
            .health
            .cooling_snapshot()
            .await
            .map(|cooling| self.error("provider.cooling", &cooling.reason));
        Ok(embedding_provider_capability(ProviderCapabilityConfig {
            provider_id: ProviderId::new("tei"),
            implementation: "tei".to_string(),
            health,
            limits: ProviderLimits {
                max_batch_size: Some(self.config.max_batch_inputs),
                timeout_ms: Some(self.config.timeout.as_millis() as u64),
                ..ProviderLimits::default()
            },
            features: vec!["dense_embeddings".to_string(), "http_client".to_string()],
            cooldown_until,
            last_error,
            reservation_policy: embedding_reservation_policy(true, QueuePolicy::Fifo, 0),
            reservation_state: embedding_reservation_state(
                if health == HealthStatus::Cooling || health == HealthStatus::Unavailable {
                    0
                } else {
                    self.config.max_batch_inputs
                },
            ),
            cost_class: ProviderCostClass::Internal,
            degraded_modes: Vec::new(),
            fake_overrides_supported: false,
            embedding: embedding_capability(EmbeddingCapabilityConfig {
                model_id: self.config.model.clone(),
                dimensions: self.config.dimensions,
                max_input_tokens: self.config.max_input_tokens,
                max_batch_tokens: self.config.max_batch_tokens,
                instruction_support: self.config.instruction_support,
                sparse_output: false,
                max_batch_items: self.config.max_batch_inputs,
                max_batch_bytes: None,
            }),
        }))
    }
}
