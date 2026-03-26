use super::{RefreshSchedule, RefreshScheduleCreate, refresh_start, resolve_refresh_schedule_urls};
use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use crate::crates::core::logging::{log_debug, log_info, log_warn};
use crate::crates::jobs::refresh::{
    claim_due_refresh_schedules_with_pool, create_refresh_schedule, delete_refresh_schedule,
    ensure_schema_once, list_refresh_schedules, mark_refresh_schedule_ran_with_pool,
    set_refresh_schedule_enabled, should_reingest_github as jobs_should_reingest_github,
};
use crate::crates::services::ingest::{self as ingest_service, IngestSource};
use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use std::error::Error;
use tokio::time::{Duration as TokioDuration, Instant};
use uuid::Uuid;

fn require_refresh_schedule_support(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if cfg.lite_mode {
        return Err("refresh schedule is not available in lite mode".into());
    }
    Ok(())
}

pub async fn refresh_schedule_list(
    cfg: &Config,
    limit: i64,
) -> Result<Vec<RefreshSchedule>, Box<dyn Error>> {
    require_refresh_schedule_support(cfg)?;
    list_refresh_schedules(cfg, limit).await
}

pub async fn refresh_schedule_create(
    cfg: &Config,
    schedule: &RefreshScheduleCreate,
) -> Result<RefreshSchedule, Box<dyn Error>> {
    require_refresh_schedule_support(cfg)?;
    create_refresh_schedule(cfg, schedule).await
}

pub async fn refresh_schedule_delete(cfg: &Config, name: &str) -> Result<bool, Box<dyn Error>> {
    require_refresh_schedule_support(cfg)?;
    delete_refresh_schedule(cfg, name).await
}

pub async fn refresh_schedule_enable(cfg: &Config, name: &str) -> Result<bool, Box<dyn Error>> {
    require_refresh_schedule_support(cfg)?;
    set_refresh_schedule_enabled(cfg, name, true).await
}

pub async fn refresh_schedule_disable(cfg: &Config, name: &str) -> Result<bool, Box<dyn Error>> {
    require_refresh_schedule_support(cfg)?;
    set_refresh_schedule_enabled(cfg, name, false).await
}

fn validate_github_repo(repo: &str) -> Result<&str, Box<dyn Error>> {
    let trimmed = repo.trim();
    let mut parts = trimmed.split('/');
    let Some(owner) = parts.next() else {
        return Err(anyhow::anyhow!("Invalid GitHub target. Expected owner/repo").into());
    };
    let Some(name) = parts.next() else {
        return Err(anyhow::anyhow!("Invalid GitHub target. Expected owner/repo").into());
    };
    if parts.next().is_some() || owner.is_empty() || name.is_empty() {
        return Err(anyhow::anyhow!("Invalid GitHub target. Expected owner/repo").into());
    }
    let valid_segment = |segment: &str| {
        segment
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
            && !segment.starts_with('.')
            && !segment.ends_with('.')
    };
    if !valid_segment(owner) || !valid_segment(name) {
        return Err(anyhow::anyhow!("Invalid GitHub target. Expected owner/repo").into());
    }
    Ok(trimmed)
}

async fn check_github_pushed_at(cfg: &Config, repo: &str) -> Result<String, Box<dyn Error>> {
    let repo = validate_github_repo(repo)?;
    let url = format!("https://api.github.com/repos/{repo}");
    let client = http_client()?;
    let mut req = client.get(&url).header("User-Agent", "axon-refresh");
    if let Some(token) = cfg.github_token.as_deref()
        && !token.is_empty()
    {
        req = req.header("Authorization", format!("Bearer {token}"));
    }
    let resp: serde_json::Value = req.send().await?.error_for_status()?.json().await?;
    resp["pushed_at"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| anyhow::anyhow!("missing pushed_at in GitHub API response").into())
}

async fn dispatch_github_refresh(
    cfg: &Config,
    pool: &PgPool,
    schedule: &RefreshSchedule,
    target: &str,
) -> Result<Option<Uuid>, Box<dyn Error>> {
    let target = validate_github_repo(target)?;
    let next_run_at = Utc::now() + Duration::seconds(schedule.every_seconds);

    match check_github_pushed_at(cfg, target).await {
        Ok(pushed_at) => {
            if refresh_should_reingest_github(&pushed_at, schedule.last_run_at) {
                match ingest_service::ingest_start(
                    cfg,
                    IngestSource::Github {
                        repo: target.to_string(),
                        include_source: true,
                    },
                )
                .await
                {
                    Ok(started) => {
                        let job_id =
                            Uuid::parse_str(&started.job_id).unwrap_or_else(|_| Uuid::nil());
                        log_info(&format!(
                            "refresh github_ingest_queued repo={target} job_id={job_id}"
                        ));
                        if let Err(err) =
                            refresh_mark_schedule_ran(pool, schedule.id, next_run_at).await
                        {
                            log_warn(&format!(
                                "refresh github mark_ran failed schedule={} id={}: {err}",
                                schedule.name, schedule.id
                            ));
                        }
                        return Ok(Some(job_id));
                    }
                    Err(err) => {
                        log_warn(&format!(
                            "refresh github ingest enqueue failed schedule={} repo={target} error={err}",
                            schedule.name
                        ));
                        return Err(err);
                    }
                }
            }
            log_debug(&format!(
                "refresh github_skip_no_push repo={target} schedule={}",
                schedule.name
            ));
            if let Err(err) = refresh_mark_schedule_ran(pool, schedule.id, next_run_at).await {
                log_warn(&format!(
                    "refresh github mark_ran failed schedule={} id={}: {err}",
                    schedule.name, schedule.id
                ));
            }
            Ok(None)
        }
        Err(err) => {
            log_warn(&format!(
                "refresh github pushed_at check failed schedule={} repo={target} error={err}",
                schedule.name
            ));
            Err(err)
        }
    }
}

