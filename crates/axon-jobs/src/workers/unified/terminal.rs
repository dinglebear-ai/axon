//! Terminal-state helpers for the unified worker: heartbeats, cancellation
//! marking, failure marking, and the shared `mark_terminal` SQL that both
//! paths (and `run_unified_claimed`'s success path in `unified.rs`) write
//! through. Split out of `unified.rs` to keep it under the monolith line cap.

use axon_api::source::{
    ApiError, ErrorStage, JobHeartbeat, LifecycleStatus, PipelinePhase, Severity,
    SourceProgressEvent, Timestamp, Visibility,
};
use sqlx::SqlitePool;

use crate::boundary::JobStore;
use crate::unified::SqliteUnifiedJobStore;

use super::UnifiedClaimedJob;
use super::helpers::{empty_counts, enum_name, json_error, source_error_from_api, sql_error};

/// Append a failure event and mark the claimed job terminal-failed with
/// `error`. Shared by every rejection path (auth denial, unsupported stage,
/// registered-runner failure) so each one only needs to construct its own
/// `ApiError`.
pub(super) async fn fail_unified_claimed(
    pool: &SqlitePool,
    store: &SqliteUnifiedJobStore,
    claimed: &UnifiedClaimedJob,
    error: ApiError,
) {
    let _ = store
        .append_event(SourceProgressEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            sequence: 0,
            job_id: claimed.job_id,
            attempt: claimed.attempt,
            stage_id: None,
            batch_id: None,
            reservation_id: None,
            checkpoint_id: None,
            dedupe_key: Some(format!("job-failed:{}:{}", error.code, claimed.job_id.0)),
            phase: PipelinePhase::Complete,
            status: LifecycleStatus::Failed,
            severity: Severity::Failed,
            visibility: Visibility::Public,
            message: error.message.clone(),
            timestamp: Timestamp::from(chrono::Utc::now()),
            source_id: None,
            canonical_uri: None,
            adapter: None,
            scope: None,
            generation: None,
            counts: empty_counts(),
            timing: None,
            current: None,
            throughput: None,
            retry: None,
            warning: None,
            error: Some(error.clone()),
        })
        .await;

    if let Err(mark_error) = mark_terminal(
        pool,
        claimed,
        LifecycleStatus::Failed,
        PipelinePhase::Complete,
        Some(error),
    )
    .await
    {
        tracing::error!(
            job_id = %claimed.job_id.0,
            error = %mark_error.message,
            "unified worker failed to mark claimed job terminal"
        );
    }
}

pub(super) async fn heartbeat(
    store: &SqliteUnifiedJobStore,
    claimed: &UnifiedClaimedJob,
    phase: PipelinePhase,
) -> Result<(), ApiError> {
    store
        .heartbeat(JobHeartbeat {
            job_id: claimed.job_id,
            attempt: claimed.attempt,
            worker_id: Some("unified-local-worker".to_string()),
            phase,
            status: LifecycleStatus::Running,
            stage_id: None,
            heartbeat_at: Timestamp::from(chrono::Utc::now()),
            last_event_sequence: None,
            counts: Some(empty_counts()),
            provider_reservations: Vec::new(),
        })
        .await
}

pub(super) async fn mark_canceled(
    pool: &SqlitePool,
    store: &SqliteUnifiedJobStore,
    claimed: &UnifiedClaimedJob,
) {
    if let Err(error) = store
        .append_event(SourceProgressEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            sequence: 0,
            job_id: claimed.job_id,
            attempt: claimed.attempt,
            stage_id: None,
            batch_id: None,
            reservation_id: None,
            checkpoint_id: None,
            dedupe_key: Some(format!("shutdown-canceled:{}", claimed.job_id.0)),
            phase: PipelinePhase::Canceled,
            status: LifecycleStatus::Canceled,
            severity: Severity::Warning,
            visibility: Visibility::Public,
            message: "unified durable runner shut down before executing job".to_string(),
            timestamp: Timestamp::from(chrono::Utc::now()),
            source_id: None,
            canonical_uri: None,
            adapter: None,
            scope: None,
            generation: None,
            counts: empty_counts(),
            timing: None,
            current: None,
            throughput: None,
            retry: None,
            warning: None,
            error: None,
        })
        .await
    {
        tracing::warn!(job_id = %claimed.job_id.0, error = %error.message, "unified worker cancel event failed");
    }
    if let Err(error) = mark_terminal(
        pool,
        claimed,
        LifecycleStatus::Canceled,
        PipelinePhase::Canceled,
        None,
    )
    .await
    {
        tracing::warn!(job_id = %claimed.job_id.0, error = %error.message, "unified worker failed to mark shutdown claim canceled");
    }
}

