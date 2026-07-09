use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::unified::{self, JobRunnerRegistry};

/// Spawn the unified durable worker task.
///
/// Dispatches every unified `JobKind` (including `Extract`, since Phase 12's
/// removal of `axon-extract`) through the injected `JobRunnerRegistry` when
/// one is supplied (built by axon-services at composition time). Kinds with
/// no registered runner keep failing with `job_runner.unsupported_stage` —
/// spawning unconditionally is safe.
pub(super) fn spawn_unified_worker(
    pool: Arc<SqlitePool>,
    unified_notify: Arc<Notify>,
    shutdown: CancellationToken,
    job_runner_registry: Option<Arc<JobRunnerRegistry>>,
    concurrency: usize,
    crawl_concurrency: usize,
) -> tokio::task::JoinHandle<()> {
    let registered_kinds = job_runner_registry
        .as_deref()
        .map(|registry| {
            [
                axon_api::source::JobKind::Memory,
                axon_api::source::JobKind::ProviderProbe,
                axon_api::source::JobKind::Research,
                axon_api::source::JobKind::Extract,
            ]
            .into_iter()
            .filter(|kind| registry.contains(*kind))
            .count()
        })
        .unwrap_or(0);
    tracing::info!(
        worker = "unified",
        concurrency,
        crawl_concurrency,
        registered_kinds,
        "jobs: spawning unified worker"
    );
    tokio::spawn(unified::unified_worker_loop_with_concurrency_limits(
        pool,
        unified_notify,
        shutdown,
        job_runner_registry,
        concurrency,
        crawl_concurrency,
    ))
}
