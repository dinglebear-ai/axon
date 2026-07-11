use axon_api::source::{
    JobCancelRequest, JobCleanupRequest, JobClearRequest, JobEventListRequest, JobId,
    JobListRequest, JobRecoveryRequest, JobRetryMode, JobRetryRequest, MetadataMap, Timestamp,
};
#[cfg(test)]
use axon_api::source::{JobKind, LifecycleStatus};
use axon_core::config::Config;
use axon_core::ui::{accent, muted, primary, status_text, symbol_for_status};
use axon_services::context::ServiceContext;
use std::error::Error;
use uuid::Uuid;

pub async fn run_jobs(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    match cfg.positional.first().map(String::as_str).unwrap_or("list") {
        "list" => list_jobs(cfg, service_context).await,
        "get" | "status" => get_job(cfg, service_context).await,
        "events" | "stream" => job_events(cfg, service_context).await,
        "cancel" => cancel_job(cfg, service_context).await,
        "retry" => retry_job(cfg, service_context).await,
        "recover" => recover_jobs(cfg, service_context).await,
        "cleanup" => cleanup_jobs(cfg, service_context).await,
        "clear" => clear_jobs(cfg, service_context).await,
        other => Err(format!("unknown jobs subcommand: {other}").into()),
    }
}

async fn list_jobs(cfg: &Config, service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let page = axon_services::jobs::list_unified_jobs(
        service_context,
        JobListRequest {
            status: parse_opt_flag(cfg, "--status")?,
            kind: parse_opt_flag(cfg, "--kind")?,
            source_id: None,
            watch_id: None,
            limit: parse_u32_flag(cfg, "--limit")?,
            cursor: flag_value(cfg, "--cursor"),
        },
    )
    .await
    .map_err(|error| Box::<dyn Error>::from(error.to_string()))?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&page)?);
        return Ok(());
    }

    println!("{}", primary("Unified Jobs"));
    if page.items.is_empty() {
        println!("  {}", muted("No unified jobs found."));
    } else {
        axon_core::ui::print_aurora_table(
            &["", "ID", "Kind", "Status", "Phase", "Updated"],
            page.items.iter().map(|job| {
                let status = enum_wire(job.status);
                vec![
                    symbol_for_status(&status),
                    job.job_id.0.to_string(),
                    enum_wire(job.kind),
                    status_text(&status),
                    enum_wire(job.phase),
                    job.updated_at.0.clone(),
                ]
            }),
        );
    }
    if let Some(cursor) = page.next_cursor.as_deref() {
        println!("{} {}", muted("Next cursor:"), cursor);
    }
    Ok(())
}

async fn get_job(cfg: &Config, service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let job_id = positional_job_id(cfg, 1)?;
    let job = axon_services::jobs::unified_job_status(service_context, job_id)
        .await
        .map_err(|error| Box::<dyn Error>::from(error.to_string()))?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&job)?);
        return Ok(());
    }

    let Some(job) = job else {
        println!(
            "{} {}",
            symbol_for_status("error"),
            muted(&format!("job not found: {}", job_id.0))
        );
        return Ok(());
    };
    println!(
        "{} {}",
        primary("Unified Job"),
        accent(&job.job_id.0.to_string())
    );
    println!("  {} {}", muted("Kind:"), enum_wire(job.kind));
    println!("  {} {}", muted("Status:"), enum_wire(job.status));
    println!("  {} {}", muted("Phase:"), enum_wire(job.phase));
    println!("  {} {}", muted("Updated:"), job.updated_at.0);
    if let Some(error) = job.last_error {
        println!("  {} {}", muted("Error:"), error.message);
    }
    println!("Job ID: {}", job.job_id.0);
    Ok(())
}

async fn job_events(cfg: &Config, service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let job_id = positional_job_id(cfg, 1)?;
    let page = axon_services::jobs::unified_job_events(
        service_context,
        JobEventListRequest {
            job_id,
            after_sequence: parse_u64_flag(cfg, "--after-sequence")?,
            limit: parse_u32_flag(cfg, "--limit")?,
            severity: None,
            visibility: None,
            phase: None,
            since_sequence: None,
            cursor: flag_value(cfg, "--cursor"),
        },
    )
    .await
    .map_err(|error| Box::<dyn Error>::from(error.to_string()))?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&page)?);
        return Ok(());
    }
    println!(
        "{} {}",
        primary("Unified Job Events"),
        accent(&job_id.0.to_string())
    );
    for event in &page.events {
        println!(
            "  #{} {} {} {}",
            event.sequence,
            enum_wire(event.phase),
            enum_wire(event.status),
            event.message
        );
    }
    println!("{} {}", muted("Last sequence:"), page.last_sequence);
    if let Some(cursor) = page.next_cursor.as_deref() {
        println!("{} {}", muted("Next cursor:"), cursor);
    }
    Ok(())
}

async fn cancel_job(cfg: &Config, service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let job_id = positional_job_id(cfg, 1)?;
    let result = axon_services::jobs::cancel_unified_job(
        service_context,
        job_id,
        JobCancelRequest {
            reason: flag_value(cfg, "--reason"),
            force_after_ms: None,
            actor: None,
        },
    )
    .await
    .map_err(|error| Box::<dyn Error>::from(error.to_string()))?;
    render_value_or_line(cfg, &result, "cancel requested")
}

