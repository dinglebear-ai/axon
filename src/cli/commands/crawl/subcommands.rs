use super::audit;
use crate::cli::commands::common::{
    handle_job_cancel, handle_job_cleanup, handle_job_clear, handle_job_list_with_rows,
    handle_job_recover, handle_job_status, handle_worker_mode,
};
use crate::cli::commands::job_contracts::JobErrorsResponse;
use crate::cli::commands::job_progress::crawl_list_progress_summary;
use crate::core::config::Config;
use crate::core::ui::{
    accent, confirm_destructive, muted, primary, status_text, symbol_for_status,
};
use crate::jobs::backend::JobKind;
use crate::jobs::store::RECLAIMED_ERROR_TEXT;
use crate::services::context::ServiceContext;
use crate::services::jobs as job_service;
use crate::services::types::ServiceJob;
use std::error::Error;
use uuid::Uuid;

pub(super) async fn maybe_handle_subcommand(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<bool, Box<dyn Error>> {
    let Some(subcmd) = cfg.positional.first().map(|s| s.as_str()) else {
        return Ok(false);
    };
    match subcmd {
        "status" => {
            let id = parse_required_job_id(cfg, "status")?;
            let job = job_service::job_status(service_context, JobKind::Crawl, id).await?;
            render_status_subcommand(cfg, job, id)?;
        }
        "cancel" => {
            let id = parse_required_job_id(cfg, "cancel")?;
            let canceled = job_service::cancel_job(service_context, JobKind::Crawl, id).await?;
            handle_job_cancel(cfg, id, canceled, "crawl")?;
        }
        "errors" => {
            let id = parse_required_job_id(cfg, "errors")?;
            let job = job_service::job_status(service_context, JobKind::Crawl, id).await?;
            render_errors_subcommand(cfg, job, id)?;
        }
        "list" => handle_list_subcommand(cfg, service_context).await?,
        "cleanup" => {
            let removed = job_service::cleanup_jobs(service_context, JobKind::Crawl).await?;
            handle_job_cleanup(cfg, removed, "crawl")?;
        }
        "clear" => {
            if confirm_destructive(cfg, "Clear all crawl jobs and purge crawl queue?")? {
                let removed = job_service::clear_jobs(service_context, JobKind::Crawl).await?;
                handle_job_clear(cfg, removed, "crawl")?;
            } else if cfg.json_output {
                println!("{}", serde_json::json!({ "removed": 0 }));
            } else {
                println!("{} aborted", symbol_for_status("canceled"));
            }
        }
        "worker" => {
            handle_worker_mode(job_service::start_worker(service_context, JobKind::Crawl).await?)?
        }
        "recover" => {
            let reclaimed = job_service::recover_jobs(service_context, JobKind::Crawl).await?;
            handle_job_recover(cfg, reclaimed, "crawl")?;
        }
        "audit" => {
            let url = cfg.positional.get(1).map(|s| s.as_str()).unwrap_or("");
            if url.is_empty() {
                return Err(anyhow::anyhow!("crawl audit requires a URL argument").into());
            }
            audit::run_crawl_audit(cfg, url).await?;
        }
        "diff" => audit::run_crawl_audit_diff(cfg).await?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn parse_required_job_id(cfg: &Config, action: &str) -> Result<Uuid, Box<dyn Error>> {
    let id = cfg
        .positional
        .get(1)
        .ok_or_else(|| format!("crawl {action} requires <job-id>"))?;
    Ok(Uuid::parse_str(id)?)
}

fn crawl_error_metrics(metrics: Option<&serde_json::Value>) -> (u64, u64, Option<&str>) {
    let Some(metrics) = metrics else {
        return (0, 0, None);
    };
    let error_pages = metrics
        .get("error_pages")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let waf_blocked_pages = metrics
        .get("waf_blocked_pages")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let sitemap_backfill_error = metrics
        .get("sitemap_backfill_error")
        .and_then(|v| v.as_str());
    (error_pages, waf_blocked_pages, sitemap_backfill_error)
}

fn is_reclaimed_retry(job: &ServiceJob) -> bool {
    job.error_text
        .as_deref()
        .map(str::trim_start)
        .is_some_and(|text| text == RECLAIMED_ERROR_TEXT)
}

fn elapsed_display(job: &ServiceJob) -> Option<String> {
    let started = job.started_at?;
    let end = job.finished_at.unwrap_or_else(chrono::Utc::now);
    let ms = (end - started).num_milliseconds().max(0) as u64;
    Some(if ms >= 60_000 {
        format!("{:.1}m", ms as f64 / 60_000.0)
    } else if ms >= 1_000 {
        format!("{:.1}s", ms as f64 / 1_000.0)
    } else {
        format!("{ms}ms")
    })
}

fn render_errors_subcommand(
    cfg: &Config,
    job: Option<ServiceJob>,
    id: Uuid,
) -> Result<(), Box<dyn Error>> {
    let Some(job) = job else {
        if cfg.json_output {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "error": format!("job not found: {id}"),
                    "job_id": id
                }))?
            );
        } else {
            println!(
                "{} {}",
                symbol_for_status("error"),
                muted(&format!("job not found: {id}"))
            );
        }
        return Ok(());
    };

    let (error_pages, waf_blocked_pages, sitemap_backfill_error) =
        crawl_error_metrics(job.result_json.as_ref());
    if cfg.json_output {
        let mut out = serde_json::to_value(JobErrorsResponse::from_job(
            job.id,
            job.status.clone(),
            job.error_text.clone(),
        ))?;
        if let Some(object) = out.as_object_mut() {
            object.insert("job_id".to_string(), serde_json::json!(job.id));
            object.insert("url".to_string(), serde_json::json!(job.url));
            object.insert("error_text".to_string(), serde_json::json!(job.error_text));
            object.insert(
                "metrics".to_string(),
                serde_json::json!({
                    "error_pages": error_pages,
                    "waf_blocked_pages": waf_blocked_pages,
                    "sitemap_backfill_error": sitemap_backfill_error,
                    "diagnostic_counts": job.result_json.as_ref().and_then(|m| m.get("diagnostic_counts")).cloned(),
                    "diagnostics": job.result_json.as_ref().and_then(|m| m.get("diagnostics")).cloned(),
                }),
            );
        }
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    println!(
        "{} crawl job {} {}",
        symbol_for_status(&job.status),
        accent(&id.to_string()),
        status_text(&job.status)
    );
    println!(
        "  {} {}",
        muted("Error:"),
        job.error_text.as_deref().unwrap_or("None")
    );
    println!("  {} {}", muted("Page errors:"), error_pages);
    println!("  {} {}", muted("WAF-blocked pages:"), waf_blocked_pages);
    if let Some(error) = sitemap_backfill_error {
        println!("  {} {}", muted("Sitemap backfill:"), error);
    }
    if let Some(counts) = job
        .result_json
        .as_ref()
        .and_then(|metrics| metrics.get("diagnostic_counts"))
        .and_then(|value| value.as_object())
        .filter(|counts| !counts.is_empty())
    {
        println!("  {}", muted("Diagnostic counts:"));
        for (class, count) in counts {
            println!("    {} {}", muted(class), count);
        }
    }
    if let Some(samples) = job
        .result_json
        .as_ref()
        .and_then(|metrics| metrics.get("diagnostics"))
        .and_then(|value| value.as_array())
        .filter(|samples| !samples.is_empty())
    {
        println!("  {}", muted("Diagnostic samples:"));
        for sample in samples.iter().take(10) {
            let phase = sample
                .get("phase")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            let class = sample
                .get("class")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            let message = sample
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let url = sample.get("url").and_then(|value| value.as_str());
            match url {
                Some(url) => println!("    {}:{} {} {}", phase, class, muted(message), muted(url)),
                None => println!("    {}:{} {}", phase, class, muted(message)),
            }
        }
    }
    if error_pages == 0 && waf_blocked_pages == 0 && sitemap_backfill_error.is_none() {
        println!("  {}", muted("No page-level crawl errors recorded."));
    }
    println!("Job ID: {id}");
    Ok(())
}

