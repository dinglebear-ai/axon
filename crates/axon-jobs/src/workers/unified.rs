use std::sync::Arc;

use axon_api::source::{
    ApiError, AuthSnapshot, ErrorStage, JobId, JobKind as UnifiedJobKind, PipelinePhase,
};
use futures::FutureExt;
use sqlx::SqlitePool;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::unified::SqliteUnifiedJobStore;

use super::auth_enforcement::{require_job_scope, required_scope_for_kind};
use super::{POLL_INTERVAL, WORKER_BATCH_LIMIT, WorkerActivity};

mod helpers;

mod claim;
#[allow(unused_imports)] // only used by #[cfg(test)] call sites in sibling test files
pub(crate) use claim::claim_next_unified_job;
use claim::claim_next_unified_job_unchecked;

mod runner_registry;
pub use runner_registry::{JobRunnerRegistry, UnifiedJobOutcome, UnifiedJobRunner};

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
/// calls the activity-aware loop with `cfg.unified_worker_concurrency`; this
/// wrapper exists for tests and any future direct caller that doesn't need a
/// configured value.
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
/// Every `Source` job (web, git, feeds, registries, Reddit, YouTube,
/// CLI/MCP tools, local paths — plus `map`) is *also* bounded by a second,
/// independent semaphore, regardless of how high `concurrency` is. This is a
/// general per-source-kind rail, not a web/Chrome-specific one: several
/// source kinds share constrained external resources (a single Chrome
/// instance for web/render-backed acquisition, upstream API rate limits for
/// other adapters), so letting them freely consume up to `concurrency`
/// general worker slots risks starving other job kinds or exhausting a
/// shared resource.
///
/// **Both permits are acquired before a job is ever claimed from the
/// database, not after.** Each pass first makes a *non-blocking* attempt at
/// the source-specific permit — this tells us, before touching the DB,
/// whether we are currently allowed to claim a `Source` job — then blocks on
/// the general permit (legitimate backpressure: by this point we already
/// know we can use it), and only then runs the claim query, telling it
/// whether a `Source` job may be selected this time. A row is therefore
/// never flipped to `running` unless a worker is about to actually run it;
/// no claimed job ever sits parked waiting on a permit. This matters for two
/// reasons: (1) a `Source` job that can't get the source-specific permit
/// right now is left `queued` rather than claimed, so it never occupies a
/// general-concurrency slot while parked — other job kinds keep being
/// claimed and run even while the source lane is completely full; and (2) a
/// claimed job's first heartbeat is written essentially immediately (no
/// permit wait stands between claim and the task starting), so it can never
/// sit `running` with a stale heartbeat long enough for the watchdog to
/// reclaim it as abandoned while its task is still alive.
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
        DEFAULT_SOURCE_CONCURRENCY,
    )
    .await;
}

/// Default source-job concurrency used by callers that don't thread a
/// configured value (matches `Config::source_job_concurrency_limit`'s
/// default). Production callers go through
/// [`crate::workers::spawn_unified::spawn_unified_worker`], which always
/// passes `cfg.source_job_concurrency_limit` and shared activity state
/// explicitly.
const DEFAULT_SOURCE_CONCURRENCY: usize = 4;

pub(crate) async fn unified_worker_loop_with_concurrency_limits(
    pool: Arc<SqlitePool>,
    notify: Arc<Notify>,
    shutdown: CancellationToken,
    registry: Option<Arc<JobRunnerRegistry>>,
    concurrency: usize,
    source_concurrency: usize,
) {
    unified_worker_loop_with_concurrency_limits_and_activity(
        pool,
        notify,
        Arc::new(WorkerActivity::default()),
        shutdown,
        registry,
        concurrency,
        source_concurrency,
    )
    .await;
}

