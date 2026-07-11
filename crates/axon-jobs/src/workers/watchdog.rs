use std::sync::Arc;
use std::time::Duration;

use axon_api::source::{JobRecoveryRequest, Timestamp};
use sqlx::SqlitePool;
use tokio_util::sync::CancellationToken;

use super::{WatchdogNotifies, starvation};
use crate::backend::JobKind;
use crate::boundary::JobStore;
use crate::cancel::CancelStore;
use crate::unified::SqliteUnifiedJobStore;
use crate::unified::retention::RetentionCutoffs;
use axon_core::config::Config;

/// Periodic watchdog: sweeps all job tables on a config-driven interval
/// (`cfg.watchdog_sweep_secs`, default 15s) while the process is alive.
///
/// Pairs with `HeartbeatGuard` — heartbeat keeps `updated_at` fresh for live jobs;
/// watchdog reclaims rows whose `updated_at` has gone stale (process died, runner
/// panicked, etc.). After a reclaim, wakes the worker channels whose kind had
/// rows reclaimed — not the others — so untouched lanes stay parked. Each tick
/// also runs the starvation detector, which covers the orthogonal case of a lane
/// that stopped claiming while jobs sit `pending` (no stale `running` row exists
/// for the reclaim path to act on).
pub(super) async fn watchdog_loop(
    pool: Arc<SqlitePool>,
    cfg: Arc<Config>,
    cancel_store: Arc<CancelStore>,
    notifies: WatchdogNotifies,
    shutdown: CancellationToken,
) {
    let stale_threshold_ms =
        (cfg.watchdog_stale_timeout_secs + cfg.watchdog_confirm_secs).max(0) * 1_000i64;
    let max_attempts = cfg.max_job_attempts;
    let starvation_threshold_ms = cfg.worker_starvation_secs.max(0) * 1_000i64;
    let sweep_interval = Duration::from_secs(cfg.watchdog_sweep_secs.max(1) as u64);
    // Differentiated retention (R1-03) runs on its own, much coarser cadence
    // than the per-tick stale-job/starvation sweep above — tracked as a
    // whole-tick counter rather than a second `tokio::time::interval` so it
    // stays inside this loop's single `select!` without adding a race.
    let retention_every_ticks = (cfg.jobs_retention_sweep_secs.max(1) as u64)
        .div_ceil(sweep_interval.as_secs().max(1))
        .max(1);
    let mut ticks_since_retention: u64 = 0;
    let unified_store = SqliteUnifiedJobStore::new((*pool).clone());
    let mut ticker = tokio::time::interval(sweep_interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Skip immediate tick — startup-time reclaim already ran in SqliteJobBackend::init.
    ticker.tick().await;
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => break,
            _ = ticker.tick() => {
                let reclaimed = crate::store::reclaim_stale_running_jobs_detailed(
                    &pool,
                    stale_threshold_ms,
                    max_attempts,
                )
                .await;
                if reclaimed.total() > 0 {
                    cancel_reclaimed_local_tokens(&cancel_store, &reclaimed);
                    // notify_waiters (not notify_one) so all parked lanes
                    // for each kind wake — a single reclaim sweep can free
                    // multiple jobs of the same kind, and embed/ingest run
                    // multiple lanes that share one Notify handle.
                    if reclaimed.count_for(JobKind::Crawl) > 0 {
                        notifies.crawl.notify_waiters();
                    }
                    if reclaimed.count_for(JobKind::Embed) > 0 {
                        notifies.embed.notify_waiters();
                    }
                    if reclaimed.count_for(JobKind::Extract) > 0 {
                        notifies.extract.notify_waiters();
                    }
                    if reclaimed.count_for(JobKind::Ingest) > 0 {
                        notifies.ingest.notify_waiters();
                    }
                }
                // Safety net: detect queues that are starving because a worker
                // lane has silently stopped claiming (a case the stale-running
                // reclaim above is blind to). Logs loudly and kicks the lane.
                starvation::detect_and_recover_starvation(
                    &pool,
                    &notifies,
                    starvation_threshold_ms,
                )
                .await;

                run_unified_sweeps(
                    &pool,
                    &cfg,
                    &unified_store,
                    &notifies,
                    stale_threshold_ms,
                    &mut ticks_since_retention,
                    retention_every_ticks,
                )
                .await;
            }
        }
    }
}

