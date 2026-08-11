//! Redaction guardrails for vector payload metadata.
//!
//! The generic field-name/value secret detectors now live in
//! `axon_core::redact` (shared with every other write surface); this module
//! keeps only the vector-payload-specific hard-fail validator, which needs
//! the local `VectorPayloadValidationError` type and a body-text carve-out
//! (`chunk_text` gets the strict body check, not the generic one).

use axon_core::redact::secret_value_detector;
use serde_json::Value;

use crate::payload::VectorPayloadValidationError;

pub(crate) use axon_core::redact::forbidden_field_name;
pub use axon_core::redact::{
    BARE_SECRET_TOKEN_PREFIXES, FORBIDDEN_FIELD_FRAGMENTS, FORBIDDEN_VALUE_FRAGMENTS,
    value_is_absolute_local_path,
};

pub(crate) fn validate_forbidden_value(
    path: &str,
    value: &Value,
) -> Result<(), VectorPayloadValidationError> {
    match value {
        Value::String(value) if forbidden_string_reason(path, value).is_some() => {
            Err(VectorPayloadValidationError::ForbiddenValue {
                field: path.to_string(),
                detector: forbidden_string_reason(path, value)
                    .expect("guarded by is_some")
                    .to_string(),
            })
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                validate_forbidden_value(&format!("{path}[{index}]"), value)?;
            }
            Ok(())
        }
        Value::Object(object) => {
            if adapter_response_blob(object) {
                return Err(VectorPayloadValidationError::ForbiddenValue {
                    field: path.to_string(),
                    detector: "adapter_response_blob".to_string(),
                });
            }
            for (field, value) in object {
                let child_path = format!("{path}.{field}");
                if forbidden_field_name(field) {
                    return Err(VectorPayloadValidationError::ForbiddenValue {
                        field: child_path,
                        detector: "forbidden_field_name".to_string(),
                    });
                }
                validate_forbidden_value(&child_path, value)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn forbidden_string_reason(path: &str, value: &str) -> Option<&'static str> {
    if BODY_TEXT_FIELDS.contains(&path) {
        return secret_value_detector(value);
    }
    let normalized = value.to_ascii_lowercase();
    secret_value_detector(value)
        .or_else(|| value_is_absolute_local_path(value).then_some("absolute_local_path"))
        .or_else(|| raw_html_blob(&normalized).then_some("raw_html_blob"))
        .or_else(|| {
            normalized
                .contains("adapter_response")
                .then_some("adapter_response_marker")
        })
}

fn raw_html_blob(normalized: &str) -> bool {
    let trimmed = normalized.trim_start();
    trimmed.starts_with("<!doctype html")
        || trimmed.starts_with("<html")
        || (normalized.contains("<html") && normalized.contains("</html>"))
        || (normalized.contains("<body") && normalized.contains("</body>"))
}

fn adapter_response_blob(object: &serde_json::Map<String, Value>) -> bool {
    let has_status = object.contains_key("status") || object.contains_key("status_code");
    let has_headers = object.contains_key("headers");
    let has_body = object.contains_key("body")
        || object.contains_key("raw_body")
        || object.contains_key("response_body");
    has_status && has_headers && has_body
}

const BODY_TEXT_FIELDS: &[&str] = &["chunk_text"];
