use crate::commands::common::{
    JobStatus, handle_job_cancel, handle_job_cleanup, handle_job_clear, handle_job_errors,
    handle_job_list_with_rows, handle_job_recover, handle_job_status, handle_worker_mode,
};
use crate::commands::job_progress::ingest_progress;
use axon_api::source::JobKind;
use axon_core::config::Config;
use axon_core::logging::log_done;
use axon_core::ui::confirm_destructive;
use axon_core::ui::{accent, muted, primary, status_text, symbol_for_status};
use axon_services::context::ServiceContext;
use axon_services::ingest::{self as ingest_service, IngestSource};
use axon_services::jobs as job_service;
use axon_services::types::ServiceJob;
use std::error::Error;
use uuid::Uuid;

/// Routes ingest subcommands (status, cancel, errors, list, cleanup, clear, worker, recover).
///
/// Returns `Ok(true)` if a subcommand was handled, `Ok(false)` if the first
/// positional arg is not a recognized subcommand (i.e. it's an ingest target).
///
/// NOTE: Target values that collide with subcommand names ("status", "list",
/// "cancel", "cleanup", "clear", "worker", "recover") will be intercepted as
/// subcommands rather than treated as ingest targets. This is a known
/// limitation shared with the crawl/batch/extract command routing.
pub async fn maybe_handle_ingest_subcommand(
    cfg: &Config,
    service_context: &ServiceContext,
    cmd_name: &str,
) -> Result<bool, Box<dyn Error>> {
    let Some(subcmd) = cfg.positional.first().map(|s| s.as_str()) else {
        return Ok(false);
    };

    match subcmd {
        "status" => {
            let id = parse_ingest_job_id(cfg, cmd_name, "status")?;
            let job = job_service::job_status(service_context, JobKind::Source, id).await?;
            render_ingest_status(cfg, job, id)?;
        }
        "cancel" => {
            let id = parse_ingest_job_id(cfg, cmd_name, "cancel")?;
            let canceled = job_service::cancel_job(service_context, JobKind::Source, id).await?;
            handle_job_cancel(cfg, id, canceled, "ingest")?;
        }
        "errors" => {
            let id = parse_ingest_job_id(cfg, cmd_name, "errors")?;
            let job = job_service::job_status(service_context, JobKind::Source, id).await?;
            handle_job_errors(cfg, job, id, "ingest")?;
        }
        "list" => {
            let source_filter = if cmd_name == "sessions" {
                Some("sessions")
            } else {
                None
            };
            let (limit, offset) = axon_services::transport::job_list_pagination(None, None);
            let jobs = job_service::list_ingest_jobs(service_context, source_filter, limit, offset)
                .await?;
            let total = jobs.len() as i64;
            render_ingest_list(cfg, jobs, total, cmd_name)?;
        }
        "cleanup" => {
            let removed = job_service::cleanup_jobs(service_context, JobKind::Source).await?;
            handle_job_cleanup(cfg, removed, "ingest")?;
        }
        "clear" => {
            if confirm_destructive(cfg, "Clear all ingest jobs and purge ingest queue?")? {
                let removed = job_service::clear_jobs(service_context, JobKind::Source).await?;
                handle_job_clear(cfg, removed, "ingest")?;
            } else if cfg.json_output {
                println!("{}", serde_json::json!({ "removed": 0 }));
            } else {
                println!("{} aborted", symbol_for_status("canceled"));
            }
        }
        "worker" => {
            handle_worker_mode(job_service::start_worker(service_context, JobKind::Source).await?)?
        }
        "recover" => {
            let reclaimed = job_service::recover_jobs(service_context, JobKind::Source).await?;
            handle_job_recover(cfg, reclaimed, "ingest")?;
        }
        _ => return Ok(false),
    }

    Ok(true)
}

pub fn parse_ingest_job_id(
    cfg: &Config,
    cmd_name: &str,
    action: &str,
) -> Result<Uuid, Box<dyn Error>> {
    let id = cfg
        .positional
        .get(1)
        .ok_or_else(|| format!("{cmd_name} {action} requires <job-id>"))?;
    Ok(Uuid::parse_str(id)?)
}

/// Extract a reclaim label if this job has been watchdog-reclaimed at least once.
fn ingest_reclaim_label(result_json: &Option<serde_json::Value>) -> Option<String> {
    let count = result_json
        .as_ref()?
        .get("_reclaim")?
        .get("count")?
        .as_u64()?;
    if count > 0 {
        Some(format!("⟳ reclaimed {count}x"))
    } else {
        None
    }
}

