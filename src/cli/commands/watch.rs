use crate::core::config::Config;
use crate::core::ui::{muted, primary};
use crate::services::context::ServiceContext;
use crate::services::watch as watch_svc;
use chrono::{Duration, Utc};
use clap::{Parser, Subcommand};
use std::error::Error;
use uuid::Uuid;

#[derive(Debug, Parser)]
struct WatchRuntimeArgs {
    #[command(subcommand)]
    action: Option<WatchRuntimeSubcommand>,
}

#[derive(Debug, Subcommand)]
enum WatchRuntimeSubcommand {
    Create {
        name: String,
        #[arg(long = "task-type")]
        task_type: String,
        #[arg(long = "every-seconds")]
        every_seconds: i64,
        #[arg(long = "task-payload")]
        task_payload: Option<String>,
    },
    List,
    Get {
        id: String,
    },
    Update {
        id: String,
        #[arg(long = "every-seconds")]
        every_seconds: Option<i64>,
    },
    #[command(name = "run-now")]
    RunNow {
        id: String,
    },
    Pause {
        id: String,
    },
    Resume {
        id: String,
    },
    Delete {
        id: String,
    },
    History {
        id: String,
        #[arg(long, default_value_t = 50)]
        limit: i64,
    },
    Artifacts {
        run_id: String,
        #[arg(long, default_value_t = 50)]
        limit: i64,
    },
}

fn parse_watch_runtime_args(args: &[String]) -> Result<WatchRuntimeArgs, Box<dyn Error>> {
    WatchRuntimeArgs::try_parse_from(
        std::iter::once("watch").chain(args.iter().map(String::as_str)),
    )
    .map_err(|err| err.to_string().into())
}

fn parse_uuid(raw: Option<&String>, action: &str) -> Result<Uuid, Box<dyn Error>> {
    let id = raw.ok_or_else(|| format!("watch {action} requires <id>"))?;
    Ok(Uuid::parse_str(id)?)
}

pub async fn run_watch(
    cfg: &Config,
    _service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let parsed = parse_watch_runtime_args(&cfg.positional)?;
    let subcmd = parsed.action.unwrap_or(WatchRuntimeSubcommand::List);
    match subcmd {
        WatchRuntimeSubcommand::Create {
            name,
            task_type,
            every_seconds,
            task_payload,
        } => handle_watch_create(cfg, name, task_type, every_seconds, task_payload).await?,
        WatchRuntimeSubcommand::List => {
            let watches = watch_svc::list_watch_defs(cfg, 200).await?;
            if cfg.json_output {
                println!("{}", serde_json::to_string_pretty(&watches)?);
            } else {
                println!("{}", primary("Watch Definitions"));
                if watches.is_empty() {
                    println!("  {}", muted("No watches defined."));
                } else {
                    for w in &watches {
                        println!("  {} {} {}", w.id, w.task_type, w.name);
                    }
                }
                println!("  {} total", watches.len());
            }
        }
        WatchRuntimeSubcommand::RunNow { id } => handle_watch_run_now(cfg, &id).await?,
        WatchRuntimeSubcommand::History { id, limit } => {
            let watch_id = parse_uuid(Some(&id), "history")?;
            let runs = watch_svc::list_watch_runs(cfg, watch_id, limit).await?;
            println!("{}", serde_json::to_string_pretty(&runs)?);
        }
        WatchRuntimeSubcommand::Get { .. } => {
            return Err(
                "'axon watch get' is not yet implemented — use 'axon watch list' to view all watches".into()
            );
        }
        WatchRuntimeSubcommand::Update { .. } => {
            return Err(
                "'axon watch update' is not yet implemented — cancel and re-create the watch with new settings".into()
            );
        }
        WatchRuntimeSubcommand::Pause { .. } => {
            return Err(
                "'axon watch pause' is not yet implemented — delete and re-create with 'axon watch create'".into()
            );
        }
        WatchRuntimeSubcommand::Resume { .. } => {
            return Err(
                "'axon watch resume' is not yet implemented — delete and re-create with 'axon watch create'".into()
            );
        }
        WatchRuntimeSubcommand::Delete { .. } => {
            return Err(
                "'axon watch delete' is not yet implemented — once implemented, \
                 use 'axon watch delete <id>' to safely remove a watch definition. \
                 Direct Postgres manipulation (DELETE FROM axon_watch_definitions) is a last resort \
                 and requires schema knowledge; ensure no running jobs reference the definition \
                 before deleting, and only do this if you understand the table relationships.".into()
            );
        }
        WatchRuntimeSubcommand::Artifacts { .. } => {
            return Err(
                "'axon watch artifacts' is not yet implemented — use 'axon watch history <id>' to view run history".into()
            );
        }
    }
    Ok(())
}

async fn handle_watch_create(
    cfg: &Config,
    name: String,
    task_type: String,
    every_seconds: i64,
    task_payload_raw: Option<String>,
) -> Result<(), Box<dyn Error>> {
    if every_seconds < 1 {
        return Err(
            format!("watch create: --every-seconds must be >= 1, got {every_seconds}").into(),
        );
    }
    let task_payload = match task_payload_raw {
        Some(raw) => Some(serde_json::from_str(&raw).map_err(|e| {
            format!("watch create: --task-payload is not valid JSON: {e} (got '{raw}')")
        })?),
        None => None,
    };
    let created = watch_svc::create_watch_def(
        cfg,
        &watch_svc::WatchDefCreate {
            name,
            task_type,
            task_payload: task_payload.unwrap_or_else(|| serde_json::json!({})),
            every_seconds,
            enabled: true,
            next_run_at: Utc::now() + Duration::seconds(every_seconds),
        },
    )
    .await?;
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&created)?);
    } else {
        println!("created watch {} ({})", created.name, created.id);
    }
    Ok(())
}

async fn handle_watch_run_now(cfg: &Config, raw_id: &str) -> Result<(), Box<dyn Error>> {
    let watch_id = parse_uuid(Some(&raw_id.to_string()), "run-now")?;
    let watch = watch_svc::get_watch_def(cfg, watch_id)
        .await?
        .ok_or("watch not found")?;
    let run = watch_svc::run_watch_now(cfg, &watch).await?;
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&run)?);
    } else {
        println!("watch run {} status={}", run.id, run.status);
    }
    Ok(())
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