pub async fn refresh_ensure_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    ensure_schema_once(pool).await
}

pub async fn refresh_claim_due_schedules(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<RefreshSchedule>, Box<dyn Error>> {
    claim_due_refresh_schedules_with_pool(pool, limit).await
}

pub async fn refresh_mark_schedule_ran(
    pool: &PgPool,
    id: Uuid,
    next_run_at: DateTime<Utc>,
) -> Result<bool, Box<dyn Error>> {
    mark_refresh_schedule_ran_with_pool(pool, id, next_run_at).await
}

pub fn refresh_should_reingest_github(pushed_at: &str, last_run_at: Option<DateTime<Utc>>) -> bool {
    jobs_should_reingest_github(pushed_at, last_run_at)
}

pub struct RefreshScheduleDueSweep {
    pub claimed_count: usize,
    pub dispatched_count: usize,
    pub skipped_count: usize,
    pub failed_count: usize,
    pub jobs: Vec<serde_json::Value>,
}

struct SweepCounters {
    dispatched: usize,
    skipped: usize,
    failed: usize,
    jobs: Vec<serde_json::Value>,
}

async fn mark_schedule_ran_or_log(pool: &PgPool, schedule: &RefreshSchedule, now: DateTime<Utc>) {
    let next_run_at = now + Duration::seconds(schedule.every_seconds);
    if let Err(err) = refresh_mark_schedule_ran(pool, schedule.id, next_run_at).await {
        log_warn(&format!(
            "refresh mark_schedule_ran failed schedule={} id={}: {err}",
            schedule.name, schedule.id
        ));
    }
}

async fn process_github_due_schedule(
    cfg: &Config,
    pool: &PgPool,
    schedule: &RefreshSchedule,
    target: &str,
    now: DateTime<Utc>,
    counters: &mut SweepCounters,
) -> Result<(), Box<dyn Error>> {
    match dispatch_github_refresh(cfg, pool, schedule, target).await {
        Ok(Some(job_id)) => {
            counters.dispatched += 1;
            let next_run_at = now + Duration::seconds(schedule.every_seconds);
            counters.jobs.push(serde_json::json!({
                "schedule_id": schedule.id,
                "name": schedule.name,
                "job_id": job_id,
                "source_type": "github",
                "target": target,
                "next_run_at": next_run_at,
            }));
        }
        Ok(None) => counters.skipped += 1,
        Err(err) => {
            log_warn(&format!(
                "refresh github dispatch failed schedule={} target={target}: {err}",
                schedule.name
            ));
            counters.failed += 1;
        }
    }
    Ok(())
}

async fn process_url_due_schedule(
    cfg: &Config,
    pool: &PgPool,
    schedule: &RefreshSchedule,
    now: DateTime<Utc>,
    counters: &mut SweepCounters,
) -> Result<(), Box<dyn Error>> {
    let urls = resolve_refresh_schedule_urls(cfg, schedule).await?;
    if urls.is_empty() {
        mark_schedule_ran_or_log(pool, schedule, now).await;
        counters.skipped += 1;
        return Ok(());
    }
    match refresh_start(cfg, &urls).await {
        Ok(started) => {
            mark_schedule_ran_or_log(pool, schedule, now).await;
            for url in &urls {
                log_info(&format!("refresh url_queued url={url}"));
            }
            counters.dispatched += 1;
            let job_id = Uuid::parse_str(&started.job_id).unwrap_or_else(|_| Uuid::nil());
            counters.jobs.push(serde_json::json!({
                "schedule_id": schedule.id,
                "name": schedule.name,
                "job_id": job_id,
                "target_count": urls.len(),
                "next_run_at": now + Duration::seconds(schedule.every_seconds),
            }));
        }
        Err(err) => {
            log_warn(&format!(
                "refresh schedule worker failed to dispatch schedule={} error={err}",
                schedule.name
            ));
            counters.failed += 1;
        }
    }
    Ok(())
}

async fn process_due_schedule(
    cfg: &Config,
    pool: &PgPool,
    schedule: &RefreshSchedule,
    now: DateTime<Utc>,
    counters: &mut SweepCounters,
) -> Result<(), Box<dyn Error>> {
    if schedule.source_type.as_deref() == Some("github")
        && let Some(target) = &schedule.target
    {
        return process_github_due_schedule(cfg, pool, schedule, target, now, counters).await;
    }
    process_url_due_schedule(cfg, pool, schedule, now, counters).await
}

pub async fn refresh_schedule_run_due(
    cfg: &Config,
    batch: usize,
) -> Result<RefreshScheduleDueSweep, Box<dyn Error>> {
    require_refresh_schedule_support(cfg)?;
    let pool = crate::crates::jobs::common::make_pool(cfg).await?;
    refresh_schedule_run_due_with_pool(cfg, &pool, batch).await
}

pub async fn refresh_schedule_run_due_with_pool(
    cfg: &Config,
    pool: &PgPool,
    batch: usize,
) -> Result<RefreshScheduleDueSweep, Box<dyn Error>> {
    require_refresh_schedule_support(cfg)?;
    refresh_ensure_schema(pool).await?;
    let claimed = refresh_claim_due_schedules(pool, batch as i64).await?;
    let now = Utc::now();
    let mut counters = SweepCounters {
        dispatched: 0,
        skipped: 0,
        failed: 0,
        jobs: Vec::new(),
    };

    if claimed.is_empty() {
        log_debug("refresh poll_idle");
    } else {
        log_info(&format!(
            "refresh schedules_claimed count={}",
            claimed.len()
        ));
    }

    for schedule in &claimed {
        process_due_schedule(cfg, pool, schedule, now, &mut counters).await?;
    }

    Ok(RefreshScheduleDueSweep {
        claimed_count: claimed.len(),
        dispatched_count: counters.dispatched,
        skipped_count: counters.skipped,
        failed_count: counters.failed,
        jobs: counters.jobs,
    })
}

const REFRESH_SCHEDULE_WORKER_DEFAULT_TICK_SECS: u64 = 30;
const REFRESH_SCHEDULE_WORKER_TICK_ENV: &str = "AXON_REFRESH_SCHEDULER_TICK_SECS";

pub fn refresh_schedule_tick_secs_default() -> u64 {
    REFRESH_SCHEDULE_WORKER_DEFAULT_TICK_SECS
}

fn refresh_schedule_tick_secs() -> u64 {
    std::env::var(REFRESH_SCHEDULE_WORKER_TICK_ENV)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .unwrap_or_else(refresh_schedule_tick_secs_default)
}

pub async fn refresh_schedule_worker(cfg: &Config) -> Result<(), Box<dyn Error>> {
    require_refresh_schedule_support(cfg)?;
    let tick_secs = refresh_schedule_tick_secs();
    let tick_duration = TokioDuration::from_secs(tick_secs);
    log_info(&format!(
        "refresh schedule worker started tick_secs={tick_secs} (env={REFRESH_SCHEDULE_WORKER_TICK_ENV})"
    ));

    let pool = crate::crates::jobs::common::make_pool(cfg).await?;

    loop {
        let sweep_start = Instant::now();
        log_info("refresh schedule worker running due sweep");
        match refresh_schedule_run_due_with_pool(cfg, &pool, 25).await {
            Ok(sweep) => {
                log_info(&format!(
                    "refresh schedule worker sweep complete claimed={} dispatched={} skipped={} failed={}",
                    sweep.claimed_count,
                    sweep.dispatched_count,
                    sweep.skipped_count,
                    sweep.failed_count
                ));
            }
            Err(err) => {
                log_warn(&format!("refresh schedule worker sweep failed: {err}"));
            }
        }

        let remaining = tick_duration.saturating_sub(sweep_start.elapsed());
        tokio::time::sleep(remaining).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn validate_github_repo_accepts_owner_repo_slug() {
        assert_eq!(
            validate_github_repo("owner/repo").expect("valid repo"),
            "owner/repo"
        );
    }

    #[test]
    fn validate_github_repo_rejects_non_slug_inputs() {
        assert!(validate_github_repo("https://github.com/owner/repo").is_err());
        assert!(validate_github_repo("owner").is_err());
        assert!(validate_github_repo("owner/repo/extra").is_err());
        assert!(validate_github_repo("../repo").is_err());
    }

    #[test]
    fn refresh_should_reingest_github_uses_jobs_predicate() {
        let pushed_at = "2026-03-25T12:00:00Z";
        let last_run_at = Some(
            Utc.with_ymd_and_hms(2026, 3, 24, 12, 0, 0)
                .single()
                .unwrap(),
        );
        assert!(refresh_should_reingest_github(pushed_at, last_run_at));
    }
}
