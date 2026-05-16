use super::server::{AppState, authorized};
use crate::services::{action_api::dispatch_action, context::ServiceContext};
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
    State((state, cfg)): State<(AppState, Arc<crate::core::config::Config>)>,
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
    let ctx = match panel_service_context(&state, cfg).await {
        Ok(ctx) => ctx,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
    };
    let action = crate::mcp::schema::AxonRequest::Crawl(crate::mcp::schema::CrawlRequest {
        subaction: Some(crate::mcp::schema::CrawlSubaction::Start),
        urls: Some(vec![url.to_string()]),
        job_id: None,
        limit: None,
        offset: None,
        response_mode: Some(crate::mcp::schema::ResponseMode::Inline),
        max_pages: Some(10),
        max_depth: Some(1),
        include_subdomains: Some(false),
        respect_robots: Some(false),
        discover_sitemaps: Some(false),
        sitemap_since_days: None,
        render_mode: Some(crate::mcp::schema::McpRenderMode::AutoSwitch),
        delay_ms: None,
    });
    match dispatch_action(&ctx, action).await {
        Ok(result) => Json(result).into_response(),
        Err(err) => (StatusCode::BAD_GATEWAY, err.message).into_response(),
    }
}

pub(super) async fn first_run_ask(
    State((state, cfg)): State<(AppState, Arc<crate::core::config::Config>)>,
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
    let ctx = match panel_service_context(&state, cfg).await {
        Ok(ctx) => ctx,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
    };
    let action = crate::mcp::schema::AxonRequest::Ask(crate::mcp::schema::AskRequest {
        query: Some(query.to_string()),
        graph: None,
        diagnostics: Some(false),
        explain: None,
        collection: None,
        since: None,
        before: None,
        hybrid_search: None,
        response_mode: Some(crate::mcp::schema::ResponseMode::Inline),
    });
    match dispatch_action(&ctx, action).await {
        Ok(result) => Json(result).into_response(),
        Err(err) => (StatusCode::BAD_GATEWAY, err.message).into_response(),
    }
}

async fn panel_service_context(
    state: &AppState,
    cfg: Arc<crate::core::config::Config>,
) -> Result<Arc<ServiceContext>, String> {
    state
        .service_context
        .get_or_try_init(|| async { ServiceContext::new_with_workers(cfg).await.map(Arc::new) })
        .await
        .map(Arc::clone)
        .map_err(|err| format!("failed to initialize service context: {err}"))
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
