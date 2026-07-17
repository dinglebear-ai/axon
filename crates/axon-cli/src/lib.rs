#![recursion_limit = "512"]

pub mod commands;
pub mod json;
pub mod schema_registry;
pub mod ui;

use axon_core::config::{CommandKind, Config, parse_args};
use axon_core::logging::{init_tracing, log_done, log_info, log_warn};
use axon_services::context::ServiceContext;
use commands::{
    run_artifacts, run_ask, run_brand, run_capabilities, run_chat, run_collections,
    run_completions, run_config, run_debug, run_diff, run_doctor, run_domains, run_endpoints,
    run_evaluate, run_extract, run_graph, run_jobs, run_map, run_mcp, run_memory, run_migrate,
    run_monitor, run_palette, run_providers, run_prune, run_query, run_research, run_reset,
    run_retrieve, run_screenshot, run_search, run_serve, run_sessions, run_setup, run_source,
    run_sources, run_stats, run_status, run_suggest, run_summarize, run_sync, run_train,
    run_update, run_uploads, run_watch, start_url_from_cfg,
};
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

const JOB_SUBCOMMANDS: &[&str] = &[
    "status", "cancel", "errors", "list", "cleanup", "clear", "worker", "recover",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobCommandMode<'a> {
    Submit { fire_and_forget: bool },
    Subcommand { name: &'a str, needs_workers: bool },
}

async fn run_once(
    cfg: &Config,
    start_url: &str,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    match cfg.command {
        CommandKind::Artifacts => run_artifacts(cfg, service_context).await?,
        CommandKind::Uploads => run_uploads(cfg, service_context).await?,
        CommandKind::Collections => run_collections(cfg, service_context).await?,
        CommandKind::Graph => run_graph(cfg, service_context).await?,
        CommandKind::Providers => run_providers(cfg, service_context).await?,
        CommandKind::Capabilities => run_capabilities(cfg).await?,
        CommandKind::Chat => run_chat(cfg, service_context).await?,
        CommandKind::Map => run_map(cfg, start_url, service_context).await?,
        CommandKind::Endpoints => run_endpoints(cfg).await?,
        CommandKind::Watch => run_watch(cfg, service_context).await?,
        CommandKind::Monitor => run_monitor(cfg, service_context).await?,
        CommandKind::Extract => run_extract(cfg, service_context).await?,
        CommandKind::Search => run_search(cfg, service_context).await?,
        CommandKind::Scrape => run_source(cfg, service_context).await?,
        CommandKind::Brand => run_brand(cfg).await?,
        CommandKind::Debug => run_debug(cfg).await?,
        CommandKind::Diff => run_diff(cfg).await?,
        CommandKind::Doctor => run_doctor(cfg, service_context).await?,
        CommandKind::Query => run_query(cfg, service_context).await?,
        CommandKind::Retrieve => run_retrieve(cfg).await?,
        CommandKind::Ask => run_ask(cfg, service_context).await?,
        CommandKind::Summarize => run_summarize(cfg).await?,
        CommandKind::Evaluate => run_evaluate(cfg, service_context).await?,
        CommandKind::Train => run_train(cfg, service_context).await?,
        CommandKind::Suggest => run_suggest(cfg).await?,
        CommandKind::Sources => run_sources(cfg).await?,
        CommandKind::Domains => run_domains(cfg).await?,
        CommandKind::Stats => run_stats(cfg).await?,
        CommandKind::Status => run_status(cfg, service_context).await?,
        CommandKind::Jobs => run_jobs(cfg, service_context).await?,
        CommandKind::Memory => run_memory(cfg, service_context).await?,
        CommandKind::Sessions => run_sessions(cfg, service_context).await?,
        CommandKind::Source => run_source(cfg, service_context).await?,
        CommandKind::Research => run_research(cfg, service_context).await?,
        CommandKind::Screenshot => run_screenshot(cfg).await?,
        CommandKind::Completions => run_completions(cfg).await?,
        CommandKind::Mcp => run_mcp(cfg).await?,
        CommandKind::Serve => run_serve(cfg).await?,
        // `reset` is dispatched early in `run()` (before any ServiceContext is
        // built) so its dry-run mutates nothing; it never reaches `run_once`.
        CommandKind::Reset => unreachable!("reset is dispatched before run_once"),
        CommandKind::Prune => run_prune(cfg, service_context).await?,
        CommandKind::Preflight | CommandKind::Smoke | CommandKind::Compose => {
            run_setup(cfg).await?
        }
        CommandKind::Setup => run_setup(cfg).await?,
        CommandKind::Migrate => run_migrate(cfg).await?,
        CommandKind::Config => run_config(cfg).await?,
        CommandKind::Sync => run_sync(cfg, service_context).await?,
        CommandKind::Update => run_update(cfg).await?,
        CommandKind::Palette => run_palette(cfg).await?,
    }
    Ok(())
}

fn job_command_mode(cfg: &Config) -> Option<JobCommandMode<'_>> {
    if !matches!(cfg.command, CommandKind::Extract) {
        return None;
    }

    if let Some(subcommand) = cfg
        .positional
        .first()
        .map(String::as_str)
        .filter(|subcommand| JOB_SUBCOMMANDS.contains(subcommand))
    {
        return Some(JobCommandMode::Subcommand {
            name: subcommand,
            needs_workers: subcommand == "worker",
        });
    }

    Some(JobCommandMode::Submit {
        fire_and_forget: !cfg.wait,
    })
}

