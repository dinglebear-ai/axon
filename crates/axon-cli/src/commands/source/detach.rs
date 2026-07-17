//! Detached default path for `axon <source>`.
//!
//! Per the command contract (`surfaces/command-contract.md`), the source
//! command without `--wait` must validate + route, enqueue a durable
//! `JobKind::Source` row, and return immediately with a job descriptor —
//! blocking is the `--wait true` opt-in. Detached jobs must not require a
//! manually started `axon serve`: after enqueueing, this module makes sure a
//! worker process exists, spawning a detached `axon jobs worker` when no
//! process currently holds the drain lock. The spawned worker drains the
//! queue, lingers `jobs.worker-idle-exit-secs`, and exits.

use std::error::Error;
use std::path::PathBuf;

use axon_api::source::{AuthSnapshot, SourceRequest, SourceResult};
use axon_core::config::Config;
use axon_core::logging::log_info;
use axon_core::ui::muted;
use axon_services::context::ServiceContext;
use axon_services::runtime::{WorkerDrainLock, drain_lock_path};
use axon_services::source::enqueue::enqueue_source;

/// Enqueue `request` as a detached source job. Returns the queued
/// `SourceResult` for rendering; the caller follows up with
/// [`ensure_worker_process`] after rendering the descriptor.
pub(crate) async fn enqueue_source_detached(
    service_context: &ServiceContext,
    request: SourceRequest,
) -> Result<SourceResult, Box<dyn Error>> {
    let store = service_context
        .job_store()
        .ok_or("detached source enqueue requires a unified job store")?;

    // Local CLI invocations are the trusted-local context from the auth
    // contract; the snapshot rides in the job row so the executing worker
    // enforces the same policy (`Local` scope gates local-path sources).
    enqueue_source(
        request,
        store.as_ref(),
        Some(AuthSnapshot::trusted_cli(env!("CARGO_PKG_VERSION"))),
    )
    .await
    .map_err(|err| -> Box<dyn Error> { err.to_string().into() })
    // No notify here: this is an enqueue-only context with no in-process
    // worker to wake. Pickup is by the auto-spawned worker (which wakes its own
    // runtime on start) or an already-running server's poll.
}

/// Make sure some process will drain the queue. Best-effort by design: a
/// failed probe or spawn must not fail the enqueue (the job is durable and
/// any later worker — `axon serve`, a manual `axon jobs worker`, the next
/// auto-spawn — picks it up), so problems are reported as warnings only.
pub(crate) async fn ensure_worker_process(cfg: &Config) {
    if !cfg.jobs_auto_worker {
        note(
            cfg,
            "worker autostart disabled (jobs.auto-worker=false); run `axon jobs worker` or `axon serve` to process the job",
        );
        return;
    }

    // The lock lives beside the jobs DB so the probe follows the same queue the
    // worker would drain even under AXON_SQLITE_PATH (axon_rust-x4gxr.14).
    let lock_path = drain_lock_path(&cfg.sqlite_path);
    match WorkerDrainLock::is_held(&lock_path).await {
        Ok(true) => {
            note(cfg, "worker process already active; job will be picked up");
            return;
        }
        Ok(false) => {}
        Err(error) => {
            note(
                cfg,
                &format!("worker liveness probe failed ({error}); attempting worker autostart"),
            );
        }
    }

    match spawn_detached_worker() {
        Ok((pid, log_path)) => note(
            cfg,
            &format!(
                "started background worker (pid {pid}); logs: {}",
                log_path.display()
            ),
        ),
        Err(error) => note(
            cfg,
            &format!(
                "failed to start background worker ({error}); run `axon jobs worker` or `axon serve` to process the job"
            ),
        ),
    }
}

/// Spawn `axon jobs worker` fully detached from this process: no inherited
/// stdio (output goes to a private log file under the data dir) and its own
/// process group, so it survives the parent CLI exiting and terminal signals.
/// Returns the child pid and the log path.
fn spawn_detached_worker() -> Result<(u32, PathBuf), Box<dyn Error>> {
    let exe = std::env::current_exe()?;
    let logs_dir = axon_core::paths::axon_data_base_dir().join("logs");
    // Own the hardening rather than relying on init_tracing having already
    // created logs/ at 0o700 (which AXON_LOG_PATH or a degraded tracing init
    // can skip). ensure_private_dir → 0o700 dir; open_private_append → 0o600 +
    // O_NOFOLLOW so worker output (URLs, local paths) is never world-readable
    // and a symlink at the path is rejected (axon_rust-x4gxr.9).
    axon_core::paths::ensure_private_dir(&logs_dir)?;
    let log_path = logs_dir.join("auto-worker.log");
    rotate_if_oversized(&log_path);
    let log = axon_core::paths::open_private_append(&log_path)?;
    let log_err = log.try_clone()?;

    let mut command = std::process::Command::new(exe);
    command
        .args(["jobs", "worker"])
        .stdin(std::process::Stdio::null())
        .stdout(log)
        .stderr(log_err);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // New session-less process group: terminal Ctrl-C aimed at the parent
        // CLI never reaches the worker.
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
        command.creation_flags(0x0000_0008 | 0x0000_0200);
    }

    let child = command.spawn()?;
    Ok((child.id(), log_path))
}

/// Cap `auto-worker.log` growth: when it exceeds 5 MiB, rename it to
/// `auto-worker.log.1` (replacing any previous `.1`) so a fresh worker starts a
/// new file. Single-generation rotation is enough to keep the data dir bounded
/// without a full rotating-appender (`axon_rust-x4gxr.13`). Best-effort — a
/// failure here must not block spawning the worker.
fn rotate_if_oversized(log_path: &std::path::Path) {
    const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
    let too_big = std::fs::metadata(log_path)
        .map(|m| m.len() > MAX_LOG_BYTES)
        .unwrap_or(false);
    if too_big {
        let _ = std::fs::rename(log_path, log_path.with_extension("log.1"));
    }
}

/// Route operator notes around stdout JSON: human runs get muted stdout
/// lines, `--json` runs keep stdout machine-parseable and use stderr.
fn note(cfg: &Config, message: &str) {
    if cfg.json_output {
        log_info(message);
    } else {
        println!("  {}", muted(message));
    }
}
