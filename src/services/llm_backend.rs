use std::error::Error as StdError;

pub mod concurrency;
pub mod headless;
pub mod types;

pub use types::{
    CompletionRequest, CompletionResponse, CompletionRunner, CompletionTurnResult, UsageSnapshot,
    extract_completion_result, normalize_stream_flag,
};

pub async fn complete_text(
    req: CompletionRequest,
) -> Result<CompletionResponse, Box<dyn StdError + Send + Sync>> {
    let _permit = concurrency::acquire_completion_permit().await?;
    headless::dispatch::complete_text(req).await
}

pub async fn complete_streaming<F>(
    req: CompletionRequest,
    on_delta: F,
) -> Result<CompletionResponse, Box<dyn StdError + Send + Sync>>
where
    F: FnMut(&str) -> Result<(), Box<dyn StdError + Send + Sync>> + Send,
{
    let _permit = concurrency::acquire_completion_permit().await?;
    headless::dispatch::complete_streaming(req, on_delta).await
}

pub async fn complete_text_with_runner<R>(
    runner: &R,
    req: CompletionRequest,
) -> Result<CompletionResponse, Box<dyn StdError + Send + Sync>>
where
    R: CompletionRunner + ?Sized,
{
    let turn_result = runner
        .complete_text(normalize_stream_flag(req, false))
        .await?;
    Ok(extract_completion_result(turn_result))
}

pub async fn complete_streaming_with_runner<R, F>(
    runner: &R,
    req: CompletionRequest,
    mut on_delta: F,
) -> Result<CompletionResponse, Box<dyn StdError + Send + Sync>>
where
    R: CompletionRunner + ?Sized,
    F: FnMut(&str) -> Result<(), Box<dyn StdError + Send + Sync>> + Send,
{
    let turn_result = runner
        .complete_streaming(normalize_stream_flag(req, true), &mut on_delta)
        .await?;
    Ok(extract_completion_result(turn_result))
}
