use crate::crates::core::config::Config;
use crate::crates::jobs::watch::WatchDefCreate;
use crate::crates::jobs::watch::create_watch_run;
use crate::crates::services::refresh as refresh_service;
use crate::crates::services::watch as watch_svc;
use chrono::{Duration, Utc};
use std::error::Error;
use uuid::Uuid;

fn parse_uuid(raw: Option<&String>, action: &str) -> Result<Uuid, Box<dyn Error>> {
    let id = raw.ok_or_else(|| format!("watch {action} requires <id>"))?;
    Ok(Uuid::parse_str(id)?)
}

pub async fn run_watch(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let subcmd = cfg.positional.first().map(|s| s.as_str()).unwrap_or("list");
    match subcmd {
        "create" => handle_watch_create(cfg).await?,
        "list" => {
            let watches = watch_svc::list_watch_defs(cfg, 200).await?;
            if cfg.json_output {
                println!("{}", serde_json::to_string_pretty(&watches)?);
            } else {
                for w in watches {
                    println!("{} {} {}", w.id, w.task_type, w.name);
                }
            }
        }
        "run-now" => handle_watch_run_now(cfg).await?,
        "history" => {
            let watch_id = parse_uuid(cfg.positional.get(1), "history")?;
            let limit = cfg
                .positional
                .iter()
                .position(|s| s == "--limit")
                .and_then(|i| cfg.positional.get(i + 1))
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(50);
            let runs = watch_svc::list_watch_runs(cfg, watch_id, limit).await?;
            println!("{}", serde_json::to_string_pretty(&runs)?);
        }
        _ => return Err(format!("unknown watch subcommand: {subcmd}").into()),
    }
    Ok(())
}

async fn handle_watch_create(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let name = cfg
        .positional
        .get(1)
        .ok_or("watch create requires <name>")?
        .clone();
    let mut task_type = None::<String>;
    let mut every_seconds = None::<i64>;
    let mut task_payload: Option<serde_json::Value> = None;
    let mut idx = 2usize;
    while idx < cfg.positional.len() {
        match cfg.positional[idx].as_str() {
            "--task-type" => {
                task_type = cfg.positional.get(idx + 1).cloned();
                idx += 2;
            }
            "--every-seconds" => {
                let raw = cfg
                    .positional
                    .get(idx + 1)
                    .ok_or("watch create: --every-seconds requires a value")?;
                let secs = raw.parse::<i64>().map_err(|_| {
                    format!("watch create: --every-seconds must be an integer, got '{raw}'")
                })?;
                if secs < 1 {
                    return Err(
                        format!("watch create: --every-seconds must be >= 1, got {secs}").into(),
                    );
                }
                every_seconds = Some(secs);
                idx += 2;
            }
            "--task-payload" => {
                let raw = cfg
                    .positional
                    .get(idx + 1)
                    .ok_or("watch create: --task-payload requires a value")?;
                let parsed = serde_json::from_str(raw).map_err(|e| {
                    format!("watch create: --task-payload is not valid JSON: {e} (got '{raw}')")
                })?;
                task_payload = Some(parsed);
                idx += 2;
            }
            _ => idx += 1,
        }
    }
    let every_seconds = every_seconds.ok_or("watch create requires --every-seconds <integer>")?;
    let created = watch_svc::create_watch_def(
        cfg,
        &WatchDefCreate {
            name,
            task_type: task_type.ok_or("watch create requires --task-type <value>")?,
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

async fn handle_watch_run_now(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let watch_id = parse_uuid(cfg.positional.get(1), "run-now")?;
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
    let run = create_watch_run(cfg, watch_id, dispatched_job_id).await?;
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
