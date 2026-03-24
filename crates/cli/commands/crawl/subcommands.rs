use super::audit;
use crate::crates::cli::commands::common::{
    filter_jobs_for_status_view, handle_job_cancel, handle_job_cleanup, handle_job_clear,
    handle_job_errors, handle_job_recover, handle_job_status, print_list_footer, truncate_chars,
};
use crate::crates::cli::commands::job_contracts::JobSummaryEntry;
use crate::crates::core::config::Config;
use crate::crates::core::ui::{
    accent, confirm_destructive, muted, primary, status_text, symbol_for_status,
};
use crate::crates::services::crawl as crawl_service;
use crate::crates::services::crawl::CrawlJob;
use std::error::Error;
use uuid::Uuid;

pub(super) async fn maybe_handle_subcommand(cfg: &Config) -> Result<bool, Box<dyn Error>> {
    let Some(subcmd) = cfg.positional.first().map(|s| s.as_str()) else {
        return Ok(false);
    };
    match subcmd {
        "status" => {
            let id = parse_required_job_id(cfg, "status")?;
            let job = crawl_service::crawl_status_raw(cfg, id).await?;
            handle_status_subcommand(cfg, job, id).await?;
        }
        "cancel" => {
            let id = parse_required_job_id(cfg, "cancel")?;
            let canceled = crawl_service::crawl_cancel(cfg, id).await?;
            handle_job_cancel(cfg, id, canceled, "crawl")?;
        }
        "errors" => {
            let id = parse_required_job_id(cfg, "errors")?;
            let job = crawl_service::crawl_status_raw(cfg, id).await?;
            handle_job_errors(cfg, job, id, "crawl")?;
        }
        "list" => handle_list_subcommand(cfg).await?,
        "cleanup" => {
            let removed = crawl_service::crawl_cleanup(cfg).await?;
            handle_job_cleanup(cfg, removed, "crawl")?;
        }
        "clear" => {
            if confirm_destructive(cfg, "Clear all crawl jobs and purge crawl queue?")? {
                let removed = crawl_service::crawl_clear(cfg).await?;
                handle_job_clear(cfg, removed, "crawl")?;
            } else if cfg.json_output {
                println!("{}", serde_json::json!({ "removed": 0 }));
            } else {
                println!("{} aborted", symbol_for_status("canceled"));
            }
        }
        "worker" => crawl_service::crawl_worker(cfg).await?,
        "recover" => {
            let reclaimed = crawl_service::crawl_recover(cfg).await?;
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

fn print_status_metrics(metrics: &serde_json::Value) {
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
    if sitemap_candidates > 0 || sitemap_written > 0 {
        println!(
            "  {} {}/{}",
            muted("sitemap written/candidates:"),
            sitemap_written,
            sitemap_candidates
        );
    }
}

async fn handle_status_subcommand(
    cfg: &Config,
    job: Option<CrawlJob>,
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
            println!("  {} {}", muted("URL:"), job.url);
            println!("  {} {}", muted("Created:"), job.created_at);
            println!("  {} {}", muted("Updated:"), job.updated_at);
            if let Some(err) = job.error_text.as_deref() {
                println!("  {} {}", muted("Error:"), err);
            }
            if let Some(metrics) = job.result_json.as_ref() {
                print_status_metrics(metrics);
            }
            println!();
            println!("Job ID: {}", job.id);
        }
        None => handle_job_status::<CrawlJob>(cfg, None, id, "Crawl")?,
    }
    Ok(())
}

/// Returns a compact inline progress string for a crawl job list row.
///
/// - running:   "127 crawled · 43 docs"
/// - completed: "342 docs · 5.2s"
/// - failed:    first 60 chars of error_text
/// - other:     None
fn job_progress_summary(job: &CrawlJob) -> Option<String> {
    match job.status.as_str() {
        "running" => {
            let metrics = job.result_json.as_ref()?;
            let crawled = metrics
                .get("pages_crawled")
                .and_then(|v: &serde_json::Value| v.as_u64())
                .unwrap_or(0);
            let docs = metrics
                .get("md_created")
                .and_then(|v: &serde_json::Value| v.as_u64())
                .unwrap_or(0);
            if crawled == 0 && docs == 0 {
                return None;
            }
            if docs > 0 {
                Some(format!("{crawled} crawled · {docs} docs"))
            } else {
                Some(format!("{crawled} crawled"))
            }
        }
        "completed" => {
            let metrics = job.result_json.as_ref()?;
            let docs = metrics
                .get("md_created")
                .and_then(|v: &serde_json::Value| v.as_u64())
                .unwrap_or(0);
            let elapsed_ms = metrics
                .get("elapsed_ms")
                .and_then(|v: &serde_json::Value| v.as_u64())
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
        _ => None,
    }
}

async fn handle_list_subcommand(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let result = crawl_service::crawl_list_raw(cfg, 50, 0).await?;
    let jobs = filter_jobs_for_status_view(cfg, &result.jobs);
    if cfg.json_output {
        let entries: Vec<JobSummaryEntry> = jobs.iter().map(JobSummaryEntry::from_crawl).collect();
        let out = serde_json::json!({
            "jobs": entries,
            "total": result.total,
            "limit": result.limit,
            "offset": result.offset,
            "truncated": result.is_truncated(),
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
                        muted(&job.url),
                        muted(&p),
                    );
                } else {
                    println!(
                        "  {} {} {} {}",
                        symbol_for_status(&job.status),
                        accent(&job.id.to_string()),
                        status_text(&job.status),
                        muted(&job.url),
                    );
                }
            }
        }

        print_list_footer(jobs.len(), result.total, result.limit, result.offset);
    }
    Ok(())
}
