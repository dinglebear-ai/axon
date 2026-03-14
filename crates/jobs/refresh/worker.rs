use super::{
    TABLE, ensure_schema_once, processor::process_refresh_job, start_refresh_job_with_pool,
};
use crate::crates::core::config::Config;
use crate::crates::core::logging::log_info;
use crate::crates::jobs::common::{claim_pending_by_id, make_pool, reclaim_stale_running_jobs};
use crate::crates::jobs::worker_lane::{
    ProcessFn, WorkerConfig, resolve_lane_count, run_job_worker,
};
use std::error::Error;
use std::sync::Arc;

pub async fn run_refresh_worker(cfg: &Config) -> Result<(), Box<dyn Error>> {
    log_info(&format!(
        "worker_start worker=refresh queue={}",
        cfg.refresh_queue
    ));

    let pool = make_pool(cfg).await?;
    ensure_schema_once(&pool).await?;

    let wc = WorkerConfig {
        table: TABLE,
        queue_name: cfg.refresh_queue.clone(),
        job_kind: "refresh",
        consumer_tag_prefix: "refresh-worker",
        lane_count: resolve_lane_count("AXON_REFRESH_LANES", 1, 4),
    };

    let process_fn: ProcessFn =
        Arc::new(|cfg, pool, id| Box::pin(process_refresh_job(cfg, pool, id)));

    run_job_worker(cfg, pool, &wc, process_fn).await
}

pub async fn run_refresh_once(
    cfg: &Config,
    urls: &[String],
) -> Result<serde_json::Value, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema_once(&pool).await?;
    let id = start_refresh_job_with_pool(&pool, cfg, urls, false).await?;
    if !claim_pending_by_id(&pool, TABLE, id).await? {
        return Err(format!("failed to claim newly created refresh job {id}").into());
    }

    process_refresh_job(cfg.clone(), pool.clone(), id).await;

    let result_json = sqlx::query_scalar::<_, Option<serde_json::Value>>(
        "SELECT result_json FROM axon_refresh_jobs WHERE id=$1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await?
    .unwrap_or_else(|| serde_json::json!({}));

    Ok(result_json)
}

pub async fn recover_stale_refresh_jobs_startup(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema_once(&pool).await?;
    let stats = reclaim_stale_running_jobs(
        &pool,
        TABLE,
        "refresh",
        cfg.watchdog_stale_timeout_secs,
        cfg.watchdog_confirm_secs,
        "startup",
    )
    .await?;
    log_info(&format!(
        "refresh watchdog startup reclaimed={} candidates={}",
        stats.reclaimed_jobs, stats.stale_candidates
    ));
    Ok(stats.reclaimed_jobs)
}
