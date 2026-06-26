use super::{
    FreshnessDispatchOutcome, FreshnessError, FreshnessRequestPayload, FreshnessRequestV1,
    SafeReplayConfigV1, freshness_lease_ttl_ms, replay_config, to_freshness_error,
    validate_freshness_payload_for_dispatch,
};
use crate::context::ServiceContext;
use crate::embed::embed_start_with_context;
use crate::ingest::ingest_start_with_context;
use crate::scrape::scrape_batch_with_optional_embed;
use axon_core::config::Config;
use axon_core::redact::redact_secrets;
use axon_jobs::backend::JobKind;
use axon_jobs::freshness::{
    FRESHNESS_RUN_STATUS_COMPLETED, FRESHNESS_RUN_STATUS_ENQUEUED, FRESHNESS_RUN_STATUS_FAILED,
    FRESHNESS_RUN_STATUS_SKIPPED_ACTIVE_JOB, FreshnessDef, FreshnessRun,
    create_freshness_run_with_pool, finish_freshness_run_with_pool, heartbeat_freshness_run,
    lease_due_freshness, lease_freshness_for_manual_run, list_freshness_runs_with_pool,
    reclaim_current_stale_freshness_leases,
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
    let max_due = service_context.cfg.freshness_max_due_per_tick.clamp(1, 4);
    let max_concurrent = service_context.cfg.freshness_max_concurrent_runs.max(1);
    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    tokio::spawn(async move {
        match reclaim_current_stale_freshness_leases(&pool).await {
            Ok(count) if count > 0 => {
                tracing::info!(count, "freshness scheduler reclaimed stale leases");
            }
            Ok(_) => {}
            Err(err) => tracing::warn!(error = %err, "freshness scheduler lease reclaim failed"),
        }
        let mut interval = tokio::time::interval(Duration::from_secs(tick_secs));
        interval.tick().await;
        loop {
            interval.tick().await;
            let due =
                match lease_due_freshness(&pool, axon_jobs::store::now_ms(), lease_ttl_ms, max_due)
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

    let (status, result_json, error_text) = match outcome {
        Ok(outcome) => (outcome.status, Some(outcome.result_json), None),
        Err(err) => (
            FRESHNESS_RUN_STATUS_FAILED.to_string(),
            None,
            Some(redact_secrets(&err.to_string())),
        ),
    };
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
            if has_active_equivalent_job(service_context, JobKind::Crawl, def).await? {
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
            if has_active_equivalent_job(service_context, JobKind::Embed, def).await? {
                return Ok(skipped_active_job(def));
            }
            let outcome = embed_start_with_context(&cfg, &input, service_context, None, None)
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
            if has_active_equivalent_job(service_context, JobKind::Ingest, def).await? {
                return Ok(skipped_active_job(def));
            }
            let outcome = ingest_start_with_context(&cfg, source, service_context)
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
    let outcome = crate::crawl::crawl_start_with_context(cfg, urls, service_context, None)
        .await
        .map_err(|err| -> FreshnessError { err.to_string().into() })?;
    Ok(serde_json::to_value(outcome.result)?)
}

async fn has_active_equivalent_job(
    service_context: &ServiceContext,
    kind: JobKind,
    def: &FreshnessDef,
) -> Result<bool, FreshnessError> {
    let active = service_context
        .jobs
        .has_active_jobs(kind)
        .await
        .map_err(|err| -> FreshnessError { err.to_string().into() })?;
    if !active {
        return Ok(false);
    }
    let jobs = service_context
        .jobs
        .list_jobs(kind, 1000, 0)
        .await
        .unwrap_or_default();
    if jobs.is_empty() {
        return Ok(true);
    }
    Ok(jobs.iter().any(|job| {
        is_active_status(&job.status)
            && (job.target.as_deref() == Some(def.target.as_str())
                || job.url.as_deref() == Some(def.target.as_str())
                || job
                    .urls_json
                    .as_ref()
                    .is_some_and(|value| value.to_string().contains(&def.target)))
    }))
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
pub(crate) async fn run_fake_freshness_scheduler_with_limits(total: usize, limit: usize) -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let semaphore = Arc::new(Semaphore::new(limit));
    let active = Arc::new(AtomicUsize::new(0));
    let max_seen = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();
    for _ in 0..total {
        let permit = Arc::clone(&semaphore)
            .acquire_owned()
            .await
            .expect("permit");
        let active = Arc::clone(&active);
        let max_seen = Arc::clone(&max_seen);
        handles.push(tokio::spawn(async move {
            let _permit = permit;
            let now = active.fetch_add(1, Ordering::SeqCst) + 1;
            max_seen.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(5)).await;
            active.fetch_sub(1, Ordering::SeqCst);
        }));
    }
    for handle in handles {
        handle.await.expect("fake dispatch");
    }
    max_seen.load(Ordering::SeqCst)
}
