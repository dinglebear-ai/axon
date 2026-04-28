#![recursion_limit = "512"]

pub mod crates;

use self::crates::cli::commands::{
    run_ask, run_completions, run_crawl, run_debug, run_dedupe, run_doctor, run_domains, run_embed,
    run_evaluate, run_extract, run_ingest, run_map, run_mcp, run_migrate, run_query, run_research,
    run_retrieve, run_scrape, run_screenshot, run_search, run_sessions, run_sources, run_stats,
    run_status, run_suggest, run_watch, start_url_from_cfg,
};
use self::crates::core::config::{CommandKind, Config, parse_args};
use self::crates::core::logging::{init_tracing, log_done, log_info, log_warn};
use self::crates::services::context::ServiceContext;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

async fn run_once(
    cfg: &Config,
    start_url: &str,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    match cfg.command {
        CommandKind::Scrape => run_scrape(cfg).await?,
        CommandKind::Map => run_map(cfg, start_url).await?,
        CommandKind::Crawl => run_crawl(cfg, service_context).await?,
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
        CommandKind::Completions => run_completions(cfg).await?,
        CommandKind::Mcp => run_mcp(cfg).await?,
        CommandKind::Migrate => run_migrate(cfg).await?,
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
            CommandKind::Crawl | CommandKind::Extract | CommandKind::Embed | CommandKind::Ingest
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

    let cfg_arc = Arc::new(cfg);
    // CLI commands use ServiceContext::new() (enqueue-only).
    // Fire-and-forget jobs (without --wait) require `axon mcp` to be running as a
    // daemon. `axon mcp` uses ServiceContext::new_with_workers() which spawns in-process workers.
    // When --wait is set, spawn workers inline so the job can complete before exit.
    let needs_workers = cfg_arc.wait;
    let service_context = if needs_workers {
        ServiceContext::new_with_workers(Arc::clone(&cfg_arc)).await
    } else {
        ServiceContext::new(Arc::clone(&cfg_arc)).await
    }
    .map_err(|e| -> Box<dyn Error> { e })?;
    let cfg = cfg_arc.as_ref();

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
    run_once(cfg, &start_url, &service_context).await?;

    if is_async_enqueue_mode(cfg) {
        log_done(&format!("command={} enqueued", cfg.command.as_str()));
    } else if let Some(sub) = job_subcommand_name(cfg) {
        log_done(&format!("command={} {} done", cfg.command.as_str(), sub));
    } else {
        log_done(&format!("command={} complete", cfg.command.as_str()));
    }
    Ok(())
}
