//! TEI embedding provider shell.

use std::time::Duration;

use async_trait::async_trait;
use axon_api::source::*;

use crate::capability::{EmbeddingCapabilityConfig, unavailable_embedding_provider_capability};
use crate::provider::{EmbeddingProvider, Result, not_wired_error};

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
}

#[async_trait]
impl EmbeddingProvider for TeiEmbeddingProvider {
    async fn embed(&self, _batch: EmbeddingBatch) -> Result<EmbeddingResult> {
        Err(not_wired_error("tei", "TEI"))
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        Ok(unavailable_embedding_provider_capability(
            ProviderId::new("tei"),
            "tei",
            ProviderLimits {
                max_batch_size: Some(self.config.max_batch_inputs),
                timeout_ms: Some(self.config.timeout.as_millis() as u64),
                ..ProviderLimits::default()
            },
            vec!["dense_embeddings".to_string(), "http_shell".to_string()],
            not_wired_error("tei", "TEI"),
            ProviderCostClass::Internal,
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
