//! Embedding provider boundary.

use async_trait::async_trait;
use axon_api::source::{ApiError, EmbeddingBatch, EmbeddingResult, ProviderCapability};

pub type Result<T> = std::result::Result<T, ApiError>;

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, batch: EmbeddingBatch) -> Result<EmbeddingResult>;
    async fn capabilities(&self) -> Result<ProviderCapability>;
}
