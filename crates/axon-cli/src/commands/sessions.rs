use crate::commands::ingest_common;
use crate::commands::source::{render_source_result, source_result_json};
use axon_api::source::{LifecycleStatus, ResponseMode, SourceIntent, SourceRequest, SourceScope};
use axon_core::config::{Config, SessionsRuntimeAction};
use axon_services::context::ServiceContext;
use axon_services::sessions as sessions_service;
use axon_services::sessions_legacy::watch::{
    SessionWatchEventSink, SessionWatchProcessEvent,
    validate::{SessionProvider, SessionWatchRoots},
};
use axon_services::source::enqueue::enqueue_source;
use std::error::Error;
use std::path::PathBuf;

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

    run_session_sources(cfg, service_context).await
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

async fn run_session_sources(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let selectors = selected_session_selectors(cfg)?;
    if selectors.is_empty() {
        if cfg.json_output {
            println!(
                "{}",
                serde_json::json!({
                    "sessions": [],
                    "warning": "no selected session roots exist on this host"
                })
            );
        } else {
            println!("No selected session roots exist on this host.");
        }
        return Ok(());
    }

    let mut results = Vec::new();
    for (provider, root) in selectors {
        let request = session_source_request(cfg, provider, root);
        let result = if cfg.wait {
            axon_services::index_source(request, service_context)
                .await
                .map_err(|err| -> Box<dyn Error> { err.to_string().into() })?
        } else {
            let store = service_context
                .job_store()
                .ok_or("sessions source enqueue requires a unified job store")?;
            enqueue_source(request, store.as_ref(), None)
                .await
                .map_err(|err| -> Box<dyn Error> { err.to_string().into() })?
        };
        if !cfg.json_output {
            render_source_result(cfg, &result);
        }
        if result.status == LifecycleStatus::Failed {
            let msg = result
                .warnings
                .first()
                .map(|warning| warning.message.clone())
                .unwrap_or_else(|| "session source indexing failed".to_string());
            return Err(msg.into());
        }
        results.push(result);
    }

    if cfg.json_output {
        let sessions = results
            .iter()
            .map(|result| source_result_json(cfg, result))
            .collect::<Vec<_>>();
        println!("{}", serde_json::json!({ "sessions": sessions }));
    }
    Ok(())
}

fn selected_session_selectors(
    cfg: &Config,
) -> Result<Vec<(SessionProvider, PathBuf)>, Box<dyn Error>> {
    let roots = SessionWatchRoots::from_config(cfg)?;
    let all = !cfg.sessions_claude && !cfg.sessions_codex && !cfg.sessions_gemini;
    let mut selected = Vec::new();
    if all || cfg.sessions_claude {
        selected.push((SessionProvider::Claude, roots.claude_projects));
    }
    if all || cfg.sessions_codex {
        selected.push((SessionProvider::Codex, roots.codex_sessions));
    }
    if all || cfg.sessions_gemini {
        selected.push((SessionProvider::Gemini, roots.gemini_history));
        selected.push((SessionProvider::Gemini, roots.gemini_tmp));
    }
    Ok(selected
        .into_iter()
        .filter(|(_, root)| root.exists())
        .collect())
}

fn session_source_request(cfg: &Config, provider: SessionProvider, root: PathBuf) -> SourceRequest {
    let mut request =
        SourceRequest::new(format!("session:{}:{}", provider.as_str(), root.display()));
    request.intent = SourceIntent::Acquire;
    request.collection = Some(cfg.collection.clone());
    request.embed = cfg.embed;
    request.scope = Some(SourceScope::Thread);
    request.output.response_mode = ResponseMode::Summary;
    if let Some(project) = cfg.sessions_project.as_deref() {
        request
            .options
            .values
            .insert("project_filter".to_string(), serde_json::json!(project));
    }
    request
}
