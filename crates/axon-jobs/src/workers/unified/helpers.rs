//! Small pure conversion/error-mapping helpers shared across the unified
//! worker's claim/run/mark-terminal paths.

use super::*;

pub(super) fn parse_enum<T: serde::de::DeserializeOwned>(value: String) -> Result<T, ApiError> {
    serde_json::from_value(serde_json::Value::String(value)).map_err(json_error)
}

pub(super) fn enum_name<T: serde::Serialize>(value: T) -> Result<String, ApiError> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .ok_or_else(|| ApiError::new("job.enum_invalid", ErrorStage::Planning, "invalid enum"))
}

pub(super) fn parse_uuid(value: String) -> Result<uuid::Uuid, ApiError> {
    uuid::Uuid::parse_str(&value).map_err(|error| {
        ApiError::new(
            "job.uuid_invalid",
            ErrorStage::Retrieving,
            format!("invalid job uuid: {error}"),
        )
    })
}

pub(super) fn json_error(error: serde_json::Error) -> ApiError {
    ApiError::new("job.json_error", ErrorStage::Publishing, error.to_string())
}

pub(super) fn sql_error(error: sqlx::Error) -> ApiError {
    ApiError::new(
        "job.sqlite_error",
        ErrorStage::Publishing,
        error.to_string(),
    )
}

pub(super) fn source_error_from_api(error: &ApiError, severity: Severity) -> SourceError {
    SourceError {
        code: error.code.to_string(),
        severity,
        message: error.message.clone(),
        source_item_key: None,
        retryable: error.retryable,
        provider_id: error
            .provider_id
            .clone()
            .map(axon_api::source::ProviderId::new),
        cause: None,
    }
}

pub(super) fn empty_counts() -> StageCounts {
    StageCounts {
        items_total: None,
        items_done: 0,
        documents_total: None,
        documents_done: 0,
        chunks_total: None,
        chunks_done: 0,
        bytes_total: None,
        bytes_done: 0,
    }
}
