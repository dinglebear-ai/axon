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
            let store = watch_svc::open_source_watch_store(cfg, shared_pool.as_deref()).await?;
            let watches = watch_svc::SourceWatchStoreTrait::list(
                &store,
                watch_svc::WatchListRequest {
                    enabled: None,
                    source_id: None,
                    adapter: None,
                    limit: Some(200),
                    cursor: None,
                },
            )
            .await?;
            if cfg.json_output {
                println!("{}", serde_json::to_string_pretty(&watches)?);
            } else {
                println!("{}", primary("Source Watches"));
                if watches.items.is_empty() {
                    println!("  {}", muted("No watches defined."));
                } else {
                    for w in &watches.items {
                        println!(
                            "  {} enabled={} every={}s",
                            w.watch_id.0, w.enabled, w.schedule.every_seconds
                        );
                    }
                }
                println!(
                    "  {} total",
                    watches.total.unwrap_or(watches.items.len() as u64)
                );
            }
        }
        WatchRuntimeSubcommand::Exec { id } => {
            handle_watch_exec(cfg, service_context, shared_pool.as_deref(), &id).await?
        }
        WatchRuntimeSubcommand::History { id, limit } => {
            handle_watch_history(cfg, shared_pool.as_deref(), &id, limit).await?
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
    axon_jobs::watch::validate_every_seconds(every_seconds)
        .map_err(|msg| format!("watch create: {msg}"))?;
    axon_jobs::watch::validate_task_type(&task_type)
        .map_err(|msg| format!("watch create: {msg}"))?;
    let source = watch_create_source(&name, &task_payload)?;
    let request = watch_svc::WatchRequest {
        source,
        schedule: watch_svc::WatchSchedule {
            every_seconds: every_seconds.max(0) as u64,
            cron: None,
            timezone: None,
        },
        embed: false,
        options: watch_svc::AdapterOptions::default(),
        scope: None,
        collection: None,
        enabled: Some(true),
    };
    let created = match pool {
        Some(pool) => watch_svc::create_source_watch(cfg, Some(pool), request).await?,
        None => watch_svc::create_source_watch(cfg, None, request).await?,
    };
    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&watch_create_json_output(&created))?
        );
    } else {
        println!("{}", watch_create_human_output(&created));
    }
    Ok(())
}

fn watch_create_json_output(created: &watch_svc::WatchResult) -> serde_json::Value {
    serde_json::json!(created)
}

fn watch_create_human_output(created: &watch_svc::WatchResult) -> String {
    format!(
        "created watch {} source={}",
        created.watch_id.0, created.canonical_uri
    )
}

fn watch_create_source(
    name_or_source: &str,
    task_payload: &serde_json::Value,
) -> Result<String, Box<dyn Error>> {
    if let Some(source) = first_watch_url(task_payload) {
        return Ok(source);
    }
    let source = name_or_source.trim();
    if source.is_empty() {
        return Err("watch create requires a source".into());
    }
    Ok(source.to_string())
}

fn first_watch_url(task_payload: &serde_json::Value) -> Option<String> {
    task_payload
        .get("urls")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

async fn handle_watch_exec(
    cfg: &Config,
    service_context: &ServiceContext,
    pool: Option<&SqlitePool>,
    raw_id: &str,
) -> Result<(), Box<dyn Error>> {
    let run = watch_svc::exec_source_watch(
        service_context,
        pool,
        watch_svc::WatchId::new(raw_id),
        watch_svc::WatchExecRequest {
            reason: None,
            refresh: None,
            wait: None,
        },
    )
    .await?;
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&run)?);
    } else {
        println!("watch job {} status={:?}", run.job_id.0, run.status);
    }
    Ok(())
}

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
    let deleted = store.delete(watch_id.clone()).await?;
    if !deleted {
        return Err(format!("watch {raw_id} not found").into());
    }
    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({"watch_id": watch_id.0, "deleted": true})
        );
    } else {
        println!("deleted watch {}", watch_id.0);
    }
    Ok(())
}

async fn handle_watch_history(
    cfg: &Config,
    pool: Option<&SqlitePool>,
    raw_id: &str,
    limit: i64,
) -> Result<(), Box<dyn Error>> {
    let limit = u32::try_from(limit.max(0)).unwrap_or(u32::MAX);
    let history = watch_svc::history_source_watch(
        cfg,
        pool,
        watch_svc::WatchHistoryRequest {
            watch_id: watch_svc::WatchId::new(raw_id),
            limit: Some(limit),
            cursor: None,
            status: None,
        },
    )
    .await?;
    if cfg.json_output {
        crate::json::print_json_gated(&history)?;
    } else {
        println!("{}", primary("Watch History"));
        if history.jobs.is_empty() {
            println!("  {}", muted("No runs found."));
        } else {
            for job in &history.jobs {
                println!("  {} {:?} {:?}", job.job_id.0, job.kind, job.status);
            }
        }
        println!("  {} total", history.jobs.len());
    }
    Ok(())
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
