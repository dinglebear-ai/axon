use super::{
    FreshnessDispatchOutcome, FreshnessError, FreshnessRequestPayload, FreshnessRequestV1,
    SafeReplayConfigV1, freshness_lease_ttl_ms, replay_config, to_freshness_error,
    validate_freshness_payload_for_dispatch,
};
use crate::context::ServiceContext;
use crate::embed::embed_start_with_context;
use crate::ingest::ingest_start_with_context;
use crate::scrape::scrape_batch_with_optional_embed;
use axon_api::ingest::target_label;
use axon_core::config::Config;
use axon_core::redact::redact_secrets;
use axon_jobs::backend::JobKind;
use axon_jobs::config_snapshot::{config_snapshot_json, ingest_config_json};
use axon_jobs::freshness::{
    FRESHNESS_RUN_STATUS_COMPLETED, FRESHNESS_RUN_STATUS_ENQUEUED, FRESHNESS_RUN_STATUS_FAILED,
    FRESHNESS_RUN_STATUS_SKIPPED_ACTIVE_JOB, FreshnessDef, FreshnessRun,
    create_freshness_run_with_pool, finish_freshness_run_with_pool, heartbeat_freshness_run,
    lease_due_freshness, lease_freshness_for_manual_run, list_freshness_runs_with_pool,
    prune_freshness_runs_for_retention, reclaim_current_stale_freshness_leases,
    set_freshness_run_dispatched_job_with_pool,
};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use uuid::Uuid;

pub async fn run_now(
    service_context: &ServiceContext,
    id: Uuid,
) -> Result<FreshnessRun, FreshnessError> {
    let pool = service_context
        .jobs
        .sqlite_pool()
        .ok_or("freshness schedules require the SQLite job runtime")?;
    let now = axon_jobs::store::now_ms();
    let Some(def) = lease_freshness_for_manual_run(
        &pool,
        id,
        now,
        freshness_lease_ttl_ms(service_context.cfg()),
    )
    .await
    .map_err(to_freshness_error)?
    else {
        return Err("freshness schedule is disabled, missing, or already running".into());
    };
    run_leased_freshness_def(service_context.clone(), Arc::clone(&pool), def).await
}

pub fn spawn_freshness_scheduler(service_context: ServiceContext) {
    let Some(pool) = service_context.jobs.sqlite_pool() else {
        tracing::warn!("freshness scheduler disabled: SQLite runtime is unavailable");
        return;
    };
    let tick_secs = service_context.cfg.freshness_tick_secs.max(1);
    let lease_ttl_ms = freshness_lease_ttl_ms(service_context.cfg());
    let max_due = service_context.cfg.freshness_max_due_per_tick.clamp(1, 100);
    let max_concurrent = service_context.cfg.freshness_max_concurrent_runs.max(1);
    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    tokio::spawn(async move {
        run_scheduler_maintenance(&pool, service_context.cfg()).await;
        let mut interval = tokio::time::interval(Duration::from_secs(tick_secs));
        interval.tick().await;
        loop {
            interval.tick().await;
            run_scheduler_maintenance(&pool, service_context.cfg()).await;
            let available = semaphore.available_permits().min(max_due as usize) as i64;
            if available == 0 {
                continue;
            }
            let due = match lease_due_freshness(
                &pool,
                axon_jobs::store::now_ms(),
                lease_ttl_ms,
                available,
            )
            .await
            {
                Ok(due) => due,
                Err(err) => {
                    tracing::warn!(error = %err, "freshness scheduler lease sweep failed");
                    continue;
                }
            };
            for def in due {
                let Ok(permit) = Arc::clone(&semaphore).acquire_owned().await else {
                    continue;
                };
                let ctx = service_context.clone();
                let pool = Arc::clone(&pool);
                tokio::spawn(async move {
                    let _permit = permit;
                    if let Err(err) = run_leased_freshness_def(ctx, pool, def).await {
                        tracing::warn!(error = %redact_secrets(&err.to_string()), "freshness run failed");
                    }
                });
            }
        }
    });
}

async fn run_scheduler_maintenance(pool: &sqlx::SqlitePool, cfg: &Config) {
    match reclaim_current_stale_freshness_leases(pool).await {
        Ok(count) if count > 0 => {
            tracing::info!(count, "freshness scheduler reclaimed stale leases");
        }
        Ok(_) => {}
        Err(err) => tracing::warn!(error = %err, "freshness scheduler lease reclaim failed"),
    }
    prune_retained_runs(pool, cfg).await;
}