fn command_needs_workers(cfg: &Config, command_mode: Option<JobCommandMode<'_>>) -> bool {
    cfg.wait
        || matches!(
            cfg.command,
            CommandKind::Source | CommandKind::Scrape | CommandKind::Map
        )
        || matches!(
            command_mode,
            Some(JobCommandMode::Subcommand {
                needs_workers: true,
                ..
            })
        )
}

/// Returns true if the process argv is the `setup plugin-hook` (or `setup hook`)
/// invocation. Inspected from raw argv before the Config is built so the plugin
/// env-var mapping can run before `parse_args()` reads the AXON_* env vars.
///
/// Scans for `setup` followed by `plugin-hook`/`hook`, skipping the leading
/// program name and tolerating global flags that may precede the subcommand.
fn is_plugin_hook_invocation<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut seen_setup = false;
    for (idx, arg) in args.into_iter().enumerate() {
        if idx == 0 {
            continue; // program name
        }
        let s = arg.as_ref().to_string_lossy();
        if !seen_setup {
            if s == "setup" {
                seen_setup = true;
            }
        } else if s == "plugin-hook" || s == "hook" {
            return true;
        } else if !s.starts_with('-') {
            // First non-flag token after `setup` is the subcommand; if it
            // isn't plugin-hook/hook, this isn't the hook invocation.
            return false;
        }
    }
    false
}

fn exit_if_reserved_source_command() {
    let command = axon_core::config::build_cli_command();
    if let Err(err) = axon_core::config::source_routing::route_bare_source_or_error(
        std::env::args().collect(),
        &command,
    ) {
        eprintln!(
            "`axon {}` has been removed from the unified source surface. {}",
            err.token(),
            err.replacement()
        );
        std::process::exit(8);
    }
}

fn exit_if_removed_command() {
    let Some(command) = std::env::args().nth(1) else {
        return;
    };
    let guidance = match command.as_str() {
        "refresh" => Some("use `axon <source> --refresh` or source refresh/watch operations"),
        "fresh" => Some("use `axon watch ...` or source freshness configuration"),
        _ => None,
    };
    if let Some(guidance) = guidance {
        eprintln!("`axon {command}` has been removed from the public CLI. {guidance}.");
        std::process::exit(8);
    }
}

