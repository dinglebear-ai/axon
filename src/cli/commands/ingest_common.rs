use crate::cli::commands::common::{
    JobStatus, handle_job_cancel, handle_job_cleanup, handle_job_clear, handle_job_errors,
    handle_job_list_with_rows, handle_job_recover, handle_job_status, handle_worker_mode,
};
use crate::cli::commands::job_progress::ingest_progress;
use crate::core::config::Config;
use crate::core::logging::log_done;
use crate::core::ui::confirm_destructive;
use crate::core::ui::{accent, muted, primary, status_text, symbol_for_status};
use crate::jobs::backend::JobKind;
use crate::services::context::ServiceContext;
use crate::services::ingest::{self as ingest_service, IngestSource};
use crate::services::jobs as job_service;
use crate::services::types::ServiceJob;
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
            let job = job_service::job_status(service_context, JobKind::Ingest, id).await?;
            render_ingest_status(cfg, job, id)?;
        }
        "cancel" => {
            let id = parse_ingest_job_id(cfg, cmd_name, "cancel")?;
            let canceled = job_service::cancel_job(service_context, JobKind::Ingest, id).await?;
            handle_job_cancel(cfg, id, canceled, "ingest")?;
        }
        "errors" => {
            let id = parse_ingest_job_id(cfg, cmd_name, "errors")?;
            let job = job_service::job_status(service_context, JobKind::Ingest, id).await?;
            handle_job_errors(cfg, job, id, "ingest")?;
        }
        "list" => {
            let source_filter = if cmd_name == "sessions" {
                Some("sessions")
            } else {
                None
            };
            let jobs = job_service::list_ingest_jobs(service_context, source_filter, 50, 0).await?;
            let total = jobs.len() as i64;
            render_ingest_list(cfg, jobs, total, cmd_name)?;
        }
        "cleanup" => {
            let removed = job_service::cleanup_jobs(service_context, JobKind::Ingest).await?;
            handle_job_cleanup(cfg, removed, "ingest")?;
        }
        "clear" => {
            if confirm_destructive(cfg, "Clear all ingest jobs and purge ingest queue?")? {
                let removed = job_service::clear_jobs(service_context, JobKind::Ingest).await?;
                handle_job_clear(cfg, removed, "ingest")?;
            } else if cfg.json_output {
                println!("{}", serde_json::json!({ "removed": 0 }));
            } else {
                println!("{} aborted", symbol_for_status("canceled"));
            }
        }
        "worker" => {
            handle_worker_mode(job_service::start_worker(service_context, JobKind::Ingest).await?)?
        }
        "recover" => {
            let reclaimed = job_service::recover_jobs(service_context, JobKind::Ingest).await?;
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

fn chunks_embedded_from_payload(payload: &serde_json::Value) -> Option<u64> {
    payload
        .get("chunks_embedded")
        .or_else(|| payload.get("chunks"))
        .and_then(|value| value.as_u64())
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
            if let Some(progress) = ingest_progress(&job.result_json) {
                println!("  {} {}", muted("Progress:"), progress);
            }
            if let Some(reclaim) = ingest_reclaim_label(&job.result_json) {
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
    let result = crate::services::types::JobListResult::new(all_jobs, total, 50, 0);
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
                ingest_progress(&job.result_json).unwrap_or_default(),
                ingest_reclaim_label(&job.result_json).unwrap_or_default(),
            ]
        },
    )
}

pub async fn enqueue_ingest_job(
    cfg: &Config,
    source: IngestSource,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let result = ingest_service::ingest_start_with_context(cfg, source, service_context).await?;
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

/// Run an ingest job synchronously (blocking until complete).
///
/// Dispatches to the appropriate service function based on the `IngestSource` variant
/// and prints a completion summary. Called by `run_ingest` when `--wait true` is set.
pub async fn run_ingest_sync(cfg: &Config, source: IngestSource) -> Result<(), Box<dyn Error>> {
    // Stamp the ingest target as the chunk origin (seed_url), matching the async
    // ingest job runner, so synchronous `--wait true` ingests record the same
    // re-ingestable origin instead of falling back to per-doc page URLs.
    let mut seeded_cfg = cfg.clone();
    seeded_cfg.seed_url = Some(crate::jobs::ingest::types::target_label(&source));
    let cfg = &seeded_cfg;
    let (chunks, source_label, target_label) = match &source {
        IngestSource::Youtube { target } => {
            let result = ingest_service::ingest_youtube(cfg, target, None).await?;
            let n = chunks_embedded_from_payload(&result.payload)
                .ok_or("ingest: service payload missing chunk count field")?
                as usize;
            (n, "youtube", target.clone())
        }
        IngestSource::Github { repo, .. } => {
            let (owner, repo_name) = repo
                .split_once('/')
                .ok_or_else(|| format!("ingest: GitHub repo must be 'owner/repo', got '{repo}'"))?;
            let result = ingest_service::ingest_github(cfg, owner, repo_name, None).await?;
            let n = chunks_embedded_from_payload(&result.payload)
                .ok_or("ingest: service payload missing chunk count field")?
                as usize;
            (n, "github", repo.clone())
        }
        IngestSource::Gitlab { target, .. } => {
            let result =
                ingest_service::ingest_gitlab_with_progress(cfg, target, None, None).await?;
            let n = chunks_embedded_from_payload(&result.payload)
                .ok_or("ingest: service payload missing chunk count field")?
                as usize;
            (n, "gitlab", target.clone())
        }
        IngestSource::Gitea { target, .. } => {
            let result =
                ingest_service::ingest_gitea_with_progress(cfg, target, None, None).await?;
            let n = chunks_embedded_from_payload(&result.payload)
                .ok_or("ingest: service payload missing chunk count field")?
                as usize;
            (n, "gitea", target.clone())
        }
        IngestSource::GenericGit { target, .. } => {
            let result =
                ingest_service::ingest_generic_git_with_progress(cfg, target, None, None).await?;
            let n = chunks_embedded_from_payload(&result.payload)
                .ok_or("ingest: service payload missing chunk count field")?
                as usize;
            (n, "git", target.clone())
        }
        IngestSource::Reddit { target } => {
            let result = ingest_service::ingest_reddit(cfg, target, None).await?;
            let n = chunks_embedded_from_payload(&result.payload)
                .ok_or("ingest: service payload missing chunk count field")?
                as usize;
            (n, "reddit", target.clone())
        }
        IngestSource::Sessions { .. } => {
            return Err(anyhow::anyhow!(
                "sessions ingest is handled by the sessions command, not ingest"
            )
            .into());
        }
        IngestSource::PreparedSessions { .. } => {
            return Err(anyhow::anyhow!(
                "prepared sessions ingest is handled by the sessions command, not ingest"
            )
            .into());
        }
    };
    print_ingest_sync_result(cfg, source_label, chunks, &target_label);
    Ok(())
}