fn print_status_metrics(id: Uuid, metrics: &serde_json::Value, has_errors: bool) {
    let md_created = metrics
        .get("md_created")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let filtered_urls = metrics
        .get("filtered_urls")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let pages_crawled = metrics
        .get("pages_crawled")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let pages_discovered = metrics
        .get("pages_discovered")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let sitemap_written = metrics
        .get("sitemap_written")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let sitemap_candidates = metrics
        .get("sitemap_candidates")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let pages_target = pages_discovered.saturating_sub(filtered_urls);
    let thin_md = metrics.get("thin_md").and_then(|v| v.as_u64()).unwrap_or(0);
    let thin_pct = if pages_target > 0 {
        (thin_md as f64 / pages_target as f64) * 100.0
    } else {
        0.0
    };
    println!("  {} {}", muted("md created:"), md_created);
    println!("  {} {}", muted("pages target:"), pages_target);
    println!("  {} {:.1}%", muted("thin % of target:"), thin_pct);
    println!("  {} {}", muted("filtered urls:"), filtered_urls);
    println!("  {} {}", muted("pages crawled:"), pages_crawled);
    println!("  {} {}", muted("pages discovered:"), pages_discovered);
    if has_errors {
        println!(
            "  {} axon crawl errors {}",
            muted("see details:"),
            muted(&id.to_string())
        );
    }
    if sitemap_candidates > 0 || sitemap_written > 0 {
        println!(
            "  {} {}/{}",
            muted("sitemap written/candidates:"),
            sitemap_written,
            sitemap_candidates
        );
    }
    if let Some(waf) = metrics.get("waf_diagnostics").and_then(|v| v.as_object()) {
        let status = waf
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let recovered = waf
            .get("recovered_pages")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let remaining = waf
            .get("remaining_pages")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        println!(
            "  {} {} (recovered {}, remaining {})",
            muted("waf recovery:"),
            status,
            recovered,
            remaining
        );
    }
}

