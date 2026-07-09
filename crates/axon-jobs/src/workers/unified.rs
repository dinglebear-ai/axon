use std::sync::Arc;

use axon_api::source::{
    ApiError, AuthSnapshot, ErrorStage, JobId, JobKind as UnifiedJobKind, LifecycleStatus,
    PipelinePhase, Timestamp,
};
use futures::FutureExt;
use sqlx::{Row, SqlitePool};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::store_inventory::detect_incompatible_legacy_jobs;
use crate::unified::SqliteUnifiedJobStore;

use super::auth_enforcement::{require_job_scope, required_scope_for_kind};
use super::{POLL_INTERVAL, WORKER_BATCH_LIMIT};

mod helpers;
use helpers::{json_error, parse_enum, parse_uuid, sql_error};

mod runner_registry;
pub use runner_registry::{JobRunnerRegistry, UnifiedJobRunner};

mod terminal;

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

/// Convenience entry point using the default concurrency. Production callers
/// go through [`crate::workers::spawn_unified::spawn_unified_worker`], which
/// always calls [`unified_worker_loop_with_concurrency`] directly with
/// `cfg.unified_worker_concurrency`; this wrapper exists for tests and any
/// future direct caller that doesn't need a configured value.
#[allow(dead_code)]
pub(crate) async fn unified_worker_loop(
    pool: Arc<SqlitePool>,
    notify: Arc<Notify>,
    shutdown: CancellationToken,
    registry: Option<Arc<JobRunnerRegistry>>,
) {
    unified_worker_loop_with_concurrency(pool, notify, shutdown, registry, DEFAULT_CONCURRENCY)
        .await;
}

/// Default concurrency used by [`unified_worker_loop`]'s convenience wrapper.
#[allow(dead_code)]
const DEFAULT_CONCURRENCY: usize = 8;

/// Claim-and-run loop for the unified durable job worker.
///
/// Claimed jobs are run concurrently, bounded by a semaphore sized to
/// `concurrency`, so one slow job (e.g. a long crawl) does not stall every
/// other queued job behind it the way a fully serial claim loop would.
///
/// `crawl_concurrency` is a *second*, independent semaphore that additionally
/// bounds how many `JobKind::Crawl` jobs may run at once, regardless of how
/// high `concurrency` is. Crawl jobs share exactly one Chrome instance, so
/// letting them freely consume up to `concurrency` general worker slots (as
/// every other job kind does) risks CDP session contention and Chrome
/// resource exhaustion — see `Config::crawl_job_concurrency_limit`'s doc
/// comment. The crawl-specific permit is acquired *inside* the spawned task
/// (not in this claim loop) so a crawl job waiting for its slot never blocks
/// the claim loop from picking up other, non-crawl work.
pub(crate) async fn unified_worker_loop_with_concurrency(
    pool: Arc<SqlitePool>,
    notify: Arc<Notify>,
    shutdown: CancellationToken,
    registry: Option<Arc<JobRunnerRegistry>>,
    concurrency: usize,
) {
    unified_worker_loop_with_concurrency_limits(
        pool,
        notify,
        shutdown,
        registry,
        concurrency,
        DEFAULT_CRAWL_CONCURRENCY,
    )
    .await;
}

/// Default crawl-job concurrency used by callers that don't thread a
/// configured value (matches `Config::crawl_job_concurrency_limit`'s
/// default). Production callers go through
/// [`crate::workers::spawn_unified::spawn_unified_worker`], which always
/// passes `cfg.crawl_job_concurrency_limit` explicitly.
const DEFAULT_CRAWL_CONCURRENCY: usize = 1;

