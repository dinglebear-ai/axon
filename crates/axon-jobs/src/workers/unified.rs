use std::sync::Arc;

use axon_api::source::{
    ApiError, AuthSnapshot, ErrorStage, JobHeartbeat, JobId, JobKind as UnifiedJobKind,
    LifecycleStatus, PipelinePhase, Severity, SourceError, SourceProgressEvent, StageCounts,
    Timestamp, Visibility,
};
use axon_core::config::Config;
use sqlx::{Row, SqlitePool};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::boundary::JobStore;
use crate::config_snapshot::apply_config_snapshot;
use crate::store_inventory::detect_incompatible_legacy_jobs;
use crate::unified::SqliteUnifiedJobStore;

use super::auth_enforcement::{require_job_scope, required_scope_for_kind};
use super::{POLL_INTERVAL, WORKER_BATCH_LIMIT};

mod extract_runner;
use extract_runner::run_extract_claimed;

mod helpers;
use helpers::{
    empty_counts, enum_name, json_error, parse_enum, parse_uuid, source_error_from_api, sql_error,
};

mod runner_registry;
pub use runner_registry::{JobRunnerRegistry, UnifiedJobRunner};

#[derive(Debug, Clone, PartialEq)]
pub struct UnifiedClaimedJob {
    pub job_id: JobId,
    pub kind: UnifiedJobKind,
    pub attempt: u32,
    pub request_json: Option<serde_json::Value>,
    /// The auth snapshot recorded at enqueue time — the *only* source of
    /// truth for what this job is allowed to do. Never re-derive scope from
    /// the current process/caller: a stale reclaim or retry must run with
    /// exactly what was granted when the job was created.
    pub auth_snapshot: AuthSnapshot,
}

pub(crate) async fn unified_worker_loop(
    pool: Arc<SqlitePool>,
    cfg: Arc<Config>,
    notify: Arc<Notify>,
    shutdown: CancellationToken,
    registry: Option<Arc<JobRunnerRegistry>>,
) {
    if let Err(error) = ensure_no_incompatible_legacy_jobs(&pool).await {
        tracing::error!(
            error = %error.message,
            code = %error.code,
            "unified worker startup blocked"
        );
        return;
    }
    let mut wake_count: u64 = 0;
    loop {
        tokio::select! {
            _ = notify.notified() => {}
            _ = tokio::time::sleep(POLL_INTERVAL) => {}
            _ = shutdown.cancelled() => break,
        }
        wake_count = wake_count.wrapping_add(1);

        let mut claimed_this_wake = 0usize;
        loop {
            let mut processed = 0usize;
            while processed < WORKER_BATCH_LIMIT && !shutdown.is_cancelled() {
                match claim_next_unified_job_unchecked(&pool).await {
                    Ok(Some(claimed)) => {
                        run_unified_claimed(&pool, &cfg, &claimed, &shutdown, registry.as_deref())
                            .await;
                        processed += 1;
                    }
                    Ok(None) => break,
                    Err(error) => {
                        tracing::error!(
                            error = %error.message,
                            code = %error.code,
                            "unified worker claim error"
                        );
                        break;
                    }
                }
            }
            claimed_this_wake += processed;
            if shutdown.is_cancelled() || processed < WORKER_BATCH_LIMIT {
                break;
            }
            tokio::task::yield_now().await;
        }
        if claimed_this_wake > 0 || wake_count.is_multiple_of(12) {
            tracing::debug!(
                claimed = claimed_this_wake,
                wake_count,
                "unified worker: poll batch complete"
            );
        }
    }
}

/// Test-only entry point: production code claims via the poll loop in
/// [`unified_worker_loop`]; tests use this to claim+run one job deterministically.
#[allow(dead_code)]
pub(crate) async fn claim_next_unified_job(
    pool: &SqlitePool,
) -> Result<Option<UnifiedClaimedJob>, ApiError> {
    ensure_no_incompatible_legacy_jobs(pool).await?;
    claim_next_unified_job_unchecked(pool).await
}

async fn ensure_no_incompatible_legacy_jobs(pool: &SqlitePool) -> Result<(), ApiError> {
    if let Some(blocker) = detect_incompatible_legacy_jobs(pool).await? {
        return Err(ApiError::new(
            "job_store.incompatible_legacy_jobs",
            ErrorStage::Planning,
            blocker.message,
        ));
    }
    Ok(())
}

