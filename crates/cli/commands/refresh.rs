mod github;
mod schedule;

use crate::crates::cli::commands::common::{
    handle_job_cancel, handle_job_cleanup, handle_job_clear, handle_job_errors, handle_job_list,
    handle_job_recover, handle_job_status, handle_worker_mode,
};
use crate::crates::cli::commands::common_urls::parse_urls;
use crate::crates::core::config::Config;
use crate::crates::core::ui::confirm_destructive;
use crate::crates::core::ui::{accent, muted, primary, symbol_for_status};
use crate::crates::jobs::backend::JobKind;
use crate::crates::services::context::ServiceContext;
use crate::crates::services::jobs as job_service;
use crate::crates::services::refresh as refresh_service;
use schedule::handle_refresh_schedule;
use std::error::Error;
use uuid::Uuid;

pub async fn run_refresh(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    if maybe_handle_refresh_subcommand(cfg, service_context).await? {
        return Ok(());
    }

    let seed_url = cfg.start_url.clone();
    let input_urls = parse_urls(cfg);
    let urls = refresh_service::resolve_refresh_urls(cfg, &seed_url, &input_urls).await?;
    if urls.is_empty() {
        return Err(anyhow::anyhow!(
            "refresh requires at least one URL or a crawl manifest seed URL"
        )
        .into());
    }

    if cfg.wait {
        let result = refresh_service::refresh_now(cfg, &urls).await?.payload;
        if cfg.json_output {
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            let checked = result.get("checked").and_then(|v| v.as_u64()).unwrap_or(0);
            let changed = result.get("changed").and_then(|v| v.as_u64()).unwrap_or(0);
            let unchanged = result
                .get("unchanged")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let failed = result.get("failed").and_then(|v| v.as_u64()).unwrap_or(0);
            println!(
                "{} checked={} changed={} unchanged={} failed={}",
                symbol_for_status("completed"),
                checked,
                changed,
                unchanged,
                failed
            );
            if let Some(path) = result.get("manifest_path").and_then(|v| v.as_str()) {
                println!("  {} {}", muted("Manifest:"), path);
            }
        }
        return Ok(());
    }

    let job_id = refresh_service::refresh_start(cfg, &urls).await?.job_id;
    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({
                "job_id": job_id,
                "status": "pending",
                "urls": urls,
            })
        );
    } else {
        println!(
            "  {} {}",
            primary("Refresh Job"),
            accent(&job_id.to_string())
        );
        println!("  {} {}", muted("Targets:"), urls.len());
        println!("Job ID: {job_id}");
    }
    Ok(())
}

#[cfg(test)]
#[path = "refresh/schedule_compat_tests.rs"]
mod schedule_compat_tests;

