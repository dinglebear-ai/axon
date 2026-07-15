use std::sync::Arc;
use std::time::Duration;

use axon_api::source::{JobRecoveryRequest, Timestamp};
use axon_core::config::Config;
use sqlx::SqlitePool;
use tokio_util::sync::CancellationToken;

use super::{WatchdogNotifies, starvation};
use crate::boundary::JobStore;
use crate::unified::SqliteUnifiedJobStore;
use crate::unified::retention::RetentionCutoffs;

/// Periodic watchdog for unified jobs.
pub(super) async fn watchdog_loop(
    pool: Arc<SqlitePool>,
    cfg: Arc<Config>,
    notifies: WatchdogNotifies,
    shutdown: CancellationToken,
) {
    let stale_threshold_ms =
        (cfg.watchdog_stale_timeout_secs + cfg.watchdog_confirm_secs).max(0) * 1_000i64;
    let sweep_interval = Duration::from_secs(cfg.watchdog_sweep_secs.max(1) as u64);
    let retention_every_ticks = (cfg.jobs_retention_sweep_secs.max(1) as u64)
        .div_ceil(sweep_interval.as_secs().max(1))
        .max(1);
    let mut ticks_since_retention: u64 = 0;
    let unified_store = SqliteUnifiedJobStore::new((*pool).clone());
    let mut ticker = tokio::time::interval(sweep_interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await;
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => break,
            _ = ticker.tick() => {
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

    starvation::detect_interactive_starvation(
        pool,
        cfg.jobs_interactive_starvation_slo_secs.max(0) * 1_000i64,
    )
    .await;
}
