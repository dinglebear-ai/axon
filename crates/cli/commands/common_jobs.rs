use crate::crates::cli::commands::job_contracts::{
    JobCancelResponse, JobErrorsResponse, JobStatusResponse, JobSummaryEntry,
};
use crate::crates::core::config::Config;
use crate::crates::core::ui::{accent, muted, primary, status_text, symbol_for_status};
use crate::crates::services::runtime::WorkerMode;
use chrono::{DateTime, Utc};
use serde::Serialize;
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
                serde_json::to_value($status_ctor(self)).unwrap_or_default()
            }
            fn to_summary_entry_json(&self) -> Value {
                serde_json::to_value($summary_ctor(self)).unwrap_or_default()
            }
            fn to_errors_response_json(&self) -> Value {
                serde_json::to_value(JobErrorsResponse::from_job(
                    self.id,
                    self.status.clone(),
                    self.error_text.clone(),
                ))
                .unwrap_or_default()
            }
        }
    };
}

impl_job_status!(
    crate::crates::jobs::crawl::CrawlJob,
    JobStatusResponse::from_crawl,
    JobSummaryEntry::from_crawl
);
impl_job_status!(
    crate::crates::jobs::extract::ExtractJob,
    JobStatusResponse::from_extract,
    JobSummaryEntry::from_extract
);
impl_job_status!(
    crate::crates::jobs::ingest::IngestJob,
    JobStatusResponse::from_ingest,
    JobSummaryEntry::from_ingest
);
impl_job_status!(
    crate::crates::jobs::embed::EmbedJob,
    JobStatusResponse::from_embed,
    JobSummaryEntry::from_embed
);
impl_job_status!(
    crate::crates::jobs::refresh::RefreshJob,
    JobStatusResponse::from_refresh,
    JobSummaryEntry::from_refresh
);
impl_job_status!(
    crate::crates::services::types::ServiceJob,
    JobStatusResponse::from_service_job,
    JobSummaryEntry::from_service_job
);

fn print_pretty_json(value: &Value) -> Result<(), Box<dyn Error>> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

pub fn handle_job_status<T: JobStatus + Serialize>(
    cfg: &Config,
    job: Option<T>,
    job_id: Uuid,
    command_name: &str,
) -> Result<(), Box<dyn Error>> {
    match job {
        Some(job) => {
            if cfg.json_output {
                let json = job.to_status_response_json();
                println!("{}", serde_json::to_string_pretty(&json)?);
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

pub fn handle_job_errors<T: JobStatus + Serialize>(
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

pub fn handle_job_list<T: JobStatus + Serialize>(
    cfg: &Config,
    jobs: Vec<T>,
    command_name: &str,
) -> Result<(), Box<dyn Error>> {
    let jobs = filter_jobs_for_status_view(cfg, jobs);
    if cfg.json_output {
        let entries: Vec<Value> = jobs.iter().map(|j| j.to_summary_entry_json()).collect();
        println!("{}", serde_json::to_string_pretty(&entries)?);
        return Ok(());
    }

    println!("{}", primary(&format!("{command_name} Jobs")));
    if jobs.is_empty() {
        println!("  {}", muted(&format!("No {command_name} jobs found.")));
        return Ok(());
    }

    for job in jobs {
        println!(
            "  {} {} {}",
            symbol_for_status(job.status()),
            accent(&job.id().to_string()),
            status_text(job.status())
        );
    }
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

/// Handle the result of `job_service::run_worker(cfg, kind).await?`.
///
/// Prints a message in lite mode (workers are in-process) and propagates
/// any `Unsupported` error. Extracted to eliminate the identical 5-arm match
/// block that appears in every command's `"worker"` subcommand handler.
pub fn handle_worker_mode(mode: WorkerMode) -> Result<(), Box<dyn Error>> {
    match mode {
        WorkerMode::InProcess => {
            println!("Lite mode: workers run in-process automatically. No separate worker needed.")
        }
        WorkerMode::Started => {}
        WorkerMode::Unsupported(message) => return Err(message.into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::JobStatus;
    use crate::crates::jobs::embed::EmbedJob;
    use crate::crates::jobs::refresh::RefreshJob;
    use chrono::{DateTime, TimeZone, Utc};
    use std::error::Error;
    use uuid::Uuid;

    fn test_ts() -> Result<DateTime<Utc>, Box<dyn Error>> {
        Utc.with_ymd_and_hms(2026, 3, 15, 12, 0, 0)
            .single()
            .ok_or_else(|| "valid timestamp".into())
    }

    fn assert_job_status_trait<T: JobStatus>(
        job: &T,
        expected_status: &str,
    ) -> Result<(), Box<dyn Error>> {
        assert_eq!(job.status(), expected_status);
        assert_eq!(job.updated_at(), test_ts()?);
        Ok(())
    }

    #[test]
    fn embed_job_implements_shared_job_status_trait() -> Result<(), Box<dyn Error>> {
        let job = EmbedJob {
            id: Uuid::parse_str("66666666-6666-6666-6666-666666666666")?,
            status: "running".to_string(),
            created_at: test_ts()?,
            updated_at: test_ts()?,
            started_at: Some(test_ts()?),
            finished_at: None,
            error_text: None,
            input_text: "/tmp/embed-input".to_string(),
            result_json: Some(serde_json::json!({"chunks_embedded": 3})),
            config_json: serde_json::json!({"collection": "cortex"}),
        };

        assert_job_status_trait(&job, "running")
    }

    #[test]
    fn refresh_job_implements_shared_job_status_trait() -> Result<(), Box<dyn Error>> {
        let job = RefreshJob {
            id: Uuid::parse_str("77777777-7777-7777-7777-777777777777")?,
            status: "completed".to_string(),
            created_at: test_ts()?,
            updated_at: test_ts()?,
            started_at: Some(test_ts()?),
            finished_at: Some(test_ts()?),
            error_text: None,
            urls_json: serde_json::json!(["https://example.com"]),
            result_json: Some(serde_json::json!({"checked": 1})),
            config_json: serde_json::json!({"embed": true}),
        };

        assert_job_status_trait(&job, "completed")
    }
}
