//! Fail-closed redaction gate for observable event writes.

use axon_api::source::{ApiError, RedactedPublicWrite, RedactionMetadata, SourceProgressEvent};
use axon_core::redact::{DefaultRedactor, RedactionContext, redact_public_write};

pub const REDACTION_FIELD_NAMES: [&str; 6] = [
    "redaction_status",
    "redaction_version",
    "redacted_field_count",
    "dropped_field_count",
    "detector_count",
    "detector_names",
];

pub fn redact_event(
    event: SourceProgressEvent,
) -> Result<RedactedPublicWrite<SourceProgressEvent>, Box<ApiError>> {
    let mut context = RedactionContext::job_event();
    context.visibility_ceiling = event.visibility;
    let value = serde_json::to_value(event)
        .map_err(|error| Box::new(serialization_error("serialize", error.to_string())))?;
    let write = redact_public_write(value, &context, &DefaultRedactor::new()).map_err(Box::new)?;
    let payload = serde_json::from_value(write.payload)
        .map_err(|error| Box::new(serialization_error("deserialize", error.to_string())))?;
    Ok(RedactedPublicWrite {
        payload,
        redaction: write.redaction,
    })
}

pub fn stamp_event_json(
    event: &SourceProgressEvent,
    redaction: &RedactionMetadata,
) -> Result<String, Box<ApiError>> {
    let mut value = serde_json::to_value(event)
        .map_err(|error| Box::new(serialization_error("serialize", error.to_string())))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| Box::new(serialization_error("serialize", "event is not an object")))?;
    let report = serde_json::to_value(redaction)
        .map_err(|error| Box::new(serialization_error("serialize", error.to_string())))?;
    let report = report.as_object().ok_or_else(|| {
        Box::new(serialization_error(
            "serialize",
            "redaction report is not an object",
        ))
    })?;
    for (field, value) in report {
        object.entry(field.clone()).or_insert_with(|| value.clone());
    }
    serde_json::to_string(&value)
        .map_err(|error| Box::new(serialization_error("serialize", error.to_string())))
}

pub fn parse_stamped_event(raw: &str) -> Result<SourceProgressEvent, Box<ApiError>> {
    let mut value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|error| Box::new(serialization_error("deserialize", error.to_string())))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| Box::new(serialization_error("deserialize", "event is not an object")))?;
    for field in REDACTION_FIELD_NAMES {
        object.remove(field);
    }
    serde_json::from_value(value)
        .map_err(|error| Box::new(serialization_error("deserialize", error.to_string())))
}

fn serialization_error(operation: &str, message: impl Into<String>) -> ApiError {
    ApiError::new(
        format!("observe.{operation}_failed"),
        axon_api::source::ErrorStage::Observing,
        message.into(),
    )
}
