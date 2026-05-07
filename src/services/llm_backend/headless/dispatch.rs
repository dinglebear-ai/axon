use std::error::Error as StdError;

use crate::services::llm_backend::{CompletionRequest, CompletionResponse};

use super::gemini;

pub async fn complete_text(
    req: CompletionRequest,
) -> Result<CompletionResponse, Box<dyn StdError + Send + Sync>> {
    complete_streaming(req, |_| Ok(())).await
}

pub async fn complete_streaming<F>(
    req: CompletionRequest,
    on_delta: F,
) -> Result<CompletionResponse, Box<dyn StdError + Send + Sync>>
where
    F: FnMut(&str) -> Result<(), Box<dyn StdError + Send + Sync>> + Send,
{
    gemini::complete_streaming(req, on_delta).await
}

pub fn validate_selected_agent() -> Result<(), Box<dyn StdError + Send + Sync>> {
    gemini::validate_command()
}
