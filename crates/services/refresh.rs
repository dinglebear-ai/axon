#[path = "refresh_schedule.rs"]
mod refresh_schedule;

use crate::crates::core::config::Config;
use crate::crates::core::content::url_to_domain;
use crate::crates::core::http::validate_url;
use crate::crates::crawl::manifest::read_manifest_urls;
use crate::crates::jobs::refresh::{
    cancel_refresh_job, cleanup_refresh_jobs, clear_refresh_jobs, get_refresh_job,
    list_refresh_jobs, recover_stale_refresh_jobs, run_refresh_once, run_refresh_worker,
    start_refresh_job,
};
use crate::crates::services::types::{
    RefreshJobListResult, RefreshJobResult, RefreshRunResult, RefreshStartResult,
};
pub use refresh_schedule::{
    RefreshScheduleDueSweep, refresh_claim_due_schedules, refresh_ensure_schema,
    refresh_mark_schedule_ran, refresh_schedule_create, refresh_schedule_delete,
    refresh_schedule_disable, refresh_schedule_enable, refresh_schedule_list,
    refresh_schedule_run_due, refresh_schedule_run_due_with_pool,
    refresh_schedule_tick_secs_default, refresh_schedule_worker, refresh_should_reingest_github,
};
use std::collections::HashSet;
use std::error::Error;
use std::path::PathBuf;
use uuid::Uuid;

pub use crate::crates::jobs::refresh::{
    RefreshJob, RefreshSchedule, RefreshScheduleCreate, create_refresh_schedule,
    delete_refresh_schedule, list_refresh_jobs as schedule_list_jobs,
};

pub async fn refresh_now(
    cfg: &Config,
    urls: &[String],
) -> Result<RefreshRunResult, Box<dyn Error>> {
    let payload = run_refresh_once(cfg, urls).await?;
    Ok(RefreshRunResult { payload })
}

pub async fn refresh_start(
    cfg: &Config,
    urls: &[String],
) -> Result<RefreshStartResult, Box<dyn Error>> {
    let job_id = start_refresh_job(cfg, urls).await?;
    Ok(RefreshStartResult {
        job_id: job_id.to_string(),
        urls: urls.to_vec(),
    })
}

pub async fn refresh_status(
    cfg: &Config,
    job_id: Uuid,
) -> Result<RefreshJobResult, Box<dyn Error>> {
    let job = get_refresh_job(cfg, job_id).await?;
    Ok(RefreshJobResult { job })
}

pub async fn refresh_list(
    cfg: &Config,
    limit: i64,
    offset: i64,
) -> Result<RefreshJobListResult, Box<dyn Error>> {
    let jobs = list_refresh_jobs(cfg, limit, offset).await?;
    Ok(RefreshJobListResult { jobs })
}

pub async fn refresh_cancel(cfg: &Config, job_id: Uuid) -> Result<bool, Box<dyn Error>> {
    cancel_refresh_job(cfg, job_id).await
}

pub async fn refresh_cleanup(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    cleanup_refresh_jobs(cfg).await
}

pub async fn refresh_clear(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    clear_refresh_jobs(cfg).await
}

pub async fn refresh_recover(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    recover_stale_refresh_jobs(cfg).await
}

pub async fn refresh_worker(cfg: &Config) -> Result<(), Box<dyn Error>> {
    run_refresh_worker(cfg).await
}

fn manifest_candidate_paths(cfg: &Config, seed_url: &str) -> Vec<PathBuf> {
    let domain = url_to_domain(seed_url);
    let base = cfg.output_dir.join("domains").join(domain);
    vec![
        base.join("latest").join("manifest.jsonl"),
        base.join("sync").join("manifest.jsonl"),
    ]
}

pub async fn urls_from_manifest_seed(
    cfg: &Config,
    seed_url: &str,
) -> Result<Vec<String>, Box<dyn Error>> {
    for path in manifest_candidate_paths(cfg, seed_url) {
        if !path.exists() {
            continue;
        }
        let urls = read_manifest_urls(&path).await?;
        if !urls.is_empty() {
            let mut sorted: Vec<String> = urls.into_iter().collect();
            sorted.sort();
            return Ok(sorted);
        }
    }
    Ok(Vec::new())
}

fn looks_like_domain_seed(url: &str) -> bool {
    let Ok(parsed) = spider::url::Url::parse(url) else {
        return false;
    };
    parsed.path() == "/" && parsed.query().is_none() && parsed.fragment().is_none()
}

pub async fn resolve_refresh_urls(
    cfg: &Config,
    seed_url: &str,
    input_urls: &[String],
) -> Result<Vec<String>, Box<dyn Error>> {
    let mut urls = input_urls.to_vec();

    if urls.is_empty() && !seed_url.trim().is_empty() {
        let seeded = urls_from_manifest_seed(cfg, seed_url).await?;
        if !seeded.is_empty() {
            urls = seeded;
        }
    } else if urls.len() == 1 && looks_like_domain_seed(&urls[0]) {
        let seeded = urls_from_manifest_seed(cfg, &urls[0]).await?;
        if !seeded.is_empty() {
            urls = seeded;
        }
    }

    let mut deduped = Vec::new();
    let mut seen = HashSet::new();
    for url in urls {
        validate_url(&url)?;
        if seen.insert(url.clone()) {
            deduped.push(url);
        }
    }

    Ok(deduped)
}

pub async fn resolve_refresh_schedule_urls(
    cfg: &Config,
    schedule: &RefreshSchedule,
) -> Result<Vec<String>, Box<dyn Error>> {
    let mut urls = match schedule.urls_json.as_ref() {
        Some(value) => serde_json::from_value::<Vec<String>>(value.clone()).unwrap_or_default(),
        None => Vec::new(),
    };

    if urls.is_empty()
        && let Some(seed_url) = schedule.seed_url.as_deref()
    {
        urls = urls_from_manifest_seed(cfg, seed_url).await?;
    }

    let mut deduped = Vec::new();
    let mut seen = HashSet::new();
    for url in urls {
        validate_url(&url)?;
        if seen.insert(url.clone()) {
            deduped.push(url);
        }
    }

    Ok(deduped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::services::types::{RefreshJobListResult, RefreshJobResult};
    use chrono::{TimeZone, Utc};

    fn test_refresh_job() -> RefreshJob {
        RefreshJob {
            id: Uuid::parse_str("88888888-8888-8888-8888-888888888888").expect("valid uuid"),
            status: "completed".to_string(),
            created_at: Utc
                .with_ymd_and_hms(2026, 3, 15, 12, 0, 0)
                .single()
                .expect("valid timestamp"),
            updated_at: Utc
                .with_ymd_and_hms(2026, 3, 15, 12, 0, 0)
                .single()
                .expect("valid timestamp"),
            started_at: None,
            finished_at: None,
            error_text: None,
            urls_json: serde_json::json!(["https://example.com"]),
            result_json: Some(serde_json::json!({"checked": 1})),
            config_json: serde_json::json!({"embed": true}),
        }
    }

    #[test]
    fn typed_refresh_result_wrappers_hold_refresh_jobs() {
        let job = test_refresh_job();
        let status = RefreshJobResult {
            job: Some(job.clone()),
        };
        let list = RefreshJobListResult {
            jobs: vec![job.clone()],
        };

        assert_eq!(status.job.expect("job").id, job.id);
        assert_eq!(list.jobs.len(), 1);
        assert_eq!(list.jobs[0].id, job.id);
    }
}
