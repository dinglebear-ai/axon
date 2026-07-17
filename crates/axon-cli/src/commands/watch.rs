use axon_core::config::Config;
use axon_core::ui::{muted, primary};
use axon_services::context::ServiceContext;
use axon_services::watch as watch_svc;
use clap::{Parser, Subcommand};
use sqlx::SqlitePool;
use std::error::Error;

#[derive(Debug, Parser)]
struct WatchRuntimeArgs {
    #[command(subcommand)]
    action: Option<WatchRuntimeSubcommand>,
}

#[derive(Debug, Subcommand)]
enum WatchRuntimeSubcommand {
    Create {
        source: String,
        #[arg(long = "every-seconds")]
        every_seconds: i64,
        #[arg(long = "collection")]
        collection: Option<String>,
    },
    List,
    Get {
        id: String,
    },
    Status {
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
}

fn parse_watch_runtime_args(args: &[String]) -> Result<WatchRuntimeArgs, Box<dyn Error>> {
    WatchRuntimeArgs::try_parse_from(
        std::iter::once("watch").chain(args.iter().map(String::as_str)),
    )
    .map_err(|err| err.to_string().into())
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
            source,
            every_seconds,
            collection,
        } => {
            handle_watch_create(
                cfg,
                shared_pool.as_deref(),
                source,
                every_seconds,
                collection,
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
        WatchRuntimeSubcommand::Status { id } => {
            handle_watch_status(cfg, service_context, shared_pool.as_deref(), &id).await?
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
    }
    Ok(())
}

async fn handle_watch_create(
    cfg: &Config,
    pool: Option<&SqlitePool>,
    source: String,
    every_seconds: i64,
    collection: Option<String>,
) -> Result<(), Box<dyn Error>> {
    axon_jobs::watch_schedule::validate_every_seconds(every_seconds)
        .map_err(|msg| format!("watch create: {msg}"))?;
    let source = source_from_arg(&source)?;
    let request = watch_svc::WatchRequest {
        source,
        schedule: watch_svc::WatchSchedule {
            every_seconds: every_seconds.max(0) as u64,
            cron: None,
            timezone: None,
        },
        embed: true,
        options: watch_svc::AdapterOptions::default(),
        scope: None,
        collection,
        enabled: Some(true),
    };
    let created = match pool {
        Some(pool) => watch_svc::create_source_watch(cfg, Some(pool), request, None).await?,
        None => watch_svc::create_source_watch(cfg, None, request, None).await?,
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

fn source_from_arg(raw_source: &str) -> Result<String, Box<dyn Error>> {
    let source = raw_source.trim();
    if source.is_empty() {
        return Err("watch create requires a source".into());
    }
    Ok(source.to_string())
}

async fn handle_watch_exec(
    cfg: &Config,
    service_context: &ServiceContext,
    pool: Option<&SqlitePool>,
    raw_id_or_source: &str,
) -> Result<(), Box<dyn Error>> {
    let watch_id = watch_svc::resolve_source_watch_id(cfg, pool, raw_id_or_source).await?;
    let run = watch_svc::exec_source_watch(
        service_context,
        pool,
        watch_id,
        watch_svc::WatchExecRequest {
            reason: None,
            refresh: None,
            wait: None,
        },
        None,
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

async fn handle_watch_status(
    cfg: &Config,
    service_context: &ServiceContext,
    pool: Option<&SqlitePool>,
    raw_id_or_source: &str,
) -> Result<(), Box<dyn Error>> {
    let watch_id = watch_svc::resolve_source_watch_id(cfg, pool, raw_id_or_source).await?;
    let store = watch_svc::open_source_watch_store(cfg, pool).await?;
    let watch = watch_svc::SourceWatchStoreTrait::get(&store, watch_id.clone())
        .await?
        .ok_or_else(|| format!("watch {} not found", watch_id.0))?;
    let latest_job_summary = match watch.latest_job.as_ref() {
        Some(job) => axon_services::jobs::unified_job_status(service_context, job.job_id)
            .await
            .map_err(|err| -> Box<dyn Error> { err.to_string().into() })?,
        None => None,
    };
    if cfg.json_output {
        crate::json::print_json_gated(&serde_json::json!({
            "watch": watch,
            "latest_job_summary": latest_job_summary,
        }))?;
    } else {
        println!("{}", primary("Watch Status"));
        println!(
            "  {} enabled={} every={}s",
            watch.watch_id.0, watch.enabled, watch.schedule.every_seconds
        );
        println!("  source {}", watch.canonical_uri);
        match latest_job_summary {
            Some(job) => println!(
                "  latest_job {} {:?} {:?}",
                job.job_id.0, job.kind, job.status
            ),
            None => println!("  {}", muted("No jobs recorded.")),
        }
    }
    Ok(())
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
    raw_id_or_source: &str,
    limit: i64,
) -> Result<(), Box<dyn Error>> {
    let limit = u32::try_from(limit.max(0)).unwrap_or(u32::MAX);
    let watch_id = watch_svc::resolve_source_watch_id(cfg, pool, raw_id_or_source).await?;
    let history = watch_svc::history_source_watch(
        cfg,
        pool,
        watch_svc::WatchHistoryRequest {
            watch_id,
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
