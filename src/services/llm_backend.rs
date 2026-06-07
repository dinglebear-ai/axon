use std::error::Error as StdError;

pub mod concurrency;
pub mod headless;
pub mod openai_compat;
pub mod types;

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
    }
}

fn completion_limiter_key(req: &CompletionRequest) -> String {
    match req.backend.kind {
        LlmBackendKind::GeminiHeadless => format!(
            "gemini:{}:{}",
            req.backend.gemini_cmd,
            req.backend.gemini_model.as_deref().unwrap_or_default()
        ),
        LlmBackendKind::OpenAiCompat => format!(
            "openai:{}:{}",
            req.backend.openai_base_url.as_deref().unwrap_or_default(),
            req.backend.openai_model.as_deref().unwrap_or_default()
        ),
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
mod tests {
    use super::*;

    #[test]
    fn limiter_key_uses_configured_backend_model_not_request_override() {
        let backend = LlmBackendConfig {
            kind: LlmBackendKind::OpenAiCompat,
            openai_base_url: Some("http://127.0.0.1:8080/v1".to_string()),
            openai_model: Some("default-model".to_string()),
            configured: true,
            ..LlmBackendConfig::default()
        };
        let mut req = CompletionRequest::new("hello").model("override-model");
        req.backend = backend;

        assert_eq!(
            completion_limiter_key(&req),
            "openai:http://127.0.0.1:8080/v1:default-model"
        );
    }

    #[test]
    fn limiter_key_falls_back_to_backend_model() {
        let backend = LlmBackendConfig {
            kind: LlmBackendKind::GeminiHeadless,
            gemini_cmd: "gemini".to_string(),
            gemini_model: Some("configured-model".to_string()),
            configured: true,
            ..LlmBackendConfig::default()
        };
        let mut req = CompletionRequest::new("hello");
        req.backend = backend;

        assert_eq!(
            completion_limiter_key(&req),
            "gemini:gemini:configured-model"
        );
    }
}
