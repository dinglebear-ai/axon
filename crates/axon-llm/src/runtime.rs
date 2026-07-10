use std::error::Error as StdError;

use crate::reservation;

pub mod codex_app_server;
pub mod completer;
pub mod concurrency;
pub mod doctor_probe;
pub mod headless;
pub mod openai_compat;

pub(crate) use concurrency::CompletionKey;

// The LLM DTO + config-derived types live in `axon-core` (they are embedded in
// `Config` and consumed by config parsing/tuning). This crate owns the executing
// backends and re-exports the shared types so callers can reach them through a
// single `axon_llm` surface.
pub use axon_core::llm::{
    CompletionRequest, CompletionResponse, CompletionRunner, CompletionTurnResult,
    LlmBackendConfig, LlmBackendKind, LlmModelPurpose, ReasoningEffort, SynthesisModelProfile,
    SynthesisModelTier, UsageSnapshot, configured_chat_model_from_config,
    configured_model_for_config, configured_model_from_config, extract_completion_result,
    normalize_stream_flag,
};

/// Dispatch a non-streaming completion to the configured backend.
///
/// Acquires an `llm`-pool reservation (see [`reservation`]) before the
/// per-backend concurrency permit and backend dispatch, and records the
/// outcome — this is the single choke point every LLM caller in the process
/// passes through, so it is where the LLM provider contract's "LLM calls use
/// `llm` reservations and cannot consume embedding capacity" is enforced.
pub async fn complete_text(
    req: CompletionRequest,
) -> Result<CompletionResponse, Box<dyn StdError + Send + Sync>> {
    ensure_configured(&req)?;
    let _reservation = reservation::reserve().await?;
    let limiter_key = completion_limiter_key(&req);
    let _permit = concurrency::acquire_completion_permit_for_key(
        limiter_key,
        req.backend.completion_concurrency,
    )
    .await?;
    let result = match req.backend.kind {
        LlmBackendKind::GeminiHeadless => headless::gemini::complete_text(req).await,
        LlmBackendKind::OpenAiCompat => openai_compat::complete_text(req).await,
        LlmBackendKind::CodexAppServer => codex_app_server::complete_text(req).await,
    };
    record_completion_outcome(&result).await;
    result
}

/// Streaming counterpart of [`complete_text`] — same reservation/outcome
/// wiring around the per-backend streaming dispatch.
pub async fn complete_streaming<F>(
    req: CompletionRequest,
    on_delta: F,
) -> Result<CompletionResponse, Box<dyn StdError + Send + Sync>>
where
    F: FnMut(&str) -> Result<(), Box<dyn StdError + Send + Sync>> + Send,
{
    ensure_configured(&req)?;
    let _reservation = reservation::reserve().await?;
    let limiter_key = completion_limiter_key(&req);
    let _permit = concurrency::acquire_completion_permit_for_key(
        limiter_key,
        req.backend.completion_concurrency,
    )
    .await?;
    let result = match req.backend.kind {
        LlmBackendKind::GeminiHeadless => headless::gemini::complete_streaming(req, on_delta).await,
        LlmBackendKind::OpenAiCompat => openai_compat::complete_streaming(req, on_delta).await,
        LlmBackendKind::CodexAppServer => codex_app_server::complete_streaming(req, on_delta).await,
    };
    record_completion_outcome(&result).await;
    result
}

/// Fold a completion outcome into the LLM reservation pool's health/cooldown
/// state. Backend errors are opaque `Box<dyn StdError>` (no structured
/// retryable classification is available across all three backends), so any
/// failure is treated as retryable — transient timeouts/rate-limits/process
/// crashes are the overwhelmingly common case, and a false-positive cooldown
/// self-heals after `LLM_COOLDOWN_SECS`.
async fn record_completion_outcome(
    result: &Result<CompletionResponse, Box<dyn StdError + Send + Sync>>,
) {
    match result {
        Ok(_) => reservation::record_success().await,
        Err(err) => {
            reservation::record_failure(err.to_string(), true).await;
        }
    }
}

pub(crate) fn completion_limiter_key(req: &CompletionRequest) -> CompletionKey {
    match req.backend.kind {
        LlmBackendKind::GeminiHeadless => CompletionKey::Gemini {
            cmd: req.backend.gemini_cmd.clone(),
            model: completion_model(req, req.backend.gemini_model.as_deref().unwrap_or_default()),
        },
        LlmBackendKind::OpenAiCompat => CompletionKey::OpenAi {
            base_url: req.backend.openai_base_url.clone().unwrap_or_default(),
            model: completion_model(req, req.backend.openai_model.as_deref().unwrap_or_default()),
        },
        LlmBackendKind::CodexAppServer => CompletionKey::Codex {
            cmd: req.backend.codex_cmd.clone(),
            model: completion_model(req, req.backend.codex_model.as_deref().unwrap_or_default()),
        },
    }
}

fn completion_model(req: &CompletionRequest, backend_default: &str) -> String {
    req.model
        .as_deref()
        .filter(|model| !model.trim().is_empty())
        .unwrap_or(backend_default)
        .to_string()
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
#[path = "runtime_tests.rs"]
mod tests;