pub(super) async fn mark_terminal(
    pool: &SqlitePool,
    claimed: &UnifiedClaimedJob,
    status: LifecycleStatus,
    phase: PipelinePhase,
    error: Option<ApiError>,
) -> Result<(), ApiError> {
    let now = Timestamp::from(chrono::Utc::now());
    let terminal_severity = match status {
        LifecycleStatus::CompletedDegraded => Severity::Degraded,
        LifecycleStatus::Canceled => Severity::Warning,
        _ => Severity::Failed,
    };
    let status = enum_name(status)?;
    let phase = enum_name(phase)?;
    let source_error_json = error
        .as_ref()
        .map(|error| source_error_from_api(error, terminal_severity))
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(json_error)?;
    let api_error_json = error
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(json_error)?;
    let mut tx = pool.begin().await.map_err(sql_error)?;
    // mark_terminal is only ever called with a terminal LifecycleStatus
    // (Completed/CompletedDegraded/Failed/Canceled — see call sites in
    // run_unified_claimed/fail_unified_claimed/mark_canceled), never Waiting,
    // so cooldown_until is unconditionally cleared here: a job that cooled
    // once and then reaches a terminal state must not retain a stale
    // cooldown. This writes directly to `jobs` via raw SQL rather than going
    // through `update_job_status`, so it needs its own clear rather than
    // inheriting the CASE-based clear added there.
    let job_result = sqlx::query(
        "UPDATE jobs SET
            status = ?,
            phase = ?,
            updated_at = ?,
            finished_at = COALESCE(finished_at, ?),
            last_error_json = ?,
            cooldown_until = NULL
         WHERE job_id = ? AND attempt = ?",
    )
    .bind(status.as_str())
    .bind(phase.as_str())
    .bind(now.0.as_str())
    .bind(now.0.as_str())
    .bind(source_error_json.as_deref())
    .bind(claimed.job_id.0.to_string())
    .bind(claimed.attempt as i64)
    .execute(&mut *tx)
    .await
    .map_err(sql_error)?;
    if job_result.rows_affected() == 0 {
        tx.commit().await.map_err(sql_error)?;
        return Err(ApiError::new(
            "job_terminal.stale_attempt",
            ErrorStage::Publishing,
            format!(
                "job {} attempt {} is no longer current; terminal update skipped",
                claimed.job_id.0, claimed.attempt
            ),
        ));
    }
    sqlx::query(
        "UPDATE job_attempts SET
            status = ?,
            finished_at = COALESCE(finished_at, ?),
            error_json = ?
         WHERE job_id = ? AND attempt = ?",
    )
    .bind(status.as_str())
    .bind(now.0.as_str())
    .bind(api_error_json.as_deref())
    .bind(claimed.job_id.0.to_string())
    .bind(claimed.attempt as i64)
    .execute(&mut *tx)
    .await
    .map_err(sql_error)?;
    sqlx::query(
        "UPDATE job_stages SET
            status = ?,
            completed_at = COALESCE(completed_at, ?),
            error_json = ?
         WHERE job_id = ? AND status IN ('queued', 'pending', 'running', 'waiting', 'blocked')",
    )
    .bind(status.as_str())
    .bind(now.0.as_str())
    .bind(source_error_json.as_deref())
    .bind(claimed.job_id.0.to_string())
    .execute(&mut *tx)
    .await
    .map_err(sql_error)?;
    tx.commit().await.map_err(sql_error)
}
