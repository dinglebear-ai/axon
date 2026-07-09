//! Enqueue change-triggered crawls; guard against piling up.
//!
//! Crawl jobs are claimed exclusively from the **unified** job store (the
//! legacy `axon_crawl_jobs` per-family worker lane was retired in
//! `ca7ea71d1`), so watch-triggered re-crawls must enqueue there too — not
//! into `axon_crawl_jobs`, which nothing claims anymore.

use crate::boundary::JobStore;
use crate::config_snapshot::config_snapshot_json;
use crate::unified::SqliteUnifiedJobStore;
use axon_api::source::{
    AuthSnapshot, JobCreateRequest, JobId, JobIntent, JobKind as UnifiedJobKind, JobPriority,
    JobStagePlan, LifecycleStatus, MetadataMap, PipelinePhase,
};
use axon_core::config::Config;
use sqlx::SqlitePool;
use std::error::Error;
use uuid::Uuid;

/// Statuses that mean a unified job is still in flight (not yet terminal).
fn lifecycle_status_active(status: LifecycleStatus) -> bool {
    matches!(
        status,
        LifecycleStatus::Queued
            | LifecycleStatus::Pending
            | LifecycleStatus::Running
            | LifecycleStatus::Waiting
            | LifecycleStatus::Blocked
            | LifecycleStatus::Canceling
    )
}

/// Whether a previously-dispatched crawl is still active. Used by the in-flight
/// guard to skip re-enqueuing a crawl for a cluster whose prior crawl hasn't
/// finished.
///
/// A query error is treated as ACTIVE (returns `true`), not inactive: a
/// transient DB error must not bypass the guard and let a duplicate crawl
/// through. Only a successful query that finds a terminal or absent status
/// returns `false`.
pub async fn crawl_job_active(pool: &SqlitePool, job_id: Uuid) -> bool {
    let store = SqliteUnifiedJobStore::new(pool.clone());
    match store.get(JobId(job_id)).await {
        Ok(Some(summary)) => lifecycle_status_active(summary.status),
        Ok(None) => false,
        Err(e) => {
            tracing::warn!(%job_id, error = %e, "watch: crawl_job_active query failed; treating as active to avoid duplicate crawl");
            true
        }
    }
}

pub async fn enqueue_change_crawl(
    pool: &SqlitePool,
    cfg: &Config,
    seed_url: &str,
    max_depth: usize,
) -> Result<Uuid, Box<dyn Error>> {
    // Defense-in-depth: the crawl worker / Spider path does not run the reqwest
    // SSRF resolver, so re-validate the seed here before enqueuing. Create-time
    // validation already covers watched URLs, but cluster seeds are derived
    // (common-prefix) and may not be one of the originally-validated URLs.
    axon_core::http::validate_url(seed_url)?;
    let mut crawl_cfg = cfg.clone();
    crawl_cfg.max_depth = max_depth;
    let config_json = config_snapshot_json(&crawl_cfg)?;

    let store = SqliteUnifiedJobStore::new(pool.clone());
    let descriptor = store
        .create(JobCreateRequest {
            request_id: None,
            job_kind: UnifiedJobKind::Crawl,
            job_intent: JobIntent::Run,
            source_id: None,
            watch_id: None,
            parent_job_id: None,
            root_job_id: None,
            attempt: 1,
            priority: JobPriority::Normal,
            idempotency_key: None,
            stage_plan: vec![JobStagePlan {
                phase: PipelinePhase::Fetching,
                required: true,
                provider_requirements: Vec::new(),
                estimated_items: None,
            }],
            request: Some(serde_json::json!({
                "urls": [seed_url],
                "config_json": config_json,
            })),
            // Watch-triggered re-crawls are system-triggered — no per-caller
            // auth identity is available at this call site (mirrors the
            // system-triggered search/research auto-crawl path).
            auth_snapshot: AuthSnapshot::trusted_system("watch-scheduler"),
            config_snapshot_id: None,
            requirements: MetadataMap::new(),
            result_schema: Some("crawl_result".to_string()),
            warnings: Vec::new(),
            error: None,
            metadata: MetadataMap::new(),
        })
        .await
        .map_err(|e| -> Box<dyn Error> { e.message.into() })?;

    Ok(descriptor.job_id.0)
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;