pub(crate) async fn unified_worker_loop_with_concurrency_limits_and_activity(
    pool: Arc<SqlitePool>,
    notify: Arc<Notify>,
    activity: Arc<WorkerActivity>,
    shutdown: CancellationToken,
    registry: Option<Arc<JobRunnerRegistry>>,
    concurrency: usize,
    source_concurrency: usize,
) {
    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency.max(1)));
    let source_semaphore = Arc::new(tokio::sync::Semaphore::new(source_concurrency.max(1)));
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
                // Speculatively reserve a source-specific slot *before*
                // touching the database. This is a non-blocking attempt: if
                // the source lane is full we simply proceed without one
                // (`allow_source = false`), which tells the claim query
                // below to skip `Source` rows entirely rather than flip one
                // to `running` with nowhere to run it. Because this process
                // is the only claimer of `source_semaphore` (spawned tasks
                // only ever hold/release a permit handed to them, they never
                // acquire one themselves), there is no race between this
                // check and the claim query that follows.
                let source_permit = match Arc::clone(&source_semaphore).try_acquire_owned() {
                    Ok(permit) => Some(permit),
                    Err(tokio::sync::TryAcquireError::NoPermits) => None,
                    Err(tokio::sync::TryAcquireError::Closed) => break, // shutting down
                };
                let allow_source = source_permit.is_some();

                // The general permit is allowed to block: by this point we
                // already know we could use it (either this is a non-Source
                // job, or it's a Source job and we already hold the narrower
                // permit above), so waiting here is real backpressure, not a
                // wasted reservation. Race it against shutdown so a
                // cancellation during a long wait unblocks the loop
                // immediately instead of leaving it parked until some other
                // task's permit frees up.
                let permit = tokio::select! {
                    biased;
                    _ = shutdown.cancelled() => break,
                    result = Arc::clone(&semaphore).acquire_owned() => match result {
                        Ok(permit) => permit,
                        Err(_) => break, // semaphore closed — shutting down
                    },
                };

                match claim_next_unified_job_unchecked(&pool, allow_source).await {
                    Ok(Some(claimed)) => {
                        // We may be holding a speculative source permit for
                        // a job that, in the end, wasn't the one claimed
                        // (e.g. a higher-priority or earlier-queued
                        // non-Source job won instead) — release it right
                        // away rather than holding it idle for the lifetime
                        // of an unrelated job.
                        let source_permit = if claimed.kind == UnifiedJobKind::Source {
                            debug_assert!(
                                source_permit.is_some(),
                                "claim query selected a Source job without a held source permit"
                            );
                            source_permit
                        } else {
                            None
                        };
                        let pool = Arc::clone(&pool);
                        let shutdown = shutdown.clone();
                        let registry = registry.clone();
                        // Increment before spawning so a task can never begin
                        // executing without being visible to the process-level
                        // idle monitor. The durable running row covers the
                        // short interval between the claim transaction and
                        // this process-local handoff.
                        let activity_guard = activity.begin();
                        in_flight.push(tokio::spawn(async move {
                            let _activity_guard = activity_guard;
                            let _source_permit = source_permit;
                            run_unified_claimed(&pool, &claimed, &shutdown, registry.as_deref())
                                .await;
                            drop(permit);
                        }));
                        processed += 1;
                    }
                    Ok(None) => break, // nothing eligible given current capacity
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

/// Test-only entry point: exercises the same terminal-failure write path as
/// `fail_unified_claimed`/`mark_terminal` (including their unconditional
/// `cooldown_until = NULL` clear) without requiring a full claim + registered
/// runner round-trip.
#[cfg(test)]
pub(crate) async fn mark_job_failed_for_tests(
    pool: &SqlitePool,
    job_id: JobId,
) -> Result<(), ApiError> {
    let attempt: i64 = sqlx::query_scalar("SELECT attempt FROM jobs WHERE job_id = ?")
        .bind(job_id.0.to_string())
        .fetch_one(pool)
        .await
        .map_err(helpers::sql_error)?;
    let error = ApiError::new(
        "job_runner.test_failure",
        ErrorStage::Publishing,
        "synthetic test failure",
    );
    terminal::mark_terminal(
        pool,
        &UnifiedClaimedJob {
            job_id,
            kind: UnifiedJobKind::Source,
            attempt: attempt.max(1) as u32,
            request_json: None,
            auth_snapshot: AuthSnapshot::default(),
        },
        axon_api::source::LifecycleStatus::Failed,
        PipelinePhase::Complete,
        None,
        Some(error),
    )
    .await
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
    let job_cancel = super::cancel_registry::register(claimed.job_id, claimed.attempt, shutdown);
    if let Err(error) = terminal::heartbeat(&store, claimed, PipelinePhase::Planning).await {
        tracing::warn!(job_id = %claimed.job_id.0, error = %error.message, "unified worker heartbeat failed");
        if cancellation_requested(pool, claimed).await {
            job_cancel.cancel();
            super::cancel_registry::unregister(claimed.job_id, claimed.attempt);
            terminal::mark_canceled(pool, &store, claimed).await;
            return;
        }
    }

    if let Some(required) = required_scope_for_kind(claimed.kind)
        && let Err(error) = require_job_scope(&claimed.auth_snapshot, required)
    {
        super::cancel_registry::unregister(claimed.job_id, claimed.attempt);
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
        super::cancel_registry::unregister(claimed.job_id, claimed.attempt);
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
    let run_result = std::panic::AssertUnwindSafe(runner.run(claimed, &store, &job_cancel))
        .catch_unwind()
        .await;

    super::cancel_registry::unregister(claimed.job_id, claimed.attempt);
    if job_cancel.is_cancelled() {
        terminal::mark_canceled(pool, &store, claimed).await;
        return;
    }

    match run_result {
        Ok(Ok(outcome)) => {
            if let Err(mark_error) = terminal::mark_terminal(
                pool,
                claimed,
                outcome.status,
                PipelinePhase::Complete,
                outcome.counts,
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

async fn cancellation_requested(pool: &SqlitePool, claimed: &UnifiedClaimedJob) -> bool {
    sqlx::query_scalar::<_, String>("SELECT status FROM jobs WHERE job_id = ? AND attempt = ?")
        .bind(claimed.job_id.0.to_string())
        .bind(claimed.attempt as i64)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .is_some_and(|status| status == "canceling" || status == "canceled")
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
