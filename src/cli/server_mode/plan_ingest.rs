use crate::cli::server_mode::ServerJobFamily;
use crate::core::config::Config;
use crate::jobs::ingest::IngestSource;
use crate::services::client_contract::{
    RestIngestRequest, RestIngestSourceType, RestSessionsIngestOptions,
};

use super::{ServerPlanError, ServerRestPlan, async_job_lifecycle_plan, json_body};

pub(super) fn ingest_server_rest_plan(
    cfg: &Config,
    sessions: bool,
) -> Result<ServerRestPlan, ServerPlanError> {
    if !sessions
        && let Some(subaction) = cfg.positional.first().map(String::as_str)
        && let Some(plan) =
            async_job_lifecycle_plan("ingest", ServerJobFamily::Ingest, subaction, cfg)?
    {
        return Ok(plan);
    }
    if sessions {
        return Ok(ServerRestPlan {
            method: "POST",
            path: "/v1/ingest".to_string(),
            body: json_body(RestIngestRequest {
                source_type: RestIngestSourceType::Sessions,
                target: None,
                include_source: None,
                sessions: Some(RestSessionsIngestOptions {
                    claude: Some(cfg.sessions_claude),
                    codex: Some(cfg.sessions_codex),
                    gemini: Some(cfg.sessions_gemini),
                    project: cfg.sessions_project.clone(),
                }),
            })?,
            label: "sessions",
            poll_family: Some(ServerJobFamily::Ingest),
        });
    }
    let target = cfg
        .positional
        .first()
        .ok_or_else(|| ServerPlanError::new("ingest requires <target>"))?;
    let source = crate::services::ingest::classify_target(target, cfg.github_include_source)
        .map_err(|err| ServerPlanError::new(err.to_string()))?;
    Ok(ServerRestPlan {
        method: "POST",
        path: "/v1/ingest".to_string(),
        body: ingest_source_action_body(source),
        label: "ingest",
        poll_family: Some(ServerJobFamily::Ingest),
    })
}

fn ingest_source_action_body(source: IngestSource) -> serde_json::Value {
    let req = match source {
        IngestSource::Github {
            repo,
            include_source,
        } => RestIngestRequest {
            source_type: RestIngestSourceType::Github,
            target: Some(repo),
            include_source: Some(include_source),
            sessions: None,
        },
        IngestSource::Gitlab {
            target,
            include_source,
        } => RestIngestRequest {
            source_type: RestIngestSourceType::Gitlab,
            target: Some(target),
            include_source: Some(include_source),
            sessions: None,
        },
        IngestSource::Gitea {
            target,
            include_source,
        } => RestIngestRequest {
            source_type: RestIngestSourceType::Gitea,
            target: Some(target),
            include_source: Some(include_source),
            sessions: None,
        },
        IngestSource::GenericGit {
            target,
            include_source,
        } => RestIngestRequest {
            source_type: RestIngestSourceType::Git,
            target: Some(target),
            include_source: Some(include_source),
            sessions: None,
        },
        IngestSource::Reddit { target } => RestIngestRequest {
            source_type: RestIngestSourceType::Reddit,
            target: Some(target),
            include_source: None,
            sessions: None,
        },
        IngestSource::Youtube { target } => RestIngestRequest {
            source_type: RestIngestSourceType::Youtube,
            target: Some(target),
            include_source: None,
            sessions: None,
        },
        IngestSource::Sessions {
            sessions_claude,
            sessions_codex,
            sessions_gemini,
            sessions_project,
        } => RestIngestRequest {
            source_type: RestIngestSourceType::Sessions,
            target: None,
            include_source: None,
            sessions: Some(RestSessionsIngestOptions {
                claude: Some(sessions_claude),
                codex: Some(sessions_codex),
                gemini: Some(sessions_gemini),
                project: sessions_project,
            }),
        },
    };
    serde_json::to_value(req).unwrap_or(serde_json::Value::Null)
}
