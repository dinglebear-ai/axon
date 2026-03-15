use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use crate::crates::core::logging::{log_debug, log_info, log_warn};
use crate::crates::core::ui::{accent, muted, symbol_for_status};
use crate::crates::jobs::refresh::{
    RefreshSchedule, RefreshScheduleCreate, create_refresh_schedule,
    mark_refresh_schedule_ran_with_pool, should_reingest_github,
};
use crate::crates::services::ingest::{self as ingest_service, IngestSource};
use chrono::{Duration, Utc};
use sqlx::PgPool;
use std::error::Error;
use uuid::Uuid;

use super::schedule::tier_to_seconds;

const REFRESH_TIER_MEDIUM_SECONDS: i64 = 21600;

fn validate_github_repo(repo: &str) -> Result<&str, Box<dyn Error>> {
    let trimmed = repo.trim();
    let mut parts = trimmed.split('/');
    let Some(owner) = parts.next() else {
        return Err("Invalid GitHub target. Expected owner/repo".into());
    };
    let Some(name) = parts.next() else {
        return Err("Invalid GitHub target. Expected owner/repo".into());
    };
    if parts.next().is_some() || owner.is_empty() || name.is_empty() {
        return Err("Invalid GitHub target. Expected owner/repo".into());
    }
    let valid_segment = |segment: &str| {
        segment
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
            && !segment.starts_with('.')
            && !segment.ends_with('.')
    };
    if !valid_segment(owner) || !valid_segment(name) {
        return Err("Invalid GitHub target. Expected owner/repo".into());
    }
    Ok(trimmed)
}

/// Create a GitHub repo re-ingest schedule.
///
/// Parses `--every-seconds` and `--tier` from remaining positional args (same as URL schedules).
/// The schedule name defaults to the repo slug with `/` replaced by `-`.
pub(crate) async fn handle_refresh_schedule_add_github(
    cfg: &Config,
    repo: &str,
    raw_name: &str,
) -> Result<(), Box<dyn Error>> {
    let repo = validate_github_repo(repo)?;
    let mut every_seconds: Option<i64> = None;
    let mut tier_seconds: Option<i64> = None;

    let mut idx = 3usize;
    while idx < cfg.positional.len() {
        match cfg.positional[idx].as_str() {
            "--every-seconds" => {
                let value = cfg
                    .positional
                    .get(idx + 1)
                    .ok_or("refresh schedule add requires value after --every-seconds")?;
                let parsed = value
                    .parse::<i64>()
                    .map_err(|_| "refresh schedule add --every-seconds must be an integer")?;
                if parsed > 0 {
                    every_seconds = Some(parsed);
                }
                idx += 2;
            }
            "--tier" => {
                let value = cfg
                    .positional
                    .get(idx + 1)
                    .ok_or("refresh schedule add requires value after --tier")?;
                tier_seconds = Some(
                    tier_to_seconds(value)
                        .ok_or("refresh schedule add --tier must be one of: high, medium, low")?,
                );
                idx += 2;
            }
            token => {
                return Err(format!("unknown flag for github refresh schedule: {token}").into());
            }
        }
    }

    let every_seconds = every_seconds
        .or(tier_seconds)
        .unwrap_or(REFRESH_TIER_MEDIUM_SECONDS);
    let schedule_name = raw_name.replace('/', "-");
    let next_run_at = Utc::now();
    let schedule = RefreshScheduleCreate {
        name: schedule_name.clone(),
        seed_url: None,
        urls: None,
        every_seconds,
        enabled: true,
        next_run_at,
        source_type: Some("github".to_string()),
        target: Some(repo.to_string()),
    };
    let created = create_refresh_schedule(cfg, &schedule).await?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&created)?);
    } else {
        println!(
            "{} created github refresh schedule {}",
            symbol_for_status("completed"),
            accent(&created.name)
        );
        println!("  {} {}", muted("Repo:"), repo);
        println!("  {} {}", muted("Every:"), created.every_seconds);
        println!("  {} {}", muted("Enabled:"), created.enabled);
    }
    Ok(())
}

/// Check the `pushed_at` timestamp from the GitHub API for a given repo.
pub(crate) async fn check_github_pushed_at(
    cfg: &Config,
    repo: &str,
) -> Result<String, Box<dyn Error>> {
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
        .ok_or_else(|| "missing pushed_at in GitHub API response".into())
}

/// Dispatch a GitHub re-ingest job for a single schedule entry.
///
/// Returns `Ok(Some(job_id))` if a job was enqueued, `Ok(None)` if skipped (no new pushes),
/// or `Err` on failure.
pub(crate) async fn dispatch_github_refresh(
    cfg: &Config,
    pool: &PgPool,
    schedule: &RefreshSchedule,
    target: &str,
) -> Result<Option<Uuid>, Box<dyn Error>> {
    let target = validate_github_repo(target)?;
    let next_run_at = Utc::now() + Duration::seconds(schedule.every_seconds);

    match check_github_pushed_at(cfg, target).await {
        Ok(pushed_at) => {
            if should_reingest_github(&pushed_at, schedule.last_run_at) {
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
                        let _ = mark_refresh_schedule_ran_with_pool(pool, schedule.id, next_run_at)
                            .await;
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
            let _ = mark_refresh_schedule_ran_with_pool(pool, schedule.id, next_run_at).await;
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

#[cfg(test)]
mod tests {
    use super::validate_github_repo;

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
}
