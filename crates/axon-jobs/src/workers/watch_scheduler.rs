//! In-process watch scheduler.
//!
//! A single periodic loop that fires recurring watches.
//!
//! The canonical source-watch pass leases enabled due `axon_source_watches`
//! rows, turns each row back into a `SourceRequest`, and enqueues a
//! `JobKind::Source` row for the unified worker. It records history in
//! `axon_source_watch_runs` and uses live source jobs as an in-flight guard.
//!
//! Tuning (read once at spawn, mirroring `AXON_JOB_WAIT_TIMEOUT_SECS` in
//! `backend.rs`):
//! - `AXON_WATCH_TICK_SECS` — seconds between sweeps (default 15, min 1).
//! - `AXON_WATCH_LEASE_SECS` — lease TTL; must exceed a single run's wall time
//!   so a long run is never double-fired (default 300, min 1).

use crate::boundary::{JobStore, WatchStore};
use crate::store::now_ms;
use crate::unified::SqliteUnifiedJobStore;
use crate::watch_schedule::parse_watch_lease_secs;
use crate::watch_store::{LeasedSourceWatch, SqliteWatchStore};
use axon_api::source::{
    JobCreateRequest, JobDescriptor, JobIntent, JobKind, MetadataMap, SourceIntent,
    SourceRefreshPolicy, SourceRequest, SourceWatchPolicy,
};
use axon_core::config::Config;
use axon_core::config::parse::tuning;
use sqlx::SqlitePool;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

#[cfg_attr(not(test), allow(dead_code))]
const DEFAULT_TICK_SECS: u64 = 15;
#[cfg(test)]
const DEFAULT_LEASE_SECS: i64 = 300;
/// Cap watches leased per tick so one sweep can't spawn an unbounded number of
/// concurrent runs. The lease keeps any watch left over due for the next tick.
const LEASE_BATCH_LIMIT: i64 = 32;

#[cfg_attr(not(test), allow(dead_code))]
fn parse_tick_secs(raw: Option<String>) -> u64 {
    raw.and_then(|raw| raw.parse::<u64>().ok())
        .filter(|secs| *secs >= 1)
        .unwrap_or(DEFAULT_TICK_SECS)
}

#[cfg_attr(not(test), allow(dead_code))]
fn parse_lease_secs(raw: Option<String>) -> i64 {
    parse_watch_lease_secs(raw)
}

fn tick_interval() -> Duration {
    Duration::from_secs(tuning::watch_tick_secs())
}

fn lease_ttl_ms() -> i64 {
    tuning::watch_lease_secs() * 1_000
}

/// Run one sweep: lease due watches and spawn a detached run for each.
///
/// Each run is spawned rather than awaited inline so a slow scrape never stalls
/// the sweep or delays other due watches; the lease prevents the next tick from
/// re-firing a watch whose run is still in flight.
async fn sweep_due_watches(
    pool: &Arc<SqlitePool>,
    _cfg: &Arc<Config>,
    unified_notify: &Arc<Notify>,
    lease_ttl_ms: i64,
) -> Result<usize, Box<dyn Error>> {
    let now = now_ms();
    let source_store = SqliteWatchStore::new((**pool).clone());
    let source_due = source_store
        .lease_due(now, lease_ttl_ms, LEASE_BATCH_LIMIT)
        .await?;
    let count = source_due.len();
    if !source_due.is_empty() {
        let job_store = SqliteUnifiedJobStore::new((**pool).clone());
        let mut enqueued = 0usize;
        for watch in source_due {
            let watch_id = watch.watch_id.0.clone();
            match enqueue_leased_source_watch(&source_store, &job_store, watch, now).await {
                Ok(job) => {
                    enqueued += 1;
                    tracing::debug!(
                        watch_id = %watch_id,
                        job_id = %job.job_id.0,
                        "source-watch scheduler: enqueued source job"
                    );
                }
                Err(err) => {
                    if let Err(release_err) = source_store
                        .release_lease(&axon_api::source::WatchId::new(watch_id.clone()))
                        .await
                    {
                        tracing::warn!(
                            watch_id = %watch_id,
                            error = %release_err,
                            "source-watch scheduler: failed to release lease after enqueue error"
                        );
                    }
                    tracing::warn!(
                        watch_id = %watch_id,
                        error = %err,
                        "source-watch scheduler: enqueue failed"
                    );
                }
            }
        }
        if enqueued > 0 {
            unified_notify.notify_waiters();
        }
    }
    Ok(count)
}

