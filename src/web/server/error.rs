use crate::services::error::{ServiceTaxonomyError, diagnostics_from_error, taxonomy_from_error};
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
    kind: &'static str,
    message: String,
    diagnostics: Option<serde_json::Value>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct ErrorBody {
    kind: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Object)]
    diagnostics: Option<serde_json::Value>,
}

impl HttpError {
    pub(crate) fn new(status: StatusCode, kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            kind,
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
        self.kind
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
        Self {
            status,
            kind,
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
                kind: self.kind.to_string(),
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
    if lc.contains("invalid endpoint discovery url") {
        (StatusCode::BAD_REQUEST, "bad_request")
    } else if contains_any(&lc, &["429", "rate limit", "rate-limited"]) {
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

fn response_message(status: StatusCode, err: &(dyn Error + 'static)) -> String {
    match status {
        StatusCode::INTERNAL_SERVER_ERROR => "internal server error".to_string(),
        StatusCode::BAD_GATEWAY => "upstream service unavailable".to_string(),
        StatusCode::GATEWAY_TIMEOUT => "upstream request timed out".to_string(),
        StatusCode::TOO_MANY_REQUESTS => "rate limited".to_string(),
        _ => err.to_string(),
    }
}
