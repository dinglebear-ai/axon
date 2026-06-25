use crate::commands::CommandFuture;
use crate::commands::common::{
    handle_job_cancel, handle_job_cleanup, handle_job_clear, handle_job_errors,
    handle_job_list_with_rows, handle_job_recover, handle_job_status, handle_worker_mode,
    parse_urls,
};
use crate::commands::job_progress::extract_progress_summary;
use axon_core::config::Config;
use axon_core::logging::log_info;
use axon_core::ui::{
    accent, confirm_destructive, muted, primary, status_text, symbol_for_status, wait_spinner_for,
};
use axon_jobs::backend::JobKind;
use axon_services::context::ServiceContext;
use axon_services::extract as extract_service;
use axon_services::jobs as job_service;
use axon_services::types::{JobListResult, StartDisposition};
use std::error::Error;
use uuid::Uuid;

pub(crate) fn render_extract_enqueue_result(
    cfg: &Config,
    job_id: &str,
    disposition: StartDisposition,
    via_server: bool,
) {
    let status = if disposition == StartDisposition::Completed {
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
        println!("  {} {}", primary("Extract Job"), accent(job_id));
        println!("  {}", muted(&format!("Status: {status}")));
        if disposition == StartDisposition::Completed {
            let message = if via_server {
                "Server completed the extract before returning."
            } else {
                "SQLite runtime completed the extract in-process."
            };
            println!("  {}", muted(message));
        }
        println!("Job ID: {job_id}");
    }
}

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
        let prompt = cfg.query.clone().unwrap_or_default();

        if !cfg.wait {
            let result = enqueue_extract_job(cfg, &urls, prompt, service_context).await;
            if result.is_ok() {
                log_info("job_enqueued command=extract");
            }
            return result;
        }

        let sp = wait_spinner_for(
            cfg,
            &format!(
                "Extracting {} URL{}…",
                urls.len(),
                if urls.len() == 1 { "" } else { "s" }
            ),
        );
        let result = extract_service::extract_sync(cfg, &urls, &prompt).await?;
        if let Some(sp) = sp {
            sp.clear();
        }
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
            let (limit, offset) = axon_services::transport::job_list_pagination(None, None);
            let jobs =
                job_service::list_jobs(service_context, JobKind::Extract, limit, offset).await?;
            let total = jobs.len() as i64;
            let result = JobListResult::new(jobs, total, limit, offset);
            render_extract_list(cfg, &result)?;
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

fn render_extract_list(
    cfg: &Config,
    result: &JobListResult<axon_services::types::ServiceJob>,
) -> Result<(), Box<dyn Error>> {
    handle_job_list_with_rows(
        cfg,
        result,
        "Extract",
        None,
        &["", "ID", "Status", "Target", "Progress"],
        |job| {
            let target = job
                .urls_json
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| job.id.to_string());
            let progress = extract_progress_summary(job)
                .map(|p| muted(&p).to_string())
                .unwrap_or_default();
            vec![
                symbol_for_status(&job.status),
                job.id.to_string(),
                status_text(&job.status),
                muted(&target).to_string(),
                progress,
            ]
        },
    )
}

fn parse_extract_job_id(cfg: &Config, action: &str) -> Result<Uuid, Box<dyn Error>> {
    let id = cfg
        .positional
        .get(1)
        .ok_or_else(|| format!("extract {action} requires <job-id>"))?;
    Ok(Uuid::parse_str(id)?)
}

async fn enqueue_extract_job(
    cfg: &Config,
    urls: &[String],
    prompt: String,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let prompt = if prompt.trim().is_empty() {
        None
    } else {
        Some(prompt)
    };
    let outcome =
        extract_service::extract_start_with_context(cfg, urls, prompt, service_context, None)
            .await?;
    let job_id = outcome.result.job_id;
    let status = if outcome.disposition == StartDisposition::Completed {
        "completed"
    } else {
        "pending"
    };
    let _ = status;
    render_extract_enqueue_result(cfg, &job_id, outcome.disposition, false);
    Ok(())
}

pub(crate) fn emit_extract_output(
    cfg: &Config,
    result: &axon_services::types::ExtractSyncResult,
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
    if let Some(message) = extract_provenance_message(summary) {
        println!("  {} {}", muted("Parser provenance:"), message);
    }
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

pub(crate) fn extract_provenance_message(summary: &serde_json::Value) -> Option<String> {
    let deterministic_pages = summary
        .get("deterministic_pages")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let llm_fallback_pages = summary
        .get("llm_fallback_pages")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    if deterministic_pages == 0 {
        return None;
    }
    let parser_hits = summary
        .get("parser_hits")
        .and_then(|value| value.as_object())
        .map(|hits| {
            let mut names = hits.keys().cloned().collect::<Vec<_>>();
            names.sort();
            names.join(", ")
        })
        .filter(|names| !names.is_empty())
        .unwrap_or_else(|| "deterministic parsers".to_string());
    if llm_fallback_pages == 0 {
        Some(format!(
            "{deterministic_pages} page(s) handled by {parser_hits}; LLM fallback was not used."
        ))
    } else {
        Some(format!(
            "{deterministic_pages} page(s) handled by {parser_hits}; LLM fallback ran for {llm_fallback_pages} page(s)."
        ))
    }
}

#[cfg(test)]
#[path = "extract_tests.rs"]
mod tests;
