//! Shared error mapping for REST handlers.

use crate::server::api_error::{api_error_from_status_kind, error_envelope_response_with_status};
use axum::{http::StatusCode, response::Response};
use std::error::Error;

/// Render a `(status, kind, message)` triple as the contract `ErrorEnvelope`.
///
/// The legacy `(status, kind)` classification is projected onto the shared
/// `axon_api::ApiError` taxonomy so this boundary emits the same envelope shape
/// as [`crate::server::error::HttpError`].
pub(crate) fn rest_error(status: StatusCode, kind: &'static str, message: String) -> Response {
    let api_error = api_error_from_status_kind(status, kind, message);
    error_envelope_response_with_status(api_error, status)
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
    if lc.contains("429")
        || lc.contains("rate limit")
        || lc.contains("rate-limited")
        || lc.contains("too many requests")
        || lc.contains("usage limit")
        || lc.contains("quota")
        || lc.contains("resource exhausted")
    {
        return (StatusCode::TOO_MANY_REQUESTS, "rate_limited");
    }

    if lc.contains("qdrant")
        || lc.contains("tei")
        || lc.contains("llm")
        || lc.contains("gemini")
        || lc.contains("codex app-server")
        || lc.contains("openai")
        || lc.contains("completion")
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
    // about the shape of the request body itself (URL list, URL syntax,
    // cursor pagination token). Service-side config/validation errors
    // ("must be set", "is required for") fall through to internal so they
    // show up in 5xx monitoring instead of being silently downgraded to 4xx.
    //
    // Keep this list narrow and additive: every marker added here is a
    // commitment that the service layer will only emit it for genuine
    // client-input failures, never for upstream or config issues.
    let client_input_markers = [
        "no urls provided",
        "invalid url",
        "invalid scrape url",
        "invalid retrieve cursor",
        "invalid cursor",
        "invalid request",
        "malformed url",
        "unsupported scheme",
    ];
    if client_input_markers.iter().any(|m| lc.contains(m)) {
        return (StatusCode::BAD_REQUEST, "bad_request");
    }

    (StatusCode::INTERNAL_SERVER_ERROR, "internal")
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;

pub(crate) fn map_service_error(err: &(dyn Error + 'static)) -> Response {
    let (status, kind) = classify_service_error(err);
    if status.is_server_error() {
        tracing::error!(status = status.as_u16(), kind, error = %err, "handler error");
    } else if status == StatusCode::TOO_MANY_REQUESTS {
        tracing::warn!(status = status.as_u16(), kind, error = %err, "handler error");
    }
    rest_error(status, kind, err.to_string())
}