/// Unified-jobs-table sweeps that run every tick after the legacy-family
/// reclaim/starvation pass above: stale-`running` reclaim, deadline
/// expiry (R1-V01), coarser-cadence differentiated retention (R1-03), and
/// the interactive-lane starvation SLO (R1-09). Split out of
/// [`watchdog_loop`] to keep that function under the monolith line cap.
#[allow(clippy::too_many_arguments)]
async fn run_unified_sweeps(
    pool: &SqlitePool,
    cfg: &Config,
    unified_store: &SqliteUnifiedJobStore,
    notifies: &WatchdogNotifies,
    stale_threshold_ms: i64,
    ticks_since_retention: &mut u64,
    retention_every_ticks: u64,
) {
    // Reclaim stale `running` rows in the *unified* jobs table.
    // Before the panic guard in `workers/unified.rs`, a crashed
    // process or an uncaught panic could leave a unified job
    // wedged in `running` forever with nothing to reclaim it
    // (the legacy-family sweep only understands the per-family
    // tables). This mirrors the on-demand `crawl recover`/`embed
    // recover`/etc. CLI/MCP paths (see
    // `axon-services/src/runtime/sqlite/*_bridge.rs::recover`)
    // but runs automatically on the same watchdog cadence so a
    // panic-guard bypass or hard process crash still self-heals.
    let stale_before = Timestamp::from(
        chrono::Utc::now() - chrono::Duration::milliseconds(stale_threshold_ms.max(0)),
    );
    match unified_store
        .recover(JobRecoveryRequest {
            kind: None,
            stale_before: Some(stale_before),
            limit: None,
            older_than_seconds: None,
            dry_run: false,
            allow_without_cutoff: false,
        })
        .await
    {
        Ok(result) if result.jobs_requeued > 0 => {
            tracing::info!(
                requeued = result.jobs_requeued,
                scanned = result.jobs_scanned,
                "watchdog: reclaimed stale unified jobs"
            );
            notifies.unified.notify_waiters();
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(
                error = %error.message,
                code = %error.code,
                "watchdog: unified job reclaim sweep failed"
            );
        }
    }

    // Deadline enforcement (R1-V01): transition past-deadline
    // `running` jobs to `expired` before they'd otherwise wait
    // out a full stale-timeout reclaim cycle.
    match unified_store.expire_past_deadline_jobs().await {
        Ok(expired) if expired > 0 => {
            tracing::info!(expired, "watchdog: expired past-deadline running jobs");
            notifies.unified.notify_waiters();
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(
                error = %error.message,
                code = %error.code,
                "watchdog: deadline expiry sweep failed"
            );
        }
    }

    // Differentiated retention (R1-03): a much coarser cadence
    // than the per-tick work above.
    *ticks_since_retention += 1;
    if *ticks_since_retention >= retention_every_ticks {
        *ticks_since_retention = 0;
        let cutoffs = RetentionCutoffs::from_config(cfg);
        match unified_store.run_retention_sweep(&cutoffs).await {
            Ok(result)
                if result.jobs_pruned > 0
                    || result.events_pruned > 0
                    || result.reservations_pruned > 0
                    || result.artifacts_pruned > 0 =>
            {
                tracing::info!(
                    jobs_pruned = result.jobs_pruned,
                    events_pruned = result.events_pruned,
                    reservations_pruned = result.reservations_pruned,
                    artifacts_pruned = result.artifacts_pruned,
                    "watchdog: differentiated retention sweep pruned rows"
                );
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(
                    error = %error.message,
                    code = %error.code,
                    "watchdog: retention sweep failed"
                );
            }
        }
    }

    // Starvation SLO (R1-09): priority-aware interactive-lane
    // watchdog, distinct from the generic per-kind age-threshold
    // detector above.
    starvation::detect_interactive_starvation(
        pool,
        cfg.jobs_interactive_starvation_slo_secs.max(0) * 1_000i64,
    )
    .await;
}

fn cancel_reclaimed_local_tokens(
    cancel_store: &CancelStore,
    reclaimed: &crate::store::ReclaimedJobs,
) {
    for kind in JobKind::all() {
        for reclaimed_job in reclaimed.jobs_for(*kind) {
            let canceled = reclaimed_job
                .attempt_id
                .as_deref()
                .is_some_and(|attempt_id| cancel_store.cancel_local(reclaimed_job.id, attempt_id));
            tracing::info!(
                table = kind.table_name(),
                job_id = %reclaimed_job.id,
                attempt_id = reclaimed_job.attempt_id.as_deref().unwrap_or("unknown"),
                canceled_local_token = canceled,
                "watchdog: canceled local owner for reclaimed job"
            );
        }
    }
}

#[cfg(test)]
#[path = "watchdog_tests.rs"]
mod tests;
