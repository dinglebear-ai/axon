use crate::cli::commands::CommandFuture;
use crate::cli::commands::common::{
    filter_jobs_for_status_view, handle_job_cancel, handle_job_cleanup, handle_job_clear,
    handle_job_errors, handle_job_list, handle_job_recover, handle_job_status, handle_worker_mode,
    print_list_footer,
};
use crate::cli::commands::status::metrics::{
    collection_from_config, display_embed_input, embed_metrics_suffix, format_error,
    job_runtime_text,
};
use crate::core::config::Config;
use crate::core::logging::{log_done, log_info};
use crate::core::ui::{
    accent, confirm_destructive, error, muted, primary, status_label, subtle, symbol_for_status,
};
use crate::jobs::backend::JobKind;
use crate::services::context::ServiceContext;
use crate::services::embed as embed_service;
use crate::services::jobs as job_service;
use crate::services::types::StartDisposition;
use std::error::Error;
use std::path::Path;
use uuid::Uuid;

pub(crate) fn render_embed_list(
    cfg: &Config,
    all_jobs: Vec<crate::services::types::ServiceJob>,
    total: i64,
) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        let result = crate::services::types::JobListResult::new(all_jobs, total, 50, 0);
        return handle_job_list(cfg, &result, "Embed");
    }
    let jobs = filter_jobs_for_status_view(cfg, all_jobs);

    println!("{}", primary("Embed Jobs"));
    if jobs.is_empty() {
        println!("  {}", muted("No embed jobs found."));
        return Ok(());
    }

    let empty_crawl_map = std::collections::HashMap::new();
    for job in &jobs {
        let target = display_embed_input(job.target.as_deref().unwrap_or(""), &empty_crawl_map);
        let metrics = embed_metrics_suffix(&job.status, job.result_json.as_ref());
        let collection =
            collection_from_config(job.config_json.as_ref().unwrap_or(&serde_json::Value::Null));
        let age = job_runtime_text(
            &job.status,
            job.started_at.as_ref(),
            job.finished_at.as_ref(),
            &job.updated_at,
        );
        let collection_str = collection
            .map(|c| format!("{}{}", subtle(" | "), accent(c)))
            .unwrap_or_default();
        let label = status_label(&job.status);
        let prefix = if label.is_empty() {
            format!("  {} ", symbol_for_status(&job.status))
        } else {
            format!("  {} {} ", symbol_for_status(&job.status), label)
        };
        let age_str = format!("{}{}", subtle(" | "), accent(&age));
        println!(
            "{}{}{}{}{} {} {}",
            prefix,
            primary(&target),
            metrics,
            collection_str,
            age_str,
            subtle("|"),
            muted(&job.id.to_string()),
        );
        if let Some(err) = format_error(job.error_text.as_deref()) {
            let err_line = error(&format!("↳ {err}"));
            println!("       {err_line}");
        }
    }

    print_list_footer(jobs.len(), total, 50, 0);
    Ok(())
}

pub(crate) fn render_embed_enqueue_result(
    cfg: &Config,
    input: &str,
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
            serde_json::json!({
                "job_id": job_id,
                "status": status,
                "target": input,
                "collection": cfg.collection,
                "source": "rust",
            })
        );
    } else {
        println!("  {} {}", primary("Embed Job"), accent(job_id));
        println!("  {}", muted(&format!("Input: {input}")));
        if disposition == StartDisposition::Completed {
            let message = if via_server {
                "Server completed the embed before returning."
            } else {
                "Lite mode completed the embed in-process."
            };
            println!("  {}", muted(message));
        }
        println!("Job ID: {job_id}");
    }
}

pub fn run_embed<'a>(cfg: &'a Config, service_context: &'a ServiceContext) -> CommandFuture<'a> {
    Box::pin(async move {
        if maybe_handle_embed_subcommand(cfg, service_context).await? {
            return Ok(());
        }

        log_info(&format!(
            "command=embed collection={} wait={}",
            cfg.collection, cfg.wait
        ));
        let embed_start = std::time::Instant::now();
        let input = resolve_embed_input(cfg);
        if !cfg.wait {
            let result = enqueue_embed_job(cfg, &input, service_context).await;
            if result.is_ok() {
                log_info("job_enqueued command=embed");
            }
            return result;
        }

        embed_service::embed_now(cfg, &input).await?;
        log_done(&format!(
            "command=embed complete collection={} duration_ms={}",
            cfg.collection,
            embed_start.elapsed().as_millis()
        ));
        Ok(())
    })
}

