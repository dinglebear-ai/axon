mod github;
mod schedule;

use crate::crates::cli::commands::common::{
    handle_job_cancel, handle_job_cleanup, handle_job_clear, handle_job_errors, handle_job_list,
    handle_job_recover, handle_job_status, handle_worker_mode,
};
use crate::crates::cli::commands::common_urls::{parse_urls, start_url_from_cfg};
use crate::crates::core::config::Config;
use crate::crates::core::ui::confirm_destructive;
use crate::crates::core::ui::{accent, muted, primary, symbol_for_status};
use crate::crates::jobs::backend::{JobKind, JobPayload};
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

    let seed_url = start_url_from_cfg(cfg);
    let input_urls = parse_urls(cfg);
    let urls = refresh_service::resolve_refresh_urls(cfg, &seed_url, &input_urls).await?;
    if urls.is_empty() {
        return Err(anyhow::anyhow!(
            "refresh requires at least one URL or a crawl manifest seed URL"
        )
        .into());
    }

    if cfg.lite_mode {
        let job_ids = enqueue_lite_refresh_jobs(service_context, &urls).await?;
        if cfg.wait {
            wait_for_lite_refresh_jobs(service_context, &job_ids).await?;
        }
        print_refresh_job_start(cfg, &urls, &job_ids)?;
        return Ok(());
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

    let job_ids = vec![refresh_service::refresh_start(cfg, &urls).await?.job_id];
    print_refresh_job_start(cfg, &urls, &job_ids)
}

async fn enqueue_lite_refresh_jobs(
    service_context: &ServiceContext,
    urls: &[String],
) -> Result<Vec<String>, Box<dyn Error>> {
    let mut ids = Vec::with_capacity(urls.len());
    for url in urls {
        let id = service_context
            .jobs
            .enqueue(JobPayload::Refresh {
                url: url.clone(),
                config_json: "{}".to_string(),
            })
            .await
            .map_err(|e| -> Box<dyn Error> { e })?;
        ids.push(id.to_string());
    }
    Ok(ids)
}

async fn wait_for_lite_refresh_jobs(
    service_context: &ServiceContext,
    job_ids: &[String],
) -> Result<(), Box<dyn Error>> {
    for job_id in job_ids {
        let id = Uuid::parse_str(job_id)?;
        let final_status = service_context
            .jobs
            .wait_for_job(id, JobKind::Refresh)
            .await
            .map_err(|e| -> Box<dyn Error> { e })?;
        match final_status.as_str() {
            "completed" => {}
            "failed" => {
                if let Ok(Some(err)) = service_context.jobs.job_errors(id, JobKind::Refresh).await {
                    return Err(format!("refresh job {id} failed: {err}").into());
                }
                return Err(format!("refresh job {id} failed").into());
            }
            "canceled" => {
                if let Ok(Some(err)) = service_context.jobs.job_errors(id, JobKind::Refresh).await {
                    return Err(format!("refresh job {id} canceled: {err}").into());
                }
                return Err(format!("refresh job {id} canceled").into());
            }
            other => {
                return Err(format!("refresh job {id} ended in unexpected state {other}").into());
            }
        }
    }
    Ok(())
}

