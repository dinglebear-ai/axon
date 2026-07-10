use axon_core::config::Config;
use axon_core::ui::{muted, primary};
use axon_services::context::ServiceContext;
use axon_services::watch as watch_svc;
use clap::{Parser, Subcommand};
use sqlx::SqlitePool;
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
        #[arg(long = "collection")]
        collection: Option<String>,
    },
    #[command(name = "exec")]
    Exec {
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
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let parsed = parse_watch_runtime_args(&cfg.positional)?;
    let subcmd = parsed.action.unwrap_or(WatchRuntimeSubcommand::List);
    let shared_pool = service_context.jobs.sqlite_pool();
    match subcmd {
        WatchRuntimeSubcommand::Create {
            name,
            task_type,
            every_seconds,
            task_payload,
        } => {
            handle_watch_create(
                cfg,
                shared_pool.as_deref(),
                name,
                task_type,
                every_seconds,
                task_payload,
            )
            .await?
        }
        WatchRuntimeSubcommand::List => {
            let watches = match shared_pool.as_deref() {
                Some(pool) => watch_svc::list_watch_defs_with_pool(pool, 200).await?,
                None => watch_svc::list_watch_defs(cfg, 200).await?,
            };
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
        WatchRuntimeSubcommand::Exec { id } => {
            handle_watch_exec(cfg, shared_pool.as_deref(), &id).await?
        }
        WatchRuntimeSubcommand::History { id, limit } => {
            let watch_id = parse_uuid(Some(&id), "history")?;
            let runs = match shared_pool.as_deref() {
                Some(pool) => watch_svc::list_watch_runs_with_pool(pool, watch_id, limit).await?,
                None => watch_svc::list_watch_runs(cfg, watch_id, limit).await?,
            };
            crate::json::print_json_gated(&runs)?;
        }
        WatchRuntimeSubcommand::Get { id } => {
            handle_watch_get(cfg, shared_pool.as_deref(), &id).await?
        }
        WatchRuntimeSubcommand::Update {
            id,
            every_seconds,
            collection,
        } => {
            let request = watch_svc::WatchUpdateRequest {
                enabled: None,
                schedule: every_seconds.map(|every_seconds| axon_api::source::WatchSchedule {
                    every_seconds: every_seconds.max(0) as u64,
                    cron: None,
                    timezone: None,
                }),
                options: None,
                embed: None,
                collection,
                scope: None,
            };
            handle_watch_update(cfg, shared_pool.as_deref(), &id, request).await?
        }
        WatchRuntimeSubcommand::Pause { id } => {
            let request = watch_svc::WatchUpdateRequest {
                enabled: Some(false),
                schedule: None,
                options: None,
                embed: None,
                collection: None,
                scope: None,
            };
            handle_watch_update(cfg, shared_pool.as_deref(), &id, request).await?
        }
        WatchRuntimeSubcommand::Resume { id } => {
            let request = watch_svc::WatchUpdateRequest {
                enabled: Some(true),
                schedule: None,
                options: None,
                embed: None,
                collection: None,
                scope: None,
            };
            handle_watch_update(cfg, shared_pool.as_deref(), &id, request).await?
        }
        WatchRuntimeSubcommand::Delete { id } => {
            handle_watch_delete(cfg, shared_pool.as_deref(), &id).await?
        }
        WatchRuntimeSubcommand::Artifacts { run_id, limit } => {
            handle_watch_artifacts(cfg, shared_pool.as_deref(), &run_id, limit).await?
        }
    }
    Ok(())
}

async fn handle_watch_artifacts(
    cfg: &Config,
    pool: Option<&SqlitePool>,
    run_id: &str,
    limit: i64,
) -> Result<(), Box<dyn Error>> {
    let run_id = parse_uuid(Some(&run_id.to_string()), "artifacts")?;
    let artifacts = match pool {
        Some(pool) => watch_svc::list_watch_run_artifacts_with_pool(pool, run_id, limit).await?,
        None => watch_svc::list_watch_run_artifacts(cfg, run_id, limit).await?,
    };
    if cfg.json_output {
        crate::json::print_json_gated(&artifacts)?;
    } else {
        println!("{}", primary("Watch Artifacts"));
        if artifacts.is_empty() {
            println!("  {}", muted("No artifacts found."));
        } else {
            for artifact in &artifacts {
                let path = artifact.path.as_deref().unwrap_or("-");
                println!("  #{} {} {}", artifact.id, artifact.kind, path);
            }
        }
        println!("  {} total", artifacts.len());
    }
    Ok(())
}

async fn handle_watch_create(
    cfg: &Config,
    pool: Option<&SqlitePool>,
    name: String,
    task_type: String,
    every_seconds: i64,
    task_payload_raw: Option<String>,
) -> Result<(), Box<dyn Error>> {
    let task_payload = match task_payload_raw {
        Some(raw) => Some(serde_json::from_str(&raw).map_err(|e| {
            format!("watch create: --task-payload is not valid JSON: {e} (got '{raw}')")
        })?),
        None => None,
    };
    let task_payload = task_payload.unwrap_or_else(|| serde_json::json!({}));
    let input = watch_svc::WatchDefCreateRequest {
        name,
        task_type,
        task_payload,
        every_seconds,
        enabled: None,
        next_run_at: None,
    }
    .into_create()
    .map_err(|msg| format!("watch create: {msg}"))?;
    let created = match pool {
        Some(pool) => watch_svc::create_watch_def_with_pool(pool, &input).await?,
        None => watch_svc::create_watch_def(cfg, &input).await?,
    };
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&created)?);
    } else {
        println!("created watch {} ({})", created.name, created.id);
    }
    Ok(())
}

