//! OpenAI-compatible embedding provider shell.

use std::time::Duration;

use async_trait::async_trait;
use axon_api::source::*;

use crate::capability::{EmbeddingCapabilityConfig, unavailable_embedding_provider_capability};
use crate::provider::{EmbeddingProvider, Result, not_wired_error};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCompatConfig {
    pub base_url: String,
    pub model: String,
    pub dimensions: u32,
    pub timeout: Duration,
    pub max_batch_inputs: u32,
    pub max_input_tokens: u32,
    pub max_batch_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct OpenAiCompatEmbeddingProvider {
    config: OpenAiCompatConfig,
}

impl OpenAiCompatEmbeddingProvider {
    pub fn new(config: OpenAiCompatConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &OpenAiCompatConfig {
        &self.config
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAiCompatEmbeddingProvider {
    async fn embed(&self, _batch: EmbeddingBatch) -> Result<EmbeddingResult> {
        Err(not_wired_error("openai-compat", "OpenAI-compatible"))
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        Ok(unavailable_embedding_provider_capability(
            ProviderId::new("openai-compat"),
            "openai-compat",
            ProviderLimits {
                max_batch_size: Some(self.config.max_batch_inputs),
                timeout_ms: Some(self.config.timeout.as_millis() as u64),
                ..ProviderLimits::default()
            },
            vec![
                "dense_embeddings".to_string(),
                "openai_compatible".to_string(),
                "http_shell".to_string(),
            ],
            not_wired_error("openai-compat", "OpenAI-compatible"),
            ProviderCostClass::Unknown,
            EmbeddingCapabilityConfig {
                model_id: self.config.model.clone(),
                dimensions: self.config.dimensions,
                max_input_tokens: self.config.max_input_tokens,
                max_batch_tokens: self.config.max_batch_tokens,
                instruction_support: InstructionSupport::None,
                sparse_output: false,
                max_batch_items: self.config.max_batch_inputs,
                max_batch_bytes: None,
            },
        ))
    }
}
