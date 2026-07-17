//! Transport-neutral provider failure projection.

use axon_api::source::{
    JobId, LifecycleStatus, PipelinePhase, RetryState, Severity, SourceGenerationId, SourceId,
    SourceItemKey, SourceProgressEvent, StageId, Timestamp,
};
use axon_error::{ApiError, SourceItemError};
use chrono::{Duration, Utc};

use crate::event::base_event;
use crate::reservation::{
    ProviderReservationManager, ProviderReservationOutcome, RecordedProviderFailure,
};

/// Correlation data required to project a provider failure consistently.
#[derive(Debug, Clone)]
pub struct ProviderFailureContext {
    pub job_id: JobId,
    pub stage_id: Option<StageId>,
    pub phase: PipelinePhase,
    pub source_id: Option<SourceId>,
    pub source_item_key: Option<SourceItemKey>,
    pub generation: Option<SourceGenerationId>,
    pub attempt: u32,
    pub max_attempts: Option<u32>,
}

/// Shared error, event, and optional item record produced from one failure.
#[derive(Debug, Clone)]
pub struct ProviderFailureProjection {
    pub error: ApiError,
    pub event: SourceProgressEvent,
    pub item_error: Option<SourceItemError>,
    pub outcome: ProviderReservationOutcome,
}

/// Record provider health and project the resulting typed failure.
///
/// Redaction failures are not provider-health failures. They bypass health
/// mutation and remain fatal, non-retryable failures with no cooling metadata.
pub async fn project_provider_failure(
    manager: &ProviderReservationManager,
    error: ApiError,
    context: ProviderFailureContext,
) -> ProviderFailureProjection {
    let recorded = if error.code.to_string() == "redaction.failed" {
        RecordedProviderFailure {
            error,
            outcome: ProviderReservationOutcome::Recorded,
        }
    } else {
        manager.record_api_failure(error).await
    };
    project_recorded_failure(recorded, context)
}

fn project_recorded_failure(
    recorded: RecordedProviderFailure,
    context: ProviderFailureContext,
) -> ProviderFailureProjection {
    let mut error = recorded.error.with_job_id(context.job_id.0.to_string());
    if let Some(source_id) = &context.source_id {
        error = error.with_source_id(source_id.0.clone());
    }
    if let Some(item_key) = &context.source_item_key {
        error = error.with_source_item_key(item_key.0.clone());
    }

    let waiting = error.retryable;
    let item_error = match (
        context.source_id.as_ref(),
        context.source_item_key.as_ref(),
        context.generation.as_ref(),
    ) {
        (Some(source_id), Some(item_key), Some(generation)) => Some(error.to_source_item_error(
            source_id.0.clone(),
            item_key.0.clone(),
            generation.0.clone(),
            if waiting { "waiting" } else { "failed" },
            context.attempt,
        )),
        _ => None,
    };

    let mut event = base_event(
        context.job_id,
        context.phase,
        if waiting {
            LifecycleStatus::Waiting
        } else {
            LifecycleStatus::Failed
        },
        event_severity(&error, waiting),
        error.message.clone(),
    );
    event.attempt = context.attempt;
    event.stage_id = context.stage_id;
    event.source_id = context.source_id;
    event.error = Some(error.clone());
    event.retry = error.retryable.then(|| RetryState {
        attempt: context.attempt,
        max_attempts: context.max_attempts,
        next_retry_at: retry_timestamp(&error),
        reason: error.code.to_string(),
    });
    event.dedupe_key = error
        .provider_id
        .as_ref()
        .map(|provider_id| format!("provider_failure:{provider_id}:{}", error.code));

    ProviderFailureProjection {
        error,
        event,
        item_error,
        outcome: recorded.outcome,
    }
}

fn retry_timestamp(error: &ApiError) -> Option<Timestamp> {
    error.cooldown_until.map(Timestamp::from).or_else(|| {
        error.retry_after_ms.map(|delay| {
            Timestamp::from(Utc::now() + Duration::milliseconds(delay.min(i64::MAX as u64) as i64))
        })
    })
}

fn event_severity(error: &ApiError, waiting: bool) -> Severity {
    if waiting {
        return Severity::Warning;
    }
    match error.severity {
        axon_error::ErrorSeverity::Info => Severity::Info,
        axon_error::ErrorSeverity::Warning => Severity::Warning,
        axon_error::ErrorSeverity::Degraded => Severity::Degraded,
        axon_error::ErrorSeverity::Failed => Severity::Failed,
        axon_error::ErrorSeverity::Fatal => Severity::Fatal,
    }
}
