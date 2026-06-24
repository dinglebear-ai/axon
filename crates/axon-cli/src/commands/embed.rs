use crate::commands::CommandFuture;
use crate::commands::common::{
    handle_job_cancel, handle_job_cleanup, handle_job_clear, handle_job_errors,
    handle_job_list_with_rows, handle_job_recover, handle_job_status, handle_worker_mode,
};
use crate::commands::job_progress::embed_progress_summary;
use crate::commands::status::metrics::{
    collection_from_config, display_embed_input, format_error, job_runtime_text,
};
use axon_core::config::Config;
use axon_core::logging::{log_done, log_info};
use axon_core::ui::wait_spinner_for;
use axon_core::ui::{accent, confirm_destructive, error, muted, primary, symbol_for_status};
use axon_jobs::backend::JobKind;
use axon_services::context::ServiceContext;
use axon_services::embed as embed_service;
use axon_services::jobs as job_service;
use axon_services::types::StartDisposition;
use std::error::Error;
use std::path::Path;
use uuid::Uuid;

pub(crate) fn render_embed_list(
    cfg: &Config,
    all_jobs: Vec<axon_services::types::ServiceJob>,
    total: i64,
) -> Result<(), Box<dyn Error>> {
    let (limit, offset) = axon_services::transport::job_list_pagination(None, None);
    let result = axon_services::types::JobListResult::new(all_jobs, total, limit, offset);
    let empty_crawl_map = std::collections::HashMap::new();
    handle_job_list_with_rows(
        cfg,
        &result,
        "Embed",
        Some("No embed jobs found."),
        &[
            "",
            "ID",
            "Status",
            "Input",
            "Progress",
            "Collection",
            "Age",
            "Error",
        ],
        |job| {
            let target = display_embed_input(
                job.target.as_deref().unwrap_or(""),
                job.config_json.as_ref(),
                &empty_crawl_map,
            );
            let collection = collection_from_config(
                job.config_json.as_ref().unwrap_or(&serde_json::Value::Null),
            )
            .unwrap_or("");
            let age = job_runtime_text(
                &job.status,
                job.started_at.as_ref(),
                job.finished_at.as_ref(),
                &job.updated_at,
            );
            vec![
                symbol_for_status(&job.status),
                job.id.to_string(),
                axon_core::ui::status_text(&job.status),
                primary(&target).to_string(),
                embed_progress_summary(job, None)
                    .map(|summary| accent(&summary).to_string())
                    .unwrap_or_default(),
                accent(collection).to_string(),
                accent(&age).to_string(),
                format_error(job.error_text.as_deref())
                    .map(|err| error(&err).to_string())
                    .unwrap_or_default(),
            ]
        },
    )
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
                "SQLite runtime completed the embed in-process."
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
        // A local path can only be embedded by a process that shares its
        // filesystem. A fire-and-forget CLI never services its own queue, so
        // an enqueued host path lands on whatever long-running worker exists —
        // usually the axon container, which cannot see the host home dir.
        // Local-path embeds therefore always run in-process here; only URL /
        // free-text inputs go through the shared queue when --wait is false.
        let input_is_local_path = Path::new(&input).exists();
        if !cfg.wait && !input_is_local_path {
            let result = enqueue_embed_job(cfg, &input, service_context).await;
            if result.is_ok() {
                log_info("job_enqueued command=embed");
            }
            return result;
        }
        if !cfg.wait && input_is_local_path {
            log_info("command=embed local_path_runs_in_process");
        }

        let sp = wait_spinner_for(cfg, &format!("Embedding {}…", input));
        embed_service::embed_now(cfg, &input).await?;
        if let Some(sp) = sp {
            sp.finish("✓ Embedded");
        }
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
    let (limit, offset) = axon_services::transport::job_list_pagination(None, None);
    let all_jobs = job_service::list_jobs(service_context, JobKind::Embed, limit, offset).await?;
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