async fn retry_job(cfg: &Config, service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let job_id = positional_job_id(cfg, 1)?;
    let result = axon_services::jobs::retry_unified_job(
        service_context,
        job_id,
        JobRetryRequest {
            mode: parse_retry_mode(
                flag_value(cfg, "--mode")
                    .as_deref()
                    .unwrap_or("same_config"),
            )?,
            from_phase: None,
            idempotency_key: None,
            overrides: MetadataMap::new(),
        },
    )
    .await
    .map_err(|error| Box::<dyn Error>::from(error.to_string()))?;
    render_value_or_line(cfg, &result, "retry queued")
}

async fn recover_jobs(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let result = axon_services::jobs::recover_unified_jobs(
        service_context,
        JobRecoveryRequest {
            kind: parse_opt_flag(cfg, "--kind")?,
            stale_before: parse_timestamp_flag(cfg, "--stale-before")?,
            limit: parse_u32_flag(cfg, "--limit")?,
            older_than_seconds: None,
            dry_run: false,
            allow_without_cutoff: false,
        },
    )
    .await
    .map_err(|error| Box::<dyn Error>::from(error.to_string()))?;
    render_value_or_line(cfg, &result, "recovery complete")
}

fn parse_timestamp_flag(cfg: &Config, name: &str) -> Result<Option<Timestamp>, Box<dyn Error>> {
    let Some(value) = flag_value(cfg, name) else {
        return Ok(None);
    };
    let parsed = chrono::DateTime::parse_from_rfc3339(&value)
        .map_err(|error| format!("{name} must be RFC3339 timestamp: {error}"))?;
    Ok(Some(Timestamp::from(parsed.with_timezone(&chrono::Utc))))
}

async fn cleanup_jobs(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let result = axon_services::jobs::cleanup_unified_jobs(
        service_context,
        JobCleanupRequest {
            dry_run: flag_present(cfg, "--dry-run"),
            kind: parse_opt_flag(cfg, "--kind")?,
            older_than: parse_timestamp_flag(cfg, "--older-than")?,
            status: parse_opt_flag(cfg, "--status")?,
            limit: parse_u32_flag(cfg, "--limit")?,
            older_than_seconds: None,
            confirm_all_terminal: true,
        },
    )
    .await
    .map_err(|error| Box::<dyn Error>::from(error.to_string()))?;
    render_value_or_line(cfg, &result, "cleanup complete")
}

async fn clear_jobs(cfg: &Config, service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    if !flag_present(cfg, "--confirm") {
        return Err("jobs clear requires --confirm".into());
    }
    let result = axon_services::jobs::clear_unified_jobs(
        service_context,
        JobClearRequest {
            status: parse_opt_flag(cfg, "--status")?,
            confirm: true,
            kind: parse_opt_flag(cfg, "--kind")?,
            older_than: None,
        },
    )
    .await
    .map_err(|error| Box::<dyn Error>::from(error.to_string()))?;
    render_value_or_line(cfg, &result, "cleared unified terminal jobs")
}

fn render_value_or_line<T: serde::Serialize>(
    cfg: &Config,
    value: &T,
    message: &str,
) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        println!("{} {message}", symbol_for_status("completed"));
    }
    Ok(())
}

fn positional_job_id(cfg: &Config, index: usize) -> Result<JobId, Box<dyn Error>> {
    let id = cfg
        .positional
        .get(index)
        .ok_or_else(|| "job_id is required".to_string())?;
    Ok(JobId::new(Uuid::parse_str(id)?))
}

fn flag_value(cfg: &Config, name: &str) -> Option<String> {
    cfg.positional
        .windows(2)
        .find_map(|window| (window[0] == name).then(|| window[1].clone()))
}

fn flag_present(cfg: &Config, name: &str) -> bool {
    cfg.positional.iter().any(|arg| arg == name)
}

fn parse_u32_flag(cfg: &Config, name: &str) -> Result<Option<u32>, Box<dyn Error>> {
    flag_value(cfg, name)
        .map(|value| value.parse::<u32>())
        .transpose()
        .map_err(Into::into)
}

fn parse_u64_flag(cfg: &Config, name: &str) -> Result<Option<u64>, Box<dyn Error>> {
    flag_value(cfg, name)
        .map(|value| value.parse::<u64>())
        .transpose()
        .map_err(Into::into)
}

fn parse_opt_flag<T>(cfg: &Config, name: &str) -> Result<Option<T>, Box<dyn Error>>
where
    T: serde::de::DeserializeOwned,
{
    flag_value(cfg, name)
        .map(|value| serde_json::from_value(serde_json::Value::String(value)))
        .transpose()
        .map_err(Into::into)
}

fn parse_retry_mode(value: &str) -> Result<JobRetryMode, Box<dyn Error>> {
    serde_json::from_value(serde_json::Value::String(value.to_string())).map_err(Into::into)
}

fn enum_wire<T: serde::Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_mode_accepts_wire_names() {
        assert!(matches!(
            parse_retry_mode("same_config").expect("same_config"),
            JobRetryMode::SameConfig
        ));
        assert!(matches!(
            parse_retry_mode("with_overrides").expect("with_overrides"),
            JobRetryMode::WithOverrides
        ));
    }

    #[test]
    fn job_filters_parse_wire_enums() {
        let cfg = Config {
            positional: vec![
                "list".to_string(),
                "--status".to_string(),
                "completed_degraded".to_string(),
                "--kind".to_string(),
                "provider_probe".to_string(),
            ],
            ..Config::default()
        };
        assert!(matches!(
            parse_opt_flag::<LifecycleStatus>(&cfg, "--status").expect("status"),
            Some(LifecycleStatus::CompletedDegraded)
        ));
        assert!(matches!(
            parse_opt_flag::<JobKind>(&cfg, "--kind").expect("kind"),
            Some(JobKind::ProviderProbe)
        ));
    }
}
