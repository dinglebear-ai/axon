use super::super::HttpError;
use super::super::state::AppState;
use super::super::types::{
    ConfigResponse, EnvConfigResponse, OpsResponse, PanelCollectionsResponse, PanelCommandRequest,
    PanelCommandResponse, PanelDoctorResponse, PanelStatusResponse, SaveConfigRequest,
    SaveConfigResponse, SaveEnvConfigRequest,
};
use super::super::utils::authorized;
use axon_api::mcp_schema::{
    AxonRequest, ExtractRequest, ExtractSubaction, ResponseMode, ScreenshotRequest, SourceRequest,
    StatusRequest,
};
use axon_api::source::SourceScope;
use axon_core::config::Config;
use axon_services::{action_api, config as config_service, query as query_service, setup, system};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use std::sync::Arc;

pub async fn get_config(
    State((state, _)): State<(AppState, Arc<Config>)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return HttpError::new(StatusCode::UNAUTHORIZED, "unauthorized", "unauthorized")
            .into_response();
    }
    match setup::config_store::read_config() {
        Ok(raw_toml) => Json(ConfigResponse {
            path: state.panel.config_path.clone(),
            raw_toml,
            restart_required: false,
        })
        .into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

pub async fn save_config(
    State((state, _)): State<(AppState, Arc<Config>)>,
    headers: HeaderMap,
    Json(req): Json<SaveConfigRequest>,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return HttpError::new(StatusCode::UNAUTHORIZED, "unauthorized", "unauthorized")
            .into_response();
    }
    match setup::config_store::write_config(&req.raw_toml) {
        Ok(()) => (
            StatusCode::ACCEPTED,
            Json(SaveConfigResponse {
                ok: true,
                restart_required: true,
                message: "Config saved. Restart Axon for changes to affect live panel requests.",
            }),
        )
            .into_response(),
        Err(err) if err.kind() == std::io::ErrorKind::InvalidInput => {
            (StatusCode::BAD_REQUEST, err.to_string()).into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

pub async fn get_env_config(
    State((state, _)): State<(AppState, Arc<Config>)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return HttpError::new(StatusCode::UNAUTHORIZED, "unauthorized", "unauthorized")
            .into_response();
    }
    let Some(path) = config_service::resolve_env_path() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "HOME unset; cannot resolve ~/.axon/.env",
        )
            .into_response();
    };
    match config_service::read_env_text(&path) {
        Ok(raw_env) => Json(EnvConfigResponse {
            path: path.display().to_string(),
            raw_env,
            restart_required: false,
        })
        .into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

pub async fn save_env_config(
    State((state, _)): State<(AppState, Arc<Config>)>,
    headers: HeaderMap,
    Json(req): Json<SaveEnvConfigRequest>,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return HttpError::new(StatusCode::UNAUTHORIZED, "unauthorized", "unauthorized")
            .into_response();
    }
    let Some(path) = config_service::resolve_env_path() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "HOME unset; cannot resolve ~/.axon/.env",
        )
            .into_response();
    };
    match config_service::write_env_text(&path, &req.raw_env) {
        Ok(()) => (
            StatusCode::ACCEPTED,
            Json(SaveConfigResponse {
                ok: true,
                restart_required: true,
                message: ".env saved. Restart Axon for changes to affect live panel requests.",
            }),
        )
            .into_response(),
        Err(err)
            if matches!(
                err.kind(),
                std::io::ErrorKind::InvalidInput | std::io::ErrorKind::InvalidData
            ) =>
        {
            (StatusCode::BAD_REQUEST, err.to_string()).into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

pub async fn ops(
    State((state, cfg)): State<(AppState, Arc<Config>)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return HttpError::new(StatusCode::UNAUTHORIZED, "unauthorized", "unauthorized")
            .into_response();
    }
    Json(OpsResponse {
        qdrant_url: cfg.qdrant_url.clone(),
        tei_url: cfg.tei_url.clone(),
        collection: cfg.collection.clone(),
        mcp_http_url: format!("http://{}:{}/mcp", cfg.mcp_http_host, cfg.mcp_http_port),
    })
    .into_response()
}

pub async fn panel_collections(
    State((state, cfg)): State<(AppState, Arc<Config>)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return HttpError::new(StatusCode::UNAUTHORIZED, "unauthorized", "unauthorized")
            .into_response();
    }

    match collections_response(&cfg).await {
        Ok(response) => Json(response).into_response(),
        Err(error) => error.into_response(),
    }
}

