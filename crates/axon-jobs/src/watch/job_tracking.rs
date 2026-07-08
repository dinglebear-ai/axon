//! Additive observability instrumentation for watch runs.
//!
//! Watch has its own lease-based scheduling (`axon_watch_defs`/
//! `axon_watch_runs`) which is the authoritative single-flight guarantee for
//! recurring watch execution — see `crate::watch` and
//! `crate::workers::watch_scheduler`. This module does NOT participate in
//! that guarantee; it only mirrors each watch run as a `JobKind::Watch` row
//! on the unified job store (`crate::unified::SqliteUnifiedJobStore`) so
//! `axon jobs list --kind watch` has real data. If the unified-job mirror
//! write fails for any reason, the failure is logged and swallowed — it must
//! never affect the real watch run's outcome or the caller's `WatchRun`
//! result.
//!
//! `axon-jobs` cannot depend on `axon-services` (see `crates/axon-jobs/src/
//! CLAUDE.md`), so this calls `crate::boundary::JobStore` directly, following
//! the pattern in `crate::workers::unified`.

use axon_api::source::{
    AuthMode, AuthSnapshot, JobCreateRequest, JobId, JobIntent, JobKind, JobPriority, JobStagePlan,
    JobStatusUpdate, LifecycleStatus, MetadataMap, PipelinePhase, Severity, SourceError, Timestamp,
    TransportKind, Visibility, WatchId,
};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::boundary::JobStore;
use crate::unified::SqliteUnifiedJobStore;

/// Create a `JobKind::Watch` row on the unified store for an about-to-run
/// watch tick, transition it to `Running`, and return its job id for the
/// caller to finalize with [`finish_watch_job`]. Returns `None` (never an
/// error) if the mirror row could not be created — callers must proceed with
/// the real watch run regardless.
pub(crate) async fn start_watch_job(pool: &SqlitePool, watch_id: Uuid) -> Option<JobId> {
    let store = SqliteUnifiedJobStore::new(pool.clone());
    let request = JobCreateRequest {
        request_id: None,
        job_kind: JobKind::Watch,
        job_intent: JobIntent::Watch,
        source_id: None,
        watch_id: Some(WatchId::new(watch_id.to_string())),
        parent_job_id: None,
        root_job_id: None,
        attempt: 1,
        priority: JobPriority::Background,
        idempotency_key: None,
        stage_plan: vec![JobStagePlan {
            phase: PipelinePhase::Diffing,
            required: true,
            provider_requirements: Vec::new(),
            estimated_items: None,
        }],
        request: Some(serde_json::json!({ "watch_id": watch_id.to_string() })),
        auth_snapshot: watch_auth_snapshot(),
        config_snapshot_id: None,
        requirements: MetadataMap::new(),
        result_schema: None,
        warnings: Vec::new(),
        error: None,
        metadata: MetadataMap::new(),
    };

    let descriptor = match store.create(request).await {
        Ok(descriptor) => descriptor,
        Err(err) => {
            tracing::warn!(%watch_id, error = %err.message, "watch job_tracking: create failed");
            return None;
        }
    };

    if let Err(err) = store
        .update_status(JobStatusUpdate {
            job_id: descriptor.job_id,
            source_id: None,
            status: LifecycleStatus::Running,
            phase: PipelinePhase::Diffing,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error: None,
        })
        .await
    {
        tracing::warn!(%watch_id, job_id = %descriptor.job_id.0, error = %err.message, "watch job_tracking: Queued->Running failed");
        return None;
    }

    Some(descriptor.job_id)
}

/// Finalize the unified-job mirror row created by [`start_watch_job`],
/// marking it `Completed` or `Failed` from the real watch run's outcome.
/// A `None` `job_id` (mirror creation failed earlier) is a no-op. Errors are
/// logged, not propagated — this must never affect the real watch result.
pub(crate) async fn finish_watch_job(
    pool: &SqlitePool,
    job_id: Option<JobId>,
    outcome: Result<(), &str>,
) {
    let Some(job_id) = job_id else {
        return;
    };
    let store = SqliteUnifiedJobStore::new(pool.clone());
    let (status, error) = match outcome {
        Ok(()) => (LifecycleStatus::Completed, None),
        Err(message) => (
            LifecycleStatus::Failed,
            Some(SourceError {
                code: "watch.run_failed".to_string(),
                severity: Severity::Failed,
                message: message.to_string(),
                source_item_key: None,
                retryable: true,
                provider_id: None,
                cause: None,
            }),
        ),
    };
    if let Err(err) = store
        .update_status(JobStatusUpdate {
            job_id,
            source_id: None,
            status,
            phase: PipelinePhase::Complete,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error,
        })
        .await
    {
        tracing::warn!(job_id = %job_id.0, error = %err.message, "watch job_tracking: finalize failed");
    }
}

/// Minimal local/system auth snapshot for scheduler- or CLI-triggered watch
/// runs. Mirrors the shape used by other in-process job creators (see
/// `crate::workers::unified` and the unified store tests) — there is no
/// end-user caller identity to carry through a watch tick.
fn watch_auth_snapshot() -> AuthSnapshot {
    AuthSnapshot {
        caller_id: None,
        transport: TransportKind::System,
        granted_scopes: Vec::new(),
        visibility_ceiling: Visibility::Internal,
        request_time: Timestamp::from(chrono::Utc::now()),
        policy_version: "watch-scheduler".to_string(),
        auth_mode: AuthMode::TrustedLocal,
        token_id: None,
        display_name: None,
    }
}

#[cfg(test)]
#[path = "job_tracking_tests.rs"]
mod tests;
