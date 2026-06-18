use super::super::error::HttpError;
use crate::services::mobile_sessions::{
    DeleteMobileSessionResponse, MobileSessionDetailResponse, MobileSessionError,
    MobileSessionListResponse, UpsertMobileSessionRequest, UpsertMobileSessionResponse,
};
use axum::{Json, extract::Path, http::StatusCode};

pub async fn list_mobile_sessions() -> Result<Json<MobileSessionListResponse>, HttpError> {
    crate::services::mobile_sessions::list_sessions()
        .await
        .map(Json)
        .map_err(map_mobile_session_error)
}

pub async fn get_mobile_session(
    Path(id): Path<String>,
) -> Result<Json<MobileSessionDetailResponse>, HttpError> {
    crate::services::mobile_sessions::get_session(&id)
        .await
        .map(Json)
        .map_err(map_mobile_session_error)
}

pub async fn upsert_mobile_session(
    Path(id): Path<String>,
    Json(request): Json<UpsertMobileSessionRequest>,
) -> Result<Json<UpsertMobileSessionResponse>, HttpError> {
    crate::services::mobile_sessions::upsert_session(&id, request)
        .await
        .map(Json)
        .map_err(map_mobile_session_error)
}

pub async fn delete_mobile_session(
    Path(id): Path<String>,
) -> Result<Json<DeleteMobileSessionResponse>, HttpError> {
    crate::services::mobile_sessions::delete_session(&id)
        .await
        .map(Json)
        .map_err(map_mobile_session_error)
}

fn map_mobile_session_error(err: MobileSessionError) -> HttpError {
    match err {
        MobileSessionError::InvalidId | MobileSessionError::IdMismatch => {
            HttpError::bad_request(err.to_string())
        }
        MobileSessionError::NotFound => {
            HttpError::new(StatusCode::NOT_FOUND, "not_found", err.to_string())
        }
        MobileSessionError::Io(_) | MobileSessionError::Json(_) => HttpError::from_error(&err),
    }
}
