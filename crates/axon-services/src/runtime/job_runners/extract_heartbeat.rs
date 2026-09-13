use std::future::Future;

use axon_api::source::{ApiError, ErrorStage};
use tokio_util::sync::CancellationToken;

use super::AttemptOwnership;

pub(super) async fn drive_extract_with_heartbeat<F, H, HF>(
    extract_fut: F,
    shutdown: &CancellationToken,
    heartbeat_interval: std::time::Duration,
    mut probe: H,
) -> Result<F::Output, ApiError>
where
    F: Future,
    H: FnMut() -> HF,
    HF: Future<Output = AttemptOwnership>,
{
    tokio::pin!(extract_fut);
    let mut heartbeat = tokio::time::interval_at(
        tokio::time::Instant::now() + heartbeat_interval,
        heartbeat_interval,
    );
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        let heartbeat_fut = async {
            heartbeat.tick().await;
            probe().await
        };
        tokio::select! {
            _ = shutdown.cancelled() => return Err(extract_lifecycle_error("extract canceled")),
            result = &mut extract_fut => return Ok(result),
            ownership = heartbeat_fut => {
                if ownership == AttemptOwnership::Lost {
                    return Err(extract_lifecycle_error("extract attempt is no longer active"));
                }
            }
        }
    }
}

fn extract_lifecycle_error(message: &'static str) -> ApiError {
    ApiError::new(
        "job_runner.extract_failed",
        ErrorStage::ParsingContent,
        message,
    )
}

#[cfg(test)]
#[path = "extract_heartbeat_tests.rs"]
mod tests;
