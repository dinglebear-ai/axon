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

/// Classify a service error to (status, kind) using narrow message heuristics.
///
/// Conservative by design: this function fires AFTER per-handler validation
/// has already returned 400 for client-input errors (empty fields, missing
/// required body keys, invalid UUIDs, invalid time_range, etc.). The only
/// thing left to distinguish here is upstream-infrastructure failure (502)
/// versus internal-server error (500).
///
/// **Upstream detection first** so a service error containing both
/// "qdrant" and "invalid" (e.g. "qdrant returned invalid response") is
/// reported as upstream, not as bad_request.
///
/// **bad_request fires only on very narrow, request-shape-specific
/// markers** — broad words like "empty", "missing", "must be", or
/// "is required" are deliberately omitted because the service layer uses
/// them for config/setup errors that the client cannot fix (e.g.
/// "GITHUB_TOKEN must be set" or "point has empty dense vector"). Those
/// belong in the 500/internal bucket so monitoring dashboards do not
/// silently downgrade outages to 4xx.
pub(crate) fn classify_service_error(err: &(dyn Error + 'static)) -> (StatusCode, &'static str) {
    let mut buf = String::new();
    let mut cur: Option<&(dyn Error + 'static)> = Some(err);
    while let Some(e) = cur {
        buf.push_str(&e.to_string());
        buf.push('\n');
        cur = e.source();
    }
    let lc = buf.to_lowercase();

    // Upstream takes precedence over bad-request heuristics so misleading
    // strings like "qdrant returned invalid response" don't get downgraded.
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

    // Narrow client-input-shape markers — anything the service layer raises
    // about the request body itself (URL list, URL syntax). Service-side
    // config/validation errors ("must be set", "is required for") fall
    // through to internal so they show up in 5xx monitoring.
    if lc.contains("no urls provided") || lc.contains("invalid url") {
        return (StatusCode::BAD_REQUEST, "bad_request");
    }

    (StatusCode::INTERNAL_SERVER_ERROR, "internal")
}

pub(crate) fn map_service_error(err: &(dyn Error + 'static)) -> Response {
    let (status, kind) = classify_service_error(err);
    rest_error(status, kind, err.to_string())
}
