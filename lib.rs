#![recursion_limit = "256"]

pub mod crates;

use self::crates::cli::commands::{
    run_ask, run_completions, run_crawl, run_debug, run_dedupe, run_doctor, run_domains, run_embed,
    run_evaluate, run_export, run_extract, run_graph, run_ingest, run_map, run_mcp, run_migrate,
    run_query, run_refresh, run_research, run_retrieve, run_scrape, run_screenshot, run_search,
    run_serve, run_sessions, run_sources, run_stats, run_status, run_suggest, run_watch,
    start_url_from_cfg,
};
use self::crates::core::config::{CommandKind, Config, parse_args};
use self::crates::core::logging::{init_tracing, log_done, log_info, log_warn};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::error::Error;
use std::sync::OnceLock;
use std::time::Duration;

/// Cached telemetry pool — initialized once per process and reused across
/// cron iterations. A single max_connections(1) pool is sufficient for the
/// lightweight INSERT telemetry fires.
static TELEMETRY_POOL: OnceLock<PgPool> = OnceLock::new();

/// Guard: DDL for `axon_command_runs` runs at most once per process.
/// Skips the CREATE TABLE round-trip on every subsequent invocation.
static COMMAND_RUNS_SCHEMA: OnceLock<()> = OnceLock::new();

pub async fn get_or_init_telemetry_pool(pg_url: &str) -> Result<&'static PgPool, sqlx::Error> {
    if let Some(pool) = TELEMETRY_POOL.get() {
        return Ok(pool);
    }
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(pg_url)
        .await?;
    // Race-safe: if another task initialized first, we drop our pool and use theirs.
    Ok(TELEMETRY_POOL.get_or_init(|| pool))
}

/// Record a CLI command invocation for telemetry. Accepts only the two fields
/// needed to avoid cloning the entire Config struct (~100 fields, 2-5KB heap).
async fn record_command_run(pg_url: String, command: String) {
    if pg_url.is_empty() {
        return;
    }
    let attempt = async {
        let pool = get_or_init_telemetry_pool(&pg_url).await?;

        // DDL guarded by OnceLock — only the first call per process issues the
        // CREATE TABLE round-trip. Subsequent calls skip straight to INSERT.
        if COMMAND_RUNS_SCHEMA.get().is_none() {
            sqlx::query(
                r#"
                CREATE TABLE IF NOT EXISTS axon_command_runs (
                    id BIGSERIAL PRIMARY KEY,
                    command TEXT NOT NULL,
                    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
                )
                "#,
            )
            .execute(pool)
            .await?;
            let _ = COMMAND_RUNS_SCHEMA.set(());
        }

        sqlx::query("INSERT INTO axon_command_runs (command) VALUES ($1)")
            .bind(&command)
            .execute(pool)
            .await?;
        Ok::<(), sqlx::Error>(())
    };
    let _ = tokio::time::timeout(Duration::from_secs(2), attempt).await;
}

async fn run_once(cfg: &Config, start_url: &str) -> Result<(), Box<dyn Error>> {
    match cfg.command {
        CommandKind::Scrape => run_scrape(cfg).await?,
        CommandKind::Map => run_map(cfg, start_url).await?,
        CommandKind::Crawl => run_crawl(cfg).await?,
        CommandKind::Refresh => run_refresh(cfg).await?,
        CommandKind::Watch => run_watch(cfg).await?,
        CommandKind::Extract => run_extract(cfg).await?,
        CommandKind::Search => run_search(cfg).await?,
        CommandKind::Embed => run_embed(cfg).await?,
        CommandKind::Debug => run_debug(cfg).await?,
        CommandKind::Doctor => run_doctor(cfg).await?,
        CommandKind::Query => run_query(cfg).await?,
        CommandKind::Retrieve => run_retrieve(cfg).await?,
        CommandKind::Ask => run_ask(cfg).await?,
        CommandKind::Evaluate => run_evaluate(cfg).await?,
        CommandKind::Suggest => run_suggest(cfg).await?,
        CommandKind::Sources => run_sources(cfg).await?,
        CommandKind::Domains => run_domains(cfg).await?,
        CommandKind::Stats => run_stats(cfg).await?,
        CommandKind::Status => run_status(cfg).await?,
        CommandKind::Dedupe => run_dedupe(cfg).await?,
        CommandKind::Ingest => run_ingest(cfg).await?,
        CommandKind::Sessions => run_sessions(cfg).await?,
        CommandKind::Research => run_research(cfg).await?,
        CommandKind::Screenshot => run_screenshot(cfg).await?,
        CommandKind::Graph => run_graph(cfg).await?,
        CommandKind::Completions => run_completions(cfg).await?,
        CommandKind::Mcp => run_mcp(cfg).await?,
        CommandKind::Serve => run_serve(cfg).await?,
        CommandKind::Migrate => run_migrate(cfg).await?,
        CommandKind::Export => run_export(cfg).await?,
    }
    Ok(())
}

