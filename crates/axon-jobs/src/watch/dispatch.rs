//! Enqueue change-triggered crawls; guard against piling up.

use crate::backend::{JobKind, JobPayload};
use crate::config_snapshot::config_snapshot_json;
use crate::ops::enqueue_job;
use crate::query::job_status_row;
use axon_core::config::Config;
use sqlx::SqlitePool;
use std::error::Error;
use uuid::Uuid;

/// Whether a previously-dispatched crawl is still active. Used by the in-flight
/// guard to skip re-enqueuing a crawl for a cluster whose prior crawl hasn't
/// finished.
///
/// A query error is treated as ACTIVE (returns `true`), not inactive: a
/// transient DB error must not bypass the guard and let a duplicate crawl
/// through. Only a successful query that finds a terminal or absent status
/// returns `false`.
pub async fn crawl_job_active(pool: &SqlitePool, job_id: Uuid) -> bool {
    match job_status_row(pool, JobKind::Crawl, job_id).await {
        Ok(Some(r)) => r.status.is_active(),
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
    let id = enqueue_job(
        pool,
        &JobPayload::Crawl {
            url: seed_url.to_string(),
            config_json,
        },
        &crawl_cfg,
    )
    .await?;
    Ok(id)
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;
