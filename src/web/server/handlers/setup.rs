use super::super::state::AppState;
use super::super::utils::authorized;
use crate::core::config::Config;
use crate::services::setup;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use std::sync::Arc;

pub async fn setup_targets(
    State((state, _)): State<(AppState, Arc<Config>)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    match setup::list_ssh_targets() {
        Ok(targets) => Json(targets).into_response(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Json(Vec::<setup::SshTarget>::new()).into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

pub async fn setup_deploy(
    State((state, _)): State<(AppState, Arc<Config>)>,
    headers: HeaderMap,
    Json(req): Json<setup::DeployRequest>,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    match setup::deploy_remote(req).await {
        Ok(result) => Json(result).into_response(),
        Err(err) => (StatusCode::BAD_GATEWAY, err.to_string()).into_response(),
    }
}
