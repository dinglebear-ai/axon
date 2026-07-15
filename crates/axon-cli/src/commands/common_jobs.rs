use crate::commands::job_contracts::{
    JobCancelResponse, JobErrorsResponse, JobStatusResponse, JobSummaryEntry,
};
use axon_core::config::Config;
use axon_core::ui::{accent, muted, primary, status_text, symbol_for_status};
use axon_services::runtime::WorkerMode;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::error::Error;
use uuid::Uuid;

pub trait JobStatus {
    fn id(&self) -> Uuid;
    fn status(&self) -> &str;
    fn created_at(&self) -> DateTime<Utc>;
    fn updated_at(&self) -> DateTime<Utc>;
    fn error_text(&self) -> Option<&str>;
    fn to_status_response_json(&self) -> Value;
    fn to_summary_entry_json(&self) -> Value;
    fn to_errors_response_json(&self) -> Value;
}

pub fn include_job_for_status_view(cfg: &Config, status: &str) -> bool {
    if cfg.active_status_only {
        return matches!(status, "pending" | "running" | "processing" | "scraping");
    }
    if cfg.recent_status_only {
        return matches!(
            status,
            "pending" | "running" | "processing" | "scraping" | "completed"
        );
    }
    true
}

pub fn filter_jobs_for_status_view<T: JobStatus>(cfg: &Config, jobs: Vec<T>) -> Vec<T> {
    jobs.into_iter()
        .filter(|job| include_job_for_status_view(cfg, job.status()))
        .collect()
}

macro_rules! impl_job_status {
    ($ty:path, $status_ctor:path, $summary_ctor:path) => {
        impl JobStatus for $ty {
            fn id(&self) -> Uuid {
                self.id
            }
            fn status(&self) -> &str {
                &self.status
            }
            fn created_at(&self) -> DateTime<Utc> {
                self.created_at
            }
            fn updated_at(&self) -> DateTime<Utc> {
                self.updated_at
            }
            fn error_text(&self) -> Option<&str> {
                self.error_text.as_deref()
            }
            fn to_status_response_json(&self) -> Value {
                serde_json::to_value($status_ctor(self))
                    .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}))
            }
            fn to_summary_entry_json(&self) -> Value {
                serde_json::to_value($summary_ctor(self))
                    .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}))
            }
            fn to_errors_response_json(&self) -> Value {
                serde_json::to_value(JobErrorsResponse::from_job(
                    self.id,
                    self.status.clone(),
                    self.error_text.clone(),
                ))
                .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}))
            }
        }
    };
}

impl_job_status!(
    axon_services::types::ServiceJob,
    JobStatusResponse::from_service_job,
    JobSummaryEntry::from_service_job
);

fn print_pretty_json(value: &Value) -> Result<(), Box<dyn Error>> {
    crate::json::print_json_gated(value)
}

pub fn handle_job_status<T: JobStatus>(
    cfg: &Config,
    job: Option<T>,
    job_id: Uuid,
    command_name: &str,
) -> Result<(), Box<dyn Error>> {
    match job {
        Some(job) => {
            if cfg.json_output {
                print_pretty_json(&job.to_status_response_json())?;
            } else {
                println!(
                    "{} {}",
                    primary(&format!("{command_name} Status for")),
                    accent(&job.id().to_string())
                );
                println!(
                    "  {} {}",
                    symbol_for_status(job.status()),
                    status_text(job.status())
                );
                println!("  {} {}", muted("Created:"), job.created_at());
                println!("  {} {}", muted("Updated:"), job.updated_at());
                if let Some(err) = job.error_text() {
                    println!("  {} {}", muted("Error:"), err);
                }
                println!("Job ID: {}", job.id());
            }
        }
        None => {
            if cfg.json_output {
                print_pretty_json(&serde_json::json!({
                    "error": format!("job not found: {job_id}"),
                    "job_id": job_id
                }))?;
            } else {
                println!(
                    "{} {}",
                    symbol_for_status("error"),
                    muted(&format!("job not found: {job_id}"))
                );
            }
        }
    }
    Ok(())
}

pub fn handle_job_cancel(
    cfg: &Config,
    id: Uuid,
    canceled: bool,
    command_name: &str,
) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        let resp = JobCancelResponse::new(id, canceled);
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else if canceled {
        println!(
            "{} canceled {command_name} job {}",
            symbol_for_status("canceled"),
            accent(&id.to_string())
        );
        println!("Job ID: {id}");
    } else {
        println!(
            "{} no cancellable {command_name} job found for {}",
            symbol_for_status("error"),
            accent(&id.to_string())
        );
        println!("Job ID: {id}");
    }
    Ok(())
}