pub(crate) fn render_status_subcommand(
    cfg: &Config,
    job: Option<ServiceJob>,
    id: Uuid,
) -> Result<(), Box<dyn Error>> {
    match job {
        Some(job) if cfg.json_output => {
            handle_job_status(cfg, Some(job), id, "Crawl")?;
        }
        Some(job) => {
            println!(
                "{} {}",
                primary("Crawl Status for"),
                accent(&job.id.to_string())
            );
            println!(
                "  {} {}",
                symbol_for_status(&job.status),
                status_text(&job.status)
            );
            println!(
                "  {} {}",
                muted("URL:"),
                job.url.as_deref().unwrap_or("unknown")
            );
            println!("  {} {}", muted("Created:"), job.created_at);
            println!("  {} {}", muted("Updated:"), job.updated_at);
            if let Some(elapsed) = elapsed_display(&job) {
                println!("  {} {}", muted("Elapsed:"), elapsed);
            }
            if job.attempt_count > 0 {
                println!("  {} {}", muted("Attempt:"), job.attempt_count);
            }
            if is_reclaimed_retry(&job) {
                println!("  {} retry after worker shutdown", muted("Reclaimed:"));
            } else if let Some(err) = job.error_text.as_deref() {
                println!("  {} {}", muted("Error:"), err);
            }
            if let Some(metrics) = job.result_json.as_ref() {
                let (error_pages, waf_blocked, _) = crawl_error_metrics(Some(metrics));
                let total_errors = error_pages + waf_blocked;
                if total_errors > 0 {
                    println!(
                        "  {} {} ({} page errors, {} waf-blocked)",
                        muted("Errors:"),
                        total_errors,
                        error_pages,
                        waf_blocked,
                    );
                }
                print_status_metrics(job.id, metrics, total_errors > 0);
            }
            println!();
            println!("Job ID: {}", job.id);
        }
        None => handle_job_status::<ServiceJob>(cfg, None, id, "Crawl")?,
    }
    Ok(())
}

pub(crate) fn render_list_subcommand(
    cfg: &Config,
    all_jobs: Vec<ServiceJob>,
    total: i64,
) -> Result<(), Box<dyn Error>> {
    let result = crate::services::types::JobListResult::new(all_jobs, total, 50, 0);
    handle_job_list_with_rows(
        cfg,
        &result,
        "Crawl",
        Some("No crawl jobs found."),
        &["", "ID", "Status", "URL", "Progress"],
        |job| {
            vec![
                symbol_for_status(&job.status),
                job.id.to_string(),
                status_text(&job.status),
                muted(job.url.as_deref().unwrap_or("")).to_string(),
                crawl_list_progress_summary(job)
                    .map(|p| muted(&p).to_string())
                    .unwrap_or_default(),
            ]
        },
    )
}

async fn handle_list_subcommand(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let all_jobs = job_service::list_jobs(service_context, JobKind::Crawl, 50, 0).await?;
    let total = all_jobs.len() as i64;
    render_list_subcommand(cfg, all_jobs, total)
}
