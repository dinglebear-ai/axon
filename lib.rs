#![recursion_limit = "512"]

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
use self::crates::jobs::backend::JobKind;
use self::crates::services::context::ServiceContext;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::error::Error;
use std::sync::Arc;
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

async fn run_once(
    cfg: &Config,
    start_url: &str,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    match cfg.command {
        CommandKind::Scrape => run_scrape(cfg).await?,
        CommandKind::Map => run_map(cfg, start_url).await?,
        CommandKind::Crawl => run_crawl(cfg, service_context).await?,
        CommandKind::Refresh => run_refresh(cfg, service_context).await?,
        CommandKind::Watch => run_watch(cfg, service_context).await?,
        CommandKind::Extract => run_extract(cfg, service_context).await?,
        CommandKind::Search => run_search(cfg).await?,
        CommandKind::Embed => run_embed(cfg, service_context).await?,
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
        CommandKind::Status => run_status(cfg, service_context).await?,
        CommandKind::Dedupe => run_dedupe(cfg).await?,
        CommandKind::Ingest => run_ingest(cfg, service_context).await?,
        CommandKind::Sessions => run_sessions(cfg, service_context).await?,
        CommandKind::Research => run_research(cfg).await?,
        CommandKind::Screenshot => run_screenshot(cfg).await?,
        CommandKind::Graph => run_graph(cfg, service_context).await?,
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

fn command_to_job_kind(cmd: CommandKind) -> Option<JobKind> {
    match cmd {
        CommandKind::Crawl => Some(JobKind::Crawl),
        CommandKind::Extract => Some(JobKind::Extract),
        CommandKind::Embed => Some(JobKind::Embed),
        CommandKind::Ingest => Some(JobKind::Ingest),
        CommandKind::Refresh => Some(JobKind::Refresh),
        _ => None,
    }
}

async fn drain_lite_async_enqueue(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    if cfg.lite_mode
        && is_async_enqueue_mode(cfg)
        && let Some(kind) = command_to_job_kind(cfg.command)
    {
        service_context
            .jobs
            .run_worker(kind)
            .await
            .map_err(|e| -> Box<dyn Error> { e })?;
    }
    Ok(())
}

/// Scan argv/env for --log-level before tracing is initialised.
/// Writes RUST_LOG so EnvFilter picks it up in init_tracing().
fn early_scan_log_level() {
    // Prefer explicit RUST_LOG — don't clobber it.
    if std::env::var("RUST_LOG").is_ok() {
        return;
    }
    let args: Vec<String> = std::env::args().collect();
    let mut level: Option<String> = None;
    for i in 0..args.len() {
        if args[i] == "--log-level" {
            level = args.get(i + 1).cloned();
            break;
        }
        if let Some(v) = args[i].strip_prefix("--log-level=") {
            level = Some(v.to_string());
            break;
        }
    }
    if level.is_none() {
        level = std::env::var("AXON_LOG_LEVEL").ok().filter(|v| !v.is_empty());
    }
    if let Some(l) = level {
        // Safety: single-threaded at this point (before tokio runtime).
        #[allow(unsafe_code)]
        unsafe { std::env::set_var("RUST_LOG", l); }
    }
}

/// Send sd_notify READY=1 via the socket advertised by systemd.
/// No-ops gracefully when not running under systemd.
fn sd_notify_ready() {
    use std::os::unix::net::UnixDatagram;
    if let Ok(path) = std::env::var("NOTIFY_SOCKET") {
        // Abstract sockets start with '@'; replace with null byte for the API.
        let path = if path.starts_with('@') {
            format!("\0{}", &path[1..])
        } else {
            path
        };
        if let Ok(sock) = UnixDatagram::unbound() {
            let _ = sock.send_to(b"READY=1\n", path);
            tracing::debug!(action = "sd_notify", "sent READY=1 to systemd");
        }
    }
}

pub async fn run() -> Result<(), Box<dyn Error>> {
    early_scan_log_level();
    // _log_guard MUST live for run() duration — dropping it stops file logging.
    let _log_guard = init_tracing();
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

    let cfg_arc = Arc::new(cfg);
    // In lite mode, mirror how the MCP server works: always spawn in-process workers for
    // async job commands so jobs are processed without needing `axon serve`.
    // Read-only management subcommands (status/cancel/list/etc.) don't need workers.
    let needs_workers = cfg_arc.lite_mode
        && matches!(
            cfg_arc.command,
            CommandKind::Crawl
                | CommandKind::Extract
                | CommandKind::Embed
                | CommandKind::Ingest
                | CommandKind::Refresh
        )
        && !matches!(
            cfg_arc.positional.first().map(|s| s.as_str()),
            Some("status" | "cancel" | "errors" | "list" | "cleanup" | "clear" | "recover")
        );
    let service_context = if needs_workers {
        ServiceContext::new_with_workers(Arc::clone(&cfg_arc)).await
    } else {
        ServiceContext::new(Arc::clone(&cfg_arc)).await
    }
    .map_err(|e| -> Box<dyn Error> { e })?;
    let cfg = cfg_arc.as_ref();

    // Notify systemd that we are fully initialised and ready to accept traffic.
    sd_notify_ready();

    // Skip Postgres telemetry in lite mode (no Postgres connection required).
    if !cfg.lite_mode {
        let pg_url = cfg.pg_url.clone();
        let command = cfg.command.as_str().to_string();
        tokio::spawn(async move {
            record_command_run(pg_url, command).await;
        });
    }

    if let Some(every_seconds) = cfg.cron_every_seconds {
        if is_job_subcommand(cfg) {
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
            match run_once(cfg, &start_url, &service_context).await {
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
    // Periodic health tick — logs uptime every 60 s for ops monitoring.
    let _health_tick = {
        let start = std::time::Instant::now();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(60));
            ticker.tick().await; // skip immediate first tick
            loop {
                ticker.tick().await;
                tracing::info!(
                    action = "health.tick",
                    uptime_s = start.elapsed().as_secs(),
                    "process alive"
                );
            }
        })
    };

    run_once(cfg, &start_url, &service_context).await?;

    // In lite mode, fire-and-forget commands enqueue a job then need to stay alive until
    // the in-process workers finish. Workers are tokio tasks — they die when the process exits.
    drain_lite_async_enqueue(cfg, &service_context).await?;

    if is_async_enqueue_mode(cfg) && !cfg.lite_mode {
        log_done(&format!("command={} enqueued", cfg.command.as_str()));
    } else if let Some(sub) = job_subcommand_name(cfg) {
        log_done(&format!("command={} {} done", cfg.command.as_str(), sub));
    } else {
        log_done(&format!("command={} complete", cfg.command.as_str()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::jobs::backend::{BackendResult, JobPayload};
    use crate::crates::services::runtime::{ServiceJobRuntime, WorkerMode};
    use crate::crates::services::types::ServiceJob;
    use async_trait::async_trait;
    use uuid::Uuid;

    struct DrainErrorRuntime;

    #[async_trait]
    impl ServiceJobRuntime for DrainErrorRuntime {
        fn mode_name(&self) -> &'static str {
            "test"
        }

        async fn enqueue(&self, _payload: JobPayload) -> BackendResult<Uuid> {
            Err("not implemented".into())
        }

        async fn wait_for_job(&self, _id: Uuid, _kind: JobKind) -> BackendResult<String> {
            Err("not implemented".into())
        }

        async fn job_errors(&self, _id: Uuid, _kind: JobKind) -> BackendResult<Option<String>> {
            Ok(None)
        }

        async fn has_active_jobs(&self, _kind: JobKind) -> BackendResult<bool> {
            Ok(false)
        }

        async fn list_jobs(
            &self,
            _kind: JobKind,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
            Ok(Vec::new())
        }

        async fn job_status(
            &self,
            _kind: JobKind,
            _id: Uuid,
        ) -> Result<Option<ServiceJob>, Box<dyn Error + Send + Sync>> {
            Ok(None)
        }

        async fn cancel_job(
            &self,
            _kind: JobKind,
            _id: Uuid,
        ) -> Result<bool, Box<dyn Error + Send + Sync>> {
            Ok(false)
        }

        async fn cleanup_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
            Ok(0)
        }

        async fn clear_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
            Ok(0)
        }

        async fn recover_jobs(
            &self,
            _kind: JobKind,
            _stale_threshold_ms: i64,
        ) -> Result<u64, Box<dyn Error + Send + Sync>> {
            Ok(0)
        }

        async fn run_worker(
            &self,
            _kind: JobKind,
        ) -> Result<WorkerMode, Box<dyn Error + Send + Sync>> {
            Err("forced drain failure".into())
        }
    }

    #[tokio::test]
    async fn lite_async_enqueue_drain_errors_are_propagated() {
        let mut cfg = Config::test_default();
        cfg.command = CommandKind::Embed;
        cfg.lite_mode = true;
        cfg.wait = false;
        let ctx = ServiceContext::from_runtime(Arc::new(cfg.clone()), Arc::new(DrainErrorRuntime));

        let err = drain_lite_async_enqueue(&cfg, &ctx)
            .await
            .expect_err("drain failure should propagate");

        assert!(err.to_string().contains("forced drain failure"));
    }
}
