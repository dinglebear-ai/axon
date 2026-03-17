use crate::crates::cli::commands::refresh::github::dispatch_github_refresh;
use crate::crates::cli::commands::refresh::resolve::resolve_schedule_urls;
use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_debug, log_info, log_warn};
use crate::crates::core::ui::symbol_for_status;
use crate::crates::jobs::common::make_pool;
use crate::crates::services::refresh as refresh_service;
use crate::crates::services::refresh::RefreshSchedule;
use chrono::{Duration, Utc};
use std::error::Error;
use uuid::Uuid;

pub(super) struct RefreshScheduleDueSweep {
    pub(super) claimed_count: usize,
    pub(super) dispatched_count: usize,
    pub(super) skipped_count: usize,
    pub(super) failed_count: usize,
    pub(super) jobs: Vec<serde_json::Value>,
}

struct SweepCounters {
    dispatched: usize,
    skipped: usize,
    failed: usize,
    jobs: Vec<serde_json::Value>,
}

pub async fn handle_refresh_schedule_run_due(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let mut batch: usize = 25;
    let mut idx = 2usize;
    while idx < cfg.positional.len() {
        match cfg.positional[idx].as_str() {
            "--batch" => {
                let value = cfg
                    .positional
                    .get(idx + 1)
                    .ok_or("refresh schedule run-due requires value after --batch")?;
                batch = value
                    .parse::<usize>()
                    .map_err(|_| "refresh schedule run-due --batch must be an integer")?;
                if batch == 0 {
                    return Err(anyhow::anyhow!(
                        "refresh schedule run-due --batch must be greater than 0"
                    )
                    .into());
                }
                idx += 2;
            }
            token => {
                return Err(
                    anyhow::anyhow!("unknown refresh schedule run-due flag: {token}").into(),
                );
            }
        }
    }

    let sweep = run_refresh_schedule_due_sweep(cfg, batch).await?;
    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({
                "claimed": sweep.claimed_count,
                "dispatched": sweep.dispatched_count,
                "skipped": sweep.skipped_count,
                "failed": sweep.failed_count,
                "jobs": sweep.jobs,
            })
        );
    } else {
        println!(
            "{} claimed={} dispatched={} skipped={} failed={}",
            symbol_for_status("completed"),
            sweep.claimed_count,
            sweep.dispatched_count,
            sweep.skipped_count,
            sweep.failed_count
        );
    }
    Ok(())
}

pub(super) async fn run_refresh_schedule_due_sweep(
    cfg: &Config,
    batch: usize,
) -> Result<RefreshScheduleDueSweep, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    run_refresh_schedule_due_sweep_with_pool(cfg, &pool, batch).await
}

pub(super) async fn run_refresh_schedule_due_sweep_with_pool(
    cfg: &Config,
    pool: &sqlx::PgPool,
    batch: usize,
) -> Result<RefreshScheduleDueSweep, Box<dyn Error>> {
    refresh_service::refresh_ensure_schema(pool).await?;
    let claimed = refresh_service::refresh_claim_due_schedules(pool, batch as i64).await?;
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

async fn process_due_schedule(
    cfg: &Config,
    pool: &sqlx::PgPool,
    schedule: &RefreshSchedule,
    now: chrono::DateTime<Utc>,
    counters: &mut SweepCounters,
) -> Result<(), Box<dyn Error>> {
    if schedule.source_type.as_deref() == Some("github")
        && let Some(target) = &schedule.target
    {
        return process_github_due_schedule(cfg, pool, schedule, target, now, counters).await;
    }
    process_url_due_schedule(cfg, pool, schedule, now, counters).await
}

async fn process_github_due_schedule(
    cfg: &Config,
    pool: &sqlx::PgPool,
    schedule: &RefreshSchedule,
    target: &str,
    now: chrono::DateTime<Utc>,
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
    pool: &sqlx::PgPool,
    schedule: &RefreshSchedule,
    now: chrono::DateTime<Utc>,
    counters: &mut SweepCounters,
) -> Result<(), Box<dyn Error>> {
    let urls = resolve_schedule_urls(cfg, schedule).await?;
    if urls.is_empty() {
        mark_schedule_ran(pool, schedule, now, true).await;
        counters.skipped += 1;
        return Ok(());
    }
    match refresh_service::refresh_start(cfg, &urls).await {
        Ok(started) => {
            mark_schedule_ran(pool, schedule, now, false).await;
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

async fn mark_schedule_ran(
    pool: &sqlx::PgPool,
    schedule: &RefreshSchedule,
    now: chrono::DateTime<Utc>,
    skipped: bool,
) {
    let next_run_at = now + Duration::seconds(schedule.every_seconds);
    if let Err(err) =
        refresh_service::refresh_mark_schedule_ran(pool, schedule.id, next_run_at).await
    {
        let context = if skipped { "skipped " } else { "" };
        log_warn(&format!(
            "refresh schedule mark_ran failed for {context}schedule={} id={}: {err}",
            schedule.name, schedule.id
        ));
    }
}