async fn maybe_handle_refresh_subcommand(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<bool, Box<dyn Error>> {
    let Some(subcmd) = cfg.positional.first().map(|s| s.as_str()) else {
        return Ok(false);
    };

    match subcmd {
        "schedule" => handle_refresh_schedule(cfg).await?,
        "status" => handle_refresh_status(cfg, service_context).await?,
        "cancel" => handle_refresh_cancel(cfg, service_context).await?,
        "errors" => handle_refresh_errors(cfg, service_context).await?,
        "list" => handle_refresh_list(cfg, service_context).await?,
        "cleanup" => handle_refresh_cleanup(cfg, service_context).await?,
        "clear" => handle_refresh_clear(cfg, service_context).await?,
        "worker" => {
            handle_worker_mode(job_service::run_worker(service_context, JobKind::Refresh).await?)?
        }
        "recover" => handle_refresh_recover(cfg, service_context).await?,
        _ => return Ok(false),
    }

    Ok(true)
}

fn parse_refresh_job_id(cfg: &Config, action: &str) -> Result<Uuid, Box<dyn Error>> {
    let id = cfg
        .positional
        .get(1)
        .ok_or_else(|| format!("refresh {action} requires <job-id>"))?;
    Ok(Uuid::parse_str(id)?)
}

async fn handle_refresh_status(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let id = parse_refresh_job_id(cfg, "status")?;
    let job = job_service::job_status(service_context, JobKind::Refresh, id).await?;
    handle_job_status(cfg, job, id, "Refresh")
}

async fn handle_refresh_cancel(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let id = parse_refresh_job_id(cfg, "cancel")?;
    let canceled = job_service::cancel_job(service_context, JobKind::Refresh, id).await?;
    handle_job_cancel(cfg, id, canceled, "refresh")
}

async fn handle_refresh_errors(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let id = parse_refresh_job_id(cfg, "errors")?;
    let job = job_service::job_status(service_context, JobKind::Refresh, id).await?;
    handle_job_errors(cfg, job, id, "refresh")
}

async fn handle_refresh_list(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let jobs = job_service::list_jobs(service_context, JobKind::Refresh, 50, 0).await?;
    handle_job_list(cfg, jobs, "Refresh")
}

async fn handle_refresh_cleanup(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let removed = job_service::cleanup_jobs(service_context, JobKind::Refresh).await?;
    handle_job_cleanup(cfg, removed, "refresh")
}

async fn handle_refresh_clear(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    if !confirm_destructive(cfg, "Clear all refresh jobs and purge refresh queue?")? {
        if cfg.json_output {
            println!("{}", serde_json::json!({ "removed": 0 }));
        } else {
            println!("{} aborted", symbol_for_status("canceled"));
        }
        return Ok(());
    }

    let removed = job_service::clear_jobs(service_context, JobKind::Refresh).await?;
    handle_job_clear(cfg, removed, "refresh")
}

async fn handle_refresh_recover(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let reclaimed = job_service::recover_jobs(service_context, JobKind::Refresh).await?;
    handle_job_recover(cfg, reclaimed, "refresh")
}

#[cfg(test)]
mod tests {
    use super::schedule::tier_to_seconds;
    use crate::crates::jobs::common::{make_pool, test_config};
    use crate::crates::services::refresh::{
        RefreshScheduleCreate, create_refresh_schedule, delete_refresh_schedule,
        schedule_list_jobs as list_refresh_jobs,
    };
    use crate::crates::services::refresh::{
        refresh_schedule_run_due, refresh_schedule_tick_secs_default,
    };
    use chrono::{Duration, Utc};
    use std::env;
    use std::error::Error;
    use tempfile::TempDir;
    use uuid::Uuid;

    #[test]
    fn refresh_tier_maps_to_expected_seconds() {
        assert_eq!(tier_to_seconds("high"), Some(1800));
        assert_eq!(tier_to_seconds("medium"), Some(21600));
        assert_eq!(tier_to_seconds("low"), Some(86400));
    }

    #[test]
    fn refresh_schedule_worker_default_tick_is_30_seconds() {
        assert_eq!(refresh_schedule_tick_secs_default(), 30);
    }

    fn pg_url() -> String {
        env::var("AXON_TEST_PG_URL")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .expect("AXON_TEST_PG_URL must be set for ignored CLI infra tests")
    }

    #[tokio::test]
    #[ignore = "requires Postgres infra; run with cargo test cli_refresh_ -- --ignored"]
    async fn cli_refresh_schedule_run_due_uses_seed_manifest_when_urls_missing()
    -> Result<(), Box<dyn Error>> {
        let pg_url = pg_url();

        let temp_dir = TempDir::new()?;
        let mut cfg = test_config(&pg_url);
        cfg.output_dir = temp_dir.path().to_path_buf();
        let pool = make_pool(&cfg).await?;

        let seed_url = "https://example.com";
        let manifest_urls = vec![
            "https://example.com/docs/a".to_string(),
            "https://example.com/docs/b".to_string(),
        ];
        let manifest_path = cfg
            .output_dir
            .join("domains")
            .join("example.com")
            .join("latest")
            .join("manifest.jsonl");
        tokio::fs::create_dir_all(
            manifest_path
                .parent()
                .ok_or("manifest path missing parent directory")?,
        )
        .await?;
        let manifest_body = manifest_urls
            .iter()
            .enumerate()
            .map(|(idx, url)| {
                serde_json::json!({
                    "url": url,
                    "relative_path": format!("markdown/{idx}.md"),
                    "markdown_chars": 100,
                    "content_hash": format!("hash-{idx}"),
                    "changed": true,
                })
                .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        tokio::fs::write(&manifest_path, manifest_body).await?;

        let schedule_name = format!("refresh-seed-fallback-{}", Uuid::new_v4());
        let _ = create_refresh_schedule(
            &cfg,
            &RefreshScheduleCreate {
                name: schedule_name.clone(),
                seed_url: Some(seed_url.to_string()),
                urls: None,
                every_seconds: 300,
                enabled: true,
                next_run_at: Utc::now() - Duration::minutes(1),
                source_type: None,
                target: None,
            },
        )
        .await?;

        refresh_schedule_run_due(&cfg, 25).await?;

        let jobs = list_refresh_jobs(&cfg, 50, 0).await?;
        let matching_job = jobs.iter().find(|job| {
            serde_json::from_value::<Vec<String>>(job.urls_json.clone())
                .map(|urls| urls == manifest_urls)
                .unwrap_or(false)
        });
        assert!(matching_job.is_some());

        if let Some(job) = matching_job {
            let _ = sqlx::query("DELETE FROM axon_refresh_jobs WHERE id = $1")
                .bind(job.id)
                .execute(&pool)
                .await?;
        }
        let _ = delete_refresh_schedule(&cfg, &schedule_name).await?;
        Ok(())
    }
}
