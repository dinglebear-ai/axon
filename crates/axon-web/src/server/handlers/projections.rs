//! Focused REST projections over the canonical source and query services.

use super::sources::{authorize_source_request, caller_context_from_auth};
use crate::server::error::HttpError;
use crate::server::json::Json;
use crate::server::state::AppState;
use axon_api::QueryResult;
use axon_api::source::*;
use axon_services::projections::{
    SourceAccessPolicy, execute_code_search_projection_batch, execute_source_projection_batch,
    preflight_code_search_batch, preflight_source_batch,
};
use axum::{Extension, extract::State, http::StatusCode};
use lab_auth::AuthContext;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

type WebState = (AppState, Arc<axon_core::config::Config>);
type CodeSearchResponseFuture =
    Pin<Box<dyn Future<Output = Result<Json<BatchResult<QueryResult>>, HttpError>> + Send>>;

macro_rules! source_handler {
    ($name:ident, $path:literal, $request:ty, $operation:ident, $project:ident, $operation_id:literal) => {
        #[utoipa::path(
            post,
            path = $path,
            operation_id = $operation_id,
            request_body = $request,
            responses(
                (status = 200, description = "Projection batch completed", body = BatchResult<SourceResult>),
                (status = 202, description = "Projection batch admitted", body = BatchResult<SourceResult>),
                (status = 400, description = "Invalid projection request", body = crate::server::error::ErrorBody),
                (status = 403, description = "Projection target is not authorized", body = crate::server::error::ErrorBody),
                (status = 409, description = "Idempotency conflict", body = crate::server::error::ErrorBody),
                (status = 413, description = "Projection request exceeds a configured limit", body = crate::server::error::ErrorBody),
                (status = 429, description = "Projection admission is saturated", body = crate::server::error::ErrorBody)
            ),
            tag = "projections"
        )]
        pub(crate) async fn $name(
            State((state, cfg)): State<WebState>,
            auth: Option<Extension<AuthContext>>,
            Json(request): Json<$request>,
        ) -> Result<(StatusCode, Json<BatchResult<SourceResult>>), HttpError> {
            source_projection(
                &state,
                &cfg,
                auth,
                ProjectionOperation::$operation,
                $project(&request).map_err(unbox_api_error)?,
            )
            .await
        }
    };
}

source_handler!(
    scrape,
    "/v1/scrape",
    ScrapeRequest,
    Scrape,
    project_scrape,
    "scrapeSources"
);
source_handler!(
    crawl,
    "/v1/crawl",
    CrawlRequest,
    Crawl,
    project_crawl,
    "crawlSources"
);
source_handler!(
    embed,
    "/v1/embed",
    EmbedRequest,
    Embed,
    project_embed,
    "embedSources"
);
source_handler!(
    ingest,
    "/v1/ingest",
    IngestRequest,
    Ingest,
    project_ingest,
    "ingestSources"
);

async fn source_projection(
    state: &AppState,
    cfg: &axon_core::config::Config,
    auth: Option<Extension<AuthContext>>,
    operation: ProjectionOperation,
    requests: Vec<SourceRequest>,
) -> Result<(StatusCode, Json<BatchResult<SourceResult>>), HttpError> {
    let auth_snapshot = if let Some(Extension(auth)) = auth {
        for request in &requests {
            authorize_source_request(request, &auth).await?;
        }
        let caller = caller_context_from_auth(&auth);
        Some(AuthSnapshot::from_caller(
            &caller,
            caller.visibility_ceiling,
            "runtime",
        ))
    } else {
        None
    };
    let access = SourceAccessPolicy {
        operator_allows_tool_execution: cfg.allow_tool_execution,
        allowed_roots: Some(cfg.source_local_allowed_roots.clone()),
        ..SourceAccessPolicy::default()
    };
    let prepared = preflight_source_batch(
        operation,
        requests,
        auth_snapshot.as_ref(),
        &cfg.projection_batch,
        &access,
    )
    .map_err(HttpError::from_api_error)?;
    let result = execute_source_projection_batch(
        state.service_context.as_ref(),
        operation,
        prepared,
        auth_snapshot,
    )
    .await
    .map_err(HttpError::from_api_error)?;
    let status = source_projection_status(result.status.clone());
    Ok((status, Json(result)))
}

fn source_projection_status(status: BatchStatus) -> StatusCode {
    if status == BatchStatus::Accepted {
        StatusCode::ACCEPTED
    } else {
        StatusCode::OK
    }
}

#[utoipa::path(
    post,
    path = "/v1/code-search",
    operation_id = "codeSearch",
    request_body = CodeSearchRequest,
    responses(
        (status = 200, description = "Committed-state code search results", body = BatchResult<QueryResult>),
        (status = 400, description = "Invalid code search request", body = crate::server::error::ErrorBody),
        (status = 413, description = "Code search request exceeds a configured limit", body = crate::server::error::ErrorBody)
    ),
    tag = "projections"
)]
pub(crate) fn code_search(
    State((state, cfg)): State<WebState>,
    auth: Option<Extension<AuthContext>>,
    Json(request): Json<CodeSearchRequest>,
) -> CodeSearchResponseFuture {
    Box::pin(code_search_owned(state, cfg, auth, request))
}

async fn code_search_owned(
    state: AppState,
    cfg: Arc<axon_core::config::Config>,
    auth: Option<Extension<AuthContext>>,
    request: CodeSearchRequest,
) -> Result<Json<BatchResult<QueryResult>>, HttpError> {
    let auth_snapshot = auth.map(|Extension(auth)| {
        let caller = caller_context_from_auth(&auth);
        AuthSnapshot::from_caller(&caller, caller.visibility_ceiling, "runtime")
    });
    let plans = project_code_search(&request).map_err(unbox_api_error)?;
    let prepared = preflight_code_search_batch(plans, &cfg.projection_batch)
        .map_err(HttpError::from_api_error)?;
    let result = execute_code_search_projection_batch(
        state.service_context.as_ref().clone(),
        prepared,
        axon_api::CodeSearchCaller::Rest,
        auth_snapshot,
    )
    .await
    .map_err(HttpError::from_api_error)?;
    Ok(Json(result))
}

fn unbox_api_error(error: Box<ApiError>) -> HttpError {
    HttpError::from_api_error(*error)
}

#[cfg(test)]
#[path = "projections_tests.rs"]
mod tests;