async fn claim_next_unified_job_unchecked(
    pool: &SqlitePool,
) -> Result<Option<UnifiedClaimedJob>, ApiError> {
    let mut tx = pool.begin().await.map_err(sql_error)?;
    let row = sqlx::query(
        "SELECT job_id, kind, attempt, request_json, auth_snapshot_json
         FROM jobs
         WHERE status IN ('queued', 'waiting', 'blocked')
         ORDER BY
           CASE priority
             WHEN 'interactive' THEN 0
             WHEN 'high' THEN 1
             WHEN 'normal' THEN 2
             WHEN 'background' THEN 3
             WHEN 'maintenance' THEN 4
             ELSE 5
           END,
           updated_at ASC,
           job_id ASC
         LIMIT 1",
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(sql_error)?;

    let Some(row) = row else {
        tx.commit().await.map_err(sql_error)?;
        return Ok(None);
    };

    let job_id = JobId::new(parse_uuid(row.get::<String, _>("job_id"))?);
    let kind = parse_enum(row.get::<String, _>("kind"))?;
    let attempt = (row.get::<i64, _>("attempt") as u32).max(1);
    let request_json = row
        .get::<Option<String>, _>("request_json")
        .map(|value| serde_json::from_str(&value).map_err(json_error))
        .transpose()?;
    let auth_snapshot: AuthSnapshot =
        serde_json::from_str(&row.get::<String, _>("auth_snapshot_json")).map_err(json_error)?;
    let now = Timestamp::from(chrono::Utc::now());

    let result = sqlx::query(
        "UPDATE jobs SET
            status = 'running',
            phase = 'planning',
            attempt = ?,
            started_at = COALESCE(started_at, ?),
            updated_at = ?
         WHERE job_id = ? AND status IN ('queued', 'waiting', 'blocked')",
    )
    .bind(attempt as i64)
    .bind(now.0.as_str())
    .bind(now.0.as_str())
    .bind(job_id.0.to_string())
    .execute(&mut *tx)
    .await
    .map_err(sql_error)?;

    if result.rows_affected() == 0 {
        tx.commit().await.map_err(sql_error)?;
        return Ok(None);
    }

    sqlx::query(
        "INSERT INTO job_attempts (
            attempt_id, job_id, attempt, status, worker_id, started_at, heartbeat_at
         ) VALUES (?, ?, ?, 'running', NULL, ?, ?)
         ON CONFLICT(job_id, attempt) DO UPDATE SET
            status = 'running',
            started_at = COALESCE(job_attempts.started_at, excluded.started_at),
            heartbeat_at = excluded.heartbeat_at",
    )
    .bind(format!("{}:{}", job_id.0, attempt))
    .bind(job_id.0.to_string())
    .bind(attempt as i64)
    .bind(now.0.as_str())
    .bind(now.0.as_str())
    .execute(&mut *tx)
    .await
    .map_err(sql_error)?;

    tx.commit().await.map_err(sql_error)?;
    Ok(Some(UnifiedClaimedJob {
        job_id,
        kind,
        attempt,
        request_json,
        auth_snapshot,
    }))
}

pub(crate) async fn run_unified_claimed(
    pool: &SqlitePool,
    cfg: &Config,
    claimed: &UnifiedClaimedJob,
    shutdown: &CancellationToken,
    registry: Option<&JobRunnerRegistry>,
) {
    let store = SqliteUnifiedJobStore::new(pool.clone());
    if shutdown.is_cancelled() {
        mark_canceled(pool, &store, claimed).await;
        return;
    }

    if let Err(error) = heartbeat(&store, claimed, PipelinePhase::Planning).await {
        tracing::warn!(job_id = %claimed.job_id.0, error = %error.message, "unified worker heartbeat failed");
    }

    if let Some(required) = required_scope_for_kind(claimed.kind)
        && let Err(error) = require_job_scope(&claimed.auth_snapshot, required)
    {
        fail_unified_claimed(pool, &store, claimed, error).await;
        return;
    }

    // Extract has a dedicated in-crate runner (its real work,
    // axon_extract::sync::extract_sync, is a pure domain call reachable
    // directly from axon-jobs). Every other kind goes through the
    // dependency-inversion registry the composition layer (axon-services)
    // populates at startup; kinds with no registered runner keep failing
    // with job_runner.unsupported_stage.
    if claimed.kind == UnifiedJobKind::Extract {
        run_extract_claimed(pool, cfg, &store, claimed, shutdown).await;
        return;
    }

    let Some(runner) = registry.and_then(|registry| registry.get(claimed.kind)) else {
        let error = ApiError::new(
            "job_runner.unsupported_stage",
            ErrorStage::Planning,
            format!(
                "unified durable runner claimed {:?} job {}, but this stage is not wired yet",
                claimed.kind, claimed.job_id.0
            ),
        );
        fail_unified_claimed(pool, &store, claimed, error).await;
        return;
    };

    match runner.run(claimed, &store, shutdown).await {
        Ok(()) => {
            if let Err(mark_error) = mark_terminal(
                pool,
                claimed,
                LifecycleStatus::Completed,
                PipelinePhase::Complete,
                None,
            )
            .await
            {
                tracing::error!(
                    job_id = %claimed.job_id.0,
                    error = %mark_error.message,
                    "unified worker failed to mark completed job terminal"
                );
            }
        }
        Err(error) => {
            fail_unified_claimed(pool, &store, claimed, error).await;
        }
    }
}

/// Append a failure event and mark the claimed job terminal-failed with
/// `error`. Shared by every rejection path (auth denial, unsupported stage,
/// registered-runner failure) so each one only needs to construct its own
/// `ApiError`.
async fn fail_unified_claimed(
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

async fn heartbeat(
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

async fn mark_canceled(
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

async fn mark_terminal(
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
    let job_result = sqlx::query(
        "UPDATE jobs SET
            status = ?,
            phase = ?,
            updated_at = ?,
            finished_at = COALESCE(finished_at, ?),
            last_error_json = ?
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
