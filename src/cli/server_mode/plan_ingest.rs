use crate::cli::server_mode::ServerJobFamily;
use crate::core::config::Config;
use crate::jobs::ingest::IngestSource;

use super::{ServerPlanError, ServerRestPlan, async_job_lifecycle_plan};

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
            body: serde_json::json!({
                "source_type": "sessions",
                "sessions": {
                    "claude": cfg.sessions_claude,
                    "codex": cfg.sessions_codex,
                    "gemini": cfg.sessions_gemini,
                    "project": cfg.sessions_project,
                },
            }),
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
    match source {
        IngestSource::Github {
            repo,
            include_source,
        } => serde_json::json!({
            "source_type": "github",
            "target": repo,
            "include_source": include_source,
        }),
        IngestSource::Gitlab {
            target,
            include_source,
        } => serde_json::json!({
            "source_type": "gitlab",
            "target": target,
            "include_source": include_source,
        }),
        IngestSource::Gitea {
            target,
            include_source,
        } => serde_json::json!({
            "source_type": "gitea",
            "target": target,
            "include_source": include_source,
        }),
        IngestSource::GenericGit {
            target,
            include_source,
        } => serde_json::json!({
            "source_type": "git",
            "target": target,
            "include_source": include_source,
        }),
        IngestSource::Reddit { target } => serde_json::json!({
            "source_type": "reddit",
            "target": target,
        }),
        IngestSource::Youtube { target } => serde_json::json!({
            "source_type": "youtube",
            "target": target,
        }),
        IngestSource::Sessions {
            sessions_claude,
            sessions_codex,
            sessions_gemini,
            sessions_project,
        } => serde_json::json!({
            "source_type": "sessions",
            "sessions": {
                "claude": sessions_claude,
                "codex": sessions_codex,
                "gemini": sessions_gemini,
                "project": sessions_project,
            },
        }),
    }
}
