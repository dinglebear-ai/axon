//! Error construction helpers for `axon-graph`.
//!
//! Graph failures are reported through the shared [`ApiError`] taxonomy owned by
//! `axon-error` (re-exported via `axon_api::source::ApiError`). Kind-rejection
//! and candidate-validation failures are `Validation`-stage errors; storage
//! failures are `Graphing`-stage errors.

use axon_api::source::ApiError;
use axon_error::ErrorStage;

/// A `Validation`-stage error, used when a candidate references an unknown
/// node/edge kind or is otherwise malformed before write.
pub fn graph_validation_error(message: impl Into<String>) -> ApiError {
    ApiError::new("graph.validation", ErrorStage::Validation, message)
}

/// A `Graphing`-stage error, used when the SQLite store itself fails.
pub fn graph_storage_error(message: impl Into<String>) -> ApiError {
    ApiError::new("graph.storage", ErrorStage::Graphing, message)
}
