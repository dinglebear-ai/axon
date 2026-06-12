use crate::core::error::{ServiceTaxonomyError, diagnostics_from_error, taxonomy_from_error};
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use std::error::Error;

#[derive(Debug, Clone)]
pub(crate) struct HttpError {
    status: StatusCode,
    kind: ErrorKind,
    message: String,
    diagnostics: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ErrorKind {
    BadGateway,
    BadRequest,
    ChallengeDetected,
    Forbidden,
    Internal,
    InvalidPath,
    LadderExhausted,
    NotFound,
    OutputDirError,
    PathError,
    PathEscape,
    PayloadTooLarge,
    RateLimited,
    ReadError,
    StructuredDataMalformed,
    SymlinkNotAllowed,
    Timeout,
    Unauthorized,
    UnsupportedMediaType,
    UpstreamUnavailable,
    VerticalAuthInvalid,
    VerticalAuthMissing,
    VerticalBlockedAntibot,
    VerticalRateLimited,
    VerticalTargetNotFound,
    VerticalTargetUnavailable,
    VerticalUnsupportedUrl,
}

impl ErrorKind {
    #[cfg(test)]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::BadGateway => "bad_gateway",
            Self::BadRequest => "bad_request",
            Self::ChallengeDetected => "challenge_detected",
            Self::Forbidden => "forbidden",
            Self::Internal => "internal",
            Self::InvalidPath => "invalid_path",
            Self::LadderExhausted => "ladder_exhausted",
            Self::NotFound => "not_found",
            Self::OutputDirError => "output_dir_error",
            Self::PathError => "path_error",
            Self::PathEscape => "path_escape",
            Self::PayloadTooLarge => "payload_too_large",
            Self::RateLimited => "rate_limited",
            Self::ReadError => "read_error",
            Self::StructuredDataMalformed => "structured_data_malformed",
            Self::SymlinkNotAllowed => "symlink_not_allowed",
            Self::Timeout => "timeout",
            Self::Unauthorized => "unauthorized",
            Self::UnsupportedMediaType => "unsupported_media_type",
            Self::UpstreamUnavailable => "upstream_unavailable",
            Self::VerticalAuthInvalid => "vertical_auth_invalid",
            Self::VerticalAuthMissing => "vertical_auth_missing",
            Self::VerticalBlockedAntibot => "vertical_blocked_antibot",
            Self::VerticalRateLimited => "vertical_rate_limited",
            Self::VerticalTargetNotFound => "vertical_target_not_found",
            Self::VerticalTargetUnavailable => "vertical_target_unavailable",
            Self::VerticalUnsupportedUrl => "vertical_unsupported_url",
        }
    }
}

impl From<&'static str> for ErrorKind {
    fn from(kind: &'static str) -> Self {
        match kind {
            "bad_gateway" => Self::BadGateway,
            "bad_request" => Self::BadRequest,
            "challenge_detected" => Self::ChallengeDetected,
            "forbidden" => Self::Forbidden,
            "internal" => Self::Internal,
            "invalid_path" => Self::InvalidPath,
            "ladder_exhausted" => Self::LadderExhausted,
            "not_found" => Self::NotFound,
            "output_dir_error" => Self::OutputDirError,
            "path_error" => Self::PathError,
            "path_escape" => Self::PathEscape,
            "payload_too_large" => Self::PayloadTooLarge,
            "rate_limited" => Self::RateLimited,
            "read_error" => Self::ReadError,
            "structured_data_malformed" => Self::StructuredDataMalformed,
            "symlink_not_allowed" => Self::SymlinkNotAllowed,
            "timeout" => Self::Timeout,
            "unauthorized" => Self::Unauthorized,
            "unsupported_media_type" => Self::UnsupportedMediaType,
            "upstream_unavailable" => Self::UpstreamUnavailable,
            "vertical_auth_invalid" => Self::VerticalAuthInvalid,
            "vertical_auth_missing" => Self::VerticalAuthMissing,
            "vertical_blocked_antibot" => Self::VerticalBlockedAntibot,
            "vertical_rate_limited" => Self::VerticalRateLimited,
            "vertical_target_not_found" => Self::VerticalTargetNotFound,
            "vertical_target_unavailable" => Self::VerticalTargetUnavailable,
            "vertical_unsupported_url" => Self::VerticalUnsupportedUrl,
            _ => Self::Internal,
        }
    }
}

#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct ErrorBody {
    kind: ErrorKind,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Object)]
    diagnostics: Option<serde_json::Value>,
}

