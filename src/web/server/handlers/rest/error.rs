//! Shared error mapping for REST handlers.

use axum::{Json, http::StatusCode, response::IntoResponse, response::Response};
use serde::Serialize;
use std::error::Error;

/// Wire-format error body used by every dedicated REST route.
#[derive(Serialize)]
pub(crate) struct RestErrorBody {
    pub kind: &'static str,
    pub message: String,
}

pub(crate) fn rest_error(status: StatusCode, kind: &'static str, message: String) -> Response {
    (status, Json(RestErrorBody { kind, message })).into_response()
}

/// Classify a service error to (status, kind) using simple message heuristics.
///
/// Mirrors `handlers::ask::classify_ask_error` but is generic across REST
/// surfaces. Treats Qdrant/TEI/timeout markers as upstream (502) and
/// "invalid"/"required"/"missing" markers as bad-request (400). Defaults to
/// 500/internal.
pub(crate) fn classify_service_error(err: &(dyn Error + 'static)) -> (StatusCode, &'static str) {
    let mut buf = String::new();
    let mut cur: Option<&(dyn Error + 'static)> = Some(err);
    while let Some(e) = cur {
        buf.push_str(&e.to_string());
        buf.push('\n');
        cur = e.source();
    }
    let lc = buf.to_lowercase();
    if lc.contains("invalid")
        || lc.contains("missing required")
        || lc.contains("must be")
        || lc.contains("is required")
        || lc.contains("no urls")
        || lc.contains("empty")
    {
        return (StatusCode::BAD_REQUEST, "bad_request");
    }
    if lc.contains("qdrant")
        || lc.contains("tei")
        || lc.contains("connection refused")
        || lc.contains("upstream")
        || lc.contains("timed out")
        || lc.contains("timeout")
        || lc.contains("dns")
        || lc.contains("502")
        || lc.contains("503")
    {
        return (StatusCode::BAD_GATEWAY, "upstream");
    }
    (StatusCode::INTERNAL_SERVER_ERROR, "internal")
}

pub(crate) fn map_service_error(err: &(dyn Error + 'static)) -> Response {
    let (status, kind) = classify_service_error(err);
    rest_error(status, kind, err.to_string())
}
