//! REST error boundary: render the contract [`ErrorEnvelope`] over [`ApiError`].
//!
//! Per `docs/pipeline-unification/runtime/error-handling.md` and
//! `docs/pipeline-unification/surfaces/rest-contract.md`, every REST failure
//! serializes as the shared [`ErrorEnvelope`] whose `error` field is the
//! transport-neutral [`ApiError`] shape (`code`, `stage`, `retryable`,
//! `severity`, `visibility`, `details`, …). The HTTP status is derived from the
//! error's code family / stage rather than from ad-hoc string heuristics.
//!
//! This module is the single place the two REST error boundaries
//! ([`super::error::HttpError`] and [`super::handlers::rest::error::rest_error`])
//! funnel through, so both transports emit one shape.

use axon_api::ApiError;
use axon_api::source::{ErrorEnvelope, TraceContext};
use axon_core::redact::{DefaultRedactor, RedactionContext, Redactor};
use axon_error::{ErrorSeverity, ErrorStage};
use axum::{
    Json,
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};

/// Contract version stamped into every REST envelope. Matches the value used by
/// the MCP tool-schema golden and the rest-contract document.
pub(crate) const CONTRACT_VERSION: &str = "2026-06-30";

/// Marker header stamped on every response this module builds. The router's
/// `jsonize_auth_error` normalizer uses it to skip responses that are already a
/// contract `ErrorEnvelope` (e.g. a handler's richer per-source `auth.forbidden`
/// carrying `required_scope`), so it only rewrites bare auth-layer 401/403s.
/// Stripped before the response leaves the server.
pub(crate) const ERROR_ENVELOPE_MARKER: &str = "x-axon-error-envelope";

/// Build the contract [`ErrorEnvelope`] response for an [`ApiError`].
///
/// The HTTP status is derived from the error via [`status_for_api_error`]; a
/// fresh `request_id`/`trace_id` are minted here because axon-web does not yet
/// carry request-scoped correlation ids (a follow-up plumbs those through).
pub(crate) fn error_envelope_response(error: ApiError) -> Response {
    let status = status_for_api_error(&error);
    error_envelope_response_with_status(error, status)
}

/// Build the contract [`ErrorEnvelope`] response, forcing a specific HTTP
/// status (used where the transport already decided the status, e.g. auth
/// middleware 401/403).
pub(crate) fn error_envelope_response_with_status(error: ApiError, status: StatusCode) -> Response {
    let error = redact_api_error(error);
    let envelope = ErrorEnvelope {
        ok: false,
        contract_version: CONTRACT_VERSION.to_string(),
        error,
        request_id: new_correlation_id("req"),
        trace: TraceContext {
            trace_id: new_correlation_id("trace"),
            span_id: None,
            parent_span_id: None,
            sampled: false,
            attributes: Default::default(),
        },
    };
    let mut response = (status, Json(envelope)).into_response();
    response
        .headers_mut()
        .insert(ERROR_ENVELOPE_MARKER, HeaderValue::from_static("1"));
    response
}

fn new_correlation_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}

/// Fail-closed redaction boundary: `error.message`/`error.details` may embed
/// an underlying cause chain (connection strings, file paths, provider
/// response bodies) that must not reach an untrusted REST caller verbatim.
fn redact_api_error(mut error: ApiError) -> ApiError {
    let redactor = DefaultRedactor::new();
    let context = RedactionContext::transport_response();
    error.message = redactor.redact_text(&error.message, &context);
    for value in error.details.values_mut() {
        *value = redactor.redact_text(value, &context);
    }
    error
}