fn print_refresh_job_start(
    cfg: &Config,
    urls: &[String],
    job_ids: &[String],
) -> Result<(), Box<dyn Error>> {
    let job_id = job_ids
        .first()
        .cloned()
        .ok_or("refresh did not enqueue any jobs")?;
    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "job_id": job_id,
                "job_ids": job_ids,
                "status": "pending",
                "urls": urls,
            }))?
        );
    } else {
        println!(
            "  {} {}",
            primary("Refresh Job"),
            accent(&job_id.to_string())
        );
        println!("  {} {}", muted("Targets:"), urls.len());
        if job_ids.len() > 1 {
            println!("  {} {}", muted("Jobs:"), job_ids.len());
        }
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
    let total = jobs.len() as i64;
    let result = crate::crates::services::types::JobListResult::new(jobs, total, 50, 0);
    handle_job_list(cfg, &result, "Refresh")
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
    use super::run_refresh;
    use super::schedule::tier_to_seconds;
    use crate::crates::core::config::CommandKind;
    use crate::crates::jobs::backend::{BackendResult, JobKind, JobPayload};
    use crate::crates::jobs::common::{make_pool, test_config};
    use crate::crates::services::context::ServiceContext;
    use crate::crates::services::refresh::{
        RefreshScheduleCreate, create_refresh_schedule, delete_refresh_schedule,
        schedule_list_jobs as list_refresh_jobs,
    };
    use crate::crates::services::refresh::{
        refresh_schedule_run_due, refresh_schedule_tick_secs_default,
    };
    use crate::crates::services::runtime::{ServiceJobRuntime, WorkerMode};
    use crate::crates::services::types::ServiceJob;
    use async_trait::async_trait;
    use chrono::{Duration, Utc};
    use std::env;
    use std::error::Error;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;
    use uuid::Uuid;

    struct CaptureRuntime {
        payloads: Mutex<Vec<JobPayload>>,
        wait_calls: Mutex<Vec<Uuid>>,
    }

    #[async_trait]
    impl ServiceJobRuntime for CaptureRuntime {
        fn mode_name(&self) -> &'static str {
            "test"
        }

        async fn enqueue(&self, payload: JobPayload) -> BackendResult<Uuid> {
            self.payloads.lock().expect("lock").push(payload);
            Ok(Uuid::new_v4())
        }

        async fn wait_for_job(&self, _id: Uuid, _kind: JobKind) -> BackendResult<String> {
            self.wait_calls.lock().expect("lock").push(_id);
            Ok("completed".to_string())
        }

        async fn job_errors(&self, _id: Uuid, _kind: JobKind) -> BackendResult<Option<String>> {
            Ok(None)
        }

        async fn has_active_jobs(&self, _kind: JobKind) -> BackendResult<bool> {
            Ok(false)
        }

        async fn list_jobs(
            &self,
            _kind: JobKind,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
            Ok(Vec::new())
        }

        async fn job_status(
            &self,
            _kind: JobKind,
            _id: Uuid,
        ) -> Result<Option<ServiceJob>, Box<dyn Error + Send + Sync>> {
            Ok(None)
        }

        async fn cancel_job(
            &self,
            _kind: JobKind,
            _id: Uuid,
        ) -> Result<bool, Box<dyn Error + Send + Sync>> {
            Ok(false)
        }

        async fn cleanup_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
            Ok(0)
        }

        async fn clear_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
            Ok(0)
        }

        async fn recover_jobs(
            &self,
            _kind: JobKind,
            _stale_threshold_ms: i64,
        ) -> Result<u64, Box<dyn Error + Send + Sync>> {
            Ok(0)
        }

        async fn run_worker(
            &self,
            _kind: JobKind,
        ) -> Result<WorkerMode, Box<dyn Error + Send + Sync>> {
            Ok(WorkerMode::InProcess)
        }
    }

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

    #[tokio::test]
    async fn cli_refresh_lite_enqueues_refresh_jobs_via_service_context()
    -> Result<(), Box<dyn Error>> {
        let mut cfg = crate::crates::core::config::Config::test_default();
        cfg.command = CommandKind::Refresh;
        cfg.lite_mode = true;
        cfg.wait = false;
        cfg.json_output = true;
        cfg.start_url = "https://example.com/a".to_string();
        cfg.positional = vec![
            "https://example.com/a".to_string(),
            "https://example.com/b".to_string(),
        ];

        let runtime = Arc::new(CaptureRuntime {
            payloads: Mutex::new(Vec::new()),
            wait_calls: Mutex::new(Vec::new()),
        });
        let service_context = ServiceContext::from_runtime(Arc::new(cfg.clone()), runtime.clone());

        run_refresh(&cfg, &service_context).await?;

        let payloads = runtime.payloads.lock().expect("lock");
        assert_eq!(payloads.len(), 2);
        assert!(matches!(
            &payloads[0],
            JobPayload::Refresh { url, .. } if url == "https://example.com/a"
        ));
        assert!(matches!(
            &payloads[1],
            JobPayload::Refresh { url, .. } if url == "https://example.com/b"
        ));
        assert!(runtime.wait_calls.lock().expect("lock").is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn cli_refresh_lite_waits_for_sqlite_refresh_jobs() -> Result<(), Box<dyn Error>> {
        let mut cfg = crate::crates::core::config::Config::test_default();
        cfg.command = CommandKind::Refresh;
        cfg.lite_mode = true;
        cfg.wait = true;
        cfg.json_output = true;
        cfg.start_url = "https://example.com/a".to_string();
        cfg.positional = vec!["https://example.com/a".to_string()];

        let runtime = Arc::new(CaptureRuntime {
            payloads: Mutex::new(Vec::new()),
            wait_calls: Mutex::new(Vec::new()),
        });
        let service_context = ServiceContext::from_runtime(Arc::new(cfg.clone()), runtime.clone());

        run_refresh(&cfg, &service_context).await?;

        assert_eq!(runtime.payloads.lock().expect("lock").len(), 1);
        assert_eq!(runtime.wait_calls.lock().expect("lock").len(), 1);
        Ok(())
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
