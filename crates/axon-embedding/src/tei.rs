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
use crate::capability::{EmbeddingCapabilityConfig, available_embedding_provider_capability};
use crate::provider::{EmbeddingProvider, Result};
use client::{TeiClient, TeiClientParams};

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
}

/// Total attempts per request = 1 initial + 5 retries, matching the legacy
/// client's default `tei_max_retries = 5`.
const MAX_ATTEMPTS: usize = 6;

#[derive(Debug, Clone)]
pub struct TeiEmbeddingProvider {
    config: TeiEmbeddingConfig,
}

impl TeiEmbeddingProvider {
    pub fn new(config: TeiEmbeddingConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &TeiEmbeddingConfig {
        &self.config
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
        let client = TeiClient::new(TeiClientParams {
            endpoint: self.config.endpoint.clone(),
            provider_id: "tei".to_string(),
            max_batch_inputs: self.config.max_batch_inputs.max(1) as usize,
            max_attempts: MAX_ATTEMPTS,
            request_timeout: self.config.timeout,
        })?;

        let started = Instant::now();
        let outcome = client.embed_all(&texts).await?;
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

    async fn capabilities(&self) -> Result<ProviderCapability> {
        Ok(available_embedding_provider_capability(
            ProviderId::new("tei"),
            "tei",
            ProviderLimits {
                max_batch_size: Some(self.config.max_batch_inputs),
                timeout_ms: Some(self.config.timeout.as_millis() as u64),
                ..ProviderLimits::default()
            },
            vec!["dense_embeddings".to_string(), "http_client".to_string()],
            ProviderCostClass::Internal,
            self.config.max_batch_inputs,
            EmbeddingCapabilityConfig {
                model_id: self.config.model.clone(),
                dimensions: self.config.dimensions,
                max_input_tokens: self.config.max_input_tokens,
                max_batch_tokens: self.config.max_batch_tokens,
                instruction_support: self.config.instruction_support,
                sparse_output: false,
                max_batch_items: self.config.max_batch_inputs,
                max_batch_bytes: None,
            },
        ))
    }
}
