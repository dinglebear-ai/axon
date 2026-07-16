use super::server::{AppState, HttpError, authorized};
use axon_api::source::{SourceLimits, SourceRequest, SourceScope};
use axon_core::config::Config;
use axon_services::service_traits::{SourceService, SourceServiceImpl};
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
        return HttpError::new(StatusCode::UNAUTHORIZED, "unauthorized", "unauthorized")
            .into_response();
    }
    let url = req.url.trim();
    let url = match validate_first_run_url(url) {
        Ok(url) => url,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    let mut request = SourceRequest::new(url);
    request.scope = Some(SourceScope::Site);
    request.collection = Some(cfg.collection.clone());
    request.limits = SourceLimits {
        max_pages: Some(10),
        max_depth: Some(1),
        ..SourceLimits::default()
    };
    request
        .options
        .values
        .insert("include_subdomains".to_string(), serde_json::json!(false));
    request
        .options
        .values
        .insert("discover_sitemaps".to_string(), serde_json::json!(false));
    request
        .options
        .values
        .insert("max_sitemaps".to_string(), serde_json::json!(0));
    request
        .options
        .values
        .insert("render_mode".to_string(), serde_json::json!("auto_switch"));

    let source_service = SourceServiceImpl::new(Arc::clone(&state.service_context));
    match source_service.submit(request).await {
        Ok(result) => Json(serde_json::json!({
            "job_ids": [result.job_id.clone()],
            "source": result,
        }))
        .into_response(),
        Err(err) => HttpError::new(
            StatusCode::BAD_GATEWAY,
            "source_unavailable",
            err.to_string(),
        )
        .into_response(),
    }
}

pub(super) async fn first_run_ask(
    State((state, cfg)): State<(AppState, Arc<Config>)>,
    headers: HeaderMap,
    Json(req): Json<FirstAskRequest>,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return HttpError::new(StatusCode::UNAUTHORIZED, "unauthorized", "unauthorized")
            .into_response();
    }
    let query = req.query.trim();
    let query = match validate_first_run_query(query) {
        Ok(query) => query,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    match axon_services::query::ask(&state.service_context, &cfg, query, None).await {
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
