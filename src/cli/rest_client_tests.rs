use super::rest_client::{ServerFailureClass, classify_server_status};
use reqwest::StatusCode;

#[test]
fn gateway_unavailable_allows_fallback() {
    assert_eq!(
        classify_server_status(StatusCode::BAD_GATEWAY, ""),
        ServerFailureClass::TransportUnavailable
    );
    assert_eq!(
        classify_server_status(StatusCode::SERVICE_UNAVAILABLE, ""),
        ServerFailureClass::TransportUnavailable
    );
}

#[test]
fn auth_and_schema_errors_do_not_allow_silent_fallback() {
    assert_eq!(
        classify_server_status(StatusCode::UNAUTHORIZED, ""),
        ServerFailureClass::PolicyFailure
    );
    assert_eq!(
        classify_server_status(StatusCode::UPGRADE_REQUIRED, "schema mismatch"),
        ServerFailureClass::SchemaMismatch
    );
}
