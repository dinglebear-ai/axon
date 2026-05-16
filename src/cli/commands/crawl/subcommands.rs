use super::audit;
use crate::cli::commands::common::{
    filter_jobs_for_status_view, handle_job_cancel, handle_job_cleanup, handle_job_clear,
    handle_job_recover, handle_job_status, handle_worker_mode, print_list_footer, truncate_chars,
};
use crate::cli::commands::job_contracts::JobSummaryEntry;
use crate::core::config::Config;
use crate::core::ui::{
    accent, confirm_destructive, muted, primary, status_text, symbol_for_status,
};
use crate::jobs::backend::JobKind;
use crate::jobs::lite::store::RECLAIMED_ERROR_TEXT;
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

fn reclaim_progress_suffix(job: &ServiceJob) -> &'static str {
    if is_reclaimed_retry(job) {
        " · reclaimed retry"
    } else {
        ""
    }
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
        let out = serde_json::json!({
            "job_id": job.id,
            "status": job.status,
            "url": job.url,
            "error_text": job.error_text,
            "metrics": {
                "error_pages": error_pages,
                "waf_blocked_pages": waf_blocked_pages,
                "sitemap_backfill_error": sitemap_backfill_error,
            }
        });
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
    if error_pages == 0 && waf_blocked_pages == 0 && sitemap_backfill_error.is_none() {
        println!("  {}", muted("No page-level crawl errors recorded."));
    }
    println!("Job ID: {id}");
    Ok(())
}

fn print_status_metrics(id: Uuid, metrics: &serde_json::Value) {
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
    let error_pages = metrics
        .get("error_pages")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let waf_blocked_pages = metrics
        .get("waf_blocked_pages")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if error_pages > 0 || waf_blocked_pages > 0 {
        println!(
            "  {} {} errors, {} waf-blocked",
            muted("crawl errors:"),
            error_pages,
            waf_blocked_pages
        );
        println!(
            "  {} axon crawl errors {}",
            muted("details:"),
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
            if is_reclaimed_retry(&job) {
                println!("  {} retry after worker shutdown", muted("Reclaimed:"));
            } else if let Some(err) = job.error_text.as_deref() {
                println!("  {} {}", muted("Error:"), err);
            }
            if let Some(metrics) = job.result_json.as_ref() {
                print_status_metrics(job.id, metrics);
            }
            println!();
            println!("Job ID: {}", job.id);
        }
        None => handle_job_status::<ServiceJob>(cfg, None, id, "Crawl")?,
    }
    Ok(())
}

/// Returns a compact inline progress string for a crawl job list row.
///
/// - running:   "127 crawled · 43 docs"
/// - completed: "342 docs · 5.2s"
/// - failed:    first 60 chars of error_text
/// - other:     None
fn job_progress_summary(job: &ServiceJob) -> Option<String> {
    match job.status.as_str() {
        "running" => {
            let metrics = job.result_json.as_ref()?;
            let crawled = metrics
                .get("pages_crawled")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let docs = metrics
                .get("md_created")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            if crawled == 0 && docs == 0 {
                return None;
            }
            let errors = metrics
                .get("error_pages")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let error_suffix = if errors > 0 {
                format!(" · {errors} errors")
            } else {
                String::new()
            };
            let reclaim_suffix = reclaim_progress_suffix(job);
            if docs > 0 {
                Some(format!(
                    "{crawled} crawled · {docs} docs{error_suffix}{reclaim_suffix}"
                ))
            } else {
                Some(format!("{crawled} crawled{error_suffix}{reclaim_suffix}"))
            }
        }
        "completed" => {
            let metrics = job.result_json.as_ref()?;
            let docs = metrics
                .get("md_created")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let elapsed_ms = metrics
                .get("elapsed_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let time = if elapsed_ms >= 1000 {
                format!("{:.1}s", elapsed_ms as f64 / 1000.0)
            } else {
                format!("{elapsed_ms}ms")
            };
            Some(format!("{docs} docs · {time}"))
        }
        "failed" => {
            let err = job.error_text.as_deref().unwrap_or("unknown error");
            let truncated = if err.chars().count() > 60 {
                format!("{}…", truncate_chars(err, 60))
            } else {
                err.to_string()
            };
            Some(truncated)
        }
        "pending" => is_reclaimed_retry(job).then(|| "reclaimed retry".to_string()),
        _ => None,
    }
}

pub(crate) fn render_list_subcommand(
    cfg: &Config,
    all_jobs: Vec<ServiceJob>,
    total: i64,
) -> Result<(), Box<dyn Error>> {
    let jobs = filter_jobs_for_status_view(cfg, all_jobs);
    if cfg.json_output {
        let entries: Vec<JobSummaryEntry> =
            jobs.iter().map(JobSummaryEntry::from_service_job).collect();
        let out = serde_json::json!({
            "jobs": entries,
            "total": total,
            "limit": 50_i64,
            "offset": 0_i64,
            "truncated": total > 50,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("{}", primary("Crawl Jobs"));
        if jobs.is_empty() {
            println!("  {}", muted("No crawl jobs found."));
        } else {
            for job in &jobs {
                let progress = job_progress_summary(job);
                if let Some(p) = progress {
                    println!(
                        "  {} {} {} {}  {}",
                        symbol_for_status(&job.status),
                        accent(&job.id.to_string()),
                        status_text(&job.status),
                        muted(job.url.as_deref().unwrap_or("")),
                        muted(&p),
                    );
                } else {
                    println!(
                        "  {} {} {} {}",
                        symbol_for_status(&job.status),
                        accent(&job.id.to_string()),
                        status_text(&job.status),
                        muted(job.url.as_deref().unwrap_or("")),
                    );
                }
            }
        }

        print_list_footer(jobs.len(), total, 50, 0);
    }
    Ok(())
}

async fn handle_list_subcommand(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let all_jobs = job_service::list_jobs(service_context, JobKind::Crawl, 50, 0).await?;
    let total = all_jobs.len() as i64;
    render_list_subcommand(cfg, all_jobs, total)
}
