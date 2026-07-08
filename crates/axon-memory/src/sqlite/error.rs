//! Shared error constructors for the SQLite memory store.

use axon_api::source::ApiError;
use axon_error::ErrorStage;

/// A storage-layer failure (SQL, JSON, or invariant violation).
pub fn store_error(message: impl Into<String>) -> ApiError {
    ApiError::new("memory.store", ErrorStage::Upserting, message)
}

/// A "memory not found" retrieval error.
pub fn not_found(memory_id: &str) -> ApiError {
    ApiError::new(
        "memory.not_found",
        ErrorStage::Retrieving,
        format!("memory {memory_id} not found"),
    )
}

/// A request-validation error (bad input).
pub fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::new("memory.invalid", ErrorStage::Validation, message)
}

/// Redaction could not be completed safely for this write — the shared
/// redaction boundary is fail-closed (see
/// `axon_core::redact::boundary`'s `RedactionStatus::Failed` doc comment):
/// the caller must not persist the value at all, and must not fall back to
/// writing it unredacted.
pub fn redaction_failed(message: impl Into<String>) -> ApiError {
    ApiError::new("redaction.failed", ErrorStage::Validation, message)
}
