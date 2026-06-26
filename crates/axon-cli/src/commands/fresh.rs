use axon_core::config::Config;
use axon_core::ui::{accent, muted, primary};
use axon_services::context::ServiceContext;
use axon_services::freshness as freshness_service;
use std::error::Error;
use uuid::Uuid;

pub async fn run_fresh(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    match cfg.positional.first().map(String::as_str) {
        None | Some("list") => run_list(cfg, service_context).await,
        Some("run-now") => run_now(cfg, service_context).await,
        Some("history") => run_history(cfg, service_context).await,
        Some(other) => Err(format!(
            "unknown fresh subcommand {other:?}; expected list, run-now, or history"
        )
        .into()),
    }
}

pub(crate) async fn create_schedule_from_command(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let schedule = freshness_service::create_from_config(cfg, service_context)
        .await
        .map_err(|err| -> Box<dyn Error> { err })?;
    if cfg.wait {
        let run = freshness_service::run_now(service_context, schedule.id)
            .await
            .map_err(|err| -> Box<dyn Error> { err })?;
        if cfg.json_output {
            let mut value = serde_json::to_value(&schedule)?;
            if let Some(obj) = value.as_object_mut() {
                obj.insert("run".to_string(), serde_json::to_value(run)?);
            }
            println!("{}", serde_json::to_string_pretty(&value)?);
        } else {
            render_created(&schedule);
            println!("  {} {}", primary("Run"), accent(&run.status));
        }
        return Ok(());
    }

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&schedule)?);
    } else {
        render_created(&schedule);
    }
    Ok(())
}

async fn run_list(cfg: &Config, service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let schedules = freshness_service::list(service_context, 200)
        .await
        .map_err(|err| -> Box<dyn Error> { err })?;
    if wants_json(cfg) {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "items": schedules }))?
        );
    } else {
        println!("{}", primary("Freshness Schedules"));
        if schedules.is_empty() {
            println!("  {}", muted("No freshness schedules defined."));
        } else {
            for item in &schedules {
                println!(
                    "  {} {} {} every {}s next {}",
                    item.id,
                    accent(&item.command),
                    item.target,
                    item.every_seconds,
                    item.next_run_at
                );
            }
        }
        println!("  {} total", schedules.len());
    }
    Ok(())
}

async fn run_now(cfg: &Config, service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let id = parse_id(cfg.positional.get(1), "run-now")?;
    let run = freshness_service::run_now(service_context, id)
        .await
        .map_err(|err| -> Box<dyn Error> { err })?;
    if wants_json(cfg) {
        println!("{}", serde_json::to_string_pretty(&run)?);
    } else {
        println!(
            "{} {}",
            primary("Freshness Run"),
            accent(&run.id.to_string())
        );
        println!("  {} {}", primary("Status:"), run.status);
    }
    Ok(())
}

async fn run_history(cfg: &Config, service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let id = parse_id(cfg.positional.get(1), "history")?;
    let limit = parse_limit(&cfg.positional).unwrap_or(50);
    let runs = freshness_service::history(service_context, id, limit)
        .await
        .map_err(|err| -> Box<dyn Error> { err })?;
    if wants_json(cfg) {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "items": runs }))?
        );
    } else {
        println!("{}", primary("Freshness History"));
        if runs.is_empty() {
            println!("  {}", muted("No freshness runs found."));
        } else {
            for run in &runs {
                println!("  {} {} {}", run.id, accent(&run.status), run.created_at);
            }
        }
        println!("  {} total", runs.len());
    }
    Ok(())
}

fn render_created(schedule: &freshness_service::FreshnessCreated) {
    println!(
        "{} {}",
        primary("Freshness Schedule"),
        accent(&schedule.id.to_string())
    );
    println!("  {} {}", primary("Command:"), schedule.command);
    println!("  {} {}", primary("Target:"), schedule.target);
    println!(
        "  {} every {}s",
        primary("Interval:"),
        schedule.every_seconds
    );
    println!("  {} {}", primary("Next run:"), schedule.next_run_at);
}

fn wants_json(cfg: &Config) -> bool {
    cfg.json_output || cfg.positional.iter().any(|arg| arg == "--json")
}

fn parse_id(raw: Option<&String>, action: &str) -> Result<Uuid, Box<dyn Error>> {
    let id = raw.ok_or_else(|| format!("fresh {action} requires <id>"))?;
    Ok(Uuid::parse_str(id)?)
}

fn parse_limit(positional: &[String]) -> Option<i64> {
    positional
        .windows(2)
        .find(|pair| pair[0] == "--limit")
        .and_then(|pair| pair[1].parse::<i64>().ok())
}

#[cfg(test)]
#[path = "fresh_tests.rs"]
mod tests;