fn is_job_subcommand(cfg: &Config) -> bool {
    matches!(
        cfg.positional.first().map(|s| s.as_str()),
        Some("status" | "cancel" | "errors" | "list" | "cleanup" | "clear" | "worker" | "recover")
    )
}

fn job_subcommand_name(cfg: &Config) -> Option<&str> {
    cfg.positional.first().map(|s| s.as_str()).filter(|s| {
        matches!(
            *s,
            "status" | "cancel" | "errors" | "list" | "cleanup" | "clear" | "worker" | "recover"
        )
    })
}

fn is_async_enqueue_mode(cfg: &Config) -> bool {
    !cfg.wait
        && matches!(
            cfg.command,
            CommandKind::Crawl
                | CommandKind::Refresh
                | CommandKind::Extract
                | CommandKind::Embed
                | CommandKind::Ingest
        )
        && !is_job_subcommand(cfg)
}

pub async fn run() -> Result<(), Box<dyn Error>> {
    init_tracing();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        pid = std::process::id(),
        "startup"
    );
    let cfg = parse_args();

    let start_url = start_url_from_cfg(&cfg);

    let _span = tracing::info_span!(
        "command",
        command = cfg.command.as_str(),
        collection = %cfg.collection
    )
    .entered();

    log_info(&format!(
        "command={} start_url={} render_mode={:?} embed={} collection={} profile={:?}",
        cfg.command.as_str(),
        start_url,
        cfg.render_mode,
        cfg.embed,
        cfg.collection,
        cfg.performance_profile
    ));
    {
        // Extract only the two fields needed — avoids cloning the entire
        // Config struct (~100 fields, 2-5KB heap) for a lightweight telemetry INSERT.
        let pg_url = cfg.pg_url.clone();
        let command = cfg.command.as_str().to_string();
        tokio::spawn(async move {
            record_command_run(pg_url, command).await;
        });
    }

    if let Some(every_seconds) = cfg.cron_every_seconds {
        if is_job_subcommand(&cfg) {
            return Err(
                "--cron-every-seconds is not supported for job subcommands (status/cancel/list/etc)"
                    .into(),
            );
        }
        let max_runs = cfg.cron_max_runs.unwrap_or(usize::MAX);
        let mut run_count = 0usize;
        while run_count < max_runs {
            run_count += 1;
            log_info(&format!(
                "cron run {} command={} interval={}s",
                run_count,
                cfg.command.as_str(),
                every_seconds
            ));
            match run_once(&cfg, &start_url).await {
                Ok(_) => {}
                Err(e) => {
                    log_warn(&format!("cron run_once failed: {e:#}"));
                }
            }
            if run_count < max_runs {
                tokio::time::sleep(Duration::from_secs(every_seconds)).await;
            }
        }
        log_done(&format!(
            "command={} cron complete runs={}",
            cfg.command.as_str(),
            run_count
        ));
        return Ok(());
    }
    run_once(&cfg, &start_url).await?;

    if is_async_enqueue_mode(&cfg) {
        log_done(&format!("command={} enqueued", cfg.command.as_str()));
    } else if let Some(sub) = job_subcommand_name(&cfg) {
        log_done(&format!("command={} {} done", cfg.command.as_str(), sub));
    } else {
        log_done(&format!("command={} complete", cfg.command.as_str()));
    }
    Ok(())
}
