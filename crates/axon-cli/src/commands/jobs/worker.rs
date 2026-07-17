//! `axon jobs worker` — host the in-process unified worker runtime in a
//! standalone process.
//!
//! Used two ways:
//! - operators run it manually to drain/serve the durable queue without a
//!   full `axon serve`;
//! - the CLI auto-spawns it (detached) after `axon <source>` enqueues a
//!   detached job and no worker process currently holds the drain lock.
//!
//! **Lock before context.** This entry acquires [`WorkerDrainLock`] *before*
//! constructing the worker-bearing `ServiceContext` (which opens + integrity-
//! checks the jobs DB and starts the claim loop). A losing invocation therefore
//! exits in milliseconds having claimed nothing — no duplicate-spawn storm and
//! no jobs claimed-then-orphaned by a short-lived loser (`axon_rust-x4gxr.3`).
//! `axon serve` / HTTP `axon mcp` hold the same lock for their lifetime
//! (`axon_rust-x4gxr.2`), so a worker started while a server is running also
//! exits immediately. Job-claiming correctness never depends on the lock — the
//! unified store's transactional claim keeps every job single-owner even if two
//! drainers race.

use std::error::Error;
use std::sync::Arc;

use axon_core::config::Config;
use axon_core::ui::{accent, muted, primary};
use axon_services::context::ServiceContext;
use axon_services::jobs::worker_loop::{WorkerLoopOptions, run_worker_until_idle};
use axon_services::runtime::{WorkerDrainLock, drain_lock_path};

/// Standalone `axon jobs worker` entrypoint. Owns its own `ServiceContext` so
/// the drain lock is taken before any job-claiming runtime exists.
pub(crate) async fn run_worker_process(cfg: Arc<Config>) -> Result<(), Box<dyn Error>> {
    let lock_path = drain_lock_path(&cfg.sqlite_path);
    let Some(lock) = WorkerDrainLock::try_hold(&lock_path)
        .await
        .map_err(|err| -> Box<dyn Error> { err.to_string().into() })?
    else {
        emit_not_acquired(&cfg);
        return Ok(());
    };

    let idle_exit_secs = resolve_idle_exit_secs(
        super::parse_u64_flag(&cfg, "--idle-exit-secs")?,
        cfg.jobs_worker_idle_exit_secs,
    );

    // Only after the lock is held do we build the claim-capable runtime.
    let service_context = ServiceContext::new_with_workers(Arc::clone(&cfg))
        .await
        .map_err(|e| -> Box<dyn Error> { e })?;

    emit_start(&cfg, idle_exit_secs);

    let report =
        run_worker_until_idle(&service_context, WorkerLoopOptions { idle_exit_secs }).await?;

    // Release the lock BEFORE emitting the report so the window between the
    // loop's final "queue empty" observation and lock release contains no I/O —
    // an enqueue landing here re-probes an unheld lock and spawns its own worker
    // rather than being stranded (`axon_rust-x4gxr.5`).
    drop(service_context);
    drop(lock);

    emit_report(&cfg, idle_exit_secs, report);
    Ok(())
}

/// Resolve the worker idle-exit window: an explicit `--idle-exit-secs` flag
/// wins, else the configured `jobs.worker-idle-exit-secs`; clamped to one day so
/// a fat-fingered value can't wedge a worker near-permanently. `0` (run until
/// stopped) survives the clamp.
fn resolve_idle_exit_secs(flag: Option<u64>, default: u64) -> u64 {
    flag.unwrap_or(default).min(86_400)
}

fn emit_not_acquired(cfg: &Config) {
    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({
                "worker": { "acquired_lock": false, "reason": "another worker process is active" }
            })
        );
    } else {
        println!(
            "  {}",
            muted("Another worker process is already active for this queue; exiting.")
        );
    }
}

fn emit_start(cfg: &Config, idle_exit_secs: u64) {
    if cfg.json_output {
        return;
    }
    println!("  {} {}", primary("Worker"), accent("unified job queue"));
    let lifetime = if idle_exit_secs == 0 {
        "until stopped".to_string()
    } else {
        format!("until idle for {idle_exit_secs}s")
    };
    println!("  {}", muted(&format!("Running {lifetime}.")));
}

fn emit_report(
    cfg: &Config,
    idle_exit_secs: u64,
    report: axon_services::jobs::worker_loop::WorkerLoopReport,
) {
    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({
                "worker": {
                    "acquired_lock": true,
                    "idle_exit_secs": idle_exit_secs,
                    "elapsed_secs": report.elapsed_secs,
                    "recovered_jobs": report.recovered_jobs,
                }
            })
        );
    } else {
        println!(
            "  {}",
            muted(&format!(
                "Queue idle — exiting after {}s ({} stale job(s) recovered).",
                report.elapsed_secs, report.recovered_jobs
            ))
        );
    }
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
