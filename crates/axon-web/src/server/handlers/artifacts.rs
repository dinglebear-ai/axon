//! Opaque-ID artifact metadata and content routes.
//!
//! The `/v1` surface never accepts a filesystem path. The path-confined helper
//! at the bottom remains private to the authenticated first-run panel.

use crate::server::error::HttpError;
use axon_api::source::{ArtifactId, ArtifactListRequest};
use axon_services::artifacts::{ArtifactDetail, ArtifactSummary};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::sync::Arc;
use tokio_util::io::ReaderStream;

/// `(AppState, Arc<Config>)` state shape used by all exploration/artifact handlers.
type WebState = (
    super::super::state::AppState,
    Arc<axon_core::config::Config>,
);

#[utoipa::path(
    get,
    path = "/v1/artifacts",
    params(
        ("source_id" = Option<String>, Query, description = "Filter by source identifier"),
        ("job_id" = Option<String>, Query, description = "Filter by job identifier"),
        ("kind" = Option<String>, Query, description = "Filter by artifact kind"),
        ("limit" = Option<u32>, Query, description = "Page size, capped at 200"),
        ("cursor" = Option<String>, Query, description = "Opaque artifact keyset cursor")
    ),
    responses(
        (status = 200, description = "Paged artifact metadata", body = axon_api::source::Page<ArtifactSummary>),
        (status = 400, description = "Invalid artifact filter or cursor", body = crate::server::error::ErrorBody)
    ),
    tag = "artifacts"
)]
pub(crate) async fn list_artifacts(
    State((state, _cfg)): State<WebState>,
    Query(request): Query<ArtifactListRequest>,
) -> Result<super::super::json::Json<axon_api::source::Page<ArtifactSummary>>, HttpError> {
    axon_services::artifacts::list_artifacts(&state.service_context, request)
        .await
        .map(super::super::json::Json)
        .map_err(HttpError::from_api_error)
}

#[utoipa::path(
    get,
    path = "/v1/artifacts/{artifact_id}",
    params(("artifact_id" = String, Path, description = "Opaque artifact identifier")),
    responses(
        (status = 200, description = "Artifact metadata", body = ArtifactDetail),
        (status = 404, description = "Artifact not found", body = crate::server::error::ErrorBody)
    ),
    tag = "artifacts"
)]
pub(crate) async fn get_artifact(
    State((state, _cfg)): State<WebState>,
    Path(artifact_id): Path<ArtifactId>,
) -> Result<super::super::json::Json<ArtifactDetail>, HttpError> {
    axon_services::artifacts::get_artifact(&state.service_context, artifact_id)
        .await
        .map(super::super::json::Json)
        .map_err(HttpError::from_api_error)
}

#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
pub(crate) struct ArtifactContentQuery {
    /// Force download disposition even for browser-safe image content.
    #[serde(default)]
    download: bool,
}

#[utoipa::path(
    get,
    path = "/v1/artifacts/{artifact_id}/content",
    params(
        ("artifact_id" = String, Path, description = "Opaque artifact identifier"),
        ArtifactContentQuery
    ),
    responses(
        (status = 200, description = "Artifact bytes"),
        (status = 404, description = "Artifact not found", body = crate::server::error::ErrorBody)
    ),
    tag = "artifacts"
)]
pub(crate) async fn artifact_content(
    State((state, _cfg)): State<WebState>,
    Path(artifact_id): Path<ArtifactId>,
    Query(query): Query<ArtifactContentQuery>,
) -> Result<Response, HttpError> {
    let content = axon_services::artifacts::artifact_content(&state.service_context, artifact_id)
        .await
        .map_err(HttpError::from_api_error)?;
    artifact_content_response(content, query).await
}

async fn artifact_content_response(
    content: axon_services::artifacts::ArtifactContentFile,
    query: ArtifactContentQuery,
) -> Result<Response, HttpError> {
    let file = tokio::fs::File::open(&content.path)
        .await
        .map_err(|error| open_artifact_error(&error, &content.artifact_id.0))?;
    let mut response = Body::from_stream(ReaderStream::new(file)).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&content.content_type).map_err(|_| {
            HttpError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "read_error",
                "artifact has an invalid content type",
            )
        })?,
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&content.size_bytes.to_string()).map_err(|_| {
            HttpError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "read_error",
                "artifact has an invalid content length",
            )
        })?,
    );
    if query.download || !is_inline_content_type(&content.content_type) {
        headers.insert(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&content.disposition).map_err(|_| {
                HttpError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "read_error",
                    "artifact has an invalid content disposition",
                )
            })?,
        );
    }
    Ok(response)
}

pub(crate) async fn serve_panel_artifact(
    context: &axon_services::context::ServiceContext,
    artifact_id: ArtifactId,
) -> Result<Response, HttpError> {
    let content = axon_services::artifacts::artifact_content(context, artifact_id)
        .await
        .map_err(HttpError::from_api_error)?;
    artifact_content_response(content, ArtifactContentQuery::default()).await
}

/// Map a `File::open` failure for an already-validated artifact to an HTTP error.
///
/// The artifact service validated existence, so a `NotFound` here means the
/// file vanished in the TOCTOU window. Every other IO error is a server error.
fn open_artifact_error(error: &std::io::Error, artifact_id: &str) -> HttpError {
    if error.kind() == std::io::ErrorKind::NotFound {
        HttpError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("artifact not found: {artifact_id}"),
        )
    } else {
        HttpError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "read_error",
            format!("failed to open artifact: {error}"),
        )
    }
}

fn is_inline_content_type(content_type: &str) -> bool {
    matches!(
        content_type,
        "image/png" | "image/jpeg" | "image/gif" | "image/webp" | "image/avif"
    )
}

#[cfg(test)]
#[path = "artifacts_tests.rs"]
mod tests;
