use std::sync::Arc;

use axon_core::config::Config;
use sqlx::SqlitePool;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::unified::{self, JobRunnerRegistry};

/// Spawn the unified durable worker task.
///
/// Always executes `JobKind::Extract` directly (its real work is a pure
/// domain call reachable from axon-jobs), and dispatches every other kind
/// through the injected `JobRunnerRegistry` when one is supplied (built by
/// axon-services at composition time). Kinds with no registered runner keep
/// failing with `job_runner.unsupported_stage` — spawning unconditionally is
/// safe.
pub(super) fn spawn_unified_worker(
    pool: Arc<SqlitePool>,
    cfg: Arc<Config>,
    unified_notify: Arc<Notify>,
    shutdown: CancellationToken,
    job_runner_registry: Option<Arc<JobRunnerRegistry>>,
) -> tokio::task::JoinHandle<()> {
    let registered_kinds = job_runner_registry
        .as_deref()
        .map(|registry| {
            [
                axon_api::source::JobKind::Memory,
                axon_api::source::JobKind::ProviderProbe,
                axon_api::source::JobKind::Research,
            ]
            .into_iter()
            .filter(|kind| registry.contains(*kind))
            .count()
        })
        .unwrap_or(0);
    tracing::info!(
        worker = "unified",
        lanes = 1,
        registered_kinds,
        "jobs: spawning unified worker"
    );
    tokio::spawn(unified::unified_worker_loop(
        pool,
        cfg,
        unified_notify,
        shutdown,
        job_runner_registry,
    ))
}
