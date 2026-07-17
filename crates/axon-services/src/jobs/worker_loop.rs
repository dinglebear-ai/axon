//! Worker-process loop for `axon jobs worker` and the CLI auto-spawned
//! drainer.
//!
//! The caller is expected to hold a [`crate::runtime::WorkerDrainLock`] and a
//! worker-bearing `ServiceContext` (in-process workers already spawned). This
//! loop only decides *when the process may exit*: it watches the durable
//! queue, periodically reclaims stale attempts (so a job orphaned by a killed
//! worker process is re-run rather than stranded in `running`), and returns
//! once the queue has been continuously quiet for the configured idle window.
//!
//! It watches [`WORKER_JOB_KINDS`] — the exact set of kinds this process's
//! in-process runtime executes ([`crate::runtime::job_runners::build_registry`])
//! — so the process never idle-exits while it still owns a running job of any
//! executable kind.
//!
//! Two sibling poll loops share the "poll `has_active_jobs` until quiet" shape
//! but answer different questions: [`crate::runtime::SqliteServiceRuntime::drain_jobs`]
//! (behind `start_worker`, used by `--wait true`) blocks until the queue is
//! empty *once*; this loop runs until the queue has been *continuously* idle
//! for `idle_exit_secs`. They are intentionally separate.

use std::error::Error;
use std::time::Duration;

use crate::context::ServiceContext;
use crate::runtime::job_runners::WORKER_JOB_KINDS;

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const RECOVER_SWEEP_SECS: u64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerLoopOptions {
    /// Exit after the queue has been continuously idle for this many seconds.
    /// `0` disables idle exit (run until the process is stopped).
    pub idle_exit_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorkerLoopReport {
    pub elapsed_secs: u64,
    pub recovered_jobs: u64,
}

/// Run the worker loop until the queue is idle (see [`WorkerLoopOptions`]).
pub async fn run_worker_until_idle(
    service_context: &ServiceContext,
    options: WorkerLoopOptions,
) -> Result<WorkerLoopReport, Box<dyn Error>> {
    // tokio Instants track the runtime clock, keeping the loop testable under
    // tokio's paused virtual time; in production they equal std Instants.
    let started = tokio::time::Instant::now();
    let mut report = WorkerLoopReport::default();
    let mut last_recover_sweep: Option<tokio::time::Instant> = None;
    let mut idle_since: Option<tokio::time::Instant> = None;

    // Wake the in-process unified worker immediately instead of waiting out
    // its poll interval — the caller usually just enqueued something.
    service_context.notify_unified();

    loop {
        if last_recover_sweep
            .map(|at| at.elapsed().as_secs() >= RECOVER_SWEEP_SECS)
            .unwrap_or(true)
        {
            report.recovered_jobs += recover_sweep(service_context).await;
            last_recover_sweep = Some(tokio::time::Instant::now());
        }

        if queue_active(service_context).await? {
            idle_since = None;
        } else if options.idle_exit_secs == 0 {
            // Idle exit disabled — keep serving.
        } else {
            let since = idle_since.get_or_insert_with(tokio::time::Instant::now);
            if since.elapsed().as_secs() >= options.idle_exit_secs {
                break;
            }
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }

    report.elapsed_secs = started.elapsed().as_secs();
    Ok(report)
}

/// True while any watched kind has pending or running rows.
async fn queue_active(service_context: &ServiceContext) -> Result<bool, Box<dyn Error>> {
    for kind in WORKER_JOB_KINDS {
        if service_context
            .jobs
            .has_active_jobs(*kind)
            .await
            .map_err(super::downgrade)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Reclaim stale attempts for every watched kind. Best-effort: a failed sweep
/// must not kill the worker loop, so errors are logged and swallowed.
async fn recover_sweep(service_context: &ServiceContext) -> u64 {
    let mut recovered = 0;
    for kind in WORKER_JOB_KINDS {
        match super::recover_jobs(service_context, *kind).await {
            Ok(count) => recovered += count,
            Err(error) => {
                tracing::warn!(?kind, %error, "worker loop recover sweep failed");
            }
        }
    }
    recovered
}

#[cfg(test)]
#[path = "worker_loop_tests.rs"]
mod tests;