pub(crate) fn render_ingest_status(
    cfg: &Config,
    job: Option<ServiceJob>,
    id: Uuid,
) -> Result<(), Box<dyn Error>> {
    match job {
        Some(job) if cfg.json_output => {
            handle_job_status(cfg, Some(job), id, "Ingest")?;
        }
        Some(job) => {
            println!(
                "{} {}",
                primary("Ingest Status for"),
                accent(&job.id.to_string())
            );
            println!(
                "  {} {}",
                symbol_for_status(&job.status),
                status_text(&job.status)
            );
            println!(
                "  {} {} / {}",
                muted("Source:"),
                job.source_type.as_deref().unwrap_or("unknown"),
                job.target.as_deref().unwrap_or("unknown")
            );
            println!("  {} {}", muted("Created:"), job.created_at);
            println!("  {} {}", muted("Updated:"), job.updated_at);
            let live_progress = job
                .progress_json
                .as_ref()
                .or(job.result_json.as_ref())
                .cloned();
            if let Some(progress) = ingest_progress(&live_progress) {
                println!("  {} {}", muted("Progress:"), progress);
            }
            if let Some(reclaim) = ingest_reclaim_label(&job.progress_json) {
                println!("  {} {}", muted("Reclaimed:"), accent(&reclaim));
            }
            if let Some(err) = job.error_text.as_deref() {
                println!("  {} {}", muted("Error:"), err);
            }
            println!("Job ID: {}", job.id);
        }
        None => handle_job_status::<ServiceJob>(cfg, None, id, "Ingest")?,
    }
    Ok(())
}

pub(crate) fn render_ingest_list(
    cfg: &Config,
    all_jobs: Vec<ServiceJob>,
    total: i64,
    cmd_name: &str,
) -> Result<(), Box<dyn Error>> {
    let (limit, offset) = axon_services::transport::job_list_pagination(None, None);
    let result = axon_services::types::JobListResult::new(all_jobs, total, limit, offset);
    let (command_name, empty_msg) = if cmd_name == "sessions" {
        ("Sessions", "No sessions jobs found.")
    } else {
        ("Ingest", "No ingest jobs found.")
    };
    handle_job_list_with_rows(
        cfg,
        &result,
        command_name,
        Some(empty_msg),
        &[
            "",
            "ID",
            "Status",
            "Source",
            "Target",
            "Progress",
            "Reclaimed",
        ],
        |job| {
            vec![
                symbol_for_status(&job.status),
                job.id().to_string(),
                status_text(&job.status),
                job.source_type.as_deref().unwrap_or("unknown").to_string(),
                job.target.as_deref().unwrap_or("unknown").to_string(),
                ingest_progress(
                    &job.progress_json
                        .as_ref()
                        .or(job.result_json.as_ref())
                        .cloned(),
                )
                .unwrap_or_default(),
                ingest_reclaim_label(&job.progress_json).unwrap_or_default(),
            ]
        },
    )
}

pub async fn enqueue_ingest_job(
    cfg: &Config,
    source: IngestSource,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    // No per-caller auth identity is threaded through the CLI ingest command
    // today — this is a genuinely internal call site, made explicit by
    // passing `None`.
    let result =
        ingest_service::ingest_start_with_context(cfg, source, service_context, None).await?;
    let job_id = result.result.job_id;
    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({"job_id": job_id, "status": "pending", "collection": cfg.collection})
        );
    } else {
        println!(
            "  {} {}",
            primary("Ingest Job"),
            accent(&job_id.to_string())
        );
        println!("  {}", muted("Status: pending"));
        println!("  {} {}", muted("Collection:"), accent(&cfg.collection));
    }
    Ok(())
}

pub fn print_ingest_sync_result(cfg: &Config, cmd_name: &str, chunks: usize, target: &str) {
    log_done(&format!(
        "{cmd_name} ingest complete: {chunks} chunks embedded"
    ));
    if cfg.json_output {
        println!("{}", serde_json::json!({"chunks_embedded": chunks}));
    } else {
        println!(
            "{} {} chunks embedded from {}",
            symbol_for_status("completed"),
            accent(&chunks.to_string()),
            muted(target)
        );
    }
}
