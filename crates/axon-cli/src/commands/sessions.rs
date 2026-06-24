use crate::commands::ingest_common;
use axon_core::config::{Config, SessionsRuntimeAction};
use axon_ingest::sessions::watch::{SessionWatchEventSink, SessionWatchProcessEvent};
use axon_jobs::ingest::IngestSource;
use axon_services::context::ServiceContext;
use axon_services::ingest as ingest_service;
use axon_services::sessions as sessions_service;
use std::error::Error;

pub async fn run_sessions(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    match cfg.sessions_action {
        Some(SessionsRuntimeAction::WatchStatus { limit }) => {
            return run_watch_status(cfg, service_context, limit).await;
        }
        Some(SessionsRuntimeAction::SmokeWatch { timeout_secs }) => {
            return run_smoke_watch(cfg, service_context, timeout_secs).await;
        }
        _ => {}
    }

    if let Some(options) = cfg.sessions_watch.clone() {
        if options.json {
            return sessions_service::run_watch_with_event_sink(
                cfg,
                service_context,
                options,
                &CliSessionWatchEventSink,
            )
            .await
            .map_err(|err| -> Box<dyn Error> { err.into() });
        }
        return sessions_service::run_watch(cfg, service_context, options)
            .await
            .map_err(|err| -> Box<dyn Error> { err.into() });
    }

    if ingest_common::maybe_handle_ingest_subcommand(cfg, service_context, "sessions").await? {
        return Ok(());
    }

    let source = IngestSource::Sessions {
        sessions_claude: cfg.sessions_claude,
        sessions_codex: cfg.sessions_codex,
        sessions_gemini: cfg.sessions_gemini,
        sessions_project: cfg.sessions_project.clone(),
    };

    if !cfg.wait {
        return ingest_common::enqueue_ingest_job(cfg, source, service_context).await;
    }

    run_ingest_sync(cfg).await
}

struct CliSessionWatchEventSink;

impl SessionWatchEventSink for CliSessionWatchEventSink {
    fn emit(&self, event: SessionWatchProcessEvent) {
        println!(
            "{}",
            serde_json::json!({
                "stage": event.stage,
                "provider": event.provider,
                "path_hash": event.path_hash,
                "basename": event.basename,
                "path": event.path,
                "detail": event.detail,
            })
        );
    }
}

async fn run_watch_status(
    cfg: &Config,
    service_context: &ServiceContext,
    limit: usize,
) -> Result<(), Box<dyn Error>> {
    let status = sessions_service::watch_status(service_context, limit).await?;
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&status)?);
    } else {
        println!(
            "session watch checkpoints={} errors={}",
            status.checkpoint_count, status.error_count
        );
    }
    Ok(())
}

async fn run_smoke_watch(
    cfg: &Config,
    service_context: &ServiceContext,
    timeout_secs: u64,
) -> Result<(), Box<dyn Error>> {
    let report = sessions_service::smoke(cfg, service_context, timeout_secs).await?;
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("session watch smoke probe ingested={}", report.ingested);
    }
    Ok(())
}

async fn run_ingest_sync(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let result = ingest_service::ingest_sessions(cfg, None).await?;
    let chunks = result.payload["chunks"]
        .as_u64()
        .ok_or("sessions: service payload missing 'chunks' field")? as usize;
    ingest_common::print_ingest_sync_result(cfg, "sessions", chunks, "local history paths");
    Ok(())
}