async fn handle_watch_exec(
    cfg: &Config,
    pool: Option<&SqlitePool>,
    raw_id: &str,
) -> Result<(), Box<dyn Error>> {
    let watch_id = parse_uuid(Some(&raw_id.to_string()), "exec")?;
    let watch = match pool {
        Some(pool) => watch_svc::get_watch_def_with_pool(pool, watch_id).await?,
        None => watch_svc::get_watch_def(cfg, watch_id).await?,
    }
    .ok_or("watch not found")?;
    let run = match pool {
        Some(pool) => watch_svc::run_watch_now_with_pool(cfg, pool, &watch).await?,
        None => watch_svc::run_watch_now(cfg, &watch).await?,
    };
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&run)?);
    } else {
        println!("watch run {} status={}", run.id, run.status);
    }
    Ok(())
}

/// `axon watch get <id>` — backed by [`watch_svc::SqliteWatchStore`], the
/// source-request-backed watch store (WS-B / audit C4-04, issue #298). This
/// is a separate model from the task_type/task_payload watches managed by
/// `create`/`list`/`history`/`exec` above (see `axon-jobs::watch_store`
/// module docs); `id` here is the store's own `watch_<uuid>` string, not the
/// legacy `axon watch create` UUID.
async fn handle_watch_get(
    cfg: &Config,
    pool: Option<&SqlitePool>,
    raw_id: &str,
) -> Result<(), Box<dyn Error>> {
    let store = watch_svc::open_source_watch_store(cfg, pool).await?;
    let watch_id = watch_svc::WatchId::new(raw_id);
    let found = watch_svc::SourceWatchStoreTrait::get(&store, watch_id).await?;
    match found {
        Some(watch) => {
            if cfg.json_output {
                println!("{}", serde_json::to_string_pretty(&watch)?);
            } else {
                println!(
                    "{} {} enabled={} every={}s",
                    watch.watch_id.0,
                    watch.canonical_uri,
                    watch.enabled,
                    watch.schedule.every_seconds
                );
            }
            Ok(())
        }
        None => Err(format!("watch {raw_id} not found").into()),
    }
}

/// Shared implementation for `axon watch update|pause|resume`.
async fn handle_watch_update(
    cfg: &Config,
    pool: Option<&SqlitePool>,
    raw_id: &str,
    request: watch_svc::WatchUpdateRequest,
) -> Result<(), Box<dyn Error>> {
    let store = watch_svc::open_source_watch_store(cfg, pool).await?;
    let watch_id = watch_svc::WatchId::new(raw_id);
    let updated = watch_svc::SourceWatchStoreTrait::update(&store, watch_id, request).await?;
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&updated)?);
    } else {
        println!(
            "watch {} updated enabled={} every={}s",
            updated.watch_id.0, updated.enabled, updated.schedule.every_seconds
        );
    }
    Ok(())
}

/// `axon watch delete <id>` — hard-deletes the watch and its run history
/// (`ON DELETE CASCADE`) from the source-request-backed watch store.
async fn handle_watch_delete(
    cfg: &Config,
    pool: Option<&SqlitePool>,
    raw_id: &str,
) -> Result<(), Box<dyn Error>> {
    let store = watch_svc::open_source_watch_store(cfg, pool).await?;
    let watch_id = watch_svc::WatchId::new(raw_id);
    let deleted = store.delete(watch_id).await?;
    if !deleted {
        return Err(format!("watch {raw_id} not found").into());
    }
    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({"watch_id": raw_id, "deleted": true})
        );
    } else {
        println!("deleted watch {raw_id}");
    }
    Ok(())
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
