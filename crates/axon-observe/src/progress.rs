//! Typed progress-update helpers over the shared event plumbing.
//!
//! [`crate::event`] owns the terminal stage-lifecycle builders (started /
//! completed / failed / degraded). `ProgressUpdate` owns the *in-flight*
//! `status=running` shape — the recurring "N of M done" ticks a stage emits
//! between its start and finish events — so callers stop hand-rolling
//! `SourceProgressEvent` literals for progress polling. See
//! `docs/pipeline-unification/runtime/observability-contract.md`
//! (`SourceProgressEvent`) and
//! `docs/pipeline-unification/schemas/event-schema.md`
//! (`SourceProgressEvent` shape).

pub const MODULE_NAME: &str = "progress";

use axon_api::source::{
    JobId, LifecycleStatus, PipelinePhase, ProgressCurrent, ProgressThroughput, Severity,
    SourceProgressEvent, StageCounts, StageId,
};

/// A typed, incremental progress tick for one job/stage. Build with
/// [`ProgressUpdate::new`] and the `with_*` setters, then materialize a
/// `status=running` [`SourceProgressEvent`] via
/// [`ProgressUpdate::into_event`]. `sequence` is left at the pure-builder
/// sentinel (`0`) and stamped by the emitting sink, matching every other
/// builder in [`crate::event`].
#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub stage_id: Option<StageId>,
    pub counts: StageCounts,
    pub current: Option<ProgressCurrent>,
    pub throughput: Option<ProgressThroughput>,
    pub message: String,
}

impl ProgressUpdate {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            stage_id: None,
            counts: crate::event::zero_counts(),
            current: None,
            throughput: None,
            message: message.into(),
        }
    }

    pub fn with_stage(mut self, stage_id: StageId) -> Self {
        self.stage_id = Some(stage_id);
        self
    }

    pub fn with_counts(mut self, counts: StageCounts) -> Self {
        self.counts = counts;
        self
    }

    pub fn with_current(mut self, current: ProgressCurrent) -> Self {
        self.current = Some(current);
        self
    }

    pub fn with_throughput(mut self, throughput: ProgressThroughput) -> Self {
        self.throughput = Some(throughput);
        self
    }

    /// Materialize this update into a `status=running`, `severity=info`
    /// `SourceProgressEvent` for `job_id`/`phase`.
    pub fn into_event(self, job_id: JobId, phase: PipelinePhase) -> SourceProgressEvent {
        let mut event = crate::event::base_event(
            job_id,
            phase,
            LifecycleStatus::Running,
            Severity::Info,
            self.message,
        );
        event.stage_id = self.stage_id;
        event.counts = self.counts;
        event.current = self.current;
        event.throughput = self.throughput;
        event
    }
}

/// Convenience free function equivalent to
/// `ProgressUpdate::new(message).with_counts(counts).into_event(job_id, phase)`
/// for the common "counts only" progress tick.
pub fn counts_update(
    job_id: JobId,
    phase: PipelinePhase,
    stage_id: Option<StageId>,
    counts: StageCounts,
    message: impl Into<String>,
) -> SourceProgressEvent {
    let mut update = ProgressUpdate::new(message).with_counts(counts);
    update.stage_id = stage_id;
    update.into_event(job_id, phase)
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
