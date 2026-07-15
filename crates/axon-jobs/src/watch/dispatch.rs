//! Enqueue change-triggered source crawls; guard against piling up.
//!
//! Crawl execution flows through detached `JobKind::Source` rows carrying
//! `SourceRequest { scope: site }`, so watch-triggered re-crawls enqueue
//! that same contract.

use crate::boundary::JobStore;
use crate::unified::SqliteUnifiedJobStore;
use axon_api::source::{
    AuthSnapshot, JobCreateRequest, JobId, JobIntent, JobKind as UnifiedJobKind, JobPriority,
    LifecycleStatus, MetadataMap, SourceIntent, SourceLimits, SourceRequest, SourceScope,
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

/// Whether a previously-dispatched source crawl is still active. Used by the
/// in-flight guard to skip re-enqueuing a crawl for a cluster whose prior job hasn't
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
            tracing::warn!(%job_id, error = %e, "watch: crawl_job_active query failed; treating as active to avoid duplicate source crawl");
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
    let mut source_request = SourceRequest::new(seed_url.to_string());
    source_request.intent = SourceIntent::Refresh;
    source_request.scope = Some(SourceScope::Site);
    source_request.embed = cfg.embed;
    source_request.collection = Some(cfg.collection.clone());
    source_request.execution.priority = JobPriority::Normal;
    source_request.limits = SourceLimits {
        max_pages: Some(cfg.max_pages as u64),
        max_depth: Some(max_depth as u32),
        ..SourceLimits::default()
    };

    let store = SqliteUnifiedJobStore::new(pool.clone());
    let descriptor = store
        .create(JobCreateRequest {
            request_id: None,
            job_kind: UnifiedJobKind::Source,
            job_intent: JobIntent::Refresh,
            source_id: None,
            watch_id: None,
            parent_job_id: None,
            root_job_id: None,
            attempt: 1,
            priority: JobPriority::Normal,
            idempotency_key: None,
            stage_plan: Vec::new(),
            request: Some(serde_json::json!({ "source_request": source_request })),
            // Watch-triggered re-crawls are system-triggered — no per-caller
            // auth identity is available at this call site (mirrors the
            // system-triggered search/research auto-crawl path).
            auth_snapshot: AuthSnapshot::trusted_system("watch-scheduler"),
            config_snapshot_id: None,
            requirements: MetadataMap::new(),
            result_schema: Some("source_result".to_string()),
            warnings: Vec::new(),
            error: None,
            metadata: MetadataMap::new(),
            deadline_at: None,
        })
        .await
        .map_err(|e| -> Box<dyn Error> { e.message.into() })?;

    Ok(descriptor.job_id.0)
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;