async fn run_leased_freshness_def(
    service_context: ServiceContext,
    pool: Arc<sqlx::SqlitePool>,
    def: FreshnessDef,
) -> Result<FreshnessRun, FreshnessError> {
    let run = create_freshness_run_with_pool(&pool, def.id, None)
        .await
        .map_err(to_freshness_error)?;
    let heartbeat = spawn_heartbeat(Arc::clone(&pool), def.id, run.id, service_context.cfg());
    let outcome = dispatch_freshness(&service_context, &def).await;
    heartbeat.abort();

    let (status, dispatched_job_id, result_json, error_text) = match outcome {
        Ok(outcome) => (
            outcome.status,
            outcome.dispatched_job_id,
            Some(outcome.result_json),
            None,
        ),
        Err(err) => (
            FRESHNESS_RUN_STATUS_FAILED.to_string(),
            None,
            None,
            Some(redact_secrets(&err.to_string())),
        ),
    };
    if let Some(dispatched_job_id) = dispatched_job_id {
        set_freshness_run_dispatched_job_with_pool(&pool, def.id, run.id, dispatched_job_id)
            .await
            .map_err(to_freshness_error)?;
    }
    finish_freshness_run_with_pool(
        &pool,
        def.id,
        run.id,
        &status,
        result_json.as_ref(),
        error_text.as_deref(),
    )
    .await
    .map_err(to_freshness_error)?;
    let runs = list_freshness_runs_with_pool(&pool, def.id, 1)
        .await
        .map_err(to_freshness_error)?;
    runs.into_iter()
        .next()
        .ok_or_else(|| "freshness run disappeared after finish".into())
}

fn spawn_heartbeat(
    pool: Arc<sqlx::SqlitePool>,
    freshness_id: Uuid,
    run_id: Uuid,
    cfg: &Config,
) -> tokio::task::JoinHandle<()> {
    let lease_ttl_ms = freshness_lease_ttl_ms(cfg);
    let every = Duration::from_secs((cfg.freshness_lease_secs / 2).max(1));
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(every);
        interval.tick().await;
        loop {
            interval.tick().await;
            let lease_until = axon_jobs::store::now_ms() + lease_ttl_ms;
            if heartbeat_freshness_run(&pool, freshness_id, run_id, lease_until)
                .await
                .ok()
                != Some(true)
            {
                break;
            }
        }
    })
}

async fn prune_retained_runs(pool: &sqlx::SqlitePool, cfg: &Config) {
    match prune_freshness_runs_for_retention(
        pool,
        cfg.freshness_run_retention_days,
        axon_jobs::store::now_ms(),
    )
    .await
    {
        Ok(count) if count > 0 => {
            tracing::info!(count, "freshness scheduler pruned old run history");
        }
        Ok(_) => {}
        Err(err) => tracing::warn!(error = %err, "freshness scheduler retention cleanup failed"),
    }
}

