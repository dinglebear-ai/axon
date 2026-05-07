use std::error::Error as StdError;

use crate::crates::services::acp_llm::{AcpCompletionRequest, AcpCompletionResponse};

use super::{HeadlessAgent, claude, codex, gemini};

pub fn resolve_agent() -> HeadlessAgent {
    match std::env::var("AXON_ASK_AGENT")
        .unwrap_or_else(|_| "claude".to_string())
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "codex" => HeadlessAgent::Codex,
        "gemini" => HeadlessAgent::Gemini,
        _ => HeadlessAgent::Claude,
    }
}

pub async fn complete_text(
    req: AcpCompletionRequest,
) -> Result<AcpCompletionResponse, Box<dyn StdError>> {
    complete_streaming(req, |_| Ok(())).await
}

pub async fn complete_streaming<F>(
    req: AcpCompletionRequest,
    on_delta: F,
) -> Result<AcpCompletionResponse, Box<dyn StdError>>
where
    F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send,
{
    match resolve_agent() {
        HeadlessAgent::Claude => claude::complete_streaming(req, on_delta).await,
        HeadlessAgent::Codex => Err(
            "Codex headless is unavailable: current CLI has no proven no-tool synthesis posture"
                .into(),
        ),
        HeadlessAgent::Gemini => Err(
            "Gemini headless is unavailable: current CLI has no proven no-tool synthesis posture"
                .into(),
        ),
    }
}

pub fn validate_selected_agent() -> Result<(), Box<dyn StdError>> {
    match resolve_agent() {
        HeadlessAgent::Claude => Ok(()),
        HeadlessAgent::Codex if codex::safe_posture_available() => Ok(()),
        HeadlessAgent::Gemini if gemini::safe_posture_available() => Ok(()),
        HeadlessAgent::Codex => Err(
            "AXON_ASK_AGENT=codex is unavailable for headless backend: no proven no-tool posture"
                .into(),
        ),
        HeadlessAgent::Gemini => Err(
            "AXON_ASK_AGENT=gemini is unavailable for headless backend: no proven no-tool posture"
                .into(),
        ),
    }
}
