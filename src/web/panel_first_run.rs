use super::server::{AppState, HttpError, authorized};
use crate::core::config::{Config, ConfigOverrides, RenderMode};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub(super) struct FirstCrawlRequest {
    url: String,
}

#[derive(Deserialize)]
pub(super) struct FirstAskRequest {
    query: String,
}

pub(super) async fn first_run_crawl(
    State((state, cfg)): State<(AppState, Arc<Config>)>,
    headers: HeaderMap,
    Json(req): Json<FirstCrawlRequest>,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let url = req.url.trim();
    let url = match validate_first_run_url(url) {
        Ok(url) => url,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    let cfg = cfg.apply_overrides(&ConfigOverrides {
        max_pages: Some(10),
        max_depth: Some(1),
        include_subdomains: Some(false),
        respect_robots: Some(false),
        discover_sitemaps: Some(false),
        discover_llms_txt: Some(false),
        render_mode: Some(RenderMode::AutoSwitch),
        ..ConfigOverrides::default()
    });
    match crate::services::crawl::crawl_start_with_context(
        &cfg,
        &[url.to_string()],
        &state.service_context,
        None,
    )
    .await
    {
        Ok(outcome) => Json(serde_json::json!({
            "job_ids": outcome.result.job_ids,
            "output_dir": outcome.result.output_dir,
            "predicted_paths": outcome.result.predicted_paths,
            "predicted_artifact_handles": outcome.result.predicted_artifact_handles,
            "jobs": outcome.result.jobs,
        }))
        .into_response(),
        Err(err) => HttpError::from_box(err).into_response(),
    }
}

pub(super) async fn first_run_ask(
    State((state, cfg)): State<(AppState, Arc<Config>)>,
    headers: HeaderMap,
    Json(req): Json<FirstAskRequest>,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let query = req.query.trim();
    let query = match validate_first_run_query(query) {
        Ok(query) => query,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    match crate::services::query::ask(&cfg, query, None).await {
        Ok(result) => Json(result).into_response(),
        Err(err) => HttpError::from_box(err).into_response(),
    }
}

fn validate_first_run_url(url: &str) -> Result<&str, &'static str> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        Err("url is required")
    } else if !is_http_url(trimmed) {
        Err("url must be an http or https URL")
    } else {
        Ok(trimmed)
    }
}

fn is_http_url(url: &str) -> bool {
    match url::Url::parse(url) {
        Ok(parsed) => matches!(parsed.scheme(), "http" | "https"),
        Err(_) => false,
    }
}

fn validate_first_run_query(query: &str) -> Result<&str, &'static str> {
    if query.trim().is_empty() {
        Err("query is required")
    } else {
        Ok(query.trim())
    }
}

#[cfg(test)]
#[path = "panel_first_run_tests.rs"]
mod tests;
