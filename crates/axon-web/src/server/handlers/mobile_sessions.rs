use super::super::error::HttpError;
use axon_services::mobile_sessions::{
    DeleteMobileSessionResponse, MobileSessionDetailResponse, MobileSessionError,
    MobileSessionListResponse, UpsertMobileSessionRequest, UpsertMobileSessionResponse,
};
use axum::{Extension, Json, extract::Path, http::StatusCode};
use lab_auth::AuthContext;

#[utoipa::path(
    get,
    path = "/v1/mobile/sessions",
    responses(
        (status = 200, description = "Mobile chat sessions", body = MobileSessionListResponse),
        (status = 500, description = "Session store error", body = crate::server::error::ErrorBody)
    ),
    tag = "mobile"
)]
pub async fn list_mobile_sessions(
    auth: Option<Extension<AuthContext>>,
) -> Result<Json<MobileSessionListResponse>, HttpError> {
    let owner = mobile_session_owner(auth.as_ref().map(|Extension(auth)| auth));
    axon_services::mobile_sessions::list_sessions(&owner)
        .await
        .map(Json)
        .map_err(map_mobile_session_error)
}

#[utoipa::path(
    get,
    path = "/v1/mobile/sessions/{id}",
    params(("id" = String, Path, description = "Mobile session id")),
    responses(
        (status = 200, description = "Mobile chat session", body = MobileSessionDetailResponse),
        (status = 404, description = "Session not found", body = crate::server::error::ErrorBody)
    ),
    tag = "mobile"
)]
pub async fn get_mobile_session(
    auth: Option<Extension<AuthContext>>,
    Path(id): Path<String>,
) -> Result<Json<MobileSessionDetailResponse>, HttpError> {
    let owner = mobile_session_owner(auth.as_ref().map(|Extension(auth)| auth));
    axon_services::mobile_sessions::get_session(&owner, &id)
        .await
        .map(Json)
        .map_err(map_mobile_session_error)
}

#[utoipa::path(
    put,
    path = "/v1/mobile/sessions/{id}",
    params(("id" = String, Path, description = "Mobile session id")),
    request_body = UpsertMobileSessionRequest,
    responses(
        (status = 200, description = "Upserted mobile chat session", body = UpsertMobileSessionResponse),
        (status = 400, description = "Invalid session payload", body = crate::server::error::ErrorBody),
        (status = 409, description = "Stale mobile session update", body = crate::server::error::ErrorBody)
    ),
    tag = "mobile"
)]
pub async fn upsert_mobile_session(
    auth: Option<Extension<AuthContext>>,
    Path(id): Path<String>,
    Json(request): Json<UpsertMobileSessionRequest>,
) -> Result<Json<UpsertMobileSessionResponse>, HttpError> {
    let owner = mobile_session_owner(auth.as_ref().map(|Extension(auth)| auth));
    axon_services::mobile_sessions::upsert_session(&owner, &id, request)
        .await
        .map(Json)
        .map_err(map_mobile_session_error)
}

#[utoipa::path(
    delete,
    path = "/v1/mobile/sessions/{id}",
    params(("id" = String, Path, description = "Mobile session id")),
    responses(
        (status = 200, description = "Deleted mobile chat session", body = DeleteMobileSessionResponse),
        (status = 500, description = "Session store error", body = crate::server::error::ErrorBody)
    ),
    tag = "mobile"
)]
pub async fn delete_mobile_session(
    auth: Option<Extension<AuthContext>>,
    Path(id): Path<String>,
) -> Result<Json<DeleteMobileSessionResponse>, HttpError> {
    let owner = mobile_session_owner(auth.as_ref().map(|Extension(auth)| auth));
    axon_services::mobile_sessions::delete_session(&owner, &id)
        .await
        .map(Json)
        .map_err(map_mobile_session_error)
}

fn mobile_session_owner(auth: Option<&AuthContext>) -> String {
    auth.and_then(|auth| {
        auth.actor_key
            .as_deref()
            .map(ToOwned::to_owned)
            .or_else(|| auth.email.clone())
            .or_else(|| Some(format!("{}:{}", auth.issuer, auth.sub)))
    })
    .unwrap_or_else(|| "loopback-dev".to_string())
}

fn map_mobile_session_error(err: MobileSessionError) -> HttpError {
    match err {
        MobileSessionError::InvalidId
        | MobileSessionError::InvalidSession(_)
        | MobileSessionError::IdMismatch => HttpError::bad_request(err.to_string()),
        MobileSessionError::NotFound => {
            HttpError::new(StatusCode::NOT_FOUND, "not_found", err.to_string())
        }
        MobileSessionError::StaleUpdate => {
            HttpError::new(StatusCode::CONFLICT, "stale_update", err.to_string())
        }
        MobileSessionError::Io(_) | MobileSessionError::Json(_) => HttpError::from_error(&err),
    }
}
