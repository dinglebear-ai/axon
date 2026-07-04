//! `POST /v1/sources` — the transport-neutral source indexing entrypoint.
//!
//! This is the REST surface for the unified source pipeline (#298). It parses
//! a JSON body into an [`axon_api::source::SourceRequest`], hands it to
//! [`axon_services::index_source`] (which classifies + acquires + dispatches to
//! the right family bridge), and returns the resulting
//! [`axon_api::source::SourceResult`] as JSON. All legacy indexing routes
//! (`/v1/embed`, `/v1/ingest`, `/v1/scrape`, `/v1/crawl`) fold into this one
//! route per the surface-removal contract.
//!
//! `index_source`'s future is not `Send` (the web-source bridge holds a
//! `Box<dyn Error>` across an `.await`), so — like `admin::run_watch` — the
//! call runs on a blocking thread via `spawn_blocking` + `Handle::block_on`,
//! whose `JoinHandle` is `Send` and thus a valid axum handler future.

use axon_api::ApiError;
use axon_api::source::{SourceRequest, SourceResult};
use axon_error::ErrorStage;
use axum::{Json, extract::State, http::StatusCode};
use std::sync::Arc;

use super::super::error::HttpError;
use super::super::state::AppState;

type WebState = (AppState, Arc<axon_core::config::Config>);

#[utoipa::path(
    post,
    path = "/v1/sources",
    request_body = SourceRequest,
    responses(
        (status = 200, description = "Source indexing result", body = SourceResult),
        (status = 400, description = "Invalid source request", body = crate::server::error::ErrorBody),
        (status = 502, description = "Upstream service unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "sources"
)]
pub(crate) async fn index_source(
    State((state, _cfg)): State<WebState>,
    Json(request): Json<SourceRequest>,
) -> Result<Json<SourceResult>, HttpError> {
    if request.source.trim().is_empty() {
        // The source pipeline produces a contract `ApiError` directly; it is
        // passed through the transport verbatim as an `ErrorEnvelope`.
        return Err(HttpError::from_api_error(
            ApiError::new(
                "route.validation.missing_field",
                ErrorStage::Validation,
                "source is required",
            )
            .with_context("field", "source"),
        ));
    }
    let service_context = Arc::clone(&state.service_context);
    let handle = tokio::runtime::Handle::current();
    let result = tokio::task::spawn_blocking(move || {
        handle.block_on(async move {
            axon_services::index_source(request, service_context.as_ref())
                .await
                .map_err(|err| err.to_string())
        })
    })
    .await
    .map_err(|err| {
        HttpError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            format!("source indexing task failed: {err}"),
        )
    })?;
    result
        .map(Json)
        .map_err(|message| HttpError::new(StatusCode::BAD_GATEWAY, "upstream_unavailable", message))
}
