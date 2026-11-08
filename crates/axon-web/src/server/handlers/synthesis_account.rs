//! Authenticated admin projection over the synthesis-account service.
use super::super::{api_error::error_envelope_response_with_status, json::Json, state::AppState};
use axon_api::ApiError;
use axon_core::config::Config;
use axon_error::ErrorStage;
use axon_services::synthesis_account::AccountStatus;
use axum::{extract::State, http::StatusCode, response::Response};
use serde::Deserialize;
use std::sync::Arc;
use utoipa::ToSchema;

type WebState = (AppState, Arc<Config>);

pub async fn status(State((state, _)): State<WebState>) -> Result<Json<AccountStatus>, Response> {
    state
        .synthesis_account
        .status()
        .await
        .map(Json)
        .map_err(error)
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LoginBody {
    switch_account: bool,
}

pub async fn start(
    State((state, _)): State<WebState>,
    Json(body): Json<LoginBody>,
) -> Result<Json<AccountStatus>, Response> {
    state
        .synthesis_account
        .start(body.switch_account)
        .await
        .map(Json)
        .map_err(error)
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CancelBody {
    login_id: String,
}

pub async fn cancel(
    State((state, _)): State<WebState>,
    Json(body): Json<CancelBody>,
) -> Result<Json<AccountStatus>, Response> {
    state
        .synthesis_account
        .cancel(&body.login_id)
        .await
        .map(Json)
        .map_err(error)
}

fn error(message: String) -> Response {
    let code = match message.split(':').next().unwrap_or_default() {
        "synthesis_account_busy" => "synthesis_account_busy",
        "synthesis_account_login_unknown" => "synthesis_account_login_unknown",
        "synthesis_account_confirmation_required" => "synthesis_account_confirmation_required",
        "synthesis_account_unavailable" => "synthesis_account_unavailable",
        "synthesis_account_cleanup_failed" => "synthesis_account_cleanup_failed",
        "synthesis_account_login_start_failed" => "synthesis_account_login_start_failed",
        "synthesis_account_unsupported" => "synthesis_account_unsupported",
        "synthesis_account_home_required" => "synthesis_account_home_required",
        "synthesis_account_protocol_mismatch" => "synthesis_account_protocol_mismatch",
        "synthesis_account_read_failed" => "synthesis_account_read_failed",
        "synthesis_account_owner_failed" => "synthesis_account_owner_failed",
        _ => "synthesis_account_error",
    };
    let status = if code == "synthesis_account_busy" {
        StatusCode::CONFLICT
    } else if code == "synthesis_account_login_unknown" {
        StatusCode::NOT_FOUND
    } else if code == "synthesis_account_confirmation_required" {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let stage = match status {
        StatusCode::BAD_REQUEST => ErrorStage::Validation,
        StatusCode::NOT_FOUND => ErrorStage::Resolving,
        StatusCode::CONFLICT => ErrorStage::Leasing,
        _ => ErrorStage::Synthesizing,
    };
    error_envelope_response_with_status(ApiError::new(code, stage, message), status)
}

#[utoipa::path(get, path = "/v1/synthesis/account", responses((status = 200, body = AccountStatus), (status = 409, description = "Generation active", body = axon_api::source::ErrorEnvelope), (status = 503, description = "Account unavailable", body = axon_api::source::ErrorEnvelope)), tag = "codex-control")]
#[allow(dead_code)]
pub async fn status_openapi_marker() {}

#[utoipa::path(post, path = "/v1/synthesis/account/login", request_body = LoginBody, responses((status = 200, body = AccountStatus), (status = 400, description = "Invalid request or missing account-switch confirmation", body = axon_api::source::ErrorEnvelope), (status = 409, description = "Generation or sign-in active", body = axon_api::source::ErrorEnvelope), (status = 415, description = "Unsupported content type", body = axon_api::source::ErrorEnvelope), (status = 422, description = "Invalid request body", body = axon_api::source::ErrorEnvelope), (status = 503, description = "Account unavailable", body = axon_api::source::ErrorEnvelope)), tag = "codex-control")]
#[allow(dead_code)]
pub async fn start_openapi_marker() {}

#[utoipa::path(post, path = "/v1/synthesis/account/login/cancel", request_body = CancelBody, responses((status = 200, body = AccountStatus), (status = 400, description = "Invalid request body", body = axon_api::source::ErrorEnvelope), (status = 404, description = "Unknown sign-in", body = axon_api::source::ErrorEnvelope), (status = 415, description = "Unsupported content type", body = axon_api::source::ErrorEnvelope), (status = 422, description = "Invalid request body", body = axon_api::source::ErrorEnvelope), (status = 503, description = "Account unavailable", body = axon_api::source::ErrorEnvelope)), tag = "codex-control")]
#[allow(dead_code)]
pub async fn cancel_openapi_marker() {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthesis_account_error_contracts_match_runtime_envelope() {
        let document =
            serde_json::to_value(super::super::super::openapi::openapi_document()).unwrap();
        for (path, method, errors) in [
            ("/v1/synthesis/account", "get", &["409", "503"][..]),
            (
                "/v1/synthesis/account/login",
                "post",
                &["400", "409", "415", "422", "503"][..],
            ),
            (
                "/v1/synthesis/account/login/cancel",
                "post",
                &["400", "404", "415", "422", "503"][..],
            ),
        ] {
            let route = crate::schema_registry::rest_route_registry()
                .iter()
                .find(|route| route.path == path)
                .unwrap();
            assert_eq!(route.required_scope, "admin");
            for status in errors {
                assert!(route.responses.contains(status), "{path}: {status}");
                assert_eq!(
                    document["paths"][path][method]["responses"][*status]["content"]["application/json"]
                        ["schema"]["$ref"],
                    "#/components/schemas/ErrorEnvelope",
                    "{path}: {status} must describe the actual error envelope",
                );
            }
        }
        let properties = &document["components"]["schemas"]["ErrorEnvelope"]["properties"];
        assert!(properties.get("ok").is_some());
        assert!(properties.get("error").is_some());
        assert!(properties.get("kind").is_none());
    }

    #[tokio::test]
    async fn synthesis_account_errors_keep_their_codes_and_statuses() {
        for (code, status) in [
            ("synthesis_account_busy", 409),
            ("synthesis_account_login_unknown", 404),
            ("synthesis_account_confirmation_required", 400),
            ("synthesis_account_unavailable", 503),
            ("synthesis_account_cleanup_failed", 503),
            ("synthesis_account_login_start_failed", 503),
            ("synthesis_account_unsupported", 503),
            ("synthesis_account_home_required", 503),
            ("synthesis_account_protocol_mismatch", 503),
            ("synthesis_account_read_failed", 503),
            ("synthesis_account_owner_failed", 503),
            ("unexpected_provider_error", 503),
        ] {
            let response = error(format!("{code}: safe explanation"));
            assert_eq!(response.status().as_u16(), status, "{code}");
            let bytes = axum::body::to_bytes(response.into_body(), 16_384)
                .await
                .unwrap();
            let envelope: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let expected = if code == "unexpected_provider_error" {
                "synthesis_account_error"
            } else {
                code
            };
            assert_eq!(envelope["ok"], false);
            assert_eq!(envelope["error"]["code"], expected);
        }
    }

    #[test]
    fn login_requires_explicit_intent_and_accepts_no_credentials() {
        assert!(serde_json::from_value::<LoginBody>(serde_json::json!({})).is_err());
        assert!(
            serde_json::from_value::<LoginBody>(
                serde_json::json!({"switch_account":true,"token":"secret"})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<LoginBody>(serde_json::json!({"switch_account":true})).is_ok()
        );
    }
}
