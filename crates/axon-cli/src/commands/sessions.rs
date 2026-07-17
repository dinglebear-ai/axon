use crate::commands::source::{render_source_result, source_result_json};
use axon_api::source::{LifecycleStatus, ResponseMode, SourceIntent, SourceRequest, SourceScope};
use axon_core::config::Config;
use axon_services::context::ServiceContext;
use axon_services::sessions::{SessionProvider, SessionRoots};
use axon_services::source::enqueue::enqueue_source;
use std::error::Error;
use std::path::PathBuf;

pub async fn run_sessions(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    run_session_sources(cfg, service_context).await
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
    let roots = SessionRoots::from_config(cfg)?;
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
