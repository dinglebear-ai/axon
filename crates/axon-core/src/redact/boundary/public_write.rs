#![allow(clippy::result_large_err)]

use axon_api::source::{ApiError, ErrorStage, MetadataMap, RedactedPublicWrite};
use serde_json::Value;

use super::{
    DefaultRedactor, MAX_REDACTABLE_TEXT_BYTES, REDACTION_VERSION, RedactionContext,
    RedactionReport, Redactor, forbidden_field_name, value_contains_secret,
};

const MAX_REDACTION_DEPTH: usize = 64;

/// Validate and redact a structured public write before a downstream writer
/// can observe it.
pub fn redact_public_write(
    value: Value,
    context: &RedactionContext,
    redactor: &dyn Redactor,
) -> Result<RedactedPublicWrite<Value>, ApiError> {
    validate_public_write_value(&value, "", 0)?;
    let (payload, report) = redactor.redact_json(value, context);
    Ok(RedactedPublicWrite {
        payload,
        redaction: report.metadata(context),
    })
}

/// Checked metadata-map form of [`redact_public_write`].
pub fn redact_metadata_checked(
    metadata: MetadataMap,
    context: &RedactionContext,
    redactor: &dyn Redactor,
) -> Result<(MetadataMap, RedactionReport), ApiError> {
    let value = Value::Object(metadata.0.into_iter().collect());
    validate_public_write_value(&value, "", 0)?;
    let (redacted, report) = redactor.redact_json(value, context);
    let Value::Object(map) = redacted else {
        return Err(redaction_failed("redactor returned non-object metadata"));
    };
    Ok((MetadataMap(map.into_iter().collect()), report))
}

fn validate_public_write_value(value: &Value, path: &str, depth: usize) -> Result<(), ApiError> {
    if depth > MAX_REDACTION_DEPTH {
        return Err(redaction_failed(
            "payload nesting exceeds the redaction limit",
        ));
    }
    match value {
        Value::String(text) => {
            if text.len() > MAX_REDACTABLE_TEXT_BYTES {
                return Err(redaction_failed(
                    "payload field exceeds the redaction size limit",
                ));
            }
            if DefaultRedactor::is_structural_field(path) && value_contains_secret(text) {
                return Err(redaction_failed(
                    "secret detected in a protected content field",
                ));
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                validate_public_write_value(item, &format!("{path}[{index}]"), depth + 1)?;
            }
        }
        Value::Object(map) => {
            for (field, child) in map {
                let child_path = if path.is_empty() {
                    field.clone()
                } else {
                    format!("{path}.{field}")
                };
                if !DefaultRedactor::is_structural_field(field) && forbidden_field_name(field) {
                    return Err(redaction_failed(
                        "payload contains a forbidden sensitive field",
                    ));
                }
                validate_public_write_value(child, &child_path, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn redaction_failed(message: &str) -> ApiError {
    ApiError::new("redaction.failed", ErrorStage::Authorizing, message)
}

/// Stamp redaction report metadata onto a public write payload.
pub fn stamp_redaction_metadata(
    mut metadata: MetadataMap,
    report: &RedactionReport,
) -> MetadataMap {
    metadata.insert(
        "redaction_status".to_string(),
        serde_json::json!(report.status()),
    );
    metadata.insert(
        "redaction_version".to_string(),
        serde_json::json!(REDACTION_VERSION),
    );
    metadata.insert(
        "redacted_field_count".to_string(),
        serde_json::json!(report.redacted_field_count()),
    );
    metadata.insert(
        "dropped_field_count".to_string(),
        serde_json::json!(report.dropped_field_count()),
    );
    metadata.insert(
        "detector_count".to_string(),
        serde_json::json!(report.detector_count()),
    );
    metadata.insert(
        "detector_names".to_string(),
        serde_json::json!(report.detectors_triggered),
    );
    metadata.insert(
        "visibility".to_string(),
        serde_json::json!(report.visibility_ceiling),
    );
    metadata
}