pub fn handle_job_errors<T: JobStatus>(
    cfg: &Config,
    job: Option<T>,
    id: Uuid,
    command_name: &str,
) -> Result<(), Box<dyn Error>> {
    match job {
        Some(job) => {
            if cfg.json_output {
                let contract = job.to_errors_response_json();
                println!("{}", serde_json::to_string_pretty(&contract)?);
            } else {
                println!(
                    "{} {} job {} {}",
                    symbol_for_status(job.status()),
                    command_name,
                    accent(&id.to_string()),
                    status_text(job.status())
                );
                println!(
                    "  {} {}",
                    muted("Error:"),
                    job.error_text().unwrap_or("None")
                );
                println!("Job ID: {id}");
            }
        }
        None => {
            if cfg.json_output {
                print_pretty_json(&serde_json::json!({
                    "error": format!("job not found: {id}"),
                    "job_id": id
                }))?;
            } else {
                println!(
                    "{} {}",
                    symbol_for_status("error"),
                    muted(&format!("job not found: {id}"))
                );
            }
        }
    }
    Ok(())
}

pub fn handle_job_list<T: JobStatus + Clone>(
    cfg: &Config,
    result: &axon_services::types::JobListResult<T>,
    command_name: &str,
) -> Result<(), Box<dyn Error>> {
    handle_job_list_with_rows(
        cfg,
        result,
        command_name,
        None,
        &["", "ID", "Status"],
        |job| {
            vec![
                symbol_for_status(job.status()),
                job.id().to_string(),
                status_text(job.status()),
            ]
        },
    )
}

pub fn handle_job_list_with_rows<T, F>(
    cfg: &Config,
    result: &axon_services::types::JobListResult<T>,
    command_name: &str,
    empty_message: Option<&str>,
    headers: &[&str],
    row: F,
) -> Result<(), Box<dyn Error>>
where
    T: JobStatus + Clone,
    F: Fn(&T) -> Vec<String>,
{
    let jobs = filter_jobs_for_status_view(cfg, result.jobs.clone());
    if cfg.json_output {
        let entries: Vec<Value> = jobs.iter().map(|j| j.to_summary_entry_json()).collect();
        let out = serde_json::json!({
            "jobs": entries,
            "total": result.total,
            "limit": result.limit,
            "offset": result.offset,
            "truncated": result.is_truncated(),
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    println!("{}", primary(&format!("{command_name} Jobs")));
    if jobs.is_empty() {
        let message = empty_message
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("No {command_name} jobs found."));
        println!("  {}", muted(&message));
    } else {
        axon_core::ui::print_aurora_table(headers, jobs.iter().map(row));
    }

    crate::commands::common::print_list_footer(
        jobs.len(),
        result.total,
        result.limit,
        result.offset,
    );
    Ok(())
}

pub fn handle_job_cleanup(
    cfg: &Config,
    removed: u64,
    command_name: &str,
) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        print_pretty_json(&serde_json::json!({ "removed": removed }))?;
    } else {
        println!(
            "{} removed {} {command_name} jobs",
            symbol_for_status("completed"),
            removed
        );
    }
    Ok(())
}

pub fn handle_job_clear(
    cfg: &Config,
    removed: u64,
    command_name: &str,
) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        print_pretty_json(&serde_json::json!({ "removed": removed }))?;
    } else {
        println!(
            "{} cleared {} {command_name} jobs and attempted queue purge",
            symbol_for_status("completed"),
            removed
        );
    }
    Ok(())
}

pub fn handle_job_recover(
    cfg: &Config,
    reclaimed: u64,
    command_name: &str,
) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        print_pretty_json(&serde_json::json!({ "reclaimed": reclaimed }))?;
    } else {
        println!(
            "{} reclaimed {} stale {command_name} jobs",
            symbol_for_status("completed"),
            reclaimed
        );
    }
    Ok(())
}

/// Handle the result of `job_service::start_worker(cfg, kind).await?`.
///
/// Prints a message in the SQLite runtime (workers are in-process) and propagates
/// any `Unsupported` error. Extracted to eliminate the identical 5-arm match
/// block that appears in every command's `"worker"` subcommand handler.
pub fn handle_worker_mode(mode: WorkerMode) -> Result<(), Box<dyn Error>> {
    match mode {
        WorkerMode::InProcess {
            pending_at_start,
            elapsed_secs,
        } => {
            println!(
                "{}",
                muted(&format!(
                    "SQLite runtime: queue drained — {pending_at_start} pending at start, {elapsed_secs}s elapsed."
                ))
            );
        }
        WorkerMode::Started => {}
        WorkerMode::Unsupported(message) => {
            println!("{}", muted(message));
        }
    }
    Ok(())
}
