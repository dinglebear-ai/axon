//! Concrete [`TextCompleter`] backed by the backend-dispatching [`complete_text`].
//!
//! `axon-core` owns the LLM DTO/config types and the [`TextCompleter`] boundary
//! but not the executing backends. This adapter lets `axon-core`-internal code
//! (the extract LLM fallback) run a completion through the real backends via an
//! injected `Arc<dyn TextCompleter>`, without `axon-core` depending on
//! `axon-llm`.

use std::error::Error as StdError;
use std::sync::Arc;

use async_trait::async_trait;
use axon_core::llm::{CompletionRequest, CompletionResponse, TextCompleter};

use crate::runtime::complete_text;

/// Text completer that dispatches to the configured backend.
#[derive(Debug, Clone, Copy, Default)]
pub struct BackendTextCompleter;

#[async_trait]
impl TextCompleter for BackendTextCompleter {
    async fn complete_text(
        &self,
        req: CompletionRequest,
    ) -> Result<CompletionResponse, Box<dyn StdError + Send + Sync>> {
        complete_text(req).await
    }
}

/// Shared [`TextCompleter`] handle for injection into `axon-core` extraction.
#[must_use]
pub fn backend_text_completer() -> Arc<dyn TextCompleter> {
    Arc::new(BackendTextCompleter)
}
