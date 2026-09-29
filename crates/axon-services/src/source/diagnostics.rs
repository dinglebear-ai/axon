//! Public diagnostic projection; source and manifest identities stay untouched.
use axon_api::source::*;

const MESSAGE_BYTES: usize = 4096;
const IDENTITY_BYTES: usize = 512;

use axon_core::redact::public_diagnostic_text as display_text;

pub(crate) fn api_error(error: &anyhow::Error) -> ApiError {
    let typed = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<ApiError>());
    let chain = display_text(&format!("{error:#}"), MESSAGE_BYTES);
    let mut result = if let Some(typed) = typed {
        let mut projected = ApiError::new(typed.code.clone(), typed.stage, chain.clone());
        projected.retryable = typed.retryable;
        projected.severity = typed.severity;
        projected.visibility = typed.visibility;
        projected.retry_after_ms = typed.retry_after_ms;
        projected.cooldown_until = typed.cooldown_until;
        projected.job_id = display_identity(typed.job_id.as_deref());
        projected.source_id = display_identity(typed.source_id.as_deref());
        projected.document_id = display_identity(typed.document_id.as_deref());
        projected.chunk_id = display_identity(typed.chunk_id.as_deref());
        projected.provider_id = display_identity(typed.provider_id.as_deref());
        projected.source_item_key = display_identity(
            typed
                .source_item_key
                .as_deref()
                .or_else(|| typed.details.get("source_item_key").map(String::as_str)),
        );
        if let Some(kind) = typed.details.get("io_kind") {
            projected
                .details
                .insert("io_kind".into(), display_text(kind, 64));
        }
        projected
    } else {
        ApiError::new("source.index_failed", ErrorStage::Internal, chain.clone())
    };
    if chain != display_text(&error.to_string(), MESSAGE_BYTES) {
        result.details.insert("cause".into(), chain);
    }
    result
}

fn display_identity(value: Option<&str>) -> Option<String> {
    value.map(|value| display_text(value, IDENTITY_BYTES))
}

pub(crate) fn source_error(error: &anyhow::Error) -> SourceError {
    let error = api_error(error);
    SourceError {
        code: error.code.to_string(),
        severity: Severity::Failed,
        message: error.message,
        source_item_key: error.source_item_key.map(SourceItemKey::new),
        retryable: error.retryable,
        provider_id: error.provider_id.map(ProviderId::new),
        cause: error.details.get("cause").cloned(),
    }
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