pub async fn collections(
    State((_state, cfg)): State<(AppState, Arc<Config>)>,
) -> impl IntoResponse {
    match collections_response(&cfg).await {
        Ok(response) => Json(response).into_response(),
        Err(error) => error.into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/v1/collections",
    responses(
        (status = 200, description = "Available Qdrant collection names", body = PanelCollectionsResponse),
        (status = 502, description = "Qdrant collections request failed", body = crate::server::error::ErrorBody)
    ),
    tag = "discovery"
)]
#[allow(dead_code)]
pub async fn collections_openapi_marker() {}

async fn collections_response(cfg: &Config) -> Result<PanelCollectionsResponse, HttpError> {
    match system::collections(cfg).await {
        Ok(result) => Ok(PanelCollectionsResponse {
            collections: result.collections,
        }),
        Err(error) => Err(collections_error_to_http(error)),
    }
}

fn collections_error_to_http(error: system::CollectionsError) -> HttpError {
    match error {
        system::CollectionsError::ClientBuild(err) => HttpError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            err.to_string(),
        ),
        err => HttpError::new(StatusCode::BAD_GATEWAY, "bad_gateway", err.to_string()),
    }
}

pub async fn panel_status(
    State((state, _)): State<(AppState, Arc<Config>)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return HttpError::new(StatusCode::UNAUTHORIZED, "unauthorized", "unauthorized")
            .into_response();
    }
    match system::full_status(&state.service_context).await {
        Ok(status) => Json(PanelStatusResponse {
            payload: sanitize_status_payload(status.payload),
            text: status.text,
            totals: status.totals,
        })
        .into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

pub async fn panel_doctor(
    State((state, _cfg)): State<(AppState, Arc<Config>)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return HttpError::new(StatusCode::UNAUTHORIZED, "unauthorized", "unauthorized")
            .into_response();
    }
    match system::doctor(&state.service_context).await {
        Ok(result) => Json(PanelDoctorResponse {
            payload: result.payload,
        })
        .into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

pub async fn panel_command(
    State((state, cfg)): State<(AppState, Arc<Config>)>,
    headers: HeaderMap,
    Json(req): Json<PanelCommandRequest>,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return HttpError::new(StatusCode::UNAUTHORIZED, "unauthorized", "unauthorized")
            .into_response();
    }
    let command = req.command.trim();
    if command.is_empty() {
        return (StatusCode::BAD_REQUEST, "command is required").into_response();
    }

    match parse_panel_command(command) {
        Ok(ParsedPanelCommand::Ask { query }) => {
            match query_service::ask(&state.service_context, &cfg, &query, None).await {
                Ok(result) => Json(PanelCommandResponse {
                    command: command.to_string(),
                    action: serde_json::json!({ "action": "ask", "query": query }),
                    result: serde_json::to_value(result).unwrap_or_else(
                        |err| serde_json::json!({ "serialization_error": err.to_string() }),
                    ),
                })
                .into_response(),
                Err(err) => (StatusCode::BAD_GATEWAY, err.to_string()).into_response(),
            }
        }
        Ok(ParsedPanelCommand::Action(action)) => {
            let action_json = serde_json::to_value(&action).unwrap_or_else(
                |err| serde_json::json!({ "serialization_error": err.to_string() }),
            );
            match action_api::dispatch_action(&state.service_context, *action).await {
                Ok(result) => Json(PanelCommandResponse {
                    command: command.to_string(),
                    action: action_json,
                    result: sanitize_status_payload(result),
                })
                .into_response(),
                Err(err) => {
                    let status = if err.kind == "invalid_request" {
                        StatusCode::BAD_REQUEST
                    } else {
                        StatusCode::BAD_GATEWAY
                    };
                    (status, err.message).into_response()
                }
            }
        }
        Err(err) => (StatusCode::BAD_REQUEST, err).into_response(),
    }
}

enum ParsedPanelCommand {
    Action(Box<AxonRequest>),
    Ask { query: String },
}

fn parse_panel_command(command: &str) -> Result<ParsedPanelCommand, String> {
    let (verb, rest) = command
        .split_once(char::is_whitespace)
        .map(|(verb, rest)| (verb.trim().to_ascii_lowercase(), rest.trim()))
        .unwrap_or_else(|| (command.trim().to_ascii_lowercase(), ""));
    match verb.as_str() {
        "status" => Ok(ParsedPanelCommand::Action(Box::new(AxonRequest::Status(
            StatusRequest {
                response_mode: Some(ResponseMode::Inline),
            },
        )))),
        "scrape" => {
            let url = required_arg(rest, "scrape requires a URL")?;
            Ok(ParsedPanelCommand::Action(Box::new(AxonRequest::Source(
                SourceRequest {
                    source: Some(normalize_url(url)),
                    scope: Some(SourceScope::Page),
                    collection: None,
                    response_mode: Some(ResponseMode::Inline),
                    detached: None,
                },
            ))))
        }
        "crawl" => {
            let url = required_arg(rest, "crawl requires a URL")?;
            Ok(ParsedPanelCommand::Action(Box::new(AxonRequest::Source(
                SourceRequest {
                    source: Some(normalize_url(url)),
                    scope: Some(SourceScope::Site),
                    collection: None,
                    response_mode: Some(ResponseMode::Inline),
                    detached: None,
                },
            ))))
        }
        "ask" => {
            let query = required_arg(rest, "ask requires a question")?;
            Ok(ParsedPanelCommand::Ask {
                query: query.to_string(),
            })
        }
        "extract" => {
            let (prompt, url) = parse_extract_args(rest)?;
            Ok(ParsedPanelCommand::Action(Box::new(AxonRequest::Extract(
                ExtractRequest {
                    subaction: Some(ExtractSubaction::Start),
                    urls: Some(vec![normalize_url(url)]),
                    prompt: Some(prompt.to_string()),
                    ..Default::default()
                },
            ))))
        }
        "screenshot" => {
            let url = required_arg(rest, "screenshot requires a URL")?;
            Ok(ParsedPanelCommand::Action(Box::new(AxonRequest::Screenshot(
                ScreenshotRequest {
                    url: Some(normalize_url(url)),
                    full_page: Some(true),
                    viewport: None,
                    output: None,
                    response_mode: Some(ResponseMode::Inline),
                },
            ))))
        }
        _ => Err("supported commands: status, scrape <url>, crawl <url>, ask <question>, extract <prompt> from <url>, screenshot <url>".to_string()),
    }
}

fn required_arg<'a>(value: &'a str, message: &'static str) -> Result<&'a str, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(message.to_string())
    } else {
        Ok(trimmed)
    }
}

