//! LLM provider boundary.

use async_trait::async_trait;
use axon_api::source::{ApiError, ProviderCapability};

use crate::completion::{LlmCompletionRequest, LlmCompletionResponse};
use crate::stream::LlmDeltaSink;

pub type Result<T> = std::result::Result<T, ApiError>;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, request: LlmCompletionRequest) -> Result<LlmCompletionResponse>;

    async fn complete_streaming(
        &self,
        request: LlmCompletionRequest,
        on_delta: LlmDeltaSink<'_>,
    ) -> Result<LlmCompletionResponse>;

    async fn capabilities(&self) -> Result<ProviderCapability>;
}

#[cfg(test)]
#[path = "provider_tests.rs"]
mod provider_tests;
