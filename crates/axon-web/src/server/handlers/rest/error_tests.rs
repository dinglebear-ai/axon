use super::classify_service_error;
use axum::http::StatusCode;
use std::fmt;

/// A simple error type for testing that wraps a static string.
#[derive(Debug)]
struct StrError(&'static str);
impl fmt::Display for StrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for StrError {}

fn check(msg: &'static str) -> (StatusCode, &'static str) {
    let err = StrError(msg);
    classify_service_error(&err)
}

#[test]
fn qdrant_error_classified_as_upstream() {
    let (status, kind) = check("qdrant connection refused");
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(kind, "upstream");
}

#[test]
fn tei_error_classified_as_upstream() {
    let (status, kind) = check("tei batch returned 503");
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(kind, "upstream");
}

#[test]
fn connection_refused_classified_as_upstream() {
    let (status, kind) = check("connection refused to axon-qdrant:6333");
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(kind, "upstream");
}

#[test]
fn timeout_classified_as_upstream() {
    let (status, kind) = check("request timed out after 30s");
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(kind, "upstream");
}

#[test]
fn invalid_url_classified_as_bad_request() {
    let (status, kind) = check("invalid scrape url: private IP rejected");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(kind, "bad_request");
}

#[test]
fn invalid_cursor_classified_as_bad_request() {
    let (status, kind) = check("invalid retrieve cursor: malformed base64");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(kind, "bad_request");
}

/// Broad strings like "must be set" come from config errors (not client input)
/// and must NOT be downgraded to 400 — they should stay 500 so monitoring
/// sees them as server errors.
#[test]
fn config_error_must_be_set_stays_internal() {
    let (status, kind) = check("GITHUB_TOKEN must be set");
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(kind, "internal");
}

#[test]
fn config_error_is_required_stays_internal() {
    let (status, kind) = check("TAVILY_API_KEY is required for search");
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(kind, "internal");
}

/// "empty" is a very broad word — should NOT route to 400 since many
/// server-side errors mention empty fields (e.g. "point has empty dense vector").
#[test]
fn empty_dense_vector_stays_internal() {
    let (status, kind) = check("point has empty dense vector");
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(kind, "internal");
}

/// "qdrant returned invalid response" must be upstream (502), not 400,
/// even though it contains "invalid". Upstream check runs first.
#[test]
fn qdrant_invalid_response_is_upstream_not_bad_request() {
    let (status, kind) = check("qdrant returned invalid response body");
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(kind, "upstream");
}

#[test]
fn unknown_error_classified_as_internal() {
    let (status, kind) = check("unexpected panic in worker thread");
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(kind, "internal");
}
