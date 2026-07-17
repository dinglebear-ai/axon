#![allow(clippy::result_large_err)]

use axon_api::source::{ApiError, ErrorStage, MetadataMap, RedactedPublicWrite};
use serde_json::Value;

use super::{
    DefaultRedactor, MAX_REDACTABLE_TEXT_BYTES, MAX_REDACTION_REPORT_ENTRIES, REDACTION_VERSION,
    RedactionContext, RedactionReport, Redactor, forbidden_field_name, value_contains_secret,
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
    let prior_status = metadata
        .get("redaction_status")
        .and_then(Value::as_str)
        .unwrap_or("clean");
    let status = merge_redaction_status(prior_status, report.status().as_str());
    let version = if prior_status == "clean" {
        REDACTION_VERSION.to_string()
    } else {
        metadata
            .get("redaction_version")
            .and_then(Value::as_str)
            .unwrap_or(REDACTION_VERSION)
            .to_string()
    };
    let redacted_field_count = merge_bounded_count(
        metadata.get("redacted_field_count"),
        report.redacted_field_count(),
    );
    let dropped_field_count = merge_bounded_count(
        metadata.get("dropped_field_count"),
        report.dropped_field_count(),
    );
    let detector_names = merge_detector_names(metadata.get("detector_names"), report);

    metadata.insert("redaction_status".to_string(), serde_json::json!(status));
    metadata.insert("redaction_version".to_string(), serde_json::json!(version));
    metadata.insert(
        "redacted_field_count".to_string(),
        serde_json::json!(redacted_field_count),
    );
    metadata.insert(
        "dropped_field_count".to_string(),
        serde_json::json!(dropped_field_count),
    );
    metadata.insert(
        "detector_count".to_string(),
        serde_json::json!(detector_names.len()),
    );
    metadata.insert(
        "detector_names".to_string(),
        serde_json::json!(detector_names),
    );
    metadata.insert(
        "visibility".to_string(),
        serde_json::json!(report.visibility_ceiling),
    );
    metadata
}

fn merge_redaction_status(prior: &str, current: &str) -> &'static str {
    match (prior, current) {
        ("failed", _) | (_, "failed") => "failed",
        ("redacted", _) | (_, "redacted") => "redacted",
        _ => "clean",
    }
}

fn merge_bounded_count(prior: Option<&Value>, current: u32) -> u32 {
    let prior = prior.and_then(Value::as_u64).unwrap_or_default();
    let total = prior.saturating_add(u64::from(current));
    total.min(MAX_REDACTION_REPORT_ENTRIES as u64) as u32
}

fn merge_detector_names(prior: Option<&Value>, report: &RedactionReport) -> Vec<String> {
    let mut names = Vec::new();
    let candidates = prior
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .chain(report.detectors_triggered.iter().map(String::as_str));
    for detector in candidates {
        if names.len() == MAX_REDACTION_REPORT_ENTRIES {
            break;
        }
        if !names.iter().any(|existing| existing == detector) {
            names.push(detector.to_string());
        }
    }
    names
}
