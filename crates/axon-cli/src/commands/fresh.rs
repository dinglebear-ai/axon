use axon_core::config::{Config, FreshAction};
use axon_core::ui::{accent, muted, primary};
use axon_services::context::ServiceContext;
use axon_services::freshness as freshness_service;
use std::error::Error;
use uuid::Uuid;

pub async fn run_fresh(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    match fresh_action(cfg)? {
        FreshAction::List { json } => run_list(json, cfg, service_context).await,
        FreshAction::RunNow { id, json } => run_now(id, json, cfg, service_context).await,
        FreshAction::History { id, limit, json } => {
            run_history(id, limit, json, cfg, service_context).await
        }
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

async fn run_list(
    json: bool,
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let schedules = freshness_service::list(service_context, 200)
        .await
        .map_err(|err| -> Box<dyn Error> { err })?;
    if wants_json(json, cfg) {
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

async fn run_now(
    id: Uuid,
    json: bool,
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let run = freshness_service::run_now(service_context, id)
        .await
        .map_err(|err| -> Box<dyn Error> { err })?;
    if wants_json(json, cfg) {
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

async fn run_history(
    id: Uuid,
    limit: usize,
    json: bool,
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let runs = freshness_service::history(service_context, id, limit as i64)
        .await
        .map_err(|err| -> Box<dyn Error> { err })?;
    if wants_json(json, cfg) {
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

fn wants_json(action_json: bool, cfg: &Config) -> bool {
    action_json || cfg.json_output
}

fn fresh_action(cfg: &Config) -> Result<FreshAction, Box<dyn Error>> {
    if let Some(action) = &cfg.fresh_action {
        return Ok(action.clone());
    }
    match cfg.positional.first().map(String::as_str) {
        None | Some("list") => Ok(FreshAction::List {
            json: cfg.positional.iter().any(|arg| arg == "--json"),
        }),
        Some("run-now") => Ok(FreshAction::RunNow {
            id: parse_id(cfg.positional.get(1), "run-now")?,
            json: cfg.positional.iter().any(|arg| arg == "--json"),
        }),
        Some("history") => Ok(FreshAction::History {
            id: parse_id(cfg.positional.get(1), "history")?,
            limit: parse_limit(&cfg.positional).unwrap_or(50),
            json: cfg.positional.iter().any(|arg| arg == "--json"),
        }),
        Some(other) => Err(format!(
            "unknown fresh subcommand {other:?}; expected list, run-now, or history"
        )
        .into()),
    }
}

fn parse_id(raw: Option<&String>, action: &str) -> Result<Uuid, Box<dyn Error>> {
    let id = raw.ok_or_else(|| format!("fresh {action} requires <id>"))?;
    Ok(Uuid::parse_str(id)?)
}

fn parse_limit(positional: &[String]) -> Option<usize> {
    positional
        .windows(2)
        .find(|pair| pair[0] == "--limit")
        .and_then(|pair| pair[1].parse::<usize>().ok())
}

#[cfg(test)]
#[path = "fresh_tests.rs"]
mod tests;