pub(crate) async fn unified_worker_loop_with_concurrency_limits(
    pool: Arc<SqlitePool>,
    notify: Arc<Notify>,
    shutdown: CancellationToken,
    registry: Option<Arc<JobRunnerRegistry>>,
    concurrency: usize,
    crawl_concurrency: usize,
) {
    if let Err(error) = ensure_no_incompatible_legacy_jobs(&pool).await {
        tracing::error!(
            error = %error.message,
            code = %error.code,
            "unified worker startup blocked"
        );
        return;
    }
    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency.max(1)));
    let crawl_semaphore = Arc::new(tokio::sync::Semaphore::new(crawl_concurrency.max(1)));
    let mut in_flight: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    let mut wake_count: u64 = 0;
    loop {
        tokio::select! {
            _ = notify.notified() => {}
            _ = tokio::time::sleep(POLL_INTERVAL) => {}
            _ = shutdown.cancelled() => break,
        }
        wake_count = wake_count.wrapping_add(1);
        in_flight.retain(|handle| !handle.is_finished());

        let mut claimed_this_wake = 0usize;
        loop {
            let mut processed = 0usize;
            while processed < WORKER_BATCH_LIMIT && !shutdown.is_cancelled() {
                match claim_next_unified_job_unchecked(&pool).await {
                    Ok(Some(claimed)) => {
                        let permit = match Arc::clone(&semaphore).acquire_owned().await {
                            Ok(permit) => permit,
                            Err(_) => break, // semaphore closed — shutting down
                        };
                        let pool = Arc::clone(&pool);
                        let shutdown = shutdown.clone();
                        let registry = registry.clone();
                        let crawl_semaphore = (claimed.kind == UnifiedJobKind::Crawl)
                            .then(|| Arc::clone(&crawl_semaphore));
                        in_flight.push(tokio::spawn(async move {
                            // Acquire the crawl-specific slot only for crawl
                            // jobs, and only inside the spawned task — this
                            // blocks that task, not the claim loop above, so
                            // other job kinds keep being claimed and run
                            // while a crawl job queues for its Chrome slot.
                            let _crawl_permit = match crawl_semaphore {
                                Some(sem) => match sem.acquire_owned().await {
                                    Ok(permit) => Some(permit),
                                    Err(_) => {
                                        drop(permit);
                                        return; // crawl semaphore closed — shutting down
                                    }
                                },
                                None => None,
                            };
                            run_unified_claimed(&pool, &claimed, &shutdown, registry.as_deref())
                                .await;
                            drop(permit);
                        }));
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
                in_flight = in_flight.len(),
                "unified worker: poll batch complete"
            );
        }
    }
    // Graceful shutdown: let already-claimed jobs finish marking their
    // terminal state (mark_canceled/mark_terminal) rather than abandoning
    // them mid-write.
    for handle in in_flight {
        let _ = handle.await;
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
    claimed: &UnifiedClaimedJob,
    shutdown: &CancellationToken,
    registry: Option<&JobRunnerRegistry>,
) {
    let store = SqliteUnifiedJobStore::new(pool.clone());
    if shutdown.is_cancelled() {
        terminal::mark_canceled(pool, &store, claimed).await;
        return;
    }

    if let Err(error) = terminal::heartbeat(&store, claimed, PipelinePhase::Planning).await {
        tracing::warn!(job_id = %claimed.job_id.0, error = %error.message, "unified worker heartbeat failed");
    }

    if let Some(required) = required_scope_for_kind(claimed.kind)
        && let Err(error) = require_job_scope(&claimed.auth_snapshot, required)
    {
        terminal::fail_unified_claimed(pool, &store, claimed, error).await;
        return;
    }

    // Every unified job kind goes through the dependency-inversion registry
    // the composition layer (axon-services) populates at startup (including
    // `Extract`, since Phase 12's removal of `axon-extract`); kinds with no
    // registered runner keep failing with job_runner.unsupported_stage.

    let Some(runner) = registry.and_then(|registry| registry.get(claimed.kind)) else {
        let error = ApiError::new(
            "job_runner.unsupported_stage",
            ErrorStage::Planning,
            format!(
                "unified durable runner claimed {:?} job {}, but this stage is not wired yet",
                claimed.kind, claimed.job_id.0
            ),
        );
        terminal::fail_unified_claimed(pool, &store, claimed, error).await;
        return;
    };

    // Panic guard: before this cutover, `panic_guard::run_catching` wrapped
    // legacy runner execution so a panic inside a runner got caught and the
    // job marked `failed` immediately. `runner.run(...)` here has no such
    // guard on its own — a panic would unwind straight past both terminal-
    // state branches below, leaving the job stuck `running` forever (the
    // enclosing `tokio::spawn` in `unified_worker_loop_with_concurrency`
    // isolates the panic from crashing the process, but nothing writes the
    // terminal state). `AssertUnwindSafe` is safe here because `runner`,
    // `claimed`, `store`, and `shutdown` are only read, never mutated, across
    // the unwind boundary — any partial state inside the runner's own future
    // is discarded along with the future itself.
    let run_result = std::panic::AssertUnwindSafe(runner.run(claimed, &store, shutdown))
        .catch_unwind()
        .await;

    match run_result {
        Ok(Ok(())) => {
            if let Err(mark_error) = terminal::mark_terminal(
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
        Ok(Err(error)) => {
            terminal::fail_unified_claimed(pool, &store, claimed, error).await;
        }
        Err(panic_payload) => {
            let message = panic_message(&panic_payload);
            tracing::error!(
                job_id = %claimed.job_id.0,
                kind = ?claimed.kind,
                panic = %message,
                "unified worker: runner panicked; marking job failed"
            );
            let error = ApiError::new(
                "job_runner.panicked",
                ErrorStage::Planning,
                format!("job runner panicked: {message}"),
            );
            terminal::fail_unified_claimed(pool, &store, claimed, error).await;
        }
    }
}

/// Best-effort extraction of a human-readable message from a caught panic
/// payload (`Box<dyn Any + Send>`). Panics via `panic!("...")` and
/// `.unwrap()`/`.expect("...")` carry a `&'static str` or `String` payload;
/// anything else falls back to a generic marker rather than failing to report.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

#[cfg(test)]
#[path = "unified_tests.rs"]
mod tests;