pub async fn run() -> Result<(), Box<dyn Error>> {
    // CRITICAL ORDERING: the `setup plugin-hook` invocation must apply the
    // Claude Code plugin-option → AXON_* env-var mapping BEFORE parse_args()
    // builds the Config (parse_args reads the AXON_* env vars). This is the
    // Rust replacement for the bash `plugin-setup.sh` SessionStart hook, which
    // used to `export` these env vars before exec'ing `axon`. Detect the
    // invocation from raw argv (the Config doesn't exist yet) and apply the
    // mapping early; doing it in the command handler would be too late.
    if is_plugin_hook_invocation(std::env::args_os()) {
        commands::apply_plugin_options();
    }

    exit_if_removed_command();
    exit_if_reserved_source_command();

    // Parse CLI args first so the user's --color choice is installed before
    // anything (including the tracing-subscriber writer) reads it. clap exits
    // on --help/--version, so init_tracing's appender guard isn't needed yet.
    let cfg = parse_args();
    axon_core::ui::install_color_choice(cfg.color_choice);
    // Hold the tracing-appender guard for the lifetime of run(): dropping it stops the
    // background flush thread and tail buffers can be lost from the rolling log file.
    let _log_guard = init_tracing();
    if let Some(warning) = axon_core::binary_status::stale_binary_warning() {
        eprintln!("warning: {warning}");
    }
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        pid = std::process::id(),
        "startup"
    );
    tracing::info!(
        command = cfg.command.as_str(),
        collection = %cfg.collection,
        sqlite_path = %cfg.sqlite_path.display(),
        data_dir = %axon_core::paths::axon_data_base_dir().display(),
        output_dir = %cfg.output_dir.display(),
        "runtime paths resolved"
    );

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

    // `reset` must run BEFORE any ServiceContext is built: constructing the
    // job runtime opens + migrates the SQLite DB (creating a fresh 42-table
    // file), which would break reset's dry-run "mutate nothing" guarantee and
    // make its inventory always report a fully-migrated DB. Reset owns store
    // lifecycle directly, so it needs no job runtime.
    if cfg.command == CommandKind::Reset {
        run_reset(&cfg).await?;
        log_done("command=reset complete");
        return Ok(());
    }

    let cfg_arc = Arc::new(cfg);
    // CLI commands use ServiceContext::new() (enqueue-only) unless the command
    // intentionally needs in-process workers. Fire-and-forget submits enqueue
    // and exit; operator `worker` subcommands spawn workers in this process.
    let command_mode = job_command_mode(&cfg_arc);
    // `source` and retained `scrape` index synchronously in the foreground but
    // need the data-plane runtime (ledger/embedding/vector stores), which is
    // only attached to a worker-bearing ServiceContext.
    let needs_workers = command_needs_workers(cfg_arc.as_ref(), command_mode);
    let service_context = if needs_workers {
        ServiceContext::new_with_workers(Arc::clone(&cfg_arc)).await
    } else {
        ServiceContext::new(Arc::clone(&cfg_arc)).await
    }
    .map_err(|e| -> Box<dyn Error> { e })?;
    let cfg = cfg_arc.as_ref();

    if let Some(every_seconds) = cfg.cron_every_seconds {
        if matches!(command_mode, Some(JobCommandMode::Subcommand { .. })) {
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

    if matches!(
        command_mode,
        Some(JobCommandMode::Submit {
            fire_and_forget: true
        })
    ) {
        log_done(&format!("command={} enqueued", cfg.command.as_str()));
    } else if let Some(JobCommandMode::Subcommand { name, .. }) = command_mode {
        log_done(&format!("command={} {} done", cfg.command.as_str(), name));
    } else {
        log_done(&format!("command={} complete", cfg.command.as_str()));
    }
    Ok(())
}

#[cfg(test)]
#[path = "scrape_map_source_projection_tests.rs"]
mod scrape_map_source_projection_tests;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
