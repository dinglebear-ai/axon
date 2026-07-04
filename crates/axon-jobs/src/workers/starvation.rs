use sqlx::SqlitePool;

use super::WatchdogNotifies;
use crate::backend::JobKind;
use crate::query::{count_jobs_by_status, oldest_pending_created_at};
use crate::status::JobStatus;
use crate::store::now_ms;
use axon_api::source::{
    JobId, LifecycleStatus, PipelinePhase, Severity, SourceProgressEvent, SourceWarning,
    StageCounts, Timestamp, Visibility,
};

/// One job kind found starving during a watchdog sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StarvationAlarm {
    pub kind: JobKind,
    pub pending: i64,
    pub oldest_age_ms: i64,
}

impl StarvationAlarm {
    #[allow(dead_code)]
    pub(crate) fn to_progress_event(
        self,
        job_id: JobId,
        sequence: u64,
        timestamp: Timestamp,
    ) -> SourceProgressEvent {
        SourceProgressEvent {
            event_id: format!("starvation-{}-{sequence}", self.kind.table_name()),
            sequence,
            job_id,
            attempt: 0,
            stage_id: None,
            batch_id: None,
            reservation_id: None,
            checkpoint_id: None,
            dedupe_key: Some(format!("starvation:{}", self.kind.table_name())),
            phase: PipelinePhase::Leasing,
            status: LifecycleStatus::Waiting,
            severity: Severity::Warning,
            visibility: Visibility::Internal,
            message: format!(
                "{} queue starved with {} pending jobs",
                self.kind.table_name(),
                self.pending
            ),
            timestamp,
            source_id: None,
            canonical_uri: None,
            adapter: None,
            scope: None,
            generation: None,
            counts: StageCounts {
                items_total: Some(self.pending.max(0) as u64),
                items_done: 0,
                documents_total: None,
                documents_done: 0,
                chunks_total: None,
                chunks_done: 0,
                bytes_total: None,
                bytes_done: 0,
            },
            timing: None,
            current: None,
            throughput: None,
            retry: None,
            warning: Some(SourceWarning {
                code: "worker.starvation".to_string(),
                severity: Severity::Warning,
                message: format!(
                    "oldest pending {} job has waited {} seconds with no running worker",
                    self.kind.table_name(),
                    self.oldest_age_ms / 1000
                ),
                source_item_key: None,
                retryable: true,
            }),
            error: None,
        }
    }
}

/// Detect and recover starved job queues.
///
/// A kind is *starving* when pending jobs exist, **zero** jobs of that kind are
/// running, and the oldest pending job has waited longer than `threshold_ms`. A
/// healthy worker wakes every `POLL_INTERVAL` (5s) and would have claimed it, so
/// persistent pending-with-nothing-running is the signature of a wedged or dead
/// lane. The existing watchdog only reclaims stale *running* rows and is blind
/// to this case — this detector is the safety net that covers it.
///
/// For each starving kind this logs LOUDLY at ERROR (so the failure is never
/// silent again) and fires the kind's `Notify` to kick a parked-but-alive lane.
/// Returns the alarms raised. `threshold_ms <= 0` disables detection.
pub(super) async fn detect_and_recover_starvation(
    pool: &SqlitePool,
    notifies: &WatchdogNotifies,
    threshold_ms: i64,
) -> Vec<StarvationAlarm> {
    if threshold_ms <= 0 {
        return Vec::new();
    }
    let now = now_ms();
    let mut alarms = Vec::new();
    for kind in JobKind::all() {
        let kind = *kind;
        let hist = match count_jobs_by_status(pool, kind).await {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!(table = kind.table_name(), error = %e,
                    "starvation detector: count_jobs_by_status failed");
                continue;
            }
        };
        let pending = hist.get(&JobStatus::Pending).copied().unwrap_or(0);
        let running = hist.get(&JobStatus::Running).copied().unwrap_or(0);
        // Only pending-with-nothing-running is starvation. A backlog queued
        // behind busy lanes (running > 0) is healthy and excluded — this also
        // correctly excludes ingest jobs waiting on a running same-target sibling.
        if pending == 0 || running > 0 {
            continue;
        }
        // Gate on age — the only extra query, run solely for starvation candidates.
        let oldest = match oldest_pending_created_at(pool, kind).await {
            Ok(Some(v)) => v,
            Ok(None) => continue, // raced to empty between the two queries
            Err(e) => {
                tracing::warn!(table = kind.table_name(), error = %e,
                    "starvation detector: oldest_pending_created_at failed");
                continue;
            }
        };
        let oldest_age_ms = now - oldest;
        if oldest_age_ms < threshold_ms {
            continue;
        }
        tracing::error!(
            table = kind.table_name(),
            pending,
            oldest_pending_age_secs = oldest_age_ms / 1000,
            starvation_threshold_secs = threshold_ms / 1000,
            "worker starvation: pending jobs exist but none are running and the \
             queue is not draining — a worker lane is wedged or dead; kicking lane(s)"
        );
        notifies.notify_kind(kind);
        alarms.push(StarvationAlarm {
            kind,
            pending,
            oldest_age_ms,
        });
    }
    alarms
}

#[cfg(test)]
#[path = "starvation_tests.rs"]
mod tests;
