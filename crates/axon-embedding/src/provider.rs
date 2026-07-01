//! Embedding provider boundary.

use async_trait::async_trait;
use axon_api::source::{ApiError, EmbeddingBatch, EmbeddingResult, ProviderCapability};
use axon_error::ErrorStage;

pub type Result<T> = std::result::Result<T, ApiError>;

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, batch: EmbeddingBatch) -> Result<EmbeddingResult>;
    async fn capabilities(&self) -> Result<ProviderCapability>;
}

pub fn not_wired_error(provider_id: &str, implementation: &str) -> ApiError {
    ApiError::new(
        "provider.not_wired",
        ErrorStage::Embedding,
        format!("{implementation} embedding provider is not wired to runtime yet"),
    )
    .with_provider_id(provider_id)
}
