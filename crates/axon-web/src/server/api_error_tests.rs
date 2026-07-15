use super::*;
use axon_error::ErrorVisibility;

#[test]
fn validation_kind_maps_to_route_validation_code() {
    let err =
        api_error_from_status_kind(StatusCode::BAD_REQUEST, "bad_request", "source is required");
    assert_eq!(err.code.0, "route.validation.invalid_field");
    assert_eq!(err.stage, ErrorStage::Validation);
    assert!(!err.retryable);
    assert_eq!(err.severity, ErrorSeverity::Failed);
    assert_eq!(err.visibility, ErrorVisibility::Public);
    assert_eq!(status_for_api_error(&err), StatusCode::BAD_REQUEST);
}

#[test]
fn unauthorized_kind_maps_to_auth_missing() {
    let err = api_error_from_status_kind(StatusCode::UNAUTHORIZED, "unauthorized", "unauthorized");
    assert_eq!(err.code.0, "auth.missing");
    assert_eq!(err.stage, ErrorStage::Authorizing);
    assert_eq!(status_for_api_error(&err), StatusCode::UNAUTHORIZED);
}

#[test]
fn forbidden_kind_maps_to_auth_forbidden() {
    let err = api_error_from_status_kind(StatusCode::FORBIDDEN, "forbidden", "requires scope");
    assert_eq!(err.code.0, "auth.forbidden");
    assert_eq!(status_for_api_error(&err), StatusCode::FORBIDDEN);
}

#[test]
fn watch_not_found_maps_to_not_found() {
    let err = ApiError::new("watch.not_found", ErrorStage::Retrieving, "watch missing");
    assert_eq!(status_for_api_error(&err), StatusCode::NOT_FOUND);
}

#[test]
fn rate_limited_kind_is_retryable_provider_error() {
    let err = api_error_from_status_kind(
        StatusCode::TOO_MANY_REQUESTS,
        "rate_limited",
        "rate limited",
    );
    assert_eq!(err.code.0, "provider.rate_limited");
    assert!(err.retryable);
    assert_eq!(status_for_api_error(&err), StatusCode::TOO_MANY_REQUESTS);
}

#[test]
fn upstream_kind_maps_to_provider_unavailable_bad_gateway() {
    let err = api_error_from_status_kind(
        StatusCode::BAD_GATEWAY,
        "upstream",
        "upstream service unavailable",
    );
    assert_eq!(err.code.0, "provider.unavailable");
    assert!(err.retryable);
    assert_eq!(status_for_api_error(&err), StatusCode::BAD_GATEWAY);
}

#[test]
fn internal_kind_maps_to_internal_server_error() {
    let err = api_error_from_status_kind(StatusCode::INTERNAL_SERVER_ERROR, "internal", "boom");
    assert_eq!(err.code.0, "internal.server_error");
    assert_eq!(
        status_for_api_error(&err),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn unknown_kind_falls_back_to_status_class() {
    let err = api_error_from_status_kind(StatusCode::NOT_FOUND, "totally_unknown", "nope");
    assert_eq!(err.code.0, "route.not_found");
    assert_eq!(status_for_api_error(&err), StatusCode::NOT_FOUND);
}

#[test]
fn envelope_response_carries_status_and_shape() {
    let err =
        api_error_from_status_kind(StatusCode::BAD_REQUEST, "bad_request", "source is required");
    let response = error_envelope_response(err);
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn redact_api_error_scrubs_message_and_details() {
    let mut err = api_error_from_status_kind(
        StatusCode::BAD_GATEWAY,
        "upstream_unavailable",
        "upstream failed: Authorization: Bearer abcdef0123456789abcdef",
    );
    err.details.insert(
        "cause".to_string(),
        "connection string had sk-proj-abcdefghijklmnopqrstuvwx".to_string(),
    );

    let redacted = redact_api_error(err);

    assert!(!redacted.message.contains("abcdef0123456789abcdef"));
    assert!(
        !redacted.details["cause"].contains("abcdefghijklmnopqrstuvwx"),
        "details value should be redacted: {}",
        redacted.details["cause"]
    );
}