async fn maybe_handle_embed_subcommand(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<bool, Box<dyn Error>> {
    let Some(subcmd) = cfg.positional.first().map(|s| s.as_str()) else {
        return Ok(false);
    };
    if cfg.positional.len() == 1 && Path::new(subcmd).exists() {
        // Allow embedding a local path literally named like a subcommand
        // (for example: "./status").
        return Ok(false);
    }

    match subcmd {
        "status" => handle_embed_status(cfg, service_context).await?,
        "cancel" => handle_embed_cancel(cfg, service_context).await?,
        "errors" => handle_embed_errors(cfg, service_context).await?,
        "list" => handle_embed_list(cfg, service_context).await?,
        "cleanup" => handle_embed_cleanup(cfg, service_context).await?,
        "clear" => handle_embed_clear(cfg, service_context).await?,
        "worker" => {
            handle_worker_mode(job_service::start_worker(service_context, JobKind::Embed).await?)?
        }
        "recover" => handle_embed_recover(cfg, service_context).await?,
        _ => return Ok(false),
    }

    Ok(true)
}

fn parse_embed_job_id(cfg: &Config, action: &str) -> Result<Uuid, Box<dyn Error>> {
    let id = cfg
        .positional
        .get(1)
        .ok_or_else(|| format!("embed {action} requires <job-id>"))?;
    Ok(Uuid::parse_str(id)?)
}

async fn handle_embed_status(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let id = parse_embed_job_id(cfg, "status")?;
    let job = job_service::job_status(service_context, JobKind::Embed, id).await?;
    handle_job_status(cfg, job, id, "Embed")
}

async fn handle_embed_cancel(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let id = parse_embed_job_id(cfg, "cancel")?;
    let canceled = job_service::cancel_job(service_context, JobKind::Embed, id).await?;
    handle_job_cancel(cfg, id, canceled, "embed")
}

async fn handle_embed_errors(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let id = parse_embed_job_id(cfg, "errors")?;
    let job = job_service::job_status(service_context, JobKind::Embed, id).await?;
    handle_job_errors(cfg, job, id, "embed")
}

async fn handle_embed_list(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let all_jobs = job_service::list_jobs(service_context, JobKind::Embed, 50, 0).await?;
    let total = all_jobs.len() as i64;
    render_embed_list(cfg, all_jobs, total)
}

async fn handle_embed_cleanup(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let removed = job_service::cleanup_jobs(service_context, JobKind::Embed).await?;
    handle_job_cleanup(cfg, removed, "embed")
}

async fn handle_embed_clear(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    if !confirm_destructive(cfg, "Clear all embed jobs and purge embed queue?")? {
        if cfg.json_output {
            println!("{}", serde_json::json!({ "removed": 0 }));
        } else {
            println!("{} aborted", symbol_for_status("canceled"));
        }
        return Ok(());
    }

    let removed = job_service::clear_jobs(service_context, JobKind::Embed).await?;
    handle_job_clear(cfg, removed, "embed")
}

async fn handle_embed_recover(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let reclaimed = job_service::recover_jobs(service_context, JobKind::Embed).await?;
    handle_job_recover(cfg, reclaimed, "embed")
}

fn resolve_embed_input(cfg: &Config) -> String {
    cfg.positional.first().cloned().unwrap_or_else(|| {
        cfg.output_dir
            .join("markdown")
            .to_string_lossy()
            .to_string()
    })
}

async fn enqueue_embed_job(
    cfg: &Config,
    input: &str,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let outcome =
        embed_service::embed_start_with_context(cfg, input, service_context, None, None).await?;
    let job_id = outcome.result.job_id;
    let status = if outcome.disposition == StartDisposition::Completed {
        "completed"
    } else {
        "pending"
    };
    let _ = status;
    render_embed_enqueue_result(cfg, input, &job_id, outcome.disposition, false);
    Ok(())
}
