//! ACP-backed LLM completion gateway.
//!
//! Provides two code paths:
//! - **One-shot** ([`complete_text`], [`complete_streaming`]): spawns a fresh adapter per request.
//! - **Pre-warmed** ([`warm_session`]): starts the adapter eagerly so the first prompt has no cold-start.

mod pool;
mod runner;
mod types;
mod warm;
mod ws_runner;

pub use pool::{init_warm_pool, pool_size};
pub use types::{
    AcpCompletionRequest, AcpCompletionResponse, AcpCompletionRunner, AcpCompletionTurnResult,
    AcpUsageSnapshot, extract_completion_result, normalize_stream_flag,
};
pub use warm::{WarmAcpSession, warm_session};

use std::error::Error as StdError;
use std::sync::{Arc, Mutex, OnceLock};

use crate::core::config::Config;
use runner::AcpRuntimeCompletionRunner;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const ACP_COMPLETION_CONCURRENCY_ENV: &str = "AXON_ACP_COMPLETION_CONCURRENCY";
const DEFAULT_ACP_COMPLETION_CONCURRENCY: usize = 4;
static COMPLETION_SEMAPHORE: OnceLock<Arc<Semaphore>> = OnceLock::new();
static PREWARM_IN_FLIGHT: OnceLock<Mutex<usize>> = OnceLock::new();

fn acp_completion_concurrency_limit() -> usize {
    std::env::var(ACP_COMPLETION_CONCURRENCY_ENV)
        .ok()
        .and_then(|raw| raw.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_ACP_COMPLETION_CONCURRENCY)
}

async fn acquire_completion_permit() -> Result<OwnedSemaphorePermit, Box<dyn StdError>> {
    COMPLETION_SEMAPHORE
        .get_or_init(|| Arc::new(Semaphore::new(acp_completion_concurrency_limit())))
        .clone()
        .acquire_owned()
        .await
        .map_err(|e| format!("ACP completion semaphore closed: {e}").into())
}

pub(super) struct PrewarmSlot;

pub(super) fn try_acquire_prewarm_slot() -> Result<PrewarmSlot, Box<dyn StdError>> {
    let limit = acp_completion_concurrency_limit();
    let mut guard = PREWARM_IN_FLIGHT
        .get_or_init(|| Mutex::new(0))
        .lock()
        .expect("prewarm concurrency mutex poisoned");
    if *guard >= limit {
        return Err(format!("ACP prewarm concurrency limit reached ({limit})").into());
    }
    *guard += 1;
    Ok(PrewarmSlot)
}

impl Drop for PrewarmSlot {
    fn drop(&mut self) {
        let mut guard = PREWARM_IN_FLIGHT
            .get_or_init(|| Mutex::new(0))
            .lock()
            .expect("prewarm concurrency mutex poisoned");
        *guard = guard.saturating_sub(1);
    }
}

pub async fn complete_text(
    cfg: &Config,
    req: AcpCompletionRequest,
) -> Result<AcpCompletionResponse, Box<dyn StdError>> {
    let _permit = acquire_completion_permit().await?;
    if cfg.acp_ws_url.is_some() {
        let runner = ws_runner::AcpWsCompletionRunner::from_config(cfg)?;
        return complete_text_with_runner(&runner, req).await;
    }
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
    let _permit = acquire_completion_permit().await?;
    if cfg.acp_ws_url.is_some() {
        let runner = ws_runner::AcpWsCompletionRunner::from_config(cfg)?;
        return complete_streaming_with_runner(&runner, req, on_delta).await;
    }
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

#[cfg(test)]
mod tests {
    use super::{acp_completion_concurrency_limit, try_acquire_prewarm_slot};

    #[test]
    fn prewarm_slots_are_bounded_and_released() {
        let limit = acp_completion_concurrency_limit();
        let slots = (0..limit)
            .map(|_| try_acquire_prewarm_slot().expect("slot should be available"))
            .collect::<Vec<_>>();
        assert!(try_acquire_prewarm_slot().is_err());
        drop(slots);
        assert!(try_acquire_prewarm_slot().is_ok());
    }
}