fn parse_extract_args(rest: &str) -> Result<(&str, &str), String> {
    let rest = required_arg(rest, "extract requires a prompt and URL")?;
    if let Some((prompt, url)) = rest.rsplit_once(" from ") {
        let prompt = required_arg(prompt, "extract requires a prompt before 'from'")?;
        let url = required_arg(url, "extract requires a URL after 'from'")?;
        return Ok((prompt, url));
    }
    Err("extract syntax: extract <prompt> from <url>".to_string())
}

fn normalize_url(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

fn sanitize_status_payload(mut value: serde_json::Value) -> serde_json::Value {
    let Some(object) = value.as_object_mut() else {
        return value;
    };
    for key in ["source_jobs", "extract_jobs", "watch_jobs", "prune_jobs"] {
        let Some(jobs) = object
            .get_mut(key)
            .and_then(serde_json::Value::as_array_mut)
        else {
            continue;
        };
        for job in jobs {
            if let Some(job) = job.as_object_mut() {
                job.remove("config_json");
            }
        }
    }
    value
}

/// Serve an artifact file from the configured output directory.
///
/// Requires a valid panel session, then delegates to
/// [`super::artifacts::serve_artifact_from_path`], which validates `rel_path`
/// (rejecting absolute paths, `..` traversal, symlinks, and escapes of
/// `cfg.output_dir`) before streaming the file. The output root is the same one
/// used when constructing the artifact handle paths the panel links to.
pub async fn panel_artifact(
    State((state, cfg)): State<(AppState, Arc<Config>)>,
    headers: HeaderMap,
    Path(rel_path): Path<String>,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return HttpError::new(StatusCode::UNAUTHORIZED, "unauthorized", "unauthorized")
            .into_response();
    }
    match super::artifacts::serve_artifact_from_path(&cfg, rel_path).await {
        Ok(response) => response,
        Err(err) => err.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collections_status_errors_map_to_bad_gateway() {
        let error = collections_error_to_http(system::CollectionsError::Status(
            StatusCode::SERVICE_UNAVAILABLE,
        ));

        assert_eq!(error.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(error.kind(), "bad_gateway");
        assert!(error.message().contains("503"));
    }
}
