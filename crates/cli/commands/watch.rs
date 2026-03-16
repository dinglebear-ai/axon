use crate::crates::core::config::Config;
use crate::crates::services::refresh as refresh_service;
use crate::crates::services::watch as watch_svc;
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

pub async fn run_watch(cfg: &Config) -> Result<(), Box<dyn Error>> {
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
                for w in watches {
                    println!("{} {} {}", w.id, w.task_type, w.name);
                }
            }
        }
        WatchRuntimeSubcommand::RunNow { id } => handle_watch_run_now(cfg, &id).await?,
        WatchRuntimeSubcommand::History { id, limit } => {
            let watch_id = parse_uuid(Some(&id), "history")?;
            let runs = watch_svc::list_watch_runs(cfg, watch_id, limit).await?;
            println!("{}", serde_json::to_string_pretty(&runs)?);
        }
        WatchRuntimeSubcommand::Get { .. } => return Err("unknown watch subcommand: get".into()),
        WatchRuntimeSubcommand::Update { .. } => {
            return Err("unknown watch subcommand: update".into());
        }
        WatchRuntimeSubcommand::Pause { .. } => {
            return Err("unknown watch subcommand: pause".into());
        }
        WatchRuntimeSubcommand::Resume { .. } => {
            return Err("unknown watch subcommand: resume".into());
        }
        WatchRuntimeSubcommand::Delete { .. } => {
            return Err("unknown watch subcommand: delete".into());
        }
        WatchRuntimeSubcommand::Artifacts { .. } => {
            return Err("unknown watch subcommand: artifacts".into());
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
    let raw_id = raw_id.to_string();
    let watch_id = parse_uuid(Some(&raw_id), "run-now")?;
    let all = watch_svc::list_watch_defs(cfg, 500).await?;
    let watch = all
        .into_iter()
        .find(|w| w.id == watch_id)
        .ok_or("watch not found")?;
    let dispatched_job_id = if watch.task_type == "refresh" {
        let urls = watch
            .task_payload
            .get("urls")
            .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
            .unwrap_or_default();
        if urls.is_empty() {
            None
        } else {
            Some(Uuid::parse_str(
                &refresh_service::refresh_start(cfg, &urls).await?.job_id,
            )?)
        }
    } else {
        None
    };
    let run = watch_svc::create_watch_run(cfg, watch_id, dispatched_job_id).await?;
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&run)?);
    } else {
        println!("watch run {} status={}", run.id, run.status);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::jobs::common::resolve_test_pg_url;
    use crate::crates::jobs::watch::list_watch_defs_with_pool;

    #[test]
    fn parse_uuid_requires_id() {
        let err = parse_uuid(None, "history").expect_err("missing id should error");
        assert!(err.to_string().contains("watch history requires <id>"));
    }

    #[test]
    fn parse_uuid_rejects_invalid_uuid() {
        let raw = "not-a-uuid".to_string();
        let err = parse_uuid(Some(&raw), "run-now").expect_err("invalid uuid should error");
        assert!(err.to_string().contains("invalid character") || err.to_string().contains("UUID"));
    }

    #[test]
    fn parse_watch_runtime_args_rejects_unknown_argument() {
        let err = parse_watch_runtime_args(&[
            "create".to_string(),
            "demo".to_string(),
            "--task-type".to_string(),
            "refresh".to_string(),
            "--every-seconds".to_string(),
            "30".to_string(),
            "--bogus".to_string(),
        ])
        .expect_err("unknown argument should error");
        assert!(err.to_string().contains("--bogus"));
    }

    #[tokio::test]
    async fn handle_watch_create_requires_every_seconds() {
        let cfg = Config::test_default();
        let err = handle_watch_create(&cfg, "demo".to_string(), "refresh".to_string(), 0, None)
            .await
            .expect_err("missing interval should error");
        assert!(err.to_string().contains("--every-seconds"));
    }

    #[tokio::test]
    async fn handle_watch_create_rejects_invalid_task_payload_json() {
        let cfg = Config::test_default();
        let err = handle_watch_create(
            &cfg,
            "demo".to_string(),
            "refresh".to_string(),
            30,
            Some("{oops".to_string()),
        )
        .await
        .expect_err("invalid json should error");
        assert!(err.to_string().contains("--task-payload is not valid JSON"));
    }

    #[tokio::test]
    async fn run_watch_rejects_unknown_subcommand() {
        let mut cfg = Config::test_default();
        cfg.positional = vec!["bogus".to_string()];
        let err = run_watch(&cfg)
            .await
            .expect_err("unknown subcommand should error");
        assert!(err.to_string().contains("bogus"));
    }

    #[tokio::test]
    #[ignore = "requires Postgres infra; run with cargo test cli_watch_ -- --ignored"]
    async fn cli_watch_create_emits_json_with_id() -> Result<(), Box<dyn Error>> {
        let pg_url = resolve_test_pg_url()
            .expect("AXON_TEST_PG_URL must be set for ignored CLI infra tests");
        let mut cfg = Config::test_default();
        cfg.pg_url = pg_url.clone();
        cfg.json_output = true;
        cfg.positional = vec![
            "create".to_string(),
            format!("watch-cli-{}", Uuid::new_v4()),
            "--task-type".to_string(),
            "refresh".to_string(),
            "--every-seconds".to_string(),
            "300".to_string(),
            "--task-payload".to_string(),
            "{\"urls\":[\"https://example.com\"]}".to_string(),
        ];
        run_watch(&cfg).await?;
        let pool = sqlx::PgPool::connect(&pg_url).await?;
        let defs = list_watch_defs_with_pool(&pool, 500).await?;
        assert!(defs.iter().any(|d| d.task_type == "refresh"));
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires Postgres infra; run with cargo test cli_watch_ -- --ignored"]
    async fn cli_watch_list_returns_definitions() -> Result<(), Box<dyn Error>> {
        let pg_url = resolve_test_pg_url()
            .expect("AXON_TEST_PG_URL must be set for ignored CLI infra tests");
        let mut cfg = Config::test_default();
        cfg.pg_url = pg_url;
        cfg.positional = vec!["list".to_string()];
        run_watch(&cfg).await?;
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires Postgres infra; run with cargo test cli_watch_ -- --ignored"]
    async fn cli_watch_run_now_dispatches_task_and_returns_run_id() -> Result<(), Box<dyn Error>> {
        let pg_url = resolve_test_pg_url()
            .expect("AXON_TEST_PG_URL must be set for ignored CLI infra tests");
        let mut cfg = Config::test_default();
        cfg.pg_url = pg_url.clone();
        cfg.positional = vec![
            "create".to_string(),
            format!("watch-run-now-{}", Uuid::new_v4()),
            "--task-type".to_string(),
            "refresh".to_string(),
            "--every-seconds".to_string(),
            "300".to_string(),
            "--task-payload".to_string(),
            "{\"urls\":[\"https://example.com\"]}".to_string(),
        ];
        run_watch(&cfg).await?;

        let pool = sqlx::PgPool::connect(&pg_url).await?;
        let defs = list_watch_defs_with_pool(&pool, 500).await?;
        let watch_id = defs
            .into_iter()
            .find(|d| d.name.starts_with("watch-run-now-"))
            .map(|d| d.id)
            .ok_or("missing watch definition")?;
        cfg.positional = vec!["run-now".to_string(), watch_id.to_string()];
        run_watch(&cfg).await?;
        Ok(())
    }
}
