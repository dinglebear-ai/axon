//! Durable observability supplement for [`SqliteUnifiedJobStore`].
//!
//! Every status transition and heartbeat routed through the job store is *also*
//! recorded in the durable `axon_observe_events`/`axon_observe_heartbeats` tables
//! via [`SqliteObservabilitySink`], so the observability contract's per-`job_id`
//! monotonic event stream is satisfied without disturbing the existing
//! `job_events`/`progress_json` streams that back SSE/status rendering.
//!
//! The sink owns sequence assignment through its shared `SequenceRegistry`, so
//! the placeholder `sequence` on the events built here is overwritten before
//! persistence — every emitted event lands with a strictly-increasing sequence
//! per job. Emission failures are logged and swallowed: the observe stream is a
//! supplement, so a sink error must never fail the authoritative status write.

use axon_api::source::{
    JobHeartbeat, JobStatusUpdate, LifecycleStatus, Severity, SourceProgressEvent, StageCounts,
    Timestamp, Visibility,
};
use axon_observe::collector::ObservabilitySink;
use chrono::Utc;
use uuid::Uuid;

use super::SqliteUnifiedJobStore;

impl SqliteUnifiedJobStore {
    /// Record a status transition as a durable observe event + heartbeat.
    ///
    /// No-op when the store was built without an observability sink. Errors are
    /// logged, not propagated — the durable status write already committed.
    pub(super) async fn observe_status(&self, status: &JobStatusUpdate) {
        let Some(sink) = self.observe.as_ref() else {
            return;
        };
        let event = status_event(status);
        if let Err(err) = sink.emit(event).await {
            tracing::warn!(
                job_id = %status.job_id.0,
                error = %err,
                "observe sink emit failed for status transition"
            );
        }
        let heartbeat = status_heartbeat(status);
        if let Err(err) = sink.heartbeat(heartbeat).await {
            tracing::warn!(
                job_id = %status.job_id.0,
                error = %err,
                "observe sink heartbeat failed for status transition"
            );
        }
    }

    /// Record a job heartbeat as a durable observe heartbeat row.
    ///
    /// No-op without a sink; errors are logged, not propagated.
    pub(super) async fn observe_heartbeat(&self, heartbeat: &JobHeartbeat) {
        let Some(sink) = self.observe.as_ref() else {
            return;
        };
        if let Err(err) = sink.heartbeat(heartbeat.clone()).await {
            tracing::warn!(
                job_id = %heartbeat.job_id.0,
                error = %err,
                "observe sink heartbeat failed"
            );
        }
    }
}

/// Build a durable [`SourceProgressEvent`] from a status transition.
///
/// The `sequence` is a placeholder (`0`); the sink's `SequenceRegistry`
/// overwrites it with the next strictly-increasing per-job value on persist.
fn status_event(status: &JobStatusUpdate) -> SourceProgressEvent {
    SourceProgressEvent {
        event_id: format!("obs-{}", Uuid::new_v4()),
        sequence: 0,
        job_id: status.job_id,
        attempt: 0,
        stage_id: status.stage_id,
        batch_id: None,
        reservation_id: None,
        checkpoint_id: None,
        dedupe_key: None,
        phase: status.phase,
        status: status.status,
        severity: severity_for(status),
        visibility: Visibility::Public,
        message: status
            .message
            .clone()
            .unwrap_or_else(|| default_message(status)),
        timestamp: now_timestamp(),
        source_id: None,
        canonical_uri: None,
        adapter: None,
        scope: None,
        generation: None,
        counts: status.counts.clone().unwrap_or_else(empty_counts),
        timing: None,
        current: status.current.clone(),
        throughput: None,
        retry: None,
        warning: None,
        error: status.error.as_ref().map(source_error_to_api),
    }
}

/// Build a durable [`JobHeartbeat`] carrying the transition's phase/status/counts.
fn status_heartbeat(status: &JobStatusUpdate) -> JobHeartbeat {
    JobHeartbeat {
        job_id: status.job_id,
        attempt: 0,
        worker_id: None,
        phase: status.phase,
        status: status.status,
        stage_id: status.stage_id,
        heartbeat_at: now_timestamp(),
        last_event_sequence: None,
        counts: status.counts.clone(),
        provider_reservations: Vec::new(),
    }
}

/// Map a lifecycle status (+ error presence) onto an event severity.
fn severity_for(status: &JobStatusUpdate) -> Severity {
    if status.error.is_some() {
        return Severity::Failed;
    }
    match status.status {
        LifecycleStatus::Failed => Severity::Failed,
        LifecycleStatus::CompletedDegraded => Severity::Degraded,
        _ => Severity::Info,
    }
}

/// Human message when the caller omitted one.
fn default_message(status: &JobStatusUpdate) -> String {
    format!("job {:?}", status.status)
}

/// Convert the DTO `SourceError` carried on a status update into an `ApiError`
/// for the observe event's `error` field.
fn source_error_to_api(err: &axon_api::source::SourceError) -> axon_api::source::ApiError {
    axon_api::source::ApiError::new(
        err.code.clone(),
        axon_api::source::ErrorStage::Observing,
        err.message.clone(),
    )
}

fn now_timestamp() -> Timestamp {
    Timestamp::from(Utc::now())
}

/// An all-zero [`StageCounts`] (the type does not derive `Default`).
fn empty_counts() -> StageCounts {
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