/// Derive the HTTP status for an [`ApiError`] from its code family, stage, and
/// severity, following the "HTTP mapping" table in the error-handling contract.
pub(crate) fn status_for_api_error(error: &ApiError) -> StatusCode {
    let code = error.code.0.as_str();
    let category = error.code.category();

    // Fatal errors that never leave the process safely map to 500.
    if error.severity == ErrorSeverity::Fatal && category == "redaction" {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    // Code-family driven mapping (most specific first).
    match category {
        "command" | "action" | "route" => match code {
            "route.not_found" => StatusCode::NOT_FOUND,
            "route.method_not_allowed" => StatusCode::METHOD_NOT_ALLOWED,
            "route.removed" => StatusCode::GONE,
            _ => StatusCode::BAD_REQUEST,
        },
        "auth" => match code {
            "auth.missing" | "auth.unauthenticated" => StatusCode::UNAUTHORIZED,
            _ => StatusCode::FORBIDDEN,
        },
        "watch" => match code {
            "watch.not_found" => StatusCode::NOT_FOUND,
            _ => status_from_stage(error.stage),
        },
        "upload" => match code {
            "upload.not_found" => StatusCode::NOT_FOUND,
            "upload.expired" => StatusCode::GONE,
            "upload.busy" | "upload.not_writable" | "upload.incomplete" => StatusCode::CONFLICT,
            "upload.too_large" => StatusCode::PAYLOAD_TOO_LARGE,
            "upload.write_failed"
            | "upload.read_failed"
            | "upload.delete_failed"
            | "upload.cleanup_failed"
            | "upload.lock_failed"
            | "upload.invalid_record" => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::BAD_REQUEST,
        },
        "artifact" => match code {
            "artifact.not_found" => StatusCode::NOT_FOUND,
            "artifact.invalid_id" => StatusCode::BAD_REQUEST,
            // read_failed / list_failed / invalid_manifest are store faults.
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        },
        "provider" => match code {
            "provider.rate_limited" | "provider.cooling" => StatusCode::TOO_MANY_REQUESTS,
            _ => StatusCode::BAD_GATEWAY,
        },
        "embedding" | "vector" => StatusCode::BAD_GATEWAY,
        _ => status_from_stage(error.stage),
    }
}

/// Fall back to a status derived from the pipeline stage when the code family
/// does not pin one.
fn status_from_stage(stage: ErrorStage) -> StatusCode {
    match stage {
        ErrorStage::Parsing | ErrorStage::Validation => StatusCode::BAD_REQUEST,
        ErrorStage::Authorizing => StatusCode::UNAUTHORIZED,
        ErrorStage::Resolving | ErrorStage::Routing => StatusCode::UNPROCESSABLE_ENTITY,
        ErrorStage::Fetching | ErrorStage::Rendering | ErrorStage::Synthesizing => {
            StatusCode::BAD_GATEWAY
        }
        ErrorStage::Leasing => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// Map a legacy `(status, kind)` classification to a contract [`ApiError`].
///
/// This bridges the existing REST classifiers (which still produce an HTTP
/// status + a snake_case `kind`) onto the shared taxonomy so callers that have
/// not yet been ported to construct an [`ApiError`] directly still emit the
/// contract shape. `retryable`/`severity`/`visibility` are derived from the
/// resulting [`ErrorCode`]'s classification.
pub(crate) fn api_error_from_status_kind(
    status: StatusCode,
    kind: &str,
    message: impl Into<String>,
) -> ApiError {
    let (code, stage) = code_and_stage_for(status, kind);
    ApiError::new(code, stage, message)
}

/// Translate a legacy `(status, kind)` pair to a contract `(ErrorCode, stage)`.
fn code_and_stage_for(status: StatusCode, kind: &str) -> (&'static str, ErrorStage) {
    // `kind` is the most specific signal; fall back to the status class.
    match kind {
        "unauthorized" => ("auth.missing", ErrorStage::Authorizing),
        "forbidden" => ("auth.forbidden", ErrorStage::Authorizing),
        "not_found" | "vertical_target_not_found" => {
            ("source.acquire.not_found", ErrorStage::Fetching)
        }
        "invalid_url" | "malformed_url" | "unsupported_scheme" => {
            ("source.resolve.invalid_uri", ErrorStage::Resolving)
        }
        "bad_request"
        | "invalid_path"
        | "path_error"
        | "path_escape"
        | "structured_data_malformed"
        | "vertical_unsupported_url" => ("route.validation.invalid_field", ErrorStage::Validation),
        "unsupported_media_type" => ("route.validation.unsupported_media", ErrorStage::Validation),
        "payload_too_large" => ("output.too_large", ErrorStage::Validation),
        "rate_limited" | "vertical_rate_limited" => ("provider.rate_limited", ErrorStage::Fetching),
        "timeout" => ("provider.unavailable", ErrorStage::Fetching),
        "upstream"
        | "upstream_unavailable"
        | "bad_gateway"
        | "challenge_detected"
        | "vertical_blocked_antibot"
        | "vertical_target_unavailable"
        | "ladder_exhausted" => ("provider.unavailable", ErrorStage::Fetching),
        "vertical_auth_missing" => ("auth.credentials_missing", ErrorStage::Authorizing),
        "vertical_auth_invalid" => ("auth.credentials_invalid", ErrorStage::Authorizing),
        "read_error" | "output_dir_error" => ("artifact.write_failed", ErrorStage::Publishing),
        "symlink_not_allowed" => ("route.validation.invalid_field", ErrorStage::Validation),
        "internal" => ("internal.server_error", ErrorStage::Observing),
        _ => code_and_stage_from_status(status),
    }
}

fn code_and_stage_from_status(status: StatusCode) -> (&'static str, ErrorStage) {
    match status {
        StatusCode::BAD_REQUEST => ("route.validation.invalid_field", ErrorStage::Validation),
        StatusCode::UNAUTHORIZED => ("auth.missing", ErrorStage::Authorizing),
        StatusCode::FORBIDDEN => ("auth.forbidden", ErrorStage::Authorizing),
        StatusCode::NOT_FOUND => ("route.not_found", ErrorStage::Parsing),
        StatusCode::TOO_MANY_REQUESTS => ("provider.rate_limited", ErrorStage::Fetching),
        StatusCode::BAD_GATEWAY | StatusCode::GATEWAY_TIMEOUT | StatusCode::FAILED_DEPENDENCY => {
            ("provider.unavailable", ErrorStage::Fetching)
        }
        _ => ("internal.server_error", ErrorStage::Observing),
    }
}

#[cfg(test)]
#[path = "api_error_tests.rs"]
mod tests;