impl HttpError {
    pub(crate) fn new(
        status: StatusCode,
        kind: impl Into<ErrorKind>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            status,
            kind: kind.into(),
            message: message.into(),
            diagnostics: None,
        }
    }

    pub(crate) fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }

    pub(crate) fn payload_too_large(message: impl Into<String>) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large", message)
    }

    #[cfg(test)]
    pub(crate) fn status(&self) -> StatusCode {
        self.status
    }

    #[cfg(test)]
    pub(crate) fn kind(&self) -> &'static str {
        self.kind.as_str()
    }

    #[cfg(test)]
    pub(crate) fn message(&self) -> &str {
        &self.message
    }

    pub(crate) fn from_error(err: &(dyn Error + 'static)) -> Self {
        Self::from_error_with_diagnostics(err, false)
    }

    pub(crate) fn from_box(err: Box<dyn Error>) -> Self {
        Self::from_error(err.as_ref())
    }

    pub(crate) fn from_box_send_sync(err: Box<dyn Error + Send + Sync>) -> Self {
        Self::from_error(err.as_ref())
    }

    pub(crate) fn from_error_with_diagnostics(
        err: &(dyn Error + 'static),
        include_diagnostics: bool,
    ) -> Self {
        let taxonomy = taxonomy_from_error(err);
        let (status, kind) = taxonomy
            .as_ref()
            .map(status_and_kind_for_taxonomy)
            .unwrap_or_else(|| status_and_kind_from_message(err));
        let diagnostics = include_diagnostics
            .then(|| diagnostics_from_error(err).cloned())
            .flatten();
        log_handler_error(status, kind, err);
        Self {
            status,
            kind: kind.into(),
            message: response_message(status, err),
            diagnostics,
        }
    }
}

impl From<Box<dyn Error>> for HttpError {
    fn from(err: Box<dyn Error>) -> Self {
        Self::from_error(err.as_ref())
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                kind: self.kind,
                message: self.message,
                diagnostics: self.diagnostics,
            }),
        )
            .into_response()
    }
}

fn status_and_kind_for_taxonomy(taxonomy: &ServiceTaxonomyError) -> (StatusCode, &'static str) {
    match taxonomy {
        ServiceTaxonomyError::Timeout { .. } => (StatusCode::GATEWAY_TIMEOUT, "timeout"),
        ServiceTaxonomyError::RateLimited { .. }
        | ServiceTaxonomyError::VerticalRateLimited { .. } => {
            (StatusCode::TOO_MANY_REQUESTS, taxonomy.mcp_code())
        }
        ServiceTaxonomyError::VerticalTargetNotFound { .. } => {
            (StatusCode::NOT_FOUND, taxonomy.mcp_code())
        }
        ServiceTaxonomyError::VerticalUnsupportedUrl { .. }
        | ServiceTaxonomyError::StructuredDataMalformed { .. } => {
            (StatusCode::BAD_REQUEST, taxonomy.mcp_code())
        }
        ServiceTaxonomyError::UpstreamUnavailable { .. }
        | ServiceTaxonomyError::VerticalTargetUnavailable { .. }
        | ServiceTaxonomyError::ChallengeDetected { .. }
        | ServiceTaxonomyError::VerticalBlockedAntibot { .. } => {
            (StatusCode::BAD_GATEWAY, taxonomy.mcp_code())
        }
        ServiceTaxonomyError::VerticalAuthMissing { .. }
        | ServiceTaxonomyError::VerticalAuthInvalid { .. } => {
            (StatusCode::FAILED_DEPENDENCY, taxonomy.mcp_code())
        }
        ServiceTaxonomyError::LadderExhausted { .. } => {
            (StatusCode::BAD_GATEWAY, taxonomy.mcp_code())
        }
    }
}

fn status_and_kind_from_message(err: &(dyn Error + 'static)) -> (StatusCode, &'static str) {
    let mut message = String::new();
    let mut cursor = Some(err);
    while let Some(current) = cursor {
        message.push_str(&current.to_string());
        message.push('\n');
        cursor = current.source();
    }
    let lc = message.to_lowercase();
    if contains_any(&lc, &["429", "rate limit", "rate-limited"]) {
        (StatusCode::TOO_MANY_REQUESTS, "rate_limited")
    } else if contains_any(&lc, &["timed out", "timeout"]) {
        (StatusCode::GATEWAY_TIMEOUT, "timeout")
    } else if contains_any(
        &lc,
        &[
            "qdrant",
            "tei",
            "chrome",
            "tavily",
            "connection refused",
            "dns",
            "502",
            "503",
            "upstream",
        ],
    ) {
        (StatusCode::BAD_GATEWAY, "upstream_unavailable")
    } else if lc.contains("query is required")
        || lc.contains("invalid endpoint discovery url")
        || lc.contains("invalid collection")
        || lc.contains("invalid query")
        || lc.contains("missing required")
    {
        (StatusCode::BAD_REQUEST, "bad_request")
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, "internal")
    }
}

fn contains_any(message: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| message.contains(needle))
}

fn error_chain(err: &(dyn Error + 'static)) -> String {
    let mut chain = err.to_string();
    let mut cursor = err.source();
    while let Some(cause) = cursor {
        chain.push_str(": ");
        chain.push_str(&cause.to_string());
        cursor = cause.source();
    }
    chain
}

fn log_handler_error(status: StatusCode, kind: &'static str, err: &(dyn Error + 'static)) {
    if status.is_client_error() && status != StatusCode::TOO_MANY_REQUESTS {
        return;
    }
    let chain = error_chain(err);
    if status.is_server_error() {
        tracing::error!(status = status.as_u16(), kind, error = %chain, "handler error");
    } else {
        tracing::warn!(status = status.as_u16(), kind, error = %chain, "handler error");
    }
}

fn response_message(status: StatusCode, err: &(dyn Error + 'static)) -> String {
    match status {
        StatusCode::INTERNAL_SERVER_ERROR => "internal server error".to_string(),
        StatusCode::BAD_GATEWAY => "upstream service unavailable".to_string(),
        StatusCode::GATEWAY_TIMEOUT => "upstream request timed out".to_string(),
        StatusCode::TOO_MANY_REQUESTS => "rate limited".to_string(),
        _ => err.to_string(),
    }
}