async fn enqueue_leased_source_watch(
    source_store: &SqliteWatchStore,
    job_store: &SqliteUnifiedJobStore,
    watch: LeasedSourceWatch,
    scheduled_at_ms: i64,
) -> Result<JobDescriptor, String> {
    let source_request = source_request_for_scheduled_watch(&watch, scheduled_at_ms);
    let descriptor = JobStore::create(
        job_store,
        source_watch_job_create_request(&watch, source_request, scheduled_at_ms),
    )
    .await
    .map_err(|err| err.to_string())?;
    WatchStore::record_run(source_store, watch.watch_id, descriptor.job_id)
        .await
        .map_err(|err| err.to_string())?;
    Ok(descriptor)
}

fn source_request_for_scheduled_watch(
    watch: &LeasedSourceWatch,
    scheduled_at_ms: i64,
) -> SourceRequest {
    let mut source = SourceRequest::new(watch.request.source.clone());
    source.intent = SourceIntent::Watch;
    source.watch = SourceWatchPolicy::Enabled;
    source.refresh = SourceRefreshPolicy::IfStale;
    source.embed = watch.request.embed;
    source.options = watch.request.options.clone();
    source.scope = watch.request.scope;
    source.collection = watch.request.collection.clone();
    source.metadata.insert(
        "source_watch_id".to_string(),
        serde_json::json!(watch.watch_id.0.clone()),
    );
    source.metadata.insert(
        "source_watch_source_id".to_string(),
        serde_json::json!(watch.source_id.0.clone()),
    );
    source.metadata.insert(
        "source_watch_trigger".to_string(),
        serde_json::json!("scheduler"),
    );
    source.metadata.insert(
        "source_watch_scheduled_at_ms".to_string(),
        serde_json::json!(scheduled_at_ms),
    );
    source.idempotency_key = Some(source_watch_idempotency_key(
        &watch.watch_id.0,
        scheduled_at_ms,
    ));
    source
}

fn source_watch_job_create_request(
    watch: &LeasedSourceWatch,
    source_request: SourceRequest,
    scheduled_at_ms: i64,
) -> JobCreateRequest {
    let priority = source_request.execution.priority;
    let idempotency_key = source_request.idempotency_key.clone();
    let mut metadata = MetadataMap::new();
    metadata.insert(
        "source_watch_id".to_string(),
        serde_json::json!(watch.watch_id.0.clone()),
    );
    metadata.insert(
        "source_watch_source_id".to_string(),
        serde_json::json!(watch.source_id.0.clone()),
    );
    metadata.insert(
        "source_watch_scheduled_at_ms".to_string(),
        serde_json::json!(scheduled_at_ms),
    );
    JobCreateRequest {
        request_id: None,
        job_kind: JobKind::Source,
        job_intent: JobIntent::Watch,
        source_id: None,
        // Watch ownership and cross-watch source coalescing are linked through
        // axon_source_watch_runs plus the canonical watch's source_id.
        watch_id: None,
        parent_job_id: None,
        root_job_id: None,
        attempt: 1,
        priority,
        idempotency_key,
        stage_plan: Vec::new(),
        request: Some(serde_json::json!({ "source_request": source_request })),
        auth_snapshot: watch.auth_snapshot.clone().unwrap_or_default(),
        config_snapshot_id: None,
        requirements: MetadataMap::new(),
        result_schema: Some("source_result".to_string()),
        warnings: Vec::new(),
        error: None,
        metadata,
        deadline_at: None,
    }
}

fn source_watch_idempotency_key(watch_id: &str, scheduled_at_ms: i64) -> String {
    format!("source-watch:{watch_id}:{scheduled_at_ms}")
}

/// Periodic scheduler loop. Spawned once by `spawn_workers`; exits on shutdown.
pub(super) async fn watch_scheduler_loop(
    pool: Arc<SqlitePool>,
    cfg: Arc<Config>,
    unified_notify: Arc<Notify>,
    shutdown: CancellationToken,
) {
    let lease_ttl = lease_ttl_ms();
    let mut ticker = tokio::time::interval(tick_interval());
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Skip the immediate first tick — startup lease reclaim already ran in
    // SqliteJobBackend init, and we want the first sweep one interval out.
    ticker.tick().await;
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => break,
            _ = ticker.tick() => {
                match sweep_due_watches(&pool, &cfg, &unified_notify, lease_ttl).await {
                    Ok(fired) if fired > 0 => {
                        tracing::debug!(fired, "watch scheduler: dispatched due watches");
                    }
                    Ok(_) => {}
                    Err(err) => {
                        tracing::warn!(error = %err, "watch scheduler: sweep failed");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "watch_scheduler_tests.rs"]
mod tests;
