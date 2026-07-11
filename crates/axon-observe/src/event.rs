//! Progress event builders for the target observability boundary.

pub const MODULE_NAME: &str = "event";

use axon_api::source::{
    ApiError, JobId, JobPriority, LifecycleStatus, PipelinePhase, ProgressCurrent, ProviderId,
    ProviderKind, ReservationId, Severity, SourceWarning, StageCounts, StageId, Timestamp,
    Visibility,
};
use chrono::Utc;
use uuid::Uuid;

pub fn stage_started(
    job_id: JobId,
    stage_id: Option<StageId>,
    phase: PipelinePhase,
    message: String,
) -> axon_api::source::SourceProgressEvent {
    base_event(
        job_id,
        phase,
        LifecycleStatus::Running,
        Severity::Info,
        message,
    )
    .with_stage(stage_id)
}

pub fn stage_completed(
    job_id: JobId,
    stage_id: Option<StageId>,
    phase: PipelinePhase,
    counts: StageCounts,
    message: String,
) -> axon_api::source::SourceProgressEvent {
    let mut event = base_event(
        job_id,
        phase,
        LifecycleStatus::Completed,
        Severity::Info,
        message,
    )
    .with_stage(stage_id);
    event.counts = counts;
    event
}

pub fn stage_degraded(
    job_id: JobId,
    stage_id: Option<StageId>,
    phase: PipelinePhase,
    warning: SourceWarning,
    message: String,
) -> axon_api::source::SourceProgressEvent {
    let mut event = base_event(
        job_id,
        phase,
        LifecycleStatus::CompletedDegraded,
        Severity::Degraded,
        message,
    )
    .with_stage(stage_id);
    event.warning = Some(warning);
    event
}

pub fn stage_failed(
    job_id: JobId,
    stage_id: Option<StageId>,
    phase: PipelinePhase,
    error: ApiError,
    message: String,
) -> axon_api::source::SourceProgressEvent {
    let mut event = base_event(
        job_id,
        phase,
        LifecycleStatus::Failed,
        Severity::Failed,
        message,
    )
    .with_stage(stage_id);
    event.error = Some(error);
    event
}

pub fn provider_waiting(
    job_id: JobId,
    stage_id: Option<StageId>,
    reservation_id: Option<ReservationId>,
    provider_kind: ProviderKind,
    _priority: JobPriority,
    message: String,
) -> axon_api::source::SourceProgressEvent {
    let provider_key = provider_key(provider_kind);
    let mut event = base_event(
        job_id,
        PipelinePhase::Embedding,
        LifecycleStatus::Waiting,
        Severity::Info,
        message,
    )
    .with_stage(stage_id);
    event.reservation_id = reservation_id;
    event.current = Some(ProgressCurrent {
        source_item_key: None,
        document_id: None,
        chunk_id: None,
        adapter: None,
        provider: Some(ProviderId::from(provider_key.as_str())),
        message: Some("waiting for provider capacity".to_string()),
    });
    event.dedupe_key = Some(format!("provider_wait:{provider_key}"));
    event
}

pub fn warning(code: impl Into<String>, message: impl Into<String>) -> SourceWarning {
    SourceWarning {
        code: code.into(),
        severity: Severity::Degraded,
        message: message.into(),
        source_item_key: None,
        retryable: true,
    }
}

/// Shared by [`crate::progress`] so `status=running` in-flight updates use the
/// same base envelope as the terminal builders above.
pub(crate) fn base_event(
    job_id: JobId,
    phase: PipelinePhase,
    status: LifecycleStatus,
    severity: Severity,
    message: String,
) -> axon_api::source::SourceProgressEvent {
    let now = Timestamp::from(Utc::now());
    axon_api::source::SourceProgressEvent {
        event_id: format!("evt_{}", Uuid::new_v4()),
        // `0` is the "unassigned" sentinel. The pure builders cannot own
        // monotonic sequence state, so the emitting sink stamps the real,
        // strictly-increasing per-`job_id` sequence at emit time via
        // `crate::sequence::SequenceRegistry`. See `crate::sink`.
        sequence: 0,
        job_id,
        attempt: 1,
        stage_id: None,
        batch_id: None,
        reservation_id: None,
        checkpoint_id: None,
        dedupe_key: None,
        phase,
        status,
        severity,
        visibility: Visibility::Internal,
        message,
        timestamp: now.clone(),
        source_id: None,
        canonical_uri: None,
        adapter: None,
        scope: None,
        generation: None,
        counts: zero_counts(),
        timing: Some(axon_api::source::ProgressTiming {
            started_at: now.clone(),
            updated_at: now,
            elapsed_ms: 0,
            eta_ms: None,
        }),
        current: None,
        throughput: None,
        retry: None,
        warning: None,
        error: None,
    }
}

trait EventBuilderExt {
    fn with_stage(self, stage_id: Option<StageId>) -> Self;
}

impl EventBuilderExt for axon_api::source::SourceProgressEvent {
    fn with_stage(mut self, stage_id: Option<StageId>) -> Self {
        self.stage_id = stage_id;
        self
    }
}

pub(crate) fn zero_counts() -> StageCounts {
    StageCounts {
        items_total: None,
        items_done: 0,
        documents_total: None,
        documents_done: 0,
        chunks_total: None,
        chunks_done: 0,
        bytes_total: None,
        bytes_done: 0,
    }
}

fn provider_key(provider_kind: ProviderKind) -> String {
    serde_json::to_value(provider_kind)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| format!("{provider_kind:?}").to_ascii_lowercase())
}
