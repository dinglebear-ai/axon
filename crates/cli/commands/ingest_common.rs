use crate::crates::cli::commands::common::{
    JobStatus, handle_job_cancel, handle_job_cleanup, handle_job_clear, handle_job_errors,
    handle_job_list, handle_job_recover, handle_job_status,
};
use crate::crates::core::config::Config;
use crate::crates::core::logging::log_done;
use crate::crates::core::ui::confirm_destructive;
use crate::crates::core::ui::{accent, muted, primary, status_text, symbol_for_status};
use crate::crates::jobs::ingest as ingest_jobs;
use crate::crates::jobs::ingest::IngestJob;
use crate::crates::services::ingest::{self as ingest_service, IngestSource};
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
    cmd_name: &str,
) -> Result<bool, Box<dyn Error>> {
    let Some(subcmd) = cfg.positional.first().map(|s| s.as_str()) else {
        return Ok(false);
    };

    match subcmd {
        "status" => {
            let id = parse_ingest_job_id(cfg, cmd_name, "status")?;
            let job = ingest_jobs::get_ingest_job(cfg, id).await?;
            handle_ingest_status(cfg, job, id).await?;
        }
        "cancel" => {
            let id = parse_ingest_job_id(cfg, cmd_name, "cancel")?;
            let canceled = ingest_jobs::cancel_ingest_job(cfg, id).await?;
            handle_job_cancel(cfg, id, canceled, "ingest")?;
        }
        "errors" => {
            let id = parse_ingest_job_id(cfg, cmd_name, "errors")?;
            let job = ingest_jobs::get_ingest_job(cfg, id).await?;
            handle_job_errors(cfg, job, id, "ingest")?;
        }
        "list" => {
            let jobs = ingest_jobs::list_ingest_jobs(cfg, 50, 0).await?;
            handle_ingest_list(cfg, jobs).await?;
        }
        "cleanup" => {
            let removed = ingest_jobs::cleanup_ingest_jobs(cfg).await?;
            handle_job_cleanup(cfg, removed, "ingest")?;
        }
        "clear" => {
            if confirm_destructive(cfg, "Clear all ingest jobs and purge ingest queue?")? {
                let removed = ingest_jobs::clear_ingest_jobs(cfg).await?;
                handle_job_clear(cfg, removed, "ingest")?;
            } else if cfg.json_output {
                println!(
                    "{}",
                    serde_json::json!({ "removed": 0, "queue_purged": false })
                );
            } else {
                println!("{} aborted", symbol_for_status("canceled"));
            }
        }
        "worker" => ingest_jobs::run_ingest_worker(cfg).await?,
        "recover" => {
            let reclaimed = ingest_jobs::recover_stale_ingest_jobs(cfg).await?;
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

/// Extract a progress string from an ingest job's `result_json`.
///
/// Handles YouTube playlists (`videos_done/videos_total`) and GitHub repos
/// (`files_done/files_total`). Returns `None` for single-video YouTube,
/// Reddit, or jobs that haven't started producing progress yet.
fn ingest_progress(result_json: &Option<serde_json::Value>) -> Option<String> {
    let r = result_json.as_ref()?;
    let chunks = r
        .get("chunks_embedded")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    // YouTube playlist progress
    if let (Some(done), Some(total)) = (
        r.get("videos_done").and_then(|v| v.as_u64()),
        r.get("videos_total").and_then(|v| v.as_u64()),
    ) {
        return Some(format!("{done} / {total} videos, {chunks} chunks embedded"));
    }

    // GitHub file-level progress
    if let (Some(done), Some(total)) = (
        r.get("files_done").and_then(|v| v.as_u64()),
        r.get("files_total").and_then(|v| v.as_u64()),
    ) {
        return Some(format!("{done} / {total} files, {chunks} chunks embedded"));
    }

    // GitHub task-level progress (before file-level kicks in)
    if let (Some(done), Some(total)) = (
        r.get("tasks_done").and_then(|v| v.as_u64()),
        r.get("tasks_total").and_then(|v| v.as_u64()),
    ) {
        if chunks > 0 {
            return Some(format!("{done} / {total} tasks, {chunks} chunks embedded"));
        }
        let phase = r.get("phase").and_then(|v| v.as_str()).unwrap_or("working");
        return Some(format!("{phase} ({done} / {total} tasks)"));
    }

    // Generic chunks-only (e.g. single YouTube video)
    if chunks > 0 {
        return Some(format!("{chunks} chunks embedded"));
    }

    None
}

async fn handle_ingest_status(
    cfg: &Config,
    job: Option<IngestJob>,
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
                job.source_type,
                job.target
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
        None => handle_job_status::<IngestJob>(cfg, None, id, "Ingest")?,
    }
    Ok(())
}

async fn handle_ingest_list(cfg: &Config, jobs: Vec<IngestJob>) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        handle_job_list(cfg, jobs, "Ingest")?;
    } else {
        println!("{}", primary("Ingest Jobs"));
        if jobs.is_empty() {
            println!("  {}", muted("No ingest jobs found."));
        } else {
            for job in jobs {
                let progress = ingest_progress(&job.result_json)
                    .map(|p| format!(" [{p}]"))
                    .unwrap_or_default();
                let reclaim = ingest_reclaim_label(&job.result_json)
                    .map(|r| format!(" {}", muted(&format!("({r})"))))
                    .unwrap_or_default();
                println!(
                    "  {} {} {}/{}{progress}{reclaim}",
                    symbol_for_status(&job.status),
                    accent(&job.id().to_string()),
                    job.source_type,
                    job.target
                );
            }
        }
    }
    Ok(())
}

pub async fn enqueue_ingest_job(cfg: &Config, source: IngestSource) -> Result<(), Box<dyn Error>> {
    let result = ingest_service::ingest_start(cfg, source).await?;
    let job_id = result.job_id;
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
        println!("Job ID: {job_id}");
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
    let (chunks, source_label, target_label) = match &source {
        IngestSource::Youtube { target } => {
            let result = ingest_service::ingest_youtube(cfg, target, None).await?;
            let n = result.payload["chunks"]
                .as_u64()
                .ok_or("ingest: service payload missing 'chunks' field")?
                as usize;
            (n, "youtube", target.clone())
        }
        IngestSource::Github { repo, .. } => {
            let (owner, repo_name) = repo
                .split_once('/')
                .ok_or_else(|| format!("ingest: GitHub repo must be 'owner/repo', got '{repo}'"))?;
            let result = ingest_service::ingest_github(cfg, owner, repo_name, None).await?;
            let n = result.payload["chunks"]
                .as_u64()
                .ok_or("ingest: service payload missing 'chunks' field")?
                as usize;
            (n, "github", repo.clone())
        }
        IngestSource::Reddit { target } => {
            let result = ingest_service::ingest_reddit(cfg, target, None).await?;
            let n = result.payload["chunks"]
                .as_u64()
                .ok_or("ingest: service payload missing 'chunks' field")?
                as usize;
            (n, "reddit", target.clone())
        }
        IngestSource::Sessions { .. } => {
            return Err("sessions ingest is handled by the sessions command, not ingest".into());
        }
    };
    print_ingest_sync_result(cfg, source_label, chunks, &target_label);
    Ok(())
}
