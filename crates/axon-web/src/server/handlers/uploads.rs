//! Authenticated staged-upload lifecycle routes.

use crate::server::error::HttpError;
use axon_api::source::{
    Page, UploadAbortRequest, UploadAbortResult, UploadCompleteRequest, UploadCompleteResult,
    UploadCreateRequest, UploadCreateResult, UploadId, UploadListRequest, UploadStatus,
};
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, header},
};
use base64::Engine as _;
use std::sync::Arc;

type WebState = (
    super::super::state::AppState,
    Arc<axon_core::config::Config>,
);

#[utoipa::path(
    post,
    path = "/v1/uploads",
    request_body = UploadCreateRequest,
    responses((status = 200, body = UploadCreateResult), (status = 400, body = crate::server::error::ErrorBody)),
    tag = "uploads"
)]
pub(crate) async fn create_upload(
    State((state, _cfg)): State<WebState>,
    super::super::json::Json(request): super::super::json::Json<UploadCreateRequest>,
) -> Result<super::super::json::Json<UploadCreateResult>, HttpError> {
    axon_services::uploads::create_upload(&state.service_context, request)
        .await
        .map(super::super::json::Json)
        .map_err(HttpError::from_api_error)
}

#[utoipa::path(
    get,
    path = "/v1/uploads",
    params(
        ("status" = Option<axon_api::source::UploadStatusKind>, Query),
        ("limit" = Option<u32>, Query),
        ("cursor" = Option<String>, Query)
    ),
    responses((status = 200, body = Page<UploadStatus>)),
    tag = "uploads"
)]
pub(crate) async fn list_uploads(
    State((state, _cfg)): State<WebState>,
    Query(request): Query<UploadListRequest>,
) -> Result<super::super::json::Json<Page<UploadStatus>>, HttpError> {
    axon_services::uploads::list_uploads(&state.service_context, request)
        .await
        .map(super::super::json::Json)
        .map_err(HttpError::from_api_error)
}

#[utoipa::path(
    get,
    path = "/v1/uploads/{upload_id}",
    params(("upload_id" = String, Path)),
    responses((status = 200, body = UploadStatus), (status = 404, body = crate::server::error::ErrorBody)),
    tag = "uploads"
)]
pub(crate) async fn get_upload(
    State((state, _cfg)): State<WebState>,
    Path(upload_id): Path<UploadId>,
) -> Result<super::super::json::Json<UploadStatus>, HttpError> {
    axon_services::uploads::get_upload(&state.service_context, upload_id)
        .await
        .map(super::super::json::Json)
        .map_err(HttpError::from_api_error)
}

#[utoipa::path(
    put,
    path = "/v1/uploads/{upload_id}/content",
    params(("upload_id" = String, Path)),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses((status = 200, body = UploadStatus), (status = 400, body = crate::server::error::ErrorBody)),
    tag = "uploads"
)]
pub(crate) async fn put_upload_content(
    State((state, _cfg)): State<WebState>,
    Path(upload_id): Path<UploadId>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Result<super::super::json::Json<UploadStatus>, HttpError> {
    let supplied_sha256 = upload_sha256_header(&headers)?;
    let supplied_content_type = headers
        .get(header::CONTENT_TYPE)
        .map(|value| value.to_str().map(str::to_string))
        .transpose()
        .map_err(|_| {
            HttpError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "upload.content_type_invalid",
                "content type header is invalid",
            )
        })?;
    axon_services::uploads::put_upload_content(
        &state.service_context,
        upload_id,
        bytes.to_vec(),
        supplied_content_type,
        supplied_sha256,
    )
    .await
    .map(super::super::json::Json)
    .map_err(HttpError::from_api_error)
}

#[utoipa::path(
    post,
    path = "/v1/uploads/{upload_id}/complete",
    params(("upload_id" = String, Path)),
    request_body = UploadCompleteRequest,
    responses((status = 200, body = UploadCompleteResult), (status = 400, body = crate::server::error::ErrorBody)),
    tag = "uploads"
)]
pub(crate) async fn complete_upload(
    State((state, _cfg)): State<WebState>,
    Path(upload_id): Path<UploadId>,
    super::super::json::Json(request): super::super::json::Json<UploadCompleteRequest>,
) -> Result<super::super::json::Json<UploadCompleteResult>, HttpError> {
    axon_services::uploads::complete_upload(&state.service_context, upload_id, request)
        .await
        .map(super::super::json::Json)
        .map_err(HttpError::from_api_error)
}

#[utoipa::path(
    delete,
    path = "/v1/uploads/{upload_id}",
    params(("upload_id" = String, Path)),
    request_body = UploadAbortRequest,
    responses((status = 200, body = UploadAbortResult), (status = 404, body = crate::server::error::ErrorBody)),
    tag = "uploads"
)]
pub(crate) async fn abort_upload(
    State((state, _cfg)): State<WebState>,
    Path(upload_id): Path<UploadId>,
    super::super::json::Json(request): super::super::json::Json<UploadAbortRequest>,
) -> Result<super::super::json::Json<UploadAbortResult>, HttpError> {
    axon_services::uploads::abort_upload(&state.service_context, upload_id, request)
        .await
        .map(super::super::json::Json)
        .map_err(HttpError::from_api_error)
}

fn upload_sha256_header(headers: &HeaderMap) -> Result<Option<String>, HttpError> {
    let value = headers
        .get("x-content-sha256")
        .or_else(|| headers.get("digest"));
    let Some(value) = value else {
        return Ok(None);
    };
    let raw = value.to_str().map_err(|_| {
        HttpError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "upload.sha256_invalid",
            "upload hash header is not valid ASCII",
        )
    })?;
    let Some(encoded) = raw.strip_prefix("sha-256=") else {
        return Ok(Some(raw.to_string()));
    };
    let encoded = encoded.trim_matches(':');
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| {
            HttpError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "upload.sha256_invalid",
                "Digest sha-256 value is not valid base64",
            )
        })?;
    if bytes.len() != 32 {
        return Err(HttpError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "upload.sha256_invalid",
            "Digest sha-256 value must decode to 32 bytes",
        ));
    }
    Ok(Some(
        bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
    ))
}

#[cfg(test)]
#[path = "uploads_tests.rs"]
mod tests;
