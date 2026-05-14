use super::super::state::AppState;
use super::super::types::{LoginRequest, LoginResponse, StateResponse};
use crate::core::config::Config;
use axum::{Json, extract::State, response::IntoResponse};
use std::sync::Arc;

async fn panel_state(State((state, _)): State<(AppState, Arc<Config>)>) -> Json<StateResponse> {
    Json(StateResponse {
        setup_required: state.panel.setup_required,
        config_path: state.panel.config_path.clone(),
    })
}

async fn login(
    State((state, _)): State<(AppState, Arc<Config>)>,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
    if state.panel.password.verify(&req.password) {
        Json(LoginResponse {
            ok: true,
            token: Some(state.panel.password.as_str().to_string()),
        })
    } else {
        Json(LoginResponse {
            ok: false,
            token: None,
        })
    }
}

pub use login;
pub use panel_state;
