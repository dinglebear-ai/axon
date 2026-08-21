//! Shared reservation error mapping and heartbeat support.

use axon_api::source::*;
use axon_jobs::scheduler::{ReservedCallError, SchedulerError};

use crate::context::TargetLocalSourceRuntime;

use super::ProviderCallContext;

pub(super) async fn record_provider_heartbeat(
    runtime: &TargetLocalSourceRuntime,
    context: &ProviderCallContext,
    reservation: Option<ProviderReservationSnapshot>,
) {
    let (Some(phase), Some(counts)) = (context.phase, context.counts.clone()) else {
        return;
    };
    let now = Timestamp::from(chrono::Utc::now());
    if let Err(error) = runtime
        .jobs
        .heartbeat(JobHeartbeat {
            job_id: context.job_id,
            attempt: context.attempt,
            worker_id: Some("source-pipeline".to_string()),
            phase,
            status: LifecycleStatus::Running,
            stage_id: context.stage_id,
            heartbeat_at: now.clone(),
            sequence: 0,
            last_progress_at: Some(now),
            last_event_sequence: None,
            counts: Some(counts),
            provider_reservations: reservation.into_iter().collect(),
        })
        .await
    {
        tracing::warn!(
            job_id = %context.job_id.0,
            phase = ?phase,
            error = %error,
            "failed to persist provider progress heartbeat"
        );
    }
}

pub(super) fn map_reserved<T>(
    result: Result<T, ReservedCallError<ApiError>>,
    stage: ErrorStage,
    provider_id: &str,
) -> Result<T, ApiError> {
    match result {
        Ok(value) => Ok(value),
        Err(ReservedCallError::Provider(error)) => Err(error),
        Err(ReservedCallError::Scheduler(error)) => Err(scheduler_error(error, stage, provider_id)),
    }
}

pub(super) fn scheduler_error(
    error: SchedulerError,
    stage: ErrorStage,
    provider_id: &str,
) -> ApiError {
    let code = match error {
        SchedulerError::RequestTooLarge => "provider.scheduler.request_too_large",
        SchedulerError::QueueFull => "provider.scheduler.queue_full",
        SchedulerError::WaitTimeout => "provider.scheduler.wait_timeout",
        SchedulerError::StaleFence => "provider.scheduler.stale_fence",
        SchedulerError::Queued => "provider.scheduler.queued",
        SchedulerError::Database(_) => "provider.scheduler.database",
    };
    ApiError::new(code, stage, error.to_string()).with_provider_id(provider_id)
}
