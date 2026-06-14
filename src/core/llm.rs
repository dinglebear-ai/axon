use std::error::Error as StdError;

pub mod codex_app_server;
pub mod concurrency;
pub mod headless;
pub mod openai_compat;
pub mod types;

pub(crate) use concurrency::CompletionKey;
pub use types::{
    CompletionRequest, CompletionResponse, CompletionRunner, CompletionTurnResult,
    LlmBackendConfig, LlmBackendKind, LlmModelPurpose, UsageSnapshot,
    configured_chat_model_from_config, configured_model_for_config, configured_model_from_config,
    extract_completion_result, normalize_stream_flag,
};

pub async fn complete_text(
    req: CompletionRequest,
) -> Result<CompletionResponse, Box<dyn StdError + Send + Sync>> {
    ensure_configured(&req)?;
    let limiter_key = completion_limiter_key(&req);
    let _permit = concurrency::acquire_completion_permit_for_key(
        limiter_key,
        req.backend.completion_concurrency,
    )
    .await?;
    match req.backend.kind {
        LlmBackendKind::GeminiHeadless => headless::gemini::complete_text(req).await,
        LlmBackendKind::OpenAiCompat => openai_compat::complete_text(req).await,
        LlmBackendKind::CodexAppServer => codex_app_server::complete_text(req).await,
    }
}

pub async fn complete_streaming<F>(
    req: CompletionRequest,
    on_delta: F,
) -> Result<CompletionResponse, Box<dyn StdError + Send + Sync>>
where
    F: FnMut(&str) -> Result<(), Box<dyn StdError + Send + Sync>> + Send,
{
    ensure_configured(&req)?;
    let limiter_key = completion_limiter_key(&req);
    let _permit = concurrency::acquire_completion_permit_for_key(
        limiter_key,
        req.backend.completion_concurrency,
    )
    .await?;
    match req.backend.kind {
        LlmBackendKind::GeminiHeadless => headless::gemini::complete_streaming(req, on_delta).await,
        LlmBackendKind::OpenAiCompat => openai_compat::complete_streaming(req, on_delta).await,
        LlmBackendKind::CodexAppServer => codex_app_server::complete_streaming(req, on_delta).await,
    }
}

pub(crate) fn completion_limiter_key(req: &CompletionRequest) -> CompletionKey {
    match req.backend.kind {
        LlmBackendKind::GeminiHeadless => CompletionKey::Gemini {
            cmd: req.backend.gemini_cmd.clone(),
            model: req.backend.gemini_model.clone().unwrap_or_default(),
        },
        LlmBackendKind::OpenAiCompat => CompletionKey::OpenAi {
            base_url: req.backend.openai_base_url.clone().unwrap_or_default(),
            model: req.backend.openai_model.clone().unwrap_or_default(),
        },
        LlmBackendKind::CodexAppServer => CompletionKey::Codex {
            cmd: req.backend.codex_cmd.clone(),
            model: req
                .model
                .clone()
                .or_else(|| req.backend.codex_model.clone())
                .unwrap_or_default(),
        },
    }
}

fn ensure_configured(req: &CompletionRequest) -> Result<(), Box<dyn StdError + Send + Sync>> {
    req.backend
        .configured
        .then_some(())
        .ok_or_else(|| "LLM completion request is missing resolved backend config".into())
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

#[cfg(test)]
#[path = "llm_backend_tests.rs"]
mod tests;
