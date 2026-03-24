//! ACP-backed LLM completion gateway.
//!
//! Provides two code paths:
//! - **One-shot** ([`complete_text`], [`complete_streaming`]): spawns a fresh adapter per request.
//! - **Pre-warmed** ([`warm_session`]): starts the adapter eagerly so the first prompt has no cold-start.

mod pool;
mod runner;
mod types;
mod warm;

pub use pool::{init_warm_pool, pool_size, try_checkout};
pub use types::{
    AcpCompletionRequest, AcpCompletionResponse, AcpCompletionRunner, AcpCompletionTurnResult,
    AcpUsageSnapshot, extract_completion_result, normalize_stream_flag,
};
pub use warm::{WarmAcpSession, warm_session};

use std::error::Error as StdError;

use crate::crates::core::config::Config;
use runner::AcpRuntimeCompletionRunner;

pub async fn complete_text(
    cfg: &Config,
    req: AcpCompletionRequest,
) -> Result<AcpCompletionResponse, Box<dyn StdError>> {
    let runner = AcpRuntimeCompletionRunner::from_config(cfg)?;
    complete_text_with_runner(&runner, req).await
}

pub async fn complete_streaming<F>(
    cfg: &Config,
    req: AcpCompletionRequest,
    on_delta: F,
) -> Result<AcpCompletionResponse, Box<dyn StdError>>
where
    F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send,
{
    let runner = AcpRuntimeCompletionRunner::from_config(cfg)?;
    complete_streaming_with_runner(&runner, req, on_delta).await
}

pub async fn complete_text_with_runner<R>(
    runner: &R,
    req: AcpCompletionRequest,
) -> Result<AcpCompletionResponse, Box<dyn StdError>>
where
    R: AcpCompletionRunner + ?Sized,
{
    let turn_result = runner
        .complete_text(normalize_stream_flag(req, false))
        .await?;
    Ok(extract_completion_result(turn_result))
}

pub async fn complete_streaming_with_runner<R, F>(
    runner: &R,
    req: AcpCompletionRequest,
    mut on_delta: F,
) -> Result<AcpCompletionResponse, Box<dyn StdError>>
where
    R: AcpCompletionRunner + ?Sized,
    F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send,
{
    let turn_result = runner
        .complete_streaming(normalize_stream_flag(req, true), &mut on_delta)
        .await?;
    Ok(extract_completion_result(turn_result))
}