pub(crate) async fn dispatch_freshness(
    service_context: &ServiceContext,
    def: &FreshnessDef,
) -> Result<FreshnessDispatchOutcome, FreshnessError> {
    let payload: FreshnessRequestPayload = serde_json::from_value(def.request_json.clone())?;
    let replay: SafeReplayConfigV1 = serde_json::from_value(def.config_json.clone())?;
    let cfg = replay_config(service_context.cfg(), &replay)?;
    validate_freshness_payload_for_dispatch(&payload, &cfg)?;

    match payload {
        FreshnessRequestPayload::V1(FreshnessRequestV1::Scrape { url }) => {
            let results = scrape_batch_with_optional_embed(&cfg, &[url], None)
                .await
                .map_err(|err| -> FreshnessError { err.to_string().into() })?;
            Ok(FreshnessDispatchOutcome {
                status: FRESHNESS_RUN_STATUS_COMPLETED.to_string(),
                dispatched_job_id: None,
                result_json: serde_json::to_value(results)?,
            })
        }
        FreshnessRequestPayload::V1(FreshnessRequestV1::Crawl { urls }) => {
            let effective = crate::crawl::apply_crawl_defaults(&cfg);
            let expected_config_json = config_snapshot_json(&effective)?;
            if has_active_equivalent_crawl(service_context, &urls, &expected_config_json).await? {
                return Ok(skipped_active_job(def));
            }
            let outcome = crawl_start_for_freshness(&cfg, &urls, service_context).await?;
            Ok(FreshnessDispatchOutcome {
                status: FRESHNESS_RUN_STATUS_ENQUEUED.to_string(),
                dispatched_job_id: first_uuid(outcome.get("job_ids")),
                result_json: outcome,
            })
        }
        FreshnessRequestPayload::V1(FreshnessRequestV1::Embed { input }) => {
            let expected_config_json = config_snapshot_json(&cfg)?;
            if has_active_equivalent_single(
                service_context,
                JobKind::Embed,
                &input,
                &expected_config_json,
            )
            .await?
            {
                return Ok(skipped_active_job(def));
            }
            // The freshness scheduler is a system-triggered background loop —
            // no real caller identity is available here.
            let outcome = embed_start_with_context(&cfg, &input, service_context, None, None, None)
                .await
                .map_err(|err| -> FreshnessError { err.to_string().into() })?;
            let job_id = Uuid::parse_str(&outcome.result.job_id).ok();
            Ok(FreshnessDispatchOutcome {
                status: FRESHNESS_RUN_STATUS_ENQUEUED.to_string(),
                dispatched_job_id: job_id,
                result_json: serde_json::to_value(outcome.result)?,
            })
        }
        FreshnessRequestPayload::V1(FreshnessRequestV1::Ingest { source }) => {
            let target = target_label(&source);
            let expected_config_json = ingest_config_json(&cfg, &source)?;
            if has_active_equivalent_single(
                service_context,
                JobKind::Ingest,
                &target,
                &expected_config_json,
            )
            .await?
            {
                return Ok(skipped_active_job(def));
            }
            // The freshness scheduler is a system-triggered background loop —
            // no real caller identity is available here.
            let outcome = ingest_start_with_context(&cfg, source, service_context, None)
                .await
                .map_err(|err| -> FreshnessError { err.to_string().into() })?;
            let job_id = Uuid::parse_str(&outcome.result.job_id).ok();
            Ok(FreshnessDispatchOutcome {
                status: FRESHNESS_RUN_STATUS_ENQUEUED.to_string(),
                dispatched_job_id: job_id,
                result_json: serde_json::to_value(outcome.result)?,
            })
        }
    }
}

async fn crawl_start_for_freshness(
    cfg: &Config,
    urls: &[String],
    service_context: &ServiceContext,
) -> Result<Value, FreshnessError> {
    // The freshness scheduler is a system-triggered background loop — no real
    // caller identity is available here.
    let outcome = crate::crawl::crawl_start_with_context(cfg, urls, service_context, None, None)
        .await
        .map_err(|err| -> FreshnessError { err.to_string().into() })?;
    Ok(serde_json::to_value(outcome.result)?)
}

async fn has_active_equivalent_single(
    service_context: &ServiceContext,
    kind: JobKind,
    target: &str,
    expected_config_json: &str,
) -> Result<bool, FreshnessError> {
    let jobs = service_context.jobs.list_jobs(kind, 1000, 0).await?;
    Ok(jobs.iter().any(|job| {
        is_active_status(&job.status)
            && (job.target.as_deref() == Some(target) || job.url.as_deref() == Some(target))
            && job_config_matches(job.config_json.as_ref(), expected_config_json)
    }))
}

async fn has_active_equivalent_crawl(
    service_context: &ServiceContext,
    urls: &[String],
    expected_config_json: &str,
) -> Result<bool, FreshnessError> {
    let jobs = service_context
        .jobs
        .list_jobs(JobKind::Crawl, 1000, 0)
        .await?;
    Ok(urls.iter().all(|url| {
        jobs.iter().any(|job| {
            is_active_status(&job.status)
                && job.url.as_deref() == Some(url.as_str())
                && job_config_matches(job.config_json.as_ref(), expected_config_json)
        })
    }))
}

fn job_config_matches(config_json: Option<&Value>, expected_config_json: &str) -> bool {
    let Some(config_json) = config_json else {
        return true;
    };
    let Ok(expected) = serde_json::from_str::<Value>(expected_config_json) else {
        return false;
    };
    *config_json == expected
}

fn is_active_status(status: &str) -> bool {
    matches!(status, "pending" | "running")
}

fn skipped_active_job(def: &FreshnessDef) -> FreshnessDispatchOutcome {
    FreshnessDispatchOutcome {
        status: FRESHNESS_RUN_STATUS_SKIPPED_ACTIVE_JOB.to_string(),
        dispatched_job_id: None,
        result_json: serde_json::json!({
            "status": FRESHNESS_RUN_STATUS_SKIPPED_ACTIVE_JOB,
            "command": def.command,
            "target": def.target,
        }),
    }
}

fn first_uuid(value: Option<&Value>) -> Option<Uuid> {
    value
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
}

#[cfg(test)]
pub(crate) fn lease_limit_for_available_capacity(max_due: i64, available_permits: usize) -> i64 {
    available_permits.min(max_due.clamp(1, 100) as usize) as i64
}
