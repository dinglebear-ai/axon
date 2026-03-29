use crate::crates::core::config::Config;
use crate::crates::core::logging::log_warn;
use std::error::Error;
use uuid::Uuid;

pub mod processor;
pub mod repo;
pub(crate) mod runtime;
pub mod sitemap;
pub mod watchdog;
pub mod worker;

pub use runtime::CrawlJob;

pub async fn doctor(cfg: &Config) -> Result<serde_json::Value, String> {
    repo::doctor(cfg).await.map_err(|e| e.to_string())
}

pub async fn start_crawl_job(cfg: &Config, start_url: &str) -> Result<Uuid, Box<dyn Error>> {
    let plan = processor::build_start_plan(start_url, &cfg.exclude_path_prefix)?;
    repo::start_crawl_job(cfg, &plan.start_url).await
}

/// Batch variant: inserts and enqueues N crawl jobs over a single Postgres pool
/// and a single AMQP connection. Returns `(url, job_id)` pairs in input order.
pub async fn start_crawl_jobs_batch(
    cfg: &Config,
    start_urls: &[&str],
) -> Result<Vec<(String, Uuid)>, Box<dyn Error>> {
    // Apply the same URL normalisation (exclude_path_prefix, trailing-slash) that
    // start_crawl_job would produce for each URL individually.
    // Excluded URLs are skipped with a warning; only error if ALL URLs are excluded.
    let mut normalised: Vec<String> = Vec::with_capacity(start_urls.len());
    for &url in start_urls {
        match processor::build_start_plan(url, &cfg.exclude_path_prefix) {
            Ok(plan) => normalised.push(plan.start_url),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("path excluded") {
                    log_warn(&format!("command=crawl_batch {msg}"));
                } else {
                    return Err(e);
                }
            }
        }
    }
    if normalised.is_empty() {
        return Err("all supplied URLs are excluded by configured path prefixes".into());
    }
    let refs: Vec<&str> = normalised.iter().map(|s| s.as_str()).collect();
    runtime::start_crawl_jobs_batch(cfg, &refs).await
}

pub async fn get_job(cfg: &Config, id: Uuid) -> Result<Option<CrawlJob>, Box<dyn Error>> {
    repo::get_job(cfg, id).await
}

pub async fn list_jobs(
    cfg: &Config,
    limit: i64,
    offset: i64,
) -> Result<Vec<CrawlJob>, Box<dyn Error>> {
    repo::list_jobs(cfg, limit, offset).await
}

pub async fn count_jobs(cfg: &Config) -> Result<i64, Box<dyn Error>> {
    runtime::count_jobs(cfg).await
}

pub async fn cancel_job(cfg: &Config, id: Uuid) -> Result<bool, Box<dyn Error>> {
    repo::cancel_job(cfg, id).await
}

pub async fn cleanup_jobs(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    repo::cleanup_jobs(cfg).await
}

pub async fn clear_jobs(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    repo::clear_jobs(cfg).await
}

pub async fn recover_stale_crawl_jobs(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    watchdog::recover_stale_crawl_jobs(cfg).await
}

pub async fn run_worker(cfg: &Config) -> Result<(), Box<dyn Error>> {
    worker::run_worker(cfg).await
}
