use crate::crates::core::config::Config;
use crate::crates::jobs::refresh::{
    cancel_refresh_job, cleanup_refresh_jobs, clear_refresh_jobs, count_refresh_jobs,
    get_refresh_job, list_refresh_jobs, list_refresh_schedules, recover_stale_refresh_jobs,
    run_refresh_once, run_refresh_worker, set_refresh_schedule_enabled, start_refresh_job,
};
use crate::crates::jobs::refresh::{
    claim_due_refresh_schedules_with_pool, ensure_schema_once, mark_refresh_schedule_ran_with_pool,
    should_reingest_github as jobs_should_reingest_github,
};

pub use crate::crates::jobs::refresh::{
    RefreshJob, RefreshSchedule, RefreshScheduleCreate, create_refresh_schedule,
    delete_refresh_schedule, list_refresh_jobs as schedule_list_jobs,
};
use crate::crates::services::types::{
    JobListResult, RefreshJobResult, RefreshRunResult, RefreshStartResult,
};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::error::Error;
use uuid::Uuid;

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
) -> Result<JobListResult<RefreshJob>, Box<dyn Error>> {
    // Run sequentially to preserve Send-ness required by the MCP #[tool] macro.
    // Box<dyn Error> is !Send, so tokio::join! would make the combined future !Send.
    let jobs = list_refresh_jobs(cfg, limit, offset).await?;
    let total = count_refresh_jobs(cfg).await.unwrap_or(jobs.len() as i64);
    Ok(JobListResult::new(jobs, total, limit, offset))
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

pub async fn refresh_schedule_list(
    cfg: &Config,
    limit: i64,
) -> Result<Vec<RefreshSchedule>, Box<dyn Error>> {
    list_refresh_schedules(cfg, limit).await
}

pub async fn refresh_schedule_create(
    cfg: &Config,
    schedule: &RefreshScheduleCreate,
) -> Result<RefreshSchedule, Box<dyn Error>> {
    create_refresh_schedule(cfg, schedule).await
}

pub async fn refresh_schedule_delete(cfg: &Config, name: &str) -> Result<bool, Box<dyn Error>> {
    delete_refresh_schedule(cfg, name).await
}

pub async fn refresh_schedule_enable(cfg: &Config, name: &str) -> Result<bool, Box<dyn Error>> {
    set_refresh_schedule_enabled(cfg, name, true).await
}

pub async fn refresh_schedule_disable(cfg: &Config, name: &str) -> Result<bool, Box<dyn Error>> {
    set_refresh_schedule_enabled(cfg, name, false).await
}

/// Initialize the refresh schema once per pool. Used by the schedule sweep worker.
pub async fn refresh_ensure_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    ensure_schema_once(pool).await
}

/// Claim due refresh schedules for processing. Returns claimed schedule records.
pub async fn refresh_claim_due_schedules(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<RefreshSchedule>, Box<dyn Error>> {
    claim_due_refresh_schedules_with_pool(pool, limit).await
}

/// Mark a refresh schedule as having run, updating `last_run_at` and `next_run_at`.
pub async fn refresh_mark_schedule_ran(
    pool: &PgPool,
    id: Uuid,
    next_run_at: DateTime<Utc>,
) -> Result<bool, Box<dyn Error>> {
    mark_refresh_schedule_ran_with_pool(pool, id, next_run_at).await
}

/// Pure function: should we re-ingest a GitHub repo given `pushed_at` vs `last_run_at`?
pub fn refresh_should_reingest_github(pushed_at: &str, last_run_at: Option<DateTime<Utc>>) -> bool {
    jobs_should_reingest_github(pushed_at, last_run_at)
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
