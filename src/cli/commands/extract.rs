use crate::cli::commands::CommandFuture;
use crate::cli::commands::common::{
    handle_job_cancel, handle_job_cleanup, handle_job_clear, handle_job_errors, handle_job_list,
    handle_job_recover, handle_job_status, handle_worker_mode, parse_urls,
};
use crate::core::config::Config;
use crate::core::logging::log_info;
use crate::core::ui::{accent, confirm_destructive, muted, primary, symbol_for_status};
use crate::jobs::backend::JobKind;
use crate::services::context::ServiceContext;
use crate::services::extract as extract_service;
use crate::services::jobs as job_service;
use crate::services::types::{JobListResult, StartDisposition};
use std::error::Error;
use uuid::Uuid;

pub fn run_extract<'a>(cfg: &'a Config, service_context: &'a ServiceContext) -> CommandFuture<'a> {
    Box::pin(async move {
        if maybe_handle_extract_subcommand(cfg, service_context).await? {
            return Ok(());
        }

        let urls = parse_urls(cfg);
        if urls.is_empty() {
            return Err(anyhow::anyhow!(
                "extract requires at least one URL (positional or --urls)"
            )
            .into());
        }
        log_info(&format!(
            "command=extract urls={} wait={}",
            urls.len(),
            cfg.wait
        ));
        let prompt = require_extract_prompt(cfg)?;

        if !cfg.wait {
            let result = enqueue_extract_job(cfg, &urls, prompt, service_context).await;
            if result.is_ok() {
                log_info("job_enqueued command=extract");
            }
            return result;
        }

        let result = extract_service::extract_sync(cfg, &urls, &prompt).await?;
        emit_extract_output(cfg, &result)?;
        if result.total_items == 0 {
            return Err(anyhow::anyhow!(
                "extract produced 0 items from {} URL(s); see {} for per-URL summary",
                urls.len(),
                result.summary_path,
            )
            .into());
        }
        Ok(())
    })
}

async fn maybe_handle_extract_subcommand(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<bool, Box<dyn Error>> {
    let Some(subcmd) = cfg.positional.first().map(|s| s.as_str()) else {
        return Ok(false);
    };

    match subcmd {
        "status" => {
            let id = parse_extract_job_id(cfg, "status")?;
            let job = job_service::job_status(service_context, JobKind::Extract, id).await?;
            handle_job_status(cfg, job, id, "Extract")?;
        }
        "cancel" => {
            let id = parse_extract_job_id(cfg, "cancel")?;
            let canceled = job_service::cancel_job(service_context, JobKind::Extract, id).await?;
            handle_job_cancel(cfg, id, canceled, "extract")?;
        }
        "errors" => {
            let id = parse_extract_job_id(cfg, "errors")?;
            let job = job_service::job_status(service_context, JobKind::Extract, id).await?;
            handle_job_errors(cfg, job, id, "extract")?;
        }
        "list" => {
            let jobs = job_service::list_jobs(service_context, JobKind::Extract, 50, 0).await?;
            let total = jobs.len() as i64;
            let result = JobListResult::new(jobs, total, 50, 0);
            handle_job_list(cfg, &result, "Extract")?;
        }
        "cleanup" => {
            let removed = job_service::cleanup_jobs(service_context, JobKind::Extract).await?;
            handle_job_cleanup(cfg, removed, "extract")?;
        }
        "clear" => {
            if confirm_destructive(cfg, "Clear all extract jobs and purge extract queue?")? {
                let removed = job_service::clear_jobs(service_context, JobKind::Extract).await?;
                handle_job_clear(cfg, removed, "extract")?;
            } else if cfg.json_output {
                println!("{}", serde_json::json!({ "removed": 0 }));
            } else {
                println!("{} aborted", symbol_for_status("canceled"));
            }
        }
        "worker" => {
            handle_worker_mode(job_service::start_worker(service_context, JobKind::Extract).await?)?
        }
        "recover" => {
            let reclaimed = job_service::recover_jobs(service_context, JobKind::Extract).await?;
            handle_job_recover(cfg, reclaimed, "extract")?;
        }
        _ => return Ok(false),
    }

    Ok(true)
}

fn parse_extract_job_id(cfg: &Config, action: &str) -> Result<Uuid, Box<dyn Error>> {
    let id = cfg
        .positional
        .get(1)
        .ok_or_else(|| format!("extract {action} requires <job-id>"))?;
    Ok(Uuid::parse_str(id)?)
}

fn require_extract_prompt(cfg: &Config) -> Result<String, Box<dyn Error>> {
    cfg.query
        .as_ref()
        .ok_or("extract requires --query <prompt>")
        .map(|v| v.to_string())
        .map_err(Into::into)
}

async fn enqueue_extract_job(
    cfg: &Config,
    urls: &[String],
    prompt: String,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let outcome =
        extract_service::extract_start_with_context(cfg, urls, Some(prompt), service_context, None)
            .await?;
    let job_id = outcome.result.job_id;
    let status = if outcome.disposition == StartDisposition::Completed {
        "completed"
    } else {
        "pending"
    };
    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({"job_id": job_id, "status": status})
        );
    } else {
        println!("  {} {}", primary("Extract Job"), accent(&job_id));
        println!("  {}", muted(&format!("Status: {status}")));
        println!("Job ID: {job_id}");
    }
    Ok(())
}

fn emit_extract_output(
    cfg: &Config,
    result: &crate::services::types::ExtractSyncResult,
) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result.summary)?);
        return Ok(());
    }

    let summary = &result.summary;
    println!("{}", primary("Extract Results"));
    println!("  {} {}", muted("Pages visited:"), summary["pages_visited"]);
    println!(
        "  {} {}",
        muted("Pages with data:"),
        summary["pages_with_data"]
    );
    println!(
        "  {} {}",
        muted("Deterministic pages:"),
        summary["deterministic_pages"]
    );
    println!(
        "  {} {}",
        muted("LLM fallback pages:"),
        summary["llm_fallback_pages"]
    );
    println!("  {} {}", muted("LLM requests:"), summary["llm_requests"]);
    println!("  {} {}", muted("Total tokens:"), summary["total_tokens"]);
    println!(
        "  {} {:.6}",
        muted("Estimated cost (USD):"),
        summary["estimated_cost_usd"].as_f64().unwrap_or(0.0)
    );
    println!("  {} {}", muted("Total items:"), summary["total_items"]);
    println!("  {} {}", muted("Summary saved:"), result.summary_path);
    println!("  {} {}", muted("Items saved:"), result.items_path);
    Ok(())
}
